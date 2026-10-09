use anchor_lang::prelude::*;
use anchor_lang::solana_program::program::invoke_signed;
use anchor_lang::solana_program::system_instruction;
use anchor_spl::token::Token;
use anchor_spl::token_2022::Token2022;
use anchor_spl::token_interface::Mint;

use crate::constants::*;
use crate::errors::FeeKitError;
use crate::events::VaultInitialized;
use crate::policy::validate_params;
use crate::pump::{assert_sol_curve, bonding_curve_pda, decode_bonding_curve};
use crate::state::{LaunchConfig, VaultParams};

pub fn handler(ctx: Context<InitializeVault>, params: VaultParams) -> Result<()> {
    validate_params(&params)?;

    let mint = ctx.accounts.mint.key();
    require_keys_eq!(
        ctx.accounts.bonding_curve.key(),
        bonding_curve_pda(&mint),
        FeeKitError::BadBondingCurve
    );
    require!(
        ctx.accounts.bonding_curve.owner == &PUMP_PROGRAM_ID,
        FeeKitError::BadBondingCurve
    );
    let curve = decode_bonding_curve(&ctx.accounts.bonding_curve.try_borrow_data()?)?;
    assert_sol_curve(&curve)?;
    require_keys_eq!(
        curve.creator,
        ctx.accounts.creator.key(),
        FeeKitError::NotCreator
    );

    let token_program = *ctx.accounts.mint.to_account_info().owner;
    require!(
        token_program == Token::id() || token_program == Token2022::id(),
        FeeKitError::UnsupportedTokenProgram
    );

    let bump = ctx.bumps.sol_vault;
    let rent = Rent::get()?.minimum_balance(0);
    let create_vault = system_instruction::create_account(
        ctx.accounts.creator.key,
        ctx.accounts.sol_vault.key,
        rent,
        0,
        &System::id(),
    );
    let bump_seed = [bump];
    invoke_signed(
        &create_vault,
        &[
            ctx.accounts.creator.to_account_info(),
            ctx.accounts.sol_vault.to_account_info(),
            ctx.accounts.system_program.to_account_info(),
        ],
        &[&[VAULT_SEED, mint.as_ref(), &bump_seed]],
    )?;

    let config = &mut ctx.accounts.config;
    config.bump = ctx.bumps.config;
    config.sol_vault_bump = bump;
    config.kit = params.kit;
    config.locked = false;
    config.slippage_bps = params.slippage_bps;
    config.drawdown_bps = params.drawdown_bps;
    config.spend_bps = params.spend_bps;
    config.mint = mint;
    config.quote_mint = NATIVE_MINT;
    config.creator = ctx.accounts.creator.key();
    config.base_token_program = token_program;
    config.sol_vault = ctx.accounts.sol_vault.key();
    config.min_execute_lamports = params.min_execute_lamports;
    config.max_spend_lamports = params.max_spend_lamports;
    config.interval_slots = params.interval_slots;
    config.mark_delay_slots = params.mark_delay_slots;

    emit!(VaultInitialized {
        mint,
        config: config.key(),
        sol_vault: config.sol_vault,
        creator: config.creator,
        kit: config.kit,
    });
    Ok(())
}

#[derive(Accounts)]
pub struct InitializeVault<'info> {
    #[account(mut)]
    pub creator: Signer<'info>,

    pub mint: InterfaceAccount<'info, Mint>,

    /// CHECK: Canonical pump bonding curve for `mint`. The handler reads creator and quote mint.
    pub bonding_curve: UncheckedAccount<'info>,

    #[account(
        init,
        payer = creator,
        space = 8 + LaunchConfig::INIT_SPACE,
        seeds = [CONFIG_SEED, mint.key().as_ref()],
        bump
    )]
    pub config: Account<'info, LaunchConfig>,

    /// CHECK: Created here as a zero-data system account. Buys transfer SOL out of this PDA.
    #[account(
        mut,
        seeds = [VAULT_SEED, mint.key().as_ref()],
        bump
    )]
    pub sol_vault: UncheckedAccount<'info>,

    pub system_program: Program<'info, System>,
}
