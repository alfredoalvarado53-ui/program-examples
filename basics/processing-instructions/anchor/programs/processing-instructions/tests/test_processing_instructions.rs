use {
    anchor_lang::{solana_program::instruction::Instruction, InstructionData, ToAccountMetas},
    litesvm::LiteSVM,
    solana_keypair::Keypair,
    solana_message::{Message, VersionedMessage},
    solana_signer::Signer,
    solana_transaction::versioned::VersionedTransaction,
};

fn setup() -> (LiteSVM, Keypair) {
    let mut svm = LiteSVM::new();
    let bytes = include_bytes!(concat!(env!("CARGO_MANIFEST_DIR"), "/../../target/deploy/processing_instructions.so"));
    svm.add_program(processing_instructions::id(), bytes).unwrap();
    let payer = Keypair::new();
    svm.airdrop(&payer.pubkey(), 1_000_000_000).unwrap();
    (svm, payer)
}

fn go_to_park(svm: &mut LiteSVM, payer: &Keypair, name: &str, height: u32) -> Vec<String> {
    let ix = Instruction::new_with_bytes(
        processing_instructions::id(),
        &processing_instructions::instruction::GoToPark { name: name.to_string(), height }.data(),
        processing_instructions::accounts::Park {}.to_account_metas(None),
    );
    let msg = Message::new_with_blockhash(&[ix], Some(&payer.pubkey()), &svm.latest_blockhash());
    let tx = VersionedTransaction::try_new(VersionedMessage::Legacy(msg), &[payer]).unwrap();
    svm.send_transaction(tx).unwrap().logs
}

fn logged(logs: &[String], text: &str) -> bool {
    logs.iter().any(|log| log == &format!("Program log: {text}"))
}

#[test]
fn go_to_park_checks_height() {
    let (mut svm, payer) = setup();

    let logs = go_to_park(&mut svm, &payer, "Jimmy", 3);
    assert!(logged(&logs, "Welcome to the park, Jimmy!"));
    assert!(logged(&logs, "You are NOT tall enough to ride this ride. Sorry mate."));

    let logs = go_to_park(&mut svm, &payer, "Mary", 10);
    assert!(logged(&logs, "Welcome to the park, Mary!"));
    assert!(logged(&logs, "You are tall enough to ride this ride. Congratulations."));
}
