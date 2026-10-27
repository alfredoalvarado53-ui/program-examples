use {
    anchor_lang::{
        solana_program::{instruction::Instruction, program_option::COption, pubkey::Pubkey, system_program},
        AccountDeserialize, InstructionData, ToAccountMetas,
    },
    anchor_spl::{
        associated_token::{get_associated_token_address, ID as ASSOCIATED_TOKEN_PROGRAM_ID},
        token::{Mint, TokenAccount, ID as TOKEN_PROGRAM_ID},
    },
    litesvm::LiteSVM,
    merkle_tree_token_claimer::{AirdropState, ClaimReceipt},
    sha2::{Digest, Sha256},
    solana_keypair::Keypair,
    solana_message::{Message, VersionedMessage},
    solana_signer::Signer,
    solana_transaction::versioned::VersionedTransaction,
};

type SendResult = Result<(), Vec<String>>;

const CLAIM_AMOUNTS: [u64; 3] = [125, 275, 400];
const ZERO_HASH: [u8; 32] = [0; 32];

fn sha256(bytes: &[u8]) -> [u8; 32] {
    Sha256::digest(bytes).into()
}

fn hash_pair(left: &[u8; 32], right: &[u8; 32]) -> [u8; 32] {
    sha256(&[left.as_slice(), right.as_slice()].concat())
}

fn leaf_bytes(wallet: &Pubkey, amount: u64) -> Vec<u8> {
    [wallet.as_ref(), &amount.to_le_bytes()].concat()
}

struct MerkleTree {
    levels: Vec<Vec<[u8; 32]>>,
}

impl MerkleTree {
    fn new(leaves: &[Vec<u8>]) -> Self {
        let mut levels = vec![leaves.iter().map(|leaf| sha256(leaf)).collect::<Vec<_>>()];
        while levels.last().unwrap().len() > 1 {
            let next = levels
                .last()
                .unwrap()
                .chunks(2)
                .map(|pair| hash_pair(&pair[0], pair.get(1).unwrap_or(&ZERO_HASH)))
                .collect();
            levels.push(next);
        }
        Self { levels }
    }

    fn root(&self) -> [u8; 32] {
        self.levels.last().unwrap()[0]
    }

    fn depth(&self) -> usize {
        self.levels.len() - 1
    }

    fn proof(&self, mut index: usize) -> Vec<u8> {
        let mut siblings = Vec::new();
        for level in &self.levels[..self.depth()] {
            siblings.extend_from_slice(level.get(index ^ 1).unwrap_or(&ZERO_HASH));
            index /= 2;
        }
        siblings
    }
}

fn build_tree(claimants: &[(Keypair, u64)], order: &[usize]) -> MerkleTree {
    let leaves: Vec<_> = order.iter().map(|&i| leaf_bytes(&claimants[i].0.pubkey(), claimants[i].1)).collect();
    MerkleTree::new(&leaves)
}

struct Env {
    svm: LiteSVM,
    authority: Keypair,
    mint: Keypair,
    claimants: Vec<(Keypair, u64)>,
    order: Vec<usize>,
    tree: MerkleTree,
    airdrop_state: Pubkey,
    vault: Pubkey,
}

impl Env {
    fn new() -> Self {
        let mut svm = LiteSVM::new();
        let bytes =
            include_bytes!(concat!(env!("CARGO_MANIFEST_DIR"), "/../../target/deploy/merkle_tree_token_claimer.so"));
        svm.add_program(merkle_tree_token_claimer::id(), bytes).unwrap();
        let authority = Keypair::new();
        svm.airdrop(&authority.pubkey(), 10_000_000_000).unwrap();
        let claimants: Vec<(Keypair, u64)> = CLAIM_AMOUNTS
            .iter()
            .map(|&amount| {
                let wallet = Keypair::new();
                svm.airdrop(&wallet.pubkey(), 1_000_000_000).unwrap();
                (wallet, amount)
            })
            .collect();
        let mint = Keypair::new();
        let airdrop_state =
            Pubkey::find_program_address(&[b"merkle_tree", mint.pubkey().as_ref()], &merkle_tree_token_claimer::id()).0;
        let vault = get_associated_token_address(&airdrop_state, &mint.pubkey());
        let order: Vec<usize> = (0..claimants.len()).collect();
        let tree = build_tree(&claimants, &order);
        Self { svm, authority, mint, claimants, order, tree, airdrop_state, vault }
    }

    fn total(&self) -> u64 {
        self.claimants.iter().map(|(_, amount)| amount).sum()
    }

    fn reverse_tree(&mut self) -> [u8; 32] {
        self.order.reverse();
        self.tree = build_tree(&self.claimants, &self.order);
        self.tree.root()
    }

    fn proof_for(&self, claimant: usize) -> (usize, Vec<u8>) {
        let index = self.order.iter().position(|&i| i == claimant).unwrap();
        (index, self.tree.proof(index))
    }

    fn send(&mut self, ix: Instruction, signers: &[&Keypair]) -> SendResult {
        let msg = Message::new_with_blockhash(&[ix], Some(&signers[0].pubkey()), &self.svm.latest_blockhash());
        let tx = VersionedTransaction::try_new(VersionedMessage::Legacy(msg), signers).unwrap();
        let result = self.svm.send_transaction(tx).map(|_| ()).map_err(|err| err.meta.logs);
        self.svm.expire_blockhash();
        result
    }

    fn initialize(&mut self) {
        let ix = Instruction::new_with_bytes(
            merkle_tree_token_claimer::id(),
            &merkle_tree_token_claimer::instruction::InitializeAirdropData {
                merkle_root: self.tree.root(),
                amount: self.total(),
            }
            .data(),
            merkle_tree_token_claimer::accounts::Initialize {
                airdrop_state: self.airdrop_state,
                mint: self.mint.pubkey(),
                vault: self.vault,
                authority: self.authority.pubkey(),
                system_program: system_program::ID,
                token_program: TOKEN_PROGRAM_ID,
                associated_token_program: ASSOCIATED_TOKEN_PROGRAM_ID,
            }
            .to_account_metas(None),
        );
        let authority = self.authority.insecure_clone();
        let mint = self.mint.insecure_clone();
        self.send(ix, &[&authority, &mint]).unwrap();
    }

    fn update_tree(&mut self, new_root: [u8; 32]) -> SendResult {
        let ix = Instruction::new_with_bytes(
            merkle_tree_token_claimer::id(),
            &merkle_tree_token_claimer::instruction::UpdateTree { new_root }.data(),
            merkle_tree_token_claimer::accounts::Update {
                airdrop_state: self.airdrop_state,
                mint: self.mint.pubkey(),
                authority: self.authority.pubkey(),
            }
            .to_account_metas(None),
        );
        let authority = self.authority.insecure_clone();
        self.send(ix, &[&authority])
    }

    fn receipt_address(&self, index: usize) -> Pubkey {
        Pubkey::find_program_address(
            &[b"claim_receipt", self.airdrop_state.as_ref(), &(index as u64).to_le_bytes()],
            &merkle_tree_token_claimer::id(),
        )
        .0
    }

    fn claim_as(&mut self, wallet: &Keypair, amount: u64, hashes: Vec<u8>, index: usize) -> SendResult {
        let ix = Instruction::new_with_bytes(
            merkle_tree_token_claimer::id(),
            &merkle_tree_token_claimer::instruction::ClaimAirdrop { amount, hashes, index: index as u64 }.data(),
            merkle_tree_token_claimer::accounts::Claim {
                airdrop_state: self.airdrop_state,
                mint: self.mint.pubkey(),
                vault: self.vault,
                claim_receipt: self.receipt_address(index),
                signer_ata: get_associated_token_address(&wallet.pubkey(), &self.mint.pubkey()),
                signer: wallet.pubkey(),
                system_program: system_program::ID,
                token_program: TOKEN_PROGRAM_ID,
                associated_token_program: ASSOCIATED_TOKEN_PROGRAM_ID,
            }
            .to_account_metas(None),
        );
        self.send(ix, &[wallet])
    }

    fn claim(&mut self, claimant: usize) -> usize {
        let (index, proof) = self.proof_for(claimant);
        let (wallet, amount) = (self.claimants[claimant].0.insecure_clone(), self.claimants[claimant].1);
        self.claim_as(&wallet, amount, proof, index).unwrap();
        index
    }

    fn read<T: AccountDeserialize>(&self, address: &Pubkey) -> T {
        let account = self.svm.get_account(address).unwrap();
        T::try_deserialize(&mut account.data.as_slice()).unwrap()
    }

    fn state(&self) -> AirdropState {
        self.read(&self.airdrop_state)
    }

    fn balance(&self, token_account: &Pubkey) -> u64 {
        self.read::<TokenAccount>(token_account).amount
    }

    fn claimant_balance(&self, claimant: usize) -> u64 {
        self.balance(&get_associated_token_address(&self.claimants[claimant].0.pubkey(), &self.mint.pubkey()))
    }
}

fn assert_fails_with(result: SendResult, error: &str) {
    let logs = result.unwrap_err();
    let needle = format!("Error Code: {error}");
    assert!(logs.iter().any(|log| log.contains(&needle)), "expected {error}, got {:?}", logs);
}

#[test]
fn initializes_airdrop_locks_mint_and_allows_root_updates_before_claims() {
    let mut env = Env::new();
    env.initialize();

    let state = env.state();
    assert_eq!(state.merkle_root, env.tree.root());
    assert_eq!(state.airdrop_amount, env.total());
    assert_eq!(state.amount_claimed, 0);
    assert_eq!(state.authority, env.authority.pubkey());

    let mint: Mint = env.read(&env.mint.pubkey());
    assert_eq!(mint.mint_authority, COption::None);
    assert_eq!(mint.supply, env.total());
    assert_eq!(env.balance(&env.vault), env.total());

    let updated_root = env.reverse_tree();
    env.update_tree(updated_root).unwrap();
    let state = env.state();
    assert_eq!(state.merkle_root, updated_root);
    assert_eq!(state.amount_claimed, 0);
}

#[test]
fn pays_out_claims_against_updated_root_and_records_receipts() {
    let mut env = Env::new();
    env.initialize();
    let root = env.reverse_tree();
    env.update_tree(root).unwrap();

    let first_index = env.claim(0);
    let receipt: ClaimReceipt = env.read(&env.receipt_address(first_index));
    assert_eq!(env.claimant_balance(0), CLAIM_AMOUNTS[0]);
    assert_eq!(receipt.claimer, env.claimants[0].0.pubkey());
    assert_eq!(receipt.amount, CLAIM_AMOUNTS[0]);
    assert_eq!(env.state().amount_claimed, CLAIM_AMOUNTS[0]);

    let second_index = env.claim(1);
    let receipt: ClaimReceipt = env.read(&env.receipt_address(second_index));
    assert_eq!(env.claimant_balance(1), CLAIM_AMOUNTS[1]);
    assert_eq!(env.state().amount_claimed, CLAIM_AMOUNTS[0] + CLAIM_AMOUNTS[1]);
    assert_eq!(env.balance(&env.vault), env.total() - CLAIM_AMOUNTS[0] - CLAIM_AMOUNTS[1]);
    assert_eq!(receipt.index, second_index as u64);
    assert_eq!(receipt.amount, CLAIM_AMOUNTS[1]);
}

#[test]
fn rejects_duplicate_claims_and_proofs_from_the_wrong_signer() {
    let mut env = Env::new();
    env.initialize();

    let claimed_index = env.claim(0);
    let (index, proof) = env.proof_for(0);
    assert_eq!(index, claimed_index);
    let wallet = env.claimants[0].0.insecure_clone();
    assert_fails_with(env.claim_as(&wallet, CLAIM_AMOUNTS[0], proof, index), "AlreadyClaimed");

    let attacker = Keypair::new();
    env.svm.airdrop(&attacker.pubkey(), 1_000_000_000).unwrap();
    let (index, proof) = env.proof_for(2);
    assert_fails_with(env.claim_as(&attacker, CLAIM_AMOUNTS[2], proof, index), "InvalidProof");
}

#[test]
fn rejects_a_valid_proof_replayed_under_a_different_receipt_index() {
    let mut env = Env::new();
    env.initialize();

    let last = env.claimants.len() - 1;
    let proof = env.tree.proof(last);
    assert_eq!(proof[..32], ZERO_HASH);

    let claimant = env.order[last];
    let (wallet, amount) = (env.claimants[claimant].0.insecure_clone(), env.claimants[claimant].1);
    env.claim_as(&wallet, amount, proof.clone(), last).unwrap();

    assert_fails_with(env.claim_as(&wallet, amount, proof.clone(), last + 1), "InvalidProof");
    let beyond_depth = last + (1 << env.tree.depth());
    assert_fails_with(env.claim_as(&wallet, amount, proof, beyond_depth), "InvalidProof");
}

#[test]
fn rejects_root_updates_after_claims_begin() {
    let mut env = Env::new();
    env.initialize();
    env.claim(0);

    let root = env.tree.root();
    assert_fails_with(env.update_tree(root), "ClaimsStarted");
}
