use {
    anchor_lang::{
        solana_program::{instruction::Instruction, pubkey::Pubkey, system_program},
        InstructionData, ToAccountMetas,
    },
    anchor_spl::{
        associated_token::{
            get_associated_token_address_with_program_id,
            spl_associated_token_account::instruction::create_associated_token_account,
        },
        token_2022::spl_token_2022::{
            self,
            extension::{non_transferable::NonTransferable, BaseStateWithExtensions, StateWithExtensions},
            state::{Account as TokenAccount, Mint},
        },
    },
    litesvm::LiteSVM,
    solana_keypair::Keypair,
    solana_message::{Message, VersionedMessage},
    solana_signer::Signer,
    solana_transaction::versioned::VersionedTransaction,
};

fn setup() -> (LiteSVM, Keypair) {
    let mut svm = LiteSVM::new();
    let bytes = include_bytes!(concat!(env!("CARGO_MANIFEST_DIR"), "/../../target/deploy/non_transferable.so"));
    svm.add_program(non_transferable::id(), bytes).unwrap();
    let payer = Keypair::new();
    svm.airdrop(&payer.pubkey(), 1_000_000_000).unwrap();
    (svm, payer)
}

fn send(svm: &mut LiteSVM, ixs: &[Instruction], payer: &Keypair, signers: &[&Keypair]) -> Result<(), Vec<String>> {
    let msg = Message::new_with_blockhash(ixs, Some(&payer.pubkey()), &svm.latest_blockhash());
    let tx = VersionedTransaction::try_new(VersionedMessage::Legacy(msg), signers).unwrap();
    let result = svm.send_transaction(tx);
    svm.expire_blockhash();
    result.map(|_| ()).map_err(|failed| failed.meta.logs)
}

fn balance(svm: &LiteSVM, token_account: &Pubkey) -> u64 {
    let account = svm.get_account(token_account).unwrap();
    StateWithExtensions::<TokenAccount>::unpack(&account.data).unwrap().base.amount
}

#[test]
fn non_transferable_mint_rejects_transfers() {
    let (mut svm, payer) = setup();
    let mint = Keypair::new();
    let recipient = Keypair::new();

    let initialize = Instruction::new_with_bytes(
        non_transferable::id(),
        &non_transferable::instruction::Initialize {}.data(),
        non_transferable::accounts::Initialize {
            payer: payer.pubkey(),
            mint_account: mint.pubkey(),
            token_program: spl_token_2022::ID,
            system_program: system_program::ID,
        }
        .to_account_metas(None),
    );
    send(&mut svm, &[initialize], &payer, &[&payer, &mint]).unwrap();

    let mint_account = svm.get_account(&mint.pubkey()).unwrap();
    let mint_state = StateWithExtensions::<Mint>::unpack(&mint_account.data).unwrap();
    assert!(mint_state.get_extension::<NonTransferable>().is_ok());
    assert_eq!(mint_state.base.decimals, 2);

    let source = get_associated_token_address_with_program_id(&payer.pubkey(), &mint.pubkey(), &spl_token_2022::ID);
    let destination =
        get_associated_token_address_with_program_id(&recipient.pubkey(), &mint.pubkey(), &spl_token_2022::ID);
    let setup_ixs = [
        create_associated_token_account(&payer.pubkey(), &payer.pubkey(), &mint.pubkey(), &spl_token_2022::ID),
        create_associated_token_account(&payer.pubkey(), &recipient.pubkey(), &mint.pubkey(), &spl_token_2022::ID),
        spl_token_2022::instruction::mint_to(&spl_token_2022::ID, &mint.pubkey(), &source, &payer.pubkey(), &[], 1)
            .unwrap(),
    ];
    send(&mut svm, &setup_ixs, &payer, &[&payer]).unwrap();
    assert_eq!(balance(&svm, &source), 1);

    let transfer = spl_token_2022::instruction::transfer_checked(
        &spl_token_2022::ID,
        &source,
        &mint.pubkey(),
        &destination,
        &payer.pubkey(),
        &[],
        1,
        2,
    )
    .unwrap();
    let logs = send(&mut svm, &[transfer], &payer, &[&payer]).unwrap_err();
    assert!(logs.iter().any(|log| log.contains("Transfer is disabled for this mint")));
    assert_eq!(balance(&svm, &source), 1);
    assert_eq!(balance(&svm, &destination), 0);
}
