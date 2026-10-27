use {
    anchor_lang::{
        solana_program::{
            instruction::Instruction, program_pack::Pack, pubkey::Pubkey, system_instruction, system_program,
        },
        AccountDeserialize, InstructionData, ToAccountMetas,
    },
    anchor_spl::{
        associated_token::{
            get_associated_token_address_with_program_id,
            spl_associated_token_account::instruction::create_associated_token_account_idempotent,
            ID as ASSOCIATED_TOKEN_PROGRAM_ID,
        },
        token_2022::{spl_token_2022, ID as TOKEN_PROGRAM_ID},
        token_interface::TokenAccount,
    },
    escrow::Offer,
    litesvm::LiteSVM,
    solana_keypair::Keypair,
    solana_message::{Message, VersionedMessage},
    solana_signer::Signer,
    solana_transaction::versioned::VersionedTransaction,
};

type SendResult = Result<(), Vec<String>>;

const INITIAL_BALANCE: u64 = 1_000_000_000;
const TOKEN_A_OFFERED_AMOUNT: u64 = 1_000_000;
const TOKEN_B_WANTED_AMOUNT: u64 = 1_000_000;

struct Env {
    svm: LiteSVM,
    alice: Keypair,
    bob: Keypair,
    mint_a: Pubkey,
    mint_b: Pubkey,
}

fn ata(owner: &Pubkey, mint: &Pubkey) -> Pubkey {
    get_associated_token_address_with_program_id(owner, mint, &TOKEN_PROGRAM_ID)
}

fn send(svm: &mut LiteSVM, ixs: &[Instruction], signers: &[&Keypair]) -> SendResult {
    let msg = Message::new_with_blockhash(ixs, Some(&signers[0].pubkey()), &svm.latest_blockhash());
    let tx = VersionedTransaction::try_new(VersionedMessage::Legacy(msg), signers).unwrap();
    svm.send_transaction(tx).map(|_| ()).map_err(|err| err.meta.logs)
}

fn create_mint_with_balance(svm: &mut LiteSVM, authority: &Keypair) -> Pubkey {
    let mint = Keypair::new();
    let space = spl_token_2022::state::Mint::LEN;
    let holder = ata(&authority.pubkey(), &mint.pubkey());
    let ixs = [
        system_instruction::create_account(
            &authority.pubkey(),
            &mint.pubkey(),
            svm.minimum_balance_for_rent_exemption(space),
            space as u64,
            &TOKEN_PROGRAM_ID,
        ),
        spl_token_2022::instruction::initialize_mint2(&TOKEN_PROGRAM_ID, &mint.pubkey(), &authority.pubkey(), None, 6)
            .unwrap(),
        create_associated_token_account_idempotent(
            &authority.pubkey(),
            &authority.pubkey(),
            &mint.pubkey(),
            &TOKEN_PROGRAM_ID,
        ),
        spl_token_2022::instruction::mint_to(
            &TOKEN_PROGRAM_ID,
            &mint.pubkey(),
            &holder,
            &authority.pubkey(),
            &[],
            INITIAL_BALANCE,
        )
        .unwrap(),
    ];
    send(svm, &ixs, &[authority, &mint]).unwrap();
    mint.pubkey()
}

fn setup() -> Env {
    let mut svm = LiteSVM::new();
    let bytes = include_bytes!(concat!(env!("CARGO_MANIFEST_DIR"), "/../../target/deploy/escrow.so"));
    svm.add_program(escrow::id(), bytes).unwrap();
    let alice = Keypair::new();
    let bob = Keypair::new();
    svm.airdrop(&alice.pubkey(), 10_000_000_000).unwrap();
    svm.airdrop(&bob.pubkey(), 10_000_000_000).unwrap();
    let mint_a = create_mint_with_balance(&mut svm, &alice);
    let mint_b = create_mint_with_balance(&mut svm, &bob);
    Env { svm, alice, bob, mint_a, mint_b }
}

fn token_balance(svm: &LiteSVM, address: &Pubkey) -> u64 {
    let account = svm.get_account(address).unwrap();
    TokenAccount::try_deserialize(&mut account.data.as_slice()).unwrap().amount
}

fn offer_address(maker: &Pubkey, id: u64) -> Pubkey {
    Pubkey::find_program_address(&[b"offer", maker.as_ref(), &id.to_le_bytes()], &escrow::id()).0
}

fn make_offer(env: &mut Env, id: u64) -> Pubkey {
    let maker = env.alice.pubkey();
    let offer = offer_address(&maker, id);
    let vault = ata(&offer, &env.mint_a);
    let ix = Instruction::new_with_bytes(
        escrow::id(),
        &escrow::instruction::MakeOffer {
            id,
            token_a_offered_amount: TOKEN_A_OFFERED_AMOUNT,
            token_b_wanted_amount: TOKEN_B_WANTED_AMOUNT,
        }
        .data(),
        escrow::accounts::MakeOffer {
            maker,
            token_mint_a: env.mint_a,
            token_mint_b: env.mint_b,
            maker_token_account_a: ata(&maker, &env.mint_a),
            offer,
            vault,
            associated_token_program: ASSOCIATED_TOKEN_PROGRAM_ID,
            token_program: TOKEN_PROGRAM_ID,
            system_program: system_program::ID,
        }
        .to_account_metas(None),
    );
    send(&mut env.svm, &[ix], &[&env.alice]).unwrap();

    assert_eq!(token_balance(&env.svm, &vault), TOKEN_A_OFFERED_AMOUNT);
    let account = env.svm.get_account(&offer).unwrap();
    let state = Offer::try_deserialize(&mut account.data.as_slice()).unwrap();
    assert_eq!(state.id, id);
    assert_eq!(state.maker, maker);
    assert_eq!(state.token_mint_a, env.mint_a);
    assert_eq!(state.token_mint_b, env.mint_b);
    assert_eq!(state.token_b_wanted_amount, TOKEN_B_WANTED_AMOUNT);
    offer
}

fn refund_offer(env: &Env, maker: Pubkey, offer: Pubkey) -> Instruction {
    Instruction::new_with_bytes(
        escrow::id(),
        &escrow::instruction::RefundOffer {}.data(),
        escrow::accounts::RefundOffer {
            maker,
            token_mint_a: env.mint_a,
            maker_token_account_a: ata(&maker, &env.mint_a),
            offer,
            vault: ata(&offer, &env.mint_a),
            token_program: TOKEN_PROGRAM_ID,
        }
        .to_account_metas(None),
    )
}

#[test]
fn make_offer_moves_offered_tokens_into_vault() {
    let mut env = setup();
    make_offer(&mut env, 1);
    assert_eq!(
        token_balance(&env.svm, &ata(&env.alice.pubkey(), &env.mint_a)),
        INITIAL_BALANCE - TOKEN_A_OFFERED_AMOUNT
    );
}

#[test]
fn take_offer_swaps_tokens_between_maker_and_taker() {
    let mut env = setup();
    let offer = make_offer(&mut env, 1);
    let maker = env.alice.pubkey();
    let taker = env.bob.pubkey();
    let vault = ata(&offer, &env.mint_a);
    let ix = Instruction::new_with_bytes(
        escrow::id(),
        &escrow::instruction::TakeOffer {}.data(),
        escrow::accounts::TakeOffer {
            taker,
            maker,
            token_mint_a: env.mint_a,
            token_mint_b: env.mint_b,
            taker_token_account_a: ata(&taker, &env.mint_a),
            taker_token_account_b: ata(&taker, &env.mint_b),
            maker_token_account_b: ata(&maker, &env.mint_b),
            offer,
            vault,
            associated_token_program: ASSOCIATED_TOKEN_PROGRAM_ID,
            token_program: TOKEN_PROGRAM_ID,
            system_program: system_program::ID,
        }
        .to_account_metas(None),
    );
    send(&mut env.svm, &[ix], &[&env.bob]).unwrap();

    assert_eq!(token_balance(&env.svm, &ata(&taker, &env.mint_a)), TOKEN_A_OFFERED_AMOUNT);
    assert_eq!(token_balance(&env.svm, &ata(&maker, &env.mint_b)), TOKEN_B_WANTED_AMOUNT);
    assert!(env.svm.get_account(&offer).is_none());
    assert!(env.svm.get_account(&vault).is_none());
}

#[test]
fn refund_offer_returns_vaulted_tokens_to_maker() {
    let mut env = setup();
    let offer = make_offer(&mut env, 1);
    let maker = env.alice.pubkey();
    let maker_ata = ata(&maker, &env.mint_a);
    let before = token_balance(&env.svm, &maker_ata);

    let ix = refund_offer(&env, maker, offer);
    send(&mut env.svm, &[ix], &[&env.alice]).unwrap();

    assert!(env.svm.get_account(&offer).is_none());
    assert!(env.svm.get_account(&ata(&offer, &env.mint_a)).is_none());
    assert_eq!(token_balance(&env.svm, &maker_ata), before + TOKEN_A_OFFERED_AMOUNT);
}

#[test]
fn rejects_refund_from_non_maker() {
    let mut env = setup();
    let offer = make_offer(&mut env, 1);
    let bob = env.bob.pubkey();
    let create_bob_ata = create_associated_token_account_idempotent(&bob, &bob, &env.mint_a, &TOKEN_PROGRAM_ID);
    send(&mut env.svm, &[create_bob_ata], &[&env.bob]).unwrap();

    let ix = refund_offer(&env, bob, offer);
    assert!(send(&mut env.svm, &[ix], &[&env.bob]).is_err());
    assert_eq!(token_balance(&env.svm, &ata(&offer, &env.mint_a)), TOKEN_A_OFFERED_AMOUNT);
}
