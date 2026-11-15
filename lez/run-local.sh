#!/usr/bin/env bash
set -euo pipefail
# Requires the official LEZ v0.4.0-rc1 source and its native build prerequisites.
# Uses only a fresh local standalone chain and development-mode receipts.
demo_root=$(cd "$(dirname "$0")/.." && pwd)
lez_source=${1:?Usage: run-local.sh LEZ_SOURCE FRESH_RUN_DIR}
run_dir=${2:?Usage: run-local.sh LEZ_SOURCE FRESH_RUN_DIR}
lez_source=$(realpath "$lez_source")
run_dir=$(realpath -m "$run_dir")
[[ ! -e "$run_dir" ]] || { echo 'Use a fresh run directory' >&2; exit 1; }
[[ $(git -C "$lez_source" rev-parse HEAD) == 1806e873ea2ac0e1b608aa7cf2f9b6e1fb12db21 ]] || {
  echo 'Expected official LEZ v0.4.0-rc1 commit 1806e873' >&2; exit 1;
}
[[ -n ${RISC0_SERVER_PATH:-} ]] || { echo 'Set RISC0_SERVER_PATH to r0vm 3.0.5' >&2; exit 1; }
mkdir -p "$lez_source/fw_demo_driver"
cp -R "$demo_root/lez/driver/." "$lez_source/fw_demo_driver/"
python3 - "$lez_source/Cargo.toml" <<'PY'
import pathlib,sys
p=pathlib.Path(sys.argv[1]); text=p.read_text()
if '"fw_demo_driver"' not in text:
    text=text.replace('members = [', 'members = [\n    "fw_demo_driver",', 1)
p.write_text(text)
PY
cp "$demo_root/lez/driver-workspace.Cargo.lock" "$lez_source/Cargo.lock"
(cd "$lez_source" && cargo build --release --locked -p finality-witness-lez-driver -p sequencer_service --features sequencer_service/standalone)
driver="$lez_source/target/release/finality-witness-lez-driver"
node="$lez_source/target/release/sequencer_service"
export RISC0_DEV_MODE=1 RISC0_EXECUTOR=ipc RISC0_FEATURE_bigint2=1 SUPPRESS_VERBOSE_PRINTS=1
"$driver" prepare "$run_dir" "$lez_source" > "$run_dir-prepare.log"
# This funding public key is only used by the standalone mock Bedrock.
"$node" setup "$run_dir/node" --config "$run_dir/local-demo.json" --create-channel \
  --funding-public-key 2e03b2eff5a45478e7e79668d2a146cf2c5c7925bce927f2b1c67f2ab4fc0d26
"$node" start "$run_dir/node" --rpc-listen-address 127.0.0.1:3040 > "$run_dir/node.log" 2>&1 &
node_pid=$!
trap 'kill -INT "$node_pid" 2>/dev/null || true' EXIT
ready=0
for ((attempt=0;attempt<120;attempt++)); do
  if curl -fsS -H 'Content-Type: application/json' --data '{"jsonrpc":"2.0","id":1,"method":"getLastBlockId","params":[]}' http://127.0.0.1:3040 | python3 -c 'import json,sys; assert "result" in json.load(sys.stdin)' 2>/dev/null; then
    ready=1; break
  fi
  sleep 1
done
[[ $ready == 1 ]] || { echo 'Node startup timed out' >&2; exit 1; }
"$driver" run "$run_dir" "$demo_root/artifacts/lez-actor.bin" "$demo_root/rust/incremental-input.json" > "$run_dir/driver.log" 2>&1
cat "$run_dir/result.json"
# Keep wallet_storage.json and node keys in run_dir; do not distribute them.
