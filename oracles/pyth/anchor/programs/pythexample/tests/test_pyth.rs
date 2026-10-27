use {
    anchor_lang::{
        error::ErrorCode,
        prelude::pubkey,
        solana_program::{instruction::Instruction, pubkey::Pubkey},
        AccountDeserialize, InstructionData, ToAccountMetas,
    },
    litesvm::LiteSVM,
    pyth_solana_receiver_sdk::price_update::PriceUpdateV2,
    solana_account::Account,
    solana_keypair::Keypair,
    solana_message::{Message, VersionedMessage},
    solana_signer::Signer,
    solana_transaction::{versioned::VersionedTransaction, InstructionError, TransactionError},
};

const PRICE_UPDATE: Pubkey = pubkey!("7UVimffxr9ow1uXYxsr4LHAcV58mLzhmwaeKvJ1pjLiE");

fn setup() -> (LiteSVM, Keypair, PriceUpdateV2) {
    let mut svm = LiteSVM::new();
    let bytes = include_bytes!(concat!(env!("CARGO_MANIFEST_DIR"), "/../../target/deploy/pythexample.so"));
    svm.add_program(pythexample::id(), bytes).unwrap();
    let payer = Keypair::new();
    svm.airdrop(&payer.pubkey(), 1_000_000_000).unwrap();

    let fixture = concat!(env!("CARGO_MANIFEST_DIR"), "/../../tests/fixtures/sol_usd_price_update.bin");
    let data = std::fs::read(fixture).expect("price update fixture missing: run `anchor test`");
    let price_update = PriceUpdateV2::try_deserialize(&mut data.as_slice()).unwrap();
    let account = Account {
        lamports: svm.minimum_balance_for_rent_exemption(data.len()),
        data,
        owner: pyth_solana_receiver_sdk::id(),
        executable: false,
        rent_epoch: 0,
    };
    svm.set_account(PRICE_UPDATE, account).unwrap();
    (svm, payer, price_update)
}

fn read_price(svm: &mut LiteSVM, payer: &Keypair, price_update: Pubkey) -> Result<Vec<String>, TransactionError> {
    let ix = Instruction::new_with_bytes(
        pythexample::id(),
        &pythexample::instruction::ReadPrice {}.data(),
        pythexample::accounts::ReadPrice { price_update }.to_account_metas(None),
    );
    let msg = Message::new_with_blockhash(&[ix], Some(&payer.pubkey()), &svm.latest_blockhash());
    let tx = VersionedTransaction::try_new(VersionedMessage::Legacy(msg), &[payer]).unwrap();
    svm.send_transaction(tx).map(|meta| meta.logs).map_err(|failed| failed.err)
}

#[test]
fn reads_sol_usd_price_feed() {
    let (mut svm, payer, price_update) = setup();
    let logs = read_price(&mut svm, &payer, PRICE_UPDATE).unwrap();

    let message = price_update.price_message;
    assert!(logs.contains(&format!("Program log: Price: {}", message.price)));
    assert!(logs.contains(&format!("Program log: Exponent: {}", message.exponent)));
}

#[test]
fn rejects_account_not_owned_by_pyth_receiver() {
    let (mut svm, payer, _) = setup();
    let err = read_price(&mut svm, &payer, payer.pubkey()).unwrap_err();
    assert_eq!(
        err,
        TransactionError::InstructionError(0, InstructionError::Custom(ErrorCode::AccountOwnedByWrongProgram.into()))
    );
}
