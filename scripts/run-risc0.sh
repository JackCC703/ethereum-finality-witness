#!/usr/bin/env bash
set -euo pipefail
PROJECT_DIR="$(cd "$(dirname "$0")/.." && pwd)"
TASK_TOOLS="${FW_TOOLCHAIN_ROOT:-$PROJECT_DIR/.tools}"
TASK_HOST="${FW_HOST_TOOLCHAIN:-1.91.1}"
TASK_GUEST_TARGET="${FW_GUEST_TARGET_DIR:-$PROJECT_DIR/rust/target}"
TASK_RUNNER_TARGET="${FW_RUNNER_TARGET_DIR:-$PROJECT_DIR/rust/runner/target}"
export CARGO_HOME="$TASK_TOOLS/cargo" RUSTUP_HOME="$TASK_TOOLS/rustup"
export PATH="$CARGO_HOME/bin:$PATH"
export RISC0_SERVER_PATH="$TASK_TOOLS/r0vm" RISC0_EXECUTOR=ipc
unset RISC0_DEV_MODE
cd "$PROJECT_DIR"
python3 verify_fixtures.py
python3 rust/make_input.py
python3 scripts/make-negative-inputs.py
cargo +"$TASK_HOST" test --release --locked --manifest-path rust/Cargo.toml -p finality-witness-core
cargo +"$TASK_HOST" run --release --locked --manifest-path rust/Cargo.toml -p finality-witness-native -- rust/input.json > rust/native-report.json
cargo +"$TASK_HOST" run --release --locked --manifest-path rust/Cargo.toml -p finality-witness-native -- --consumer-demo rust/input.json > rust/consumer-native-report.json
CARGO_TARGET_DIR="$TASK_GUEST_TARGET" RUSTFLAGS='-C passes=lower-atomic -C link-arg=-Ttext=0x00200800 -C link-arg=--fatal-warnings -C panic=abort --cfg getrandom_backend="custom"' \
  cargo +risc0 build --release --locked --manifest-path rust/Cargo.toml -p finality-witness-guest --target riscv32im-risc0-zkvm-elf
CARGO_TARGET_DIR="$TASK_RUNNER_TARGET" cargo +"$TASK_HOST" build --release --locked --manifest-path rust/runner/Cargo.toml
"$TASK_RUNNER_TARGET/release/finality-witness-risc0-runner" \
  "$TASK_GUEST_TARGET/riscv32im-risc0-zkvm-elf/release/finality-witness-guest" \
  rust/input.json rust/executor-report.json
for TASK_CASE in bad-state bad-signature; do
  "$TASK_RUNNER_TARGET/release/finality-witness-risc0-runner" \
    "$TASK_GUEST_TARGET/riscv32im-risc0-zkvm-elf/release/finality-witness-guest" \
    "rust/$TASK_CASE-input.json" "rust/$TASK_CASE-executor-report.json"
done
cp "$TASK_GUEST_TARGET/riscv32im-risc0-zkvm-elf/release/finality-witness-guest" artifacts/verifier.elf
cp "$TASK_GUEST_TARGET/riscv32im-risc0-zkvm-elf/release/finality-witness-guest.bin" artifacts/verifier.bin
python3 scripts/check-results.py
python3 scripts/render-benchmarks.py
