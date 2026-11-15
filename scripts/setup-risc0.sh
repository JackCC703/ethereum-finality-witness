#!/usr/bin/env bash
set -euo pipefail
PROJECT_DIR="$(cd "$(dirname "$0")/.." && pwd)"
TASK_TOOLS="${FW_TOOLCHAIN_ROOT:-$PROJECT_DIR/.tools}"
TASK_HOST="${FW_HOST_TOOLCHAIN:-1.91.1}"
if [[ "$(uname -s)-$(uname -m)" != Linux-x86_64 ]]; then
  echo 'This pinned installer supports Linux x86_64. Use official RISC Zero tooling on other systems.' >&2
  exit 1
fi
mkdir -p "$TASK_TOOLS"
export CARGO_HOME="$TASK_TOOLS/cargo" RUSTUP_HOME="$TASK_TOOLS/rustup"
export PATH="$CARGO_HOME/bin:$PATH"
if [[ ! -x "$CARGO_HOME/bin/rustup" ]]; then
  curl -fL https://static.rust-lang.org/rustup/dist/x86_64-unknown-linux-gnu/rustup-init -o "$TASK_TOOLS/rustup-init"
  chmod +x "$TASK_TOOLS/rustup-init"
  "$TASK_TOOLS/rustup-init" -y --no-modify-path --profile minimal --default-toolchain "$TASK_HOST"
fi
rustup toolchain install "$TASK_HOST" --profile minimal
if [[ ! -f "$TASK_TOOLS/risc0-tools.tgz" ]]; then
  curl -fL https://github.com/risc0/risc0/releases/download/v3.0.5/cargo-risczero-x86_64-unknown-linux-gnu.tgz -o "$TASK_TOOLS/risc0-tools.tgz"
fi
printf '%s  %s\n' 936ef988b78f20e3bd9f80e375f3adc934b13addc6ae2680f2e5fc0bcc966158 "$TASK_TOOLS/risc0-tools.tgz" | sha256sum -c -
tar -xzf "$TASK_TOOLS/risc0-tools.tgz" -C "$TASK_TOOLS"
if [[ ! -f "$TASK_TOOLS/risc0-rust.tar.gz" ]]; then
  curl -fL https://github.com/risc0/rust/releases/download/r0.1.91.1/rust-toolchain-x86_64-unknown-linux-gnu.tar.gz -o "$TASK_TOOLS/risc0-rust.tar.gz"
fi
printf '%s  %s\n' e082a1dc44abdef1d95460295a70218eb294ab999b834570ec932d05641cce5d "$TASK_TOOLS/risc0-rust.tar.gz" | sha256sum -c -
mkdir -p "$TASK_TOOLS/risc0-rust"
tar -xzf "$TASK_TOOLS/risc0-rust.tar.gz" -C "$TASK_TOOLS/risc0-rust"
rustup toolchain link risc0 "$TASK_TOOLS/risc0-rust"
echo "Installed. Run with FW_TOOLCHAIN_ROOT=$TASK_TOOLS ./scripts/run-risc0.sh"
