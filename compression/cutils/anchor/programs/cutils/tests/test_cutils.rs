use {
    anchor_lang::{
        solana_program::{
            instruction::{AccountMeta, Instruction},
            pubkey::Pubkey,
            system_instruction, system_program,
        },
        InstructionData, ToAccountMetas,
    },
    borsh::BorshSerialize,
    cutils::bubblegum_types::{
        get_asset_id, leaf_schema_v1_hash, Collection, Creator, MetadataArgs, TokenProgramVersion, TokenStandard,
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
const TOKEN_METADATA: Pubkey = Pubkey::from_str_const("metaqbxxUerdq28cj1RbAWkYQm3ybzjb6a8bt518x1s");
const TOKEN: Pubkey = Pubkey::from_str_const("TokenkegQfeZyiNwAJbNbGKPFXCWuBvf9Ss623VQ5DA");

const MAX_DEPTH: usize = 5;
const MAX_BUFFER_SIZE: usize = 8;
const CREATE_TREE: [u8; 8] = [165, 83, 136, 142, 89, 202, 47, 220];
const CREATE_METADATA_ACCOUNT_V3: u8 = 33;
const CREATE_MASTER_EDITION_V3: u8 = 17;
const URI: &str = "https://arweave.net/nVRvZDaOk5YAdr4ZBEeMjOVhynuv8P3vywvuN5sYSPo";

#[derive(BorshSerialize)]
struct CreateMetadataAccountArgsV3 {
    name: String,
    symbol: String,
    uri: String,
    seller_fee_basis_points: u16,
    creators: Option<Vec<Creator>>,
    collection: Option<Collection>,
    uses: Option<u8>,
    is_mutable: bool,
    collection_details: Option<u8>,
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
    let bytes = include_bytes!(concat!(env!("CARGO_MANIFEST_DIR"), "/../../target/deploy/cutils.so"));
    svm.add_program(cutils::id(), bytes).unwrap();
    svm.add_program_from_file(BUBBLEGUM, fixture("bubblegum")).unwrap();
    svm.add_program_from_file(COMPRESSION, fixture("spl_account_compression")).unwrap();
    svm.add_program_from_file(NOOP, fixture("spl_noop")).unwrap();
    svm.add_program_from_file(TOKEN_METADATA, fixture("token_metadata")).unwrap();
    let payer = Keypair::new();
    svm.airdrop(&payer.pubkey(), 10_000_000_000).unwrap();
    (svm, payer)
}

fn send(svm: &mut LiteSVM, ixs: &[Instruction], payer: &Keypair, signers: &[&Keypair]) -> Result<(), Vec<String>> {
    let msg = Message::new_with_blockhash(ixs, Some(&payer.pubkey()), &svm.latest_blockhash());
    let tx = VersionedTransaction::try_new(VersionedMessage::Legacy(msg), signers).unwrap();
    let result = svm.send_transaction(tx).map(|_| ()).map_err(|failed| failed.meta.logs);
    svm.expire_blockhash();
    result
}

fn tree_config(tree: &Pubkey) -> Pubkey {
    Pubkey::find_program_address(&[tree.as_ref()], &BUBBLEGUM).0
}

fn create_account(svm: &LiteSVM, payer: &Keypair, account: &Pubkey, size: usize, owner: &Pubkey) -> Instruction {
    let lamports = svm.minimum_balance_for_rent_exemption(size);
    system_instruction::create_account(&payer.pubkey(), account, lamports, size as u64, owner)
}

fn create_tree(svm: &mut LiteSVM, payer: &Keypair) -> Pubkey {
    let tree = Keypair::new();
    let size = 56 + 24 + (MAX_BUFFER_SIZE + 1) * (32 * MAX_DEPTH + 40);
    let allocate = create_account(svm, payer, &tree.pubkey(), size, &COMPRESSION);
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
    send(svm, &[allocate, create], payer, &[payer, &tree]).unwrap();
    tree.pubkey()
}

fn metadata_pda(mint: &Pubkey) -> Pubkey {
    Pubkey::find_program_address(&[b"metadata", TOKEN_METADATA.as_ref(), mint.as_ref()], &TOKEN_METADATA).0
}

fn edition_pda(mint: &Pubkey) -> Pubkey {
    Pubkey::find_program_address(&[b"metadata", TOKEN_METADATA.as_ref(), mint.as_ref(), b"edition"], &TOKEN_METADATA).0
}

fn create_collection(svm: &mut LiteSVM, payer: &Keypair) -> Pubkey {
    let mint = Keypair::new();
    let token_account = Keypair::new();
    let authority = payer.pubkey();

    let mut initialize_mint = vec![20, 0];
    initialize_mint.extend(authority.as_ref());
    initialize_mint.extend([1]);
    initialize_mint.extend(authority.as_ref());
    let mut initialize_account = vec![18];
    initialize_account.extend(authority.as_ref());
    let mut mint_to = vec![7];
    mint_to.extend(1u64.to_le_bytes());
    let token_ixs = [
        create_account(svm, payer, &mint.pubkey(), 82, &TOKEN),
        Instruction::new_with_bytes(TOKEN, &initialize_mint, vec![AccountMeta::new(mint.pubkey(), false)]),
        create_account(svm, payer, &token_account.pubkey(), 165, &TOKEN),
        Instruction::new_with_bytes(
            TOKEN,
            &initialize_account,
            vec![AccountMeta::new(token_account.pubkey(), false), AccountMeta::new_readonly(mint.pubkey(), false)],
        ),
        Instruction::new_with_bytes(
            TOKEN,
            &mint_to,
            vec![
                AccountMeta::new(mint.pubkey(), false),
                AccountMeta::new(token_account.pubkey(), false),
                AccountMeta::new_readonly(authority, true),
            ],
        ),
    ];
    send(svm, &token_ixs, payer, &[payer, &mint, &token_account]).unwrap();

    let mut metadata = vec![CREATE_METADATA_ACCOUNT_V3];
    CreateMetadataAccountArgsV3 {
        name: "Super Sweet NFT Collection".to_string(),
        symbol: "SSNC".to_string(),
        uri: "https://supersweetcollection.notarealurl/collection.json".to_string(),
        seller_fee_basis_points: 100,
        creators: Some(vec![Creator { address: authority, verified: false, share: 100 }]),
        collection: None,
        uses: None,
        is_mutable: false,
        collection_details: None,
    }
    .serialize(&mut metadata)
    .unwrap();
    let mut master_edition = vec![CREATE_MASTER_EDITION_V3, 1];
    master_edition.extend(0u64.to_le_bytes());
    let metadata_ixs = [
        Instruction::new_with_bytes(
            TOKEN_METADATA,
            &metadata,
            vec![
                AccountMeta::new(metadata_pda(&mint.pubkey()), false),
                AccountMeta::new_readonly(mint.pubkey(), false),
                AccountMeta::new_readonly(authority, true),
                AccountMeta::new(authority, true),
                AccountMeta::new_readonly(authority, true),
                AccountMeta::new_readonly(system_program::ID, false),
            ],
        ),
        Instruction::new_with_bytes(
            TOKEN_METADATA,
            &master_edition,
            vec![
                AccountMeta::new(edition_pda(&mint.pubkey()), false),
                AccountMeta::new(mint.pubkey(), false),
                AccountMeta::new_readonly(authority, true),
                AccountMeta::new_readonly(authority, true),
                AccountMeta::new(authority, true),
                AccountMeta::new(metadata_pda(&mint.pubkey()), false),
                AccountMeta::new_readonly(TOKEN, false),
                AccountMeta::new_readonly(system_program::ID, false),
            ],
        ),
    ];
    send(svm, &metadata_ixs, payer, &[payer]).unwrap();
    mint.pubkey()
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

fn minted_hashes(collection_authority: &Pubkey, collection_mint: &Pubkey) -> ([u8; 32], [u8; 32]) {
    let metadata = MetadataArgs {
        name: "BURGER".to_string(),
        symbol: "BURG".to_string(),
        uri: URI.to_string(),
        seller_fee_basis_points: 0,
        primary_sale_happened: false,
        is_mutable: false,
        edition_nonce: Some(0),
        token_standard: Some(TokenStandard::NonFungible),
        collection: Some(Collection { verified: true, key: *collection_mint }),
        uses: None,
        token_program_version: TokenProgramVersion::Original,
        creators: vec![Creator { address: *collection_authority, verified: false, share: 100 }],
    };
    let data_hash =
        keccak(&[&keccak(&[&borsh::to_vec(&metadata).unwrap()]), &metadata.seller_fee_basis_points.to_le_bytes()]);
    let creators: Vec<u8> =
        metadata.creators.iter().flat_map(|c| [c.address.as_ref(), &[c.verified as u8, c.share]].concat()).collect();
    (data_hash, keccak(&[&creators]))
}

fn verify(owner: &Pubkey, tree: &Pubkey, params: cutils::VerifyParams, proof: &[[u8; 32]]) -> Instruction {
    let mut accounts = cutils::accounts::Verify {
        leaf_owner: *owner,
        leaf_delegate: *owner,
        merkle_tree: *tree,
        compression_program: COMPRESSION,
    }
    .to_account_metas(None);
    accounts.extend(proof.iter().map(|node| AccountMeta::new_readonly(Pubkey::new_from_array(*node), false)));
    Instruction::new_with_bytes(cutils::id(), &cutils::instruction::Verify { params }.data(), accounts)
}

#[test]
fn mint_and_verify() {
    let (mut svm, payer) = setup();
    let tree = create_tree(&mut svm, &payer);
    let collection_mint = create_collection(&mut svm, &payer);
    let test_wallet = Keypair::new();

    let mint = Instruction::new_with_bytes(
        cutils::id(),
        &cutils::instruction::Mint { params: cutils::MintParams { uri: URI.to_string() } }.data(),
        cutils::accounts::Mint {
            payer: payer.pubkey(),
            tree_authority: tree_config(&tree),
            leaf_owner: test_wallet.pubkey(),
            leaf_delegate: test_wallet.pubkey(),
            merkle_tree: tree,
            tree_delegate: payer.pubkey(),
            collection_authority: payer.pubkey(),
            collection_authority_record_pda: BUBBLEGUM,
            collection_mint,
            collection_metadata: metadata_pda(&collection_mint),
            edition_account: edition_pda(&collection_mint),
            bubblegum_signer: Pubkey::find_program_address(&[b"collection_cpi"], &BUBBLEGUM).0,
            log_wrapper: NOOP,
            compression_program: COMPRESSION,
            token_metadata_program: TOKEN_METADATA,
            bubblegum_program: BUBBLEGUM,
            system_program: system_program::ID,
        }
        .to_account_metas(None),
    );
    send(&mut svm, &[mint], &payer, &[&payer]).unwrap();

    let (data_hash, creator_hash) = minted_hashes(&payer.pubkey(), &collection_mint);
    let leaf = leaf_schema_v1_hash(
        &get_asset_id(&tree, 0),
        &test_wallet.pubkey(),
        &test_wallet.pubkey(),
        0,
        &data_hash,
        &creator_hash,
    );
    let (root, proof) = root_and_proof(&[leaf], 0);
    assert_eq!(onchain_root(&svm, &tree), root);

    let params = || cutils::VerifyParams { root, data_hash, creator_hash, nonce: 0, index: 0 };
    let wrong_owner = verify(&payer.pubkey(), &tree, params(), &proof);
    let logs = send(&mut svm, &[wrong_owner], &payer, &[&payer]).unwrap_err();
    assert!(logs.iter().any(|log| log.contains("Invalid root recomputed from proof")));

    let owner = verify(&test_wallet.pubkey(), &tree, params(), &proof);
    send(&mut svm, &[owner], &payer, &[&payer, &test_wallet]).unwrap();
}
