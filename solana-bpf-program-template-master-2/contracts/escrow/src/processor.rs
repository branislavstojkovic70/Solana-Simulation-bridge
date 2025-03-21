use solana_program::{
    account_info::{next_account_info, AccountInfo},
    entrypoint::ProgramResult,
    msg,
    instruction::{Instruction},
    program::{invoke, invoke_signed},
    program_error::ProgramError,
    program_pack::{IsInitialized, Pack},
    pubkey::Pubkey,
    sysvar::{rent::Rent, Sysvar},
};
use spl_token::state::Account as TokenAccount;
use crate::{error::EscrowError, instruction::EscrowInstruction, state::Escrow};
use std::str::FromStr;
use solana_program::sysvar::{clock::Clock};
use solana_program::instruction::AccountMeta;

pub struct EscrowProcessor;
impl EscrowProcessor {
    pub fn process(
        program_id: &Pubkey,
        accounts: &[AccountInfo],
        instruction_data: &[u8],
    ) -> ProgramResult {
        let instruction = EscrowInstruction::unpack(instruction_data)?;

        match instruction {
            EscrowInstruction::InitEscrow { amount } => {
                msg!("Instruction: InitEscrow");
                Self::process_init_escrow(accounts, amount, program_id)
            }
            EscrowInstruction::Exchange { amount } => {
                msg!("Instruction: Exchange");
                Self::process_exchange(accounts, amount, program_id)
            }
        }
    }
    
    pub fn process_init_escrow(
        accounts: &[AccountInfo],
        amount: u64,
        program_id: &Pubkey,
    ) -> ProgramResult {
        let account_info_iter = &mut accounts.iter();
    
        // 1) initializer (signer)
        let initializer = next_account_info(account_info_iter)?;
        msg!("Checking if the initializer is a signer...");
        if !initializer.is_signer {
            msg!("Error: Initializer is not a signer.");
            return Err(ProgramError::MissingRequiredSignature);
        }
    
        // 2) temp_token_account
        let temp_token_account = next_account_info(account_info_iter)?;
        msg!("Temporary token account: {}", temp_token_account.key);
    
        // 3) token_to_receive_account
        let token_to_receive_account = next_account_info(account_info_iter)?;
        msg!("Token to receive account: {}", token_to_receive_account.key);
        msg!("Token to receive account owner: {}", token_to_receive_account.owner);
    
        if *token_to_receive_account.owner != spl_token::id() {
            msg!("Error: Token to receive account is not owned by SPL Token program.");
            return Err(ProgramError::IncorrectProgramId);
        }
    
        // 4) escrow_account
        let escrow_account = next_account_info(account_info_iter)?;
        msg!("Escrow account: {}", escrow_account.key);
    
        // 5) rent sysvar
        let rent_info = next_account_info(account_info_iter)?;
        let rent = &Rent::from_account_info(rent_info)?;
        msg!("Checking rent exemption...");
        if !rent.is_exempt(escrow_account.lamports(), escrow_account.data_len()) {
            msg!("Error: Escrow account is not rent exempt.");
            return Err(EscrowError::NotRentExempt.into());
        }
    
        // Unpack or init the escrow data
        let mut escrow_info = Escrow::unpack_unchecked(&escrow_account.data.borrow())?;
        if escrow_info.is_initialized() {
            msg!("Error: Escrow account is already initialized.");
            return Err(ProgramError::AccountAlreadyInitialized);
        }
        msg!("Initializing escrow account...");
        escrow_info.is_initialized = true;
        escrow_info.initializer_pubkey = *initializer.key;
        escrow_info.temp_token_account_pubkey = *temp_token_account.key;
        escrow_info.initializer_token_to_receive_account_pubkey = *token_to_receive_account.key;
        escrow_info.expected_amount = amount;
    
        msg!("Packing escrow data into account...");
        Escrow::pack(escrow_info, &mut escrow_account.data.borrow_mut())?;
    
        // Derive PDA
        let (pda, _nonce) = Pubkey::find_program_address(&[b"escrow"], program_id);
        msg!("Derived PDA: {}", pda);
    
        // 6) token_program
        let token_program = next_account_info(account_info_iter)?;
        msg!("Token program account: {}", token_program.key);
    
        // Transfer authority of temp_token_account to the PDA
        let owner_change_ix = spl_token::instruction::set_authority(
            token_program.key,
            temp_token_account.key,
            Some(&pda),
            spl_token::instruction::AuthorityType::AccountOwner,
            initializer.key,
            &[&initializer.key],
        )?;
        msg!("Calling the token program to transfer token account ownership...");
        invoke(
            &owner_change_ix,
            &[
                temp_token_account.clone(),
                initializer.clone(),
                token_program.clone(),
            ],
        )?;
        msg!("Token account ownership transferred successfully.");
    
        // 7) logger program
        let logger_program = next_account_info(account_info_iter)?;
        msg!("Logger program account: {}", logger_program.key);
        
        // 8) Logger state account
        let logger_state_account = next_account_info(account_info_iter)?;
        msg!("Logger state account: {}", logger_state_account.key);
        
        if *logger_state_account.owner != *logger_program.key {
            msg!("Error: Logger state account is not owned by Logger program.");
            return Err(ProgramError::IncorrectProgramId);
        }
        
        let clock = Clock::get()?;
        let timestamp = clock.unix_timestamp as u64;

        let mut logger_data = vec![0u8; 80];
        logger_data[..32].copy_from_slice(initializer.key.as_ref());
        logger_data[32..64].copy_from_slice(token_to_receive_account.key.as_ref());
        logger_data[64..72].copy_from_slice(&amount.to_le_bytes());
        logger_data[72..80].copy_from_slice(&timestamp.to_le_bytes());
        
        let logger_ix = Instruction {
            program_id: *logger_program.key,
            accounts: vec![
                AccountMeta::new(*logger_state_account.key, false), 
            ],
            data: logger_data,
        };
        
        msg!("Invoking the logger program with extended data (80 bytes)...");
        invoke(
            &logger_ix,
            &[
                logger_program.clone(),
                logger_state_account.clone(), 
            ],
        )?;
        msg!("Logger contract invoked successfully.");
        
        Ok(())
    }

    

    fn process_exchange(
        accounts: &[AccountInfo],
        amount_expected_by_taker: u64,
        program_id: &Pubkey,
    ) -> ProgramResult {
        let account_info_iter = &mut accounts.iter();
        let taker = next_account_info(account_info_iter)?;

        if !taker.is_signer {
            return Err(ProgramError::MissingRequiredSignature);
        }
        let takers_sending_token_account = next_account_info(account_info_iter)?;

        let takers_token_to_receive_account = next_account_info(account_info_iter)?;

        let pdas_temp_token_account = next_account_info(account_info_iter)?;
        let pdas_temp_token_account_info =
            TokenAccount::unpack(&pdas_temp_token_account.data.borrow())?;
        let (pda, nonce) = Pubkey::find_program_address(&[b"escrow"], program_id);

        if amount_expected_by_taker != pdas_temp_token_account_info.amount {
            return Err(EscrowError::ExpectedAmountMismatch.into());
        }
        let initializers_main_account = next_account_info(account_info_iter)?;
        let initializers_token_to_receive_account = next_account_info(account_info_iter)?;
        let escrow_account = next_account_info(account_info_iter)?;

        let escrow_info = Escrow::unpack(&escrow_account.data.borrow())?;

        if escrow_info.temp_token_account_pubkey != *pdas_temp_token_account.key {
            return Err(ProgramError::InvalidAccountData);
        }
        if escrow_info.initializer_pubkey != *initializers_main_account.key {
            return Err(ProgramError::InvalidAccountData);
        }
        if escrow_info.initializer_token_to_receive_account_pubkey
            != *initializers_token_to_receive_account.key
        {
            return Err(ProgramError::InvalidAccountData);
        }

        let token_program = next_account_info(account_info_iter)?;

        let transfer_to_initializer_ix = spl_token::instruction::transfer(
            token_program.key,
            takers_sending_token_account.key,
            initializers_token_to_receive_account.key,
            taker.key,
            &[&taker.key],
            escrow_info.expected_amount,
        )?;
        msg!("Calling the token program to transfer tokens to the escrow's initializer...");
        invoke(
            &transfer_to_initializer_ix,
            &[
                takers_sending_token_account.clone(),
                initializers_token_to_receive_account.clone(),
                taker.clone(),
                token_program.clone(),
            ],
        )?;

        let pda_account = next_account_info(account_info_iter)?;

        let transfer_to_taker_ix = spl_token::instruction::transfer(
            token_program.key,
            pdas_temp_token_account.key,
            takers_token_to_receive_account.key,
            &pda,
            &[&pda],
            pdas_temp_token_account_info.amount,
        )?;
        msg!("Calling the token program to transfer tokens to the taker...");
        invoke_signed(
            &transfer_to_taker_ix,
            &[
                pdas_temp_token_account.clone(),
                takers_token_to_receive_account.clone(),
                pda_account.clone(),
                token_program.clone(),
            ],
            &[&[&b"escrow"[..], &[nonce]]],
        )?;

        let close_pdas_temp_acc_ix = spl_token::instruction::close_account(
            token_program.key,
            pdas_temp_token_account.key,
            initializers_main_account.key,
            &pda,
            &[&pda],
        )?;
        msg!("Calling the token program to close pda's temp account...");
        invoke_signed(
            &close_pdas_temp_acc_ix,
            &[
                pdas_temp_token_account.clone(),
                initializers_main_account.clone(),
                pda_account.clone(),
                token_program.clone(),
            ],
            &[&[&b"escrow"[..], &[nonce]]],
        )?;

        msg!("Closing the escrow account...");
        **initializers_main_account.lamports.borrow_mut() = initializers_main_account
            .lamports()
            .checked_add(escrow_account.lamports())
            .ok_or(EscrowError::AmountOverflow)?;
        **escrow_account.lamports.borrow_mut() = 0;

        Ok(())
    }
}