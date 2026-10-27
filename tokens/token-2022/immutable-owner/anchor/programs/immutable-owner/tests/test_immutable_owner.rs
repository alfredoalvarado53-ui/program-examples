use {
    anchor_lang::{
        solana_program::{
            instruction::Instruction, program_pack::Pack, pubkey::Pubkey, rent::Rent, system_instruction,
            system_program,
        },
        InstructionData, ToAccountMetas,
    },
    anchor_spl::token_2022::spl_token_2022::{
        self,
        error::TokenError,
        extension::{immutable_owner::ImmutableOwner, BaseStateWithExtensions, StateWithExtensions},
        instruction::{initialize_mint2, set_authority, AuthorityType},
        state::{Account as TokenAccountState, Mint},
    },
    litesvm::LiteSVM,
    solana_keypair::Keypair,
    solana_message::{Message, VersionedMessage},
    solana_signer::Signer,
    solana_transaction::{versioned::VersionedTransaction, InstructionError, TransactionError},
};

fn send(svm: &mut LiteSVM, ixs: &[Instruction], payer: &Keypair, signers: &[&Keypair]) -> Result<(), TransactionError> {
    let msg = Message::new_with_blockhash(ixs, Some(&payer.pubkey()), &svm.latest_blockhash());
    let tx = VersionedTransaction::try_new(VersionedMessage::Legacy(msg), signers).unwrap();
    let result = svm.send_transaction(tx).map(|_| ()).map_err(|failed| failed.err);
    svm.expire_blockhash();
    result
}

#[test]
fn owner_cannot_be_changed() {
    let mut svm = LiteSVM::new();
    let bytes = include_bytes!(concat!(env!("CARGO_MANIFEST_DIR"), "/../../target/deploy/immutable_owner.so"));
    svm.add_program(immutable_owner::id(), bytes).unwrap();
    let payer = Keypair::new();
    svm.airdrop(&payer.pubkey(), 1_000_000_000).unwrap();
    let mint = Keypair::new();
    let token = Keypair::new();

    let mint_len = Mint::LEN;
    send(
        &mut svm,
        &[
            system_instruction::create_account(
                &payer.pubkey(),
                &mint.pubkey(),
                Rent::default().minimum_balance(mint_len),
                mint_len as u64,
                &spl_token_2022::ID,
            ),
            initialize_mint2(&spl_token_2022::ID, &mint.pubkey(), &payer.pubkey(), None, 2).unwrap(),
        ],
        &payer,
        &[&payer, &mint],
    )
    .unwrap();

    send(
        &mut svm,
        &[Instruction::new_with_bytes(
            immutable_owner::id(),
            &immutable_owner::instruction::Initialize {}.data(),
            immutable_owner::accounts::Initialize {
                payer: payer.pubkey(),
                token_account: token.pubkey(),
                mint_account: mint.pubkey(),
                token_program: spl_token_2022::ID,
                system_program: system_program::ID,
            }
            .to_account_metas(None),
        )],
        &payer,
        &[&payer, &token],
    )
    .unwrap();

    let account = svm.get_account(&token.pubkey()).unwrap();
    let state = StateWithExtensions::<TokenAccountState>::unpack(&account.data).unwrap();
    assert!(state.get_extension::<ImmutableOwner>().is_ok());
    assert_eq!(state.base.owner, payer.pubkey());

    let failed = send(
        &mut svm,
        &[set_authority(
            &spl_token_2022::ID,
            &token.pubkey(),
            Some(&Pubkey::new_unique()),
            AuthorityType::AccountOwner,
            &payer.pubkey(),
            &[],
        )
        .unwrap()],
        &payer,
        &[&payer],
    )
    .unwrap_err();
    assert_eq!(
        failed,
        TransactionError::InstructionError(0, InstructionError::Custom(TokenError::ImmutableOwner as u32))
    );

    let account = svm.get_account(&token.pubkey()).unwrap();
    let state = StateWithExtensions::<TokenAccountState>::unpack(&account.data).unwrap();
    assert_eq!(state.base.owner, payer.pubkey());
}
