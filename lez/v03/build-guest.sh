#!/usr/bin/env bash
set -euo pipefail
root=$(cd "$(dirname "$0")/../.." && pwd)
tools=${FW_TOOLCHAIN_ROOT:-$root/.tools}
target=${FW_V03_GUEST_TARGET_DIR:-$root/work/v03-guest-target}
export CARGO_HOME="$tools/cargo" RUSTUP_HOME="$tools/rustup" CARGO_TARGET_DIR="$target"
export PATH="$CARGO_HOME/bin:$PATH" CARGO_BUILD_JOBS="${CARGO_BUILD_JOBS:-2}"
export RISC0_FEATURE_bigint2=1 RISC0_SERVER_PATH="$tools/r0vm" RISC0_EXECUTOR=ipc
unset RISC0_DEV_MODE
compiler=${FW_CLANG:-$(command -v clang || command -v clang-19 || true)}
[[ -n $compiler ]] || { echo 'Set FW_CLANG to clang 19.1.7 or install clang.' >&2; exit 1; }
export CC_riscv32im_risc0_zkvm_elf="$compiler"
export CFLAGS_riscv32im_risc0_zkvm_elf='--target=riscv32-unknown-none-elf -march=rv32im -mabi=ilp32'
cd "$root"
cargo +1.98.1 test --release --locked --manifest-path rust/lez-v03/Cargo.toml --lib
cargo +1.98.1 build --release --locked --manifest-path rust/lez-v03/Cargo.toml --bin export-v03-frames
mkdir -p "$target/artifacts"
"$target/release/export-v03-frames" rust/compact-sequence.json "$target/artifacts/frames"
RUSTFLAGS='-C passes=lower-atomic -C link-arg=-Ttext=0x00200800 -C link-arg=--fatal-warnings -C panic=abort --cfg getrandom_backend="custom"' \
 cargo +risc0 build --release --locked --manifest-path rust/lez-v03/Cargo.toml --target riscv32im-risc0-zkvm-elf --bin finality-witness-lez-v03
python3 - "$root" "$target" <<'PYCODE'
from pathlib import Path
import os,struct,hashlib,sys
root,target=map(Path,sys.argv[1:]);reference=(root/'artifacts/lez-actor-compact.bin').read_bytes()
assert reference[:8]==b'R0BF\x01\0\0\0'
header=reference[:12+struct.unpack('<I',reference[8:12])[0]]
elf=(target/'riscv32im-risc0-zkvm-elf/release/finality-witness-lez-v03').read_bytes()
kernels=list(Path(os.environ['CARGO_HOME']).glob('registry/src/*/risc0-zkos-v1compat-2.2.2/elfs/v1compat.elf'))
assert len(kernels)==1;kernel=kernels[0].read_bytes()
assert hashlib.sha256(kernel).hexdigest()=='7ffd942e4e8babd771094f549ad080cfea8ecb2c05c986e275c011f41979d921'
out=target/'artifacts';(out/'finality-witness-v03.elf').write_bytes(elf)
(out/'finality-witness-v03.bin').write_bytes(header+struct.pack('<I',len(elf))+elf+kernel)
PYCODE
"$tools/r0vm" --elf "$target/artifacts/finality-witness-v03.bin" --id
echo "Built into $target/artifacts; compare the image ID before reusing evidence."
