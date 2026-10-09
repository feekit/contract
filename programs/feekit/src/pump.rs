use anchor_lang::prelude::*;
use anchor_lang::solana_program::instruction::{AccountMeta, Instruction};
use anchor_lang::solana_program::program::invoke_signed;

use crate::constants::*;
use crate::errors::FeeKitError;

pub struct CurveView {
    pub virtual_token_reserves: u64,
    pub virtual_quote_reserves: u64,
    pub complete: bool,
    pub creator: Pubkey,
    pub quote_mint: Option<Pubkey>,
    pub is_holder_reward: bool,
}

pub struct PoolView {
    pub base_mint: Pubkey,
    pub quote_mint: Pubkey,
    pub pool_base_token_account: Pubkey,
    pub pool_quote_token_account: Pubkey,
    pub coin_creator: Pubkey,
    pub virtual_quote_reserves: u64,
}

pub fn bonding_curve_pda(mint: &Pubkey) -> Pubkey {
    Pubkey::find_program_address(&[b"bonding-curve", mint.as_ref()], &PUMP_PROGRAM_ID).0
}

pub fn sharing_config_pda(mint: &Pubkey) -> Pubkey {
    Pubkey::find_program_address(&[b"sharing-config", mint.as_ref()], &FEE_PROGRAM_ID).0
}

pub fn pump_creator_vault_pda(sharing_config: &Pubkey) -> Pubkey {
    Pubkey::find_program_address(
        &[b"creator-vault", sharing_config.as_ref()],
        &PUMP_PROGRAM_ID,
    )
    .0
}

pub fn amm_creator_vault_pda(sharing_config: &Pubkey) -> Pubkey {
    Pubkey::find_program_address(
        &[b"creator_vault", sharing_config.as_ref()],
        &PUMP_AMM_PROGRAM_ID,
    )
    .0
}

pub fn canonical_pool_pda(mint: &Pubkey) -> Pubkey {
    let authority =
        Pubkey::find_program_address(&[b"pool-authority", mint.as_ref()], &PUMP_PROGRAM_ID).0;
    Pubkey::find_program_address(
        &[
            b"pool",
            &0u16.to_le_bytes(),
            authority.as_ref(),
            mint.as_ref(),
            NATIVE_MINT.as_ref(),
        ],
        &PUMP_AMM_PROGRAM_ID,
    )
    .0
}

pub fn pool_v2_pda(mint: &Pubkey) -> Pubkey {
    Pubkey::find_program_address(&[b"pool-v2", mint.as_ref()], &PUMP_AMM_PROGRAM_ID).0
}

pub fn pump_global_pda() -> Pubkey {
    Pubkey::find_program_address(&[b"global"], &PUMP_PROGRAM_ID).0
}

pub fn pump_event_authority_pda() -> Pubkey {
    Pubkey::find_program_address(&[b"__event_authority"], &PUMP_PROGRAM_ID).0
}

pub fn fee_event_authority_pda() -> Pubkey {
    Pubkey::find_program_address(&[b"__event_authority"], &FEE_PROGRAM_ID).0
}

pub fn amm_event_authority_pda() -> Pubkey {
    Pubkey::find_program_address(&[b"__event_authority"], &PUMP_AMM_PROGRAM_ID).0
}

pub fn amm_global_config_pda() -> Pubkey {
    Pubkey::find_program_address(&[b"global_config"], &PUMP_AMM_PROGRAM_ID).0
}

pub fn pump_fee_config_pda() -> Pubkey {
    Pubkey::find_program_address(&[b"fee_config", PUMP_PROGRAM_ID.as_ref()], &FEE_PROGRAM_ID).0
}

pub fn amm_fee_config_pda() -> Pubkey {
    Pubkey::find_program_address(
        &[b"fee_config", PUMP_AMM_PROGRAM_ID.as_ref()],
        &FEE_PROGRAM_ID,
    )
    .0
}

pub fn volume_accumulator_pda(program: &Pubkey, user: &Pubkey) -> Pubkey {
    Pubkey::find_program_address(&[b"user_volume_accumulator", user.as_ref()], program).0
}

pub fn global_volume_accumulator_pda(program: &Pubkey) -> Pubkey {
    Pubkey::find_program_address(&[b"global_volume_accumulator"], program).0
}

pub fn ata(owner: &Pubkey, token_program: &Pubkey, mint: &Pubkey) -> Pubkey {
    Pubkey::find_program_address(
        &[owner.as_ref(), token_program.as_ref(), mint.as_ref()],
        &anchor_spl::associated_token::ID,
    )
    .0
}

pub fn decode_bonding_curve(data: &[u8]) -> Result<CurveView> {
    require!(
        data.len() >= 81 && data[..8] == BONDING_CURVE_DISC,
        FeeKitError::BadAccountData
    );
    let virtual_token_reserves = read_u64(data, 8)?;
    let virtual_quote_reserves = read_u64(data, 16)?;
    let complete = data[48] == 1;
    let creator = read_pubkey(&data[49..81])?;
    let (quote_mint, is_holder_reward) = if data.len() >= 125 {
        (Some(read_pubkey(&data[83..115])?), data[124] == 1)
    } else {
        (None, false)
    };
    Ok(CurveView {
        virtual_token_reserves,
        virtual_quote_reserves,
        complete,
        creator,
        quote_mint,
        is_holder_reward,
    })
}

pub fn assert_sol_curve(curve: &CurveView) -> Result<()> {
    if curve.is_holder_reward {
        return err!(FeeKitError::HolderRewardsUnsupported);
    }
    if let Some(quote) = curve.quote_mint {
        if quote != Pubkey::default() && quote != NATIVE_MINT {
            return err!(FeeKitError::QuoteNotSol);
        }
    }
    Ok(())
}

pub fn decode_pool(data: &[u8]) -> Result<PoolView> {
    require!(
        data.len() >= 243 && data[..8] == POOL_DISC,
        FeeKitError::BadAccountData
    );
    Ok(PoolView {
        base_mint: read_pubkey(&data[43..75])?,
        quote_mint: read_pubkey(&data[75..107])?,
        pool_base_token_account: read_pubkey(&data[139..171])?,
        pool_quote_token_account: read_pubkey(&data[171..203])?,
        coin_creator: read_pubkey(&data[211..243])?,
        virtual_quote_reserves: virtual_quote_reserves(data),
    })
}

/// PumpSwap stores virtual quote as an i128 after the mayhem and cashback flags.
fn virtual_quote_reserves(data: &[u8]) -> u64 {
    if data.len() < 261 {
        return 0;
    }
    let mut bytes = [0u8; 16];
    bytes.copy_from_slice(&data[245..261]);
    let raw = i128::from_le_bytes(bytes);
    if raw <= 0 {
        return 0;
    }
    u64::try_from(raw).unwrap_or(u64::MAX)
}

pub fn token_amount(data: &[u8], expected_mint: &Pubkey) -> Result<u64> {
    require!(data.len() >= 72, FeeKitError::BadAccountData);
    let mint = read_pubkey(&data[0..32])?;
    require_keys_eq!(mint, *expected_mint, FeeKitError::BadAccountData);
    read_u64(data, 64)
}

pub fn require_fee_routing(mint: &Pubkey, sol_vault: &Pubkey, sharing: &AccountInfo) -> Result<()> {
    require_keys_eq!(
        *sharing.key,
        sharing_config_pda(mint),
        FeeKitError::FeeRecipientMismatch
    );
    require!(
        sharing.owner == &FEE_PROGRAM_ID,
        FeeKitError::FeeRecipientMismatch
    );
    assert_fee_routing(&sharing.try_borrow_data()?, mint, sol_vault)
}

/// Sharing config must be active, revoked, and split between the kit vault and the treasury.
pub fn assert_fee_routing(data: &[u8], mint: &Pubkey, sol_vault: &Pubkey) -> Result<()> {
    require!(
        data.len() >= 148 && data[..8] == SHARING_CONFIG_DISC,
        FeeKitError::BadAccountData
    );
    require!(
        data[10] == SHARING_ACTIVE,
        FeeKitError::FeeRecipientMismatch
    );
    let config_mint = read_pubkey(&data[11..43])?;
    require_keys_eq!(config_mint, *mint, FeeKitError::FeeRecipientMismatch);
    require!(data[75] == 1, FeeKitError::FeeRecipientMismatch);
    let shareholders = u32::from_le_bytes(
        data[76..80]
            .try_into()
            .map_err(|_| error!(FeeKitError::BadAccountData))?,
    );
    require!(shareholders == 2, FeeKitError::FeeRecipientMismatch);
    let (vault, vault_bps) = shareholder_at(data, 0)?;
    let (treasury, treasury_bps) = shareholder_at(data, 1)?;
    require_keys_eq!(vault, *sol_vault, FeeKitError::FeeRecipientMismatch);
    require!(vault_bps == KIT_SHARE_BPS, FeeKitError::FeeRecipientMismatch);
    let (platform, _) = treasury_pda();
    require_keys_eq!(treasury, platform, FeeKitError::FeeRecipientMismatch);
    require!(
        treasury_bps == PLATFORM_FEE_BPS,
        FeeKitError::FeeRecipientMismatch
    );
    Ok(())
}

pub fn treasury_pda() -> (Pubkey, u8) {
    Pubkey::find_program_address(&[TREASURY_SEED], &crate::ID)
}

/// Borsh `Vec<Shareholder>` with the kit vault at 85% and the treasury at 15%.
pub fn update_fee_shares_v2_data(sol_vault: &Pubkey, treasury: &Pubkey) -> Vec<u8> {
    let mut data = Vec::with_capacity(80);
    data.extend_from_slice(&UPDATE_FEE_SHARES_V2);
    data.extend_from_slice(&2u32.to_le_bytes());
    data.extend_from_slice(sol_vault.as_ref());
    data.extend_from_slice(&KIT_SHARE_BPS.to_le_bytes());
    data.extend_from_slice(treasury.as_ref());
    data.extend_from_slice(&PLATFORM_FEE_BPS.to_le_bytes());
    data
}

fn shareholder_at(data: &[u8], index: usize) -> Result<(Pubkey, u16)> {
    let start = 80 + index * 34;
    let recipient = read_pubkey(
        data.get(start..start + 32)
            .ok_or(FeeKitError::BadAccountData)?,
    )?;
    let bps = u16::from_le_bytes(
        data.get(start + 32..start + 34)
            .ok_or(FeeKitError::BadAccountData)?
            .try_into()
            .map_err(|_| error!(FeeKitError::BadAccountData))?,
    );
    Ok((recipient, bps))
}

pub struct IxAccount<'a> {
    pub info: AccountInfo<'a>,
    pub signer: bool,
    pub writable: bool,
}

pub fn invoke<'a>(
    program: &AccountInfo<'a>,
    accounts: &[IxAccount<'a>],
    data: Vec<u8>,
    signer_seeds: &[&[&[u8]]],
) -> Result<()> {
    let metas = accounts
        .iter()
        .map(|account| AccountMeta {
            pubkey: *account.info.key,
            is_signer: account.signer,
            is_writable: account.writable,
        })
        .collect();
    let infos: Vec<AccountInfo> = accounts
        .iter()
        .map(|account| account.info.clone())
        .collect();
    let instruction = Instruction {
        program_id: *program.key,
        accounts: metas,
        data,
    };
    invoke_signed(&instruction, &infos, signer_seeds)?;
    Ok(())
}

fn read_u64(data: &[u8], offset: usize) -> Result<u64> {
    let bytes: [u8; 8] = data
        .get(offset..offset + 8)
        .ok_or(FeeKitError::BadAccountData)?
        .try_into()
        .map_err(|_| error!(FeeKitError::BadAccountData))?;
    Ok(u64::from_le_bytes(bytes))
}

fn read_pubkey(data: &[u8]) -> Result<Pubkey> {
    let bytes: [u8; 32] = data
        .try_into()
        .map_err(|_| error!(FeeKitError::BadAccountData))?;
    Ok(Pubkey::new_from_array(bytes))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sharing_config_accepts_the_vault_and_treasury() {
        let mint = Pubkey::new_unique();
        let vault = Pubkey::new_unique();
        let (treasury, _) = treasury_pda();
        let mut data = vec![0u8; 148];
        data[..8].copy_from_slice(&SHARING_CONFIG_DISC);
        data[10] = SHARING_ACTIVE;
        data[11..43].copy_from_slice(mint.as_ref());
        data[75] = 1;
        data[76..80].copy_from_slice(&2u32.to_le_bytes());
        data[80..112].copy_from_slice(vault.as_ref());
        data[112..114].copy_from_slice(&KIT_SHARE_BPS.to_le_bytes());
        data[114..146].copy_from_slice(treasury.as_ref());
        data[146..148].copy_from_slice(&PLATFORM_FEE_BPS.to_le_bytes());
        assert_fee_routing(&data, &mint, &vault).unwrap();

        data[112..114].copy_from_slice(&10_000u16.to_le_bytes());
        assert!(assert_fee_routing(&data, &mint, &vault).is_err());
    }

    #[test]
    fn fee_share_payload_keeps_fifteen_percent_for_the_platform() {
        let vault = Pubkey::new_unique();
        let (treasury, _) = treasury_pda();
        let data = update_fee_shares_v2_data(&vault, &treasury);
        assert_eq!(data.len(), 80);
        assert_eq!(&data[..8], &UPDATE_FEE_SHARES_V2);
        assert_eq!(u32::from_le_bytes(data[8..12].try_into().unwrap()), 2);
        assert_eq!(&data[12..44], vault.as_ref());
        assert_eq!(
            u16::from_le_bytes(data[44..46].try_into().unwrap()),
            KIT_SHARE_BPS
        );
        assert_eq!(&data[46..78], treasury.as_ref());
        assert_eq!(
            u16::from_le_bytes(data[78..80].try_into().unwrap()),
            PLATFORM_FEE_BPS
        );
    }

    #[test]
    fn curve_and_pool_pdas_are_deterministic() {
        let mint = Pubkey::new_unique();
        assert_ne!(bonding_curve_pda(&mint), canonical_pool_pda(&mint));
        assert_eq!(bonding_curve_pda(&mint), bonding_curve_pda(&mint));
    }
}
