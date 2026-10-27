use {
    anchor_lang::{
        solana_program::{instruction::Instruction, pubkey::Pubkey, rent::Rent, system_instruction, system_program},
        InstructionData, ToAccountMetas,
    },
    anchor_spl::token_2022::spl_token_2022::{
        self,
        error::TokenError,
        extension::{
            cpi_guard::instruction::{disable_cpi_guard, enable_cpi_guard},
            ExtensionType, StateWithExtensions,
        },
        instruction::{initialize_account, initialize_mint2, mint_to},
        state::{Account as TokenAccountState, Mint},
    },
    litesvm::LiteSVM,
    solana_keypair::Keypair,
    solana_message::{Message, VersionedMessage},
    solana_signer::Signer,
    solana_transaction::{versioned::VersionedTransaction, InstructionError, TransactionError},
};

fn send(svm: &mut LiteSVM, ixs: &[Instruction], payer: &Keypair, signers: &[&Keypair]) -> Result<(), TransactionError> {
    let msg = Message::new_with_blockhash(ixs, Some(&payer.pubkey()), &svm.latest_blockhash());
    let tx = VersionedTransaction::try_new(VersionedMessage::Legacy(msg), signers).unwrap();
    let result = svm.send_transaction(tx).map(|_| ()).map_err(|failed| failed.err);
    svm.expire_blockhash();
    result
}

fn cpi_transfer(payer: &Keypair, sender_token_account: Pubkey, mint: Pubkey) -> Instruction {
    let (recipient_token_account, _) = Pubkey::find_program_address(&[b"pda"], &cpi_guard::id());
    Instruction::new_with_bytes(
        cpi_guard::id(),
        &cpi_guard::instruction::CpiTransfer {}.data(),
        cpi_guard::accounts::CpiTransfer {
            sender: payer.pubkey(),
            sender_token_account,
            recipient_token_account,
            mint_account: mint,
            token_program: spl_token_2022::ID,
            system_program: system_program::ID,
        }
        .to_account_metas(None),
    )
}

fn token_amount(svm: &LiteSVM, account: &Pubkey) -> u64 {
    let account = svm.get_account(account).unwrap();
    StateWithExtensions::<TokenAccountState>::unpack(&account.data).unwrap().base.amount
}

#[test]
fn cpi_guard_blocks_transfer_until_disabled() {
    let mut svm = LiteSVM::new();
    let bytes = include_bytes!(concat!(env!("CARGO_MANIFEST_DIR"), "/../../target/deploy/cpi_guard.so"));
    svm.add_program(cpi_guard::id(), bytes).unwrap();
    let payer = Keypair::new();
    svm.airdrop(&payer.pubkey(), 1_000_000_000).unwrap();
    let mint = Keypair::new();
    let token = Keypair::new();

    let mint_len = ExtensionType::try_calculate_account_len::<Mint>(&[]).unwrap();
    send(
        &mut svm,
        &[
            system_instruction::create_account(
                &payer.pubkey(),
                &mint.pubkey(),
                Rent::default().minimum_balance(mint_len),
                mint_len as u64,
                &spl_token_2022::ID,
            ),
            initialize_mint2(&spl_token_2022::ID, &mint.pubkey(), &payer.pubkey(), None, 2).unwrap(),
        ],
        &payer,
        &[&payer, &mint],
    )
    .unwrap();

    let account_len =
        ExtensionType::try_calculate_account_len::<TokenAccountState>(&[ExtensionType::CpiGuard]).unwrap();
    send(
        &mut svm,
        &[
            system_instruction::create_account(
                &payer.pubkey(),
                &token.pubkey(),
                Rent::default().minimum_balance(account_len),
                account_len as u64,
                &spl_token_2022::ID,
            ),
            initialize_account(&spl_token_2022::ID, &token.pubkey(), &mint.pubkey(), &payer.pubkey()).unwrap(),
            enable_cpi_guard(&spl_token_2022::ID, &token.pubkey(), &payer.pubkey(), &[]).unwrap(),
            mint_to(&spl_token_2022::ID, &mint.pubkey(), &token.pubkey(), &payer.pubkey(), &[], 1).unwrap(),
        ],
        &payer,
        &[&payer, &token],
    )
    .unwrap();

    let blocked =
        send(&mut svm, &[cpi_transfer(&payer, token.pubkey(), mint.pubkey())], &payer, &[&payer]).unwrap_err();
    assert_eq!(
        blocked,
        TransactionError::InstructionError(0, InstructionError::Custom(TokenError::CpiGuardTransferBlocked as u32))
    );

    send(
        &mut svm,
        &[disable_cpi_guard(&spl_token_2022::ID, &token.pubkey(), &payer.pubkey(), &[]).unwrap()],
        &payer,
        &[&payer],
    )
    .unwrap();

    send(&mut svm, &[cpi_transfer(&payer, token.pubkey(), mint.pubkey())], &payer, &[&payer]).unwrap();
    let (recipient, _) = Pubkey::find_program_address(&[b"pda"], &cpi_guard::id());
    assert_eq!(token_amount(&svm, &token.pubkey()), 0);
    assert_eq!(token_amount(&svm, &recipient), 1);
}
