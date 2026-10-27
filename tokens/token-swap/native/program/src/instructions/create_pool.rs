use {
    crate::{
        constants::{AUTHORITY_SEED, LIQUIDITY_SEED},
        error::SwapError,
        state::{Amm, Pool},
        utils::create_pda_account,
    },
    borsh::{BorshDeserialize, BorshSerialize},
    solana_program::{
        account_info::AccountInfo,
        entrypoint::ProgramResult,
        program::{invoke, invoke_signed},
        program_error::ProgramError,
        program_pack::Pack,
        pubkey::Pubkey,
    },
    spl_associated_token_account_interface::instruction as associated_token_instruction,
    spl_token_interface::{instruction as token_instruction, state::Mint},
};

pub struct CreatePool;

impl CreatePool {
    pub fn process(program_id: &Pubkey, accounts: &[AccountInfo]) -> ProgramResult {
        let [
            amm_info,
            pool_info,
            pool_authority_info,
            mint_liquidity_info,
            mint_a_info,
            mint_b_info,
            pool_account_a_info,
            pool_account_b_info,
            payer_info,
            token_program_info,
            associated_token_program_info,
            system_program_info,
        ] = accounts else {
            return Err(ProgramError::NotEnoughAccountKeys);
        };

        if !payer_info.is_signer {
            return Err(ProgramError::MissingRequiredSignature);
        }

        if mint_a_info.key == mint_b_info.key {
            return Err(SwapError::InvalidMint.into());
        }

        // Validate AMM state
        let _amm = Amm::try_from_slice(&amm_info.data.borrow())
            .map_err(|_| ProgramError::InvalidAccountData)?;

        let (expected_amm_key, _) =
            Pubkey::find_program_address(&[_amm.id.as_ref()], program_id);
        if *amm_info.key != expected_amm_key {
            return Err(SwapError::AccountMismatch.into());
        }

        // 1. Verify Pool PDA
        let pool_seeds_prefix: &[&[u8]] = &[
            amm_info.key.as_ref(),
            mint_a_info.key.as_ref(),
            mint_b_info.key.as_ref(),
        ];
        let (expected_pool_key, pool_bump) =
            Pubkey::find_program_address(pool_seeds_prefix, program_id);
        if *pool_info.key != expected_pool_key {
            return Err(SwapError::AccountMismatch.into());
        }

        // 2. Verify Pool Authority PDA
        let authority_seeds_prefix: &[&[u8]] = &[
            amm_info.key.as_ref(),
            mint_a_info.key.as_ref(),
            mint_b_info.key.as_ref(),
            AUTHORITY_SEED,
        ];
        let (expected_authority_key, _authority_bump) =
            Pubkey::find_program_address(authority_seeds_prefix, program_id);
        if *pool_authority_info.key != expected_authority_key {
            return Err(SwapError::AccountMismatch.into());
        }

        // 3. Verify Liquidity Mint PDA
        let liquidity_seeds_prefix: &[&[u8]] = &[
            amm_info.key.as_ref(),
            mint_a_info.key.as_ref(),
            mint_b_info.key.as_ref(),
            LIQUIDITY_SEED,
        ];
        let (expected_liquidity_mint_key, liquidity_bump) =
            Pubkey::find_program_address(liquidity_seeds_prefix, program_id);
        if *mint_liquidity_info.key != expected_liquidity_mint_key {
            return Err(SwapError::AccountMismatch.into());
        }

        // Create Pool PDA Account
        let pool_signer_seeds: &[&[u8]] = &[
            amm_info.key.as_ref(),
            mint_a_info.key.as_ref(),
            mint_b_info.key.as_ref(),
            &[pool_bump],
        ];
        create_pda_account(
            payer_info,
            pool_info,
            system_program_info,
            program_id,
            Pool::LEN,
            pool_signer_seeds,
        )?;

        let pool = Pool {
            amm: *amm_info.key,
            mint_a: *mint_a_info.key,
            mint_b: *mint_b_info.key,
        };
        pool.serialize(&mut &mut pool_info.data.borrow_mut()[..])?;

        // Create and initialize Liquidity Mint account (decimals = 6)
        let liquidity_signer_seeds: &[&[u8]] = &[
            amm_info.key.as_ref(),
            mint_a_info.key.as_ref(),
            mint_b_info.key.as_ref(),
            LIQUIDITY_SEED,
            &[liquidity_bump],
        ];
        create_pda_account(
            payer_info,
            mint_liquidity_info,
            system_program_info,
            token_program_info.key,
            Mint::LEN,
            liquidity_signer_seeds,
        )?;

        let init_mint_ix = token_instruction::initialize_mint2(
            token_program_info.key,
            mint_liquidity_info.key,
            pool_authority_info.key,
            None,
            6,
        )?;
        invoke(
            &init_mint_ix,
            &[mint_liquidity_info.clone()],
        )?;

        // Create Pool ATAs for Token A and Token B owned by pool_authority
        let create_ata_a_ix = associated_token_instruction::create_associated_token_account_idempotent(
            payer_info.key,
            pool_authority_info.key,
            mint_a_info.key,
            token_program_info.key,
        );
        invoke(
            &create_ata_a_ix,
            &[
                payer_info.clone(),
                pool_account_a_info.clone(),
                pool_authority_info.clone(),
                mint_a_info.clone(),
                system_program_info.clone(),
                token_program_info.clone(),
                associated_token_program_info.clone(),
            ],
        )?;

        let create_ata_b_ix = associated_token_instruction::create_associated_token_account_idempotent(
            payer_info.key,
            pool_authority_info.key,
            mint_b_info.key,
            token_program_info.key,
        );
        invoke(
            &create_ata_b_ix,
            &[
                payer_info.clone(),
                pool_account_b_info.clone(),
                pool_authority_info.clone(),
                mint_b_info.clone(),
                system_program_info.clone(),
                token_program_info.clone(),
                associated_token_program_info.clone(),
            ],
        )?;

        Ok(())
    }
}
