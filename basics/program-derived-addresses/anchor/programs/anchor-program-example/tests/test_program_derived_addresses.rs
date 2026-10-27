use {
    anchor_lang::{
        solana_program::{instruction::Instruction, pubkey::Pubkey, system_program},
        AccountDeserialize, InstructionData, ToAccountMetas,
    },
    litesvm::LiteSVM,
    program_derived_addresses_program::state::PageVisits,
    solana_keypair::Keypair,
    solana_message::{Message, VersionedMessage},
    solana_signer::Signer,
    solana_transaction::versioned::VersionedTransaction,
};

fn setup() -> (LiteSVM, Keypair) {
    let mut svm = LiteSVM::new();
    let bytes = include_bytes!(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/../../target/deploy/program_derived_addresses_program.so"
    ));
    svm.add_program(program_derived_addresses_program::id(), bytes).unwrap();
    let payer = Keypair::new();
    svm.airdrop(&payer.pubkey(), 1_000_000_000).unwrap();
    (svm, payer)
}

fn send(svm: &mut LiteSVM, ix: Instruction, payer: &Keypair) {
    let msg = Message::new_with_blockhash(&[ix], Some(&payer.pubkey()), &svm.latest_blockhash());
    let tx = VersionedTransaction::try_new(VersionedMessage::Legacy(msg), &[payer]).unwrap();
    svm.send_transaction(tx).unwrap();
    svm.expire_blockhash();
}

fn page_visits(svm: &LiteSVM, pda: &Pubkey) -> u32 {
    let account = svm.get_account(pda).unwrap();
    PageVisits::try_deserialize(&mut account.data.as_slice()).unwrap().page_visits
}

#[test]
fn create_and_visit_page() {
    let (mut svm, payer) = setup();
    let (pda, _) = Pubkey::find_program_address(
        &[PageVisits::SEED_PREFIX, payer.pubkey().as_ref()],
        &program_derived_addresses_program::id(),
    );

    let create = Instruction::new_with_bytes(
        program_derived_addresses_program::id(),
        &program_derived_addresses_program::instruction::CreatePageVisits {}.data(),
        program_derived_addresses_program::accounts::CreatePageVisits {
            payer: payer.pubkey(),
            page_visits: pda,
            system_program: system_program::ID,
        }
        .to_account_metas(None),
    );
    send(&mut svm, create, &payer);
    assert_eq!(page_visits(&svm, &pda), 0);

    let increment = Instruction::new_with_bytes(
        program_derived_addresses_program::id(),
        &program_derived_addresses_program::instruction::IncrementPageVisits {}.data(),
        program_derived_addresses_program::accounts::IncrementPageVisits { user: payer.pubkey(), page_visits: pda }
            .to_account_metas(None),
    );
    send(&mut svm, increment.clone(), &payer);
    assert_eq!(page_visits(&svm, &pda), 1);

    send(&mut svm, increment, &payer);
    assert_eq!(page_visits(&svm, &pda), 2);
}
