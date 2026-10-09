use anchor_lang::prelude::*;

use crate::constants::*;
use crate::errors::FeeKitError;
use crate::events::{PriceCheckpoint, RallyOpened, RallyResolved};
use crate::policy::{on_checkpoint, on_rally_checkpoint, price_q64, spendable_lamports};
use crate::pump::{
    assert_sol_curve, bonding_curve_pda, canonical_pool_pda, decode_bonding_curve, decode_pool,
    require_fee_routing, token_amount,
};
use crate::state::LaunchConfig;

pub fn handler(ctx: Context<Checkpoint>) -> Result<()> {
    let config = &ctx.accounts.config;
    require!(config.locked, FeeKitError::FeesNotLocked);
    require_fee_routing(
        &config.mint,
        &config.sol_vault,
        &ctx.accounts.sharing_config.to_account_info(),
    )?;

    let curve = load_curve(&config.mint, &ctx.accounts.bonding_curve)?;
    require_keys_eq!(
        curve.creator,
        ctx.accounts.sharing_config.key(),
        FeeKitError::FeeRecipientMismatch
    );

    let (quote_reserve, token_reserve) = if curve.complete {
        require!(
            ctx.remaining_accounts.len() == 3,
            FeeKitError::BadRemainingAccounts
        );
        let pool = &ctx.remaining_accounts[0];
        let base_vault = &ctx.remaining_accounts[1];
        let quote_vault = &ctx.remaining_accounts[2];
        require_keys_eq!(
            *pool.key,
            canonical_pool_pda(&config.mint),
            FeeKitError::BadPool
        );
        require!(pool.owner == &PUMP_AMM_PROGRAM_ID, FeeKitError::BadPool);
        let view = decode_pool(&pool.try_borrow_data()?)?;
        require_keys_eq!(view.base_mint, config.mint, FeeKitError::BadPool);
        require_keys_eq!(view.quote_mint, NATIVE_MINT, FeeKitError::QuoteNotSol);
        require_keys_eq!(
            view.pool_base_token_account,
            *base_vault.key,
            FeeKitError::BadPool
        );
        require_keys_eq!(
            view.pool_quote_token_account,
            *quote_vault.key,
            FeeKitError::BadPool
        );
        require_keys_eq!(
            view.coin_creator,
            ctx.accounts.sharing_config.key(),
            FeeKitError::FeeRecipientMismatch
        );
        (
            token_amount(&quote_vault.try_borrow_data()?, &NATIVE_MINT)?,
            token_amount(&base_vault.try_borrow_data()?, &config.mint)?,
        )
    } else {
        require!(
            ctx.remaining_accounts.is_empty(),
            FeeKitError::BadRemainingAccounts
        );
        (curve.virtual_quote_reserves, curve.virtual_token_reserves)
    };

    let price = price_q64(quote_reserve, token_reserve)?;
    let slot = Clock::get()?.slot;
    if ctx.accounts.config.is_rally() {
        let before_status = ctx.accounts.config.rally_status;
        let before_cohort = ctx.accounts.config.rally_cohort;
        let rent = Rent::get()?.minimum_balance(0);
        let spendable = spendable_lamports(ctx.accounts.sol_vault.lamports(), rent);
        let config = &mut ctx.accounts.config;
        on_rally_checkpoint(config, price, slot, spendable)?;
        if config.rally_status == RALLY_OPEN && before_status != RALLY_OPEN {
            emit!(RallyOpened {
                mint: config.mint,
                cohort: config.rally_cohort,
                target_q64: config.rally_target_q64,
                deadline_slot: config.rally_deadline_slot,
            });
        }
        if before_status == RALLY_OPEN
            && (config.rally_status == RALLY_WON || config.rally_status == RALLY_LOST)
        {
            emit!(RallyResolved {
                mint: config.mint,
                cohort: before_cohort,
                won: config.rally_status == RALLY_WON,
                pot: config.rally_pot,
                slot,
            });
        }
    } else {
        let config = &mut ctx.accounts.config;
        on_checkpoint(config, price, slot)?;
    }

    let config = &ctx.accounts.config;
    emit!(PriceCheckpoint {
        mint: config.mint,
        price_q64: price,
        high_water_price_q64: config.high_water_price_q64,
        armed_slot: config.armed_slot,
        confirm_slot: config.confirm_slot,
        slot,
    });
    Ok(())
}

pub fn load_curve(mint: &Pubkey, account: &AccountInfo) -> Result<crate::pump::CurveView> {
    require_keys_eq!(
        *account.key,
        bonding_curve_pda(mint),
        FeeKitError::BadBondingCurve
    );
    require!(
        account.owner == &PUMP_PROGRAM_ID,
        FeeKitError::BadBondingCurve
    );
    let curve = decode_bonding_curve(&account.try_borrow_data()?)?;
    assert_sol_curve(&curve)?;
    Ok(curve)
}

#[derive(Accounts)]
pub struct Checkpoint<'info> {
    pub crank: Signer<'info>,

    #[account(
        mut,
        seeds = [CONFIG_SEED, config.mint.as_ref()],
        bump = config.bump,
    )]
    pub config: Account<'info, LaunchConfig>,

    /// CHECK: System-owned SOL vault. Rally reads it to snapshot a winning pot.
    #[account(
        mut,
        seeds = [VAULT_SEED, config.mint.as_ref()],
        bump = config.sol_vault_bump,
    )]
    pub sol_vault: UncheckedAccount<'info>,

    /// CHECK: Canonical bonding curve.
    pub bonding_curve: UncheckedAccount<'info>,

    /// CHECK: Sharing config locked to this launch.
    pub sharing_config: UncheckedAccount<'info>,
}
