use {
    anchor_lang::{
        solana_program::{instruction::Instruction, pubkey::Pubkey, system_program},
        AnchorDeserialize, InstructionData, ToAccountMetas,
    },
    anchor_spl::{
        token_2022::spl_token_2022::{
            self,
            extension::{BaseStateWithExtensions, StateWithExtensions},
            state::Mint,
        },
        token_2022_extensions::spl_token_metadata_interface::state::TokenMetadata,
    },
    litesvm::LiteSVM,
    metadata::instructions::{AnchorField, TokenMetadataArgs, UpdateFieldArgs},
    solana_keypair::Keypair,
    solana_message::{Message, VersionedMessage},
    solana_signer::Signer,
    solana_transaction::versioned::VersionedTransaction,
};

const URI: &str =
    "https://raw.githubusercontent.com/solana-developers/opos-asset/main/assets/DeveloperPortal/metadata.json";

fn transaction(svm: &LiteSVM, ix: Instruction, payer: &Keypair, signers: &[&Keypair]) -> VersionedTransaction {
    let msg = Message::new_with_blockhash(&[ix], Some(&payer.pubkey()), &svm.latest_blockhash());
    VersionedTransaction::try_new(VersionedMessage::Legacy(msg), signers).unwrap()
}

fn send(svm: &mut LiteSVM, ix: Instruction, payer: &Keypair, signers: &[&Keypair]) -> Vec<u8> {
    let tx = transaction(svm, ix, payer, signers);
    let meta = svm.send_transaction(tx).unwrap();
    svm.expire_blockhash();
    meta.return_data.data
}

fn token_metadata(svm: &LiteSVM, mint: &Pubkey) -> TokenMetadata {
    let account = svm.get_account(mint).unwrap();
    StateWithExtensions::<Mint>::unpack(&account.data).unwrap().get_variable_len_extension::<TokenMetadata>().unwrap()
}

fn update_field(payer: &Keypair, mint: Pubkey, field: AnchorField, value: &str) -> Instruction {
    Instruction::new_with_bytes(
        metadata::id(),
        &metadata::instruction::UpdateField { args: UpdateFieldArgs { field, value: value.to_string() } }.data(),
        metadata::accounts::UpdateField {
            authority: payer.pubkey(),
            mint_account: mint,
            token_program: spl_token_2022::ID,
            system_program: system_program::ID,
        }
        .to_account_metas(None),
    )
}

fn emit(mint: Pubkey) -> Instruction {
    Instruction::new_with_bytes(
        metadata::id(),
        &metadata::instruction::Emit {}.data(),
        metadata::accounts::Emit { mint_account: mint, token_program: spl_token_2022::ID }.to_account_metas(None),
    )
}

#[test]
fn metadata_lifecycle() {
    let mut svm = LiteSVM::new();
    let bytes = include_bytes!(concat!(env!("CARGO_MANIFEST_DIR"), "/../../target/deploy/metadata.so"));
    svm.add_program(metadata::id(), bytes).unwrap();
    let payer = Keypair::new();
    svm.airdrop(&payer.pubkey(), 1_000_000_000).unwrap();
    let mint = Keypair::new();
    let mint_key = mint.pubkey();

    send(
        &mut svm,
        Instruction::new_with_bytes(
            metadata::id(),
            &metadata::instruction::Initialize {
                args: TokenMetadataArgs { name: "OPOS".to_string(), symbol: "OPOS".to_string(), uri: URI.to_string() },
            }
            .data(),
            metadata::accounts::Initialize {
                payer: payer.pubkey(),
                mint_account: mint_key,
                token_program: spl_token_2022::ID,
                system_program: system_program::ID,
            }
            .to_account_metas(None),
        ),
        &payer,
        &[&payer, &mint],
    );
    let state = token_metadata(&svm, &mint_key);
    assert_eq!(state.name, "OPOS");
    assert_eq!(state.symbol, "OPOS");
    assert_eq!(state.uri, URI);
    assert_eq!(state.mint, mint_key);
    assert_eq!(Option::<Pubkey>::from(state.update_authority), Some(payer.pubkey()));

    send(&mut svm, update_field(&payer, mint_key, AnchorField::Name, "Solana"), &payer, &[&payer]);
    assert_eq!(token_metadata(&svm, &mint_key).name, "Solana");

    send(&mut svm, update_field(&payer, mint_key, AnchorField::Key("color".to_string()), "red"), &payer, &[&payer]);
    assert_eq!(token_metadata(&svm, &mint_key).additional_metadata, vec![("color".to_string(), "red".to_string())]);

    send(
        &mut svm,
        Instruction::new_with_bytes(
            metadata::id(),
            &metadata::instruction::RemoveKey { key: "color".to_string() }.data(),
            metadata::accounts::RemoveKey {
                update_authority: payer.pubkey(),
                mint_account: mint_key,
                token_program: spl_token_2022::ID,
                system_program: system_program::ID,
            }
            .to_account_metas(None),
        ),
        &payer,
        &[&payer],
    );
    assert!(token_metadata(&svm, &mint_key).additional_metadata.is_empty());

    send(
        &mut svm,
        Instruction::new_with_bytes(
            metadata::id(),
            &metadata::instruction::UpdateAuthority {}.data(),
            metadata::accounts::UpdateAuthority {
                current_authority: payer.pubkey(),
                new_authority: None,
                mint_account: mint_key,
                token_program: spl_token_2022::ID,
                system_program: system_program::ID,
            }
            .to_account_metas(None),
        ),
        &payer,
        &[&payer],
    );
    let expected = token_metadata(&svm, &mint_key);
    assert_eq!(Option::<Pubkey>::from(expected.update_authority), None);

    let emitted = send(&mut svm, emit(mint_key), &payer, &[&payer]);
    assert_eq!(TokenMetadata::try_from_slice(&emitted).unwrap(), expected);

    let simulated = svm.simulate_transaction(transaction(&svm, emit(mint_key), &payer, &[&payer])).unwrap();
    assert_eq!(TokenMetadata::try_from_slice(&simulated.meta.return_data.data).unwrap(), expected);
}
