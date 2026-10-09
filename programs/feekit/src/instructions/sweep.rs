use anchor_lang::prelude::*;
use anchor_spl::associated_token::AssociatedToken;
use anchor_spl::token::Token;

use crate::constants::*;
use crate::errors::FeeKitError;
use crate::events::AmmFeesSwept;
use crate::instructions::lock::acc;
use crate::pump::{
    amm_creator_vault_pda, amm_event_authority_pda, assert_sol_curve, ata, bonding_curve_pda,
    decode_bonding_curve, pump_creator_vault_pda, require_fee_routing,
};
use crate::state::LaunchConfig;

pub fn handler(ctx: Context<SweepAmmFees>) -> Result<()> {
    let config = &ctx.accounts.config;
    require!(config.locked, FeeKitError::FeesNotLocked);

    let mint = config.mint;
    require_fee_routing(
        &mint,
        &ctx.accounts.sol_vault.key(),
        &ctx.accounts.sharing_config.to_account_info(),
    )?;
    require_keys_eq!(
        ctx.accounts.bonding_curve.key(),
        bonding_curve_pda(&mint),
        FeeKitError::BadBondingCurve
    );
    let curve = decode_bonding_curve(&ctx.accounts.bonding_curve.try_borrow_data()?)?;
    assert_sol_curve(&curve)?;
    require!(curve.complete, FeeKitError::NotGraduated);
    require_keys_eq!(
        curve.creator,
        ctx.accounts.sharing_config.key(),
        FeeKitError::FeeRecipientMismatch
    );

    let sharing = ctx.accounts.sharing_config.key();
    let amm_vault = amm_creator_vault_pda(&sharing);
    let pump_vault = pump_creator_vault_pda(&sharing);
    require_keys_eq!(
        ctx.accounts.coin_creator_vault_authority.key(),
        amm_vault,
        FeeKitError::BadAccountData
    );
    require_keys_eq!(
        ctx.accounts.coin_creator_vault_ata.key(),
        ata(&amm_vault, &Token::id(), &NATIVE_MINT),
        FeeKitError::BadAccountData
    );
    require_keys_eq!(
        ctx.accounts.pump_creator_vault.key(),
        pump_vault,
        FeeKitError::BadAccountData
    );
    require_keys_eq!(
        ctx.accounts.pump_creator_vault_ata.key(),
        ata(&pump_vault, &Token::id(), &NATIVE_MINT),
        FeeKitError::BadAccountData
    );
    require_keys_eq!(
        ctx.accounts.event_authority.key(),
        amm_event_authority_pda(),
        FeeKitError::BadAccountData
    );

    let accounts = vec![
        acc(&ctx.accounts.crank, true, true),
        acc(&ctx.accounts.quote_mint, false, false),
        acc(&ctx.accounts.quote_token_program, false, false),
        acc(&ctx.accounts.system_program, false, false),
        acc(&ctx.accounts.associated_token_program, false, false),
        acc(&ctx.accounts.sharing_config, false, false),
        acc(&ctx.accounts.coin_creator_vault_authority, false, true),
        acc(&ctx.accounts.coin_creator_vault_ata, false, true),
        acc(&ctx.accounts.pump_creator_vault, false, true),
        acc(&ctx.accounts.pump_creator_vault_ata, false, true),
        acc(&ctx.accounts.event_authority, false, false),
        acc(&ctx.accounts.amm_program, false, false),
    ];
    crate::pump::invoke(
        &ctx.accounts.amm_program.to_account_info(),
        &accounts,
        TRANSFER_CREATOR_FEES_TO_PUMP_V2.to_vec(),
        &[],
    )?;

    emit!(AmmFeesSwept {
        mint,
        slot: Clock::get()?.slot,
    });
    Ok(())
}

#[derive(Accounts)]
pub struct SweepAmmFees<'info> {
    #[account(mut)]
    pub crank: Signer<'info>,

    #[account(
        seeds = [CONFIG_SEED, config.mint.as_ref()],
        bump = config.bump,
    )]
    pub config: Account<'info, LaunchConfig>,

    #[account(
        seeds = [VAULT_SEED, config.mint.as_ref()],
        bump = config.sol_vault_bump,
    )]
    pub sol_vault: SystemAccount<'info>,

    /// CHECK: Canonical bonding curve. Graduation is read from its `complete` flag.
    pub bonding_curve: UncheckedAccount<'info>,

    /// CHECK: Sharing config. Also the PumpSwap coin creator after fee routing is locked.
    pub sharing_config: UncheckedAccount<'info>,

    /// CHECK: Wrapped SOL mint.
    #[account(address = NATIVE_MINT)]
    pub quote_mint: UncheckedAccount<'info>,

    /// CHECK: SPL Token program.
    #[account(address = Token::id())]
    pub quote_token_program: UncheckedAccount<'info>,

    pub system_program: Program<'info, System>,
    pub associated_token_program: Program<'info, AssociatedToken>,

    /// CHECK: PumpSwap creator-vault authority.
    #[account(mut)]
    pub coin_creator_vault_authority: UncheckedAccount<'info>,

    /// CHECK: PumpSwap creator-vault wrapped-SOL account.
    #[account(mut)]
    pub coin_creator_vault_ata: UncheckedAccount<'info>,

    /// CHECK: Pump creator vault that receives the swept fees.
    #[account(mut)]
    pub pump_creator_vault: UncheckedAccount<'info>,

    /// CHECK: Wrapped-SOL ATA of the pump creator vault.
    #[account(mut)]
    pub pump_creator_vault_ata: UncheckedAccount<'info>,

    /// CHECK: PumpSwap event authority.
    pub event_authority: UncheckedAccount<'info>,

    /// CHECK: PumpSwap program.
    #[account(address = PUMP_AMM_PROGRAM_ID)]
    pub amm_program: UncheckedAccount<'info>,
}
