use anchor_lang::prelude::*;

pub const CONFIG_SEED: &[u8] = b"config";
pub const VAULT_SEED: &[u8] = b"vault";
pub const PLATFORM_SEED: &[u8] = b"platform";
pub const TREASURY_SEED: &[u8] = b"treasury";

/// Pump pays the treasury this share. It stays with FeeKit.
pub const PLATFORM_FEE_BPS: u16 = 1_500;
/// Pump pays the kit vault this share. `collect_fees` forwards the keeper cut from it.
pub const KIT_SHARE_BPS: u16 = 8_500;
/// Crank reward, in bps of the whole creator fee. Taken from the vault's incoming
/// share, which leaves the kit 80% and the platform 15%.
pub const KEEPER_FEE_BPS: u16 = 500;

pub const KIT_BURN: u8 = 0;
/// Retired kit ids. `initialize_vault` rejects them.
pub const KIT_DRIP: u8 = 1;
pub const KIT_CATCH: u8 = 2;
/// Buy back along the curve when spot is under a line beneath the high. Not a resting bid.
pub const KIT_FLOOR: u8 = 3;
/// Retired kit id. `initialize_vault` rejects it.
pub const KIT_RALLY: u8 = 4;
/// Buy the bonding curve until graduation, then pay creator fees to the creator.
pub const KIT_GRADUATE: u8 = 5;
/// Fixed buy slippage for Graduate. The kit has no caller-chosen parameters.
pub const GRADUATE_SLIPPAGE_BPS: u16 = 500;

pub const RALLY_IDLE: u8 = 0;
pub const RALLY_OPEN: u8 = 1;
pub const RALLY_WON: u8 = 2;
pub const RALLY_LOST: u8 = 3;

pub const ESCROW_SEED: &[u8] = b"escrow";
pub const POSITION_SEED: &[u8] = b"position";

pub const VENUE_CURVE: u8 = 0;
pub const VENUE_SWAP: u8 = 1;

pub const BPS_DENOMINATOR: u64 = 10_000;

/// Smallest vault balance that may trigger a buy. Keeps dust from being cranked.
pub const MIN_EXECUTE_LAMPORTS: u64 = 50_000;
pub const MAX_SLIPPAGE_BPS: u16 = 2_000;
pub const MAX_DRAWDOWN_BPS: u16 = 9_000;

/// Catch cannot arm and fire in the same few slots.
pub const MIN_MARK_DELAY_SLOTS: u64 = 150;
/// After a dip is armed, a second down print must land inside this window.
pub const CONFIRM_WINDOW_SLOTS: u64 = 300;
/// A confirmed dip can be executed only while this window is still open.
pub const EXECUTE_WINDOW_SLOTS: u64 = 150;
/// A new high must hold inside this band across `mark_delay_slots` before it replaces the high-water mark.
pub const HWM_CONFIRM_BAND_BPS: u128 = 500;

/// Constant-product quotes ignore pump's exact fee tier. This haircut keeps the
/// slippage floor below a real fill without accepting a 1-token minimum.
pub const FEE_QUOTE_HAIRCUT_BPS: u64 = 500;
pub const MAX_QUOTE_HAIRCUT_BPS: u64 = 5_000;

/// Left untouched in the SOL vault so a buy can pay the one-time pump volume-account rent
/// without dropping the vault below rent-exemption.
pub const EXECUTION_RENT_BUFFER: u64 = 3_000_000;

pub const PUMP_PROGRAM_ID: Pubkey = pubkey!("6EF8rrecthR5Dkzon8Nwu78hRvfCKubJ14M5uBEwF6P");
pub const PUMP_AMM_PROGRAM_ID: Pubkey = pubkey!("pAMMBay6oceH9fJKBRHGP5D4bD4sWpmSwMn52FMfXEA");
pub const FEE_PROGRAM_ID: Pubkey = pubkey!("pfeeUxB6jkeY1Hxd7CsFCAjcbHA9rWtchMGdZ6VojVZ");
pub const NATIVE_MINT: Pubkey = pubkey!("So11111111111111111111111111111111111111112");

pub const SHARING_CONFIG_DISC: [u8; 8] = [216, 74, 9, 0, 56, 140, 93, 75];
pub const BONDING_CURVE_DISC: [u8; 8] = [23, 183, 248, 55, 96, 216, 172, 96];
pub const POOL_DISC: [u8; 8] = [241, 154, 109, 4, 17, 177, 109, 188];

/// pump-public-docs `buy_exact_quote_in_v2`
pub const BUY_EXACT_QUOTE_IN_V2: [u8; 8] = [194, 171, 28, 70, 104, 77, 91, 47];
/// pump-public-docs `buy_exact_quote_in` on PumpSwap
pub const AMM_BUY_EXACT_QUOTE_IN: [u8; 8] = [198, 46, 21, 82, 180, 217, 232, 112];
pub const CREATE_FEE_SHARING_CONFIG: [u8; 8] = [195, 78, 86, 76, 111, 52, 251, 213];
pub const UPDATE_FEE_SHARES_V2: [u8; 8] = [111, 251, 49, 6, 78, 78, 106, 18];
pub const DISTRIBUTE_CREATOR_FEES_V2: [u8; 8] = [255, 203, 19, 79, 244, 68, 8, 159];
pub const TRANSFER_CREATOR_FEES_TO_PUMP_V2: [u8; 8] = [1, 33, 78, 185, 33, 67, 44, 92];

pub const SHARING_ACTIVE: u8 = 1;
