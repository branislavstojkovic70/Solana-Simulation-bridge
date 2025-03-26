use thiserror::Error;
use solana_program::program_error::ProgramError;

#[derive(Error, Debug, Copy, Clone)]
pub enum EscrowError {
    #[error("Invalid Instruction")]
    InvalidInstruction,
    #[error("Not Rent Exempt")]
    NotRentExempt,
    #[error("Escrow Account Already Initialized")]
    AlreadyInitialized,
    #[error("Mint Mismatch")]
    MintMismatch,
    #[error("Amount Overflow")]
    AmountOverflow,
    #[error("Insufficient Amount")]
    InsufficientAmount,
}

impl From<EscrowError> for ProgramError {
    fn from(e: EscrowError) -> Self {
        ProgramError::Custom(e as u32)
    }
}
