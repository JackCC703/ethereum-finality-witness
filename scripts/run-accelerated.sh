#!/usr/bin/env bash
set -euo pipefail
PROJECT_DIR="$(cd "$(dirname "$0")/.." && pwd)"
TASK_TOOLS="${FW_TOOLCHAIN_ROOT:-$PROJECT_DIR/.tools}"
TASK_HOST="${FW_HOST_TOOLCHAIN:-1.91.1}"
TASK_TARGET="${FW_ACCELERATED_TARGET_DIR:-$PROJECT_DIR/rust/accelerated/target}"
TASK_RUNNER_TARGET="${FW_RUNNER_TARGET_DIR:-$PROJECT_DIR/rust/runner/target}"
export CARGO_HOME="$TASK_TOOLS/cargo" RUSTUP_HOME="$TASK_TOOLS/rustup"
export PATH="$CARGO_HOME/bin:$PATH" RISC0_SERVER_PATH="$TASK_TOOLS/r0vm" RISC0_EXECUTOR=ipc
export RISC0_FEATURE_bigint2=1
unset RISC0_DEV_MODE
cd "$PROJECT_DIR"
python3 verify_fixtures.py
python3 rust/make_input.py
python3 scripts/make-negative-inputs.py
CARGO_TARGET_DIR="$TASK_TARGET" cargo +"$TASK_HOST" test --release --locked --manifest-path rust/accelerated/Cargo.toml -p finality-witness-core
mkdir -p work
cargo +"$TASK_HOST" run --release --locked --manifest-path rust/Cargo.toml -p finality-witness-native --bin export_snapshot -- rust/input.json > work/snapshot-rebuilt.json
python3 - <<'PY'
from pathlib import Path
import json
actual=json.loads(Path('work/snapshot-rebuilt.json').read_text())
assert actual == json.loads(Path('rust/historical-store.json').read_text()), 'Rebuilt snapshot differs'
full=json.loads(Path('rust/input.json').read_text())
Path('rust/incremental-input.json').write_text(json.dumps({'store':actual,'update':full['finality'],'state_proof':full['state_proof']},separators=(',',':'))+'\n')
PY
RISC0_FEATURE_bigint2=1 CARGO_TARGET_DIR="$TASK_TARGET" RUSTFLAGS='-C passes=lower-atomic -C link-arg=-Ttext=0x00200800 -C link-arg=--fatal-warnings -C panic=abort --cfg getrandom_backend="custom"' \
 cargo +risc0 build --release --locked --manifest-path rust/accelerated/Cargo.toml --bin finality-witness-guest-accelerated --bin finality-witness-incremental-guest --target riscv32im-risc0-zkvm-elf
CARGO_TARGET_DIR="$TASK_RUNNER_TARGET" cargo +"$TASK_HOST" build --release --locked --manifest-path rust/runner/Cargo.toml
TASK_RUNNER="$TASK_RUNNER_TARGET/release/finality-witness-risc0-runner"
TASK_ELF="$TASK_TARGET/riscv32im-risc0-zkvm-elf/release/finality-witness-guest-accelerated"
"$TASK_RUNNER" "$TASK_ELF" rust/input.json rust/accelerated/optimized-executor-report.json
for TASK_CASE in bad-state bad-signature; do
 "$TASK_RUNNER" "$TASK_ELF" "rust/$TASK_CASE-input.json" "rust/accelerated/$TASK_CASE-executor-report.json"
done
TASK_INCREMENTAL="$TASK_TARGET/riscv32im-risc0-zkvm-elf/release/finality-witness-incremental-guest"
"$TASK_RUNNER" "$TASK_INCREMENTAL" rust/incremental-input.json rust/accelerated/incremental-executor-report.json
cp "$TASK_ELF" artifacts/verifier-accelerated.elf
cp "$TASK_ELF.bin" artifacts/verifier-accelerated.bin
cp "$TASK_INCREMENTAL" artifacts/verifier-incremental.elf
cp "$TASK_INCREMENTAL.bin" artifacts/verifier-incremental.bin
python3 scripts/check-accelerated-results.py
python3 scripts/render-benchmarks.py
