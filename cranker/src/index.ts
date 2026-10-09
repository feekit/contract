import { parseArgs } from "node:util";
import { Connection, PublicKey } from "@solana/web3.js";
import { fileURLToPath } from "node:url";
import {
  clusterUrl,
  defaultKeypairPath,
  loadKeypair,
  openCranker,
  printStatus,
  readIdl,
  runOnce,
} from "./crank.js";

const USAGE = `Usage: npm run crank -- <command> [options]

Commands:
  run       Watch every locked FeeKit launch and crank it.
  once      Crank once and exit.
  status    List FeeKit configs.

Options:
  --cluster mainnet|devnet|localnet   default mainnet
  --url <rpc>                         overrides the cluster URL
  --keypair <path>                    default ~/.config/solana/id.json
  --interval <ms>                     run loop delay, default 20000
  --mint <pubkey>                     only this coin
  --idl <path>                        default cranker/idl/feekit.json
`;

function optionValue(value: string | boolean | undefined, name: string): string {
  if (typeof value !== "string" || value.length === 0) {
    throw new Error(`missing --${name}`);
  }
  return value;
}

function optional(value: string | boolean | undefined): string | undefined {
  return typeof value === "string" && value.length > 0 ? value : undefined;
}

async function main() {
  const [command, ...argv] = process.argv.slice(2);
  if (!command || command === "help" || command === "--help") {
    console.log(USAGE);
    return;
  }

  const { values } = parseArgs({
    args: argv,
    options: {
      cluster: { type: "string", default: "mainnet" },
      url: { type: "string" },
      keypair: { type: "string" },
      interval: { type: "string" },
      mint: { type: "string" },
      idl: { type: "string" },
    },
    strict: true,
  });

  const connection = new Connection(
    optional(values.url) ?? clusterUrl(optionValue(values.cluster, "cluster")),
    "confirmed",
  );
  const payer = loadKeypair(optional(values.keypair) ?? defaultKeypairPath());
  const idlPath =
    optional(values.idl) ?? fileURLToPath(new URL("../idl/feekit.json", import.meta.url));
  const cranker = openCranker(connection, payer, await readIdl(idlPath));
  const mint = optional(values.mint) ? new PublicKey(optionValue(values.mint, "mint")) : null;

  if (command === "status") {
    await printStatus(cranker, mint);
    return;
  }

  if (command === "once") {
    await runOnce(cranker, mint);
    return;
  }

  if (command === "run") {
    const interval = Number(optional(values.interval) ?? "20000");
    if (!Number.isFinite(interval) || interval < 1000) {
      throw new Error("--interval is milliseconds and must be at least 1000");
    }
    console.log(`cranking every ${interval}ms as ${payer.publicKey.toBase58()}`);
    for (;;) {
      await runOnce(cranker, mint);
      await new Promise((resolve) => setTimeout(resolve, interval));
    }
  }

  throw new Error(`unknown command ${command}`);
}

main().catch((err: unknown) => {
  console.error(err instanceof Error ? err.message : err);
  process.exitCode = 1;
});
