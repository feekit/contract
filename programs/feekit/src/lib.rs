//! FeeKit routes pump.fun creator fees into one permanent job per coin.
//!
//! Launch transaction, signed by the coin creator:
//! 1. Pump `create` / `create_v2` for a normal Creator Fees coin.
//! 2. `initialize_vault` with Burn, Floor, or Graduate parameters.
//! 3. `lock_fees`, which assigns 85% of creator fees to the SOL vault and 15%
//!    to the FeeKit treasury, then revokes pump's sharing admin. Pass the
//!    canonical pool as the only remaining account when the coin has already graduated.
//!
//! Anyone can then crank:
//! - `collect_fees` pays the bonding-curve creator vault into the SOL vault.
//! - `sweep_amm_fees` first, once the coin has graduated.
//! - `execute_curve` buys and burns on the bonding curve.
//! - `execute_swap` buys and burns on the canonical PumpSwap pool.
//! - `release_creator` pays a Graduate vault to the creator once the curve is complete.
//! - `checkpoint` maintains the Floor kit's high-water mark. Call it on its own.
//!
//! The SOL vault is a zero-data system account so pump can transfer SOL out of it.
//! There is no withdraw instruction. The BPF upgrade authority is outside this
//! program and should be revoked after the deployment is audited.

use anchor_lang::prelude::*;

mod constants;
mod errors;
mod events;
mod instructions;
mod policy;
mod pump;
mod state;

pub use instructions::checkpoint::Checkpoint;
pub use instructions::collect::CollectFees;
pub use instructions::execute::{ExecuteCurve, ExecuteSwap};
pub use instructions::initialize::InitializeVault;
pub use instructions::lock::LockFees;
pub use instructions::platform::{InitializePlatform, WithdrawPlatform};
pub use instructions::release::ReleaseCreator;
pub use instructions::sweep::SweepAmmFees;
pub use state::VaultParams;

pub(crate) use instructions::checkpoint::__client_accounts_checkpoint;
pub(crate) use instructions::collect::__client_accounts_collect_fees;
pub(crate) use instructions::execute::__client_accounts_execute_curve;
pub(crate) use instructions::execute::__client_accounts_execute_swap;
pub(crate) use instructions::initialize::__client_accounts_initialize_vault;
pub(crate) use instructions::lock::__client_accounts_lock_fees;
pub(crate) use instructions::platform::__client_accounts_initialize_platform;
pub(crate) use instructions::platform::__client_accounts_withdraw_platform;
pub(crate) use instructions::release::__client_accounts_release_creator;
pub(crate) use instructions::sweep::__client_accounts_sweep_amm_fees;

declare_id!("9EMWVqoVNW9armPPwiY7DtW7kgQ14LTgk8F3mCkMZ11C");

#[program]
pub mod feekit {
    use super::*;

    pub fn initialize_platform(ctx: Context<InitializePlatform>) -> Result<()> {
        super::instructions::platform::initialize(ctx)
    }

    pub fn withdraw_platform(ctx: Context<WithdrawPlatform>, amount: u64) -> Result<()> {
        super::instructions::platform::withdraw(ctx, amount)
    }

    pub fn initialize_vault(ctx: Context<InitializeVault>, params: VaultParams) -> Result<()> {
        super::instructions::initialize::handler(ctx, params)
    }

    pub fn lock_fees(ctx: Context<LockFees>) -> Result<()> {
        super::instructions::lock::handler(ctx)
    }

    pub fn collect_fees(ctx: Context<CollectFees>) -> Result<()> {
        super::instructions::collect::handler(ctx)
    }

    pub fn sweep_amm_fees(ctx: Context<SweepAmmFees>) -> Result<()> {
        super::instructions::sweep::handler(ctx)
    }

    pub fn checkpoint(ctx: Context<Checkpoint>) -> Result<()> {
        super::instructions::checkpoint::handler(ctx)
    }

    pub fn execute_curve(ctx: Context<ExecuteCurve>, min_tokens_out: u64) -> Result<()> {
        super::instructions::execute::execute_curve(ctx, min_tokens_out)
    }

    pub fn execute_swap(ctx: Context<ExecuteSwap>, min_tokens_out: u64) -> Result<()> {
        super::instructions::execute::execute_swap(ctx, min_tokens_out)
    }

    pub fn release_creator(ctx: Context<ReleaseCreator>) -> Result<()> {
        super::instructions::release::handler(ctx)
    }
}
