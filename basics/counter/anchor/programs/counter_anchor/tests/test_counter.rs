use {
    anchor_lang::{
        solana_program::{instruction::Instruction, pubkey::Pubkey, system_program},
        AccountDeserialize, InstructionData, ToAccountMetas,
    },
    counter_anchor::Counter,
    litesvm::LiteSVM,
    solana_keypair::Keypair,
    solana_message::{Message, VersionedMessage},
    solana_signer::Signer,
    solana_transaction::versioned::VersionedTransaction,
};

fn setup() -> (LiteSVM, Keypair) {
    let mut svm = LiteSVM::new();
    let bytes = include_bytes!(concat!(env!("CARGO_MANIFEST_DIR"), "/../../target/deploy/counter_anchor.so"));
    svm.add_program(counter_anchor::id(), bytes).unwrap();
    let payer = Keypair::new();
    svm.airdrop(&payer.pubkey(), 1_000_000_000).unwrap();
    (svm, payer)
}

fn send(svm: &mut LiteSVM, ix: Instruction, payer: &Keypair, signers: &[&Keypair]) {
    let msg = Message::new_with_blockhash(&[ix], Some(&payer.pubkey()), &svm.latest_blockhash());
    let tx = VersionedTransaction::try_new(VersionedMessage::Legacy(msg), signers).unwrap();
    svm.send_transaction(tx).unwrap();
    svm.expire_blockhash();
}

fn count(svm: &LiteSVM, counter: &Pubkey) -> u64 {
    let account = svm.get_account(counter).unwrap();
    Counter::try_deserialize(&mut account.data.as_slice()).unwrap().count
}

fn increment(counter: Pubkey) -> Instruction {
    Instruction::new_with_bytes(
        counter_anchor::id(),
        &counter_anchor::instruction::Increment {}.data(),
        counter_anchor::accounts::Increment { counter }.to_account_metas(None),
    )
}

#[test]
fn initialize_and_increment() {
    let (mut svm, payer) = setup();
    let counter = Keypair::new();

    let initialize = Instruction::new_with_bytes(
        counter_anchor::id(),
        &counter_anchor::instruction::InitializeCounter {}.data(),
        counter_anchor::accounts::InitializeCounter {
            payer: payer.pubkey(),
            counter: counter.pubkey(),
            system_program: system_program::ID,
        }
        .to_account_metas(None),
    );
    send(&mut svm, initialize, &payer, &[&payer, &counter]);
    assert_eq!(count(&svm, &counter.pubkey()), 0);

    send(&mut svm, increment(counter.pubkey()), &payer, &[&payer]);
    assert_eq!(count(&svm, &counter.pubkey()), 1);

    send(&mut svm, increment(counter.pubkey()), &payer, &[&payer]);
    assert_eq!(count(&svm, &counter.pubkey()), 2);
}
