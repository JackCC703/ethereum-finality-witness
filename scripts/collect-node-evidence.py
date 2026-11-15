#!/usr/bin/env python3
"""Collect public RPC evidence and verify completed real receipts; exclude wallet/node keys."""
import argparse
import json
import os
import pathlib
import shutil
import subprocess
import urllib.request

parser = argparse.ArgumentParser()
parser.add_argument("--run", type=pathlib.Path, required=True)
parser.add_argument("--out", type=pathlib.Path, required=True)
parser.add_argument("--rpc", default="http://127.0.0.1:3040")
parser.add_argument("--driver", required=True)
parser.add_argument("--rpc-evidence", type=pathlib.Path,
                    help="Resume offline checks using RPC responses captured before node shutdown")
args = parser.parse_args()
result = json.loads((args.run / "sequence-result.json").read_text())
assert result["replay"]["state_unchanged"] is True
args.out.mkdir(parents=True, exist_ok=True)
for name in ("sequence-result.json", "accounts.json", "local-demo.json", "driver.log"):
    shutil.copyfile(args.run / name, args.out / name)
for name in ("local-demo-initial.json", "runtime-config-change.json", "runtime-mode.json"):
    if (args.run / name).exists():
        shutil.copyfile(args.run / name, args.out / name)
responses = []
saved = json.loads(args.rpc_evidence.read_text()) if args.rpc_evidence else None
for step in result["steps"]:
    for method, params in (("getTransaction", [step["tx"]]), ("getBlock", [step["block"]])):
        request = dict(jsonrpc="2.0", id=len(responses) + 1, method=method, params=params)
        if saved is not None:
            record = saved[len(responses)]
            assert record["request"] == request, "saved RPC request mismatch"
            response = record["response"]
        else:
            response = json.load(urllib.request.urlopen(urllib.request.Request(
                args.rpc, data=json.dumps(request).encode(), headers={"Content-Type": "application/json"})))
        assert "error" not in response, response
        if method == "getTransaction":
            assert response["result"] == [step["transaction"], step["block"]]
        responses.append(dict(request=request, response=response))
if saved is not None:
    assert len(saved) == len(responses), "unexpected saved RPC responses"
(args.out / "rpc-responses.json").write_text(json.dumps(responses, indent=2) + "\n")

if result["risc0_dev_mode"] is None:
    env = os.environ.copy()
    env.pop("RISC0_DEV_MODE", None)
    outcomes = []
    for step in result["steps"]:
        if "proof" not in step:
            continue
        assert step["proof"]["kind"] == "cryptographic"
        assert step["proof"]["independently_verified"] is True
        receipt_name = step["label"] + "-receipt.bincode"
        receipt = args.out / receipt_name
        shutil.copyfile(args.run / "proof-evidence" / receipt_name, receipt)
        image = step["proof"]["circuit_image_id"]
        command = [args.driver, "verify-receipt", str(receipt), image]
        good = subprocess.run(command, env=env, capture_output=True, text=True)
        assert good.returncode == 0, good.stderr
        wrong_image = subprocess.run(command[:-1] + ["00" * 32], env=env, capture_output=True, text=True)
        assert wrong_image.returncode != 0, "receipt accepted for wrong circuit image"
        corrupted = args.run / (receipt_name + ".tampered-test")
        content = bytearray(receipt.read_bytes())
        journal_name = step["label"] + "-journal.bin"
        journal = (args.run / "proof-evidence" / journal_name).read_bytes()
        # RISC0 3.0.5 serializes the top-level journal immediately before the
        # 32-byte ReceiptMetadata digest. The same bytes may also occur in an
        # opened inner claim, so uniqueness is not a valid format assumption.
        assert len(journal) > 32
        journal_offset = len(content) - 32 - len(journal)
        assert journal_offset >= 8
        assert content[journal_offset:journal_offset + len(journal)] == journal
        assert content[journal_offset - 8:journal_offset] == len(journal).to_bytes(8, "little")
        content[journal_offset + len(journal) // 2] ^= 1
        shutil.copyfile(args.run / "proof-evidence" / journal_name, args.out / journal_name)
        corrupted.write_bytes(content)
        try:
            bad = subprocess.run([args.driver, "verify-receipt", str(corrupted), image], env=env, capture_output=True, text=True)
            assert bad.returncode != 0, "tampered receipt accepted"
            assert "claim digest does not match" in bad.stderr.lower(), bad.stderr
        finally:
            corrupted.unlink()
        outcomes.append(dict(receipt=receipt_name, circuit_image_id=image, verified=True,
                             wrong_image_rejected=True, tampered_receipt_rejected=True,
                             journal_byte_offset=journal_offset + len(journal) // 2,
                             correct_image_stdout=good.stdout.strip(),
                             wrong_image_error=wrong_image.stderr.strip(),
                             tampered_receipt_error=bad.stderr.strip()))
    assert outcomes, "no real receipt found"
    (args.out / "offline-verification.json").write_text(json.dumps(outcomes, indent=2) + "\n")
print(f"Collected {len(result['steps'])} settled transactions and their independent RPC evidence.")
