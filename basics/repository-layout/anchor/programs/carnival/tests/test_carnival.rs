use {
    anchor_lang::{solana_program::instruction::Instruction, InstructionData, ToAccountMetas},
    litesvm::LiteSVM,
    solana_keypair::Keypair,
    solana_message::{Message, VersionedMessage},
    solana_signer::Signer,
    solana_transaction::versioned::VersionedTransaction,
};

fn setup() -> (LiteSVM, Keypair) {
    let mut svm = LiteSVM::new();
    let bytes = include_bytes!(concat!(env!("CARGO_MANIFEST_DIR"), "/../../target/deploy/carnival.so"));
    svm.add_program(carnival::id(), bytes).unwrap();
    let payer = Keypair::new();
    svm.airdrop(&payer.pubkey(), 5_000_000_000).unwrap();
    (svm, payer)
}

fn ix(payer: &Keypair, data: impl InstructionData) -> Instruction {
    Instruction::new_with_bytes(
        carnival::id(),
        &data.data(),
        carnival::accounts::CarnivalContext { payer: payer.pubkey() }.to_account_metas(None),
    )
}

fn program_logs(svm: &mut LiteSVM, payer: &Keypair, ixs: &[Instruction]) -> Vec<String> {
    let msg = Message::new_with_blockhash(ixs, Some(&payer.pubkey()), &svm.latest_blockhash());
    let tx = VersionedTransaction::try_new(VersionedMessage::Legacy(msg), &[payer]).unwrap();
    svm.send_transaction(tx)
        .unwrap()
        .logs
        .into_iter()
        .filter_map(|log| log.strip_prefix("Program log: ").map(str::to_string))
        .filter(|log| !log.starts_with("Instruction: "))
        .collect()
}

fn go_on_ride(payer: &Keypair, name: &str, height: u32, ticket_count: u32, ride_name: &str) -> Instruction {
    ix(payer, carnival::instruction::GoOnRide { name: name.into(), height, ticket_count, ride_name: ride_name.into() })
}

fn play_game(payer: &Keypair, name: &str, ticket_count: u32, game_name: &str) -> Instruction {
    ix(payer, carnival::instruction::PlayGame { name: name.into(), ticket_count, game_name: game_name.into() })
}

fn eat_food(payer: &Keypair, name: &str, ticket_count: u32, food_stand_name: &str) -> Instruction {
    ix(
        payer,
        carnival::instruction::EatFood { name: name.into(), ticket_count, food_stand_name: food_stand_name.into() },
    )
}

#[test]
fn go_on_some_rides() {
    let (mut svm, payer) = setup();
    let logs = program_logs(
        &mut svm,
        &payer,
        &[
            go_on_ride(&payer, "Jimmy", 36, 15, "Scrambler"),
            go_on_ride(&payer, "Mary", 52, 1, "Ferris Wheel"),
            go_on_ride(&payer, "Alice", 56, 15, "Scrambler"),
            go_on_ride(&payer, "Bob", 49, 6, "Tilt-a-Whirl"),
        ],
    );
    assert_eq!(
        logs,
        [
            "You're about to ride the Scrambler!",
            "  Sorry Jimmy, you need to be 48\" tall to ride the Scrambler!",
            "You're about to ride the Ferris Wheel!",
            "  Sorry Mary, you need 5 tickets to ride the Ferris Wheel!",
            "You're about to ride the Scrambler!",
            "  Welcome aboard the Scrambler!",
            "You're about to ride the Tilt-a-Whirl!",
            "  Welcome aboard the Tilt-a-Whirl!",
        ]
    );
}

#[test]
fn play_some_games() {
    let (mut svm, payer) = setup();
    let logs = program_logs(
        &mut svm,
        &payer,
        &[
            play_game(&payer, "Jimmy", 15, "I Got It!"),
            play_game(&payer, "Mary", 1, "Ring Toss"),
            play_game(&payer, "Alice", 15, "Ladder Climb"),
            play_game(&payer, "Bob", 6, "Ring Toss"),
        ],
    );
    assert_eq!(
        logs,
        [
            "You're about to play I Got It!!",
            "  Let's see what you got!",
            "  You get 12 attempts and the prize is a goldfish!",
            "You're about to play Ring Toss!",
            "  Sorry Mary, you need 3 tickets to play Ring Toss!",
            "You're about to play Ladder Climb!",
            "  Let's see what you got!",
            "  You get 1 attempts and the prize is a popcorn bucket!",
            "You're about to play Ring Toss!",
            "  Let's see what you got!",
            "  You get 5 attempts and the prize is a teddy bear!",
        ]
    );
}

#[test]
fn eat_some_food() {
    let (mut svm, payer) = setup();
    let logs = program_logs(
        &mut svm,
        &payer,
        &[
            eat_food(&payer, "Jimmy", 15, "Taco Shack"),
            eat_food(&payer, "Mary", 1, "Larry's Pizza"),
            eat_food(&payer, "Alice", 15, "Dough Boy's"),
            eat_food(&payer, "Bob", 6, "Dough Boy's"),
        ],
    );
    assert_eq!(
        logs,
        [
            "Welcome to Taco Shack! What can I get you?",
            "  Enjoy your taco!",
            "Welcome to Larry's Pizza! What can I get you?",
            "  Sorry Mary, our pizza is 3 tickets!",
            "Welcome to Dough Boy's! What can I get you?",
            "  Enjoy your fried dough!",
            "Welcome to Dough Boy's! What can I get you?",
            "  Enjoy your fried dough!",
        ]
    );
}
