use {
    anchor_lang::{
        error::ErrorCode,
        solana_program::{instruction::Instruction, system_instruction, system_program},
        InstructionData, ToAccountMetas,
    },
    litesvm::LiteSVM,
    solana_keypair::Keypair,
    solana_message::{Message, VersionedMessage},
    solana_signer::Signer,
    solana_transaction::{versioned::VersionedTransaction, InstructionError, TransactionError},
};

const LAMPORTS_PER_SOL: u64 = 1_000_000_000;

fn setup() -> (LiteSVM, Keypair) {
    let mut svm = LiteSVM::new();
    let bytes = include_bytes!(concat!(env!("CARGO_MANIFEST_DIR"), "/../../target/deploy/transfer_sol.so"));
    svm.add_program(transfer_sol::id(), bytes).unwrap();
    let payer = Keypair::new();
    svm.airdrop(&payer.pubkey(), 5 * LAMPORTS_PER_SOL).unwrap();
    (svm, payer)
}

fn send(svm: &mut LiteSVM, ix: Instruction, payer: &Keypair, signers: &[&Keypair]) -> Result<(), TransactionError> {
    let msg = Message::new_with_blockhash(&[ix], Some(&payer.pubkey()), &svm.latest_blockhash());
    let tx = VersionedTransaction::try_new(VersionedMessage::Legacy(msg), signers).unwrap();
    let result = svm.send_transaction(tx).map(|_| ()).map_err(|failed| failed.err);
    svm.expire_blockhash();
    result
}

fn create_program_owned_account(svm: &mut LiteSVM, payer: &Keypair) -> Keypair {
    let account = Keypair::new();
    let ix = system_instruction::create_account(
        &payer.pubkey(),
        &account.pubkey(),
        LAMPORTS_PER_SOL,
        0,
        &transfer_sol::id(),
    );
    send(svm, ix, payer, &[payer, &account]).unwrap();
    account
}

#[test]
fn transfer_sol_with_cpi() {
    let (mut svm, payer) = setup();
    let recipient = Keypair::new();

    let ix = Instruction::new_with_bytes(
        transfer_sol::id(),
        &transfer_sol::instruction::TransferSolWithCpi { amount: LAMPORTS_PER_SOL }.data(),
        transfer_sol::accounts::TransferSolWithCpi {
            payer: payer.pubkey(),
            recipient: recipient.pubkey(),
            system_program: system_program::ID,
        }
        .to_account_metas(None),
    );
    send(&mut svm, ix, &payer, &[&payer]).unwrap();

    assert_eq!(svm.get_balance(&recipient.pubkey()), Some(LAMPORTS_PER_SOL));
}

#[test]
fn transfer_sol_with_program() {
    let (mut svm, payer) = setup();
    let payer_account = create_program_owned_account(&mut svm, &payer);
    let recipient = Keypair::new();

    let ix = Instruction::new_with_bytes(
        transfer_sol::id(),
        &transfer_sol::instruction::TransferSolWithProgram { amount: LAMPORTS_PER_SOL }.data(),
        transfer_sol::accounts::TransferSolWithProgram { payer: payer_account.pubkey(), recipient: recipient.pubkey() }
            .to_account_metas(None),
    );
    send(&mut svm, ix, &payer, &[&payer, &payer_account]).unwrap();

    assert_eq!(svm.get_balance(&recipient.pubkey()), Some(LAMPORTS_PER_SOL));
}

#[test]
fn attacker_cannot_drain_program_owned_account() {
    let (mut svm, payer) = setup();
    let victim = create_program_owned_account(&mut svm, &payer);
    let attacker_recipient = Keypair::new();

    let mut ix = Instruction::new_with_bytes(
        transfer_sol::id(),
        &transfer_sol::instruction::TransferSolWithProgram { amount: LAMPORTS_PER_SOL }.data(),
        transfer_sol::accounts::TransferSolWithProgram {
            payer: victim.pubkey(),
            recipient: attacker_recipient.pubkey(),
        }
        .to_account_metas(None),
    );
    ix.accounts[0].is_signer = false;

    let err = send(&mut svm, ix, &payer, &[&payer]).unwrap_err();
    assert_eq!(
        err,
        TransactionError::InstructionError(0, InstructionError::Custom(ErrorCode::AccountNotSigner.into()))
    );
    assert_eq!(svm.get_balance(&victim.pubkey()), Some(LAMPORTS_PER_SOL));
    assert_eq!(svm.get_balance(&attacker_recipient.pubkey()), None);
}
