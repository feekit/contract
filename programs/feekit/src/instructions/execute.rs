use anchor_lang::prelude::*;
use anchor_lang::solana_program::program::invoke_signed;
use anchor_lang::solana_program::system_instruction;
use anchor_spl::associated_token::AssociatedToken;
use anchor_spl::token::{self, SyncNative};
use anchor_spl::token_interface::{self, Mint, TokenAccount};

use crate::constants::*;
use crate::errors::FeeKitError;
use crate::events::KitExecuted;
use crate::instructions::checkpoint::load_curve;
use crate::instructions::lock::acc;
use crate::policy::{
    min_acceptable_tokens, planned_spend, price_q64, quote_in_to_reach_price, spendable_lamports,
    trigger_price,
};
use crate::pump::{
    amm_fee_config_pda, amm_global_config_pda, ata, canonical_pool_pda, decode_pool,
    global_volume_accumulator_pda, pool_v2_pda, pump_creator_vault_pda, pump_event_authority_pda,
    pump_fee_config_pda, pump_global_pda, require_fee_routing, token_amount,
    volume_accumulator_pda,
};
use crate::state::LaunchConfig;

pub fn execute_curve(ctx: Context<ExecuteCurve>, min_tokens_out: u64) -> Result<()> {
    let config = &ctx.accounts.config;
    require!(config.locked, FeeKitError::FeesNotLocked);
    require!(config.kit != KIT_RALLY, FeeKitError::KitDoesNotBuy);
    let mint = config.mint;
    let sol_vault = ctx.accounts.sol_vault.key();
    require_fee_routing(
        &mint,
        &sol_vault,
        &ctx.accounts.sharing_config.to_account_info(),
    )?;

    let curve = load_curve(&mint, &ctx.accounts.bonding_curve.to_account_info())?;
    require!(!curve.complete, FeeKitError::Graduated);
    require_keys_eq!(
        curve.creator,
        ctx.accounts.sharing_config.key(),
        FeeKitError::FeeRecipientMismatch
    );
    assert_curve_buy_accounts(&ctx, &sol_vault)?;

    let rent = Rent::get()?.minimum_balance(0);
    let spendable = spendable_lamports(ctx.accounts.sol_vault.lamports(), rent);
    let price = price_q64(curve.virtual_quote_reserves, curve.virtual_token_reserves)?;
    let slot = Clock::get()?.slot;
    if ctx.accounts.config.kit == KIT_FLOOR && price > ctx.accounts.config.high_water_price_q64 {
        ctx.accounts.config.high_water_price_q64 = price;
    }
    let config = &ctx.accounts.config;
    let mut spend = planned_spend(config, spendable, slot, price)?;
    if config.kit == KIT_FLOOR {
        spend = clamp_floor_buy(
            spend,
            config,
            curve.virtual_quote_reserves,
            curve.virtual_token_reserves,
        )?;
    }
    let floor = min_acceptable_tokens(
        spend,
        curve.virtual_quote_reserves,
        curve.virtual_token_reserves,
        config.slippage_bps,
    )?;
    require!(min_tokens_out >= floor, FeeKitError::SlippageExceeded);

    let before_tokens = ctx.accounts.vault_base_ata.amount;
    let before_lamports = ctx.accounts.sol_vault.lamports();
    let bump = [config.sol_vault_bump];
    let signer: &[&[u8]] = &[VAULT_SEED, mint.as_ref(), &bump];

    let mut data = BUY_EXACT_QUOTE_IN_V2.to_vec();
    data.extend_from_slice(&spend.to_le_bytes());
    data.extend_from_slice(&min_tokens_out.to_le_bytes());
    // OptionBool::Some(true). Lets the last buy complete the curve.
    data.push(1);
    {
        let accounts = vec![
            acc(&ctx.accounts.buy.global, false, false),
            acc(&ctx.accounts.base_mint, false, false),
            acc(&ctx.accounts.quote_mint, false, false),
            acc(&ctx.accounts.base_token_program, false, false),
            acc(&ctx.accounts.quote_token_program, false, false),
            acc(&ctx.accounts.associated_token_program, false, false),
            acc(&ctx.accounts.buy.fee_recipient, false, true),
            acc(
                &ctx.accounts.buy.associated_quote_fee_recipient,
                false,
                true,
            ),
            acc(&ctx.accounts.buy.buyback_fee_recipient, false, true),
            acc(
                &ctx.accounts.buy.associated_quote_buyback_fee_recipient,
                false,
                true,
            ),
            acc(&ctx.accounts.bonding_curve, false, true),
            acc(&ctx.accounts.buy.associated_base_bonding_curve, false, true),
            acc(
                &ctx.accounts.buy.associated_quote_bonding_curve,
                false,
                true,
            ),
            acc(&ctx.accounts.sol_vault, true, true),
            acc(&*ctx.accounts.vault_base_ata, false, true),
            acc(&ctx.accounts.buy.associated_quote_user, false, true),
            acc(&ctx.accounts.buy.creator_vault, false, true),
            acc(&ctx.accounts.buy.associated_creator_vault, false, true),
            acc(&ctx.accounts.sharing_config, false, false),
            acc(&ctx.accounts.buy.global_volume_accumulator, false, false),
            acc(&ctx.accounts.buy.user_volume_accumulator, false, true),
            acc(
                &ctx.accounts.buy.associated_user_volume_accumulator,
                false,
                true,
            ),
            acc(&ctx.accounts.buy.fee_config, false, false),
            acc(&ctx.accounts.buy.fee_program, false, false),
            acc(&ctx.accounts.system_program, false, false),
            acc(&ctx.accounts.buy.event_authority, false, false),
            acc(&ctx.accounts.pump_program, false, false),
        ];
        crate::pump::invoke(
            &ctx.accounts.pump_program.to_account_info(),
            &accounts,
            data,
            &[signer],
        )?;
    }

    let sol_spent = before_lamports
        .checked_sub(ctx.accounts.sol_vault.lamports())
        .ok_or(FeeKitError::ZeroOutput)?;
    require!(
        sol_spent <= spend.saturating_add(EXECUTION_RENT_BUFFER),
        FeeKitError::Overspend
    );
    let (bought, burned) = burn_purchased(
        &ctx.accounts.base_token_program.to_account_info(),
        &ctx.accounts.base_mint.to_account_info(),
        &ctx.accounts.vault_base_ata.to_account_info(),
        &ctx.accounts.sol_vault.to_account_info(),
        before_tokens,
        min_tokens_out,
        &[signer],
    )?;
    finish(
        &mut ctx.accounts.config,
        ctx.accounts.crank.key(),
        VENUE_CURVE,
        sol_spent,
        bought,
        burned,
    )
}

pub fn execute_swap(ctx: Context<ExecuteSwap>, min_tokens_out: u64) -> Result<()> {
    let config = &ctx.accounts.config;
    require!(config.locked, FeeKitError::FeesNotLocked);
    require!(config.kit != KIT_RALLY, FeeKitError::KitDoesNotBuy);
    require!(config.kit != KIT_GRADUATE, FeeKitError::GraduatePaysCreator);
    let mint = config.mint;
    let sol_vault = ctx.accounts.sol_vault.key();
    require_fee_routing(
        &mint,
        &sol_vault,
        &ctx.accounts.market.sharing_config.to_account_info(),
    )?;

    let curve = load_curve(&mint, &ctx.accounts.market.bonding_curve.to_account_info())?;
    require!(curve.complete, FeeKitError::NotGraduated);
    require_keys_eq!(
        curve.creator,
        ctx.accounts.market.sharing_config.key(),
        FeeKitError::FeeRecipientMismatch
    );
    let (quote_reserve, token_reserve) = assert_swap_accounts(&ctx)?;

    let rent = Rent::get()?.minimum_balance(0);
    let spendable = spendable_lamports(ctx.accounts.sol_vault.lamports(), rent);
    let price = price_q64(quote_reserve, token_reserve)?;
    let slot = Clock::get()?.slot;
    if ctx.accounts.config.kit == KIT_FLOOR && price > ctx.accounts.config.high_water_price_q64 {
        ctx.accounts.config.high_water_price_q64 = price;
    }
    let config = &ctx.accounts.config;
    let mut spend = planned_spend(config, spendable, slot, price)?;
    if config.kit == KIT_FLOOR {
        spend = clamp_floor_buy(spend, config, quote_reserve, token_reserve)?;
    }
    require_keys_eq!(
        ctx.accounts.vault_base_ata.key(),
        ata(&sol_vault, &ctx.accounts.base_token_program.key(), &mint),
        FeeKitError::BadAccountData
    );
    require_keys_eq!(
        ctx.accounts.vault_quote_ata.key(),
        ata(
            &sol_vault,
            &ctx.accounts.quote_token_program.key(),
            &NATIVE_MINT
        ),
        FeeKitError::BadAccountData
    );
    ensure_ata(
        &ctx.accounts.crank.to_account_info(),
        &ctx.accounts.vault_base_ata.to_account_info(),
        &ctx.accounts.sol_vault.to_account_info(),
        &ctx.accounts.base_mint.to_account_info(),
        &ctx.accounts.system_program.to_account_info(),
        &ctx.accounts.base_token_program.to_account_info(),
        &ctx.accounts.associated_token_program.to_account_info(),
    )?;
    ensure_ata(
        &ctx.accounts.crank.to_account_info(),
        &ctx.accounts.vault_quote_ata.to_account_info(),
        &ctx.accounts.sol_vault.to_account_info(),
        &ctx.accounts.quote_mint.to_account_info(),
        &ctx.accounts.system_program.to_account_info(),
        &ctx.accounts.quote_token_program.to_account_info(),
        &ctx.accounts.associated_token_program.to_account_info(),
    )?;
    let existing = ata_balance(&ctx.accounts.vault_quote_ata.to_account_info())?;
    let wrap = spend.saturating_sub(existing);
    let floor = min_acceptable_tokens(spend, quote_reserve, token_reserve, config.slippage_bps)?;
    require!(min_tokens_out >= floor, FeeKitError::SlippageExceeded);

    let bump = [config.sol_vault_bump];
    let signer: &[&[u8]] = &[VAULT_SEED, mint.as_ref(), &bump];
    let lamports_before = ctx.accounts.sol_vault.lamports();
    if wrap > 0 {
        let transfer = system_instruction::transfer(
            ctx.accounts.sol_vault.key,
            ctx.accounts.vault_quote_ata.to_account_info().key,
            wrap,
        );
        invoke_signed(
            &transfer,
            &[
                ctx.accounts.sol_vault.to_account_info(),
                ctx.accounts.vault_quote_ata.to_account_info(),
                ctx.accounts.system_program.to_account_info(),
            ],
            &[signer],
        )?;
        token::sync_native(CpiContext::new(
            ctx.accounts.quote_token_program.to_account_info(),
            SyncNative {
                account: ctx.accounts.vault_quote_ata.to_account_info(),
            },
        ))?;
    }
    let quote_wrapped = ata_balance(&ctx.accounts.vault_quote_ata.to_account_info())?;
    require!(quote_wrapped >= spend, FeeKitError::ZeroOutput);

    let quote_before = quote_wrapped;
    let base_before = ata_balance(&ctx.accounts.vault_base_ata.to_account_info())?;
    let mut data = AMM_BUY_EXACT_QUOTE_IN.to_vec();
    data.extend_from_slice(&spend.to_le_bytes());
    data.extend_from_slice(&min_tokens_out.to_le_bytes());
    data.push(0);
    {
        let accounts = vec![
            acc(&ctx.accounts.market.pool, false, true),
            acc(&ctx.accounts.sol_vault, true, true),
            acc(&ctx.accounts.market.global_config, false, false),
            acc(&ctx.accounts.base_mint, false, false),
            acc(&ctx.accounts.quote_mint, false, false),
            acc(&ctx.accounts.vault_base_ata, false, true),
            acc(&ctx.accounts.vault_quote_ata, false, true),
            acc(&ctx.accounts.market.pool_base_token_account, false, true),
            acc(&ctx.accounts.market.pool_quote_token_account, false, true),
            acc(&ctx.accounts.market.protocol_fee_recipient, false, false),
            acc(
                &ctx.accounts.market.protocol_fee_recipient_token_account,
                false,
                true,
            ),
            acc(&ctx.accounts.base_token_program, false, false),
            acc(&ctx.accounts.quote_token_program, false, false),
            acc(&ctx.accounts.system_program, false, false),
            acc(&ctx.accounts.associated_token_program, false, false),
            acc(&ctx.accounts.market.event_authority, false, false),
            acc(&ctx.accounts.market.amm_program, false, false),
            acc(&ctx.accounts.market.coin_creator_vault_ata, false, true),
            acc(
                &ctx.accounts.market.coin_creator_vault_authority,
                false,
                false,
            ),
            acc(&ctx.accounts.market.global_volume_accumulator, false, false),
            acc(&ctx.accounts.market.user_volume_accumulator, false, true),
            acc(&ctx.accounts.market.fee_config, false, false),
            acc(&ctx.accounts.market.fee_program, false, false),
            acc(&ctx.accounts.market.pool_v2, false, false),
            acc(&ctx.accounts.market.buyback_fee_recipient, false, false),
            acc(&ctx.accounts.market.buyback_fee_recipient_ata, false, true),
        ];
        crate::pump::invoke(
            &ctx.accounts.market.amm_program.to_account_info(),
            &accounts,
            data,
            &[signer],
        )?;
    }

    let lamports_spent = lamports_before
        .checked_sub(ctx.accounts.sol_vault.lamports())
        .ok_or(FeeKitError::Overflow)?;
    require!(
        lamports_spent <= wrap.saturating_add(EXECUTION_RENT_BUFFER),
        FeeKitError::Overspend
    );
    let sol_spent = quote_before
        .checked_sub(ata_balance(
            &ctx.accounts.vault_quote_ata.to_account_info(),
        )?)
        .ok_or(FeeKitError::Overflow)?;
    require!(sol_spent <= spend, FeeKitError::Overspend);

    let (bought, burned) = burn_purchased(
        &ctx.accounts.base_token_program.to_account_info(),
        &ctx.accounts.base_mint.to_account_info(),
        &ctx.accounts.vault_base_ata.to_account_info(),
        &ctx.accounts.sol_vault.to_account_info(),
        base_before,
        min_tokens_out,
        &[signer],
    )?;
    finish(
        &mut ctx.accounts.config,
        ctx.accounts.crank.key(),
        VENUE_SWAP,
        sol_spent,
        bought,
        burned,
    )
}

fn clamp_floor_buy(
    spend: u64,
    config: &LaunchConfig,
    quote_reserve: u64,
    token_reserve: u64,
) -> Result<u64> {
    let target = trigger_price(config.high_water_price_q64, config.drawdown_bps)
        .ok_or(FeeKitError::MarkNotReady)?;
    let needed = quote_in_to_reach_price(quote_reserve, token_reserve, target)?;
    require!(needed > 0, FeeKitError::DrawdownNotMet);
    let spend = spend.min(needed);
    require!(
        spend >= config.min_execute_lamports,
        FeeKitError::ThresholdNotMet
    );
    Ok(spend)
}

fn finish(
    config: &mut Account<LaunchConfig>,
    executor: Pubkey,
    venue: u8,
    sol_spent: u64,
    bought: u64,
    burned: u64,
) -> Result<()> {
    let slot = Clock::get()?.slot;
    config.sol_spent = config
        .sol_spent
        .checked_add(sol_spent)
        .ok_or(FeeKitError::Overflow)?;
    config.tokens_bought = config
        .tokens_bought
        .checked_add(bought)
        .ok_or(FeeKitError::Overflow)?;
    config.tokens_burned = config
        .tokens_burned
        .checked_add(burned)
        .ok_or(FeeKitError::Overflow)?;
    config.last_execution_slot = slot;
    config.execution_count = config
        .execution_count
        .checked_add(1)
        .ok_or(FeeKitError::Overflow)?;
    config.last_executor = executor;
    emit!(KitExecuted {
        mint: config.mint,
        kit: config.kit,
        venue,
        sol_spent,
        tokens_bought: bought,
        tokens_burned: burned,
        executor,
        slot,
    });
    Ok(())
}

fn burn_purchased<'info>(
    token_program: &AccountInfo<'info>,
    mint: &AccountInfo<'info>,
    from: &AccountInfo<'info>,
    authority: &AccountInfo<'info>,
    balance_before: u64,
    min_tokens_out: u64,
    signer: &[&[&[u8]]],
) -> Result<(u64, u64)> {
    let balance = ata_balance(from)?;
    let bought = balance
        .checked_sub(balance_before)
        .ok_or(FeeKitError::Overflow)?;
    require!(bought >= min_tokens_out, FeeKitError::SlippageExceeded);
    require!(bought > 0, FeeKitError::ZeroOutput);
    token_interface::burn(
        CpiContext::new_with_signer(
            token_program.clone(),
            token_interface::Burn {
                mint: mint.clone(),
                from: from.clone(),
                authority: authority.clone(),
            },
            signer,
        ),
        balance,
    )?;
    Ok((bought, balance))
}

fn ata_balance(account: &AccountInfo) -> Result<u64> {
    let data = account.try_borrow_data()?;
    require!(data.len() >= 72, FeeKitError::BadAccountData);
    let bytes: [u8; 8] = data[64..72]
        .try_into()
        .map_err(|_| error!(FeeKitError::BadAccountData))?;
    Ok(u64::from_le_bytes(bytes))
}

pub(crate) fn ensure_ata_pub<'info>(
    payer: &AccountInfo<'info>,
    ata_account: &AccountInfo<'info>,
    authority: &AccountInfo<'info>,
    mint: &AccountInfo<'info>,
    system_program: &AccountInfo<'info>,
    token_program: &AccountInfo<'info>,
    ata_program: &AccountInfo<'info>,
) -> Result<()> {
    ensure_ata(
        payer,
        ata_account,
        authority,
        mint,
        system_program,
        token_program,
        ata_program,
    )
}

fn ensure_ata<'info>(
    payer: &AccountInfo<'info>,
    ata_account: &AccountInfo<'info>,
    authority: &AccountInfo<'info>,
    mint: &AccountInfo<'info>,
    system_program: &AccountInfo<'info>,
    token_program: &AccountInfo<'info>,
    ata_program: &AccountInfo<'info>,
) -> Result<()> {
    if ata_account.data_is_empty() {
        anchor_spl::associated_token::create(CpiContext::new(
            ata_program.clone(),
            anchor_spl::associated_token::Create {
                payer: payer.clone(),
                associated_token: ata_account.clone(),
                authority: authority.clone(),
                mint: mint.clone(),
                system_program: system_program.clone(),
                token_program: token_program.clone(),
            },
        ))?;
    }
    Ok(())
}

fn assert_curve_buy_accounts(ctx: &Context<ExecuteCurve>, sol_vault: &Pubkey) -> Result<()> {
    let mint = ctx.accounts.config.mint;
    let quote_program = ctx.accounts.quote_token_program.key();
    let base_program = ctx.accounts.base_token_program.key();
    let curve = ctx.accounts.bonding_curve.key();
    let sharing = ctx.accounts.sharing_config.key();
    let creator_vault = pump_creator_vault_pda(&sharing);
    let user_volume = volume_accumulator_pda(&PUMP_PROGRAM_ID, sol_vault);

    require_keys_eq!(
        ctx.accounts.buy.global.key(),
        pump_global_pda(),
        FeeKitError::BadAccountData
    );
    require_keys_eq!(
        ctx.accounts.buy.event_authority.key(),
        pump_event_authority_pda(),
        FeeKitError::BadAccountData
    );
    require_keys_eq!(
        ctx.accounts.buy.fee_config.key(),
        pump_fee_config_pda(),
        FeeKitError::BadAccountData
    );
    require_keys_eq!(
        ctx.accounts.buy.global_volume_accumulator.key(),
        global_volume_accumulator_pda(&PUMP_PROGRAM_ID),
        FeeKitError::BadAccountData
    );
    require_keys_eq!(
        ctx.accounts.buy.user_volume_accumulator.key(),
        user_volume,
        FeeKitError::BadAccountData
    );
    require_keys_eq!(
        ctx.accounts.buy.creator_vault.key(),
        creator_vault,
        FeeKitError::BadAccountData
    );
    require_keys_eq!(
        ctx.accounts.buy.associated_base_bonding_curve.key(),
        ata(&curve, &base_program, &mint),
        FeeKitError::BadAccountData
    );
    require_keys_eq!(
        ctx.accounts.buy.associated_quote_bonding_curve.key(),
        ata(&curve, &quote_program, &NATIVE_MINT),
        FeeKitError::BadAccountData
    );
    require_keys_eq!(
        ctx.accounts.buy.associated_quote_user.key(),
        ata(sol_vault, &quote_program, &NATIVE_MINT),
        FeeKitError::BadAccountData
    );
    require_keys_eq!(
        ctx.accounts.buy.associated_creator_vault.key(),
        ata(&creator_vault, &quote_program, &NATIVE_MINT),
        FeeKitError::BadAccountData
    );
    require_keys_eq!(
        ctx.accounts.buy.associated_user_volume_accumulator.key(),
        ata(&user_volume, &quote_program, &NATIVE_MINT),
        FeeKitError::BadAccountData
    );
    require_keys_eq!(
        ctx.accounts.buy.associated_quote_fee_recipient.key(),
        ata(
            &ctx.accounts.buy.fee_recipient.key(),
            &quote_program,
            &NATIVE_MINT
        ),
        FeeKitError::BadAccountData
    );
    require_keys_eq!(
        ctx.accounts
            .buy
            .associated_quote_buyback_fee_recipient
            .key(),
        ata(
            &ctx.accounts.buy.buyback_fee_recipient.key(),
            &quote_program,
            &NATIVE_MINT
        ),
        FeeKitError::BadAccountData
    );
    Ok(())
}

fn assert_swap_accounts(ctx: &Context<ExecuteSwap>) -> Result<(u64, u64)> {
    let mint = ctx.accounts.config.mint;
    let sol_vault = ctx.accounts.sol_vault.key();
    let sharing = ctx.accounts.market.sharing_config.key();
    require_keys_eq!(
        ctx.accounts.market.pool.key(),
        canonical_pool_pda(&mint),
        FeeKitError::BadPool
    );
    require!(
        ctx.accounts.market.pool.owner == &PUMP_AMM_PROGRAM_ID,
        FeeKitError::BadPool
    );
    let pool = decode_pool(&ctx.accounts.market.pool.try_borrow_data()?)?;
    require_keys_eq!(pool.base_mint, mint, FeeKitError::BadPool);
    require_keys_eq!(pool.quote_mint, NATIVE_MINT, FeeKitError::QuoteNotSol);
    require_keys_eq!(
        pool.pool_base_token_account,
        ctx.accounts.market.pool_base_token_account.key(),
        FeeKitError::BadPool
    );
    require_keys_eq!(
        pool.pool_quote_token_account,
        ctx.accounts.market.pool_quote_token_account.key(),
        FeeKitError::BadPool
    );
    require_keys_eq!(
        pool.coin_creator,
        sharing,
        FeeKitError::FeeRecipientMismatch
    );

    let quote_program = ctx.accounts.quote_token_program.key();
    let amm_vault = crate::pump::amm_creator_vault_pda(&sharing);
    let user_volume = volume_accumulator_pda(&PUMP_AMM_PROGRAM_ID, &sol_vault);
    require_keys_eq!(
        ctx.accounts.market.global_config.key(),
        amm_global_config_pda(),
        FeeKitError::BadAccountData
    );
    require_keys_eq!(
        ctx.accounts.market.event_authority.key(),
        crate::pump::amm_event_authority_pda(),
        FeeKitError::BadAccountData
    );
    require_keys_eq!(
        ctx.accounts.market.fee_config.key(),
        amm_fee_config_pda(),
        FeeKitError::BadAccountData
    );
    require_keys_eq!(
        ctx.accounts.market.coin_creator_vault_authority.key(),
        amm_vault,
        FeeKitError::BadAccountData
    );
    require_keys_eq!(
        ctx.accounts.market.coin_creator_vault_ata.key(),
        ata(&amm_vault, &quote_program, &NATIVE_MINT),
        FeeKitError::BadAccountData
    );
    require_keys_eq!(
        ctx.accounts.market.global_volume_accumulator.key(),
        global_volume_accumulator_pda(&PUMP_AMM_PROGRAM_ID),
        FeeKitError::BadAccountData
    );
    require_keys_eq!(
        ctx.accounts.market.user_volume_accumulator.key(),
        user_volume,
        FeeKitError::BadAccountData
    );
    require_keys_eq!(
        ctx.accounts
            .market
            .protocol_fee_recipient_token_account
            .key(),
        ata(
            &ctx.accounts.market.protocol_fee_recipient.key(),
            &quote_program,
            &NATIVE_MINT
        ),
        FeeKitError::BadAccountData
    );

    require_keys_eq!(
        ctx.accounts.market.pool_v2.key(),
        pool_v2_pda(&mint),
        FeeKitError::BadPool
    );
    require_keys_eq!(
        ctx.accounts.market.buyback_fee_recipient_ata.key(),
        ata(
            &ctx.accounts.market.buyback_fee_recipient.key(),
            &quote_program,
            &NATIVE_MINT
        ),
        FeeKitError::BadAccountData
    );

    let quote_reserve = token_amount(
        &ctx.accounts
            .market
            .pool_quote_token_account
            .try_borrow_data()?,
        &NATIVE_MINT,
    )?
    .saturating_add(pool.virtual_quote_reserves);
    let token_reserve = token_amount(
        &ctx.accounts
            .market
            .pool_base_token_account
            .try_borrow_data()?,
        &mint,
    )?;
    Ok((quote_reserve, token_reserve))
}

#[derive(Accounts)]
pub struct ExecuteCurve<'info> {
    #[account(mut)]
    pub crank: Signer<'info>,

    #[account(
        mut,
        seeds = [CONFIG_SEED, config.mint.as_ref()],
        bump = config.bump,
    )]
    pub config: Box<Account<'info, LaunchConfig>>,

    #[account(
        mut,
        seeds = [VAULT_SEED, config.mint.as_ref()],
        bump = config.sol_vault_bump,
    )]
    pub sol_vault: SystemAccount<'info>,

    #[account(
        mut,
        constraint = base_mint.key() == config.mint,
        constraint = base_mint.to_account_info().owner == &config.base_token_program,
    )]
    pub base_mint: InterfaceAccount<'info, Mint>,

    #[account(
        init_if_needed,
        payer = crank,
        associated_token::mint = base_mint,
        associated_token::authority = sol_vault,
        associated_token::token_program = base_token_program,
    )]
    pub vault_base_ata: Box<InterfaceAccount<'info, TokenAccount>>,

    /// CHECK: Wrapped SOL mint.
    #[account(address = NATIVE_MINT)]
    pub quote_mint: UncheckedAccount<'info>,

    /// CHECK: Token program recorded when the vault was initialized.
    #[account(address = config.base_token_program)]
    pub base_token_program: UncheckedAccount<'info>,

    /// CHECK: SPL Token program for wrapped SOL.
    #[account(address = anchor_spl::token::ID)]
    pub quote_token_program: UncheckedAccount<'info>,

    pub associated_token_program: Program<'info, AssociatedToken>,

    /// CHECK: Canonical bonding curve.
    #[account(mut)]
    pub bonding_curve: UncheckedAccount<'info>,

    /// CHECK: Sharing config locked to the SOL vault.
    pub sharing_config: UncheckedAccount<'info>,

    pub system_program: Program<'info, System>,

    /// CHECK: Pump program.
    #[account(address = PUMP_PROGRAM_ID)]
    pub pump_program: UncheckedAccount<'info>,

    pub buy: CurveBuy<'info>,
}

/// Pump buy accounts that do not depend on the FeeKit config.
/// Split out so account validation stays inside the SBF stack limit.
#[derive(Accounts)]
pub struct CurveBuy<'info> {
    /// CHECK: Pump global PDA.
    pub global: UncheckedAccount<'info>,

    /// CHECK: Pump fee recipient. Pump checks it against global config.
    #[account(mut)]
    pub fee_recipient: UncheckedAccount<'info>,

    /// CHECK: Wrapped-SOL ATA of `fee_recipient`.
    #[account(mut)]
    pub associated_quote_fee_recipient: UncheckedAccount<'info>,

    /// CHECK: Pump buyback fee recipient.
    #[account(mut)]
    pub buyback_fee_recipient: UncheckedAccount<'info>,

    /// CHECK: Wrapped-SOL ATA of `buyback_fee_recipient`.
    #[account(mut)]
    pub associated_quote_buyback_fee_recipient: UncheckedAccount<'info>,

    /// CHECK: Bonding curve base-token ATA.
    #[account(mut)]
    pub associated_base_bonding_curve: UncheckedAccount<'info>,

    /// CHECK: Bonding curve wrapped-SOL ATA.
    #[account(mut)]
    pub associated_quote_bonding_curve: UncheckedAccount<'info>,

    /// CHECK: Vault wrapped-SOL ATA. Native SOL buys ignore its balance.
    #[account(mut)]
    pub associated_quote_user: UncheckedAccount<'info>,

    /// CHECK: Pump creator vault of the sharing config.
    #[account(mut)]
    pub creator_vault: UncheckedAccount<'info>,

    /// CHECK: Wrapped-SOL ATA of the creator vault.
    #[account(mut)]
    pub associated_creator_vault: UncheckedAccount<'info>,

    /// CHECK: Pump global volume accumulator.
    pub global_volume_accumulator: UncheckedAccount<'info>,

    /// CHECK: Pump volume accumulator for the SOL vault. Pump may initialize it.
    #[account(mut)]
    pub user_volume_accumulator: UncheckedAccount<'info>,

    /// CHECK: Wrapped-SOL ATA of the user volume accumulator.
    #[account(mut)]
    pub associated_user_volume_accumulator: UncheckedAccount<'info>,

    /// CHECK: Pump fee config PDA.
    pub fee_config: UncheckedAccount<'info>,

    /// CHECK: Pump Fees program.
    #[account(address = FEE_PROGRAM_ID)]
    pub fee_program: UncheckedAccount<'info>,

    /// CHECK: Pump event authority.
    pub event_authority: UncheckedAccount<'info>,
}

#[derive(Accounts)]
pub struct ExecuteSwap<'info> {
    #[account(mut)]
    pub crank: Signer<'info>,

    #[account(
        mut,
        seeds = [CONFIG_SEED, config.mint.as_ref()],
        bump = config.bump,
    )]
    pub config: Box<Account<'info, LaunchConfig>>,

    #[account(
        mut,
        seeds = [VAULT_SEED, config.mint.as_ref()],
        bump = config.sol_vault_bump,
    )]
    pub sol_vault: SystemAccount<'info>,

    #[account(
        mut,
        constraint = base_mint.key() == config.mint,
        constraint = base_mint.to_account_info().owner == &config.base_token_program,
    )]
    pub base_mint: InterfaceAccount<'info, Mint>,

    /// CHECK: Vault base ATA. Created in the handler when it is missing.
    #[account(mut)]
    pub vault_base_ata: UncheckedAccount<'info>,

    /// CHECK: Wrapped SOL mint.
    #[account(address = NATIVE_MINT)]
    pub quote_mint: UncheckedAccount<'info>,

    /// CHECK: Vault wrapped-SOL ATA. Created in the handler when it is missing.
    #[account(mut)]
    pub vault_quote_ata: UncheckedAccount<'info>,

    /// CHECK: Token program recorded at initialization.
    #[account(address = config.base_token_program)]
    pub base_token_program: UncheckedAccount<'info>,

    /// CHECK: SPL Token program for wrapped SOL.
    #[account(address = anchor_spl::token::ID)]
    pub quote_token_program: UncheckedAccount<'info>,

    pub system_program: Program<'info, System>,
    pub associated_token_program: Program<'info, AssociatedToken>,

    pub market: SwapMarket<'info>,
}

/// PumpSwap accounts that do not depend on the FeeKit config.
#[derive(Accounts)]
pub struct SwapMarket<'info> {
    /// CHECK: Canonical bonding curve. `complete` selects this venue.
    pub bonding_curve: UncheckedAccount<'info>,

    /// CHECK: Sharing config locked to the SOL vault.
    pub sharing_config: UncheckedAccount<'info>,

    /// CHECK: Canonical PumpSwap pool.
    #[account(mut)]
    pub pool: UncheckedAccount<'info>,

    /// CHECK: PumpSwap global config.
    pub global_config: UncheckedAccount<'info>,

    /// CHECK: Pool base vault. The spot price is read from its balance.
    #[account(mut)]
    pub pool_base_token_account: UncheckedAccount<'info>,

    /// CHECK: Pool wrapped-SOL vault.
    #[account(mut)]
    pub pool_quote_token_account: UncheckedAccount<'info>,

    /// CHECK: PumpSwap protocol fee recipient.
    pub protocol_fee_recipient: UncheckedAccount<'info>,

    /// CHECK: Wrapped-SOL ATA of the protocol fee recipient.
    #[account(mut)]
    pub protocol_fee_recipient_token_account: UncheckedAccount<'info>,

    /// CHECK: PumpSwap event authority.
    pub event_authority: UncheckedAccount<'info>,

    /// CHECK: PumpSwap program.
    #[account(address = PUMP_AMM_PROGRAM_ID)]
    pub amm_program: UncheckedAccount<'info>,

    /// CHECK: PumpSwap creator-vault wrapped-SOL account.
    #[account(mut)]
    pub coin_creator_vault_ata: UncheckedAccount<'info>,

    /// CHECK: PumpSwap creator-vault authority.
    pub coin_creator_vault_authority: UncheckedAccount<'info>,

    /// CHECK: PumpSwap global volume accumulator.
    pub global_volume_accumulator: UncheckedAccount<'info>,

    /// CHECK: PumpSwap volume accumulator for the SOL vault.
    #[account(mut)]
    pub user_volume_accumulator: UncheckedAccount<'info>,

    /// CHECK: PumpSwap fee config PDA.
    pub fee_config: UncheckedAccount<'info>,

    /// CHECK: Pump Fees program.
    #[account(address = FEE_PROGRAM_ID)]
    pub fee_program: UncheckedAccount<'info>,

    /// CHECK: PumpSwap pool-v2 PDA. Required when the pool has a coin creator.
    pub pool_v2: UncheckedAccount<'info>,

    /// CHECK: PumpSwap buyback fee recipient. Pump checks it against global config.
    pub buyback_fee_recipient: UncheckedAccount<'info>,

    /// CHECK: Wrapped-SOL ATA of `buyback_fee_recipient`.
    #[account(mut)]
    pub buyback_fee_recipient_ata: UncheckedAccount<'info>,
}
