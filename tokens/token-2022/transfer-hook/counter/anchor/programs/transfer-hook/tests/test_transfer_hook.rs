use {
    anchor_lang::{
        solana_program::{
            instruction::{AccountMeta, Instruction},
            pubkey::Pubkey,
            system_instruction::create_account,
            system_program,
        },
        AccountDeserialize, InstructionData, ToAccountMetas,
    },
    anchor_spl::{
        associated_token::{
            self, get_associated_token_address_with_program_id,
            spl_associated_token_account::instruction::create_associated_token_account,
        },
        token_2022::{
            spl_token_2022::{
                self,
                extension::{transfer_hook as hook_extension, ExtensionType},
                state::Mint,
            },
            ID as TOKEN_2022_PROGRAM_ID,
        },
        token_interface::TokenAccount,
    },
    litesvm::LiteSVM,
    solana_keypair::Keypair,
    solana_message::{Message, VersionedMessage},
    solana_signer::Signer,
    solana_transaction::versioned::VersionedTransaction,
};

const DECIMALS: u8 = 9;

struct Env {
    svm: LiteSVM,
    payer: Keypair,
    mint: Pubkey,
    source_token: Pubkey,
    destination_token: Pubkey,
    extra_account_meta_list: Pubkey,
    counter: Pubkey,
}

fn send(
    svm: &mut LiteSVM,
    ixs: &[Instruction],
    payer: &Keypair,
    signers: &[&Keypair],
) -> Result<Vec<String>, Vec<String>> {
    let msg = Message::new_with_blockhash(ixs, Some(&payer.pubkey()), &svm.latest_blockhash());
    let tx = VersionedTransaction::try_new(VersionedMessage::Legacy(msg), signers).unwrap();
    let result = svm.send_transaction(tx).map(|meta| meta.logs).map_err(|failed| failed.meta.logs);
    svm.expire_blockhash();
    result
}

fn ata(owner: &Pubkey, mint: &Pubkey) -> Pubkey {
    get_associated_token_address_with_program_id(owner, mint, &TOKEN_2022_PROGRAM_ID)
}

fn balance(svm: &LiteSVM, token_account: &Pubkey) -> u64 {
    let account = svm.get_account(token_account).unwrap();
    TokenAccount::try_deserialize(&mut account.data.as_slice()).unwrap().amount
}

fn setup() -> Env {
    let mut svm = LiteSVM::new();
    let bytes = include_bytes!(concat!(env!("CARGO_MANIFEST_DIR"), "/../../target/deploy/transfer_hook.so"));
    svm.add_program(transfer_hook::id(), bytes).unwrap();
    let payer = Keypair::new();
    svm.airdrop(&payer.pubkey(), 10_000_000_000).unwrap();

    let mint_kp = Keypair::new();
    let mint = mint_kp.pubkey();
    let space = ExtensionType::try_calculate_account_len::<Mint>(&[ExtensionType::TransferHook]).unwrap();
    let ixs = [
        create_account(
            &payer.pubkey(),
            &mint,
            svm.minimum_balance_for_rent_exemption(space),
            space as u64,
            &TOKEN_2022_PROGRAM_ID,
        ),
        hook_extension::instruction::initialize(
            &TOKEN_2022_PROGRAM_ID,
            &mint,
            Some(payer.pubkey()),
            Some(transfer_hook::id()),
        )
        .unwrap(),
        spl_token_2022::instruction::initialize_mint2(&TOKEN_2022_PROGRAM_ID, &mint, &payer.pubkey(), None, DECIMALS)
            .unwrap(),
    ];
    send(&mut svm, &ixs, &payer, &[&payer, &mint_kp]).unwrap();

    let recipient = Pubkey::new_unique();
    let source_token = ata(&payer.pubkey(), &mint);
    let destination_token = ata(&recipient, &mint);
    let ixs = [
        create_associated_token_account(&payer.pubkey(), &payer.pubkey(), &mint, &TOKEN_2022_PROGRAM_ID),
        create_associated_token_account(&payer.pubkey(), &recipient, &mint, &TOKEN_2022_PROGRAM_ID),
        spl_token_2022::instruction::mint_to(
            &TOKEN_2022_PROGRAM_ID,
            &mint,
            &source_token,
            &payer.pubkey(),
            &[],
            100 * 10u64.pow(DECIMALS as u32),
        )
        .unwrap(),
    ];
    send(&mut svm, &ixs, &payer, &[&payer]).unwrap();

    let extra_account_meta_list =
        Pubkey::find_program_address(&[b"extra-account-metas", mint.as_ref()], &transfer_hook::id()).0;
    let counter = Pubkey::find_program_address(&[b"counter"], &transfer_hook::id()).0;
    let init_meta_list = Instruction::new_with_bytes(
        transfer_hook::id(),
        &transfer_hook::instruction::InitializeExtraAccountMetaList {}.data(),
        transfer_hook::accounts::InitializeExtraAccountMetaList {
            payer: payer.pubkey(),
            extra_account_meta_list,
            mint,
            counter_account: counter,
            token_program: TOKEN_2022_PROGRAM_ID,
            associated_token_program: associated_token::ID,
            system_program: system_program::ID,
        }
        .to_account_metas(None),
    );
    send(&mut svm, &[init_meta_list], &payer, &[&payer]).unwrap();
    assert_eq!(svm.get_account(&counter).unwrap().owner, transfer_hook::id());

    Env { svm, payer, mint, source_token, destination_token, extra_account_meta_list, counter }
}

#[test]
fn transfer_invokes_the_hook_with_the_counter_account() {
    let mut env = setup();
    let amount = 10u64.pow(DECIMALS as u32);

    let mut transfer = spl_token_2022::instruction::transfer_checked(
        &TOKEN_2022_PROGRAM_ID,
        &env.source_token,
        &env.mint,
        &env.destination_token,
        &env.payer.pubkey(),
        &[],
        amount,
        DECIMALS,
    )
    .unwrap();
    transfer.accounts.extend([
        AccountMeta::new(env.counter, false),
        AccountMeta::new_readonly(transfer_hook::id(), false),
        AccountMeta::new_readonly(env.extra_account_meta_list, false),
    ]);

    let logs = send(&mut env.svm, &[transfer], &env.payer, &[&env.payer]).unwrap();
    assert!(logs.iter().any(|log| log.contains("This token has been transferred 1 times")));

    assert_eq!(balance(&env.svm, &env.destination_token), amount);
    assert_eq!(balance(&env.svm, &env.source_token), 99 * amount);
}

#[test]
fn calling_the_hook_outside_a_transfer_fails() {
    let mut env = setup();

    let transfer_hook_ix = Instruction::new_with_bytes(
        transfer_hook::id(),
        &transfer_hook::instruction::TransferHook { amount: 1 }.data(),
        transfer_hook::accounts::TransferHook {
            source_token: env.source_token,
            mint: env.mint,
            destination_token: env.destination_token,
            owner: env.payer.pubkey(),
            extra_account_meta_list: env.extra_account_meta_list,
            counter_account: env.counter,
        }
        .to_account_metas(None),
    );

    let logs = send(&mut env.svm, &[transfer_hook_ix], &env.payer, &[&env.payer]).unwrap_err();
    assert!(logs.iter().any(|log| log.contains("The token is not currently transferring")));
}
