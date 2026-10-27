use {
    anchor_lang::{
        solana_program::{instruction::Instruction, pubkey::Pubkey, system_program},
        AccountDeserialize, InstructionData, ToAccountMetas,
    },
    anchor_realloc::Message as MessageAccount,
    litesvm::LiteSVM,
    solana_keypair::Keypair,
    solana_message::{Message, VersionedMessage},
    solana_signer::Signer,
    solana_transaction::versioned::VersionedTransaction,
};

fn setup() -> (LiteSVM, Keypair) {
    let mut svm = LiteSVM::new();
    let bytes = include_bytes!(concat!(env!("CARGO_MANIFEST_DIR"), "/../../target/deploy/anchor_realloc.so"));
    svm.add_program(anchor_realloc::id(), bytes).unwrap();
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

fn assert_message(svm: &LiteSVM, account: &Pubkey, expected: &str) {
    let account = svm.get_account(account).unwrap();
    assert_eq!(account.data.len(), 8 + 4 + expected.len());
    let decoded = MessageAccount::try_deserialize(&mut account.data.as_slice()).unwrap();
    assert_eq!(decoded.message, expected);
}

fn update(svm: &mut LiteSVM, payer: &Keypair, message_account: Pubkey, input: &str) {
    let ix = Instruction::new_with_bytes(
        anchor_realloc::id(),
        &anchor_realloc::instruction::Update { input: input.to_string() }.data(),
        anchor_realloc::accounts::Update { payer: payer.pubkey(), message_account, system_program: system_program::ID }
            .to_account_metas(None),
    );
    send(svm, ix, payer, &[payer]);
}

#[test]
fn initialize_then_grow_and_shrink() {
    let (mut svm, payer) = setup();
    let message_account = Keypair::new();

    let initialize = Instruction::new_with_bytes(
        anchor_realloc::id(),
        &anchor_realloc::instruction::Initialize { input: "hello".to_string() }.data(),
        anchor_realloc::accounts::Initialize {
            payer: payer.pubkey(),
            message_account: message_account.pubkey(),
            system_program: system_program::ID,
        }
        .to_account_metas(None),
    );
    send(&mut svm, initialize, &payer, &[&payer, &message_account]);
    assert_message(&svm, &message_account.pubkey(), "hello");

    update(&mut svm, &payer, message_account.pubkey(), "hello world");
    assert_message(&svm, &message_account.pubkey(), "hello world");

    update(&mut svm, &payer, message_account.pubkey(), "hi");
    assert_message(&svm, &message_account.pubkey(), "hi");
}
