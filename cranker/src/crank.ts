import { AnchorProvider, Program, Wallet, type Idl } from "@coral-xyz/anchor";
import { TOKEN_PROGRAM_ID } from "@solana/spl-token";
import {
  Connection,
  Keypair,
  PublicKey,
  SystemProgram,
  type AccountMeta,
  type TransactionInstruction,
} from "@solana/web3.js";
import { ComputeBudgetProgram } from "@solana/web3.js";
import bs58 from "bs58";
import { readFileSync } from "node:fs";
import { readFile } from "node:fs/promises";
import { homedir } from "node:os";
import { join } from "node:path";
import {
  ammCreatorVaultPda,
  ammEventAuthorityPda,
  ammFeeConfigPda,
  ammGlobalConfigPda,
  ata,
  bondingCurvePda,
  canonicalPoolPda,
  poolV2Pda,
  decodeCurve,
  decodeFeeRecipients,
  decodeLaunchConfig,
  decodePool,
  globalVolumeAccumulatorPda,
  pumpCreatorVaultPda,
  pumpEventAuthorityPda,
  pumpFeeConfigPda,
  platformPda,
  pumpGlobalPda,
  treasuryPda,
  tokenAmount,
  volumeAccumulatorPda,
  type FeeRecipients,
  type LaunchConfig,
} from "./accounts.js";
import {
  ERROR_NAMES,
  FEE_PROGRAM_ID,
  KIT_FLOOR,
  KIT_GRADUATE,
  LAUNCH_DISCRIMINATOR,
  NATIVE_MINT,
  PUMP_AMM_PROGRAM_ID,
  PUMP_PROGRAM_ID,
  SKIP_ERRORS,
  kitName,
} from "./ids.js";
import { minTokensOut, needsCheckpoint, plannedSpend, spendableLamports } from "./quote.js";

const ASSOCIATED_TOKEN_PROGRAM_ID = new PublicKey("ATokenGPvbdGVxr1b2hvZbsiqW5xWH25efTNsLJA8knL");

type IxBuilder = {
  accounts(accounts: Record<string, unknown>): IxBuilder;
  preInstructions(ixs: TransactionInstruction[]): IxBuilder;
  remainingAccounts(accounts: AccountMeta[]): IxBuilder;
  simulate(): Promise<unknown>;
  rpc(opts?: { skipPreflight?: boolean; commitment?: "confirmed" }): Promise<string>;
};

type Methods = {
  checkpoint(): IxBuilder;
  collectFees(): IxBuilder;
  sweepAmmFees(): IxBuilder;
  executeCurve(minTokensOut: InstanceType<typeof import("@coral-xyz/anchor").BN>): IxBuilder;
  executeSwap(minTokensOut: InstanceType<typeof import("@coral-xyz/anchor").BN>): IxBuilder;
  releaseCreator(): IxBuilder;
};

export type Cranker = {
  connection: Connection;
  payer: Keypair;
  program: Program;
  programId: PublicKey;
};

export function clusterUrl(cluster: string): string {
  if (cluster === "mainnet") return "https://api.mainnet-beta.solana.com";
  if (cluster === "devnet") return "https://api.devnet.solana.com";
  if (cluster === "localnet") return "http://127.0.0.1:8899";
  throw new Error("--cluster must be mainnet, devnet, or localnet");
}

export function defaultKeypairPath(): string {
  return join(homedir(), ".config", "solana", "id.json");
}

export function loadKeypair(file: string): Keypair {
  const secret = JSON.parse(readFileSync(file, "utf8")) as number[];
  return Keypair.fromSecretKey(Uint8Array.from(secret));
}

export function openCranker(connection: Connection, payer: Keypair, idl: Idl): Cranker {
  const provider = new AnchorProvider(connection, new Wallet(payer), { commitment: "confirmed" });
  const program = new Program(idl, provider);
  return { connection, payer, program, programId: new PublicKey(idl.address) };
}

export async function listConfigs(cranker: Cranker, mint: PublicKey | null): Promise<PublicKey[]> {
  if (mint) {
    const [config] = PublicKey.findProgramAddressSync(
      [Buffer.from("config"), mint.toBuffer()],
      cranker.programId,
    );
    const info = await cranker.connection.getAccountInfo(config);
    return info ? [config] : [];
  }
  const accounts = await cranker.connection.getProgramAccounts(cranker.programId, {
    filters: [{ memcmp: { offset: 0, bytes: bs58.encode(LAUNCH_DISCRIMINATOR) } }],
    dataSlice: { offset: 0, length: 0 },
  });
  return accounts.map((account) => account.pubkey);
}

export async function printStatus(cranker: Cranker, mint: PublicKey | null): Promise<void> {
  const configs = await listConfigs(cranker, mint);
  if (configs.length === 0) {
    console.log(mint ? `no FeeKit config for ${mint.toBase58()}` : "no FeeKit launches");
    return;
  }
  for (const configPk of configs) {
    const info = await cranker.connection.getAccountInfo(configPk);
    if (!info) continue;
    const config = decodeLaunchConfig(info.data);
    const curveInfo = await cranker.connection.getAccountInfo(bondingCurvePda(config.mint));
    const complete = curveInfo ? decodeCurve(curveInfo.data).complete : false;
    const vault = await cranker.connection.getAccountInfo(config.solVault);
    console.log(
      `${config.mint.toBase58()} ${kitName(config.kit)} ${config.locked ? "locked" : "unlocked"} ${
        complete ? "graduated" : "curve"
      } vault=${vault?.lamports ?? 0}`,
    );
  }
}

export async function runOnce(cranker: Cranker, mint: PublicKey | null): Promise<void> {
  const configs = await listConfigs(cranker, mint);
  if (configs.length === 0) {
    console.log(mint ? `no FeeKit config for ${mint.toBase58()}` : "no FeeKit launches");
    return;
  }
  const [fees, rentExempt, slot] = await Promise.all([
    loadFeeRecipients(cranker.connection),
    cranker.connection.getMinimumBalanceForRentExemption(0),
    cranker.connection.getSlot("confirmed"),
  ]);
  for (const config of configs) {
    try {
      await crankConfig(cranker, config, fees, BigInt(rentExempt), BigInt(slot));
    } catch (err) {
      console.error(`${config.toBase58()} ${message(err)}`);
    }
  }
}

async function crankConfig(
  cranker: Cranker,
  configPk: PublicKey,
  fees: FeeRecipients,
  rentExempt: bigint,
  slot: bigint,
): Promise<void> {
  const info = await cranker.connection.getAccountInfo(configPk);
  if (!info) return;
  const config = decodeLaunchConfig(info.data);
  const label = `${config.mint.toBase58()} ${kitName(config.kit)}`;
  if (!config.locked) {
    console.log(`${label} waiting for fee lock`);
    return;
  }
  if (!config.quoteMint.equals(NATIVE_MINT)) {
    console.log(`${label} quote is not SOL`);
    return;
  }

  const curvePk = bondingCurvePda(config.mint);
  const curveInfo = await cranker.connection.getAccountInfo(curvePk);
  if (!curveInfo) {
    console.log(`${label} bonding curve is missing`);
    return;
  }
  const curve = decodeCurve(curveInfo.data);
  console.log(`${label} ${curve.complete ? "graduated" : "curve"}`);

  if (config.kit === KIT_FLOOR && needsCheckpoint(config, slot)) {
    await checkpoint(cranker, config, configPk, curvePk, curve.complete, label);
  }
  if (curve.complete) {
    await sweep(cranker, config, configPk, curvePk, label);
  }
  await collect(cranker, config, configPk, curvePk, label);
  await execute(cranker, config, configPk, curvePk, curve, fees, rentExempt, label);
}

async function checkpoint(
  cranker: Cranker,
  config: LaunchConfig,
  configPk: PublicKey,
  curvePk: PublicKey,
  complete: boolean,
  label: string,
): Promise<void> {
  const remaining: AccountMeta[] = [];
  if (complete) {
    const poolPk = canonicalPoolPda(config.mint);
    const poolInfo = await cranker.connection.getAccountInfo(poolPk);
    if (!poolInfo) {
      console.log(`${label} checkpoint skipped, canonical pool is missing`);
      return;
    }
    const pool = decodePool(poolInfo.data);
    remaining.push(
      meta(poolPk, false),
      meta(pool.baseVault, false),
      meta(pool.quoteVault, false),
    );
  }
  const builder = methods(cranker)
    .checkpoint()
    .accounts({
      crank: cranker.payer.publicKey,
      config: configPk,
      solVault: config.solVault,
      bondingCurve: curvePk,
      sharingConfig: config.sharingConfig,
    })
    .remainingAccounts(remaining)
    .preInstructions([budget(200_000)]);
  await submit(builder, `${label} checkpoint`);
}

async function sweep(
  cranker: Cranker,
  config: LaunchConfig,
  configPk: PublicKey,
  curvePk: PublicKey,
  label: string,
): Promise<void> {
  const sharing = config.sharingConfig;
  const ammVault = ammCreatorVaultPda(sharing);
  const ammAta = ata(ammVault, NATIVE_MINT, TOKEN_PROGRAM_ID);
  const ataInfo = await cranker.connection.getAccountInfo(ammAta);
  if (!ataInfo || tokenAmount(ataInfo.data) === 0n) return;

  const pumpVault = pumpCreatorVaultPda(sharing);
  const builder = methods(cranker)
    .sweepAmmFees()
    .accounts({
      crank: cranker.payer.publicKey,
      config: configPk,
      solVault: config.solVault,
      bondingCurve: curvePk,
      sharingConfig: sharing,
      quoteMint: NATIVE_MINT,
      quoteTokenProgram: TOKEN_PROGRAM_ID,
      systemProgram: SystemProgram.programId,
      associatedTokenProgram: ASSOCIATED_TOKEN_PROGRAM_ID,
      coinCreatorVaultAuthority: ammVault,
      coinCreatorVaultAta: ammAta,
      pumpCreatorVault: pumpVault,
      pumpCreatorVaultAta: ata(pumpVault, NATIVE_MINT, TOKEN_PROGRAM_ID),
      eventAuthority: ammEventAuthorityPda(),
      ammProgram: PUMP_AMM_PROGRAM_ID,
    })
    .preInstructions([budget(200_000)]);
  await submit(builder, `${label} sweep`);
}

async function collect(
  cranker: Cranker,
  config: LaunchConfig,
  configPk: PublicKey,
  curvePk: PublicKey,
  label: string,
): Promise<void> {
  const creatorVault = pumpCreatorVaultPda(config.sharingConfig);
  const vault = await cranker.connection.getAccountInfo(creatorVault);
  const rent = BigInt(await cranker.connection.getMinimumBalanceForRentExemption(vault?.data.length ?? 0));
  if (!vault || BigInt(vault.lamports) <= rent) return;

  const builder = methods(cranker)
    .collectFees()
    .accounts({
      crank: cranker.payer.publicKey,
      config: configPk,
      solVault: config.solVault,
      mint: config.mint,
      bondingCurve: curvePk,
      sharingConfig: config.sharingConfig,
      creatorVault,
      creatorVaultQuoteAta: ata(creatorVault, NATIVE_MINT, TOKEN_PROGRAM_ID),
      quoteMint: NATIVE_MINT,
      quoteTokenProgram: TOKEN_PROGRAM_ID,
      associatedTokenProgram: ASSOCIATED_TOKEN_PROGRAM_ID,
      systemProgram: SystemProgram.programId,
      eventAuthority: pumpEventAuthorityPda(),
      pumpProgram: PUMP_PROGRAM_ID,
      treasury: treasuryPda(cranker.programId),
      platform: platformPda(cranker.programId),
    })
    .preInstructions([budget(250_000)]);
  await submit(builder, `${label} collect`);
}

async function execute(
  cranker: Cranker,
  config: LaunchConfig,
  configPk: PublicKey,
  curvePk: PublicKey,
  curve: ReturnType<typeof decodeCurve>,
  fees: FeeRecipients,
  rentExempt: bigint,
  label: string,
): Promise<void> {
  const solVault = await cranker.connection.getAccountInfo(config.solVault);
  const balance = BigInt(solVault?.lamports ?? 0);
  const slot = BigInt(await cranker.connection.getSlot("confirmed"));
  const spendable = spendableLamports(balance, rentExempt);
  const spend = plannedSpend(config, spendable, slot);
  if (spend === null) {
    console.log(`${label} execute waiting`);
    return;
  }

  if (curve.complete && config.kit === KIT_GRADUATE) {
    const builder = methods(cranker)
      .releaseCreator()
      .accounts({
        crank: cranker.payer.publicKey,
        config: configPk,
        solVault: config.solVault,
        creator: config.creator,
        bondingCurve: curvePk,
        sharingConfig: config.sharingConfig,
        systemProgram: SystemProgram.programId,
      })
      .preInstructions([budget(200_000)]);
    await submit(builder, `${label} release creator`);
    return;
  }

  if (!curve.complete) {
    const minOut = minTokensOut(spend, curve.virtualQuoteReserves, curve.virtualTokenReserves, config.slippageBps);
    if (!minOut) {
      console.log(`${label} execute waiting`);
      return;
    }
    const feeRecipient = curve.mayhem ? fees.curveMayhem : fees.curve;
    const creatorVault = pumpCreatorVaultPda(config.sharingConfig);
    const userVolume = volumeAccumulatorPda(PUMP_PROGRAM_ID, config.solVault);
    const builder = methods(cranker)
      .executeCurve(minOut)
      .accounts({
        crank: cranker.payer.publicKey,
        config: configPk,
        solVault: config.solVault,
        baseMint: config.mint,
        vaultBaseAta: ata(config.solVault, config.mint, config.baseTokenProgram),
        quoteMint: NATIVE_MINT,
        baseTokenProgram: config.baseTokenProgram,
        quoteTokenProgram: TOKEN_PROGRAM_ID,
        associatedTokenProgram: ASSOCIATED_TOKEN_PROGRAM_ID,
        bondingCurve: curvePk,
        sharingConfig: config.sharingConfig,
        systemProgram: SystemProgram.programId,
        pumpProgram: PUMP_PROGRAM_ID,
        buy: {
          global: pumpGlobalPda(),
          feeRecipient,
          associatedQuoteFeeRecipient: ata(feeRecipient, NATIVE_MINT, TOKEN_PROGRAM_ID),
          buybackFeeRecipient: fees.buyback,
          associatedQuoteBuybackFeeRecipient: ata(fees.buyback, NATIVE_MINT, TOKEN_PROGRAM_ID),
          associatedBaseBondingCurve: ata(curvePk, config.mint, config.baseTokenProgram),
          associatedQuoteBondingCurve: ata(curvePk, NATIVE_MINT, TOKEN_PROGRAM_ID),
          associatedQuoteUser: ata(config.solVault, NATIVE_MINT, TOKEN_PROGRAM_ID),
          creatorVault,
          associatedCreatorVault: ata(creatorVault, NATIVE_MINT, TOKEN_PROGRAM_ID),
          globalVolumeAccumulator: globalVolumeAccumulatorPda(PUMP_PROGRAM_ID),
          userVolumeAccumulator: userVolume,
          associatedUserVolumeAccumulator: ata(userVolume, NATIVE_MINT, TOKEN_PROGRAM_ID),
          feeConfig: pumpFeeConfigPda(),
          feeProgram: FEE_PROGRAM_ID,
          eventAuthority: pumpEventAuthorityPda(),
        },
      })
      .preInstructions([budget(600_000)]);
    await submit(builder, `${label} execute curve`);
    return;
  }

  const poolPk = canonicalPoolPda(config.mint);
  const poolInfo = await cranker.connection.getAccountInfo(poolPk);
  if (!poolInfo) {
    console.log(`${label} execute waiting, canonical pool is missing`);
    return;
  }
  const pool = decodePool(poolInfo.data);
  const [baseInfo, quoteInfo] = await cranker.connection.getMultipleAccountsInfo([
    pool.baseVault,
    pool.quoteVault,
  ]);
  if (!baseInfo || !quoteInfo) {
    console.log(`${label} execute waiting, pool vaults are missing`);
    return;
  }
  const quoteReserve = tokenAmount(quoteInfo.data) + pool.virtualQuote;
  const minOut = minTokensOut(
    spend,
    quoteReserve,
    tokenAmount(baseInfo.data),
    config.slippageBps,
  );
  if (!minOut) {
    console.log(`${label} execute waiting`);
    return;
  }
  const protocolRecipient = pool.mayhem ? fees.swapMayhem : fees.swap;
  const userVolume = volumeAccumulatorPda(PUMP_AMM_PROGRAM_ID, config.solVault);
  const ammVault = ammCreatorVaultPda(config.sharingConfig);
  const builder = methods(cranker)
    .executeSwap(minOut)
    .accounts({
      crank: cranker.payer.publicKey,
      config: configPk,
      solVault: config.solVault,
      bondingCurve: curvePk,
      sharingConfig: config.sharingConfig,
      baseMint: config.mint,
      vaultBaseAta: ata(config.solVault, config.mint, config.baseTokenProgram),
      quoteMint: NATIVE_MINT,
      vaultQuoteAta: ata(config.solVault, NATIVE_MINT, TOKEN_PROGRAM_ID),
      baseTokenProgram: config.baseTokenProgram,
      quoteTokenProgram: TOKEN_PROGRAM_ID,
      systemProgram: SystemProgram.programId,
      associatedTokenProgram: ASSOCIATED_TOKEN_PROGRAM_ID,
      market: {
        bondingCurve: curvePk,
        sharingConfig: config.sharingConfig,
        pool: poolPk,
        globalConfig: ammGlobalConfigPda(),
        poolBaseTokenAccount: pool.baseVault,
        poolQuoteTokenAccount: pool.quoteVault,
        protocolFeeRecipient: protocolRecipient,
        protocolFeeRecipientTokenAccount: ata(protocolRecipient, NATIVE_MINT, TOKEN_PROGRAM_ID),
        eventAuthority: ammEventAuthorityPda(),
        ammProgram: PUMP_AMM_PROGRAM_ID,
        coinCreatorVaultAta: ata(ammVault, NATIVE_MINT, TOKEN_PROGRAM_ID),
        coinCreatorVaultAuthority: ammVault,
        globalVolumeAccumulator: globalVolumeAccumulatorPda(PUMP_AMM_PROGRAM_ID),
        userVolumeAccumulator: userVolume,
        feeConfig: ammFeeConfigPda(),
        feeProgram: FEE_PROGRAM_ID,
        poolV2: poolV2Pda(config.mint),
        buybackFeeRecipient: fees.swapBuyback,
        buybackFeeRecipientAta: ata(fees.swapBuyback, NATIVE_MINT, TOKEN_PROGRAM_ID),
      },
    })
    .preInstructions([budget(600_000)]);
  await submit(builder, `${label} execute swap`);
}

async function loadFeeRecipients(connection: Connection): Promise<FeeRecipients> {
  const [pump, amm] = await connection.getMultipleAccountsInfo([pumpGlobalPda(), ammGlobalConfigPda()]);
  if (!pump || !amm) throw new Error("pump fee config accounts are missing");
  return decodeFeeRecipients(pump.data, amm.data);
}

function methods(cranker: Cranker): Methods {
  return cranker.program.methods as unknown as Methods;
}

function budget(units: number): TransactionInstruction {
  return ComputeBudgetProgram.setComputeUnitLimit({ units });
}

function meta(pubkey: PublicKey, isWritable: boolean): AccountMeta {
  return { pubkey, isWritable, isSigner: false };
}

async function submit(builder: IxBuilder, label: string): Promise<void> {
  try {
    await builder.simulate();
  } catch (err) {
    const described = describe(err);
    if (described.code !== null && SKIP_ERRORS.has(described.code)) {
      console.log(`${label} skipped (${described.text})`);
      return;
    }
    console.error(`${label} failed: ${described.text}`);
    return;
  }
  const signature = await builder.rpc({ skipPreflight: true, commitment: "confirmed" });
  console.log(`${label} ${signature}`);
}

function describe(err: unknown): { code: number | null; text: string } {
  if (err && typeof err === "object" && "error" in err) {
    const coded = err as { error?: { errorCode?: { number?: number; code?: string } } };
    const number = coded.error?.errorCode?.number;
    const name = coded.error?.errorCode?.code;
    if (typeof number === "number") {
      return { code: number, text: name ?? ERROR_NAMES[number] ?? String(number) };
    }
  }
  const text = message(err);
  const logs = err && typeof err === "object" && "logs" in err ? (err.logs as string[] | undefined) : undefined;
  const blob = [text, ...(logs ?? [])].join("\n");
  const match = blob.match(/Error Number: (\d+)/);
  const code = match ? Number(match[1]) : null;
  const named = code !== null ? ERROR_NAMES[code] : undefined;
  const detail = logs?.find((line) => line.includes("Error") || line.includes("failed")) ?? text.split("\n")[0];
  return { code, text: named ?? detail ?? text };
}

function message(err: unknown): string {
  return err instanceof Error ? err.message : String(err);
}

export async function readIdl(file: string): Promise<Idl> {
  return JSON.parse(await readFile(file, "utf8")) as Idl;
}
