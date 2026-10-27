use {
    anchor_lang::{
        solana_program::{instruction::Instruction, pubkey::Pubkey, rent, system_program},
        InstructionData, ToAccountMetas,
    },
    anchor_spl::{
        associated_token::{self, get_associated_token_address_with_program_id},
        token_2022::spl_token_2022::{
            self,
            extension::{
                metadata_pointer::MetadataPointer, BaseStateWithExtensions, StateWithExtensions,
            },
            state::{Account as TokenAccount, Mint},
        },
        token_2022_extensions::spl_token_metadata_interface::state::TokenMetadata,
    },
    litesvm::LiteSVM,
    solana_keypair::Keypair,
    solana_message::{Message, VersionedMessage},
    solana_signer::Signer,
    solana_transaction::versioned::VersionedTransaction,
};

#[test]
fn mint_nft() {
    let mut svm = LiteSVM::new();
    let bytes = include_bytes!(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/../../target/deploy/extension_nft.so"
    ));
    svm.add_program(extension_nft::id(), bytes).unwrap();
    let payer = Keypair::new();
    svm.airdrop(&payer.pubkey(), 1_000_000_000).unwrap();

    let mint = Keypair::new();
    let token_account = get_associated_token_address_with_program_id(
        &payer.pubkey(),
        &mint.pubkey(),
        &spl_token_2022::ID,
    );
    let (nft_authority, _) =
        Pubkey::find_program_address(&[b"nft_authority"], &extension_nft::id());

    let ix = Instruction::new_with_bytes(
        extension_nft::id(),
        &extension_nft::instruction::MintNft {}.data(),
        extension_nft::accounts::MintNft {
            signer: payer.pubkey(),
            system_program: system_program::ID,
            token_program: spl_token_2022::ID,
            token_account,
            mint: mint.pubkey(),
            rent: rent::ID,
            associated_token_program: associated_token::ID,
            nft_authority,
        }
        .to_account_metas(None),
    );
    let msg = Message::new_with_blockhash(&[ix], Some(&payer.pubkey()), &svm.latest_blockhash());
    let tx =
        VersionedTransaction::try_new(VersionedMessage::Legacy(msg), &[&payer, &mint]).unwrap();
    svm.send_transaction(tx).unwrap();

    let token = svm.get_account(&token_account).unwrap();
    let token = StateWithExtensions::<TokenAccount>::unpack(&token.data).unwrap();
    assert_eq!(token.base.amount, 1);
    assert_eq!(token.base.owner, payer.pubkey());

    let mint_account = svm.get_account(&mint.pubkey()).unwrap();
    let mint_state = StateWithExtensions::<Mint>::unpack(&mint_account.data).unwrap();
    assert_eq!(mint_state.base.supply, 1);
    assert_eq!(mint_state.base.decimals, 0);
    assert!(mint_state.base.mint_authority.is_none());

    let pointer = mint_state.get_extension::<MetadataPointer>().unwrap();
    assert_eq!(
        Option::<Pubkey>::from(pointer.metadata_address),
        Some(mint.pubkey())
    );

    let metadata = mint_state
        .get_variable_len_extension::<TokenMetadata>()
        .unwrap();
    assert_eq!(metadata.name, "Beaver");
    assert_eq!(metadata.symbol, "BVA");
    assert_eq!(
        metadata.additional_metadata,
        vec![("level".to_string(), "1".to_string())]
    );
}
