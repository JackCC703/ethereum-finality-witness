# Stateful historical updates and real proving

## Seven updates on one actual instance

The v2 actor starts at the original trusted Ethereum checkpoint, slot 7,069,376,
execution block 17,883,333. `initialize` independently checks the bootstrap header
and committee branch against the fixed checkpoint. There is no imported,
pre-advanced store trust input in this flow.

Seven successive updates settled on one local LEZ v0.4.0-rc1 instance. The first
update installs the next committee without moving finality. The following updates
rotate the current committee five times and eventually reach slot 7,109,344,
execution block 17,923,026. Account/storage verification is included in the last
update. Every resulting public commitment equals the corresponding native result.

| Action | Recorded local block | Finalized slot |
| --- | ---: | ---: |
| Initialize from checkpoint | 13 | 7,069,376 |
| Update 0, learn next committee | 16 | 7,069,376 |
| Update 1 | 18 | 7,070,047 |
| Update 2 | 21 | 7,078,240 |
| Update 3 | 23 | 7,089,280 |
| Update 4 | 26 | 7,094,272 |
| Update 5 | 28 | 7,104,096 |
| Update 6, finality and state witness | 30 | 7,109,344 |

Evidence: `evidence/sequential-dev/sequence-result.json`, independent RPC replies,
and the driver log. This seven-update run uses development receipts. A replay
is refused at wallet preflight with `FW_STALE_STORE`; it is not submitted to the
node. Ten native actor tests additionally cover cross-program or external Apply,
unbound old snapshots, bootstrap corruption, bad signatures and reinitialization.

## Trust and persistence

`verify_update` checks a supplied predecessor store and update. Its result is not
independently trusted by a consumer: the public actor's same-program-only Apply
requires `previous_store_commitment` to equal the actual current instance state.
Only a verified successor can replace that commitment. This allows arbitrary
valid historical successors within the supported profile, rather than one fixed
snapshot transition. A committee-only update may leave finality unchanged, but
must change the store commitment. Finality cannot decrease.

`rust/native/src/bin/export_sequence.rs` reconstructs and verifies all predecessors
from bootstrap; `rust/sequence.json` retains each private witness and expected
summary. The public actor stores only a compact summary, not full committees.

## Separate real-proof run

A fresh local node and wallet are started with `RISC0_DEV_MODE` unset on both
sides. The real run requests one actual private Verify -> public Apply from the
original checkpoint. This run completed and its receipt independently verified; the real update settled at local block 1101. The seven-update
development run must not be described as seven real proofs.

The driver rejects fake receipts in non-development mode, reconstructs each
receipt's journal from the actual transaction, verifies it against the official
privacy-preserving circuit image ID and archives the full receipt. It also offers
an offline `verify-receipt RECEIPT_BIN IMAGE_ID` command that rejects fake receipts.

To fit the available memory, `prover-segments.patch` sets both wallet proving
environments to `segment_limit_po2(18)`. This modifies host-side proof scheduling
only. It does not modify the program, circuit, gas limit, receipt verifier or
consensus rules. Lower segments can make CPU proving slower.

## Reproduce

Use official LEZ v0.4.0-rc1 commit
`1806e873ea2ac0e1b608aa7cf2f9b6e1fb12db21` and its native build prerequisites.
The script adds the supplied driver, copies the exact workspace lock and applies
the two-line prover segment patch. It does not enable the large in-process prover
build: the official prebuilt r0vm 3.0.5 provides IPC execution/proving.

```sh
export RISC0_SERVER_PATH=/absolute/path/to/r0vm
# Fast integration run: seven updates, placeholder receipts.
bash lez/run-sequential.sh /absolute/path/to/lez-source /absolute/path/to/fresh-dev dev
# CPU intensive: one complete real private transaction, not just guest execution.
bash lez/run-sequential.sh /absolute/path/to/lez-source /absolute/path/to/fresh-real real 1
```

The scripts use local genesis assets and publicly known official test keys only.
New instance/transaction IDs will differ. Never use these keys with real assets.
Do not distribute runtime wallet storage or node keys. Proving may require substantial
CPU time, memory and disk; actual measurements belong to the completed run.

## Remaining scope

These are recorded 2023 Capella updates, not live Ethereum following. Current
forks, checkpoint freshness, automatic network polling, operational recovery and
public testnet deployment remain separate work. The v1 snapshot artifacts and
reports are retained as historical baseline evidence; use
`artifacts/lez-actor-compact.bin` and `rust/compact-sequence.json` for the current flow. The generic full-store v2 artifact remains available as `lez-actor-sequential.bin`.

## Compact committee profile

The current actor enables `blst-backend` and `compact-store`. A committee is stored as
its 512-key SSZ vector root and its committed aggregate public key. Bootstrap and
next-committee branches authenticate the pair against the original Ethereum roots.
Every non-signer supplies its public key and a depth-nine Merkle branch at exactly
the zero-bit position. Missing, extra, reordered or unbound keys are rejected.
The signers' key is the authenticated aggregate minus those verified non-signers.
BLS verification, including signature subgroup checks, runs inside the guest.

The compact store retains canonical beacon fields and the verified execution block
number/hash/state root. Full execution-header and finality branches are verified
before this summary is stored. This reduces repeated JSON parsing/serialization
without accepting a host validity flag or an unbound committee aggregate.

On exactly the same first historical update, actual actor execution at po2=18:

| Version | User cycles | Segments |
| --- | ---: | ---: |
| Generic full-store actor | 37,044,569 | 180 |
| Official blst, full store | 23,246,065 | 114 |
| Official blst, compact store | 13,977,215 | 66 |

The first input frame falls from 111,361 to 7,370 bytes. These are execution
measurements, not proving times or transaction gas. The workload is the first
committee-only update; other updates and participation levels can cost differently.
Seven compact Ethereum results match the full-store reference at every step,
including the final account/storage witness. The compact actor also settled all
seven updates on an actual local node (initialize block81; updates84,86,89,91,93,95,98).
See `evidence/compact-dev/` and `../rust/lez-actor-fast/evidence/`.

The compact build passed 29 shared-core tests and ten actor tests. Additionally,
the actual RISC0 guest rejects a forged non-signer membership witness and a
valid-curve BLS signature from the wrong update. The original default core also
passes its 25 tests with the optional profiles disabled.

Pinned blst source: `risc0/blst` commit
`7d1fc3e6d8cf0d52a291f52455233bf8d6432ed2` (Apache-2.0). Native/guest builds used
clang 19.1.7 for C code. To avoid a chunk-count mismatch in the official example
deployment helper, the archived compact ELF has 1,517 non-loaded trailing zero
bytes. Its execution image ID is unchanged by that padding.

`../scripts/build-compact-actor.sh` rebuilds into a separate work directory and
does not overwrite reference proof artifacts. Set `FW_ACTOR_BIN` and
`FW_SEQUENCE_JSON` when running a rebuilt variant. A new build/image requires
its own proof; the archived receipt must be verified against its recorded image.
Original v1 source/locks are preserved in `../legacy/v1-source.zip`.

Completed real-proof evidence and explicit scope: [RESULT.md](evidence/compact-real/RESULT.md).
