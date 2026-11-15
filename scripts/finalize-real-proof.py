#!/usr/bin/env python3
"""Finalize only already verified real-proof evidence, then atomically update ZIP."""
import argparse
import hashlib
import json
import os
import pathlib
import shutil
import subprocess
import sys
import zipfile

ROOT = pathlib.Path(os.environ.get("FW_PROOF_PROJECT_ROOT", pathlib.Path(__file__).resolve().parents[1]))
parser = argparse.ArgumentParser()
parser.add_argument("--run-dir", type=pathlib.Path, required=True)
args = parser.parse_args()
evidence = ROOT / "lez/evidence/compact-real"
result = json.loads((evidence / "sequence-result.json").read_text())
checks = json.loads((evidence / "offline-verification.json").read_text())
assert result["risc0_dev_mode"] is None and result["updates_applied"] == 1
assert result["replay"]["state_unchanged"] is True
assert all(check["verified"] and check["wrong_image_rejected"] and check["tampered_receipt_rejected"] for check in checks)
assert len(checks) == 1
step = result["steps"][1]
assert step["reverted"] is False and step["proof"]["kind"] == "cryptographic"
assert step["proof"]["independently_verified"] is True
snapshot = json.loads((args.run_dir / "launch-snapshot.json").read_text())
shutil.copyfile(args.run_dir / "launch-snapshot.json", evidence / "launch-snapshot.json")
receipt = evidence / checks[0]["receipt"]
assert receipt.exists()
actor = args.run_dir / "inputs/lez-actor-compact.bin"
assert hashlib.sha256(actor.read_bytes()).hexdigest() == snapshot["actor_sha256"]
assert hashlib.sha256((args.run_dir / "inputs/compact-sequence.json").read_bytes()).hexdigest() == snapshot["sequence_sha256"]
shutil.copyfile(actor, ROOT / "artifacts/lez-actor-real-proven.bin")
shutil.copyfile(args.run_dir / "inputs/compact-sequence.json", evidence / "proven-sequence.json")

hours = step["wall_seconds"] / 3600
image = step["proof"]["circuit_image_id"]
summary = {
    "status": "verified", "scope": "one real private committee update accepted by a local standalone LEZ node",
    "development_mode": False, "accepted_block": step["block"], "transaction": step["tx"],
    "updates_applied": 1, "committee_only_first_update": True,
    "actor_image_id": "35bbc235664c1a6adb0d41c93b2f49f2af7619371a615609a06a40aa327a1bf8",
    "circuit_image_id": image, "wallet_proving_to_inclusion_seconds": step["wall_seconds"],
    "proof_bytes": step["proof"]["bytes"], "receipt_file": str(receipt.relative_to(ROOT)),
    "receipt_sha256": hashlib.sha256(receipt.read_bytes()).hexdigest(),
    "correct_image_verified": True, "wrong_image_rejected": True, "tampered_journal_rejected": True,
    "limitations": "Only one update has a real proof. Seven-update runs use development receipts. Historical Capella, mock Bedrock, no public testnet or live follower."
}
(evidence / "summary.json").write_text(json.dumps(summary, indent=2) + "\n")
report = f"""# Real-proof result

One complete private committee update was accepted at local LEZ block **{step['block']}**.
`RISC0_DEV_MODE` was unset. The actual transaction receipt independently verifies
against official circuit image `{image}`. Wrong-image verification and journal
tampering are rejected. The receipt is `{checks[0]['receipt']}`.

Wallet proving through inclusion took **{step['wall_seconds']:.3f} seconds
({hours:.2f} hours)** on the recorded two-core CPU environment. This includes the
program proof, privacy-preserving circuit proof and block wait.

This first update installs the next sync committee. Its finalized slot remains
{result['after']['finalized_slot']}; `updates_applied` changes from 0 to 1. Seven
successive historical updates were separately tested using development receipts.
This does not claim seven real proofs, public-testnet deployment or live tracking.

Transaction: `{step['tx']}`. Detailed state and raw RPC evidence are in
`sequence-result.json` and `rpc-responses.json`.
"""
(evidence / "RESULT.md").write_text(report)

def unchanged(name):
    return hashlib.sha256((ROOT / name).read_bytes()).hexdigest() == snapshot["sha256"][name]

# Never overwrite edits made while this long-running job was proving.
if unchanged("README.md"):
    path = ROOT / "README.md"
    text = path.read_text()
    text = text.replace("另一个全新节点上的真实证明正在单独验证（使用委员会根与官方 blst 加速版）。",
                        "另一个全新节点已关闭开发模式，完成一条真实委员会更新证明并由节点接受。")
    text = text.replace("- 本地节点 receipt 为开发模式占位符，没有生成真实 zk proof；不是公开测试网或完整共识网络。",
                        "- 七次全链运行使用开发模式；另有一次真实证明通过。没有公开测试网或完整共识网络记录。")
    text += f"\n## 真实证明结果\n\n已完成一次完整私有更新的真实证明，并在本地节点区块 {step['block']} 被接受。独立 receipt 验证通过，错误 image 与篡改 journal 均被拒绝。整笔钱包证明至入块耗时 {hours:.2f} 小时（2 核 CPU）。该条只学习下一届委员会，最终性高度不变；七次连续更新记录仍使用开发模式。详见 [证明结果](lez/evidence/compact-real/RESULT.md)。\n"
    path.write_text(text)
if unchanged("lez/CONTINUOUS-UPDATES.md"):
    path = ROOT / "lez/CONTINUOUS-UPDATES.md"
    text = path.read_text().replace(
        "Proving and node acceptance are not established until that\nrun completes and its receipt is independently verified.",
        f"This run completed and its receipt independently verified; the real update settled at local block {step['block']}.")
    text += "\nCompleted real-proof evidence and explicit scope: [RESULT.md](evidence/compact-real/RESULT.md).\n"
    path.write_text(text)
if unchanged("APPLICATION-ADDENDUM.md"):
    (ROOT / "APPLICATION-ADDENDUM.md").write_text(f"""## Reproducible pre-application demonstration

Independent Python and Rust implementations verify a historical Capella slice from a fixed Ethereum mainnet checkpoint: bootstrap committee inclusion, seven BLS sync-committee updates, committee transitions, finality/execution branches, and the deposit-contract account and nonzero storage slot under the same finalized execution block, 17,923,026. Python additionally checks a transaction SSZ inclusion proof; inclusion is not a claim of successful execution.

The current LEZ v0.4.0-rc1 actor authenticates bootstrap against that checkpoint and advances an authenticated store commitment with same-program Apply and compare-and-swap. A compact profile commits committee vector roots and aggregates and verifies individual Merkle witnesses for exactly the non-signers before BLS verification. It uses pinned official RISC Zero acceleration, including blst. The first update measures 13,977,215 executor user cycles versus 37,044,569 for the generic full-store actor. These are equivalent Ethereum verification workloads with different authenticated encodings, not proving-time or transaction-gas measurements.

All seven successive compact updates settled on one actual local node using development receipts, including five current-committee rotations, and reach slot 7,109,344. A separate fresh local node and wallet ran with RISC0_DEV_MODE unset. One complete real private Verify -> public Apply was accepted at block {step['block']}; its receipt independently verifies against the official privacy-preserving circuit image, while wrong-image and journal-tamper checks fail. This first update installs the next committee without advancing finalized height. Whole wallet proving through inclusion took {step['wall_seconds']:.3f} seconds on the recorded two-core CPU environment. Only this one update has a real proof; the seven-update run uses development receipts. Evidence: lez/evidence/compact-real/.

There are 39 passing Rust tests in the compact profile, 25 passing default-core tests, and actual-guest rejection evidence for forged non-signer membership and a valid-curve signature on the wrong message. A native consumer separately calls the full verifier and atomically grants one demonstration ticket with a nullifier. It is not an asset-handling or deployed consumer application.

Pinned dependencies, licences, input provenance, source fingerprints, ELF/image identities, raw RPC evidence and reproduction scripts are included. The integration is new and unaudited. Historical Capella only; no current-fork coverage, live follower, public-testnet deployment, consensus-network claim, full-chain real proving or independent audit. Standalone baseline guests use V1COMPAT 2.2.3; the LEZ actor uses the protocol-required 2.2.2. The old checkpoint must not bootstrap a live client today.
""")

subprocess.run([sys.executable, str(pathlib.Path(__file__).with_name("render-benchmarks.py"))], check=True)
manifest_path = ROOT / "artifacts/manifest.json"
manifest = json.loads(manifest_path.read_text())
manifest.setdefault("variants", {})["real_proven_actor"] = dict(
    image_id=summary["actor_image_id"], program_binary="artifacts/lez-actor-real-proven.bin",
    kernel="risc0-zkos-v1compat 2.2.2", mode="one real private update accepted; receipt independently verified",
    proof_circuit_image_id=image, evidence="lez/evidence/compact-real/summary.json")
for path in ROOT.rglob("*"):
    if path.is_file() and (str(path.relative_to(ROOT)).startswith(("artifacts/", "lez/evidence/compact-real/"))):
        if path.name != "manifest.json":
            manifest.setdefault("sha256", {})[str(path.relative_to(ROOT))] = hashlib.sha256(path.read_bytes()).hexdigest()
manifest_path.write_text(json.dumps(manifest, indent=2) + "\n")

excluded = {".venv", ".tools", "target", "__pycache__", ".git", "work"}
files = sorted(path for path in ROOT.rglob("*") if path.is_file()
               and not any(part in excluded for part in path.relative_to(ROOT).parts)
               and path.name not in ("CHECKSUMS.sha256", ".background-proof.lock"))
assert not any(path.name in {"wallet_storage.json", "channel_signing_key", "node.pid", "node.log"} for path in files)
(ROOT / "CHECKSUMS.sha256").write_text("".join(hashlib.sha256(path.read_bytes()).hexdigest() + "  "
    + str(path.relative_to(ROOT)) + "\n" for path in files))
files.append(ROOT / "CHECKSUMS.sha256")
temporary = ROOT.parent / ".finality-witness-demo.zip.building"
with zipfile.ZipFile(temporary, "w", zipfile.ZIP_DEFLATED) as archive:
    for path in files:
        archive.write(path, str(pathlib.Path(ROOT.name) / path.relative_to(ROOT)))
with zipfile.ZipFile(temporary) as archive:
    assert archive.testzip() is None
temporary.replace(ROOT.parent / "finality-witness-demo.zip")
print(json.dumps(dict(status="complete", accepted_block=step["block"],
    wall_seconds=step["wall_seconds"], receipt_verified=True, archive_files=len(files))))
