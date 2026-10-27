pub mod constants;
pub mod error;
pub mod instructions;
pub mod state;
pub mod utils;

use {
    borsh::{BorshDeserialize, BorshSerialize},
    instructions::*,
    solana_program::{
        account_info::AccountInfo,
        entrypoint,
        entrypoint::ProgramResult,
        pubkey::Pubkey,
    },
};

entrypoint!(process_instruction);

pub fn process_instruction(
    program_id: &Pubkey,
    accounts: &[AccountInfo],
    instruction_data: &[u8],
) -> ProgramResult {
    let instruction = TokenSwapInstruction::try_from_slice(instruction_data)?;

    match instruction {
        TokenSwapInstruction::CreateAmm(args) => {
            CreateAmm::process(program_id, accounts, args)
        }
        TokenSwapInstruction::CreatePool => {
            CreatePool::process(program_id, accounts)
        }
        TokenSwapInstruction::DepositLiquidity(args) => {
            DepositLiquidity::process(program_id, accounts, args)
        }
        TokenSwapInstruction::SwapExactTokensForTokens(args) => {
            SwapExactTokensForTokens::process(program_id, accounts, args)
        }
        TokenSwapInstruction::WithdrawLiquidity(args) => {
            WithdrawLiquidity::process(program_id, accounts, args)
        }
    }
}

#[derive(BorshSerialize, BorshDeserialize, Debug, Clone)]
pub enum TokenSwapInstruction {
    CreateAmm(CreateAmmArgs),
    CreatePool,
    DepositLiquidity(DepositLiquidityArgs),
    SwapExactTokensForTokens(SwapExactTokensForTokensArgs),
    WithdrawLiquidity(WithdrawLiquidityArgs),
}
