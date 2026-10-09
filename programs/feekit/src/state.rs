use anchor_lang::prelude::*;

use crate::constants::*;

/// Immutable kit selection and running totals for one mint.
///
/// SOL sits on a separate system-owned PDA (`sol_vault`). This account never
/// has a withdraw instruction. `reserved` is unused space for a later upgrade.
#[account]
#[derive(InitSpace)]
pub struct LaunchConfig {
    pub bump: u8,
    pub sol_vault_bump: u8,
    pub kit: u8,
    pub locked: bool,
    pub slippage_bps: u16,
    pub drawdown_bps: u16,
    pub spend_bps: u16,
    pub mint: Pubkey,
    pub quote_mint: Pubkey,
    pub creator: Pubkey,
    pub base_token_program: Pubkey,
    pub sol_vault: Pubkey,
    pub sharing_config: Pubkey,
    pub min_execute_lamports: u64,
    /// Zero on a Burn kit means the whole spendable balance can be used.
    pub max_spend_lamports: u64,
    pub interval_slots: u64,
    pub mark_delay_slots: u64,
    pub high_water_price_q64: u128,
    pub pending_high_q64: u128,
    pub pending_high_slot: u64,
    pub armed_slot: u64,
    pub confirm_slot: u64,
    pub confirm_price_q64: u128,
    pub last_checkpoint_slot: u64,
    pub last_checkpoint_price_q64: u128,
    pub fees_collected: u64,
    pub sol_spent: u64,
    pub tokens_bought: u64,
    pub tokens_burned: u64,
    pub last_execution_slot: u64,
    pub execution_count: u64,
    pub last_executor: Pubkey,
    /// 0 idle, 1 open, 2 won, 3 lost.
    pub rally_status: u8,
    pub rally_cohort: u64,
    pub rally_open_slot: u64,
    pub rally_deadline_slot: u64,
    pub rally_settle_slot: u64,
    pub rally_target_q64: u128,
    pub rally_pot: u64,
    pub rally_paid: u64,
    /// Snapshotted `locked * (settle_slot + 1) - moment` when a cohort wins.
    pub rally_weight: u128,
    pub rally_locked: u64,
    /// Sum of `amount * lock_slot` for tokens currently locked.
    pub rally_moment: u128,
    /// SOL paid to the creator after a Graduate coin completes.
    pub creator_paid: u64,
    pub reserved: [u8; 24],
}

/// Deployer-owned platform fee account. The treasury PDA is a separate system account.
#[account]
#[derive(InitSpace)]
pub struct PlatformConfig {
    pub bump: u8,
    pub treasury_bump: u8,
    pub authority: Pubkey,
}

/// One holder's lock for the current rally cohort.
#[account]
#[derive(InitSpace)]
pub struct RallyPosition {
    pub bump: u8,
    pub cohort: u64,
    pub owner: Pubkey,
    pub amount: u64,
    pub lock_slot: u64,
}

#[derive(AnchorSerialize, AnchorDeserialize, Clone, Copy, Debug)]
pub struct VaultParams {
    pub kit: u8,
    pub slippage_bps: u16,
    pub min_execute_lamports: u64,
    pub max_spend_lamports: u64,
    pub interval_slots: u64,
    pub drawdown_bps: u16,
    pub spend_bps: u16,
    pub mark_delay_slots: u64,
}

impl LaunchConfig {
    pub fn is_catch(&self) -> bool {
        self.kit == KIT_CATCH
    }

    pub fn is_floor(&self) -> bool {
        self.kit == KIT_FLOOR
    }

    pub fn is_rally(&self) -> bool {
        self.kit == KIT_RALLY
    }

    pub fn is_graduate(&self) -> bool {
        self.kit == KIT_GRADUATE
    }
}

#[cfg(test)]
impl Default for LaunchConfig {
    fn default() -> Self {
        Self {
            bump: 0,
            sol_vault_bump: 0,
            kit: 0,
            locked: false,
            slippage_bps: 0,
            drawdown_bps: 0,
            spend_bps: 0,
            mint: Pubkey::default(),
            quote_mint: Pubkey::default(),
            creator: Pubkey::default(),
            base_token_program: Pubkey::default(),
            sol_vault: Pubkey::default(),
            sharing_config: Pubkey::default(),
            min_execute_lamports: 0,
            max_spend_lamports: 0,
            interval_slots: 0,
            mark_delay_slots: 0,
            high_water_price_q64: 0,
            pending_high_q64: 0,
            pending_high_slot: 0,
            armed_slot: 0,
            confirm_slot: 0,
            confirm_price_q64: 0,
            last_checkpoint_slot: 0,
            last_checkpoint_price_q64: 0,
            fees_collected: 0,
            sol_spent: 0,
            tokens_bought: 0,
            tokens_burned: 0,
            last_execution_slot: 0,
            execution_count: 0,
            last_executor: Pubkey::default(),
            rally_status: 0,
            rally_cohort: 0,
            rally_open_slot: 0,
            rally_deadline_slot: 0,
            rally_settle_slot: 0,
            rally_target_q64: 0,
            rally_pot: 0,
            rally_paid: 0,
            rally_weight: 0,
            rally_locked: 0,
            rally_moment: 0,
            creator_paid: 0,
            reserved: [0; 24],
        }
    }
}
