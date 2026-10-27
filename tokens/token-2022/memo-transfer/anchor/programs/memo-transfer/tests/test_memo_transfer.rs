use {
    anchor_lang::{
        solana_program::{
            instruction::{AccountMeta, Instruction},
            program_pack::Pack,
            pubkey::Pubkey,
            rent::Rent,
            system_instruction, system_program,
        },
        InstructionData, ToAccountMetas,
    },
    anchor_spl::token_2022::spl_token_2022::{
        self,
        error::TokenError,
        extension::{memo_transfer::MemoTransfer, BaseStateWithExtensions, StateWithExtensions},
        instruction::{initialize_account, initialize_mint2, mint_to, transfer_checked},
        state::{Account as TokenAccountState, Mint},
    },
    litesvm::LiteSVM,
    solana_keypair::Keypair,
    solana_message::{Message, VersionedMessage},
    solana_signer::Signer,
    solana_transaction::{versioned::VersionedTransaction, InstructionError, TransactionError},
};

const MEMO_PROGRAM_ID: Pubkey = Pubkey::from_str_const("MemoSq4gqABAXKb96qnH8TysNcWxMyWCqXgDLGmfcHr");

fn send(svm: &mut LiteSVM, ixs: &[Instruction], payer: &Keypair, signers: &[&Keypair]) -> Result<(), TransactionError> {
    let msg = Message::new_with_blockhash(ixs, Some(&payer.pubkey()), &svm.latest_blockhash());
    let tx = VersionedTransaction::try_new(VersionedMessage::Legacy(msg), signers).unwrap();
    let result = svm.send_transaction(tx).map(|_| ()).map_err(|failed| failed.err);
    svm.expire_blockhash();
    result
}

fn funded_source(svm: &mut LiteSVM, payer: &Keypair, mint: &Pubkey) -> Pubkey {
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
            mint_to(&spl_token_2022::ID, mint, &account.pubkey(), &payer.pubkey(), &[], 1).unwrap(),
        ],
        payer,
        &[payer, &account],
    )
    .unwrap();
    account.pubkey()
}

fn transfer_one(source: &Pubkey, mint: &Pubkey, destination: &Pubkey, owner: &Pubkey) -> Instruction {
    transfer_checked(&spl_token_2022::ID, source, mint, destination, owner, &[], 1, 2).unwrap()
}

fn memo(signer: &Pubkey, text: &str) -> Instruction {
    Instruction::new_with_bytes(MEMO_PROGRAM_ID, text.as_bytes(), vec![AccountMeta::new_readonly(*signer, true)])
}

fn token_account(svm: &LiteSVM, account: &Pubkey) -> (u64, bool) {
    let account = svm.get_account(account).unwrap();
    let state = StateWithExtensions::<TokenAccountState>::unpack(&account.data).unwrap();
    let required = bool::from(state.get_extension::<MemoTransfer>().unwrap().require_incoming_transfer_memos);
    (state.base.amount, required)
}

#[test]
fn required_memo_enforced_until_disabled() {
    let mut svm = LiteSVM::new();
    let bytes = include_bytes!(concat!(env!("CARGO_MANIFEST_DIR"), "/../../target/deploy/memo_transfer.so"));
    svm.add_program(memo_transfer::id(), bytes).unwrap();
    let payer = Keypair::new();
    svm.airdrop(&payer.pubkey(), 1_000_000_000).unwrap();
    let mint = Keypair::new();
    let token = Keypair::new();

    send(
        &mut svm,
        &[
            system_instruction::create_account(
                &payer.pubkey(),
                &mint.pubkey(),
                Rent::default().minimum_balance(Mint::LEN),
                Mint::LEN as u64,
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
            memo_transfer::id(),
            &memo_transfer::instruction::Initialize {}.data(),
            memo_transfer::accounts::Initialize {
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
    assert_eq!(token_account(&svm, &token.pubkey()), (0, true));

    let source = funded_source(&mut svm, &payer, &mint.pubkey());
    let failed =
        send(&mut svm, &[transfer_one(&source, &mint.pubkey(), &token.pubkey(), &payer.pubkey())], &payer, &[&payer])
            .unwrap_err();
    assert_eq!(failed, TransactionError::InstructionError(0, InstructionError::Custom(TokenError::NoMemo as u32)));

    let source = funded_source(&mut svm, &payer, &mint.pubkey());
    send(
        &mut svm,
        &[
            memo(&payer.pubkey(), "hello, world"),
            transfer_one(&source, &mint.pubkey(), &token.pubkey(), &payer.pubkey()),
        ],
        &payer,
        &[&payer],
    )
    .unwrap();
    assert_eq!(token_account(&svm, &token.pubkey()), (1, true));

    send(
        &mut svm,
        &[Instruction::new_with_bytes(
            memo_transfer::id(),
            &memo_transfer::instruction::Disable {}.data(),
            memo_transfer::accounts::Disable {
                owner: payer.pubkey(),
                token_account: token.pubkey(),
                token_program: spl_token_2022::ID,
            }
            .to_account_metas(None),
        )],
        &payer,
        &[&payer],
    )
    .unwrap();
    assert_eq!(token_account(&svm, &token.pubkey()), (1, false));

    let source = funded_source(&mut svm, &payer, &mint.pubkey());
    send(&mut svm, &[transfer_one(&source, &mint.pubkey(), &token.pubkey(), &payer.pubkey())], &payer, &[&payer])
        .unwrap();
    assert_eq!(token_account(&svm, &token.pubkey()), (2, false));
}
