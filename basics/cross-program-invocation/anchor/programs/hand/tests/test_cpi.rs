use {
    anchor_lang::{
        solana_program::{instruction::Instruction, pubkey::Pubkey, system_program},
        AccountDeserialize, InstructionData, ToAccountMetas,
    },
    hand::lever::{self, accounts::PowerStatus, client},
    litesvm::LiteSVM,
    solana_keypair::Keypair,
    solana_message::{Message, VersionedMessage},
    solana_signer::Signer,
    solana_transaction::versioned::VersionedTransaction,
};

fn send(svm: &mut LiteSVM, ix: Instruction, payer: &Keypair, signers: &[&Keypair]) {
    let msg = Message::new_with_blockhash(&[ix], Some(&payer.pubkey()), &svm.latest_blockhash());
    let tx = VersionedTransaction::try_new(VersionedMessage::Legacy(msg), signers).unwrap();
    svm.send_transaction(tx).unwrap();
}

fn is_on(svm: &LiteSVM, power: &Pubkey) -> bool {
    let account = svm.get_account(power).unwrap();
    PowerStatus::try_deserialize(&mut account.data.as_slice()).unwrap().is_on
}

fn pull_lever(power: Pubkey, name: &str) -> Instruction {
    Instruction::new_with_bytes(
        hand::id(),
        &hand::instruction::PullLever { name: name.to_string() }.data(),
        hand::accounts::PullLever { power, lever_program: lever::ID }.to_account_metas(None),
    )
}

#[test]
fn initialize_and_pull_the_lever() {
    let mut svm = LiteSVM::new();
    let deploy = concat!(env!("CARGO_MANIFEST_DIR"), "/../../target/deploy");
    svm.add_program_from_file(hand::id(), format!("{deploy}/hand.so")).unwrap();
    svm.add_program_from_file(lever::ID, format!("{deploy}/lever.so")).unwrap();
    let payer = Keypair::new();
    svm.airdrop(&payer.pubkey(), 1_000_000_000).unwrap();
    let power = Keypair::new();

    let initialize = Instruction::new_with_bytes(
        lever::ID,
        &client::args::Initialize {}.data(),
        client::accounts::Initialize {
            power: power.pubkey(),
            user: payer.pubkey(),
            system_program: system_program::ID,
        }
        .to_account_metas(None),
    );
    send(&mut svm, initialize, &payer, &[&payer, &power]);
    let account = svm.get_account(&power.pubkey()).unwrap();
    assert_eq!(account.lamports, svm.minimum_balance_for_rent_exemption(8 + 8));
    assert!(!is_on(&svm, &power.pubkey()));

    send(&mut svm, pull_lever(power.pubkey(), "Jacob"), &payer, &[&payer]);
    assert!(is_on(&svm, &power.pubkey()));

    send(&mut svm, pull_lever(power.pubkey(), "sol-warrior"), &payer, &[&payer]);
    assert!(!is_on(&svm, &power.pubkey()));
}
