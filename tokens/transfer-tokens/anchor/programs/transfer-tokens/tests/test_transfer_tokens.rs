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
};

const NAME: &str = "Solana Gold";
const SYMBOL: &str = "GOLDSOL";
const URI: &str = "https://raw.githubusercontent.com/solana-developers/program-examples/new-examples/tokens/tokens/.assets/spl-token.json";
const DECIMALS: u32 = 9;

fn setup() -> (LiteSVM, Keypair) {
    let mut svm = LiteSVM::new();
    let bytes = include_bytes!(concat!(env!("CARGO_MANIFEST_DIR"), "/../../target/deploy/transfer_tokens.so"));
    svm.add_program(transfer_tokens::id(), bytes).unwrap();
    svm.add_program_from_file(
        METADATA_PROGRAM_ID,
        concat!(env!("CARGO_MANIFEST_DIR"), "/../../tests/fixtures/token_metadata.so"),
    )
    .unwrap();
    let payer = Keypair::new();
    svm.airdrop(&payer.pubkey(), 10_000_000_000).unwrap();
    (svm, payer)
}

fn send(svm: &mut LiteSVM, ix: Instruction, payer: &Keypair, signers: &[&Keypair]) {
    let msg = Message::new_with_blockhash(&[ix], Some(&payer.pubkey()), &svm.latest_blockhash());
    let tx = VersionedTransaction::try_new(VersionedMessage::Legacy(msg), signers).unwrap();
    svm.send_transaction(tx).unwrap();
}

fn token_amount(svm: &LiteSVM, address: &Pubkey) -> u64 {
    let account = svm.get_account(address).unwrap();
    TokenAccount::try_deserialize(&mut account.data.as_slice()).unwrap().amount
}

#[test]
fn create_mint_and_transfer() {
    let (mut svm, payer) = setup();
    let mint = Keypair::new();
    let recipient = Keypair::new();
    let sender_ata = get_associated_token_address(&payer.pubkey(), &mint.pubkey());
    let recipient_ata = get_associated_token_address(&recipient.pubkey(), &mint.pubkey());
    let metadata_account = Pubkey::find_program_address(
        &[b"metadata", METADATA_PROGRAM_ID.as_ref(), mint.pubkey().as_ref()],
        &METADATA_PROGRAM_ID,
    )
    .0;

    let create = Instruction::new_with_bytes(
        transfer_tokens::id(),
        &transfer_tokens::instruction::CreateToken {
            token_title: NAME.to_string(),
            token_symbol: SYMBOL.to_string(),
            token_uri: URI.to_string(),
        }
        .data(),
        transfer_tokens::accounts::CreateToken {
            payer: payer.pubkey(),
            mint_account: mint.pubkey(),
            metadata_account,
            token_program: TOKEN_PROGRAM_ID,
            token_metadata_program: METADATA_PROGRAM_ID,
            system_program: system_program::ID,
            rent: Rent::id(),
        }
        .to_account_metas(None),
    );
    send(&mut svm, create, &payer, &[&payer, &mint]);
    let mint_account = svm.get_account(&mint.pubkey()).unwrap();
    let mint_state = Mint::try_deserialize(&mut mint_account.data.as_slice()).unwrap();
    assert_eq!(mint_state.decimals, DECIMALS as u8);
    let metadata = svm.get_account(&metadata_account).unwrap();
    let metadata = MetadataAccount::try_deserialize(&mut metadata.data.as_slice()).unwrap();
    assert_eq!(metadata.name.trim_end_matches('\0'), NAME);
    assert_eq!(metadata.symbol.trim_end_matches('\0'), SYMBOL);
    assert_eq!(metadata.uri.trim_end_matches('\0'), URI);

    let mint_to = Instruction::new_with_bytes(
        transfer_tokens::id(),
        &transfer_tokens::instruction::MintToken { amount: 100 }.data(),
        transfer_tokens::accounts::MintToken {
            mint_authority: payer.pubkey(),
            recipient: payer.pubkey(),
            mint_account: mint.pubkey(),
            associated_token_account: sender_ata,
            token_program: TOKEN_PROGRAM_ID,
            associated_token_program: ASSOCIATED_TOKEN_PROGRAM_ID,
            system_program: system_program::ID,
        }
        .to_account_metas(None),
    );
    send(&mut svm, mint_to, &payer, &[&payer]);
    assert_eq!(token_amount(&svm, &sender_ata), 100 * 10u64.pow(DECIMALS));

    let transfer = Instruction::new_with_bytes(
        transfer_tokens::id(),
        &transfer_tokens::instruction::TransferTokens { amount: 50 }.data(),
        transfer_tokens::accounts::TransferTokens {
            sender: payer.pubkey(),
            recipient: recipient.pubkey(),
            mint_account: mint.pubkey(),
            sender_token_account: sender_ata,
            recipient_token_account: recipient_ata,
            token_program: TOKEN_PROGRAM_ID,
            associated_token_program: ASSOCIATED_TOKEN_PROGRAM_ID,
            system_program: system_program::ID,
        }
        .to_account_metas(None),
    );
    send(&mut svm, transfer, &payer, &[&payer]);
    assert_eq!(token_amount(&svm, &sender_ata), 50 * 10u64.pow(DECIMALS));
    assert_eq!(token_amount(&svm, &recipient_ata), 50 * 10u64.pow(DECIMALS));
}
