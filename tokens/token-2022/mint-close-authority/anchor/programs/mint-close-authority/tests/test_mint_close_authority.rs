use {
    anchor_lang::{
        solana_program::{instruction::Instruction, pubkey::Pubkey, system_program},
        InstructionData, ToAccountMetas,
    },
    anchor_spl::token_2022::spl_token_2022::{
        self,
        extension::{mint_close_authority::MintCloseAuthority, BaseStateWithExtensions, StateWithExtensions},
        state::Mint,
    },
    litesvm::LiteSVM,
    solana_keypair::Keypair,
    solana_message::{Message, VersionedMessage},
    solana_signer::Signer,
    solana_transaction::versioned::VersionedTransaction,
};

fn setup() -> (LiteSVM, Keypair) {
    let mut svm = LiteSVM::new();
    let bytes = include_bytes!(concat!(env!("CARGO_MANIFEST_DIR"), "/../../target/deploy/mint_close_authority.so"));
    svm.add_program(mint_close_authority::id(), bytes).unwrap();
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

fn initialize(payer: &Keypair, mint: &Keypair) -> Instruction {
    Instruction::new_with_bytes(
        mint_close_authority::id(),
        &mint_close_authority::instruction::Initialize {}.data(),
        mint_close_authority::accounts::Initialize {
            payer: payer.pubkey(),
            mint_account: mint.pubkey(),
            token_program: spl_token_2022::ID,
            system_program: system_program::ID,
        }
        .to_account_metas(None),
    )
}

fn assert_close_authority(svm: &LiteSVM, mint: &Pubkey, expected: &Pubkey) {
    let account = svm.get_account(mint).unwrap();
    assert_eq!(account.owner, spl_token_2022::ID);
    let state = StateWithExtensions::<Mint>::unpack(&account.data).unwrap();
    let extension = state.get_extension::<MintCloseAuthority>().unwrap();
    assert_eq!(Option::<Pubkey>::from(extension.close_authority), Some(*expected));
}

fn assert_closed(svm: &LiteSVM, mint: &Pubkey) {
    assert!(svm.get_account(mint).is_none_or(|a| a.lamports == 0));
}

#[test]
fn close_mint_with_anchor_cpi_then_with_token_program() {
    let (mut svm, payer) = setup();
    let mint = Keypair::new();

    send(&mut svm, initialize(&payer, &mint), &payer, &[&payer, &mint]);
    assert_close_authority(&svm, &mint.pubkey(), &payer.pubkey());

    let close = Instruction::new_with_bytes(
        mint_close_authority::id(),
        &mint_close_authority::instruction::Close {}.data(),
        mint_close_authority::accounts::Close {
            authority: payer.pubkey(),
            mint_account: mint.pubkey(),
            token_program: spl_token_2022::ID,
        }
        .to_account_metas(None),
    );
    send(&mut svm, close, &payer, &[&payer]);
    assert_closed(&svm, &mint.pubkey());

    send(&mut svm, initialize(&payer, &mint), &payer, &[&payer, &mint]);
    assert_close_authority(&svm, &mint.pubkey(), &payer.pubkey());

    let close = spl_token_2022::instruction::close_account(
        &spl_token_2022::ID,
        &mint.pubkey(),
        &payer.pubkey(),
        &payer.pubkey(),
        &[],
    )
    .unwrap();
    send(&mut svm, close, &payer, &[&payer]);
    assert_closed(&svm, &mint.pubkey());
}
