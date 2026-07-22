# SukiSave

On-chain micro-savings vaults for street vendors, built on Stellar Soroban.

## Problem

Rosa, a fish ball cart vendor in Marilao, Bulacan, earns roughly ₱800 a day in
cash but has no bank account. Her earnings get absorbed into daily household
spending, or handed over to an informal "paluwagan" collector who takes a
cut. When her cart eventually breaks down, she has nothing saved and has to
borrow at predatory interest just to keep working.

## Solution

SukiSave lets a vendor set a concrete savings goal (e.g. "₱5,000 for a new
cart wheel") and tap "Save ₱20" after every sale. Each tap deposits a small
amount of USDC into a personal, on-chain vault via a Soroban smart contract.
Funds stay locked until the goal is met — or the vendor can withdraw early
for a small penalty in a genuine emergency. Stellar's near-zero transaction
fees are what make dozens of tiny daily deposits actually viable; on most
other chains, gas fees alone would eat the savings.

## Timeline (hackathon build)

- **Day 1:** Contract design, `lib.rs` core logic (goal setting, deposit,
  withdraw, early withdraw), local unit tests.
- **Day 2:** Testnet deployment, CLI invocation walkthrough, mobile-first
  front end wired to the contract, demo script polish.

## Stellar Features Used

- **XLM / USDC transfers** — the actual movement of value on each deposit
  and withdrawal.
- **Soroban smart contracts** — the savings vault logic (goals, balances,
  penalties) lives entirely on-chain.
- **Trustlines** — vendors establish a trustline to USDC issued by a local
  anchor.
- **Built-in DEX** — optional XLM → USDC conversion path for vendors who
  start with only XLM.

## Vision and Purpose

Informal savings groups ("paluwagan," ROSCAs, and similar mechanisms) exist
across Southeast Asia precisely because unbanked vendors need a way to
commit to saving. SukiSave keeps that same behavioral commitment device —
but removes the collector, the theft risk, and the opacity, replacing them
with a transparent, low-fee, vendor-controlled smart contract. The goal is
not to introduce vendors to "crypto" — it's to give them a safer version of
a savings habit they already practice.

## Prerequisites

- Rust (stable toolchain, 1.74+) with the `wasm32-unknown-unknown` target:
  ```bash
  rustup target add wasm32-unknown-unknown
  ```
- Soroban CLI v21+:
  ```bash
  cargo install --locked soroban-cli --features opt
  ```

## Build

```bash
soroban contract build
```

The compiled Wasm binary will be at
`target/wasm32-unknown-unknown/release/sukisave.wasm`.

## Test

```bash
cargo test
```

This runs all five tests in `src/test.rs`, covering the happy path, an
unauthorized-caller failure, storage state verification, a premature
withdrawal failure, and the early-withdrawal penalty path.

## Deploy to Testnet

```bash
soroban contract deploy \
  --wasm target/wasm32-unknown-unknown/release/sukisave.wasm \
  --source alice \
  --network testnet
```

This prints the deployed contract ID — save it for the next steps.

Then initialize the contract with the USDC token address you want it to
accept:

```bash
soroban contract invoke \
  --id <CONTRACT_ID> \
  --source alice \
  --network testnet \
  -- \
  initialize \
  --token <USDC_TOKEN_CONTRACT_ADDRESS>
```

## Sample CLI Invocation (MVP flow)

Set a goal:

```bash
soroban contract invoke \
  --id <CONTRACT_ID> \
  --source vendor_rosa \
  --network testnet \
  -- \
  set_goal \
  --vendor <ROSA_ADDRESS> \
  --goal_amount 5000 \
  --goal_name "New cart wheel"
```

Deposit after a sale:

```bash
soroban contract invoke \
  --id <CONTRACT_ID> \
  --source vendor_rosa \
  --network testnet \
  -- \
  deposit \
  --vendor <ROSA_ADDRESS> \
  --amount 20
```

Withdraw once the goal is met:

```bash
soroban contract invoke \
  --id <CONTRACT_ID> \
  --source vendor_rosa \
  --network testnet \
  -- \
  withdraw \
  --vendor <ROSA_ADDRESS>
```
LINK

🔗 https://stellar.expert/explorer/testnet/tx/ef11893eadcc76751f6fd78c530ecf0539c2b4f8312a01f673a2fd7f8a91f12f
🔗 https://lab.stellar.org/r/testnet/contract/CCB5GUWMTBYQ7O4GEJ6II27L4S6SVGJKIO7FXN6JQ7CSNTSMVQM7H63C

## License

MIT

