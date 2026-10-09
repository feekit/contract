use anchor_lang::prelude::*;
use anchor_lang::solana_program::program::invoke_signed;
use anchor_lang::solana_program::system_instruction;

use crate::constants::*;
use crate::errors::FeeKitError;
use crate::events::CreatorPaid;
use crate::policy::spendable_lamports;
use crate::pump::{bonding_curve_pda, decode_bonding_curve, require_fee_routing};
use crate::state::LaunchConfig;

pub fn handler(ctx: Context<ReleaseCreator>) -> Result<()> {
    let config = &ctx.accounts.config;
    require!(config.locked, FeeKitError::FeesNotLocked);
    require!(config.is_graduate(), FeeKitError::InvalidParams);

    let mint = config.mint;
    let sol_vault = ctx.accounts.sol_vault.key();
    require_keys_eq!(
        ctx.accounts.creator.key(),
        config.creator,
        FeeKitError::NotCreator
    );
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

    let curve = decode_bonding_curve(&ctx.accounts.bonding_curve.try_borrow_data()?)?;
    require!(curve.complete, FeeKitError::NotGraduated);
    require_keys_eq!(
        curve.creator,
        ctx.accounts.sharing_config.key(),
        FeeKitError::FeeRecipientMismatch
    );

    let rent = Rent::get()?.minimum_balance(0);
    let payout = spendable_lamports(ctx.accounts.sol_vault.lamports(), rent);
    require!(
        payout >= config.min_execute_lamports,
        FeeKitError::ThresholdNotMet
    );

    let bump = [config.sol_vault_bump];
    let seeds: &[&[u8]] = &[VAULT_SEED, mint.as_ref(), &bump];
    invoke_signed(
        &system_instruction::transfer(
            ctx.accounts.sol_vault.key,
            ctx.accounts.creator.key,
            payout,
        ),
        &[
            ctx.accounts.sol_vault.to_account_info(),
            ctx.accounts.creator.to_account_info(),
            ctx.accounts.system_program.to_account_info(),
        ],
        &[seeds],
    )?;

    let config = &mut ctx.accounts.config;
    config.creator_paid = config
        .creator_paid
        .checked_add(payout)
        .ok_or(FeeKitError::Overflow)?;
    config.last_execution_slot = Clock::get()?.slot;
    config.execution_count = config
        .execution_count
        .checked_add(1)
        .ok_or(FeeKitError::Overflow)?;
    config.last_executor = ctx.accounts.crank.key();

    emit!(CreatorPaid {
        mint,
        creator: ctx.accounts.creator.key(),
        lamports: payout,
    });
    Ok(())
}

#[derive(Accounts)]
pub struct ReleaseCreator<'info> {
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

    /// CHECK: Coin creator stored on the launch. The crank pays this account.
    #[account(mut)]
    pub creator: UncheckedAccount<'info>,

    /// CHECK: Canonical bonding curve. Must be complete.
    pub bonding_curve: UncheckedAccount<'info>,

    /// CHECK: Pump Fees sharing config. Must name the SOL vault as the only shareholder.
    pub sharing_config: UncheckedAccount<'info>,

    pub system_program: Program<'info, System>,
}
