#!/usr/bin/env python3
"""Check distributed fixture bytes; checksums are not consensus verification."""
import hashlib
import json
from pathlib import Path

base = Path(__file__).resolve().parent / 'fixtures'
manifest = json.loads((base / 'provenance.json').read_text())
for name, info in manifest['files'].items():
    actual = hashlib.sha256((base / name).read_bytes()).hexdigest()
    if actual != info['sha256']:
        raise SystemExit(f'FAIL fixture checksum: {name}')
    print(f'PASS fixture checksum: {name}')
print('Integrity only: trust anchors and consensus verification are separate.')
