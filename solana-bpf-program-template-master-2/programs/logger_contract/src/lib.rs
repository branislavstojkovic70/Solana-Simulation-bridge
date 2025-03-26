use solana_program::{
    account_info::{next_account_info, AccountInfo},
    entrypoint,
    entrypoint::ProgramResult,
    msg,
    program::{invoke_signed},
    program_error::ProgramError,
    program_pack::{Pack, Sealed},
    pubkey::Pubkey,
    system_instruction,
    sysvar::{rent::Rent, Sysvar},
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
        if src.len() < 8 {
            return Err(ProgramError::InvalidAccountData);
        }
        let sequence = u64::from_le_bytes(src[..8].try_into().unwrap());
        Ok(LoggerState { sequence })
    }

    fn pack_into_slice(&self, dst: &mut [u8]) {
        dst[..8].copy_from_slice(&self.sequence.to_le_bytes());
    }
}

#[repr(C)]
#[derive(Clone, Debug, Default, PartialEq)]
pub struct MessageData {
    pub from_pubkey: Pubkey,
    pub to_pubkey: Pubkey,
    pub amount: u64,
    pub timestamp: u64,
    pub sequence: u64,
}

impl Sealed for MessageData {}
impl Pack for MessageData {
    const LEN: usize = 32 + 32 + 8 + 8 + 8;

    fn unpack_from_slice(src: &[u8]) -> Result<Self, ProgramError> {
        if src.len() < Self::LEN {
            return Err(ProgramError::InvalidAccountData);
        }
        let from_pubkey = Pubkey::new(&src[0..32]);
        let to_pubkey = Pubkey::new(&src[32..64]);
        let amount = u64::from_le_bytes(src[64..72].try_into().unwrap());
        let timestamp = u64::from_le_bytes(src[72..80].try_into().unwrap());
        let sequence = u64::from_le_bytes(src[80..88].try_into().unwrap());

        Ok(Self {
            from_pubkey,
            to_pubkey,
            amount,
            timestamp,
            sequence,
        })
    }

    fn pack_into_slice(&self, dst: &mut [u8]) {
        dst[0..32].copy_from_slice(self.from_pubkey.as_ref());
        dst[32..64].copy_from_slice(self.to_pubkey.as_ref());
        dst[64..72].copy_from_slice(&self.amount.to_le_bytes());
        dst[72..80].copy_from_slice(&self.timestamp.to_le_bytes());
        dst[80..88].copy_from_slice(&self.sequence.to_le_bytes());
    }
}

entrypoint!(process_instruction);

pub fn process_instruction(
    program_id: &Pubkey,
    accounts: &[AccountInfo],
    instruction_data: &[u8],
) -> ProgramResult {
    if instruction_data.len() != 80 {
        msg!("Logger: Invalid instruction data length, expected 80 bytes.");
        return Err(ProgramError::InvalidInstructionData);
    }

    let accounts_iter = &mut accounts.iter();
    let state_account = next_account_info(accounts_iter)?;
    let message_pda_account = next_account_info(accounts_iter)?;
    let payer_account = next_account_info(accounts_iter)?;
    let system_program_account = next_account_info(accounts_iter)?;

    if !state_account.is_writable || !message_pda_account.is_writable {
        msg!("Logger: One of the accounts is not writable.");
        return Err(ProgramError::InvalidAccountData);
    }

    let mut state_data = state_account.data.borrow_mut();
    let mut logger_state = LoggerState::unpack_from_slice(&state_data)?;
    logger_state.sequence += 1;
    LoggerState::pack(logger_state, &mut state_data)?;

    let from_pubkey = Pubkey::new(&instruction_data[0..32]);
    let to_pubkey = Pubkey::new(&instruction_data[32..64]);
    let amount = u64::from_le_bytes(instruction_data[64..72].try_into().unwrap());
    let timestamp = u64::from_le_bytes(instruction_data[72..80].try_into().unwrap());

    let (expected_pda, bump) = Pubkey::find_program_address(
        &[b"logger", &logger_state.sequence.to_le_bytes()],
        program_id,
    );

    if &expected_pda != message_pda_account.key {
        msg!("Logger: Incorrect PDA address provided.");
        return Err(ProgramError::InvalidArgument);
    }

    let space = MessageData::LEN;
    let rent_lamports = Rent::get()?.minimum_balance(space);

    if message_pda_account.lamports() == 0 {
        invoke_signed(
            &system_instruction::create_account(
                payer_account.key,
                message_pda_account.key,
                rent_lamports,
                space as u64,
                program_id,
            ),
            &[
                payer_account.clone(),
                message_pda_account.clone(),
                system_program_account.clone(),
            ],
            &[&[b"logger", &logger_state.sequence.to_le_bytes(), &[bump]]],
        )?;
    }

    let message_data = MessageData {
        from_pubkey,
        to_pubkey,
        amount,
        timestamp,
        sequence: logger_state.sequence,
    };

    let mut pda_data = message_pda_account.data.borrow_mut();
    MessageData::pack(message_data, &mut pda_data)?;

    msg!(
        "--------------------------------\n\
         FROM:      {}\n\
         TO:        {}\n\
         AMOUNT:    {}\n\
         TIMESTAMP: {}\n\
         SEQUENCE:  {}\n\
         --------------------------------",
        from_pubkey,
        to_pubkey,
        amount,
        timestamp,
        logger_state.sequence
    );
    

    Ok(())
}
