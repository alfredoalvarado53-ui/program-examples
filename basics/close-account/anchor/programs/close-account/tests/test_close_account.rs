use {
    anchor_lang::{
        solana_program::{instruction::Instruction, pubkey::Pubkey, system_program},
        AccountDeserialize, InstructionData, ToAccountMetas,
    },
    close_account_program::state::UserState,
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
}

#[test]
fn create_and_close_user() {
    let mut svm = LiteSVM::new();
    let bytes = include_bytes!(concat!(env!("CARGO_MANIFEST_DIR"), "/../../target/deploy/close_account_program.so"));
    svm.add_program(close_account_program::id(), bytes).unwrap();
    let payer = Keypair::new();
    svm.airdrop(&payer.pubkey(), 5_000_000_000).unwrap();
    let (user_account, _) =
        Pubkey::find_program_address(&[b"USER", payer.pubkey().as_ref()], &close_account_program::id());

    let create = Instruction::new_with_bytes(
        close_account_program::id(),
        &close_account_program::instruction::CreateUser { name: "John Doe".to_string() }.data(),
        close_account_program::accounts::CreateUserContext {
            user: payer.pubkey(),
            user_account,
            system_program: system_program::ID,
        }
        .to_account_metas(None),
    );
    send(&mut svm, create, &payer);
    let account = svm.get_account(&user_account).unwrap();
    let user = UserState::try_deserialize(&mut account.data.as_slice()).unwrap();
    assert_eq!(user.name, "John Doe");
    assert_eq!(user.user, payer.pubkey());

    let close = Instruction::new_with_bytes(
        close_account_program::id(),
        &close_account_program::instruction::CloseUser {}.data(),
        close_account_program::accounts::CloseUserContext { user: payer.pubkey(), user_account }.to_account_metas(None),
    );
    send(&mut svm, close, &payer);
    assert!(svm.get_account(&user_account).is_none());
}
