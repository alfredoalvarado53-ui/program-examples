use {
    crate::{error::SwapError, state::Amm, utils::create_pda_account},
    borsh::{BorshDeserialize, BorshSerialize},
    solana_program::{
        account_info::AccountInfo, entrypoint::ProgramResult, program_error::ProgramError,
        pubkey::Pubkey,
    },
};

#[derive(BorshSerialize, BorshDeserialize, Debug, Clone)]
pub struct CreateAmmArgs {
    pub id: Pubkey,
    pub fee: u16,
}

pub struct CreateAmm;

impl CreateAmm {
    pub fn process(
        program_id: &Pubkey,
        accounts: &[AccountInfo],
        args: CreateAmmArgs,
    ) -> ProgramResult {
        let [
            amm_info,
            admin_info,
            payer_info,
            system_program_info,
        ] = accounts else {
            return Err(ProgramError::NotEnoughAccountKeys);
        };

        if !payer_info.is_signer {
            return Err(ProgramError::MissingRequiredSignature);
        }

        if args.fee >= 10_000 {
            return Err(SwapError::InvalidFee.into());
        }

        let (expected_amm_key, bump) =
            Pubkey::find_program_address(&[args.id.as_ref()], program_id);

        if *amm_info.key != expected_amm_key {
            return Err(SwapError::AccountMismatch.into());
        }

        let signer_seeds: &[&[u8]] = &[args.id.as_ref(), &[bump]];

        create_pda_account(
            payer_info,
            amm_info,
            system_program_info,
            program_id,
            Amm::LEN,
            signer_seeds,
        )?;

        let amm = Amm {
            id: args.id,
            admin: *admin_info.key,
            fee: args.fee,
        };

        amm.serialize(&mut &mut amm_info.data.borrow_mut()[..])?;

        Ok(())
    }
}
