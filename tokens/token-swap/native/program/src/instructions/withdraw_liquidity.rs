use {
    crate::{
        constants::{AUTHORITY_SEED, MINIMUM_LIQUIDITY},
        error::SwapError,
        state::Pool,
        utils::{assert_is_associated_token_account, token_burn, token_transfer, unpack_token_account},
    },
    borsh::{BorshDeserialize, BorshSerialize},
    solana_program::{
        account_info::AccountInfo,
        entrypoint::ProgramResult,
        program_error::ProgramError,
        program_pack::Pack,
        pubkey::Pubkey,
    },
    spl_token_interface::state::Mint,
};

#[derive(BorshSerialize, BorshDeserialize, Debug, Clone)]
pub struct WithdrawLiquidityArgs {
    pub amount: u64,
}

pub struct WithdrawLiquidity;

impl WithdrawLiquidity {
    pub fn process(
        program_id: &Pubkey,
        accounts: &[AccountInfo],
        args: WithdrawLiquidityArgs,
    ) -> ProgramResult {
        let [
            amm_info,
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
            token_program_info,
        ] = accounts else {
            return Err(ProgramError::NotEnoughAccountKeys);
        };

        if !depositor_info.is_signer {
            return Err(ProgramError::MissingRequiredSignature);
        }

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

        let mint_liquidity = Mint::unpack(&mint_liquidity_info.data.borrow())
            .map_err(|_| ProgramError::InvalidAccountData)?;

        let pool_token_a = unpack_token_account(pool_account_a_info)?;
        let pool_token_b = unpack_token_account(pool_account_b_info)?;

        let total_supply = (mint_liquidity.supply as u128)
            .checked_add(MINIMUM_LIQUIDITY as u128)
            .ok_or(SwapError::MathOverflow)?;

        let amount_u128 = args.amount as u128;

        let amount_a = (amount_u128)
            .checked_mul(pool_token_a.amount as u128)
            .ok_or(SwapError::MathOverflow)?
            .checked_div(total_supply)
            .ok_or(SwapError::MathOverflow)? as u64;

        let amount_b = (amount_u128)
            .checked_mul(pool_token_b.amount as u128)
            .ok_or(SwapError::MathOverflow)?
            .checked_div(total_supply)
            .ok_or(SwapError::MathOverflow)? as u64;

        let authority_signer_seeds: &[&[u8]] = &[
            pool.amm.as_ref(),
            mint_a_info.key.as_ref(),
            mint_b_info.key.as_ref(),
            AUTHORITY_SEED,
            &[authority_bump],
        ];

        // Transfer Token A to depositor
        token_transfer(
            token_program_info,
            pool_account_a_info,
            depositor_account_a_info,
            pool_authority_info,
            amount_a,
            Some(authority_signer_seeds),
        )?;

        // Transfer Token B to depositor
        token_transfer(
            token_program_info,
            pool_account_b_info,
            depositor_account_b_info,
            pool_authority_info,
            amount_b,
            Some(authority_signer_seeds),
        )?;

        // Burn liquidity LP tokens from depositor
        token_burn(
            token_program_info,
            mint_liquidity_info,
            depositor_account_liquidity_info,
            depositor_info,
            args.amount,
        )?;

        Ok(())
    }
}
