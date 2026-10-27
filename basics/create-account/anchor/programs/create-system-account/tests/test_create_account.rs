use {
    anchor_lang::{
        prelude::borsh,
        solana_program::{instruction::Instruction, system_program},
        InstructionData, ToAccountMetas,
    },
    create_system_account::AddressData,
    litesvm::LiteSVM,
    solana_keypair::Keypair,
    solana_message::{Message, VersionedMessage},
    solana_signer::Signer,
    solana_transaction::versioned::VersionedTransaction,
};

#[test]
fn create_the_account() {
    let mut svm = LiteSVM::new();
    let bytes = include_bytes!(concat!(env!("CARGO_MANIFEST_DIR"), "/../../target/deploy/create_system_account.so"));
    svm.add_program(create_system_account::id(), bytes).unwrap();
    let payer = Keypair::new();
    svm.airdrop(&payer.pubkey(), 1_000_000_000).unwrap();
    let new_account = Keypair::new();

    let address_data =
        AddressData { name: "Marcus".to_string(), address: "123 Main St. San Francisco, CA".to_string() };
    let data_len = borsh::to_vec(&address_data).unwrap().len();

    let ix = Instruction::new_with_bytes(
        create_system_account::id(),
        &create_system_account::instruction::CreateSystemAccount { address_data }.data(),
        create_system_account::accounts::CreateSystemAccount {
            payer: payer.pubkey(),
            new_account: new_account.pubkey(),
            system_program: system_program::ID,
        }
        .to_account_metas(None),
    );
    let msg = Message::new_with_blockhash(&[ix], Some(&payer.pubkey()), &svm.latest_blockhash());
    let tx = VersionedTransaction::try_new(VersionedMessage::Legacy(msg), &[&payer, &new_account]).unwrap();
    svm.send_transaction(tx).unwrap();

    let account = svm.get_account(&new_account.pubkey()).unwrap();
    assert_eq!(account.data.len(), data_len);
    assert_eq!(account.lamports, svm.minimum_balance_for_rent_exemption(data_len));
}
