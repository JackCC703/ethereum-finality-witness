#!/usr/bin/env python3
"""Cross-check recorded public states with the native-verified witness sequence."""
import hashlib
import json
import pathlib
import sys

root = pathlib.Path(__file__).resolve().parents[1]
sequence_path = pathlib.Path(sys.argv[2]) if len(sys.argv) > 2 else root / "rust/sequence.json"
sequence = json.loads(sequence_path.read_text())
result = json.loads(pathlib.Path(sys.argv[1]).read_text())
count = result["updates_applied"]
assert 1 <= count <= len(sequence["steps"])
assert len(result["states"]) == count + 1
assert result["states"][0] == result["initial"]
assert result["states"][-1] == result["after"]
for index, step in enumerate(sequence["steps"][:count]):
    before, after = result["states"][index:index + 2]
    witness, expected = step["witness"], step["verified"]
    encoded = json.dumps(witness["store"], sort_keys=True, separators=(",", ":")).encode()
    commitment = hashlib.sha256(b"finality-witness/verified-store/v1\0" + encoded).hexdigest()
    assert commitment == before["store_commitment"] == expected["previous_store_commitment"]
    assert after["store_commitment"] == expected["new_store_commitment"]
    assert after["finalized_slot"] == expected["finalized_slot"]
    assert after["execution_block"] == expected["execution_block"]
    assert after["updates_applied"] == index + 1
    transaction = result["steps"][index + 1]
    assert transaction["reverted"] is False
    assert transaction["block"] > result["steps"][index]["block"]
    if result["risc0_dev_mode"] is None:
        assert transaction["proof"]["kind"] == "cryptographic"
        assert transaction["proof"]["independently_verified"] is True
assert result["replay"]["stage"] == "wallet_preflight_not_submitted"
assert result["replay"]["state_unchanged"] is True
print(f"PASS: {count} settled sequential updates, commitments, finality and replay rejection agree.")
print("Development receipts." if result["risc0_dev_mode"] == "1" else "Real transaction receipts independently verified by the driver; verify archived receipts separately.")
