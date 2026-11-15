#!/usr/bin/env python3
import copy
import json
from pathlib import Path

root = Path(__file__).resolve().parent.parent / 'rust'
original = json.loads((root / 'input.json').read_text())
bad_state = copy.deepcopy(original)
bad_state['state_proof']['result']['storageProof'][0]['value'] = '0x1'
(root / 'bad-state-input.json').write_text(json.dumps(bad_state, separators=(',', ':')) + '\n')
bad_signature = copy.deepcopy(original)
signature = bytearray.fromhex(bad_signature['updates'][0]['data']['sync_aggregate']['sync_committee_signature'][2:])
signature[0] ^= 1
bad_signature['updates'][0]['data']['sync_aggregate']['sync_committee_signature'] = '0x' + signature.hex()
(root / 'bad-signature-input.json').write_text(json.dumps(bad_signature, separators=(',', ':')) + '\n')
