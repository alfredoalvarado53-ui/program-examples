use {
    account_data_anchor_program::state::AddressInfo,
    anchor_lang::{
        solana_program::{instruction::Instruction, system_program},
        AccountDeserialize, InstructionData, ToAccountMetas,
    },
    litesvm::LiteSVM,
    solana_keypair::Keypair,
    solana_message::{Message, VersionedMessage},
    solana_signer::Signer,
    solana_transaction::versioned::VersionedTransaction,
};

#[test]
fn create_and_read_address_info() {
    let mut svm = LiteSVM::new();
    let bytes =
        include_bytes!(concat!(env!("CARGO_MANIFEST_DIR"), "/../../target/deploy/account_data_anchor_program.so"));
    svm.add_program(account_data_anchor_program::id(), bytes).unwrap();
    let payer = Keypair::new();
    svm.airdrop(&payer.pubkey(), 100_000_000_000).unwrap();
    let address_info = Keypair::new();

    let ix = Instruction::new_with_bytes(
        account_data_anchor_program::id(),
        &account_data_anchor_program::instruction::CreateAddressInfo {
            name: "Joe C".to_string(),
            house_number: 136,
            street: "Mile High Dr.".to_string(),
            city: "Solana Beach".to_string(),
        }
        .data(),
        account_data_anchor_program::accounts::CreateAddressInfo {
            payer: payer.pubkey(),
            address_info: address_info.pubkey(),
            system_program: system_program::ID,
        }
        .to_account_metas(None),
    );
    let msg = Message::new_with_blockhash(&[ix], Some(&payer.pubkey()), &svm.latest_blockhash());
    let tx = VersionedTransaction::try_new(VersionedMessage::Legacy(msg), &[&payer, &address_info]).unwrap();
    svm.send_transaction(tx).unwrap();

    let account = svm.get_account(&address_info.pubkey()).unwrap();
    let info = AddressInfo::try_deserialize(&mut account.data.as_slice()).unwrap();
    assert_eq!(info.name, "Joe C");
    assert_eq!(info.house_number, 136);
    assert_eq!(info.street, "Mile High Dr.");
    assert_eq!(info.city, "Solana Beach");
}
