use anchor_lang::prelude::*;

#[event]
pub struct VaultInitialized {
    pub mint: Pubkey,
    pub config: Pubkey,
    pub sol_vault: Pubkey,
    pub creator: Pubkey,
    pub kit: u8,
}

#[event]
pub struct FeesLocked {
    pub mint: Pubkey,
    pub sol_vault: Pubkey,
    pub sharing_config: Pubkey,
}

#[event]
pub struct FeesCollected {
    pub mint: Pubkey,
    /// Lamports left in the kit vault after the keeper is paid.
    pub lamports: u64,
    pub slot: u64,
    /// Paid to the crank. Older events omit this field.
    pub keeper_lamports: u64,
}

#[event]
pub struct AmmFeesSwept {
    pub mint: Pubkey,
    pub slot: u64,
}

#[event]
pub struct PriceCheckpoint {
    pub mint: Pubkey,
    pub price_q64: u128,
    pub high_water_price_q64: u128,
    pub armed_slot: u64,
    pub confirm_slot: u64,
    pub slot: u64,
}

#[event]
pub struct RallyOpened {
    pub mint: Pubkey,
    pub cohort: u64,
    pub target_q64: u128,
    pub deadline_slot: u64,
}

#[event]
pub struct RallyResolved {
    pub mint: Pubkey,
    pub cohort: u64,
    pub won: bool,
    pub pot: u64,
    pub slot: u64,
}

#[event]
pub struct RallyLocked {
    pub mint: Pubkey,
    pub cohort: u64,
    pub owner: Pubkey,
    pub amount: u64,
    pub slot: u64,
}

#[event]
pub struct RallyClaimed {
    pub mint: Pubkey,
    pub cohort: u64,
    pub owner: Pubkey,
    pub amount: u64,
    pub payout: u64,
}

#[event]
pub struct CreatorPaid {
    pub mint: Pubkey,
    pub creator: Pubkey,
    pub lamports: u64,
}

#[event]
pub struct KitExecuted {
    pub mint: Pubkey,
    pub kit: u8,
    pub venue: u8,
    pub sol_spent: u64,
    pub tokens_bought: u64,
    pub tokens_burned: u64,
    pub executor: Pubkey,
    pub slot: u64,
}
