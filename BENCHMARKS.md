# RISC0 execution baseline

Measured by the actual RISC Zero 3.0.5 executor. No prover was called and no proof/receipt was generated.

- Image ID: `068b791223b43893a462f188fa50cf93169f55fdd13174c7521e8149c6d12e09`
- Total executor user cycles: **1,747,839,425**
- Segments at po2=20: 1,770
- Executor wall time: 29.756 seconds (NOT proving time)
- Machine: AMD EPYC 9V74 80-Core Processor; CPU quota `200000 100000`; memory limit 8589934592 bytes.
- Peak memory and target-laptop performance were not measured.

The workload starts from the pinned historical checkpoint, verifies all seven updates, and then verifies the deposit-contract account and slot 0 under finalised block 17,923,026.

| Phase | Guest cycle delta |
| --- | ---: |
| Bootstrap (after input decoding) | 18,389,620 |
| Update 0, entire verification | 251,097,267 |
| Update 1, entire verification | 244,979,322 |
| Update 2, entire verification | 251,126,080 |
| Update 3, entire verification | 257,268,873 |
| Update 4, entire verification | 244,979,326 |
| Update 5, entire verification | 244,979,398 |
| Update 6, entire verification | 227,898,055 |
| Account + one storage proof | 959,450 |

BLS signature sections together: **1,612,944,579** guest cycles.

Method: phase boundaries use guest `env::cycle_count()`. They include bookkeeping between adjacent boundaries; they are not separately executed LEZ transactions. Input parsing, instrumentation, serialization and execution overhead explain differences from total executor user cycles.

This is a **stock `bls12_381` software baseline**, with two pairing evaluations per signature and no RISC0 cryptographic acceleration. It replays consensus history from scratch. It is not comparable to a competitor's incremental, accelerated single update by dividing totals or comparing wall time.

This stock full replay is not a single LEZ transaction. See the separately measured acceleration and incremental sections below; neither full replay nor incremental execution fits the 10M public cap.

Negative inputs are also executed in the same guest. The native consumer independently checks proof validity before committing state. See reports and `scripts/check-results.py`.

Toolchains: Rust 1.91.1; RISC0 Rust r0.1.91.1; SDK/r0vm 3.0.5. Dependency lockfiles and artifact hashes are in `artifacts/manifest.json`.

## Official acceleration and equivalent pairing optimization

| Configuration | Seven-update executor user cycles |
| --- | ---: |
| Original stock baseline | 1,747,839,425 |
| Official BLS finite-field patch only | 417,787,748 |
| Official BLS + SHA2 patches, combined pairing | 231,806,170 |

Optimized cycle speedup: **7.540x**. Verified result is identical.
Pinned official sources and licences are recorded in rust/accelerated/PROVENANCE.json. The combined pairing uses upstream multi_miller_loop and one final_exponentiation; it does not skip BLS verification.
The original stock workspace remains unpatched. Round-one configuration is retained in rust/accelerated/bls-only/. Bad-signature and false-storage witnesses are rejected by the optimized guest.

## One incremental historical update

- Actual executor user cycles: **32,211,104** (includes matching account/storage verification).
- Image ID: `097ce76abaf5bdfccaf08f675a96f0dce050b7293ffffda11b71a7617a286567`
- The snapshot is rebuilt by verifying the original bootstrap and six updates, then the guest verifies the seventh/finality update.
- Snapshot commitment: `76084610a9e4dfe1aea101c87bf9734b3d7a5a423d0d63f222c2b75ecfdc3aac`.
- Importing this pinned historical snapshot is an explicit trust boundary, not a verified prior zk receipt or a fresh checkpoint.
- This exceeds LEZ v0.4 public MAX_GAS_EXEC=10,000,000. The node cap was not increased. Private Verify -> public Apply is the intended local placement.

All standalone comparisons use V1COMPAT kernel 2.2.3. The separate LEZ actor targets 2.2.2 and real actor framing. A different kernel/image is not silently treated as the same benchmark.
Separate local LEZ evidence in lez/evidence/result.json: init block175, private Verify -> public Apply block177, external forged Apply reverted block178; replay refused in wallet preflight before submission. Slot7104096 ->7109344 and execution block17917816 ->17923026. This uses standalone mock Bedrock and dev-mode placeholder receipts, not real zk proofs or a public testnet. These node results are separate from executor measurements.
No proving time, zk receipt, proof size or LEZ gas follows from executor cycles.

## Stateful actor and compact committee profile

Seven historical updates settled on one actual local LEZ instance, including committee transitions. The compact run uses development receipts; see lez/evidence/compact-dev/.

| First historical update, actor framing, po2=18 | User cycles |
| --- | ---: |
| Generic full store | 37,044,569 |
| Official blst, full store | 23,246,065 |
| Official blst, compact authenticated store | 13,977,215 |

All seven Ethereum verification results match the full-store reference. Committee aggregate roots and exact non-signer Merkle witnesses remain verified inside the guest.
These are executor measurements, not proving times or transaction gas. The first update learns the next committee without advancing finality. See lez/CONTINUOUS-UPDATES.md.

## Complete real private transaction

A real private committee update was accepted by the standalone node at block 1101. RISC0_DEV_MODE was unset on wallet/node. Its whole wallet proving-to-inclusion time was 26113.142 seconds on the recorded hardware.
This includes the actor proof, privacy-preserving circuit proof and block wait; it is not a guest-only proving measurement.
The archived receipt independently verifies against the official circuit image. Wrong-image and journal-tamper checks fail. Only this one update was run with real proofs; the seven-update run uses development receipts. Local standalone mock Bedrock, not public testnet.
