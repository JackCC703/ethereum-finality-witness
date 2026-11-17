# Separate LEZ v0.3 public-testnet adapter

**Status: deployed and initialised on the public LEZ v0.3 testnet on 2026-11-16.** Program account `5FKkrZ6Fch7zRRNAmUuosidosJoLAgamaEciRNtdAx3x`, instance `3UEzjhNLDdBTpVx5moZdZQGV1efyUpqt9X1gv9cXENHg`. Full identities, transactions, on-chain state and the block-cadence measurements behind the upload time are in [PUBLIC-DEPLOYMENT.md](PUBLIC-DEPLOYMENT.md). The instance has applied **zero** updates. The original v0.4 actor and real proof remain the primary implementation, unchanged.

This profile targets official LEZ v0.3.1 commit `1a003bfbfb89fcf987f62975cd08a401169e1b8c` and its plan/apply interface. It reuses the same authenticated historical Capella verification core, not the v0.4 actor ABI. A v0.3 deployment does not satisfy the RFP's public v0.4 requirement.

## Verified locally

- Ten native adapter tests pass: authenticated bootstrap, seven successor commitments, reinitialization, wrong instance, wrong program shard, forged Apply instruction, replay, corrupt bootstrap, finality regression and chain binding.
- The actual zkVM binary executes initialization, first-update and final-update Plan/Apply pairs. Every journal exactly matches the corresponding native output. Actual guest executions also reject a wrong instance and corrupt bootstrap.
- The actual `lee::V03State` executes initialization and all seven transitions and rejects reinitialization, forged Apply and replays. **These update tests explicitly use a 20M in-memory cycle budget** to exercise state transitions; they are not public admission tests. A separate attempt at the actual public **10M** budget is rejected.
- The freshly built v0.3 wallet/driver connects to the official public endpoint and matches its circuit image `189513eb0a037edb638b3da824952e69894a4d11ab05c5956a5986181ab02721`, and the adapter is since deployed and initialised there — see [PUBLIC-DEPLOYMENT.md](PUBLIC-DEPLOYMENT.md).

Raw evidence: [native tests](evidence/native-tests.txt), [executor results](evidence/executor/summary.json), [state-machine results](evidence/state-machine.json), and [funding/deployment status](evidence/deployment-status.json).

| Call | Executor user cycles |
| --- | ---: |
| Initialize Plan | 414,358 |
| Initialize Apply | 9,521 |
| First update Plan | 13,962,227 |
| First update Apply | 60,418 |
| Final update Plan, including account/storage witness | 13,903,837 |
| Final update Apply | 84,095 |

These are executor measurements, not proof times or public transaction gas quotes. The whole update exceeds the public 10M cap; the driver therefore uses real privacy-preserving proving for updates. No v0.3 real-proof result exists yet. The v0.4 receipt cannot be reused as a v0.3 receipt.

## State and authorization

One instance is the PDA derived from the program's deployment account and fixed seed `[0x46; 32]`. Plan requires exactly that instance and this program's shard. Initialize authenticates bootstrap against the core's fixed historical checkpoint; Apply refuses existing state. Update verifies the full witness in Plan, then emits an internal effect. Apply compares the verified predecessor against current state before committing a nondecreasing-finality successor. There is no caller-facing instruction that directly accepts a verified effect. Runtime Plan/Apply validation binds emitted effects to this program.

The program is permissionless within this fixed historical profile. It has no live checkpoint, current-fork or real-time following claim. A public deployment would still be a historical demonstration.

## Build

The archived guest binary is [artifacts/finality-witness-v03.bin](artifacts/finality-witness-v03.bin); image ID and source/dependency fingerprints are in [manifest.json](artifacts/manifest.json). It uses the protocol-required V1COMPAT 2.2.2 kernel. Build tools: host Rust 1.98.1, RISC Zero Rust r0.1.91.1, SDK/r0vm 3.0.5, clang 19.1.7.

```sh
FW_TOOLCHAIN_ROOT=/path/to/tools FW_HOST_TOOLCHAIN=1.98.1 bash scripts/setup-risc0.sh
FW_TOOLCHAIN_ROOT=/path/to/tools FW_CLANG=/path/to/clang-19 bash lez/v03/build-guest.sh
```

Build the driver in a fresh official SDK checkout. It needs the SDK's native dependencies, including PCSC, libclang and RocksDB build support. Set `LBC_ROOT_DIR` to the extracted official v0.5.7 circuit directory if needed. The script pins the workspace lock and applies the documented host-only [proof segmentation patch](prover-segments.patch).

```sh
git clone https://github.com/logos-blockchain/logos-execution-zone.git /path/to/lez-v031
git -C /path/to/lez-v031 checkout 1a003bfbfb89fcf987f62975cd08a401169e1b8c
bash lez/v03/build-driver.sh /path/to/lez-v031 /path/to/v03-target
```

## Wallet and deployment

Keep the runtime directory **outside the repository**. Prepare generates a fresh random testnet signing key and wallet unless a separate secret has already been supplied locally; it refuses to replace an existing wallet. It never imports the public local-genesis keys used by old demonstrations. The runtime contains secrets and must not be published.

```sh
export RISC0_SERVER_PATH=/path/to/r0vm RISC0_EXECUTOR=ipc RISC0_FEATURE_bigint2=1
unset RISC0_DEV_MODE
/path/to/v03-target/debug/finality-witness-v03-driver prepare /path/to/private-v03-run
/path/to/v03-target/debug/finality-witness-v03-driver check /path/to/private-v03-run
```

For the prepared run in this session, the public payer is `ArSC2eGjcRXCm3bFTfTFAgR8jUcep2ksSMXb1roHTDK`. It currently has no test tokens. A normal [official faucet claim](https://github.com/logos-co/lez-faucet/tree/main/claimd) was attempted once with a persistent idempotency key, but the hosted service's HTTPS connection returned 403 in this environment. No claim is known to have reached the service. Do not buy tokens or use real assets.

Once funded, the driver uploads the actual guest ELF in segments, creates an immutable loader header, initializes the instance, and checks the resulting on-chain state. Upload progress is saved after every included and checked segment and can resume using the same binary and wallet.

```sh
/path/to/v03-target/debug/finality-witness-v03-driver deploy /path/to/private-v03-run \
  lez/v03/artifacts/finality-witness-v03.bin rust/compact-sequence.json
# Real proving, substantial CPU time. Run only after deployment is verified:
/path/to/v03-target/debug/finality-witness-v03-driver update /path/to/private-v03-run \
  lez/v03/artifacts/finality-witness-v03.bin rust/compact-sequence.json 0
```

The v0.3 SDK's block type has no per-transaction revert bitmap. The driver therefore checks transaction inclusion and exact resulting segment/header/instance state; it does not infer successful execution from inclusion alone or fabricate a revert flag. A real update additionally archives and independently verifies the original cryptographic receipt against the v0.3 circuit.

If the SDK's direct TCP connection cannot use the environment's configured HTTPS proxy, `rpc-relay.py` forwards unmodified HTTP request/response bodies to the fixed official public endpoint, listening only on loopback. It is a transport bridge, **not a sequencer**. Set the run's wallet `sequencer.sequencer_addr` to `http://127.0.0.1:4091` when using it; all public evidence must still identify the official upstream.

```sh
python3 lez/v03/rpc-relay.py
```

Funding, public deployment, a public accepted update, and a real v0.3 receipt remain distinct milestones. Update status only after independent evidence exists.
