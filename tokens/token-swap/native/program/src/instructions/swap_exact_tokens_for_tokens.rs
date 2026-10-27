use {
    crate::{
        constants::AUTHORITY_SEED,
        error::SwapError,
        state::{Amm, Pool},
        utils::{assert_is_associated_token_account, token_transfer, unpack_token_account},
    },
    borsh::{BorshDeserialize, BorshSerialize},
    solana_program::{
        account_info::AccountInfo,
        entrypoint::ProgramResult,
        msg,
        program_error::ProgramError,
        pubkey::Pubkey,
    },
};

#[derive(BorshSerialize, BorshDeserialize, Debug, Clone)]
pub struct SwapExactTokensForTokensArgs {
    pub swap_a: bool,
    pub input_amount: u64,
    pub min_output_amount: u64,
}

pub struct SwapExactTokensForTokens;

impl SwapExactTokensForTokens {
    pub fn process(
        program_id: &Pubkey,
        accounts: &[AccountInfo],
        args: SwapExactTokensForTokensArgs,
    ) -> ProgramResult {
        let [
            amm_info,
            pool_info,
            pool_authority_info,
            trader_info,
            mint_a_info,
            mint_b_info,
            pool_account_a_info,
            pool_account_b_info,
            trader_account_a_info,
            trader_account_b_info,
            _payer_info,
            token_program_info,
            _associated_token_program_info,
            _system_program_info,
        ] = accounts else {
            return Err(ProgramError::NotEnoughAccountKeys);
        };

        if !trader_info.is_signer {
            return Err(ProgramError::MissingRequiredSignature);
        }

        let amm = Amm::try_from_slice(&amm_info.data.borrow())
            .map_err(|_| ProgramError::InvalidAccountData)?;
        let pool = Pool::try_from_slice(&pool_info.data.borrow())
            .map_err(|_| ProgramError::InvalidAccountData)?;

        if pool.amm != *amm_info.key {
            return Err(SwapError::AccountMismatch.into());
        }
        if pool.mint_a != *mint_a_info.key || pool.mint_b != *mint_b_info.key {
            return Err(SwapError::InvalidMint.into());
        }

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

        assert_is_associated_token_account(pool_account_a_info.key, pool_authority_info.key, mint_a_info.key)?;
        assert_is_associated_token_account(pool_account_b_info.key, pool_authority_info.key, mint_b_info.key)?;

        let trader_token_a = unpack_token_account(trader_account_a_info)?;
        let trader_token_b = unpack_token_account(trader_account_b_info)?;

        let input = if args.swap_a {
            args.input_amount.min(trader_token_a.amount)
        } else {
            args.input_amount.min(trader_token_b.amount)
        };

        let fee_amount = ((input as u128)
            .checked_mul(amm.fee as u128)
            .ok_or(SwapError::MathOverflow)?
            .checked_div(10_000)
            .ok_or(SwapError::MathOverflow)?) as u64;

        let taxed_input = input
            .checked_sub(fee_amount)
            .ok_or(SwapError::MathOverflow)?;

        let pool_token_a = unpack_token_account(pool_account_a_info)?;
        let pool_token_b = unpack_token_account(pool_account_b_info)?;

        let pool_a_amount = pool_token_a.amount as u128;
        let pool_b_amount = pool_token_b.amount as u128;
        let invariant = pool_a_amount
            .checked_mul(pool_b_amount)
            .ok_or(SwapError::MathOverflow)?;

        let output_u128 = if args.swap_a {
            (pool_b_amount)
                .checked_mul(taxed_input as u128)
                .ok_or(SwapError::MathOverflow)?
                .checked_div(pool_a_amount + (taxed_input as u128))
                .ok_or(SwapError::MathOverflow)?
        } else {
            (pool_a_amount)
                .checked_mul(taxed_input as u128)
                .ok_or(SwapError::MathOverflow)?
                .checked_div(pool_b_amount + (taxed_input as u128))
                .ok_or(SwapError::MathOverflow)?
        };

        let output = output_u128 as u64;

        if output < args.min_output_amount {
            return Err(SwapError::OutputTooSmall.into());
        }

        let authority_signer_seeds: &[&[u8]] = &[
            pool.amm.as_ref(),
            mint_a_info.key.as_ref(),
            mint_b_info.key.as_ref(),
            AUTHORITY_SEED,
            &[authority_bump],
        ];

        if args.swap_a {
            // Transfer output of Token B from pool to trader
            token_transfer(
                token_program_info,
                pool_account_b_info,
                trader_account_b_info,
                pool_authority_info,
                output,
                Some(authority_signer_seeds),
            )?;

            // Transfer input of Token A from trader to pool
            token_transfer(
                token_program_info,
                trader_account_a_info,
                pool_account_a_info,
                trader_info,
                input,
                None,
            )?;
        } else {
            // Transfer output of Token A from pool to trader
            token_transfer(
                token_program_info,
                pool_account_a_info,
                trader_account_a_info,
                pool_authority_info,
                output,
                Some(authority_signer_seeds),
            )?;

            // Transfer input of Token B from trader to pool
            token_transfer(
                token_program_info,
                trader_account_b_info,
                pool_account_b_info,
                trader_info,
                input,
                None,
            )?;
        }

        msg!("Swap executed: {} in (taxed: {}) -> {} out", input, taxed_input, output);

        // Verify invariant preservation
        let updated_pool_a = unpack_token_account(pool_account_a_info)?;
        let updated_pool_b = unpack_token_account(pool_account_b_info)?;
        let new_invariant = (updated_pool_a.amount as u128)
            .checked_mul(updated_pool_b.amount as u128)
            .ok_or(SwapError::MathOverflow)?;

        if invariant > new_invariant {
            return Err(SwapError::InvariantViolated.into());
        }

        Ok(())
    }
}
