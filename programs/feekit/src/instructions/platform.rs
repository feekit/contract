use anchor_lang::prelude::*;
use anchor_lang::solana_program::program::invoke_signed;
use anchor_lang::solana_program::system_instruction;

use crate::constants::*;
use crate::errors::FeeKitError;
use crate::state::PlatformConfig;

pub fn initialize(ctx: Context<InitializePlatform>) -> Result<()> {
    let bump = ctx.bumps.treasury;
    let rent = Rent::get()?.minimum_balance(0);
    let create_treasury = system_instruction::create_account(
        ctx.accounts.authority.key,
        ctx.accounts.treasury.key,
        rent,
        0,
        &System::id(),
    );
    let bump_seed = [bump];
    invoke_signed(
        &create_treasury,
        &[
            ctx.accounts.authority.to_account_info(),
            ctx.accounts.treasury.to_account_info(),
            ctx.accounts.system_program.to_account_info(),
        ],
        &[&[TREASURY_SEED, &bump_seed]],
    )?;

    let platform = &mut ctx.accounts.platform;
    platform.bump = ctx.bumps.platform;
    platform.treasury_bump = bump;
    platform.authority = ctx.accounts.authority.key();
    Ok(())
}

pub fn withdraw(ctx: Context<WithdrawPlatform>, amount: u64) -> Result<()> {
    let rent = Rent::get()?.minimum_balance(0);
    let available = ctx.accounts.treasury.lamports().saturating_sub(rent);
    require!(amount > 0 && amount <= available, FeeKitError::ThresholdNotMet);

    let bump = [ctx.accounts.platform.treasury_bump];
    invoke_signed(
        &system_instruction::transfer(
            ctx.accounts.treasury.key,
            ctx.accounts.destination.key,
            amount,
        ),
        &[
            ctx.accounts.treasury.to_account_info(),
            ctx.accounts.destination.to_account_info(),
            ctx.accounts.system_program.to_account_info(),
        ],
        &[&[TREASURY_SEED, &bump]],
    )?;
    Ok(())
}

#[derive(Accounts)]
pub struct InitializePlatform<'info> {
    #[account(mut)]
    pub authority: Signer<'info>,

    #[account(
        init,
        payer = authority,
        space = 8 + PlatformConfig::INIT_SPACE,
        seeds = [PLATFORM_SEED],
        bump
    )]
    pub platform: Account<'info, PlatformConfig>,

    /// CHECK: Created here as a zero-data system account so pump can transfer SOL in.
    #[account(
        mut,
        seeds = [TREASURY_SEED],
        bump
    )]
    pub treasury: UncheckedAccount<'info>,

    pub system_program: Program<'info, System>,
}

#[derive(Accounts)]
pub struct WithdrawPlatform<'info> {
    pub authority: Signer<'info>,

    #[account(
        seeds = [PLATFORM_SEED],
        bump = platform.bump,
        has_one = authority,
    )]
    pub platform: Account<'info, PlatformConfig>,

    /// CHECK: System-owned treasury PDA.
    #[account(
        mut,
        seeds = [TREASURY_SEED],
        bump = platform.treasury_bump,
    )]
    pub treasury: UncheckedAccount<'info>,

    /// CHECK: Destination chosen by the platform authority.
    #[account(mut)]
    pub destination: UncheckedAccount<'info>,

    pub system_program: Program<'info, System>,
}
