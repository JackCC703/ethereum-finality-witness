#!/usr/bin/env python3
"""Build the shared native/zkVM input exclusively from checked fixture files."""
from pathlib import Path
import json
base=Path(__file__).resolve().parent
names={'bootstrap':'bootstrap.json','updates':'updates.json','finality':'finality.json','state_proof':'finalized-state-proof.json'}
v={k:json.loads((base.parent/'fixtures'/n).read_text()) for k,n in names.items()}
(base/'input.json').write_text(json.dumps(v,separators=(',',':'))+'\n')
print('Wrote input.json; consensus checkpoint and network are pinned inside verifier code.')
