use {
    anchor_lang::{
        prelude::Rent,
        solana_program::{instruction::Instruction, pubkey::Pubkey, system_program, sysvar::SysvarId},
        AccountDeserialize, InstructionData, ToAccountMetas,
    },
    anchor_spl::{
        metadata::{MetadataAccount, ID as METADATA_PROGRAM_ID},
        token::{Mint, ID as TOKEN_PROGRAM_ID},
    },
    litesvm::LiteSVM,
    solana_keypair::Keypair,
    solana_message::{Message, VersionedMessage},
    solana_signer::Signer,
    solana_transaction::versioned::VersionedTransaction,
};

const NAME: &str = "Solana Gold";
const SYMBOL: &str = "GOLDSOL";
const URI: &str = "https://raw.githubusercontent.com/solana-developers/program-examples/new-examples/tokens/tokens/.assets/spl-token.json";

fn setup() -> (LiteSVM, Keypair) {
    let mut svm = LiteSVM::new();
    let bytes = include_bytes!(concat!(env!("CARGO_MANIFEST_DIR"), "/../../target/deploy/create_token.so"));
    svm.add_program(create_token::id(), bytes).unwrap();
    svm.add_program_from_file(
        METADATA_PROGRAM_ID,
        concat!(env!("CARGO_MANIFEST_DIR"), "/../../tests/fixtures/token_metadata.so"),
    )
    .unwrap();
    let payer = Keypair::new();
    svm.airdrop(&payer.pubkey(), 10_000_000_000).unwrap();
    (svm, payer)
}

fn create_token_mint(svm: &mut LiteSVM, payer: &Keypair, decimals: u8) {
    let mint = Keypair::new();
    let (metadata_account, _) = Pubkey::find_program_address(
        &[b"metadata", METADATA_PROGRAM_ID.as_ref(), mint.pubkey().as_ref()],
        &METADATA_PROGRAM_ID,
    );
    let ix = Instruction::new_with_bytes(
        create_token::id(),
        &create_token::instruction::CreateTokenMint {
            _token_decimals: decimals,
            token_name: NAME.to_string(),
            token_symbol: SYMBOL.to_string(),
            token_uri: URI.to_string(),
        }
        .data(),
        create_token::accounts::CreateTokenMint {
            payer: payer.pubkey(),
            metadata_account,
            mint_account: mint.pubkey(),
            token_metadata_program: METADATA_PROGRAM_ID,
            token_program: TOKEN_PROGRAM_ID,
            system_program: system_program::ID,
            rent: Rent::id(),
        }
        .to_account_metas(None),
    );
    let msg = Message::new_with_blockhash(&[ix], Some(&payer.pubkey()), &svm.latest_blockhash());
    let tx = VersionedTransaction::try_new(VersionedMessage::Legacy(msg), &[payer, &mint]).unwrap();
    svm.send_transaction(tx).unwrap();

    let mint_account = svm.get_account(&mint.pubkey()).unwrap();
    let mint_state = Mint::try_deserialize(&mut mint_account.data.as_slice()).unwrap();
    assert_eq!(mint_state.decimals, decimals);
    assert_eq!(mint_state.mint_authority, Some(payer.pubkey()).into());

    let metadata = svm.get_account(&metadata_account).unwrap();
    assert_eq!(metadata.owner, METADATA_PROGRAM_ID);
    let metadata = MetadataAccount::try_deserialize(&mut metadata.data.as_slice()).unwrap();
    assert_eq!(metadata.mint, mint.pubkey());
    assert_eq!(metadata.update_authority, payer.pubkey());
    assert_eq!(metadata.name.trim_end_matches('\0'), NAME);
    assert_eq!(metadata.symbol.trim_end_matches('\0'), SYMBOL);
    assert_eq!(metadata.uri.trim_end_matches('\0'), URI);
    assert!(!metadata.is_mutable);
}

#[test]
fn create_spl_token() {
    let (mut svm, payer) = setup();
    create_token_mint(&mut svm, &payer, 9);
}

#[test]
fn create_nft() {
    let (mut svm, payer) = setup();
    create_token_mint(&mut svm, &payer, 0);
}
