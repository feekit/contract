use anchor_lang::prelude::*;
use anchor_spl::associated_token::AssociatedToken;
use anchor_spl::token::Token;
use anchor_spl::token_interface::Mint;

use crate::constants::*;
use crate::errors::FeeKitError;
use crate::events::FeesLocked;
use crate::pump::{
    amm_creator_vault_pda, amm_event_authority_pda, assert_fee_routing, assert_sol_curve, ata,
    bonding_curve_pda, canonical_pool_pda, decode_bonding_curve, fee_event_authority_pda,
    pump_creator_vault_pda, pump_event_authority_pda, pump_global_pda, sharing_config_pda,
    update_fee_shares_v2_data, IxAccount,
};
use crate::state::LaunchConfig;

pub fn handler(ctx: Context<LockFees>) -> Result<()> {
    let config = &ctx.accounts.config;
    require!(!config.locked, FeeKitError::AlreadyLocked);
    require_keys_eq!(
        ctx.accounts.creator.key(),
        config.creator,
        FeeKitError::NotCreator
    );

    let mint = config.mint;
    let sol_vault = config.sol_vault;
    require_keys_eq!(
        ctx.accounts.bonding_curve.key(),
        bonding_curve_pda(&mint),
        FeeKitError::BadBondingCurve
    );
    require_keys_eq!(
        ctx.accounts.sharing_config.key(),
        sharing_config_pda(&mint),
        FeeKitError::FeeRecipientMismatch
    );
    require_keys_eq!(
        ctx.accounts.global.key(),
        pump_global_pda(),
        FeeKitError::BadAccountData
    );
    require_keys_eq!(
        ctx.accounts.pump_event_authority.key(),
        pump_event_authority_pda(),
        FeeKitError::BadAccountData
    );
    require_keys_eq!(
        ctx.accounts.fee_event_authority.key(),
        fee_event_authority_pda(),
        FeeKitError::BadAccountData
    );
    require_keys_eq!(
        ctx.accounts.amm_event_authority.key(),
        amm_event_authority_pda(),
        FeeKitError::BadAccountData
    );

    let creator_vault = pump_creator_vault_pda(&ctx.accounts.sharing_config.key());
    let amm_vault = amm_creator_vault_pda(&ctx.accounts.sharing_config.key());
    require_keys_eq!(
        ctx.accounts.pump_creator_vault.key(),
        creator_vault,
        FeeKitError::BadAccountData
    );
    require_keys_eq!(
        ctx.accounts.pump_creator_vault_ata.key(),
        ata(&creator_vault, &Token::id(), &NATIVE_MINT),
        FeeKitError::BadAccountData
    );
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

    let curve = decode_bonding_curve(&ctx.accounts.bonding_curve.try_borrow_data()?)?;
    assert_sol_curve(&curve)?;
    let sharing_key = ctx.accounts.sharing_config.key();
    require!(
        curve.creator == config.creator || curve.creator == sharing_key,
        FeeKitError::NotCreator
    );

    let mut create_accounts = vec![
        acc(&ctx.accounts.fee_event_authority, false, false),
        acc(&ctx.accounts.fee_program, false, false),
        acc(&ctx.accounts.creator, true, true),
        acc(&ctx.accounts.global, false, false),
        acc(&ctx.accounts.mint, false, false),
        acc(&ctx.accounts.sharing_config, false, true),
        acc(&ctx.accounts.system_program, false, false),
        acc(&ctx.accounts.bonding_curve, false, true),
        acc(&ctx.accounts.pump_program, false, false),
        acc(&ctx.accounts.pump_event_authority, false, false),
    ];
    // create_fee_sharing_config always has the pool slot. Before graduation the
    // live program treats the fee program id as "no pool".
    if curve.complete {
        let pool = ctx.accounts.pool.as_ref().ok_or(FeeKitError::BadPool)?;
        require!(pool.is_writable, FeeKitError::BadPool);
        require_keys_eq!(pool.key(), canonical_pool_pda(&mint), FeeKitError::BadPool);
        create_accounts.push(acc(pool, false, true));
    } else {
        require!(ctx.accounts.pool.is_none(), FeeKitError::BadRemainingAccounts);
        create_accounts.push(acc(&ctx.accounts.fee_program, false, false));
    }
    create_accounts.push(acc(&ctx.accounts.amm_program, false, false));
    create_accounts.push(acc(&ctx.accounts.amm_event_authority, false, false));

    crate::pump::invoke(
        &ctx.accounts.fee_program.to_account_info(),
        &create_accounts,
        CREATE_FEE_SHARING_CONFIG.to_vec(),
        &[],
    )?;

    // The creator is both the admin and the current 100% shareholder. Pending fees
    // are paid to that shareholder before the vault becomes the only recipient.
    let update_accounts = vec![
        acc(&ctx.accounts.fee_event_authority, false, false),
        acc(&ctx.accounts.fee_program, false, false),
        acc(&ctx.accounts.creator, true, true),
        acc(&ctx.accounts.global, false, false),
        acc(&ctx.accounts.mint, false, false),
        acc(&ctx.accounts.sharing_config, false, true),
        acc(&ctx.accounts.bonding_curve, false, false),
        acc(&ctx.accounts.pump_creator_vault, false, true),
        acc(&ctx.accounts.pump_creator_vault_ata, false, true),
        acc(&ctx.accounts.system_program, false, false),
        acc(&ctx.accounts.pump_program, false, false),
        acc(&ctx.accounts.pump_event_authority, false, false),
        acc(&ctx.accounts.amm_program, false, false),
        acc(&ctx.accounts.amm_event_authority, false, false),
        acc(&ctx.accounts.quote_mint, false, false),
        acc(&ctx.accounts.quote_token_program, false, false),
        acc(&ctx.accounts.associated_token_program, false, false),
        acc(&ctx.accounts.coin_creator_vault_authority, false, true),
        acc(&ctx.accounts.coin_creator_vault_ata, false, true),
        acc(&ctx.accounts.creator, true, true),
    ];
    crate::pump::invoke(
        &ctx.accounts.fee_program.to_account_info(),
        &update_accounts,
        update_fee_shares_v2_data(&sol_vault, &ctx.accounts.treasury.key()),
        &[],
    )?;

    require!(
        ctx.accounts.treasury.lamports() > 0,
        FeeKitError::PlatformNotReady
    );
    assert_fee_routing(
        &ctx.accounts.sharing_config.try_borrow_data()?,
        &mint,
        &sol_vault,
    )?;
    let curve = decode_bonding_curve(&ctx.accounts.bonding_curve.try_borrow_data()?)?;
    require_keys_eq!(
        curve.creator,
        sharing_key,
        FeeKitError::FeeRecipientMismatch
    );

    let config = &mut ctx.accounts.config;
    config.locked = true;
    config.sharing_config = sharing_key;

    emit!(FeesLocked {
        mint,
        sol_vault,
        sharing_config: sharing_key,
    });
    Ok(())
}

pub(crate) fn acc<'info>(
    account: &impl ToAccountInfo<'info>,
    signer: bool,
    writable: bool,
) -> IxAccount<'info> {
    IxAccount {
        info: account.to_account_info(),
        signer,
        writable,
    }
}

#[derive(Accounts)]
pub struct LockFees<'info> {
    #[account(mut)]
    pub creator: Signer<'info>,

    #[account(
        mut,
        seeds = [CONFIG_SEED, config.mint.as_ref()],
        bump = config.bump,
    )]
    pub config: Account<'info, LaunchConfig>,

    #[account(
        seeds = [VAULT_SEED, config.mint.as_ref()],
        bump = config.sol_vault_bump,
    )]
    pub sol_vault: SystemAccount<'info>,

    #[account(constraint = mint.key() == config.mint)]
    pub mint: InterfaceAccount<'info, Mint>,

    /// CHECK: Pump global config, seeds ["global"].
    pub global: UncheckedAccount<'info>,

    /// CHECK: Pump Fees sharing-config PDA. Created or updated by the CPI.
    #[account(mut)]
    pub sharing_config: UncheckedAccount<'info>,

    /// CHECK: Canonical bonding curve. create_fee_sharing_config rewrites its creator.
    #[account(mut)]
    pub bonding_curve: UncheckedAccount<'info>,

    pub system_program: Program<'info, System>,

    /// CHECK: Pump program.
    #[account(address = PUMP_PROGRAM_ID)]
    pub pump_program: UncheckedAccount<'info>,

    /// CHECK: Pump event authority.
    pub pump_event_authority: UncheckedAccount<'info>,

    /// CHECK: Pump Fees event authority.
    pub fee_event_authority: UncheckedAccount<'info>,

    /// CHECK: Pump Fees program.
    #[account(address = FEE_PROGRAM_ID)]
    pub fee_program: UncheckedAccount<'info>,

    /// CHECK: Pump creator-fee vault for the sharing config.
    #[account(mut)]
    pub pump_creator_vault: UncheckedAccount<'info>,

    /// CHECK: Wrapped-SOL ATA of the pump creator vault. Unused for native SOL, still required.
    #[account(mut)]
    pub pump_creator_vault_ata: UncheckedAccount<'info>,

    /// CHECK: Wrapped SOL mint.
    #[account(address = NATIVE_MINT)]
    pub quote_mint: UncheckedAccount<'info>,

    /// CHECK: SPL Token program. Wrapped SOL uses it.
    #[account(address = Token::id())]
    pub quote_token_program: UncheckedAccount<'info>,

    pub associated_token_program: Program<'info, AssociatedToken>,

    /// CHECK: PumpSwap creator-vault authority for the sharing config.
    #[account(mut)]
    pub coin_creator_vault_authority: UncheckedAccount<'info>,

    /// CHECK: Wrapped-SOL ATA of the PumpSwap creator vault.
    #[account(mut)]
    pub coin_creator_vault_ata: UncheckedAccount<'info>,

    /// CHECK: PumpSwap program.
    #[account(address = PUMP_AMM_PROGRAM_ID)]
    pub amm_program: UncheckedAccount<'info>,

    /// CHECK: PumpSwap event authority.
    pub amm_event_authority: UncheckedAccount<'info>,

    #[account(
        seeds = [PLATFORM_SEED],
        bump = platform.bump,
    )]
    pub platform: Account<'info, crate::state::PlatformConfig>,

    /// CHECK: System-owned FeeKit treasury. Pump pays the platform share here.
    #[account(
        seeds = [TREASURY_SEED],
        bump = platform.treasury_bump,
    )]
    pub treasury: UncheckedAccount<'info>,

    /// Canonical PumpSwap pool after graduation. Before graduation, leave this unset.
    /// CHECK: When present, it must be the canonical pool PDA for this mint and wrapped SOL.
    pub pool: Option<UncheckedAccount<'info>>,
}
