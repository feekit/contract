import { getAssociatedTokenAddressSync } from "@solana/spl-token";
import { PublicKey } from "@solana/web3.js";
import {
  BONDING_CURVE_DISCRIMINATOR,
  FEE_PROGRAM_ID,
  NATIVE_MINT,
  POOL_DISCRIMINATOR,
  PUMP_AMM_PROGRAM_ID,
  PUMP_PROGRAM_ID,
} from "./ids.js";

const ZERO = PublicKey.default;

export type LaunchConfig = {
  kit: number;
  locked: boolean;
  slippageBps: number;
  drawdownBps: number;
  spendBps: number;
  mint: PublicKey;
  quoteMint: PublicKey;
  creator: PublicKey;
  baseTokenProgram: PublicKey;
  solVault: PublicKey;
  sharingConfig: PublicKey;
  minExecuteLamports: bigint;
  maxSpendLamports: bigint;
  intervalSlots: bigint;
  markDelaySlots: bigint;
  armedSlot: bigint;
  confirmSlot: bigint;
  lastCheckpointSlot: bigint;
  lastExecutionSlot: bigint;
  feesCollected: bigint;
  solSpent: bigint;
  tokensBought: bigint;
  tokensBurned: bigint;
};

export type CurveState = {
  virtualTokenReserves: bigint;
  virtualQuoteReserves: bigint;
  complete: boolean;
  mayhem: boolean;
};

export type PoolState = {
  baseVault: PublicKey;
  quoteVault: PublicKey;
  mayhem: boolean;
  virtualQuote: bigint;
};

export type FeeRecipients = {
  curve: PublicKey;
  curveMayhem: PublicKey;
  buyback: PublicKey;
  swap: PublicKey;
  swapMayhem: PublicKey;
  swapBuyback: PublicKey;
};

export function pda(seeds: Array<Buffer | Uint8Array>, program: PublicKey): PublicKey {
  return PublicKey.findProgramAddressSync(seeds, program)[0];
}

export function u16le(value: number): Buffer {
  const out = Buffer.alloc(2);
  out.writeUInt16LE(value);
  return out;
}

export function treasuryPda(program: PublicKey): PublicKey {
  return pda([Buffer.from("treasury")], program);
}

export function platformPda(program: PublicKey): PublicKey {
  return pda([Buffer.from("platform")], program);
}

export function bondingCurvePda(mint: PublicKey): PublicKey {
  return pda([Buffer.from("bonding-curve"), mint.toBuffer()], PUMP_PROGRAM_ID);
}

export function sharingConfigPda(mint: PublicKey): PublicKey {
  return pda([Buffer.from("sharing-config"), mint.toBuffer()], FEE_PROGRAM_ID);
}

export function pumpCreatorVaultPda(sharingConfig: PublicKey): PublicKey {
  return pda([Buffer.from("creator-vault"), sharingConfig.toBuffer()], PUMP_PROGRAM_ID);
}

export function ammCreatorVaultPda(sharingConfig: PublicKey): PublicKey {
  return pda([Buffer.from("creator_vault"), sharingConfig.toBuffer()], PUMP_AMM_PROGRAM_ID);
}

export function poolV2Pda(mint: PublicKey): PublicKey {
  return pda([Buffer.from("pool-v2"), mint.toBuffer()], PUMP_AMM_PROGRAM_ID);
}

export function canonicalPoolPda(mint: PublicKey): PublicKey {
  const authority = pda([Buffer.from("pool-authority"), mint.toBuffer()], PUMP_PROGRAM_ID);
  return pda(
    [Buffer.from("pool"), u16le(0), authority.toBuffer(), mint.toBuffer(), NATIVE_MINT.toBuffer()],
    PUMP_AMM_PROGRAM_ID,
  );
}

export function pumpGlobalPda(): PublicKey {
  return pda([Buffer.from("global")], PUMP_PROGRAM_ID);
}

export function pumpEventAuthorityPda(): PublicKey {
  return pda([Buffer.from("__event_authority")], PUMP_PROGRAM_ID);
}

export function ammEventAuthorityPda(): PublicKey {
  return pda([Buffer.from("__event_authority")], PUMP_AMM_PROGRAM_ID);
}

export function ammGlobalConfigPda(): PublicKey {
  return pda([Buffer.from("global_config")], PUMP_AMM_PROGRAM_ID);
}

export function pumpFeeConfigPda(): PublicKey {
  return pda([Buffer.from("fee_config"), PUMP_PROGRAM_ID.toBuffer()], FEE_PROGRAM_ID);
}

export function ammFeeConfigPda(): PublicKey {
  return pda([Buffer.from("fee_config"), PUMP_AMM_PROGRAM_ID.toBuffer()], FEE_PROGRAM_ID);
}

export function volumeAccumulatorPda(program: PublicKey, user: PublicKey): PublicKey {
  return pda([Buffer.from("user_volume_accumulator"), user.toBuffer()], program);
}

export function globalVolumeAccumulatorPda(program: PublicKey): PublicKey {
  return pda([Buffer.from("global_volume_accumulator")], program);
}

export function ata(owner: PublicKey, mint: PublicKey, tokenProgram: PublicKey): PublicKey {
  return getAssociatedTokenAddressSync(mint, owner, true, tokenProgram);
}

export function decodeLaunchConfig(data: Buffer): LaunchConfig {
  let offset = 8;
  const u8 = () => data[offset++];
  const u16 = () => {
    const value = data.readUInt16LE(offset);
    offset += 2;
    return value;
  };
  const u64 = () => {
    const value = data.readBigUInt64LE(offset);
    offset += 8;
    return value;
  };
  const u128 = () => {
    offset += 16;
  };
  const pubkey = () => {
    const value = new PublicKey(data.subarray(offset, offset + 32));
    offset += 32;
    return value;
  };

  u8();
  u8();
  const kit = u8();
  const locked = data[offset++] === 1;
  const slippageBps = u16();
  const drawdownBps = u16();
  const spendBps = u16();
  const mint = pubkey();
  const quoteMint = pubkey();
  const creator = pubkey();
  const baseTokenProgram = pubkey();
  const solVault = pubkey();
  const sharingConfig = pubkey();
  const minExecuteLamports = u64();
  const maxSpendLamports = u64();
  const intervalSlots = u64();
  const markDelaySlots = u64();
  u128();
  u128();
  u64();
  const armedSlot = u64();
  const confirmSlot = u64();
  u128();
  const lastCheckpointSlot = u64();
  u128();
  const feesCollected = u64();
  const solSpent = u64();
  const tokensBought = u64();
  const tokensBurned = u64();
  const lastExecutionSlot = u64();

  return {
    kit,
    locked,
    slippageBps,
    drawdownBps,
    spendBps,
    mint,
    quoteMint,
    creator,
    baseTokenProgram,
    solVault,
    sharingConfig,
    minExecuteLamports,
    maxSpendLamports,
    intervalSlots,
    markDelaySlots,
    armedSlot,
    confirmSlot,
    lastCheckpointSlot,
    lastExecutionSlot,
    feesCollected,
    solSpent,
    tokensBought,
    tokensBurned,
  };
}

export function decodeCurve(data: Buffer): CurveState {
  if (data.length < 81 || !data.subarray(0, 8).equals(BONDING_CURVE_DISCRIMINATOR)) {
    throw new Error("bonding curve account has an unexpected layout");
  }
  return {
    virtualTokenReserves: data.readBigUInt64LE(8),
    virtualQuoteReserves: data.readBigUInt64LE(16),
    complete: data[48] === 1,
    mayhem: data.length > 81 && data[81] === 1,
  };
}

export function decodePool(data: Buffer): PoolState {
  if (data.length < 203 || !data.subarray(0, 8).equals(POOL_DISCRIMINATOR)) {
    throw new Error("PumpSwap pool account has an unexpected layout");
  }
  return {
    baseVault: new PublicKey(data.subarray(139, 171)),
    quoteVault: new PublicKey(data.subarray(171, 203)),
    mayhem: data.length > 243 && data[243] === 1,
    virtualQuote: data.length >= 261 ? positiveI128(data, 245) : 0n,
  };
}

function positiveI128(data: Buffer, offset: number): bigint {
  const lo = data.readBigUInt64LE(offset);
  const hi = data.readBigUInt64LE(offset + 8);
  let value = (hi << 64n) + lo;
  if (hi >= 0x8000000000000000n) value -= 1n << 128n;
  return value > 0n ? value : 0n;
}

export function tokenAmount(data: Buffer): bigint {
  if (data.length < 72) return 0n;
  return data.readBigUInt64LE(64);
}

export function decodeFeeRecipients(pumpGlobal: Buffer, ammGlobal: Buffer): FeeRecipients {
  const curve = readRecipients(pumpGlobal, 41, 1).concat(readRecipients(pumpGlobal, 162, 7));
  const curveMayhem = readRecipients(pumpGlobal, 483, 1).concat(readRecipients(pumpGlobal, 516, 7));
  const buyback = readRecipients(pumpGlobal, 741, 8);
  const swap = readRecipients(ammGlobal, 57, 8);
  const swapMayhem = readRecipients(ammGlobal, 385, 1).concat(readRecipients(ammGlobal, 418, 7));
  const swapBuyback = readRecipients(ammGlobal, 643, 8);
  return {
    curve: firstRecipient(curve, "pump fee recipient"),
    curveMayhem: firstRecipient(curveMayhem, "pump mayhem fee recipient"),
    buyback: firstRecipient(buyback, "pump buyback fee recipient"),
    swap: firstRecipient(swap, "PumpSwap fee recipient"),
    swapMayhem: firstRecipient(swapMayhem, "PumpSwap mayhem fee recipient"),
    swapBuyback: firstRecipient(swapBuyback, "PumpSwap buyback fee recipient"),
  };
}

function readRecipients(data: Buffer, offset: number, count: number): PublicKey[] {
  const out: PublicKey[] = [];
  for (let i = 0; i < count; i += 1) {
    const start = offset + i * 32;
    if (start + 32 > data.length) break;
    out.push(new PublicKey(data.subarray(start, start + 32)));
  }
  return out;
}

function firstRecipient(keys: PublicKey[], label: string): PublicKey {
  const found = keys.find((key) => !key.equals(ZERO));
  if (!found) throw new Error(`missing ${label}`);
  return found;
}
