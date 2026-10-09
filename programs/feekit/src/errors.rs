use anchor_lang::prelude::*;

#[error_code]
pub enum FeeKitError {
    #[msg("Kit parameters are not valid for the selected kit.")]
    InvalidParams,
    #[msg("Quote mint must be native SOL.")]
    QuoteNotSol,
    #[msg("Base mint must use the SPL Token or Token-2022 program.")]
    UnsupportedTokenProgram,
    #[msg("Bonding curve account does not match this mint.")]
    BadBondingCurve,
    #[msg("Only the pump coin creator can initialize and lock fee routing.")]
    NotCreator,
    #[msg("Holder-reward coins are not supported.")]
    HolderRewardsUnsupported,
    #[msg("Fee routing is already locked.")]
    AlreadyLocked,
    #[msg("Fee routing is not locked to the FeeKit vault.")]
    FeesNotLocked,
    #[msg("Creator fees are not assigned entirely to the FeeKit vault.")]
    FeeRecipientMismatch,
    #[msg("This coin is still on the bonding curve.")]
    NotGraduated,
    #[msg("This coin has graduated. Buy it through PumpSwap.")]
    Graduated,
    #[msg("Canonical PumpSwap pool does not match this mint.")]
    BadPool,
    #[msg("Vault balance is below the execution threshold.")]
    ThresholdNotMet,
    #[msg("Drip interval has not elapsed.")]
    IntervalNotElapsed,
    #[msg("Price is not far enough below the confirmed high.")]
    DrawdownNotMet,
    #[msg("Catch conditions are not confirmed yet.")]
    MarkNotReady,
    #[msg("This kit does not use price checkpoints.")]
    CheckpointNotUsed,
    #[msg("Minimum tokens out is below the slippage floor.")]
    SlippageExceeded,
    #[msg("Buy would spend more SOL than this kit allows.")]
    Overspend,
    #[msg("Buy produced no tokens.")]
    ZeroOutput,
    #[msg("Pump account data did not match the expected layout.")]
    BadAccountData,
    #[msg("SOL vault does not match this launch.")]
    VaultMismatch,
    #[msg("Arithmetic overflow.")]
    Overflow,
    #[msg("Remaining accounts do not match this coin's venue.")]
    BadRemainingAccounts,
    #[msg("This kit does not buy the coin.")]
    KitDoesNotBuy,
    #[msg("Rally is not open for locks.")]
    RallyNotOpen,
    #[msg("Rally has not resolved.")]
    RallyNotResolved,
    #[msg("This lock belongs to another rally.")]
    RallyCohortMismatch,
    #[msg("After graduation this kit pays the creator.")]
    GraduatePaysCreator,
    #[msg("FeeKit treasury is not initialized.")]
    PlatformNotReady,
}
