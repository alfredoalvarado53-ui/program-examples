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
        extension::{default_account_state::DefaultAccountState, BaseStateWithExtensions, StateWithExtensions},
        instruction::{initialize_account, mint_to},
        state::{Account as TokenAccountState, AccountState, Mint},
    },
    default_account_state::AnchorAccountState,
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

fn create_token_account(svm: &mut LiteSVM, payer: &Keypair, mint: &Pubkey) -> Pubkey {
    let account = Keypair::new();
    let len = TokenAccountState::LEN;
    send(
        svm,
        &[
            system_instruction::create_account(
                &payer.pubkey(),
                &account.pubkey(),
                Rent::default().minimum_balance(len),
                len as u64,
                &spl_token_2022::ID,
            ),
            initialize_account(&spl_token_2022::ID, &account.pubkey(), mint, &payer.pubkey()).unwrap(),
        ],
        payer,
        &[payer, &account],
    )
    .unwrap();
    account.pubkey()
}

fn mint_one(svm: &mut LiteSVM, payer: &Keypair, mint: &Pubkey, account: &Pubkey) -> Result<(), TransactionError> {
    let ix = mint_to(&spl_token_2022::ID, mint, account, &payer.pubkey(), &[], 1).unwrap();
    send(svm, &[ix], payer, &[payer])
}

fn token_account(svm: &LiteSVM, account: &Pubkey) -> TokenAccountState {
    let account = svm.get_account(account).unwrap();
    StateWithExtensions::<TokenAccountState>::unpack(&account.data).unwrap().base
}

fn default_state(svm: &LiteSVM, mint: &Pubkey) -> u8 {
    let account = svm.get_account(mint).unwrap();
    let mint = StateWithExtensions::<Mint>::unpack(&account.data).unwrap();
    mint.get_extension::<DefaultAccountState>().unwrap().state
}

#[test]
fn default_state_frozen_then_initialized() {
    let mut svm = LiteSVM::new();
    let bytes = include_bytes!(concat!(env!("CARGO_MANIFEST_DIR"), "/../../target/deploy/default_account_state.so"));
    svm.add_program(default_account_state::id(), bytes).unwrap();
    let payer = Keypair::new();
    svm.airdrop(&payer.pubkey(), 1_000_000_000).unwrap();
    let mint = Keypair::new();

    send(
        &mut svm,
        &[Instruction::new_with_bytes(
            default_account_state::id(),
            &default_account_state::instruction::Initialize {}.data(),
            default_account_state::accounts::Initialize {
                payer: payer.pubkey(),
                mint_account: mint.pubkey(),
                token_program: spl_token_2022::ID,
                system_program: system_program::ID,
            }
            .to_account_metas(None),
        )],
        &payer,
        &[&payer, &mint],
    )
    .unwrap();
    assert_eq!(default_state(&svm, &mint.pubkey()), AccountState::Frozen as u8);

    let frozen = create_token_account(&mut svm, &payer, &mint.pubkey());
    assert_eq!(token_account(&svm, &frozen).state, AccountState::Frozen);
    let failed = mint_one(&mut svm, &payer, &mint.pubkey(), &frozen).unwrap_err();
    assert_eq!(
        failed,
        TransactionError::InstructionError(0, InstructionError::Custom(TokenError::AccountFrozen as u32))
    );

    send(
        &mut svm,
        &[Instruction::new_with_bytes(
            default_account_state::id(),
            &default_account_state::instruction::UpdateDefaultState { account_state: AnchorAccountState::Initialized }
                .data(),
            default_account_state::accounts::UpdateDefaultState {
                freeze_authority: payer.pubkey(),
                mint_account: mint.pubkey(),
                token_program: spl_token_2022::ID,
                system_program: system_program::ID,
            }
            .to_account_metas(None),
        )],
        &payer,
        &[&payer],
    )
    .unwrap();
    assert_eq!(default_state(&svm, &mint.pubkey()), AccountState::Initialized as u8);

    let initialized = create_token_account(&mut svm, &payer, &mint.pubkey());
    assert_eq!(token_account(&svm, &initialized).state, AccountState::Initialized);
    mint_one(&mut svm, &payer, &mint.pubkey(), &initialized).unwrap();
    assert_eq!(token_account(&svm, &initialized).amount, 1);
}
