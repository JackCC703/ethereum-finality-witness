# Official RISC Zero crypto acceleration

The original `rust/Cargo.toml` and lock retain crates.io stock BLS/SHA2. This workspace patches
only official RISC Zero repositories at fixed commits (see PROVENANCE.json and Cargo.lock).
It uses the same core and full guest source files; `combined-pairing` checks the equivalent
product pairing equation with one final exponentiation. `bls-only/` preserves the first-round
configuration with no combined pairing or SHA2 patch.

Actual execution results, RISC0 3.0.5 + V1COMPAT 2.2.3, same input witness:

| Variant | Seven-update user cycles |
|---|---:|
| Stock | 1,747,839,425 |
| Official BLS Fp patch only | 417,787,748 |
| Official BLS + SHA2, combined pairing | 231,806,170 |

Verified results are equal. The optimized speedup versus stock is 7.54× in cycles, not a
comparison of wall times under different host loads. Bad state and bad signature witnesses
remain rejected in real executor runs. The one-update historical snapshot path takes
32,211,104 cycles including account/storage validation, exceeding LEZ's 10M public cap.

Reports: `executor-report.json` (first round), `optimized-executor-report.json`,
`optimized-comparison-report.json`, `incremental-executor-report.json`, and negative reports.
These are executor measurements, not receipts, proofs, LEZ gas or deployment evidence.
The actor workspace separately targets LEZ's kernel version 2.2.2.

Build guest with `cargo +risc0 build --release --locked --manifest-path
rust/accelerated/Cargo.toml --bin finality-witness-guest-accelerated --target
riscv32im-risc0-zkvm-elf`, project target flags, and `RISC0_FEATURE_bigint2=1` (normally injected
by risc0-build). Run using the project's IPC executor runner. Incremental guest bin is
`finality-witness-incremental-guest`. The source verification is newly written and unaudited;
using mature crypto forks does not transfer upstream audit coverage to this integration.
