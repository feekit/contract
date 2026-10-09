use anchor_lang::prelude::*;
use anchor_lang::solana_program::program::invoke_signed;
use anchor_lang::solana_program::system_instruction;
use anchor_spl::associated_token::AssociatedToken;
use anchor_spl::token::Token;
use anchor_spl::token_interface::Mint;

use crate::constants::*;
use crate::errors::FeeKitError;
use crate::events::FeesCollected;
use crate::instructions::lock::acc;
use crate::pump::{
    assert_sol_curve, bonding_curve_pda, decode_bonding_curve, pump_creator_vault_pda,
    pump_event_authority_pda, require_fee_routing,
};
use crate::state::LaunchConfig;

pub fn handler(ctx: Context<CollectFees>) -> Result<()> {
    let config = &ctx.accounts.config;
    require!(config.locked, FeeKitError::FeesNotLocked);

    let mint = config.mint;
    let sol_vault = ctx.accounts.sol_vault.key();
    require_fee_routing(
        &mint,
        &sol_vault,
        &ctx.accounts.sharing_config.to_account_info(),
    )?;
    require_keys_eq!(
        ctx.accounts.bonding_curve.key(),
        bonding_curve_pda(&mint),
        FeeKitError::BadBondingCurve
    );
    require_keys_eq!(
        ctx.accounts.creator_vault.key(),
        pump_creator_vault_pda(&ctx.accounts.sharing_config.key()),
        FeeKitError::BadAccountData
    );
    require_keys_eq!(
        ctx.accounts.event_authority.key(),
        pump_event_authority_pda(),
        FeeKitError::BadAccountData
    );

    let curve = decode_bonding_curve(&ctx.accounts.bonding_curve.try_borrow_data()?)?;
    assert_sol_curve(&curve)?;
    require_keys_eq!(
        curve.creator,
        ctx.accounts.sharing_config.key(),
        FeeKitError::FeeRecipientMismatch
    );

    let vault_bump = ctx.accounts.config.sol_vault_bump;
    let before = ctx.accounts.sol_vault.lamports();
    let treasury_before = ctx.accounts.treasury.lamports();
    let mut data = DISTRIBUTE_CREATOR_FEES_V2.to_vec();
    data.push(0);
    let accounts = vec![
        acc(&ctx.accounts.crank, true, true),
        acc(&ctx.accounts.mint, false, false),
        acc(&ctx.accounts.bonding_curve, false, false),
        acc(&ctx.accounts.sharing_config, false, false),
        acc(&ctx.accounts.creator_vault, false, true),
        acc(&ctx.accounts.system_program, false, false),
        acc(&ctx.accounts.event_authority, false, false),
        acc(&ctx.accounts.pump_program, false, false),
        acc(&ctx.accounts.creator_vault_quote_ata, false, true),
        acc(&ctx.accounts.quote_mint, false, false),
        acc(&ctx.accounts.quote_token_program, false, false),
        acc(&ctx.accounts.associated_token_program, false, false),
        acc(&ctx.accounts.sol_vault, false, true),
        acc(&ctx.accounts.treasury, false, true),
    ];
    crate::pump::invoke(
        &ctx.accounts.pump_program.to_account_info(),
        &accounts,
        data,
        &[],
    )?;

    let vault_in = ctx.accounts.sol_vault.lamports().saturating_sub(before);
    let treasury_in = ctx.accounts.treasury.lamports().saturating_sub(treasury_before);
    let reward = keeper_cut(vault_in, treasury_in)?;
    if reward > 0 {
        let bump = [vault_bump];
        invoke_signed(
            &system_instruction::transfer(ctx.accounts.sol_vault.key, ctx.accounts.crank.key, reward),
            &[
                ctx.accounts.sol_vault.to_account_info(),
                ctx.accounts.crank.to_account_info(),
                ctx.accounts.system_program.to_account_info(),
            ],
            &[&[VAULT_SEED, mint.as_ref(), &bump]],
        )?;
    }
    let kept = vault_in.checked_sub(reward).ok_or(FeeKitError::Overflow)?;
    let config = &mut ctx.accounts.config;
    config.fees_collected = config
        .fees_collected
        .checked_add(kept)
        .ok_or(FeeKitError::Overflow)?;

    emit!(FeesCollected {
        mint,
        lamports: kept,
        slot: Clock::get()?.slot,
        keeper_lamports: reward,
    });
    Ok(())
}

/// The vault received `KIT_SHARE_BPS` of the fee and the treasury received the platform share.
/// The keeper's 5% of the whole fee is taken from the vault, which leaves the kit 80%.
fn keeper_cut(vault_in: u64, treasury_in: u64) -> Result<u64> {
    let gross = (vault_in as u128)
        .checked_add(treasury_in as u128)
        .ok_or(FeeKitError::Overflow)?;
    let reward = gross
        .checked_mul(KEEPER_FEE_BPS as u128)
        .and_then(|value| value.checked_div(BPS_DENOMINATOR as u128))
        .ok_or(FeeKitError::Overflow)?;
    let reward = u64::try_from(reward).map_err(|_| error!(FeeKitError::Overflow))?;
    require!(reward <= vault_in, FeeKitError::Overflow);
    Ok(reward)
}

#[derive(Accounts)]
pub struct CollectFees<'info> {
    #[account(mut)]
    pub crank: Signer<'info>,

    #[account(
        mut,
        seeds = [CONFIG_SEED, config.mint.as_ref()],
        bump = config.bump,
    )]
    pub config: Account<'info, LaunchConfig>,

    #[account(
        mut,
        seeds = [VAULT_SEED, config.mint.as_ref()],
        bump = config.sol_vault_bump,
    )]
    pub sol_vault: SystemAccount<'info>,

    #[account(constraint = mint.key() == config.mint)]
    pub mint: InterfaceAccount<'info, Mint>,

    /// CHECK: Canonical bonding curve. Its creator is the sharing config after lock.
    pub bonding_curve: UncheckedAccount<'info>,

    /// CHECK: Pump Fees sharing config. Must name the SOL vault as the only shareholder.
    pub sharing_config: UncheckedAccount<'info>,

    /// CHECK: Pump creator-vault that holds undistributed creator fees.
    #[account(mut)]
    pub creator_vault: UncheckedAccount<'info>,

    /// CHECK: Wrapped-SOL ATA of the creator vault. Unused for native SOL.
    #[account(mut)]
    pub creator_vault_quote_ata: UncheckedAccount<'info>,

    /// CHECK: Wrapped SOL mint.
    #[account(address = NATIVE_MINT)]
    pub quote_mint: UncheckedAccount<'info>,

    /// CHECK: SPL Token program.
    #[account(address = Token::id())]
    pub quote_token_program: UncheckedAccount<'info>,

    pub associated_token_program: Program<'info, AssociatedToken>,
    pub system_program: Program<'info, System>,

    /// CHECK: Pump event authority.
    pub event_authority: UncheckedAccount<'info>,

    /// CHECK: Pump program.
    #[account(address = PUMP_PROGRAM_ID)]
    pub pump_program: UncheckedAccount<'info>,

    /// CHECK: FeeKit treasury. Second shareholder, paid 15% by pump.
    #[account(
        mut,
        seeds = [TREASURY_SEED],
        bump = platform.treasury_bump,
    )]
    pub treasury: UncheckedAccount<'info>,

    #[account(
        seeds = [PLATFORM_SEED],
        bump = platform.bump,
    )]
    pub platform: Account<'info, crate::state::PlatformConfig>,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn keeper_takes_five_percent_and_leaves_the_kit_eighty() {
        let gross = 1_000_000_000u64;
        let vault_in = gross * KIT_SHARE_BPS as u64 / BPS_DENOMINATOR;
        let treasury_in = gross * PLATFORM_FEE_BPS as u64 / BPS_DENOMINATOR;
        let reward = keeper_cut(vault_in, treasury_in).unwrap();
        assert_eq!(reward, gross * KEEPER_FEE_BPS as u64 / BPS_DENOMINATOR);
        assert_eq!(vault_in - reward, gross * 8_000 / BPS_DENOMINATOR);
    }
}
