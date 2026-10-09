use anchor_lang::prelude::*;

use crate::constants::*;
use crate::errors::FeeKitError;
use crate::state::{LaunchConfig, VaultParams};

pub fn validate_params(params: &VaultParams) -> Result<()> {
    require!(
        params.min_execute_lamports >= MIN_EXECUTE_LAMPORTS,
        FeeKitError::InvalidParams
    );
    require!(
        params.slippage_bps <= MAX_SLIPPAGE_BPS,
        FeeKitError::InvalidParams
    );

    match params.kit {
        KIT_BURN => {
            require!(
                params.interval_slots == 0
                    && params.drawdown_bps == 0
                    && params.spend_bps == BPS_DENOMINATOR as u16
                    && params.mark_delay_slots == 0,
                FeeKitError::InvalidParams
            );
            if params.max_spend_lamports != 0 {
                require!(
                    params.max_spend_lamports >= params.min_execute_lamports,
                    FeeKitError::InvalidParams
                );
            }
        }
        KIT_FLOOR => {
            // `drawdown_bps` is the cushion under the high. The floor only moves up with that high.
            require!(
                params.interval_slots == 0
                    && params.mark_delay_slots == 0
                    && params.drawdown_bps >= 1
                    && params.drawdown_bps <= MAX_DRAWDOWN_BPS
                    && params.spend_bps == BPS_DENOMINATOR as u16,
                FeeKitError::InvalidParams
            );
            if params.max_spend_lamports != 0 {
                require!(
                    params.max_spend_lamports >= params.min_execute_lamports,
                    FeeKitError::InvalidParams
                );
            }
        }
        KIT_GRADUATE => {
            require!(
                params.slippage_bps == GRADUATE_SLIPPAGE_BPS
                    && params.min_execute_lamports == MIN_EXECUTE_LAMPORTS
                    && params.max_spend_lamports == 0
                    && params.interval_slots == 0
                    && params.drawdown_bps == 0
                    && params.spend_bps == BPS_DENOMINATOR as u16
                    && params.mark_delay_slots == 0,
                FeeKitError::InvalidParams
            );
        }
        _ => return err!(FeeKitError::InvalidParams),
    }
    Ok(())
}

pub fn spendable_lamports(balance: u64, rent_exempt: u64) -> u64 {
    balance.saturating_sub(rent_exempt.saturating_add(EXECUTION_RENT_BUFFER))
}

/// Constant-product tokens out, before pump fees.
pub fn quote_tokens_out(quote_in: u64, quote_reserve: u64, token_reserve: u64) -> Result<u64> {
    require!(quote_in > 0 && token_reserve > 0, FeeKitError::ZeroOutput);
    let quote_in = quote_in as u128;
    let quote_reserve = quote_reserve as u128;
    let token_reserve = token_reserve as u128;
    let denominator = quote_reserve
        .checked_add(quote_in)
        .ok_or(FeeKitError::Overflow)?;
    let out = quote_in
        .checked_mul(token_reserve)
        .ok_or(FeeKitError::Overflow)?
        .checked_div(denominator)
        .ok_or(FeeKitError::Overflow)?;
    u64::try_from(out).map_err(|_| error!(FeeKitError::Overflow))
}

pub fn min_acceptable_tokens(
    quote_in: u64,
    quote_reserve: u64,
    token_reserve: u64,
    slippage_bps: u16,
) -> Result<u64> {
    let no_fee = quote_tokens_out(quote_in, quote_reserve, token_reserve)?;
    let haircut = (slippage_bps as u64 + FEE_QUOTE_HAIRCUT_BPS).min(MAX_QUOTE_HAIRCUT_BPS);
    let floor = (no_fee as u128)
        .checked_mul(BPS_DENOMINATOR as u128 - haircut as u128)
        .ok_or(FeeKitError::Overflow)?
        / BPS_DENOMINATOR as u128;
    let floor = u64::try_from(floor).map_err(|_| error!(FeeKitError::Overflow))?;
    require!(floor > 0, FeeKitError::ZeroOutput);
    Ok(floor)
}

pub fn price_q64(quote_reserve: u64, token_reserve: u64) -> Result<u128> {
    require!(token_reserve > 0, FeeKitError::BadAccountData);
    Ok((quote_reserve as u128)
        .checked_shl(64)
        .ok_or(FeeKitError::Overflow)?
        / token_reserve as u128)
}

/// Quote that must be bought in to lift a constant-product curve to `target_q64`.
///
/// Returns 0 when spot is already at or above the target. The curve has no resting
/// bid, so this is the whole repair: spend this much, or less if the vault is shorter.
pub fn quote_in_to_reach_price(
    quote_reserve: u64,
    token_reserve: u64,
    target_q64: u128,
) -> Result<u64> {
    let spot = price_q64(quote_reserve, token_reserve)?;
    if spot >= target_q64 || target_q64 == 0 {
        return Ok(0);
    }
    let k = (quote_reserve as u128)
        .checked_mul(token_reserve as u128)
        .ok_or(FeeKitError::Overflow)?;
    let squared = mul_shr64(target_q64, k)?;
    let mut new_quote = isqrt(squared);
    if new_quote.saturating_mul(new_quote) < squared {
        new_quote = new_quote.saturating_add(1);
    }
    let quote = quote_reserve as u128;
    if new_quote <= quote {
        return Ok(0);
    }
    u64::try_from(new_quote - quote).map_err(|_| error!(FeeKitError::Overflow))
}

/// `(a * b) >> 64`, rejecting products whose shifted value does not fit in `u128`.
fn mul_shr64(a: u128, b: u128) -> Result<u128> {
    let a_lo = a as u64 as u128;
    let a_hi = a >> 64;
    let b_lo = b as u64 as u128;
    let b_hi = b >> 64;
    let ll = a_lo * b_lo;
    let lh = a_lo * b_hi;
    let hl = a_hi * b_lo;
    let hh = a_hi * b_hi;
    let mid = (ll >> 64) + (lh & ((1u128 << 64) - 1)) + (hl & ((1u128 << 64) - 1));
    let lo = (ll & ((1u128 << 64) - 1)) | ((mid & ((1u128 << 64) - 1)) << 64);
    let hi = hh + (lh >> 64) + (hl >> 64) + (mid >> 64);
    if hi >> 64 != 0 {
        return err!(FeeKitError::Overflow);
    }
    Ok((hi << 64) | (lo >> 64))
}

fn isqrt(n: u128) -> u128 {
    if n < 2 {
        return n;
    }
    let mut lo = 1u128;
    let mut hi = n.min(1u128 << 64);
    while lo < hi {
        let mid = (lo + hi + 1) / 2;
        if mid <= n / mid {
            lo = mid;
        } else {
            hi = mid - 1;
        }
    }
    lo
}

pub fn trigger_price(high_water: u128, drawdown_bps: u16) -> Option<u128> {
    if high_water == 0 || drawdown_bps == 0 || drawdown_bps as u64 > BPS_DENOMINATOR {
        return None;
    }
    Some(high_water * (BPS_DENOMINATOR as u128 - drawdown_bps as u128) / BPS_DENOMINATOR as u128)
}

pub fn planned_spend(
    config: &LaunchConfig,
    spendable: u64,
    _slot: u64,
    fresh_price_q64: u128,
) -> Result<u64> {
    require!(
        spendable >= config.min_execute_lamports,
        FeeKitError::ThresholdNotMet
    );

    let spend = match config.kit {
        KIT_BURN | KIT_GRADUATE => cap_spend(spendable, config.max_spend_lamports),
        KIT_FLOOR => {
            let high = config.high_water_price_q64.max(fresh_price_q64);
            let line = trigger_price(high, config.drawdown_bps).ok_or(FeeKitError::MarkNotReady)?;
            require!(fresh_price_q64 <= line, FeeKitError::DrawdownNotMet);
            cap_spend(spendable, config.max_spend_lamports)
        }
        _ => return err!(FeeKitError::InvalidParams),
    };

    require!(
        spend >= config.min_execute_lamports && spend <= spendable,
        FeeKitError::ThresholdNotMet
    );
    Ok(spend)
}

fn cap_spend(amount: u64, max_spend: u64) -> u64 {
    if max_spend == 0 {
        amount
    } else {
        amount.min(max_spend)
    }
}

/// Records a price print for Catch kits.
///
/// The high-water mark moves up only after a higher price holds for `mark_delay_slots`.
/// A dip arms on the first down print and confirms on a later down print. Confirmation
/// can be refreshed while the dip lasts, so a checkpoint during the dip does not restart the wait.
pub fn on_checkpoint(config: &mut LaunchConfig, price: u128, slot: u64) -> Result<()> {
    require!(price > 0, FeeKitError::BadAccountData);
    if config.is_floor() {
        if price > config.high_water_price_q64 {
            config.high_water_price_q64 = price;
        }
        config.last_checkpoint_price_q64 = price;
        config.last_checkpoint_slot = slot;
        return Ok(());
    }
    require!(config.is_catch(), FeeKitError::CheckpointNotUsed);
    expire_arm(config, slot);
    update_high_water(config, price, slot)?;
    update_arm(config, price, slot)?;
    config.last_checkpoint_price_q64 = price;
    config.last_checkpoint_slot = slot;
    Ok(())
}

pub fn assert_catch_ready(config: &LaunchConfig, fresh_price: u128, slot: u64) -> Result<()> {
    let trigger = trigger_price(config.high_water_price_q64, config.drawdown_bps)
        .ok_or(FeeKitError::MarkNotReady)?;
    require!(
        config.armed_slot != 0 && config.confirm_slot != 0,
        FeeKitError::MarkNotReady
    );
    let earliest = config
        .armed_slot
        .checked_add(config.mark_delay_slots)
        .ok_or(FeeKitError::Overflow)?;
    require!(config.confirm_slot >= earliest, FeeKitError::MarkNotReady);
    let execute_deadline = config
        .confirm_slot
        .checked_add(EXECUTE_WINDOW_SLOTS)
        .ok_or(FeeKitError::Overflow)?;
    require!(
        slot >= config.confirm_slot && slot <= execute_deadline,
        FeeKitError::MarkNotReady
    );
    require!(
        fresh_price <= trigger && config.confirm_price_q64 <= trigger,
        FeeKitError::DrawdownNotMet
    );
    Ok(())
}

fn expire_arm(config: &mut LaunchConfig, slot: u64) {
    if config.armed_slot != 0 && config.confirm_slot == 0 {
        let deadline = config
            .armed_slot
            .saturating_add(config.mark_delay_slots)
            .saturating_add(CONFIRM_WINDOW_SLOTS);
        if slot > deadline {
            config.armed_slot = 0;
        }
    }
    if config.confirm_slot != 0 {
        let deadline = config.confirm_slot.saturating_add(EXECUTE_WINDOW_SLOTS);
        if slot > deadline {
            clear_arm(config);
        }
    }
}

fn update_high_water(config: &mut LaunchConfig, price: u128, slot: u64) -> Result<()> {
    if config.high_water_price_q64 == 0 {
        config.high_water_price_q64 = price;
        config.pending_high_q64 = 0;
        config.pending_high_slot = 0;
        return Ok(());
    }

    if price <= config.high_water_price_q64 {
        config.pending_high_q64 = 0;
        config.pending_high_slot = 0;
        return Ok(());
    }

    if config.pending_high_q64 == 0 {
        config.pending_high_q64 = price;
        config.pending_high_slot = slot;
        return Ok(());
    }

    let restart_line = config
        .pending_high_q64
        .checked_mul(BPS_DENOMINATOR as u128 + HWM_CONFIRM_BAND_BPS)
        .ok_or(FeeKitError::Overflow)?
        / BPS_DENOMINATOR as u128;
    let confirm_line = config
        .pending_high_q64
        .checked_mul(BPS_DENOMINATOR as u128 - HWM_CONFIRM_BAND_BPS)
        .ok_or(FeeKitError::Overflow)?
        / BPS_DENOMINATOR as u128;

    if price > restart_line {
        config.pending_high_q64 = price;
        config.pending_high_slot = slot;
    } else if slot
        >= config
            .pending_high_slot
            .saturating_add(config.mark_delay_slots)
        && price >= confirm_line
    {
        config.high_water_price_q64 = price;
        config.pending_high_q64 = 0;
        config.pending_high_slot = 0;
    } else if price < confirm_line {
        config.pending_high_q64 = price;
        config.pending_high_slot = slot;
    }
    Ok(())
}

fn update_arm(config: &mut LaunchConfig, price: u128, slot: u64) -> Result<()> {
    let Some(trigger) = trigger_price(config.high_water_price_q64, config.drawdown_bps) else {
        return Ok(());
    };

    if price > trigger {
        clear_arm(config);
        return Ok(());
    }

    if config.armed_slot == 0 {
        config.armed_slot = slot;
        return Ok(());
    }

    let earliest = config
        .armed_slot
        .checked_add(config.mark_delay_slots)
        .ok_or(FeeKitError::Overflow)?;
    let latest = earliest
        .checked_add(CONFIRM_WINDOW_SLOTS)
        .ok_or(FeeKitError::Overflow)?;

    if config.confirm_slot == 0 {
        if slot >= earliest && slot <= latest {
            config.confirm_slot = slot;
            config.confirm_price_q64 = price;
        }
    } else if slot <= config.confirm_slot.saturating_add(EXECUTE_WINDOW_SLOTS) {
        config.confirm_slot = slot;
        config.confirm_price_q64 = price;
    }
    Ok(())
}

fn clear_arm(config: &mut LaunchConfig) {
    config.armed_slot = 0;
    config.confirm_slot = 0;
    config.confirm_price_q64 = 0;
}

/// Opens, wins, or loses a rally from a price print.
///
/// `spendable` is the vault balance that can be paid out. A win with no lockers,
/// or a pot under `min_execute_lamports`, rolls the fees forward.
pub fn on_rally_checkpoint(
    config: &mut LaunchConfig,
    price: u128,
    slot: u64,
    spendable: u64,
) -> Result<()> {
    require!(price > 0, FeeKitError::BadAccountData);
    require!(config.is_rally(), FeeKitError::CheckpointNotUsed);

    if config.rally_status == RALLY_OPEN {
        if slot <= config.rally_deadline_slot && price >= config.rally_target_q64 {
            finish_rally_win(config, slot, spendable)?;
        } else if slot > config.rally_deadline_slot {
            finish_rally_loss(config, slot);
        }
        update_high_water(config, price, slot)?;
        config.last_checkpoint_price_q64 = price;
        config.last_checkpoint_slot = slot;
        return Ok(());
    }

    if config.rally_status == RALLY_WON || config.rally_status == RALLY_LOST {
        if config.rally_locked == 0 {
            close_rally(config);
        } else {
            update_high_water(config, price, slot)?;
            config.last_checkpoint_price_q64 = price;
            config.last_checkpoint_slot = slot;
            return Ok(());
        }
    }

    expire_arm(config, slot);
    update_high_water(config, price, slot)?;
    update_arm(config, price, slot)?;
    config.last_checkpoint_price_q64 = price;
    config.last_checkpoint_slot = slot;
    try_open_rally(config, price, slot)
}

fn try_open_rally(config: &mut LaunchConfig, price: u128, slot: u64) -> Result<()> {
    if config.rally_status != RALLY_IDLE || config.rally_locked != 0 {
        return Ok(());
    }
    let Some(trigger) = trigger_price(config.high_water_price_q64, config.drawdown_bps) else {
        return Ok(());
    };
    if config.armed_slot == 0 || config.confirm_slot == 0 || price > trigger {
        return Ok(());
    }
    let earliest = config
        .armed_slot
        .checked_add(config.mark_delay_slots)
        .ok_or(FeeKitError::Overflow)?;
    if config.confirm_slot < earliest || slot < config.confirm_slot {
        return Ok(());
    }

    config.rally_cohort = config
        .rally_cohort
        .checked_add(1)
        .ok_or(FeeKitError::Overflow)?;
    config.rally_status = RALLY_OPEN;
    config.rally_open_slot = slot;
    config.rally_deadline_slot = slot
        .checked_add(config.interval_slots)
        .ok_or(FeeKitError::Overflow)?;
    config.rally_settle_slot = 0;
    config.rally_target_q64 = config.high_water_price_q64;
    config.rally_pot = 0;
    config.rally_paid = 0;
    config.rally_weight = 0;
    clear_arm(config);
    Ok(())
}

fn finish_rally_win(config: &mut LaunchConfig, slot: u64, spendable: u64) -> Result<()> {
    if config.rally_locked == 0 || spendable < config.min_execute_lamports {
        if config.rally_locked == 0 {
            close_rally(config);
        } else {
            finish_rally_loss(config, slot);
        }
        return Ok(());
    }
    config.rally_status = RALLY_WON;
    config.rally_settle_slot = slot;
    config.rally_pot = spendable;
    config.rally_paid = 0;
    config.rally_weight = rally_total_weight(config.rally_locked, config.rally_moment, slot)?;
    Ok(())
}

fn finish_rally_loss(config: &mut LaunchConfig, slot: u64) {
    config.rally_status = RALLY_LOST;
    config.rally_settle_slot = slot;
    config.rally_pot = 0;
    config.rally_paid = 0;
    config.rally_weight = 0;
    if config.rally_locked == 0 {
        close_rally(config);
    }
}

pub fn close_rally(config: &mut LaunchConfig) {
    config.rally_status = RALLY_IDLE;
    config.rally_open_slot = 0;
    config.rally_deadline_slot = 0;
    config.rally_settle_slot = 0;
    config.rally_target_q64 = 0;
    config.rally_pot = 0;
    config.rally_paid = 0;
    config.rally_weight = 0;
}

pub fn note_lock(config: &mut LaunchConfig, amount: u64, slot: u64) -> Result<()> {
    require!(amount > 0, FeeKitError::InvalidParams);
    let moment = (amount as u128)
        .checked_mul(slot as u128)
        .ok_or(FeeKitError::Overflow)?;
    config.rally_moment = config
        .rally_moment
        .checked_add(moment)
        .ok_or(FeeKitError::Overflow)?;
    config.rally_locked = config
        .rally_locked
        .checked_add(amount)
        .ok_or(FeeKitError::Overflow)?;
    Ok(())
}

pub fn release_lock(config: &mut LaunchConfig, amount: u64, slot: u64) -> Result<()> {
    let moment = (amount as u128)
        .checked_mul(slot as u128)
        .ok_or(FeeKitError::Overflow)?;
    config.rally_moment = config
        .rally_moment
        .checked_sub(moment)
        .ok_or(FeeKitError::Overflow)?;
    config.rally_locked = config
        .rally_locked
        .checked_sub(amount)
        .ok_or(FeeKitError::Overflow)?;
    if config.rally_locked == 0 {
        close_rally(config);
    }
    Ok(())
}

pub fn rally_total_weight(locked: u64, moment: u128, settle_slot: u64) -> Result<u128> {
    let span = (settle_slot as u128)
        .checked_add(1)
        .ok_or(FeeKitError::Overflow)?;
    Ok((locked as u128)
        .checked_mul(span)
        .ok_or(FeeKitError::Overflow)?
        .checked_sub(moment)
        .ok_or(FeeKitError::Overflow)?)
}

pub fn position_weight(amount: u64, lock_slot: u64, settle_slot: u64) -> Result<u128> {
    require!(settle_slot >= lock_slot, FeeKitError::Overflow);
    let span = (settle_slot - lock_slot) as u128 + 1;
    Ok((amount as u128)
        .checked_mul(span)
        .ok_or(FeeKitError::Overflow)?)
}

/// Pro-rata share of the snapshotted pot. The last locker takes the remainder.
pub fn rally_payout(
    pot: u64,
    paid: u64,
    weight: u128,
    total_weight: u128,
    last: bool,
) -> Result<u64> {
    let remaining = pot.checked_sub(paid).ok_or(FeeKitError::Overflow)?;
    if remaining == 0 || total_weight == 0 {
        return Ok(0);
    }
    if last {
        return Ok(remaining);
    }
    let share = (pot as u128)
        .checked_mul(weight)
        .ok_or(FeeKitError::Overflow)?
        / total_weight;
    let share = u64::try_from(share).unwrap_or(remaining);
    Ok(share.min(remaining))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn burn_params() -> VaultParams {
        VaultParams {
            kit: KIT_BURN,
            slippage_bps: 100,
            min_execute_lamports: 1_000_000,
            max_spend_lamports: 0,
            interval_slots: 0,
            drawdown_bps: 0,
            spend_bps: 10_000,
            mark_delay_slots: 0,
        }
    }

    fn catch_config() -> LaunchConfig {
        LaunchConfig {
            kit: KIT_CATCH,
            slippage_bps: 100,
            drawdown_bps: 2_000,
            spend_bps: 5_000,
            min_execute_lamports: 1_000_000,
            max_spend_lamports: 5_000_000_000,
            mark_delay_slots: 150,
            ..LaunchConfig::default()
        }
    }

    #[test]
    fn rejects_a_burn_kit_that_carries_drip_fields() {
        let mut params = burn_params();
        params.interval_slots = 10;
        assert!(validate_params(&params).is_err());
    }

    #[test]
    fn graduate_spends_the_whole_curve_balance_and_rejects_extra_knobs() {
        let params = VaultParams {
            kit: KIT_GRADUATE,
            slippage_bps: GRADUATE_SLIPPAGE_BPS,
            min_execute_lamports: MIN_EXECUTE_LAMPORTS,
            max_spend_lamports: 0,
            interval_slots: 0,
            drawdown_bps: 0,
            spend_bps: BPS_DENOMINATOR as u16,
            mark_delay_slots: 0,
        };
        validate_params(&params).unwrap();
        let mut capped = params;
        capped.max_spend_lamports = 1_000_000;
        assert!(validate_params(&capped).is_err());

        let mut config = LaunchConfig::default();
        config.kit = KIT_GRADUATE;
        config.min_execute_lamports = MIN_EXECUTE_LAMPORTS;
        assert_eq!(planned_spend(&config, 2_000_000, 10, 0).unwrap(), 2_000_000);
        assert!(planned_spend(&config, 1_000, 10, 0).is_err());
    }

    #[test]
    fn burn_spends_the_full_balance_once_the_threshold_is_met() {
        let mut config = LaunchConfig::default();
        config.kit = KIT_BURN;
        config.min_execute_lamports = 1_000_000;
        config.spend_bps = 10_000;
        assert_eq!(planned_spend(&config, 5_000_000, 10, 0).unwrap(), 5_000_000);
        assert!(planned_spend(&config, 10, 10, 0).is_err());
    }

    #[test]
    fn quote_floor_rejects_a_one_token_minimum() {
        let floor =
            min_acceptable_tokens(1_000_000_000, 30_000_000_000, 1_000_000_000_000, 100).unwrap();
        assert!(floor > 1);
        let no_fee = quote_tokens_out(1_000_000_000, 30_000_000_000, 1_000_000_000_000).unwrap();
        assert!(floor < no_fee);
    }

    #[test]
    fn first_print_sets_the_high_and_does_not_arm() {
        let mut config = catch_config();
        on_checkpoint(&mut config, 1_000, 10).unwrap();
        assert_eq!(config.high_water_price_q64, 1_000);
        assert_eq!(config.armed_slot, 0);
    }

    #[test]
    fn a_price_wick_does_not_become_the_high_water_mark() {
        let mut config = catch_config();
        on_checkpoint(&mut config, 1_000, 10).unwrap();
        on_checkpoint(&mut config, 2_000, 20).unwrap();
        assert_eq!(config.high_water_price_q64, 1_000);
        assert_eq!(config.pending_high_q64, 2_000);
        on_checkpoint(&mut config, 1_000, 200).unwrap();
        assert_eq!(config.high_water_price_q64, 1_000);
        assert_eq!(config.pending_high_q64, 0);
    }

    #[test]
    fn a_sustained_rise_confirms_the_new_high() {
        let mut config = catch_config();
        on_checkpoint(&mut config, 1_000, 10).unwrap();
        on_checkpoint(&mut config, 1_200, 20).unwrap();
        on_checkpoint(&mut config, 1_180, 20 + 150).unwrap();
        assert_eq!(config.high_water_price_q64, 1_180);
        assert_eq!(config.pending_high_q64, 0);
    }

    #[test]
    fn catch_cannot_fire_inside_the_delay() {
        let mut config = catch_config();
        on_checkpoint(&mut config, 1_000, 1_000).unwrap();
        // 20% drawdown trigger is 800. 700 arms.
        on_checkpoint(&mut config, 700, 1_100).unwrap();
        assert_eq!(config.armed_slot, 1_100);
        assert_eq!(config.confirm_slot, 0);
        assert!(assert_catch_ready(&config, 700, 1_100).is_err());
        on_checkpoint(&mut config, 700, 1_100 + 150).unwrap();
        assert_eq!(config.confirm_slot, 1_250);
        assert_catch_ready(&config, 690, 1_250).unwrap();
    }

    #[test]
    fn a_later_checkpoint_during_the_dip_does_not_restart_the_wait() {
        let mut config = catch_config();
        on_checkpoint(&mut config, 1_000, 1_000).unwrap();
        on_checkpoint(&mut config, 700, 1_100).unwrap();
        let armed = config.armed_slot;
        on_checkpoint(&mut config, 680, 1_120).unwrap();
        assert_eq!(config.armed_slot, armed);
        assert_eq!(config.confirm_slot, 0);
    }

    #[test]
    fn recovery_clears_an_armed_dip() {
        let mut config = catch_config();
        on_checkpoint(&mut config, 1_000, 1_000).unwrap();
        on_checkpoint(&mut config, 700, 1_100).unwrap();
        on_checkpoint(&mut config, 950, 1_130).unwrap();
        assert_eq!(config.armed_slot, 0);
        assert_eq!(config.confirm_slot, 0);
    }

    #[test]
    fn an_unconfirmed_arm_expires() {
        let mut config = catch_config();
        on_checkpoint(&mut config, 1_000, 1_000).unwrap();
        on_checkpoint(&mut config, 700, 1_100).unwrap();
        let too_late = 1_100 + 150 + CONFIRM_WINDOW_SLOTS + 1;
        on_checkpoint(&mut config, 700, too_late).unwrap();
        assert_eq!(config.armed_slot, too_late);
        assert_eq!(config.confirm_slot, 0);
    }

    #[test]
    fn floor_ratchets_the_high_and_buys_only_under_the_line() {
        let mut config = LaunchConfig {
            kit: KIT_FLOOR,
            drawdown_bps: 2_000,
            spend_bps: 10_000,
            min_execute_lamports: 50_000,
            max_spend_lamports: 0,
            ..LaunchConfig::default()
        };
        on_checkpoint(&mut config, 1_000, 10).unwrap();
        on_checkpoint(&mut config, 2_000, 11).unwrap();
        assert_eq!(config.high_water_price_q64, 2_000);
        assert!(planned_spend(&config, 5_000_000, 12, 1_700).is_err());
        assert_eq!(
            planned_spend(&config, 5_000_000, 12, 1_600).unwrap(),
            5_000_000
        );
    }

    #[test]
    fn quote_in_lifts_the_curve_to_the_target_and_no_further() {
        let quote = 30_000_000_000u64;
        let token = 1_000_000_000_000u64;
        let spot = price_q64(quote, token).unwrap();
        assert_eq!(quote_in_to_reach_price(quote, token, spot).unwrap(), 0);
        let target = spot / 2;
        assert_eq!(quote_in_to_reach_price(quote, token, target).unwrap(), 0);
        let target = spot.saturating_mul(2);
        let buy = quote_in_to_reach_price(quote, token, target).unwrap();
        assert!(buy > 0);
        let new_quote = quote + buy;
        let new_token = (quote as u128 * token as u128) / new_quote as u128;
        let landed = price_q64(new_quote, new_token as u64).unwrap();
        assert!(landed + 5 >= target);
    }

    fn rally_config() -> LaunchConfig {
        LaunchConfig {
            kit: KIT_RALLY,
            drawdown_bps: 2_000,
            mark_delay_slots: 150,
            interval_slots: 1_000,
            min_execute_lamports: 1_000_000,
            ..LaunchConfig::default()
        }
    }

    fn open_rally() -> LaunchConfig {
        let mut config = rally_config();
        on_rally_checkpoint(&mut config, 1_000, 10, 0).unwrap();
        on_rally_checkpoint(&mut config, 700, 20, 0).unwrap();
        on_rally_checkpoint(&mut config, 700, 170, 0).unwrap();
        config
    }

    #[test]
    fn rally_opens_on_a_confirmed_dip_and_pays_lockers_who_hold_the_recovery() {
        let mut config = open_rally();
        assert_eq!(config.rally_status, RALLY_OPEN);
        assert_eq!(config.rally_target_q64, 1_000);
        assert_eq!(config.rally_deadline_slot, 1_170);
        assert_eq!(config.rally_cohort, 1);

        note_lock(&mut config, 100, 180).unwrap();
        note_lock(&mut config, 100, 200).unwrap();
        on_rally_checkpoint(&mut config, 1_000, 300, 5_000_000).unwrap();
        assert_eq!(config.rally_status, RALLY_WON);
        assert_eq!(config.rally_pot, 5_000_000);
        let total = rally_total_weight(200, 100 * 180 + 100 * 200, 300).unwrap();
        assert_eq!(config.rally_weight, total);

        let early = position_weight(100, 180, 300).unwrap();
        let late = position_weight(100, 200, 300).unwrap();
        let first = rally_payout(config.rally_pot, 0, early, total, false).unwrap();
        let second = rally_payout(config.rally_pot, first, late, total, true).unwrap();
        assert!(first > second);
        assert_eq!(first + second, config.rally_pot);
    }

    #[test]
    fn rally_returns_tokens_and_rolls_fees_when_the_window_ends() {
        let mut config = open_rally();
        note_lock(&mut config, 100, 180).unwrap();
        on_rally_checkpoint(&mut config, 700, 1_171, 5_000_000).unwrap();
        assert_eq!(config.rally_status, RALLY_LOST);
        assert_eq!(config.rally_pot, 0);
        assert_eq!(config.rally_locked, 100);
    }

    #[test]
    fn rally_with_no_lockers_does_not_pay_when_price_recovers() {
        let mut config = open_rally();
        on_rally_checkpoint(&mut config, 1_000, 200, 5_000_000).unwrap();
        assert_eq!(config.rally_status, RALLY_IDLE);
        assert_eq!(config.rally_pot, 0);
    }

    #[test]
    fn a_price_wick_does_not_open_a_rally() {
        let mut config = rally_config();
        on_rally_checkpoint(&mut config, 1_000, 10, 0).unwrap();
        on_rally_checkpoint(&mut config, 700, 20, 0).unwrap();
        on_rally_checkpoint(&mut config, 1_000, 170, 0).unwrap();
        assert_eq!(config.rally_status, RALLY_IDLE);
    }
}
