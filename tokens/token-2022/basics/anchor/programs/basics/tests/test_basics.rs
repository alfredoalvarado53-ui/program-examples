use {
    anchor_lang::{
        solana_program::{instruction::Instruction, pubkey::Pubkey, system_program},
        InstructionData, ToAccountMetas,
    },
    anchor_spl::{
        associated_token::{self, get_associated_token_address_with_program_id},
        token_2022::spl_token_2022::{self, extension::StateWithExtensions, state::Account as TokenAccountState},
    },
    litesvm::LiteSVM,
    solana_keypair::Keypair,
    solana_message::{Message, VersionedMessage},
    solana_signer::Signer,
    solana_transaction::versioned::VersionedTransaction,
};

fn send(svm: &mut LiteSVM, ix: Instruction, payer: &Keypair) {
    let msg = Message::new_with_blockhash(&[ix], Some(&payer.pubkey()), &svm.latest_blockhash());
    let tx = VersionedTransaction::try_new(VersionedMessage::Legacy(msg), &[payer]).unwrap();
    svm.send_transaction(tx).unwrap();
    svm.expire_blockhash();
}

fn token_amount(svm: &LiteSVM, account: &Pubkey) -> u64 {
    let account = svm.get_account(account).unwrap();
    StateWithExtensions::<TokenAccountState>::unpack(&account.data).unwrap().base.amount
}

#[test]
fn create_mint_and_transfer() {
    let mut svm = LiteSVM::new();
    let bytes = include_bytes!(concat!(env!("CARGO_MANIFEST_DIR"), "/../../target/deploy/anchor.so"));
    svm.add_program(anchor::id(), bytes).unwrap();
    let payer = Keypair::new();
    svm.airdrop(&payer.pubkey(), 1_000_000_000).unwrap();

    let token_name = "TestToken".to_string();
    let (mint, _) = Pubkey::find_program_address(
        &[b"token-2022-token", payer.pubkey().as_ref(), token_name.as_bytes()],
        &anchor::id(),
    );
    let payer_ata = get_associated_token_address_with_program_id(&payer.pubkey(), &mint, &spl_token_2022::ID);
    let receiver = Keypair::new();
    let receiver_ata = get_associated_token_address_with_program_id(&receiver.pubkey(), &mint, &spl_token_2022::ID);

    send(
        &mut svm,
        Instruction::new_with_bytes(
            anchor::id(),
            &anchor::instruction::CreateToken { _token_name: token_name }.data(),
            anchor::accounts::CreateToken {
                signer: payer.pubkey(),
                mint,
                system_program: system_program::ID,
                token_program: spl_token_2022::ID,
            }
            .to_account_metas(None),
        ),
        &payer,
    );
    assert_eq!(svm.get_account(&mint).unwrap().owner, spl_token_2022::ID);

    send(
        &mut svm,
        Instruction::new_with_bytes(
            anchor::id(),
            &anchor::instruction::CreateAssociatedTokenAccount {}.data(),
            anchor::accounts::CreateAssociatedTokenAccount {
                signer: payer.pubkey(),
                mint,
                token_account: payer_ata,
                system_program: system_program::ID,
                token_program: spl_token_2022::ID,
                associated_token_program: associated_token::ID,
            }
            .to_account_metas(None),
        ),
        &payer,
    );
    assert_eq!(token_amount(&svm, &payer_ata), 0);

    send(
        &mut svm,
        Instruction::new_with_bytes(
            anchor::id(),
            &anchor::instruction::MintToken { amount: 200_000_000 }.data(),
            anchor::accounts::MintToken {
                signer: payer.pubkey(),
                mint,
                receiver: payer_ata,
                token_program: spl_token_2022::ID,
            }
            .to_account_metas(None),
        ),
        &payer,
    );
    assert_eq!(token_amount(&svm, &payer_ata), 200_000_000);

    send(
        &mut svm,
        Instruction::new_with_bytes(
            anchor::id(),
            &anchor::instruction::TransferToken { amount: 100 }.data(),
            anchor::accounts::TransferToken {
                signer: payer.pubkey(),
                from: payer_ata,
                to: receiver.pubkey(),
                to_ata: receiver_ata,
                mint,
                token_program: spl_token_2022::ID,
                system_program: system_program::ID,
                associated_token_program: associated_token::ID,
            }
            .to_account_metas(None),
        ),
        &payer,
    );
    assert_eq!(token_amount(&svm, &payer_ata), 199_999_900);
    assert_eq!(token_amount(&svm, &receiver_ata), 100);
}
