use {
    anchor_lang::{
        solana_program::{clock::Clock, instruction::Instruction, pubkey::Pubkey, system_program},
        InstructionData, ToAccountMetas,
    },
    anchor_spl::token_2022::spl_token_2022::{
        self,
        extension::{interest_bearing_mint::InterestBearingConfig, BaseStateWithExtensions, StateWithExtensions},
        instruction::amount_to_ui_amount,
        state::Mint,
    },
    litesvm::LiteSVM,
    solana_keypair::Keypair,
    solana_message::{Message, VersionedMessage},
    solana_signer::Signer,
    solana_transaction::versioned::VersionedTransaction,
};

const SECONDS_PER_YEAR: i64 = 31_556_736;

fn transaction(svm: &LiteSVM, ix: Instruction, payer: &Keypair, signers: &[&Keypair]) -> VersionedTransaction {
    let msg = Message::new_with_blockhash(&[ix], Some(&payer.pubkey()), &svm.latest_blockhash());
    VersionedTransaction::try_new(VersionedMessage::Legacy(msg), signers).unwrap()
}

fn send(svm: &mut LiteSVM, ix: Instruction, payer: &Keypair, signers: &[&Keypair]) {
    let tx = transaction(svm, ix, payer, signers);
    svm.send_transaction(tx).unwrap();
    svm.expire_blockhash();
}

fn interest_config(svm: &LiteSVM, mint: &Pubkey) -> InterestBearingConfig {
    let account = svm.get_account(mint).unwrap();
    let mint = StateWithExtensions::<Mint>::unpack(&account.data).unwrap();
    *mint.get_extension::<InterestBearingConfig>().unwrap()
}

fn ui_amount(svm: &LiteSVM, payer: &Keypair, mint: &Pubkey, amount: u64) -> String {
    let ix = amount_to_ui_amount(&spl_token_2022::ID, mint, amount).unwrap();
    let simulated = svm.simulate_transaction(transaction(svm, ix, payer, &[payer])).unwrap();
    String::from_utf8(simulated.meta.return_data.data).unwrap()
}

#[test]
fn initialize_update_rate_and_accrue() {
    let mut svm = LiteSVM::new();
    let bytes = include_bytes!(concat!(env!("CARGO_MANIFEST_DIR"), "/../../target/deploy/interest_bearing.so"));
    svm.add_program(interest_bearing::id(), bytes).unwrap();
    let payer = Keypair::new();
    svm.airdrop(&payer.pubkey(), 1_000_000_000).unwrap();
    let mint = Keypair::new();

    send(
        &mut svm,
        Instruction::new_with_bytes(
            interest_bearing::id(),
            &interest_bearing::instruction::Initialize { rate: 0 }.data(),
            interest_bearing::accounts::Initialize {
                payer: payer.pubkey(),
                mint_account: mint.pubkey(),
                token_program: spl_token_2022::ID,
                system_program: system_program::ID,
            }
            .to_account_metas(None),
        ),
        &payer,
        &[&payer, &mint],
    );
    let config = interest_config(&svm, &mint.pubkey());
    assert_eq!(i16::from(config.current_rate), 0);
    assert_eq!(Option::<Pubkey>::from(config.rate_authority), Some(payer.pubkey()));

    send(
        &mut svm,
        Instruction::new_with_bytes(
            interest_bearing::id(),
            &interest_bearing::instruction::UpdateRate { rate: 100 }.data(),
            interest_bearing::accounts::UpdateRate {
                authority: payer.pubkey(),
                mint_account: mint.pubkey(),
                token_program: spl_token_2022::ID,
                system_program: system_program::ID,
            }
            .to_account_metas(None),
        ),
        &payer,
        &[&payer],
    );
    assert_eq!(i16::from(interest_config(&svm, &mint.pubkey()).current_rate), 100);

    assert_eq!(ui_amount(&svm, &payer, &mint.pubkey(), 1000), "10");

    let mut clock = svm.get_sysvar::<Clock>();
    clock.unix_timestamp += SECONDS_PER_YEAR;
    svm.set_sysvar(&clock);
    assert_eq!(ui_amount(&svm, &payer, &mint.pubkey(), 1000), "10.1");
}
