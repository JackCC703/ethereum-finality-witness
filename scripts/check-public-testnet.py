#!/usr/bin/env python3
"""Read-only compatibility check for the archived LEZ v0.4 real-proof stack.

No wallet, secrets, faucet requests or transaction submissions are used.
A matching circuit is necessary, not sufficient, for a deployment.
"""
import argparse
import datetime
import json
from pathlib import Path
import urllib.request

ROOT = Path(__file__).resolve().parents[1]
# Keep the available network operations explicitly read-only.
METHODS = ("checkHealth", "getLastBlockId", "getProgramIds", "getFeeState", "getChannelId")


def inspect(rpc, expected):
    report = {
        "checked_utc": datetime.datetime.now(datetime.timezone.utc).isoformat(),
        "rpc": rpc,
        "expected_circuit_image_id": expected,
        "expected_stack": "archived LEZ v0.4.0-rc1 local real-proof integration",
        "status": "blocked",
        "deployed": False,
        "transactions_submitted": 0,
        "responses": [],
    }
    try:
        for index, method in enumerate(METHODS, 1):
            request = {"jsonrpc": "2.0", "id": index, "method": method, "params": []}
            http = urllib.request.Request(rpc, json.dumps(request).encode(),
                                          headers={"Content-Type": "application/json"})
            with urllib.request.urlopen(http, timeout=20) as response:
                result = json.load(response)
            report["responses"].append({"request": request, "response": result})
            if "error" in result:
                raise ValueError(f"{method}: {result['error']}")
            if result.get("id") != index or result.get("jsonrpc") != "2.0" or "result" not in result:
                raise ValueError(f"Invalid JSON-RPC response for {method}")
        programs = report["responses"][2]["response"]["result"]
        words = programs["privacy_preserving_circuit"]
        if (not isinstance(words, list) or len(words) != 8
                or any(type(word) is not int or not 0 <= word < 2**32 for word in words)):
            raise ValueError("Expected eight u32 circuit image words")
        actual = b"".join(word.to_bytes(4, "little") for word in words).hex()
        report["observed_circuit_image_id"] = actual
        report["circuit_matches_archived_stack"] = actual == expected
        if actual != expected:
            report["reason"] = "Public circuit differs from archived LEZ v0.4 stack; do not deploy with this wallet/program integration."
        else:
            report["status"] = "fingerprint_matches"
            report["reason"] = "Circuit matches. Runtime ABI, wallet, fee funding, and program execution still require validation before any deployment claim."
    except (OSError, ValueError, KeyError, TypeError) as error:
        report["status"] = "blocked"
        report["reason"] = str(error)
    return report


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--rpc", default="https://testnet.lez.logos.co")
    parser.add_argument("--out", type=Path)
    args = parser.parse_args()
    reference = json.loads((ROOT / "lez/evidence/compact-real/summary.json").read_text())
    assert reference["development_mode"] is False and reference["correct_image_verified"] is True
    report = inspect(args.rpc, reference["circuit_image_id"])
    content = json.dumps(report, indent=2) + "\n"
    if args.out:
        args.out.parent.mkdir(parents=True, exist_ok=True)
        args.out.write_text(content)
    print(content, end="")
    return 0 if report["status"] == "fingerprint_matches" else 2


if __name__ == "__main__":
    raise SystemExit(main())
