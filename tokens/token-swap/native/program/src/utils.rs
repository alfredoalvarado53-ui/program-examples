use {
    crate::error::SwapError,
    solana_program::{
        account_info::AccountInfo,
        entrypoint::ProgramResult,
        program::{invoke, invoke_signed},
        program_error::ProgramError,
        program_pack::Pack,
        pubkey::Pubkey,
        rent::Rent,
        sysvar::Sysvar,
    },
    solana_system_interface::instruction as system_instruction,
    spl_associated_token_account_interface::address::get_associated_token_address,
    spl_token_interface::{instruction as token_instruction, state::Account as TokenAccount},
};

pub fn assert_is_associated_token_account(
    token_address: &Pubkey,
    owner: &Pubkey,
    mint: &Pubkey,
) -> Result<(), ProgramError> {
    let expected = get_associated_token_address(owner, mint);
    if token_address != &expected {
        return Err(SwapError::TokenAccountMismatch.into());
    }
    Ok(())
}

pub fn create_pda_account<'a>(
    payer: &AccountInfo<'a>,
    target_pda: &AccountInfo<'a>,
    system_program: &AccountInfo<'a>,
    program_id: &Pubkey,
    space: usize,
    signer_seeds: &[&[u8]],
) -> ProgramResult {
    let rent = Rent::get()?;
    let required_lamports = rent.minimum_balance(space);

    let create_account_ix = system_instruction::create_account(
        payer.key,
        target_pda.key,
        required_lamports,
        space as u64,
        program_id,
    );

    invoke_signed(
        &create_account_ix,
        &[payer.clone(), target_pda.clone(), system_program.clone()],
        &[signer_seeds],
    )
}

pub fn unpack_token_account(account_info: &AccountInfo) -> Result<TokenAccount, ProgramError> {
    TokenAccount::unpack(&account_info.data.borrow()).map_err(|_| ProgramError::InvalidAccountData)
}

pub fn token_transfer<'a>(
    token_program: &AccountInfo<'a>,
    from: &AccountInfo<'a>,
    to: &AccountInfo<'a>,
    authority: &AccountInfo<'a>,
    amount: u64,
    signer_seeds: Option<&[&[u8]]>,
) -> ProgramResult {
    let ix = token_instruction::transfer(
        token_program.key,
        from.key,
        to.key,
        authority.key,
        &[],
        amount,
    )?;

    let account_infos = [from.clone(), to.clone(), authority.clone(), token_program.clone()];

    if let Some(seeds) = signer_seeds {
        invoke_signed(&ix, &account_infos, &[seeds])
    } else {
        invoke(&ix, &account_infos)
    }
}

pub fn token_mint_to<'a>(
    token_program: &AccountInfo<'a>,
    mint: &AccountInfo<'a>,
    to: &AccountInfo<'a>,
    authority: &AccountInfo<'a>,
    amount: u64,
    signer_seeds: &[&[u8]],
) -> ProgramResult {
    let ix = token_instruction::mint_to(
        token_program.key,
        mint.key,
        to.key,
        authority.key,
        &[],
        amount,
    )?;

    invoke_signed(
        &ix,
        &[mint.clone(), to.clone(), authority.clone(), token_program.clone()],
        &[signer_seeds],
    )
}

pub fn token_burn<'a>(
    token_program: &AccountInfo<'a>,
    mint: &AccountInfo<'a>,
    from: &AccountInfo<'a>,
    authority: &AccountInfo<'a>,
    amount: u64,
) -> ProgramResult {
    let ix = token_instruction::burn(
        token_program.key,
        from.key,
        mint.key,
        authority.key,
        &[],
        amount,
    )?;

    invoke(&ix, &[mint.clone(), from.clone(), authority.clone(), token_program.clone()])
}
