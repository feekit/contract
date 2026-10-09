import { BN } from "@coral-xyz/anchor";
import {
  createAssociatedTokenAccountIdempotentInstruction,
  getAssociatedTokenAddressSync,
  TOKEN_2022_PROGRAM_ID,
  TOKEN_PROGRAM_ID,
} from "@solana/spl-token";
import {
  ComputeBudgetProgram,
  Connection,
  Keypair,
  PublicKey,
  SystemProgram,
  TransactionInstruction,
  TransactionMessage,
  VersionedTransaction,
} from "@solana/web3.js";
import { homedir } from "node:os";
import { join } from "node:path";
import { fileURLToPath } from "node:url";
import {
  ammCreatorVaultPda,
  ammEventAuthorityPda,
  ata,
  bondingCurvePda,
  decodeLaunchConfig,
  pumpCreatorVaultPda,
  pumpEventAuthorityPda,
  pumpGlobalPda,
  sharingConfigPda,
} from "./accounts.js";
import { loadKeypair, openCranker, readIdl, runOnce } from "./crank.js";
import { FEE_PROGRAM_ID, NATIVE_MINT, PUMP_AMM_PROGRAM_ID, PUMP_PROGRAM_ID } from "./ids.js";

const MAYHEM_PROGRAM_ID = new PublicKey("MAyhSmzXzV1pTf7LsNkrNwkWKTo4ougAJ1PPg47MD4e");
const ASSOCIATED_TOKEN_PROGRAM_ID = new PublicKey("ATokenGPvbdGVxr1b2hvZbsiqW5xWH25efTNsLJA8knL");
const CREATE_V2 = Buffer.from([214, 144, 76, 236, 95, 139, 49, 180]);
const BUY_EXACT_QUOTE_IN_V2 = Buffer.from([194, 171, 28, 70, 104, 77, 91, 47]);

function pda(seeds: Array<Buffer | Uint8Array>, program: PublicKey): PublicKey {
  return PublicKey.findProgramAddressSync(seeds, program)[0];
}

function encodeString(value: string): Buffer {
  const bytes = Buffer.from(value, "utf8");
  const len = Buffer.alloc(4);
  len.writeUInt32LE(bytes.length);
  return Buffer.concat([len, bytes]);
}

function encodeCreate(name: string, symbol: string, uri: string, creator: PublicKey): Buffer {
  const feeBps = Buffer.alloc(8);
  return Buffer.concat([
    CREATE_V2,
    encodeString(name),
    encodeString(symbol),
    encodeString(uri),
    creator.toBuffer(),
    Buffer.from([0, 0]),
    feeBps,
    Buffer.from([0]),
  ]);
}

async function send(
  connection: Connection,
  payer: Keypair,
  instructions: TransactionInstruction[],
  extra?: Keypair[],
): Promise<string> {
  const latest = await connection.getLatestBlockhash();
  const message = new TransactionMessage({
    payerKey: payer.publicKey,
    recentBlockhash: latest.blockhash,
    instructions,
  }).compileToV0Message();
  const tx = new VersionedTransaction(message);
  tx.sign([payer, ...(extra ?? [])]);
  const signature = await connection.sendTransaction(tx, { skipPreflight: false });
  await connection.confirmTransaction({ signature, ...latest }, "confirmed");
  return signature;
}

async function createCoin(connection: Connection, creator: Keypair): Promise<PublicKey> {
  const mint = Keypair.generate();
  const bondingCurve = bondingCurvePda(mint.publicKey);
  const mayhemSol = pda([Buffer.from("sol-vault")], MAYHEM_PROGRAM_ID);
  const ix = new TransactionInstruction({
    programId: PUMP_PROGRAM_ID,
    keys: [
      { pubkey: mint.publicKey, isSigner: true, isWritable: true },
      { pubkey: pda([Buffer.from("mint-authority")], PUMP_PROGRAM_ID), isSigner: false, isWritable: false },
      { pubkey: bondingCurve, isSigner: false, isWritable: true },
      {
        pubkey: getAssociatedTokenAddressSync(mint.publicKey, bondingCurve, true, TOKEN_2022_PROGRAM_ID),
        isSigner: false,
        isWritable: true,
      },
      { pubkey: pumpGlobalPda(), isSigner: false, isWritable: false },
      { pubkey: creator.publicKey, isSigner: true, isWritable: true },
      { pubkey: SystemProgram.programId, isSigner: false, isWritable: false },
      { pubkey: TOKEN_2022_PROGRAM_ID, isSigner: false, isWritable: false },
      { pubkey: ASSOCIATED_TOKEN_PROGRAM_ID, isSigner: false, isWritable: false },
      { pubkey: MAYHEM_PROGRAM_ID, isSigner: false, isWritable: true },
      { pubkey: pda([Buffer.from("global-params")], MAYHEM_PROGRAM_ID), isSigner: false, isWritable: false },
      { pubkey: mayhemSol, isSigner: false, isWritable: true },
      {
        pubkey: pda([Buffer.from("mayhem-state"), mint.publicKey.toBuffer()], MAYHEM_PROGRAM_ID),
        isSigner: false,
        isWritable: true,
      },
      {
        pubkey: getAssociatedTokenAddressSync(mint.publicKey, mayhemSol, true, TOKEN_2022_PROGRAM_ID),
        isSigner: false,
        isWritable: true,
      },
      { pubkey: pumpEventAuthorityPda(), isSigner: false, isWritable: false },
      { pubkey: PUMP_PROGRAM_ID, isSigner: false, isWritable: false },
    ],
    data: encodeCreate("FeeKit Test", "FKT", "https://feekit.fun", creator.publicKey),
  });
  const signature = await send(connection, creator, [ix], [mint]);
  console.log(`created ${mint.publicKey.toBase58()} ${signature}`);
  return mint.publicKey;
}

async function main() {
  const connection = new Connection("http://127.0.0.1:8899", "confirmed");
  const creator = loadKeypair(join(homedir(), ".config", "solana", "id.json"));
  const idlPath = fileURLToPath(new URL("../idl/feekit.json", import.meta.url));
  const idl = await readIdl(idlPath);
  const cranker = openCranker(connection, creator, idl);
  const programId = cranker.programId;

  const mint = await createCoin(connection, creator);
  const config = pda([Buffer.from("config"), mint.toBuffer()], programId);
  const solVault = pda([Buffer.from("vault"), mint.toBuffer()], programId);
  const curve = bondingCurvePda(mint);

  await cranker.program.methods
    .initializeVault({
      kit: 0,
      slippageBps: 500,
      minExecuteLamports: new BN(50_000),
      maxSpendLamports: new BN(0),
      intervalSlots: new BN(0),
      drawdownBps: 0,
      spendBps: 10_000,
      markDelaySlots: new BN(0),
    })
    .accounts({
      creator: creator.publicKey,
      mint,
      bondingCurve: curve,
      config,
      solVault,
      systemProgram: SystemProgram.programId,
    })
    .rpc();
  console.log(`initialized vault ${solVault.toBase58()}`);

  const sharing = sharingConfigPda(mint);
  const pumpVault = pumpCreatorVaultPda(sharing);
  const ammVault = ammCreatorVaultPda(sharing);
  await cranker.program.methods
    .lockFees()
    .accounts({
      creator: creator.publicKey,
      config,
      solVault,
      mint,
      global: pumpGlobalPda(),
      sharingConfig: sharing,
      bondingCurve: curve,
      systemProgram: SystemProgram.programId,
      pumpProgram: PUMP_PROGRAM_ID,
      pumpEventAuthority: pumpEventAuthorityPda(),
      feeEventAuthority: pda([Buffer.from("__event_authority")], FEE_PROGRAM_ID),
      feeProgram: FEE_PROGRAM_ID,
      pumpCreatorVault: pumpVault,
      pumpCreatorVaultAta: ata(pumpVault, NATIVE_MINT, TOKEN_PROGRAM_ID),
      quoteMint: NATIVE_MINT,
      quoteTokenProgram: TOKEN_PROGRAM_ID,
      associatedTokenProgram: ASSOCIATED_TOKEN_PROGRAM_ID,
      coinCreatorVaultAuthority: ammVault,
      coinCreatorVaultAta: ata(ammVault, NATIVE_MINT, TOKEN_PROGRAM_ID),
      ammProgram: PUMP_AMM_PROGRAM_ID,
      ammEventAuthority: ammEventAuthorityPda(),
      pool: programId,
    })
    .preInstructions([ComputeBudgetProgram.setComputeUnitLimit({ units: 600_000 })])
    .rpc();
  const locked = decodeLaunchConfig((await connection.getAccountInfo(config))!.data);
  if (!locked.locked || !locked.sharingConfig.equals(sharing)) {
    throw new Error("fee routing did not lock to the FeeKit vault");
  }
  console.log("locked creator fees to the FeeKit vault");

  const spend = 2_000_000_000n;
  const userBaseAta = getAssociatedTokenAddressSync(mint, creator.publicKey, true, TOKEN_2022_PROGRAM_ID);
  const createUserAta = createAssociatedTokenAccountIdempotentInstruction(
    creator.publicKey,
    userBaseAta,
    creator.publicKey,
    mint,
    TOKEN_2022_PROGRAM_ID,
  );
  const buy = new TransactionInstruction({
    programId: PUMP_PROGRAM_ID,
    keys: buyAccounts(creator.publicKey, mint, curve),
    data: Buffer.concat([
      BUY_EXACT_QUOTE_IN_V2,
      u64(spend),
      u64(1n),
    ]),
  });
  console.log(`bought ${await send(connection, creator, [createUserAta, buy])}`);

  await runOnce(cranker, mint);
  const after = decodeLaunchConfig((await connection.getAccountInfo(config))!.data);
  console.log(
    `fees=${after.feesCollected} spent=${after.solSpent} bought=${after.tokensBought} burned=${after.tokensBurned}`,
  );
  if (after.feesCollected === 0n || after.tokensBurned === 0n || after.solSpent === 0n) {
    throw new Error("crank did not collect fees and burn tokens");
  }
  if (after.tokensBought !== after.tokensBurned) {
    throw new Error("bought tokens were not all burned");
  }
}

function u64(value: bigint): Buffer {
  const out = Buffer.alloc(8);
  out.writeBigUInt64LE(value);
  return out;
}

function buyAccounts(user: PublicKey, mint: PublicKey, curve: PublicKey) {
  const { feeRecipient, buyback } = {
    feeRecipient: new PublicKey("62qc2CNXwrYqQScmEdiZFFAnJR262PxWEuNQtxfafNgV"),
    buyback: new PublicKey("5YxQFdt3Tr9zJLvkFccqXVUwhdTWJQc1fFg2YPbxvxeD"),
  };
  const sharing = sharingConfigPda(mint);
  const creatorVault = pumpCreatorVaultPda(sharing);
  const userVolume = pda([Buffer.from("user_volume_accumulator"), user.toBuffer()], PUMP_PROGRAM_ID);
  return [
    [pumpGlobalPda(), false, false],
    [mint, false, false],
    [NATIVE_MINT, false, false],
    [TOKEN_2022_PROGRAM_ID, false, false],
    [TOKEN_PROGRAM_ID, false, false],
    [ASSOCIATED_TOKEN_PROGRAM_ID, false, false],
    [feeRecipient, true, false],
    [ata(feeRecipient, NATIVE_MINT, TOKEN_PROGRAM_ID), true, false],
    [buyback, true, false],
    [ata(buyback, NATIVE_MINT, TOKEN_PROGRAM_ID), true, false],
    [curve, true, false],
    [getAssociatedTokenAddressSync(mint, curve, true, TOKEN_2022_PROGRAM_ID), true, false],
    [ata(curve, NATIVE_MINT, TOKEN_PROGRAM_ID), true, false],
    [user, true, true],
    [getAssociatedTokenAddressSync(mint, user, true, TOKEN_2022_PROGRAM_ID), true, false],
    [ata(user, NATIVE_MINT, TOKEN_PROGRAM_ID), true, false],
    [creatorVault, true, false],
    [ata(creatorVault, NATIVE_MINT, TOKEN_PROGRAM_ID), true, false],
    [sharing, false, false],
    [pda([Buffer.from("global_volume_accumulator")], PUMP_PROGRAM_ID), false, false],
    [userVolume, true, false],
    [ata(userVolume, NATIVE_MINT, TOKEN_PROGRAM_ID), true, false],
    [pda([Buffer.from("fee_config"), PUMP_PROGRAM_ID.toBuffer()], FEE_PROGRAM_ID), false, false],
    [FEE_PROGRAM_ID, false, false],
    [SystemProgram.programId, false, false],
    [pumpEventAuthorityPda(), false, false],
    [PUMP_PROGRAM_ID, false, false],
  ].map(([pubkey, isWritable, isSigner]) => ({
    pubkey: pubkey as PublicKey,
    isWritable: isWritable as boolean,
    isSigner: isSigner as boolean,
  }));
}

main().catch((err: unknown) => {
  console.error(err instanceof Error ? err.message : err);
  if (err && typeof err === "object" && "logs" in err) {
    console.error((err.logs as string[]).join("\n"));
  }
  process.exitCode = 1;
});
