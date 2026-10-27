use {
    anchor_lang::{
        solana_program::{
            instruction::{AccountMeta, Instruction},
            program_pack::Pack,
            pubkey::Pubkey,
            system_instruction::{create_account, transfer},
            system_program,
        },
        AccountDeserialize, InstructionData, ToAccountMetas,
    },
    anchor_spl::{
        associated_token::{
            self, get_associated_token_address_with_program_id,
            spl_associated_token_account::instruction::create_associated_token_account,
        },
        token::{spl_token, ID as TOKEN_PROGRAM_ID},
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
    solana_account::Account,
    solana_keypair::Keypair,
    solana_message::{Message, VersionedMessage},
    solana_signer::Signer,
    solana_transaction::versioned::VersionedTransaction,
};

const DECIMALS: u8 = 9;
const NATIVE_MINT: Pubkey = spl_token::native_mint::ID;

fn send(svm: &mut LiteSVM, ixs: &[Instruction], payer: &Keypair, signers: &[&Keypair]) {
    let msg = Message::new_with_blockhash(ixs, Some(&payer.pubkey()), &svm.latest_blockhash());
    let tx = VersionedTransaction::try_new(VersionedMessage::Legacy(msg), signers).unwrap();
    svm.send_transaction(tx).unwrap();
    svm.expire_blockhash();
}

fn balance(svm: &LiteSVM, token_account: &Pubkey) -> u64 {
    let account = svm.get_account(token_account).unwrap();
    TokenAccount::try_deserialize(&mut account.data.as_slice()).unwrap().amount
}

fn add_native_mint(svm: &mut LiteSVM) {
    let mut data = vec![0; spl_token::state::Mint::LEN];
    spl_token::state::Mint { decimals: 9, is_initialized: true, ..Default::default() }.pack_into_slice(&mut data);
    let account = Account {
        lamports: svm.minimum_balance_for_rent_exemption(data.len()),
        data,
        owner: TOKEN_PROGRAM_ID,
        executable: false,
        rent_epoch: 0,
    };
    svm.set_account(NATIVE_MINT, account).unwrap();
}

#[test]
fn transfer_charges_a_wsol_fee_through_the_hook() {
    let mut svm = LiteSVM::new();
    let bytes = include_bytes!(concat!(env!("CARGO_MANIFEST_DIR"), "/../../target/deploy/transfer_hook.so"));
    svm.add_program(transfer_hook::id(), bytes).unwrap();
    add_native_mint(&mut svm);
    let payer = Keypair::new();
    svm.airdrop(&payer.pubkey(), 1_000_000_000_000).unwrap();

    let delegate = Pubkey::find_program_address(&[b"delegate"], &transfer_hook::id()).0;
    let sender_wsol = get_associated_token_address_with_program_id(&payer.pubkey(), &NATIVE_MINT, &TOKEN_PROGRAM_ID);
    let delegate_wsol = get_associated_token_address_with_program_id(&delegate, &NATIVE_MINT, &TOKEN_PROGRAM_ID);
    let ixs = [
        create_associated_token_account(&payer.pubkey(), &payer.pubkey(), &NATIVE_MINT, &TOKEN_PROGRAM_ID),
        create_associated_token_account(&payer.pubkey(), &delegate, &NATIVE_MINT, &TOKEN_PROGRAM_ID),
    ];
    send(&mut svm, &ixs, &payer, &[&payer]);

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
    send(&mut svm, &ixs, &payer, &[&payer, &mint_kp]);

    let recipient = Pubkey::new_unique();
    let source_token = get_associated_token_address_with_program_id(&payer.pubkey(), &mint, &TOKEN_2022_PROGRAM_ID);
    let destination_token = get_associated_token_address_with_program_id(&recipient, &mint, &TOKEN_2022_PROGRAM_ID);
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
    send(&mut svm, &ixs, &payer, &[&payer]);

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
            system_program: system_program::ID,
        }
        .to_account_metas(None),
    );
    send(&mut svm, &[init_meta_list], &payer, &[&payer]);

    let amount = 10u64.pow(DECIMALS as u32);
    let mut transfer_ix = spl_token_2022::instruction::transfer_checked(
        &TOKEN_2022_PROGRAM_ID,
        &source_token,
        &mint,
        &destination_token,
        &payer.pubkey(),
        &[],
        amount,
        DECIMALS,
    )
    .unwrap();
    transfer_ix.accounts.extend([
        AccountMeta::new_readonly(NATIVE_MINT, false),
        AccountMeta::new_readonly(TOKEN_PROGRAM_ID, false),
        AccountMeta::new_readonly(associated_token::ID, false),
        AccountMeta::new(delegate, false),
        AccountMeta::new(delegate_wsol, false),
        AccountMeta::new(sender_wsol, false),
        AccountMeta::new(counter, false),
        AccountMeta::new_readonly(transfer_hook::id(), false),
        AccountMeta::new_readonly(extra_account_meta_list, false),
    ]);
    let ixs = [
        transfer(&payer.pubkey(), &sender_wsol, amount),
        spl_token::instruction::sync_native(&TOKEN_PROGRAM_ID, &sender_wsol).unwrap(),
        spl_token::instruction::approve(&TOKEN_PROGRAM_ID, &sender_wsol, &delegate, &payer.pubkey(), &[], amount)
            .unwrap(),
        transfer_ix,
    ];
    send(&mut svm, &ixs, &payer, &[&payer]);

    assert_eq!(balance(&svm, &delegate_wsol), amount);
    assert_eq!(balance(&svm, &sender_wsol), 0);
    assert_eq!(balance(&svm, &destination_token), amount);
}
