#!/usr/bin/env bash
set -euo pipefail
root=$(cd "$(dirname "$0")/../.." && pwd)
sdk=${1:?pass a fresh LEZ v0.3.1 checkout}
target=${2:?pass a build directory outside this repository}
sdk=$(cd "$sdk" && pwd)
expected=1a003bfbfb89fcf987f62975cd08a401169e1b8c
actual=$(git -C "$sdk" rev-parse HEAD)
[[ $actual == "$expected" ]] || { echo "Expected LEZ $expected, got $actual" >&2; exit 1; }
case "$target/" in "$root/"*) echo 'Use a build directory outside the repository.' >&2; exit 1;; esac
mkdir -p "$sdk/fw_v03_driver/src"
cp "$root/lez/v03/driver/Cargo.toml" "$sdk/fw_v03_driver/Cargo.toml"
cp "$root/lez/v03/driver/src/main.rs" "$sdk/fw_v03_driver/src/main.rs"
python3 - "$sdk/Cargo.toml" <<'PYCODE'
from pathlib import Path
import sys
p=Path(sys.argv[1]);s=p.read_text()
if '"fw_v03_driver"' not in s:s=s.replace('members = [','members = [\n    "fw_v03_driver",',1)
p.write_text(s)
PYCODE
# Only proof segmentation is patched, not guest bytecode, verifier, fees or consensus.
if git -C "$sdk" apply --reverse --check "$root/lez/v03/prover-segments.patch" 2>/dev/null; then
  :
else
  git -C "$sdk" apply --check "$root/lez/v03/prover-segments.patch"
  git -C "$sdk" apply "$root/lez/v03/prover-segments.patch"
fi
cp "$root/lez/v03/driver-workspace.Cargo.lock" "$sdk/Cargo.lock"
export CARGO_TARGET_DIR="$target" CARGO_BUILD_JOBS="${CARGO_BUILD_JOBS:-2}"
export CARGO_PROFILE_DEV_DEBUG=0 CARGO_INCREMENTAL=0 RISC0_SKIP_BUILD_KERNELS=1
cd "$sdk"
cargo +1.98.1 build --locked -p finality-witness-v03-driver
echo "Driver: $target/debug/finality-witness-v03-driver"
