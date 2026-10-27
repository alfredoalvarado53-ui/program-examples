use {
    anchor_lang::{
        solana_program::{instruction::Instruction, pubkey::Pubkey, system_program},
        InstructionData, ToAccountMetas,
    },
    anchor_spl::token_2022::spl_token_2022::{
        self,
        extension::{group_pointer::GroupPointer, BaseStateWithExtensions, StateWithExtensions},
        state::Mint,
    },
    litesvm::LiteSVM,
    solana_keypair::Keypair,
    solana_message::{Message, VersionedMessage},
    solana_signer::Signer,
    solana_transaction::versioned::VersionedTransaction,
};

#[test]
fn create_mint_with_group_pointer() {
    let mut svm = LiteSVM::new();
    let bytes = include_bytes!(concat!(env!("CARGO_MANIFEST_DIR"), "/../../target/deploy/group.so"));
    svm.add_program(group::id(), bytes).unwrap();
    let payer = Keypair::new();
    svm.airdrop(&payer.pubkey(), 1_000_000_000).unwrap();
    let (mint, _) = Pubkey::find_program_address(&[b"group"], &group::id());

    let ix = Instruction::new_with_bytes(
        group::id(),
        &group::instruction::TestInitializeGroup {}.data(),
        group::accounts::InitializeGroup {
            payer: payer.pubkey(),
            mint_account: mint,
            token_program: spl_token_2022::ID,
            system_program: system_program::ID,
        }
        .to_account_metas(None),
    );
    let msg = Message::new_with_blockhash(&[ix], Some(&payer.pubkey()), &svm.latest_blockhash());
    let tx = VersionedTransaction::try_new(VersionedMessage::Legacy(msg), &[&payer]).unwrap();
    svm.send_transaction(tx).unwrap();

    let account = svm.get_account(&mint).unwrap();
    assert_eq!(account.owner, spl_token_2022::ID);
    let state = StateWithExtensions::<Mint>::unpack(&account.data).unwrap();
    assert_eq!(state.base.decimals, 2);
    assert_eq!(Option::<Pubkey>::from(state.base.mint_authority), Some(mint));
    assert_eq!(Option::<Pubkey>::from(state.base.freeze_authority), Some(mint));
    let pointer = state.get_extension::<GroupPointer>().unwrap();
    assert_eq!(Option::<Pubkey>::from(pointer.authority), Some(mint));
    assert_eq!(Option::<Pubkey>::from(pointer.group_address), Some(mint));
}
