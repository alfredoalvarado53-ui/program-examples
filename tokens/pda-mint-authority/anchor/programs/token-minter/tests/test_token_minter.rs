use {
    anchor_lang::{
        prelude::Rent,
        solana_program::{instruction::Instruction, pubkey::Pubkey, system_program, sysvar::SysvarId},
        AccountDeserialize, InstructionData, ToAccountMetas,
    },
    anchor_spl::{
        associated_token::{get_associated_token_address, ID as ASSOCIATED_TOKEN_PROGRAM_ID},
        metadata::{MetadataAccount, ID as METADATA_PROGRAM_ID},
        token::{Mint, TokenAccount, ID as TOKEN_PROGRAM_ID},
    },
    litesvm::LiteSVM,
    solana_keypair::Keypair,
    solana_message::{Message, VersionedMessage},
    solana_signer::Signer,
    solana_transaction::versioned::VersionedTransaction,
    token_minter::state::MintConfig,
};

type SendResult = Result<(), Vec<String>>;

const NAME: &str = "Solana Gold";
const SYMBOL: &str = "GOLDSOL";
const URI: &str = "https://raw.githubusercontent.com/solana-developers/program-examples/new-examples/tokens/tokens/.assets/spl-token.json";

fn pda(seeds: &[&[u8]]) -> Pubkey {
    Pubkey::find_program_address(seeds, &token_minter::id()).0
}

fn setup() -> (LiteSVM, Keypair) {
    let mut svm = LiteSVM::new();
    let bytes = include_bytes!(concat!(env!("CARGO_MANIFEST_DIR"), "/../../target/deploy/token_minter.so"));
    svm.add_program(token_minter::id(), bytes).unwrap();
    svm.add_program_from_file(
        METADATA_PROGRAM_ID,
        concat!(env!("CARGO_MANIFEST_DIR"), "/../../tests/fixtures/token_metadata.so"),
    )
    .unwrap();
    let payer = Keypair::new();
    svm.airdrop(&payer.pubkey(), 10_000_000_000).unwrap();
    (svm, payer)
}

fn send(svm: &mut LiteSVM, ix: Instruction, payer: &Keypair) -> SendResult {
    let msg = Message::new_with_blockhash(&[ix], Some(&payer.pubkey()), &svm.latest_blockhash());
    let tx = VersionedTransaction::try_new(VersionedMessage::Legacy(msg), &[payer]).unwrap();
    svm.send_transaction(tx).map(|_| ()).map_err(|err| err.meta.logs)
}

fn create_token(svm: &mut LiteSVM, payer: &Keypair) {
    let mint_account = pda(&[b"mint"]);
    let metadata_account = Pubkey::find_program_address(
        &[b"metadata", METADATA_PROGRAM_ID.as_ref(), mint_account.as_ref()],
        &METADATA_PROGRAM_ID,
    )
    .0;
    let ix = Instruction::new_with_bytes(
        token_minter::id(),
        &token_minter::instruction::CreateToken {
            token_name: NAME.to_string(),
            token_symbol: SYMBOL.to_string(),
            token_uri: URI.to_string(),
        }
        .data(),
        token_minter::accounts::CreateToken {
            payer: payer.pubkey(),
            mint_account,
            metadata_account,
            mint_config: pda(&[b"mint_config"]),
            token_program: TOKEN_PROGRAM_ID,
            token_metadata_program: METADATA_PROGRAM_ID,
            system_program: system_program::ID,
            rent: Rent::id(),
        }
        .to_account_metas(None),
    );
    send(svm, ix, payer).unwrap();

    let mint = svm.get_account(&mint_account).unwrap();
    let mint = Mint::try_deserialize(&mut mint.data.as_slice()).unwrap();
    assert_eq!(mint.decimals, 9);
    assert_eq!(mint.mint_authority, Some(mint_account).into());

    let metadata = svm.get_account(&metadata_account).unwrap();
    let metadata = MetadataAccount::try_deserialize(&mut metadata.data.as_slice()).unwrap();
    assert_eq!(metadata.update_authority, mint_account);
    assert_eq!(metadata.name.trim_end_matches('\0'), NAME);
    assert_eq!(metadata.symbol.trim_end_matches('\0'), SYMBOL);
    assert_eq!(metadata.uri.trim_end_matches('\0'), URI);

    let config = svm.get_account(&pda(&[b"mint_config"])).unwrap();
    let config = MintConfig::try_deserialize(&mut config.data.as_slice()).unwrap();
    assert_eq!(config.admin, payer.pubkey());
}

fn mint_token(payer: &Keypair, amount: u64) -> Instruction {
    let mint_account = pda(&[b"mint"]);
    Instruction::new_with_bytes(
        token_minter::id(),
        &token_minter::instruction::MintToken { amount }.data(),
        token_minter::accounts::MintToken {
            payer: payer.pubkey(),
            mint_account,
            mint_config: pda(&[b"mint_config"]),
            associated_token_account: get_associated_token_address(&payer.pubkey(), &mint_account),
            token_program: TOKEN_PROGRAM_ID,
            associated_token_program: ASSOCIATED_TOKEN_PROGRAM_ID,
            system_program: system_program::ID,
        }
        .to_account_metas(None),
    )
}

#[test]
fn create_and_mint_token() {
    let (mut svm, payer) = setup();
    create_token(&mut svm, &payer);

    send(&mut svm, mint_token(&payer, 100), &payer).unwrap();

    let ata = get_associated_token_address(&payer.pubkey(), &pda(&[b"mint"]));
    let ata = svm.get_account(&ata).unwrap();
    let ata = TokenAccount::try_deserialize(&mut ata.data.as_slice()).unwrap();
    assert_eq!(ata.amount, 100 * 10u64.pow(9));
}

#[test]
fn rejects_mint_from_non_admin() {
    let (mut svm, payer) = setup();
    create_token(&mut svm, &payer);

    let outsider = Keypair::new();
    svm.airdrop(&outsider.pubkey(), 1_000_000_000).unwrap();
    let logs = send(&mut svm, mint_token(&outsider, 100), &outsider).unwrap_err();
    assert!(logs.iter().any(|log| log.contains("Error Code: Unauthorized")));
}
