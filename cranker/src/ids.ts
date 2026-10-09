import { PublicKey } from "@solana/web3.js";

export const PUMP_PROGRAM_ID = new PublicKey("6EF8rrecthR5Dkzon8Nwu78hRvfCKubJ14M5uBEwF6P");
export const PUMP_AMM_PROGRAM_ID = new PublicKey("pAMMBay6oceH9fJKBRHGP5D4bD4sWpmSwMn52FMfXEA");
export const FEE_PROGRAM_ID = new PublicKey("pfeeUxB6jkeY1Hxd7CsFCAjcbHA9rWtchMGdZ6VojVZ");
export const NATIVE_MINT = new PublicKey("So11111111111111111111111111111111111111112");

export const KIT_BURN = 0;
export const KIT_FLOOR = 3;
export const KIT_GRADUATE = 5;

export const EXECUTION_RENT_BUFFER = 3_000_000n;
export const FEE_QUOTE_HAIRCUT_BPS = 500n;
export const MAX_QUOTE_HAIRCUT_BPS = 5_000n;
export const BPS = 10_000n;

export const LAUNCH_DISCRIMINATOR = Buffer.from([18, 161, 9, 224, 102, 145, 29, 94]);
export const BONDING_CURVE_DISCRIMINATOR = Buffer.from([23, 183, 248, 55, 96, 216, 172, 96]);
export const POOL_DISCRIMINATOR = Buffer.from([241, 154, 109, 4, 17, 177, 109, 188]);

/** Anchor custom errors start at 6000. */
export const ERROR = {
  notGraduated: 6009,
  graduated: 6010,
  thresholdNotMet: 6012,
  intervalNotElapsed: 6013,
  drawdownNotMet: 6014,
  markNotReady: 6015,
  checkpointNotUsed: 6016,
  zeroOutput: 6019,
} as const;

export const SKIP_ERRORS = new Set<number>([
  ERROR.notGraduated,
  ERROR.graduated,
  ERROR.thresholdNotMet,
  ERROR.intervalNotElapsed,
  ERROR.drawdownNotMet,
  ERROR.markNotReady,
  ERROR.checkpointNotUsed,
  ERROR.zeroOutput,
]);

export const ERROR_NAMES: Record<number, string> = {
  [ERROR.notGraduated]: "NotGraduated",
  [ERROR.graduated]: "Graduated",
  [ERROR.thresholdNotMet]: "ThresholdNotMet",
  [ERROR.intervalNotElapsed]: "IntervalNotElapsed",
  [ERROR.drawdownNotMet]: "DrawdownNotMet",
  [ERROR.markNotReady]: "MarkNotReady",
  [ERROR.checkpointNotUsed]: "CheckpointNotUsed",
  [ERROR.zeroOutput]: "ZeroOutput",
};

export function kitName(kit: number): string {
  if (kit === KIT_BURN) return "Burn";
  if (kit === KIT_FLOOR) return "Floor";
  if (kit === KIT_GRADUATE) return "Graduate";
  return `kit ${kit}`;
}
