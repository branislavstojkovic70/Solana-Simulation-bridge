use solana_program::{
    program_error::ProgramError,
    program_pack::{IsInitialized, Pack, Sealed},
    pubkey::Pubkey,
};
use arrayref::{array_mut_ref, array_ref, array_refs, mut_array_refs};

pub struct EscrowState {
    pub is_initialized: bool,
    pub token_mint: Pubkey,
    pub escrow_vault_account: Pubkey, // SPL Token account (PDA) koji drži tokene
    pub total_deposited: u64,
}

impl Sealed for EscrowState {}

impl IsInitialized for EscrowState {
    fn is_initialized(&self) -> bool {
        self.is_initialized
    }
}

impl Pack for EscrowState {
    // 1 bajt + 32 + 32 + 8 = 73 bajta
    const LEN: usize = 73;

    fn unpack_from_slice(src: &[u8]) -> Result<Self, ProgramError> {
        let src = array_ref![src, 0, EscrowState::LEN];
        let (is_init_arr, mint_arr, vault_arr, deposited_arr) = array_refs![src, 1, 32, 32, 8];

        let is_initialized = match is_init_arr {
            [0] => false,
            [1] => true,
            _ => return Err(ProgramError::InvalidAccountData),
        };

        Ok(EscrowState {
            is_initialized,
            token_mint: Pubkey::new_from_array(*mint_arr),
            escrow_vault_account: Pubkey::new_from_array(*vault_arr),
            total_deposited: u64::from_le_bytes(*deposited_arr),
        })
    }

    fn pack_into_slice(&self, dst: &mut [u8]) {
        let dst = array_mut_ref![dst, 0, EscrowState::LEN];
        let (is_init_dst, mint_dst, vault_dst, deposited_dst) =
            mut_array_refs![dst, 1, 32, 32, 8];

        is_init_dst[0] = self.is_initialized as u8;
        mint_dst.copy_from_slice(self.token_mint.as_ref());
        vault_dst.copy_from_slice(self.escrow_vault_account.as_ref());
        *deposited_dst = self.total_deposited.to_le_bytes();
    }
}
