# LEZ v0.4 Verify → Apply actor

This is a historical, single-update placement demonstration, **not a live Ethereum follower**.
The initial store commitment is compiled into the verifier. Rebuild its complete witness by
verifying the original historical checkpoint and six updates with `export_snapshot`;
`rust/historical-store.json` contains the result, and `rust/incremental-input.json` supplies
it plus the seventh/finality update and the matching account/storage proof.

`PINNED_SNAPSHOT_SHA256` is `76084610a9e4dfe1aea101c87bf9734b3d7a5a423d0d63f222c2b75ecfdc3aac`.
Its domain is `finality-witness/verified-store/v1\0`, followed by deterministic serde_json
serialization. This imported snapshot is an explicitly pinned historical trust input,
not a prior zk receipt. Callers cannot replace its roots or committee keys.

## Wire protocol

The actor entrypoint is the real `lee_core::actor_entrypoint!` API at LEZ
`1806e873ea2ac0e1b608aa7cf2f9b6e1fb12db21` (`v0.4.0-rc1`). Message data is **Borsh
serialization of a Vec<u8> whose bytes are JSON**, not the standalone benchmark's length-JSON
input framing.

```json
{"op":"initialize"}
```

Creates a compact public instance with fixed seed commitment, finalized slot 7104096,
execution block 17917816 and update count zero. Reinitialization fails. No caller-provided
checkpoint, chain/root, or store is used.

```json
{"op":"verify","public_instance":"BASE58_ACCOUNT_ID","witness":{}}
```

Replace `witness` with the contents of `rust/incremental-input.json`. Run this actor privately:
it validates the pinned store, Ethereum finality update, and optional account/storage proof,
then sends a synchronous `apply` to the target account under its own program ID. It preserves
its own state. The compiled program constructs the entire apply message; callers cannot
supply a purportedly verified result through this branch.

The internal message is `{"op":"apply","verified": VERIFIER_SUMMARY}`. Apply requires
`from_own_program()`, checks the old store commitment by compare-and-swap, and requires a
strictly increasing finalized slot. Only then does it replace the public summary and increment
the update count. It stores no full committee arrays or 100KB witness. Direct, other-program,
and replayed apply messages fail.

## Build and evidence boundaries

Separate Cargo workspace/lock; stock verifier workspace remains unpatched. Official RISC0
BLS/SHA2 patches are fixed by commit in Cargo.toml. Build the actor ELF with the RISC0 Rust
compiler, target `riscv32im-risc0-zkvm-elf`, and `RISC0_FEATURE_bigint2=1`. The project's existing
RISC0 target flags are required. Package it with **risc0-zkos-v1compat 2.2.2** for this LEZ
version, not the 2.2.3 kernel used in the standalone comparison runner.

`native-tests.txt` records seven successful handler tests. Native handler tests alone are
neither a sequencer run nor a cryptographic proof. Node/wallet RPC deployment and transaction
evidence must be collected separately. If the node/wallet uses `RISC0_DEV_MODE=1`, explicitly
state that it validates execution/placement with placeholder receipts, not real zk proofs.
The measured incremental verification exceeds the public 10M cycle cap, so it belongs in the
private Verify action; this project does not raise the public cap to make it fit.
