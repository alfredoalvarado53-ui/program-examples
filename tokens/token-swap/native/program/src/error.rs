use solana_program::program_error::ProgramError;
use thiserror::Error;

#[derive(Debug, Error)]
pub enum SwapError {
    #[error("Invalid fee value (must be strictly less than 10,000 basis points)")]
    InvalidFee,

    #[error("Invalid mint for the pool")]
    InvalidMint,

    #[error("Depositing too little liquidity (below MINIMUM_LIQUIDITY threshold)")]
    DepositTooSmall,

    #[error("Output is below the minimum expected amount")]
    OutputTooSmall,

    #[error("Invariant does not hold after swap execution")]
    InvariantViolated,

    #[error("Math overflow occurred during calculation")]
    MathOverflow,

    #[error("Provided account does not match expected PDA or configuration")]
    AccountMismatch,

    #[error("Token account owner or mint does not match expected authority")]
    TokenAccountMismatch,
}

impl From<SwapError> for ProgramError {
    fn from(e: SwapError) -> Self {
        ProgramError::Custom(e as u32)
    }
}
