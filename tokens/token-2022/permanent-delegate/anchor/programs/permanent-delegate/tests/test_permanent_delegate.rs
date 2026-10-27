use {
    anchor_lang::{
        solana_program::{instruction::Instruction, pubkey::Pubkey, system_program},
        InstructionData, ToAccountMetas,
    },
    anchor_spl::{
        associated_token::{
            get_associated_token_address_with_program_id,
            spl_associated_token_account::instruction::create_associated_token_account,
        },
        token_2022::spl_token_2022::{
            self,
            extension::{permanent_delegate::PermanentDelegate, BaseStateWithExtensions, StateWithExtensions},
            state::{Account as TokenAccount, Mint},
        },
    },
    litesvm::LiteSVM,
    solana_keypair::Keypair,
    solana_message::{Message, VersionedMessage},
    solana_signer::Signer,
    solana_transaction::versioned::VersionedTransaction,
};

fn setup() -> (LiteSVM, Keypair) {
    let mut svm = LiteSVM::new();
    let bytes = include_bytes!(concat!(env!("CARGO_MANIFEST_DIR"), "/../../target/deploy/permanent_delegate.so"));
    svm.add_program(permanent_delegate::id(), bytes).unwrap();
    let payer = Keypair::new();
    svm.airdrop(&payer.pubkey(), 1_000_000_000).unwrap();
    (svm, payer)
}

fn send(svm: &mut LiteSVM, ixs: &[Instruction], payer: &Keypair, signers: &[&Keypair]) {
    let msg = Message::new_with_blockhash(ixs, Some(&payer.pubkey()), &svm.latest_blockhash());
    let tx = VersionedTransaction::try_new(VersionedMessage::Legacy(msg), signers).unwrap();
    svm.send_transaction(tx).unwrap();
    svm.expire_blockhash();
}

fn balance(svm: &LiteSVM, token_account: &Pubkey) -> u64 {
    let account = svm.get_account(token_account).unwrap();
    StateWithExtensions::<TokenAccount>::unpack(&account.data).unwrap().base.amount
}

#[test]
fn permanent_delegate_burns_from_any_token_account() {
    let (mut svm, payer) = setup();
    let mint = Keypair::new();

    let initialize = Instruction::new_with_bytes(
        permanent_delegate::id(),
        &permanent_delegate::instruction::Initialize {}.data(),
        permanent_delegate::accounts::Initialize {
            payer: payer.pubkey(),
            mint_account: mint.pubkey(),
            token_program: spl_token_2022::ID,
            system_program: system_program::ID,
        }
        .to_account_metas(None),
    );
    send(&mut svm, &[initialize], &payer, &[&payer, &mint]);

    let mint_account = svm.get_account(&mint.pubkey()).unwrap();
    let mint_state = StateWithExtensions::<Mint>::unpack(&mint_account.data).unwrap();
    let extension = mint_state.get_extension::<PermanentDelegate>().unwrap();
    assert_eq!(Option::<Pubkey>::from(extension.delegate), Some(payer.pubkey()));

    let owner = Keypair::new();
    let token_account =
        get_associated_token_address_with_program_id(&owner.pubkey(), &mint.pubkey(), &spl_token_2022::ID);
    let amount = 100;
    let setup_ixs = [
        create_associated_token_account(&payer.pubkey(), &owner.pubkey(), &mint.pubkey(), &spl_token_2022::ID),
        spl_token_2022::instruction::mint_to(
            &spl_token_2022::ID,
            &mint.pubkey(),
            &token_account,
            &payer.pubkey(),
            &[],
            amount,
        )
        .unwrap(),
    ];
    send(&mut svm, &setup_ixs, &payer, &[&payer]);
    assert_eq!(balance(&svm, &token_account), amount);

    let burn = spl_token_2022::instruction::burn_checked(
        &spl_token_2022::ID,
        &token_account,
        &mint.pubkey(),
        &payer.pubkey(),
        &[],
        amount,
        2,
    )
    .unwrap();
    send(&mut svm, &[burn], &payer, &[&payer]);
    assert_eq!(balance(&svm, &token_account), 0);
}
