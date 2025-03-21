use solana_program::{
    account_info::{next_account_info, AccountInfo},
    entrypoint,
    entrypoint::ProgramResult,
    msg,
    program_error::ProgramError,
    pubkey::Pubkey,
    program_pack::{Pack, Sealed},
};

#[repr(C)]
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct LoggerState {
    pub sequence: u64,
}

impl Sealed for LoggerState {}
impl Pack for LoggerState {
    const LEN: usize = 8;

    fn unpack_from_slice(src: &[u8]) -> Result<Self, ProgramError> {
        if src.len() < Self::LEN {
            return Err(ProgramError::InvalidAccountData);
        }
        let sequence = u64::from_le_bytes(src[..8].try_into().unwrap());
        Ok(LoggerState { sequence })
    }

    fn pack_into_slice(&self, dst: &mut [u8]) {
        dst[..8].copy_from_slice(&self.sequence.to_le_bytes());
    }
}

entrypoint!(process_instruction);

pub fn process_instruction(
    _program_id: &Pubkey,
    accounts: &[AccountInfo], 
    instruction_data: &[u8],
) -> ProgramResult {
    if instruction_data.len() != 80 {
        msg!("Logger: Invalid instruction data length, expected 80 bytes.");
        return Err(ProgramError::InvalidInstructionData);
    }

    let accounts_iter = &mut accounts.iter();
    let state_account = next_account_info(accounts_iter)?;

    if !state_account.is_writable {
        msg!("Logger: State account is not writable!");
        return Err(ProgramError::InvalidAccountData);
    }

    let mut state_data = state_account.data.borrow_mut();
    let mut logger_state = LoggerState::unpack_from_slice(&state_data)?;

    msg!("Logger: Current sequence: {}", logger_state.sequence);

    logger_state.sequence += 1;

    msg!("Logger: New sequence: {}", logger_state.sequence);

    LoggerState::pack(logger_state, &mut state_data)?;

    let from_pubkey = Pubkey::new(&instruction_data[0..32]);
    let to_pubkey = Pubkey::new(&instruction_data[32..64]);
    let amount = u64::from_le_bytes(instruction_data[64..72].try_into().unwrap());
    let timestamp = u64::from_le_bytes(instruction_data[72..80].try_into().unwrap());

    msg!("--------------------------------");
    msg!("FROM:      {}", from_pubkey);
    msg!("TO:        {}", to_pubkey);
    msg!("AMOUNT:    {}", amount);
    msg!("TIMESTAMP: {}", timestamp);
    msg!("SEQUENCE:  {}", logger_state.sequence);
    msg!("--------------------------------");

    Ok(())
}
