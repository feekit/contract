use anchor_lang::prelude::*;
use anchor_lang::solana_program::program::invoke_signed;
use anchor_lang::solana_program::system_instruction;
use anchor_spl::associated_token::AssociatedToken;
use anchor_spl::token_interface::{self, Mint, TokenAccount, TokenInterface, TransferChecked};

use crate::constants::*;
use crate::errors::FeeKitError;
use crate::events::{RallyClaimed, RallyLocked};
use crate::instructions::execute::ensure_ata_pub;
use crate::policy::{note_lock, position_weight, rally_payout, release_lock};
use crate::pump::ata;
use crate::state::{LaunchConfig, RallyPosition};

pub fn lock(ctx: Context<LockRally>, amount: u64) -> Result<()> {
    let config = &ctx.accounts.config;
    require!(config.locked, FeeKitError::FeesNotLocked);
    require!(config.is_rally(), FeeKitError::InvalidParams);
    require!(config.rally_status == RALLY_OPEN, FeeKitError::RallyNotOpen);
    let slot = Clock::get()?.slot;
    require!(slot <= config.rally_deadline_slot, FeeKitError::RallyNotOpen);
    require!(amount > 0, FeeKitError::InvalidParams);

    let mint = ctx.accounts.mint.key();
    let escrow = ctx.accounts.escrow.key();
    let token_program = ctx.accounts.token_program.key();
    require_keys_eq!(
        ctx.accounts.escrow_ata.key(),
        ata(&escrow, &token_program, &mint),
        FeeKitError::BadAccountData
    );
    require_keys_eq!(
        *ctx.accounts.mint.to_account_info().owner,
        token_program,
        FeeKitError::UnsupportedTokenProgram
    );

    ensure_ata_pub(
        &ctx.accounts.holder.to_account_info(),
        &ctx.accounts.escrow_ata.to_account_info(),
        &ctx.accounts.escrow.to_account_info(),
        &ctx.accounts.mint.to_account_info(),
        &ctx.accounts.system_program.to_account_info(),
        &ctx.accounts.token_program.to_account_info(),
        &ctx.accounts.associated_token_program.to_account_info(),
    )?;

    token_interface::transfer_checked(
        CpiContext::new(
            ctx.accounts.token_program.to_account_info(),
            TransferChecked {
                from: ctx.accounts.holder_ata.to_account_info(),
                mint: ctx.accounts.mint.to_account_info(),
                to: ctx.accounts.escrow_ata.to_account_info(),
                authority: ctx.accounts.holder.to_account_info(),
            },
        ),
        amount,
        ctx.accounts.mint.decimals,
    )?;

    let cohort = config.rally_cohort;
    let owner = ctx.accounts.holder.key();
    {
        let position = &mut ctx.accounts.position;
        position.bump = ctx.bumps.position;
        position.cohort = cohort;
        position.owner = owner;
        position.amount = amount;
        position.lock_slot = slot;
    }
    note_lock(&mut ctx.accounts.config, amount, slot)?;

    emit!(RallyLocked {
        mint,
        cohort,
        owner,
        amount,
        slot,
    });
    Ok(())
}

pub fn claim(ctx: Context<ClaimRally>) -> Result<()> {
    let (amount, lock_slot, payout, cohort) = {
        let config = &ctx.accounts.config;
        require!(config.locked, FeeKitError::FeesNotLocked);
        require!(config.is_rally(), FeeKitError::InvalidParams);
        require!(
            config.rally_status == RALLY_WON || config.rally_status == RALLY_LOST,
            FeeKitError::RallyNotResolved
        );
        let position = &ctx.accounts.position;
        require_keys_eq!(
            position.owner,
            ctx.accounts.holder.key(),
            FeeKitError::RallyCohortMismatch
        );
        require!(
            position.cohort == config.rally_cohort,
            FeeKitError::RallyCohortMismatch
        );
        let amount = position.amount;
        let lock_slot = position.lock_slot;
        let payout = if config.rally_status == RALLY_WON {
            let weight = position_weight(amount, lock_slot, config.rally_settle_slot)?;
            let last = config.rally_locked == amount;
            rally_payout(
                config.rally_pot,
                config.rally_paid,
                weight,
                config.rally_weight,
                last,
            )?
        } else {
            0
        };
        (amount, lock_slot, payout, config.rally_cohort)
    };

    let mint_key = ctx.accounts.mint.key();
    let bump = ctx.bumps.escrow;
    let bump_seed = [bump];
    let mint_ref = mint_key;
    let seeds: &[&[u8]] = &[ESCROW_SEED, mint_ref.as_ref(), &bump_seed];

    token_interface::transfer_checked(
        CpiContext::new_with_signer(
            ctx.accounts.token_program.to_account_info(),
            TransferChecked {
                from: ctx.accounts.escrow_ata.to_account_info(),
                mint: ctx.accounts.mint.to_account_info(),
                to: ctx.accounts.holder_ata.to_account_info(),
                authority: ctx.accounts.escrow.to_account_info(),
            },
            &[seeds],
        ),
        amount,
        ctx.accounts.mint.decimals,
    )?;

    if payout > 0 {
        let vault_bump = [ctx.accounts.config.sol_vault_bump];
        let vault_seeds: &[&[u8]] = &[VAULT_SEED, mint_key.as_ref(), &vault_bump];
        invoke_signed(
            &system_instruction::transfer(
                ctx.accounts.sol_vault.key,
                ctx.accounts.holder.key,
                payout,
            ),
            &[
                ctx.accounts.sol_vault.to_account_info(),
                ctx.accounts.holder.to_account_info(),
                ctx.accounts.system_program.to_account_info(),
            ],
            &[vault_seeds],
        )?;
        ctx.accounts.config.rally_paid = ctx
            .accounts
            .config
            .rally_paid
            .checked_add(payout)
            .ok_or(FeeKitError::Overflow)?;
    }

    release_lock(&mut ctx.accounts.config, amount, lock_slot)?;

    emit!(RallyClaimed {
        mint: mint_key,
        cohort,
        owner: ctx.accounts.holder.key(),
        amount,
        payout,
    });
    Ok(())
}

#[derive(Accounts)]
pub struct LockRally<'info> {
    #[account(mut)]
    pub holder: Signer<'info>,

    #[account(
        mut,
        seeds = [CONFIG_SEED, config.mint.as_ref()],
        bump = config.bump,
    )]
    pub config: Account<'info, LaunchConfig>,

    pub mint: InterfaceAccount<'info, Mint>,

    #[account(
        mut,
        token::mint = mint,
        token::authority = holder,
        token::token_program = token_program,
    )]
    pub holder_ata: InterfaceAccount<'info, TokenAccount>,

    /// CHECK: PDA that owns the locked-token account. It holds no data.
    #[account(
        seeds = [ESCROW_SEED, mint.key().as_ref()],
        bump,
    )]
    pub escrow: UncheckedAccount<'info>,

    /// CHECK: Created on the first lock. Authority is `escrow`.
    #[account(mut)]
    pub escrow_ata: UncheckedAccount<'info>,

    #[account(
        init,
        payer = holder,
        space = 8 + RallyPosition::INIT_SPACE,
        seeds = [POSITION_SEED, mint.key().as_ref(), holder.key().as_ref()],
        bump,
    )]
    pub position: Account<'info, RallyPosition>,

    pub token_program: Interface<'info, TokenInterface>,
    pub associated_token_program: Program<'info, AssociatedToken>,
    pub system_program: Program<'info, System>,
}

#[derive(Accounts)]
pub struct ClaimRally<'info> {
    #[account(mut)]
    pub holder: Signer<'info>,

    #[account(
        mut,
        seeds = [CONFIG_SEED, config.mint.as_ref()],
        bump = config.bump,
    )]
    pub config: Account<'info, LaunchConfig>,

    /// CHECK: System-owned SOL vault. Wins transfer a share of the snapshotted pot from here.
    #[account(
        mut,
        seeds = [VAULT_SEED, config.mint.as_ref()],
        bump = config.sol_vault_bump,
    )]
    pub sol_vault: UncheckedAccount<'info>,

    pub mint: InterfaceAccount<'info, Mint>,

    #[account(
        mut,
        token::mint = mint,
        token::authority = holder,
        token::token_program = token_program,
    )]
    pub holder_ata: InterfaceAccount<'info, TokenAccount>,

    /// CHECK: PDA that owns the locked-token account.
    #[account(
        seeds = [ESCROW_SEED, mint.key().as_ref()],
        bump,
    )]
    pub escrow: UncheckedAccount<'info>,

    #[account(
        mut,
        associated_token::mint = mint,
        associated_token::authority = escrow,
        associated_token::token_program = token_program,
    )]
    pub escrow_ata: InterfaceAccount<'info, TokenAccount>,

    #[account(
        mut,
        close = holder,
        seeds = [POSITION_SEED, mint.key().as_ref(), holder.key().as_ref()],
        bump = position.bump,
    )]
    pub position: Account<'info, RallyPosition>,

    pub token_program: Interface<'info, TokenInterface>,
    pub system_program: Program<'info, System>,
}
