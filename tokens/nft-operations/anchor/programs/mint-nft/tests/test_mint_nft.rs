use {
    anchor_lang::{
        prelude::Instructions,
        solana_program::{instruction::Instruction, pubkey::Pubkey, system_program, sysvar::SysvarId},
        AccountDeserialize, InstructionData, ToAccountMetas,
    },
    anchor_spl::{
        associated_token::{get_associated_token_address, ID as ASSOCIATED_TOKEN_PROGRAM_ID},
        metadata::{MasterEditionAccount, MetadataAccount, ID as METADATA_PROGRAM_ID},
        token::{TokenAccount, ID as TOKEN_PROGRAM_ID},
    },
    litesvm::LiteSVM,
    solana_keypair::Keypair,
    solana_message::{Message, VersionedMessage},
    solana_signer::Signer,
    solana_transaction::versioned::VersionedTransaction,
};

fn setup() -> (LiteSVM, Keypair) {
    let mut svm = LiteSVM::new();
    let bytes = include_bytes!(concat!(env!("CARGO_MANIFEST_DIR"), "/../../target/deploy/mint_nft.so"));
    svm.add_program(mint_nft::id(), bytes).unwrap();
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

fn metadata_pda(mint: &Pubkey) -> Pubkey {
    Pubkey::find_program_address(&[b"metadata", METADATA_PROGRAM_ID.as_ref(), mint.as_ref()], &METADATA_PROGRAM_ID).0
}

fn master_edition_pda(mint: &Pubkey) -> Pubkey {
    Pubkey::find_program_address(
        &[b"metadata", METADATA_PROGRAM_ID.as_ref(), mint.as_ref(), b"edition"],
        &METADATA_PROGRAM_ID,
    )
    .0
}

fn metadata(svm: &LiteSVM, mint: &Pubkey) -> MetadataAccount {
    let account = svm.get_account(&metadata_pda(mint)).unwrap();
    MetadataAccount::try_deserialize(&mut account.data.as_slice()).unwrap()
}

fn token_amount(svm: &LiteSVM, address: &Pubkey) -> u64 {
    let account = svm.get_account(address).unwrap();
    TokenAccount::try_deserialize(&mut account.data.as_slice()).unwrap().amount
}

#[test]
fn create_collection_mint_nft_and_verify() {
    let (mut svm, payer) = setup();
    let mint_authority = Pubkey::find_program_address(&[b"authority"], &mint_nft::id()).0;
    let collection = Keypair::new();
    let nft = Keypair::new();

    let collection_destination = get_associated_token_address(&payer.pubkey(), &collection.pubkey());
    let create_collection = Instruction::new_with_bytes(
        mint_nft::id(),
        &mint_nft::instruction::CreateCollection {}.data(),
        mint_nft::accounts::CreateCollection {
            user: payer.pubkey(),
            mint: collection.pubkey(),
            mint_authority,
            metadata: metadata_pda(&collection.pubkey()),
            master_edition: master_edition_pda(&collection.pubkey()),
            destination: collection_destination,
            system_program: system_program::ID,
            token_program: TOKEN_PROGRAM_ID,
            associated_token_program: ASSOCIATED_TOKEN_PROGRAM_ID,
            token_metadata_program: METADATA_PROGRAM_ID,
        }
        .to_account_metas(None),
    );
    send(&mut svm, create_collection, &payer, &[&payer, &collection]);
    assert_eq!(token_amount(&svm, &collection_destination), 1);
    let collection_metadata = metadata(&svm, &collection.pubkey());
    assert_eq!(collection_metadata.name.trim_end_matches('\0'), "DummyCollection");
    assert!(collection_metadata.collection_details.is_some());
    let edition = svm.get_account(&master_edition_pda(&collection.pubkey())).unwrap();
    MasterEditionAccount::try_deserialize(&mut edition.data.as_slice()).unwrap();

    let nft_destination = get_associated_token_address(&payer.pubkey(), &nft.pubkey());
    let mint_nft_ix = Instruction::new_with_bytes(
        mint_nft::id(),
        &mint_nft::instruction::MintNft {}.data(),
        mint_nft::accounts::MintNFT {
            owner: payer.pubkey(),
            mint: nft.pubkey(),
            destination: nft_destination,
            metadata: metadata_pda(&nft.pubkey()),
            master_edition: master_edition_pda(&nft.pubkey()),
            mint_authority,
            collection_mint: collection.pubkey(),
            system_program: system_program::ID,
            token_program: TOKEN_PROGRAM_ID,
            associated_token_program: ASSOCIATED_TOKEN_PROGRAM_ID,
            token_metadata_program: METADATA_PROGRAM_ID,
        }
        .to_account_metas(None),
    );
    send(&mut svm, mint_nft_ix, &payer, &[&payer, &nft]);
    assert_eq!(token_amount(&svm, &nft_destination), 1);
    let nft_collection = metadata(&svm, &nft.pubkey()).collection.clone().unwrap();
    assert_eq!(nft_collection.key, collection.pubkey());
    assert!(!nft_collection.verified);

    let verify = Instruction::new_with_bytes(
        mint_nft::id(),
        &mint_nft::instruction::VerifyCollection {}.data(),
        mint_nft::accounts::VerifyCollectionMint {
            authority: payer.pubkey(),
            metadata: metadata_pda(&nft.pubkey()),
            mint: nft.pubkey(),
            mint_authority,
            collection_mint: collection.pubkey(),
            collection_metadata: metadata_pda(&collection.pubkey()),
            collection_master_edition: master_edition_pda(&collection.pubkey()),
            system_program: system_program::ID,
            sysvar_instruction: Instructions::id(),
            token_metadata_program: METADATA_PROGRAM_ID,
        }
        .to_account_metas(None),
    );
    send(&mut svm, verify, &payer, &[&payer]);
    assert!(metadata(&svm, &nft.pubkey()).collection.clone().unwrap().verified);
}
