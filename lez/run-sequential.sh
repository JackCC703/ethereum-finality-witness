#!/usr/bin/env bash
set -euo pipefail
# Usage: run-sequential.sh LEZ_SOURCE FRESH_RUN_DIR dev|real [UPDATE_COUNT]
demo_root=$(cd "$(dirname "$0")/.." && pwd)
lez_source=$(realpath "${1:?LEZ source}")
run_dir=$(realpath -m "${2:?Fresh run directory}")
mode=${3:?dev or real}
actor_bin=${FW_ACTOR_BIN:-$demo_root/artifacts/lez-actor-compact.bin}
sequence_json=${FW_SEQUENCE_JSON:-$demo_root/rust/compact-sequence.json}
case "$mode" in dev) count=${4:-7};; real) count=${4:-1};; *) echo 'Expected dev or real' >&2; exit 1;; esac
[[ $count =~ ^[1-7]$ ]] || { echo 'Update count must be 1 through 7' >&2; exit 1; }
[[ ! -e "$run_dir" ]] || { echo 'Use a fresh run directory' >&2; exit 1; }
[[ $(git -C "$lez_source" rev-parse HEAD) == 1806e873ea2ac0e1b608aa7cf2f9b6e1fb12db21 ]] || {
  echo 'Expected official LEZ v0.4.0-rc1 commit 1806e873' >&2; exit 1;
}
[[ -n ${RISC0_SERVER_PATH:-} ]] || { echo 'Set RISC0_SERVER_PATH to r0vm 3.0.5' >&2; exit 1; }
mkdir -p "$lez_source/fw_demo_driver"
cp -R "$demo_root/lez/driver/." "$lez_source/fw_demo_driver/"
python3 - "$lez_source/Cargo.toml" <<'PY'
import pathlib,sys
p=pathlib.Path(sys.argv[1]);s=p.read_text()
if '"fw_demo_driver"' not in s:s=s.replace('members = [','members = [\n    "fw_demo_driver",',1)
p.write_text(s)
PY
cp "$demo_root/lez/driver-workspace.Cargo.lock" "$lez_source/Cargo.lock"
if git -C "$lez_source" apply --check "$demo_root/lez/prover-segments.patch" 2>/dev/null; then
  git -C "$lez_source" apply "$demo_root/lez/prover-segments.patch"
else
  git -C "$lez_source" apply --reverse --check "$demo_root/lez/prover-segments.patch"
fi
(cd "$lez_source" && cargo build --release --locked -p finality-witness-lez-driver -p sequencer_service --features sequencer_service/standalone)
driver="$lez_source/target/release/finality-witness-lez-driver"
node="$lez_source/target/release/sequencer_service"
export RISC0_EXECUTOR=ipc RISC0_PROVER=ipc RISC0_FEATURE_bigint2=1 SUPPRESS_VERBOSE_PRINTS=1
export RAYON_NUM_THREADS=${RAYON_NUM_THREADS:-2}
if [[ $mode == dev ]]; then export RISC0_DEV_MODE=1; else unset RISC0_DEV_MODE; fi
"$driver" prepare "$run_dir" "$lez_source" > "$run_dir-prepare.log"
if [[ $mode == real ]]; then
  python3 - "$run_dir/local-demo.json" <<'PY'
import pathlib,json,sys
p=pathlib.Path(sys.argv[1]);d=json.loads(p.read_text());d['block_create_timeout']='24s';p.write_text(json.dumps(d,indent=2)+'\n')
PY
fi
"$node" setup "$run_dir/node" --config "$run_dir/local-demo.json" --create-channel \
  --funding-public-key 2e03b2eff5a45478e7e79668d2a146cf2c5c7925bce927f2b1c67f2ab4fc0d26
"$node" start "$run_dir/node" --rpc-listen-address 127.0.0.1:3040 > "$run_dir/node.log" 2>&1 &
node_pid=$!
trap 'kill -INT "$node_pid" 2>/dev/null || true' EXIT
ready=0
for ((attempt=0;attempt<120;attempt++)); do
  if curl -fsS -H 'Content-Type: application/json' --data '{"jsonrpc":"2.0","id":1,"method":"getLastBlockId","params":[]}' http://127.0.0.1:3040 | python3 -c 'import json,sys; assert "result" in json.load(sys.stdin)' 2>/dev/null; then ready=1;break;fi
  sleep 1
done
[[ $ready == 1 ]] || { echo 'Node startup timed out' >&2; exit 1; }
export FW_PROOF_EVIDENCE_DIR="$run_dir/proof-evidence"
"$driver" run-sequence "$run_dir" "$actor_bin" "$sequence_json" "$count" > "$run_dir/driver.log" 2>&1
python3 "$demo_root/scripts/check-sequence.py" "$run_dir/sequence-result.json" "$sequence_json"
if [[ $mode == real ]]; then
  image_id=$(python3 - "$run_dir/sequence-result.json" <<'PY'
import json,sys
print(json.load(open(sys.argv[1]))['steps'][1]['proof']['circuit_image_id'])
PY
)
  for receipt in "$run_dir"/proof-evidence/*-receipt.bincode; do
    "$driver" verify-receipt "$receipt" "$image_id"
  done
fi
python3 "$demo_root/scripts/collect-node-evidence.py" --run "$run_dir" --out "$run_dir/public-evidence" --rpc http://127.0.0.1:3040 --driver "$driver"
echo "Evidence retained in $run_dir (wallet/node files are private local runtime files)."
