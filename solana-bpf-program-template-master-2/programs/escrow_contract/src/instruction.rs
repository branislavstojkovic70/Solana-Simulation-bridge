use solana_program::program_error::ProgramError;
use crate::error::EscrowError::InvalidInstruction;
use std::convert::TryInto;

pub enum EscrowInstruction {
    Deposit {
        amount: u64,
    },
    Withdraw {
        amount: u64,
    },
}

impl EscrowInstruction {
    pub fn unpack(input: &[u8]) -> Result<Self, ProgramError> {
        let (tag, rest) = input.split_first().ok_or(InvalidInstruction)?;
        Ok(match tag {
            0 => {
                let amount = Self::unpack_amount(rest)?;
                EscrowInstruction::Deposit { amount }
            },
            1 => {
                let amount = Self::unpack_amount(rest)?;
                EscrowInstruction::Withdraw { amount }
            },
            _ => return Err(InvalidInstruction.into()),
        })
    }

    fn unpack_amount(input: &[u8]) -> Result<u64, ProgramError> {
        if input.len() < 8 {
            return Err(InvalidInstruction.into());
        }
        let amount = u64::from_le_bytes(input[..8].try_into().unwrap());
        Ok(amount)
    }
}
