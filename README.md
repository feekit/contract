# FeeKit

FeeKit locks pump.fun creator fees to one job per coin. The creator launches a normal pump coin, then this program takes the fee route away from that wallet. Anyone can crank the job. The kit vault has no withdraw instruction.

Program id: `9EMWVqoVNW9armPPwiY7DtW7kgQ14LTgk8F3mCkMZ11C`

SOL-quote coins only. A coin that already pays someone else, or a holder-rewards coin, cannot be locked.

## Fee split

Pump assigns creator fees **85%** to the kit vault and **15%** to the FeeKit treasury, then the sharing admin is revoked. On `collect_fees`, **5%** of the whole creator fee is paid to the crank from the vault's incoming share. The kit keeps **80%**. The treasury keeps **15%**.

| Share | Where it goes |
| --- | --- |
| 80% | Kit vault. Stays there until the kit spends it. |
| 15% | Platform treasury. The platform authority can withdraw this. |
| 5% | Keeper who runs the collect. |

`withdraw_platform` can only move the treasury. It cannot touch a kit vault.

The vault is a zero-data system account so pump can transfer SOL into it. `0.003` SOL stays behind as a buffer so a buy can open pump's volume account without dropping the vault under rent.

## Kits

`initialize_vault` accepts three kit bytes. The choice is permanent.

| Kit | Byte | What a crank does |
| --- | --- | --- |
| Burn | `0` | Buys the coin with the vault and burns the tokens. On the bonding curve until graduation, then on the canonical PumpSwap pool. |
| Floor | `3` | Same buy-and-burn, and only while spot is under a line beneath the high. The line is `drawdown_bps` under that high, and it only moves up. This is not a resting bid. |
| Graduate | `5` | Buys the bonding curve until it completes, with slippage fixed at 5%. `release_creator` then pays the spendable vault to the creator stored at initialize. Graduate never buys on PumpSwap. |

Kit bytes `1` (Drip), `2` (Catch), and `4` (Rally) are retired. `initialize_vault` rejects them. Do not reuse those numbers. The rally fields on `LaunchConfig` are leftover account space, not a live instruction.

Draw is not a kit byte. An app that wants a draw configures a Burn vault with `min_execute_lamports` set to `u64::MAX`, so `execute_curve` and `execute_swap` can never spend it. This program does not pick a winner and does not pay one.

## Instructions

Call `initialize_platform` once per deployment. The signer becomes the platform authority.

A new coin is two steps, both signed by the pump creator:

1. `initialize_vault` with the kit parameters.
2. `lock_fees`, which installs the 85/15 split and revokes pump's sharing admin. If the coin has already graduated, pass the canonical PumpSwap pool as the remaining account.

After that, anyone can crank:

| Instruction | When |
| --- | --- |
| `collect_fees` | Pull bonding-curve creator fees into the vault and pay the keeper. |
| `sweep_amm_fees` | First, once the coin has graduated, so PumpSwap fees are wrapped and collected. |
| `checkpoint` | On its own, to maintain a Floor coin's high. |
| `execute_curve` | Buy and burn on the bonding curve. The last buy can complete the curve. |
| `execute_swap` | Buy and burn on the canonical pool. Rejected for Graduate. |
| `release_creator` | Graduate only, after the curve is complete. |

A buy spends only when the vault has at least `min_execute_lamports` above the rent buffer. The default minimum is `50_000` lamports.

## Build

Anchor 0.30.1. From the repo root:

```bash
anchor build
cargo test --manifest-path programs/feekit/Cargo.toml
```

`Anchor.toml` targets localnet. The IDL is written to `target/idl/feekit.json`. The cranker ships a copy at `cranker/idl/feekit.json`.

## Local validator

`scripts/local-validator.sh` clones the pump programs from mainnet and loads `target/deploy/feekit.so`. It always starts with `--reset`, which wipes the local ledger.

```bash
bash scripts/local-validator.sh
```

RPC is `http://127.0.0.1:8899`. After a reset, run `initialize_platform` again before locking a coin.

## Cranker

The TypeScript cranker watches locked coins and sends collect, sweep, checkpoint, execute, and release. It does not choose a Draw winner.

```bash
cd cranker
npm install
npm run crank -- status
npm run crank -- once
npm run crank -- run
```

`--cluster` is `mainnet`, `devnet`, or `localnet`. The default is mainnet. `--keypair` defaults to `~/.config/solana/id.json`. `--interval` defaults to 20 seconds.

```bash
npm run crank -- run --cluster localnet --interval 20000
```

## Layout

```
programs/feekit/   Anchor program
cranker/           Open cranker and IDL
scripts/           Local validator
```
