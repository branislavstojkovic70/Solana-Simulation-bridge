use solana_program::{
    account_info::{next_account_info, AccountInfo},
    entrypoint::ProgramResult,
    instruction::Instruction,
    msg,
    instruction::AccountMeta,
    program::{invoke, invoke_signed},
    program_error::ProgramError,
    sysvar::clock::Clock,
    program_pack::Pack,
    pubkey::Pubkey,
    system_instruction,
    sysvar::{rent::Rent, Sysvar},
};
use spl_token::state::Account as TokenAccount;
use crate::{error::EscrowError, instruction::EscrowInstruction, state::EscrowState};


pub struct EscrowProcessor;
impl EscrowProcessor {
    pub fn process(
        program_id: &Pubkey,
        accounts: &[AccountInfo],
        instruction_data: &[u8],
    ) -> ProgramResult {
        let instruction = EscrowInstruction::unpack(instruction_data)?;
        match instruction {
            EscrowInstruction::Deposit { amount } => {
                msg!("Instruction: Deposit {}", amount);
                Self::process_deposit(accounts, amount, program_id)
            }
            EscrowInstruction::Withdraw { amount } => {
                msg!("Instruction: Withdraw {}", amount);
                Self::process_withdraw(accounts, amount, program_id)
            }
        }
    }
    fn process_deposit(
        accounts: &[AccountInfo],
        amount: u64,
        program_id: &Pubkey,
    ) -> ProgramResult {
        let acc_iter = &mut accounts.iter();

        let user_signer = next_account_info(acc_iter)?;
        if !user_signer.is_signer {
            return Err(ProgramError::MissingRequiredSignature);
        }

        let user_token_acc_info = next_account_info(acc_iter)?;
        let escrow_data_acc_info = next_account_info(acc_iter)?;
        let vault_acc_info = next_account_info(acc_iter)?;
        let system_program_info = next_account_info(acc_iter)?;
        let token_program_info = next_account_info(acc_iter)?;
        let rent_sysvar_info = next_account_info(acc_iter)?;

        // Logger
        let logger_program_info = next_account_info(acc_iter)?;
        let logger_state_acc_info = next_account_info(acc_iter)?;
        let message_pda_info = next_account_info(acc_iter)?;
        let payer_account_info = next_account_info(acc_iter)?;
        let logger_system_program_info = next_account_info(acc_iter)?;
        let mint_acc_info = next_account_info(acc_iter)?; 

        if *user_token_acc_info.owner != spl_token::id() {
            return Err(ProgramError::IncorrectProgramId);
        }

        let token_mint = *mint_acc_info.key;

        let user_token_data = TokenAccount::unpack(&user_token_acc_info.data.borrow())?;
        if user_token_data.mint != token_mint {
            return Err(ProgramError::InvalidAccountData);
        }

        let (expected_escrow_pda, escrow_bump) =
            Pubkey::find_program_address(&[b"escrow", token_mint.as_ref()], program_id);
        if expected_escrow_pda != *escrow_data_acc_info.key {
            return Err(ProgramError::InvalidAccountData);
        }

        let (expected_vault_pda, vault_bump) =
            Pubkey::find_program_address(&[b"vault", token_mint.as_ref()], program_id);
        if expected_vault_pda != *vault_acc_info.key {
            msg!("Vault PDA mismatch.");
            return Err(ProgramError::InvalidAccountData);
        }

        if escrow_data_acc_info.lamports() == 0 {
            let space = EscrowState::LEN;
            let rent_lamports = Rent::get()?.minimum_balance(space);
            let create_ix = system_instruction::create_account(
                user_signer.key,
                escrow_data_acc_info.key,
                rent_lamports,
                space as u64,
                program_id,
            );
            invoke_signed(
                &create_ix,
                &[
                    user_signer.clone(),
                    escrow_data_acc_info.clone(),
                    system_program_info.clone(),
                ],
                &[&[b"escrow", token_mint.as_ref(), &[escrow_bump]]],
            )?;

            let escrow_state = EscrowState {
                is_initialized: true,
                token_mint,
                escrow_vault_account: expected_vault_pda,
                total_deposited: 0,
            };
            EscrowState::pack(escrow_state, &mut escrow_data_acc_info.data.borrow_mut())?;
            msg!("Escrow account created and initialized.");
        } else {
            let existing = EscrowState::unpack(&escrow_data_acc_info.data.borrow())?;
            if existing.token_mint != token_mint {
                return Err(EscrowError::MintMismatch.into());
            }
        }

        if vault_acc_info.lamports() == 0 {
            let rent = Rent::get()?.minimum_balance(spl_token::state::Account::LEN);
            let create_ix = system_instruction::create_account(
                user_signer.key,
                vault_acc_info.key,
                rent,
                spl_token::state::Account::LEN as u64,
                &spl_token::id(),
            );
            invoke_signed(
                &create_ix,
                &[
                    user_signer.clone(),
                    vault_acc_info.clone(),
                    system_program_info.clone(),
                ],
                &[&[b"vault", token_mint.as_ref(), &[vault_bump]]],
            )?;

            let init_ix = spl_token::instruction::initialize_account(
                token_program_info.key,
                vault_acc_info.key,
                &token_mint,
                &expected_vault_pda, 
            )?;
            invoke_signed(
                &init_ix,
                &[
                    vault_acc_info.clone(),
                    mint_acc_info.clone(),
                    escrow_data_acc_info.clone(),
                    rent_sysvar_info.clone(),
                    token_program_info.clone(),
                ],
                &[&[b"vault", token_mint.as_ref(), &[vault_bump]]],
            )?;

            msg!("Vault account created and initialized.");
        } else {
            let vault_data = TokenAccount::unpack(&vault_acc_info.data.borrow())?;
            if vault_data.mint != token_mint {
                return Err(EscrowError::MintMismatch.into());
            }
        }

        msg!("Transferring {} tokens to vault...", amount);
        let transfer_ix = spl_token::instruction::transfer(
            token_program_info.key,
            user_token_acc_info.key,
            vault_acc_info.key,
            user_signer.key,
            &[&user_signer.key],
            amount,
        )?;
        invoke(
            &transfer_ix,
            &[
                user_token_acc_info.clone(),
                vault_acc_info.clone(),
                user_signer.clone(),
                token_program_info.clone(),
            ],
        )?;
        msg!("Token transfer complete.");

        let mut escrow_state = EscrowState::unpack(&escrow_data_acc_info.data.borrow())?;
        escrow_state.total_deposited = escrow_state
            .total_deposited
            .checked_add(amount)
            .ok_or(EscrowError::AmountOverflow)?;
        EscrowState::pack(escrow_state, &mut escrow_data_acc_info.data.borrow_mut())?;

        // Logger
        let clock = Clock::get()?;
        let timestamp = clock.unix_timestamp as u64;
        let mut logger_data = vec![0u8; 80];
        logger_data[..32].copy_from_slice(user_signer.key.as_ref());
        logger_data[32..64].copy_from_slice(vault_acc_info.key.as_ref());
        logger_data[64..72].copy_from_slice(&amount.to_le_bytes());
        logger_data[72..80].copy_from_slice(&timestamp.to_le_bytes());

        let logger_ix = Instruction {
            program_id: *logger_program_info.key,
            accounts: vec![
                AccountMeta::new(*logger_state_acc_info.key, false),
                AccountMeta::new(*message_pda_info.key, false),
                AccountMeta::new(*payer_account_info.key, true),
                AccountMeta::new_readonly(*logger_system_program_info.key, false),
            ],
            data: logger_data,
        };

        invoke(
            &logger_ix,
            &[
                logger_program_info.clone(),
                logger_state_acc_info.clone(),
                message_pda_info.clone(),
                payer_account_info.clone(),
                logger_system_program_info.clone(),
            ],
        )?;
        msg!("Logger invoked successfully.");

        Ok(())
    }

    fn process_withdraw(
        accounts: &[AccountInfo],
        amount: u64,
        program_id: &Pubkey,
    ) -> ProgramResult {
        msg!("Starting process_withdraw with amount: {}", amount);
        
        let acc_iter = &mut accounts.iter();
    
        let user_signer = next_account_info(acc_iter)?;
        if !user_signer.is_signer {
            return Err(ProgramError::MissingRequiredSignature);
        }
    
        let user_token_acc_info = next_account_info(acc_iter)?;
        let escrow_data_acc_info = next_account_info(acc_iter)?;
        let vault_acc_info = next_account_info(acc_iter)?;
        let token_program_info = next_account_info(acc_iter)?;
        let logger_program_info = next_account_info(acc_iter)?;
        let logger_state_acc_info = next_account_info(acc_iter)?;
        let vault_authority_info = next_account_info(acc_iter)?; // PDA ["vault", mint]
        let message_pda_info = next_account_info(acc_iter)?;
        let payer_account_info = next_account_info(acc_iter)?;
        let logger_system_program_info = next_account_info(acc_iter)?;
    
        let mut escrow_state = EscrowState::unpack(&escrow_data_acc_info.data.borrow())?;
        if !escrow_state.is_initialized {
            return Err(ProgramError::UninitializedAccount);
        }
    
        let token_mint = escrow_state.token_mint;
        let (expected_escrow_pda, _) = Pubkey::find_program_address(&[b"escrow", token_mint.as_ref()], program_id);
        if expected_escrow_pda != *escrow_data_acc_info.key {
            return Err(ProgramError::InvalidAccountData);
        }
    
        if escrow_state.escrow_vault_account != *vault_acc_info.key {
            return Err(ProgramError::InvalidAccountData);
        }
    
        if escrow_state.total_deposited < amount {
            return Err(EscrowError::InsufficientAmount.into());
        }
        escrow_state.total_deposited -= amount;
    
        let vault_data = TokenAccount::unpack(&vault_acc_info.data.borrow())?;
        if vault_data.mint != token_mint {
            return Err(EscrowError::MintMismatch.into());
        }
    
        let (vault_pda, vault_bump) = Pubkey::find_program_address(&[b"vault", token_mint.as_ref()], program_id);
        if vault_pda != *vault_authority_info.key {
            return Err(ProgramError::InvalidSeeds);
        }
    
        let transfer_out_ix = spl_token::instruction::transfer(
            token_program_info.key,
            vault_acc_info.key,
            user_token_acc_info.key,
            &vault_pda,
            &[],
            amount,
        )?;
    
        invoke_signed(
            &transfer_out_ix,
            &[
                vault_acc_info.clone(),
                user_token_acc_info.clone(),
                vault_authority_info.clone(),
                token_program_info.clone(),
            ],
            &[&[b"vault", token_mint.as_ref(), &[vault_bump]]],
        )?;
    
        EscrowState::pack(escrow_state, &mut escrow_data_acc_info.data.borrow_mut())?;
    
        let clock = Clock::get()?;
        let timestamp = clock.unix_timestamp as u64;
    
        let mut logger_data = vec![0u8; 80];
        logger_data[..32].copy_from_slice(vault_acc_info.key.as_ref());
        logger_data[32..64].copy_from_slice(user_signer.key.as_ref());
        logger_data[64..72].copy_from_slice(&amount.to_le_bytes());
        logger_data[72..80].copy_from_slice(&timestamp.to_le_bytes());
    
        let logger_ix = Instruction {
            program_id: *logger_program_info.key,
            accounts: vec![
                AccountMeta::new(*logger_state_acc_info.key, false),
                AccountMeta::new(*message_pda_info.key, false),
                AccountMeta::new(*payer_account_info.key, true),
                AccountMeta::new_readonly(*logger_system_program_info.key, false),
            ],
            data: logger_data,
        };
    
        invoke(
            &logger_ix,
            &[
                logger_program_info.clone(),
                logger_state_acc_info.clone(),
                message_pda_info.clone(),
                payer_account_info.clone(),
                logger_system_program_info.clone(),
            ],
        )?;
    
        msg!("Withdraw completed.");
        Ok(())
    }
}
