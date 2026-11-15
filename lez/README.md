# Local LEZ update evidence (legacy snapshot run)

For the current stateful compact actor, seven-update integration and completed real proof, see [CONTINUOUS-UPDATES.md](CONTINUOUS-UPDATES.md) and [compact-real/RESULT.md](evidence/compact-real/RESULT.md). The remainder of this page records the earlier snapshot-based run.

This demonstration deployed a real actor through the official program loader,
then submitted transactions over JSON-RPC to a running LEZ v0.4.0-rc1 node.
The node used standalone mock Bedrock and `RISC0_DEV_MODE=1`. Its receipts are
development placeholders, not cryptographic proofs. This is not a public testnet
deployment or a production bridge.

## Recorded outcome

| Operation | Local block | Reverted | Result |
| --- | ---: | --- | --- |
| Fund private verifier | 163 | false | Local genesis test funds only |
| Initialize public instance | 175 | false | Pinned snapshot, slot 7,104,096 |
| Private Verify → public Apply | 177 | false | Slot 7,109,344; execution block 17,923,026 |
| External forged Apply | 178 | true | Instance state unchanged |
| Repeat the same update | not submitted | — | Wallet preflight rejects `FW_STALE_STORE`; state unchanged |

Program account: `tUZFvxwtVw89BuXSCbBGssZKoqrdhR4TzSbqJTQdd8B`.
Instance: `8hqwCm49GLeNqu1FmzjuVMNTKLZWeWCcZgk8BiNH1vw2`.
These identifiers belong only to the recorded ephemeral local chain.

The account/storage witness is anchored to the finalized Ethereum state root
`226f5ff47ab3725b5a4a3afc74b1e79e4aa3a29704561eccce590e58900baec3`.
The actor verifies one update against a hardcoded historical snapshot commitment.
It is a single-transition demonstration, not a reusable follower: there is no
second transition fixture, checkpoint freshness policy, fork upgrade support,
or prior-proof attestation of that snapshot. The full-history executor separately
checks the checkpoint and seven updates that produced the state.

`verify` runs privately; it sends an authenticated same-program `apply` message
with the verified summary. `apply` checks sender provenance, the previous store
commitment and increasing finalized slot. Only a small summary is persisted
publicly. The standard public gas limit was not changed. The standalone
incremental verifier still costs 32,211,104 user cycles, so its heavy verification
is not a public root call.

## Evidence

- `evidence/result.json`: transaction identifiers, inclusion heights, revert flags,
  states and the exact replay-preflight error. The recorded 6.716-second sequence
  includes the update and subsequent negative checks; it is not proof time.
- `evidence/rpc-responses.json`: independent `getTransaction` and `getBlock`
  responses fetched from the running node after the driver completed. Binary
  transactions and blocks use the official RPC base64 encoding.
- `evidence/state-before.json`, `state-after.json`: decoded public actor state.
- `evidence/local-demo.json`: official local network config with a two-second
  block timeout and one funded, publicly known local test key.
- `evidence/driver-log.txt`: filtered driver output, including dev-mode warnings.
- `../rust/lez-actor/native-tests.txt`: seven native actor handler checks. These
  are distinct from the actual RPC transactions above.

No wallet storage, mnemonics, private node keys, or node data directory are
included in the deliverable. The deterministic `[42; 32]` key in the driver is
only for an isolated local genesis and must never hold real assets.

## Reproduce

Use a fresh clone of the official LEZ repository at
`1806e873ea2ac0e1b608aa7cf2f9b6e1fb12db21` (v0.4.0-rc1), its Rust 1.98.1
toolchain and native prerequisites. Set `RISC0_SERVER_PATH` to r0vm 3.0.5.
`LBC_ROOT_DIR` may point at the official v0.5.7 prebuilt blockchain circuits
when the build cannot download them. RocksDB, libclang, PCSC and platform build
dependencies must be available.

```sh
export RISC0_SERVER_PATH=/absolute/path/to/r0vm
bash lez/run-local.sh /absolute/path/to/lez-source /absolute/path/to/fresh-run
```

The script adds the included driver as a workspace member in that fresh clone,
builds it and the official standalone node, prepares an ephemeral wallet, starts
the node, deploys the archived actor, performs the update/negative checks and
stops the node. Port 3040 must be free. Build and deployment can take time.
Fresh local account and transaction identifiers will differ from this recording.

The archived LEZ actor uses the official V1COMPAT kernel 2.2.2, as required by
the loader. The separate executor-only verifier artifacts use kernel 2.2.3.
Their image IDs and SHA-256 hashes are in `../artifacts/manifest.json`.
