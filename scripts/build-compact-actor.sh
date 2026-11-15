#!/usr/bin/env bash
set -euo pipefail
root=$(cd "$(dirname "$0")/.." && pwd)
task_tools=${FW_TOOLCHAIN_ROOT:-$root/.tools}
task_target=${FW_COMPACT_TARGET_DIR:-$root/work/compact-target}
task_host=${FW_HOST_TOOLCHAIN:-1.98.1}
export CARGO_HOME="$task_tools/cargo" RUSTUP_HOME="$task_tools/rustup"
export PATH="$CARGO_HOME/bin:$PATH" RISC0_SERVER_PATH="$task_tools/r0vm" RISC0_EXECUTOR=ipc
export RISC0_FEATURE_bigint2=1
unset RISC0_DEV_MODE
compiler=${FW_CLANG:-}
if [[ -z $compiler ]]; then compiler=$(command -v clang || command -v clang-19 || true); fi
[[ -n $compiler ]] || { echo 'Install clang (reference: 19.1.7), or set FW_CLANG.' >&2; exit 1; }
export CC_riscv32im_risc0_zkvm_elf="$compiler"
export CFLAGS_riscv32im_risc0_zkvm_elf='--target=riscv32-unknown-none-elf -march=rv32im -mabi=ilp32'
export CARGO_TARGET_DIR="$task_target"
cd "$root"
python3 verify_fixtures.py
cargo +"$task_host" test --release --locked --manifest-path rust/lez-actor-fast/Cargo.toml -p finality-witness-core -p finality-witness-lez-actor-fast --lib --test security
cargo +"$task_host" build --release --locked --manifest-path rust/lez-actor-fast/Cargo.toml --bin export-compact-sequence --bin measure-actor
mkdir -p "$task_target/artifacts"
"$task_target/release/export-compact-sequence" rust/input.json "$task_target/artifacts/compact-sequence.json"
RUSTFLAGS='-C passes=lower-atomic -C link-arg=-Ttext=0x00200800 -C link-arg=--fatal-warnings -C panic=abort --cfg getrandom_backend="custom"' \
  cargo +risc0 build --release --locked --manifest-path rust/lez-actor-fast/Cargo.toml --bin finality-witness-lez-actor-fast --target riscv32im-risc0-zkvm-elf
python3 - "$root" "$task_target" <<'PY'
from pathlib import Path
import math,struct,sys,hashlib,json
root,target=map(Path,sys.argv[1:])
elf=(target/'riscv32im-risc0-zkvm-elf/release/finality-witness-lez-actor-fast').read_bytes()
reference=(root/'artifacts/lez-actor.bin').read_bytes()
assert reference[:8]==b'R0BF\x01\0\0\0'
length=struct.unpack('<I',reference[8:12])[0]
header=reference[:12+length]
import os
kernels=list(Path(os.environ['CARGO_HOME']).glob('registry/src/*/risc0-zkos-v1compat-2.2.2/elfs/v1compat.elf'))
assert len(kernels)==1,kernels
kernel=kernels[0].read_bytes()
assert hashlib.sha256(kernel).hexdigest()=='7ffd942e4e8babd771094f549ad080cfea8ecb2c05c986e275c011f41979d921'
# The official example helper counts wrapped bytes; wallet deployment counts user ELF.
# Add non-loaded tail bytes only when needed to make those chunk counts agree.
chunk=96*1024
required=math.ceil((len(header)+4+len(elf)+len(kernel))/chunk)
padding=max(0,(required-1)*chunk+1-len(elf))
elf+=bytes(padding)
binary=header+struct.pack('<I',len(elf))+elf+kernel
assert math.ceil(len(elf)/chunk)==math.ceil(len(binary)/chunk)
out=target/'artifacts'
(out/'lez-actor-compact.elf').write_bytes(elf)
(out/'lez-actor-compact.bin').write_bytes(binary)
(out/'build-metadata.json').write_text(json.dumps({'non_loaded_elf_tail_padding':padding,'sha256':hashlib.sha256(binary).hexdigest()},indent=2)+'\n')
PY
"$task_tools/r0vm" --elf "$task_target/artifacts/lez-actor-compact.bin" --id > "$task_target/artifacts/image-id.txt"
echo "Build artifacts: $task_target/artifacts"
echo 'Reference proof artifacts in the deliverable have not been overwritten.'
