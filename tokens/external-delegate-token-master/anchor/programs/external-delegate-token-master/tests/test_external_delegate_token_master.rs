use {
    anchor_lang::{
        solana_program::{
            instruction::Instruction, program_pack::Pack, pubkey::Pubkey, system_instruction, system_program,
        },
        AccountDeserialize, InstructionData, ToAccountMetas,
    },
    anchor_spl::{
        associated_token::{
            get_associated_token_address, spl_associated_token_account::instruction::create_associated_token_account,
        },
        token::{spl_token, TokenAccount, ID as TOKEN_PROGRAM_ID},
    },
    external_delegate_token_master::UserAccount,
    k256::ecdsa::SigningKey,
    litesvm::LiteSVM,
    sha3::{Digest, Keccak256},
    solana_keypair::Keypair,
    solana_message::{Message, VersionedMessage},
    solana_signer::Signer,
    solana_transaction::versioned::VersionedTransaction,
};

type SendResult = Result<(), Vec<String>>;

const TRANSFER_DOMAIN: &[u8] = b"external-delegate-token-master:transfer_tokens:v1";
const INITIAL_BALANCE: u64 = 1_000_000_000;

struct Env {
    svm: LiteSVM,
    authority: Keypair,
    user_account: Pubkey,
    user_pda: Pubkey,
    user_token_account: Pubkey,
    recipient_token_account: Pubkey,
}

fn send(svm: &mut LiteSVM, ixs: &[Instruction], signers: &[&Keypair]) -> SendResult {
    let msg = Message::new_with_blockhash(ixs, Some(&signers[0].pubkey()), &svm.latest_blockhash());
    let tx = VersionedTransaction::try_new(VersionedMessage::Legacy(msg), signers).unwrap();
    let result = svm.send_transaction(tx).map(|_| ()).map_err(|err| err.meta.logs);
    svm.expire_blockhash();
    result
}

fn user_account_state(env: &Env) -> UserAccount {
    let account = env.svm.get_account(&env.user_account).unwrap();
    UserAccount::try_deserialize(&mut account.data.as_slice()).unwrap()
}

fn token_balance(svm: &LiteSVM, address: &Pubkey) -> u64 {
    let account = svm.get_account(address).unwrap();
    TokenAccount::try_deserialize(&mut account.data.as_slice()).unwrap().amount
}

fn eth_key(seed: u8) -> SigningKey {
    SigningKey::from_bytes(&[seed; 32].into()).unwrap()
}

fn eth_address(key: &SigningKey) -> [u8; 20] {
    let uncompressed = key.verifying_key().to_encoded_point(false);
    let hash = Keccak256::digest(&uncompressed.as_bytes()[1..]);
    hash[12..].try_into().unwrap()
}

fn transfer_digest(env: &Env, amount: u64, nonce: u64) -> [u8; 32] {
    let mut hasher = Keccak256::new();
    hasher.update(TRANSFER_DOMAIN);
    hasher.update(external_delegate_token_master::id().as_ref());
    hasher.update(env.user_account.as_ref());
    hasher.update(env.user_token_account.as_ref());
    hasher.update(env.recipient_token_account.as_ref());
    hasher.update(amount.to_le_bytes());
    hasher.update(nonce.to_le_bytes());
    hasher.finalize().into()
}

fn sign(digest: &[u8; 32], key: &SigningKey) -> [u8; 65] {
    let (signature, recovery_id) = key.sign_prehash_recoverable(digest).unwrap();
    let mut out = [0u8; 65];
    out[..64].copy_from_slice(&signature.to_bytes());
    out[64] = recovery_id.to_byte();
    out
}

fn set_ethereum_address(env: &mut Env, ethereum_address: [u8; 20]) {
    let ix = Instruction::new_with_bytes(
        external_delegate_token_master::id(),
        &external_delegate_token_master::instruction::SetEthereumAddress { ethereum_address }.data(),
        external_delegate_token_master::accounts::SetEthereumAddress {
            user_account: env.user_account,
            authority: env.authority.pubkey(),
        }
        .to_account_metas(None),
    );
    send(&mut env.svm, &[ix], &[&env.authority]).unwrap();
}

fn transfer_tokens(env: &mut Env, amount: u64, signature: [u8; 65]) -> SendResult {
    let ix = Instruction::new_with_bytes(
        external_delegate_token_master::id(),
        &external_delegate_token_master::instruction::TransferTokens { amount, signature }.data(),
        external_delegate_token_master::accounts::TransferTokens {
            user_account: env.user_account,
            authority: env.authority.pubkey(),
            user_token_account: env.user_token_account,
            recipient_token_account: env.recipient_token_account,
            user_pda: env.user_pda,
            token_program: TOKEN_PROGRAM_ID,
        }
        .to_account_metas(None),
    );
    send(&mut env.svm, &[ix], &[&env.authority])
}

fn assert_invalid_signature(result: SendResult) {
    let logs = result.unwrap_err();
    assert!(logs.iter().any(|log| log.contains("Error Code: InvalidSignature")));
}

fn setup() -> Env {
    let mut svm = LiteSVM::new();
    let bytes =
        include_bytes!(concat!(env!("CARGO_MANIFEST_DIR"), "/../../target/deploy/external_delegate_token_master.so"));
    svm.add_program(external_delegate_token_master::id(), bytes).unwrap();
    let authority = Keypair::new();
    svm.airdrop(&authority.pubkey(), 10_000_000_000).unwrap();

    let user_account = Keypair::new();
    let initialize = Instruction::new_with_bytes(
        external_delegate_token_master::id(),
        &external_delegate_token_master::instruction::Initialize {}.data(),
        external_delegate_token_master::accounts::Initialize {
            user_account: user_account.pubkey(),
            authority: authority.pubkey(),
            system_program: system_program::ID,
        }
        .to_account_metas(None),
    );
    send(&mut svm, &[initialize], &[&authority, &user_account]).unwrap();

    let user_pda =
        Pubkey::find_program_address(&[user_account.pubkey().as_ref()], &external_delegate_token_master::id()).0;
    let mint = Keypair::new();
    let recipient = Pubkey::new_unique();
    let user_token_account = get_associated_token_address(&user_pda, &mint.pubkey());
    let recipient_token_account = get_associated_token_address(&recipient, &mint.pubkey());
    let space = spl_token::state::Mint::LEN;
    let token_setup = [
        system_instruction::create_account(
            &authority.pubkey(),
            &mint.pubkey(),
            svm.minimum_balance_for_rent_exemption(space),
            space as u64,
            &TOKEN_PROGRAM_ID,
        ),
        spl_token::instruction::initialize_mint2(&TOKEN_PROGRAM_ID, &mint.pubkey(), &authority.pubkey(), None, 6)
            .unwrap(),
        create_associated_token_account(&authority.pubkey(), &user_pda, &mint.pubkey(), &TOKEN_PROGRAM_ID),
        create_associated_token_account(&authority.pubkey(), &recipient, &mint.pubkey(), &TOKEN_PROGRAM_ID),
        spl_token::instruction::mint_to(
            &TOKEN_PROGRAM_ID,
            &mint.pubkey(),
            &user_token_account,
            &authority.pubkey(),
            &[],
            INITIAL_BALANCE,
        )
        .unwrap(),
    ];
    send(&mut svm, &token_setup, &[&authority, &mint]).unwrap();

    Env { svm, authority, user_account: user_account.pubkey(), user_pda, user_token_account, recipient_token_account }
}

#[test]
fn initialize_sets_authority_and_clears_state() {
    let env = setup();
    let state = user_account_state(&env);
    assert_eq!(state.authority, env.authority.pubkey());
    assert_eq!(state.ethereum_address, [0; 20]);
    assert_eq!(state.nonce, 0);
}

#[test]
fn set_ethereum_address_stores_address() {
    let mut env = setup();
    let address: [u8; 20] = [
        0x1c, 0x8c, 0xd0, 0xc3, 0x8f, 0x8d, 0xe3, 0x5d, 0x60, 0x56, 0xc7, 0xc7, 0xab, 0xfa, 0x7e, 0x65, 0xd2, 0x60,
        0xe8, 0x16,
    ];
    set_ethereum_address(&mut env, address);
    assert_eq!(user_account_state(&env).ethereum_address, address);
}

#[test]
fn authority_transfer_moves_tokens() {
    let mut env = setup();
    let ix = Instruction::new_with_bytes(
        external_delegate_token_master::id(),
        &external_delegate_token_master::instruction::AuthorityTransfer { amount: 250 }.data(),
        external_delegate_token_master::accounts::AuthorityTransfer {
            user_account: env.user_account,
            authority: env.authority.pubkey(),
            user_token_account: env.user_token_account,
            recipient_token_account: env.recipient_token_account,
            user_pda: env.user_pda,
            token_program: TOKEN_PROGRAM_ID,
        }
        .to_account_metas(None),
    );
    send(&mut env.svm, &[ix], &[&env.authority]).unwrap();
    assert_eq!(token_balance(&env.svm, &env.recipient_token_account), 250);
    assert_eq!(token_balance(&env.svm, &env.user_token_account), INITIAL_BALANCE - 250);
}

#[test]
fn transfer_tokens_with_valid_signature_then_rejects_replay() {
    let mut env = setup();
    let key = eth_key(1);
    set_ethereum_address(&mut env, eth_address(&key));

    let signature = sign(&transfer_digest(&env, 300, 0), &key);
    transfer_tokens(&mut env, 300, signature).unwrap();
    assert_eq!(token_balance(&env.svm, &env.recipient_token_account), 300);
    assert_eq!(token_balance(&env.svm, &env.user_token_account), INITIAL_BALANCE - 300);
    assert_eq!(user_account_state(&env).nonce, 1);

    assert_invalid_signature(transfer_tokens(&mut env, 300, signature));
    assert_eq!(user_account_state(&env).nonce, 1);
}

#[test]
fn rejects_signature_for_a_different_amount() {
    let mut env = setup();
    let key = eth_key(1);
    set_ethereum_address(&mut env, eth_address(&key));

    let signature = sign(&transfer_digest(&env, 100, 0), &key);
    assert_invalid_signature(transfer_tokens(&mut env, 999, signature));
}

#[test]
fn rejects_signature_from_unregistered_key() {
    let mut env = setup();
    set_ethereum_address(&mut env, eth_address(&eth_key(1)));

    let signature = sign(&transfer_digest(&env, 100, 0), &eth_key(2));
    assert_invalid_signature(transfer_tokens(&mut env, 100, signature));
}
