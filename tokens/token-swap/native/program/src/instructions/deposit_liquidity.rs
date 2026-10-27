use {
    crate::{
        constants::{AUTHORITY_SEED, LIQUIDITY_SEED, MINIMUM_LIQUIDITY},
        error::SwapError,
        state::Pool,
        utils::{assert_is_associated_token_account, token_mint_to, token_transfer, unpack_token_account},
    },
    borsh::{BorshDeserialize, BorshSerialize},
    solana_program::{
        account_info::AccountInfo,
        entrypoint::ProgramResult,
        program::invoke,
        program_error::ProgramError,
        pubkey::Pubkey,
    },
    spl_associated_token_account_interface::instruction as associated_token_instruction,
};

#[derive(BorshSerialize, BorshDeserialize, Debug, Clone)]
pub struct DepositLiquidityArgs {
    pub amount_a: u64,
    pub amount_b: u64,
}

pub struct DepositLiquidity;

impl DepositLiquidity {
    pub fn process(
        program_id: &Pubkey,
        accounts: &[AccountInfo],
        args: DepositLiquidityArgs,
    ) -> ProgramResult {
        let [
            pool_info,
            pool_authority_info,
            depositor_info,
            mint_liquidity_info,
            mint_a_info,
            mint_b_info,
            pool_account_a_info,
            pool_account_b_info,
            depositor_account_liquidity_info,
            depositor_account_a_info,
            depositor_account_b_info,
            payer_info,
            token_program_info,
            associated_token_program_info,
            system_program_info,
        ] = accounts else {
            return Err(ProgramError::NotEnoughAccountKeys);
        };

        if !depositor_info.is_signer {
            return Err(ProgramError::MissingRequiredSignature);
        }

        let pool = Pool::try_from_slice(&pool_info.data.borrow())
            .map_err(|_| ProgramError::InvalidAccountData)?;

        if pool.mint_a != *mint_a_info.key || pool.mint_b != *mint_b_info.key {
            return Err(SwapError::InvalidMint.into());
        }

        // Validate PDAs
        let authority_seeds_prefix: &[&[u8]] = &[
            pool.amm.as_ref(),
            mint_a_info.key.as_ref(),
            mint_b_info.key.as_ref(),
            AUTHORITY_SEED,
        ];
        let (expected_authority_key, authority_bump) =
            Pubkey::find_program_address(authority_seeds_prefix, program_id);
        if *pool_authority_info.key != expected_authority_key {
            return Err(SwapError::AccountMismatch.into());
        }

        let liquidity_seeds_prefix: &[&[u8]] = &[
            pool.amm.as_ref(),
            mint_a_info.key.as_ref(),
            mint_b_info.key.as_ref(),
            LIQUIDITY_SEED,
        ];
        let (expected_liquidity_mint_key, _) =
            Pubkey::find_program_address(liquidity_seeds_prefix, program_id);
        if *mint_liquidity_info.key != expected_liquidity_mint_key {
            return Err(SwapError::AccountMismatch.into());
        }

        assert_is_associated_token_account(pool_account_a_info.key, pool_authority_info.key, mint_a_info.key)?;
        assert_is_associated_token_account(pool_account_b_info.key, pool_authority_info.key, mint_b_info.key)?;

        let depositor_token_a = unpack_token_account(depositor_account_a_info)?;
        let depositor_token_b = unpack_token_account(depositor_account_b_info)?;

        let amount_a = args.amount_a.min(depositor_token_a.amount);
        let amount_b = args.amount_b.min(depositor_token_b.amount);

        let pool_token_a = unpack_token_account(pool_account_a_info)?;
        let pool_token_b = unpack_token_account(pool_account_b_info)?;

        let pool_creation = pool_token_a.amount == 0 && pool_token_b.amount == 0;

        let (final_amount_a, final_amount_b) = if pool_creation {
            (amount_a, amount_b)
        } else {
            let amount_a_u128 = amount_a as u128;
            let amount_b_u128 = amount_b as u128;
            let pool_a_u128 = pool_token_a.amount as u128;
            let pool_b_u128 = pool_token_b.amount as u128;

            let amount_b_required = amount_a_u128
                .checked_mul(pool_b_u128)
                .ok_or(SwapError::MathOverflow)?
                .checked_div(pool_a_u128)
                .ok_or(SwapError::MathOverflow)?;

            if amount_b_required <= amount_b_u128 {
                (amount_a, amount_b_required as u64)
            } else {
                let amount_a_required = amount_b_u128
                    .checked_mul(pool_a_u128)
                    .ok_or(SwapError::MathOverflow)?
                    .checked_div(pool_b_u128)
                    .ok_or(SwapError::MathOverflow)?;
                (amount_a_required as u64, amount_b)
            }
        };

        let mut liquidity = ((final_amount_a as u128)
            .checked_mul(final_amount_b as u128)
            .ok_or(SwapError::MathOverflow)?)
            .isqrt() as u64;

        if pool_creation {
            if liquidity < MINIMUM_LIQUIDITY {
                return Err(SwapError::DepositTooSmall.into());
            }
            liquidity = liquidity
                .checked_sub(MINIMUM_LIQUIDITY)
                .ok_or(SwapError::MathOverflow)?;
        }

        // Transfer Token A from depositor to pool
        token_transfer(
            token_program_info,
            depositor_account_a_info,
            pool_account_a_info,
            depositor_info,
            final_amount_a,
            None,
        )?;

        // Transfer Token B from depositor to pool
        token_transfer(
            token_program_info,
            depositor_account_b_info,
            pool_account_b_info,
            depositor_info,
            final_amount_b,
            None,
        )?;

        // Create depositor liquidity ATA if it does not exist
        let create_ata_ix = associated_token_instruction::create_associated_token_account_idempotent(
            payer_info.key,
            depositor_info.key,
            mint_liquidity_info.key,
            token_program_info.key,
        );
        invoke(
            &create_ata_ix,
            &[
                payer_info.clone(),
                depositor_account_liquidity_info.clone(),
                depositor_info.clone(),
                mint_liquidity_info.clone(),
                system_program_info.clone(),
                token_program_info.clone(),
                associated_token_program_info.clone(),
            ],
        )?;

        // Mint liquidity tokens to depositor using pool_authority PDA signer seeds
        let authority_signer_seeds: &[&[u8]] = &[
            pool.amm.as_ref(),
            mint_a_info.key.as_ref(),
            mint_b_info.key.as_ref(),
            AUTHORITY_SEED,
            &[authority_bump],
        ];

        token_mint_to(
            token_program_info,
            mint_liquidity_info,
            depositor_account_liquidity_info,
            pool_authority_info,
            liquidity,
            authority_signer_seeds,
        )?;

        Ok(())
    }
}
