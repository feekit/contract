import { BN } from "@coral-xyz/anchor";
import { BPS, EXECUTION_RENT_BUFFER, FEE_QUOTE_HAIRCUT_BPS, MAX_QUOTE_HAIRCUT_BPS } from "./ids.js";
import type { LaunchConfig } from "./accounts.js";

export function spendableLamports(balance: bigint, rentExempt: bigint): bigint {
  const reserve = rentExempt + EXECUTION_RENT_BUFFER;
  return balance > reserve ? balance - reserve : 0n;
}

/** Matches the program's spend rule. Returns null when the kit would reject the crank. */
export function plannedSpend(config: LaunchConfig, spendable: bigint, slot: bigint): bigint | null {
  if (spendable < config.minExecuteLamports) return null;
  if (
    config.intervalSlots > 0n &&
    config.lastExecutionSlot !== 0n &&
    slot < config.lastExecutionSlot + config.intervalSlots
  ) {
    return null;
  }

  const spend = cap(spendable, config.maxSpendLamports);
  if (spend < config.minExecuteLamports || spend > spendable) return null;
  return spend;
}

export function minTokensOut(
  quoteIn: bigint,
  quoteReserve: bigint,
  tokenReserve: bigint,
  slippageBps: number,
): BN | null {
  if (quoteIn <= 0n || tokenReserve <= 0n) return null;
  const noFee = (quoteIn * tokenReserve) / (quoteReserve + quoteIn);
  const haircut = BigInt(slippageBps) + FEE_QUOTE_HAIRCUT_BPS;
  const applied = haircut < MAX_QUOTE_HAIRCUT_BPS ? haircut : MAX_QUOTE_HAIRCUT_BPS;
  const floor = (noFee * (BPS - applied)) / BPS;
  if (floor <= 0n || floor > BigInt("18446744073709551615")) return null;
  return new BN(floor.toString());
}

export function needsCheckpoint(config: LaunchConfig, slot: bigint): boolean {
  if (config.lastCheckpointSlot === 0n) return true;
  if (slot >= config.lastCheckpointSlot + config.markDelaySlots) return true;
  return (
    config.armedSlot !== 0n &&
    config.confirmSlot === 0n &&
    slot >= config.armedSlot + config.markDelaySlots
  );
}

function cap(amount: bigint, maxSpend: bigint): bigint {
  if (maxSpend === 0n) return amount;
  return amount < maxSpend ? amount : maxSpend;
}
