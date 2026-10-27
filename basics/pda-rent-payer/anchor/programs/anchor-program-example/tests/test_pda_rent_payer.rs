use {
    anchor_lang::{
        solana_program::{instruction::Instruction, pubkey::Pubkey, system_program},
        InstructionData, ToAccountMetas,
    },
    litesvm::LiteSVM,
    solana_keypair::Keypair,
    solana_message::{Message, VersionedMessage},
    solana_signer::Signer,
    solana_transaction::versioned::VersionedTransaction,
};

const LAMPORTS_PER_SOL: u64 = 1_000_000_000;

fn setup() -> (LiteSVM, Keypair) {
    let mut svm = LiteSVM::new();
    let bytes = include_bytes!(concat!(env!("CARGO_MANIFEST_DIR"), "/../../target/deploy/pda_rent_payer.so"));
    svm.add_program(pda_rent_payer::id(), bytes).unwrap();
    let payer = Keypair::new();
    svm.airdrop(&payer.pubkey(), 10 * LAMPORTS_PER_SOL).unwrap();
    (svm, payer)
}

fn send(svm: &mut LiteSVM, ix: Instruction, payer: &Keypair, signers: &[&Keypair]) {
    let msg = Message::new_with_blockhash(&[ix], Some(&payer.pubkey()), &svm.latest_blockhash());
    let tx = VersionedTransaction::try_new(VersionedMessage::Legacy(msg), signers).unwrap();
    svm.send_transaction(tx).unwrap();
}

fn rent_vault() -> Pubkey {
    Pubkey::find_program_address(&[b"rent_vault"], &pda_rent_payer::id()).0
}

fn init_rent_vault(svm: &mut LiteSVM, payer: &Keypair) {
    let ix = Instruction::new_with_bytes(
        pda_rent_payer::id(),
        &pda_rent_payer::instruction::InitRentVault { fund_lamports: LAMPORTS_PER_SOL }.data(),
        pda_rent_payer::accounts::InitRentVault {
            payer: payer.pubkey(),
            rent_vault: rent_vault(),
            system_program: system_program::ID,
        }
        .to_account_metas(None),
    );
    send(svm, ix, payer, &[payer]);
}

#[test]
fn init_rent_vault_funds_pda() {
    let (mut svm, payer) = setup();
    init_rent_vault(&mut svm, &payer);
    assert_eq!(svm.get_account(&rent_vault()).unwrap().lamports, LAMPORTS_PER_SOL);
}

#[test]
fn create_new_account_paid_by_rent_vault() {
    let (mut svm, payer) = setup();
    init_rent_vault(&mut svm, &payer);
    let new_account = Keypair::new();

    let ix = Instruction::new_with_bytes(
        pda_rent_payer::id(),
        &pda_rent_payer::instruction::CreateNewAccount {}.data(),
        pda_rent_payer::accounts::CreateNewAccount {
            new_account: new_account.pubkey(),
            rent_vault: rent_vault(),
            system_program: system_program::ID,
        }
        .to_account_metas(None),
    );
    send(&mut svm, ix, &payer, &[&payer, &new_account]);

    let min_lamports = svm.minimum_balance_for_rent_exemption(0);
    assert_eq!(svm.get_account(&new_account.pubkey()).unwrap().lamports, min_lamports);
    assert_eq!(svm.get_account(&rent_vault()).unwrap().lamports, LAMPORTS_PER_SOL - min_lamports);
}
