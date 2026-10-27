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
            get_associated_token_address_with_program_id,
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
    transfer_switch::state::{AdminConfig, TransferSwitch},
};

const DECIMALS: u8 = 9;

struct Env {
    svm: LiteSVM,
    payer: Keypair,
    sender: Keypair,
    mint: Pubkey,
    sender_token: Pubkey,
    extra_account_metas_list: Pubkey,
    admin_config: Pubkey,
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

fn wallet_switch_address(wallet: &Pubkey) -> Pubkey {
    Pubkey::find_program_address(&[wallet.as_ref()], &transfer_switch::id()).0
}

fn setup() -> Env {
    let mut svm = LiteSVM::new();
    let bytes = include_bytes!(concat!(env!("CARGO_MANIFEST_DIR"), "/../../target/deploy/transfer_switch.so"));
    svm.add_program(transfer_switch::id(), bytes).unwrap();
    let payer = Keypair::new();
    svm.airdrop(&payer.pubkey(), 10_000_000_000).unwrap();
    let sender = Keypair::new();

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
            Some(transfer_switch::id()),
        )
        .unwrap(),
        spl_token_2022::instruction::initialize_mint2(&TOKEN_2022_PROGRAM_ID, &mint, &payer.pubkey(), None, DECIMALS)
            .unwrap(),
    ];
    send(&mut svm, &ixs, &payer, &[&payer, &mint_kp]).unwrap();

    let sender_token = ata(&sender.pubkey(), &mint);
    let ixs = [
        create_associated_token_account(&payer.pubkey(), &sender.pubkey(), &mint, &TOKEN_2022_PROGRAM_ID),
        spl_token_2022::instruction::mint_to(
            &TOKEN_2022_PROGRAM_ID,
            &mint,
            &sender_token,
            &payer.pubkey(),
            &[],
            100 * 10u64.pow(DECIMALS as u32),
        )
        .unwrap(),
    ];
    send(&mut svm, &ixs, &payer, &[&payer]).unwrap();

    let extra_account_metas_list =
        Pubkey::find_program_address(&[b"extra-account-metas", mint.as_ref()], &transfer_switch::id()).0;
    let init_metas_list = Instruction::new_with_bytes(
        transfer_switch::id(),
        &transfer_switch::instruction::InitializeExtraAccountMetasList {}.data(),
        transfer_switch::accounts::InitializeExtraAccountMetas {
            payer: payer.pubkey(),
            token_mint: mint,
            extra_account_metas_list,
            system_program: system_program::ID,
        }
        .to_account_metas(None),
    );
    send(&mut svm, &[init_metas_list], &payer, &[&payer]).unwrap();

    let admin_config = Pubkey::find_program_address(&[b"admin-config"], &transfer_switch::id()).0;
    let configure_admin = Instruction::new_with_bytes(
        transfer_switch::id(),
        &transfer_switch::instruction::ConfigureAdmin {}.data(),
        transfer_switch::accounts::ConfigureAdmin {
            admin: payer.pubkey(),
            new_admin: payer.pubkey(),
            admin_config,
            system_program: system_program::ID,
        }
        .to_account_metas(None),
    );
    send(&mut svm, &[configure_admin], &payer, &[&payer]).unwrap();

    Env { svm, payer, sender, mint, sender_token, extra_account_metas_list, admin_config }
}

fn switch(env: &mut Env, on: bool) -> TransferSwitch {
    let wallet_switch = wallet_switch_address(&env.sender.pubkey());
    let ix = Instruction::new_with_bytes(
        transfer_switch::id(),
        &transfer_switch::instruction::Switch { on }.data(),
        transfer_switch::accounts::Switch {
            admin: env.payer.pubkey(),
            wallet: env.sender.pubkey(),
            admin_config: env.admin_config,
            wallet_switch,
            system_program: system_program::ID,
        }
        .to_account_metas(None),
    );
    send(&mut env.svm, &[ix], &env.payer, &[&env.payer]).unwrap();
    let account = env.svm.get_account(&wallet_switch).unwrap();
    TransferSwitch::try_deserialize(&mut account.data.as_slice()).unwrap()
}

fn transfer_to_new_recipient(env: &mut Env, amount: u64) -> (Pubkey, Result<Vec<String>, Vec<String>>) {
    let recipient = Pubkey::new_unique();
    let recipient_token = ata(&recipient, &env.mint);
    let create_recipient_token =
        create_associated_token_account(&env.payer.pubkey(), &recipient, &env.mint, &TOKEN_2022_PROGRAM_ID);
    send(&mut env.svm, &[create_recipient_token], &env.payer, &[&env.payer]).unwrap();

    let mut transfer = spl_token_2022::instruction::transfer_checked(
        &TOKEN_2022_PROGRAM_ID,
        &env.sender_token,
        &env.mint,
        &recipient_token,
        &env.sender.pubkey(),
        &[],
        amount,
        DECIMALS,
    )
    .unwrap();
    transfer.accounts.extend([
        AccountMeta::new_readonly(wallet_switch_address(&env.sender.pubkey()), false),
        AccountMeta::new_readonly(transfer_switch::id(), false),
        AccountMeta::new_readonly(env.extra_account_metas_list, false),
    ]);
    let result = send(&mut env.svm, &[transfer], &env.payer, &[&env.payer, &env.sender]);
    (recipient_token, result)
}

#[test]
fn configure_admin_sets_the_payer_as_admin() {
    let env = setup();
    let account = env.svm.get_account(&env.admin_config).unwrap();
    let admin_config = AdminConfig::try_deserialize(&mut account.data.as_slice()).unwrap();
    assert!(admin_config.is_initialised);
    assert_eq!(admin_config.admin, env.payer.pubkey());
}

#[test]
fn transfers_follow_the_sender_switch() {
    let mut env = setup();
    let amount = 10u64.pow(DECIMALS as u32);

    let wallet_switch = switch(&mut env, false);
    assert_eq!(wallet_switch.wallet, env.sender.pubkey());
    assert!(!wallet_switch.on);

    let (recipient_token, result) = transfer_to_new_recipient(&mut env, amount);
    let logs = result.unwrap_err();
    assert!(logs.iter().any(|log| log.contains("The transfer switch is currently not on")));
    assert_eq!(balance(&env.svm, &recipient_token), 0);

    let wallet_switch = switch(&mut env, true);
    assert_eq!(wallet_switch.wallet, env.sender.pubkey());
    assert!(wallet_switch.on);

    let (recipient_token, result) = transfer_to_new_recipient(&mut env, amount);
    result.unwrap();
    assert_eq!(balance(&env.svm, &recipient_token), amount);
}
