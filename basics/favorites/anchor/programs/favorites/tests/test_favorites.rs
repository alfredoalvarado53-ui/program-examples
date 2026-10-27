use {
    anchor_lang::{
        solana_program::{instruction::Instruction, pubkey::Pubkey, system_program},
        AccountDeserialize, InstructionData, ToAccountMetas,
    },
    favorites::Favorites,
    litesvm::LiteSVM,
    solana_keypair::Keypair,
    solana_message::{Message, VersionedMessage},
    solana_signer::Signer,
    solana_transaction::versioned::VersionedTransaction,
};

const NUMBER: u64 = 23;
const COLOR: &str = "purple";

fn setup() -> (LiteSVM, Keypair) {
    let mut svm = LiteSVM::new();
    let bytes = include_bytes!(concat!(env!("CARGO_MANIFEST_DIR"), "/../../target/deploy/favorites.so"));
    svm.add_program(favorites::id(), bytes).unwrap();
    let user = Keypair::new();
    svm.airdrop(&user.pubkey(), 1_000_000_000).unwrap();
    (svm, user)
}

fn favorites_pda(user: &Pubkey) -> Pubkey {
    Pubkey::find_program_address(&[b"favorites", user.as_ref()], &favorites::id()).0
}

fn set_favorites(user: Pubkey, favorites: Pubkey, hobbies: &[&str]) -> Instruction {
    Instruction::new_with_bytes(
        favorites::id(),
        &favorites::instruction::SetFavorites {
            number: NUMBER,
            color: COLOR.to_string(),
            hobbies: hobbies.iter().map(|h| h.to_string()).collect(),
        }
        .data(),
        favorites::accounts::SetFavorites { user, favorites, system_program: system_program::ID }
            .to_account_metas(None),
    )
}

fn tx(svm: &LiteSVM, ix: Instruction, signer: &Keypair) -> VersionedTransaction {
    let msg = Message::new_with_blockhash(&[ix], Some(&signer.pubkey()), &svm.latest_blockhash());
    VersionedTransaction::try_new(VersionedMessage::Legacy(msg), &[signer]).unwrap()
}

fn assert_favorites(svm: &LiteSVM, pda: &Pubkey, hobbies: &[&str]) {
    let account = svm.get_account(pda).unwrap();
    let data = Favorites::try_deserialize(&mut account.data.as_slice()).unwrap();
    assert_eq!(data.number, NUMBER);
    assert_eq!(data.color, COLOR);
    assert_eq!(data.hobbies, hobbies);
}

#[test]
fn writes_and_updates_favorites() {
    let (mut svm, user) = setup();
    let pda = favorites_pda(&user.pubkey());

    let hobbies = ["skiing", "skydiving", "biking"];
    svm.send_transaction(tx(&svm, set_favorites(user.pubkey(), pda, &hobbies), &user)).unwrap();
    assert_favorites(&svm, &pda, &hobbies);

    let new_hobbies = ["skiing", "skydiving", "biking", "swimming"];
    svm.send_transaction(tx(&svm, set_favorites(user.pubkey(), pda, &new_hobbies), &user)).unwrap();
    assert_favorites(&svm, &pda, &new_hobbies);
}

#[test]
fn rejects_unauthorized_signers() {
    let (mut svm, user) = setup();
    let pda = favorites_pda(&user.pubkey());
    let hobbies = ["skiing", "skydiving", "biking"];
    svm.send_transaction(tx(&svm, set_favorites(user.pubkey(), pda, &hobbies), &user)).unwrap();

    let some_random_guy = Keypair::new();
    svm.airdrop(&some_random_guy.pubkey(), 1_000_000_000).unwrap();

    let ix = set_favorites(user.pubkey(), pda, &["coding"]);
    let msg = Message::new_with_blockhash(&[ix], Some(&user.pubkey()), &svm.latest_blockhash());
    assert!(VersionedTransaction::try_new(VersionedMessage::Legacy(msg), &[&some_random_guy]).is_err());

    let result =
        svm.send_transaction(tx(&svm, set_favorites(some_random_guy.pubkey(), pda, &["coding"]), &some_random_guy));
    let err = result.unwrap_err();
    assert!(err.meta.logs.iter().any(|log| log.contains("ConstraintSeeds")));
    assert_favorites(&svm, &pda, &hobbies);
}
