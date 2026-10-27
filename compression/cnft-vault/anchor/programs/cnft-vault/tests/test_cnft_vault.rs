use {
    anchor_lang::{
        prelude::AnchorSerialize,
        solana_program::{
            instruction::{AccountMeta, Instruction},
            pubkey::Pubkey,
            rent::Rent,
            system_instruction, system_program,
        },
        InstructionData, ToAccountMetas,
    },
    litesvm::LiteSVM,
    sha3::{Digest, Keccak256},
    solana_keypair::Keypair,
    solana_message::{Message, VersionedMessage},
    solana_signer::Signer,
    solana_transaction::versioned::VersionedTransaction,
};

const BUBBLEGUM: Pubkey = Pubkey::from_str_const("BGUMAp9Gq7iTEuizy4pqaxsTyUCBK68MDfK752saRPUY");
const COMPRESSION: Pubkey = Pubkey::from_str_const("cmtDvXumGCrqC1Age74AVPhSRVXJMd8PJS91L8KbNCK");
const NOOP: Pubkey = Pubkey::from_str_const("noopb9bkMVfRPU8AsbpTUg8AQkHtKwMYZiFUjNRtMmV");

const MAX_DEPTH: usize = 5;
const MAX_BUFFER_SIZE: usize = 8;
const CREATE_TREE: [u8; 8] = [165, 83, 136, 142, 89, 202, 47, 220];
const MINT_V1: [u8; 8] = [145, 98, 192, 118, 184, 147, 118, 104];

#[derive(AnchorSerialize)]
struct Creator {
    address: Pubkey,
    verified: bool,
    share: u8,
}

#[derive(AnchorSerialize)]
struct MetadataArgs {
    name: String,
    symbol: String,
    uri: String,
    seller_fee_basis_points: u16,
    primary_sale_happened: bool,
    is_mutable: bool,
    edition_nonce: Option<u8>,
    token_standard: Option<u8>,
    collection: Option<u8>,
    uses: Option<u8>,
    token_program_version: u8,
    creators: Vec<Creator>,
}

fn keccak(parts: &[&[u8]]) -> [u8; 32] {
    let mut hasher = Keccak256::new();
    for part in parts {
        hasher.update(part);
    }
    hasher.finalize().into()
}

fn fixture(name: &str) -> String {
    format!("{}/../../tests/fixtures/{name}.so", env!("CARGO_MANIFEST_DIR"))
}

fn setup() -> (LiteSVM, Keypair) {
    let mut svm = LiteSVM::new();
    let bytes = include_bytes!(concat!(env!("CARGO_MANIFEST_DIR"), "/../../target/deploy/cnft_vault.so"));
    svm.add_program(cnft_vault::id(), bytes).unwrap();
    svm.add_program_from_file(BUBBLEGUM, fixture("bubblegum")).unwrap();
    svm.add_program_from_file(COMPRESSION, fixture("spl_account_compression")).unwrap();
    svm.add_program_from_file(NOOP, fixture("spl_noop")).unwrap();
    let payer = Keypair::new();
    svm.airdrop(&payer.pubkey(), 10_000_000_000).unwrap();
    (svm, payer)
}

fn send(svm: &mut LiteSVM, ixs: &[Instruction], payer: &Keypair, signers: &[&Keypair]) {
    let msg = Message::new_with_blockhash(ixs, Some(&payer.pubkey()), &svm.latest_blockhash());
    let tx = VersionedTransaction::try_new(VersionedMessage::Legacy(msg), signers).unwrap();
    svm.send_transaction(tx).unwrap();
    svm.expire_blockhash();
}

fn tree_config(tree: &Pubkey) -> Pubkey {
    Pubkey::find_program_address(&[tree.as_ref()], &BUBBLEGUM).0
}

fn create_tree(svm: &mut LiteSVM, payer: &Keypair) -> Pubkey {
    let tree = Keypair::new();
    let size = 56 + 24 + (MAX_BUFFER_SIZE + 1) * (32 * MAX_DEPTH + 40);
    let allocate = system_instruction::create_account(
        &payer.pubkey(),
        &tree.pubkey(),
        Rent::default().minimum_balance(size),
        size as u64,
        &COMPRESSION,
    );
    let mut data = CREATE_TREE.to_vec();
    data.extend((MAX_DEPTH as u32).to_le_bytes());
    data.extend((MAX_BUFFER_SIZE as u32).to_le_bytes());
    data.push(0);
    let create = Instruction::new_with_bytes(
        BUBBLEGUM,
        &data,
        vec![
            AccountMeta::new(tree_config(&tree.pubkey()), false),
            AccountMeta::new(tree.pubkey(), false),
            AccountMeta::new(payer.pubkey(), true),
            AccountMeta::new_readonly(payer.pubkey(), true),
            AccountMeta::new_readonly(NOOP, false),
            AccountMeta::new_readonly(COMPRESSION, false),
            AccountMeta::new_readonly(system_program::ID, false),
        ],
    );
    send(svm, &[allocate, create], payer, &[payer, &tree]);
    tree.pubkey()
}

struct Leaf {
    data_hash: [u8; 32],
    creator_hash: [u8; 32],
    nonce: u64,
}

impl Leaf {
    fn hash(&self, tree: &Pubkey, owner: &Pubkey, delegate: &Pubkey) -> [u8; 32] {
        let nonce = self.nonce.to_le_bytes();
        let asset_id = Pubkey::find_program_address(&[b"asset", tree.as_ref(), &nonce], &BUBBLEGUM).0;
        keccak(&[
            &[1],
            asset_id.as_ref(),
            owner.as_ref(),
            delegate.as_ref(),
            &nonce,
            &self.data_hash,
            &self.creator_hash,
        ])
    }
}

fn mint(svm: &mut LiteSVM, payer: &Keypair, tree: &Pubkey, owner: &Pubkey, nonce: u64) -> Leaf {
    let metadata = MetadataArgs {
        name: "Test Vault".to_string(),
        symbol: "TV".to_string(),
        uri: "https://supersweetcollection.notarealurl/nft.json".to_string(),
        seller_fee_basis_points: 100,
        primary_sale_happened: false,
        is_mutable: false,
        edition_nonce: Some(0),
        token_standard: Some(0),
        collection: None,
        uses: None,
        token_program_version: 0,
        creators: vec![Creator { address: payer.pubkey(), verified: false, share: 100 }],
    };
    let mut data = MINT_V1.to_vec();
    metadata.serialize(&mut data).unwrap();
    let ix = Instruction::new_with_bytes(
        BUBBLEGUM,
        &data,
        vec![
            AccountMeta::new(tree_config(tree), false),
            AccountMeta::new_readonly(*owner, false),
            AccountMeta::new_readonly(*owner, false),
            AccountMeta::new(*tree, false),
            AccountMeta::new_readonly(payer.pubkey(), true),
            AccountMeta::new_readonly(payer.pubkey(), true),
            AccountMeta::new_readonly(NOOP, false),
            AccountMeta::new_readonly(COMPRESSION, false),
            AccountMeta::new_readonly(system_program::ID, false),
        ],
    );
    send(svm, &[ix], payer, &[payer]);

    let creators: Vec<u8> =
        metadata.creators.iter().flat_map(|c| [c.address.as_ref(), &[c.verified as u8, c.share]].concat()).collect();
    Leaf {
        data_hash: keccak(&[
            &keccak(&[&borsh::to_vec(&metadata).unwrap()]),
            &metadata.seller_fee_basis_points.to_le_bytes(),
        ]),
        creator_hash: keccak(&[&creators]),
        nonce,
    }
}

fn root_and_proof(leaves: &[[u8; 32]], index: usize) -> ([u8; 32], Vec<[u8; 32]>) {
    let mut level = leaves.to_vec();
    level.resize(1 << MAX_DEPTH, [0; 32]);
    let mut proof = Vec::new();
    let mut i = index;
    while level.len() > 1 {
        proof.push(level[i ^ 1]);
        level = level.chunks(2).map(|pair| keccak(&[&pair[0], &pair[1]])).collect();
        i /= 2;
    }
    (level[0], proof)
}

fn onchain_root(svm: &LiteSVM, tree: &Pubkey) -> [u8; 32] {
    let data = svm.get_account(tree).unwrap().data;
    // 56-byte header, then u64 sequence_number, u64 active_index, u64 buffer_size, then the change logs.
    let active_index = u64::from_le_bytes(data[64..72].try_into().unwrap()) as usize;
    let start = 80 + active_index * (32 * MAX_DEPTH + 40);
    data[start..start + 32].try_into().unwrap()
}

fn vault() -> Pubkey {
    Pubkey::find_program_address(&[b"cNFT-vault"], &cnft_vault::id()).0
}

fn proof_metas(proof: &[[u8; 32]]) -> Vec<AccountMeta> {
    proof.iter().map(|node| AccountMeta::new_readonly(Pubkey::new_from_array(*node), false)).collect()
}

#[test]
fn withdraw_cnft() {
    let (mut svm, payer) = setup();
    let tree = create_tree(&mut svm, &payer);
    let leaf = mint(&mut svm, &payer, &tree, &vault(), 0);
    let (root, proof) = root_and_proof(&[leaf.hash(&tree, &vault(), &vault())], 0);
    assert_eq!(onchain_root(&svm, &tree), root);

    let receiver = Keypair::new().pubkey();
    let mut accounts = cnft_vault::accounts::Withdraw {
        tree_authority: tree_config(&tree),
        leaf_owner: vault(),
        new_leaf_owner: receiver,
        merkle_tree: tree,
        log_wrapper: NOOP,
        compression_program: COMPRESSION,
        bubblegum_program: BUBBLEGUM,
        system_program: system_program::ID,
    }
    .to_account_metas(None);
    accounts.extend(proof_metas(&proof));
    let withdraw = Instruction::new_with_bytes(
        cnft_vault::id(),
        &cnft_vault::instruction::WithdrawCnft {
            root,
            data_hash: leaf.data_hash,
            creator_hash: leaf.creator_hash,
            nonce: leaf.nonce,
            index: 0,
        }
        .data(),
        accounts,
    );
    send(&mut svm, &[withdraw], &payer, &[&payer]);

    let transferred = leaf.hash(&tree, &receiver, &receiver);
    assert_eq!(onchain_root(&svm, &tree), root_and_proof(&[transferred], 0).0);
}

#[test]
fn withdraw_two_cnfts() {
    let (mut svm, payer) = setup();
    let tree1 = create_tree(&mut svm, &payer);
    let tree2 = create_tree(&mut svm, &payer);
    let leaf1 = mint(&mut svm, &payer, &tree1, &vault(), 0);
    let leaf2 = mint(&mut svm, &payer, &tree2, &vault(), 0);
    let (root1, proof1) = root_and_proof(&[leaf1.hash(&tree1, &vault(), &vault())], 0);
    let (root2, proof2) = root_and_proof(&[leaf2.hash(&tree2, &vault(), &vault())], 0);

    let receiver1 = Keypair::new().pubkey();
    let receiver2 = Keypair::new().pubkey();
    let mut accounts = cnft_vault::accounts::WithdrawTwo {
        tree_authority1: tree_config(&tree1),
        leaf_owner: vault(),
        new_leaf_owner1: receiver1,
        merkle_tree1: tree1,
        tree_authority2: tree_config(&tree2),
        new_leaf_owner2: receiver2,
        merkle_tree2: tree2,
        log_wrapper: NOOP,
        compression_program: COMPRESSION,
        bubblegum_program: BUBBLEGUM,
        system_program: system_program::ID,
    }
    .to_account_metas(None);
    accounts.extend(proof_metas(&proof1));
    accounts.extend(proof_metas(&proof2));
    let withdraw = Instruction::new_with_bytes(
        cnft_vault::id(),
        &cnft_vault::instruction::WithdrawTwoCnfts {
            root1,
            data_hash1: leaf1.data_hash,
            creator_hash1: leaf1.creator_hash,
            nonce1: leaf1.nonce,
            index1: 0,
            proof_1_length: proof1.len() as u8,
            root2,
            data_hash2: leaf2.data_hash,
            creator_hash2: leaf2.creator_hash,
            nonce2: leaf2.nonce,
            index2: 0,
            _proof_2_length: proof2.len() as u8,
        }
        .data(),
        accounts,
    );
    send(&mut svm, &[withdraw], &payer, &[&payer]);

    let transferred1 = leaf1.hash(&tree1, &receiver1, &receiver1);
    let transferred2 = leaf2.hash(&tree2, &receiver2, &receiver2);
    assert_eq!(onchain_root(&svm, &tree1), root_and_proof(&[transferred1], 0).0);
    assert_eq!(onchain_root(&svm, &tree2), root_and_proof(&[transferred2], 0).0);
}
