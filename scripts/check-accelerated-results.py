#!/usr/bin/env python3
"""Check actual execution evidence; never treats a JSON report as a validity oracle."""
import json
from pathlib import Path
root=Path(__file__).resolve().parent.parent
def report(name):return json.loads((root/name).read_text())
stock=report('rust/executor-report.json'); accelerated=report('rust/accelerated/optimized-executor-report.json')
sj=json.loads(stock['journal']);aj=json.loads(accelerated['journal'])
assert sj['accepted'] and aj['accepted'] and sj['result']==aj['result']
assert stock['mode']==accelerated['mode']=='risc0_executor_only_no_proof'
assert 0<accelerated['user_cycles']<stock['user_cycles']
for case in ('bad-state','bad-signature'):
 r=report(f'rust/accelerated/{case}-executor-report.json');j=json.loads(r['journal'])
 assert r['image_id']==accelerated['image_id'] and not j['accepted']
r=report('rust/accelerated/incremental-executor-report.json');j=json.loads(r['journal'])
assert j['accepted'] and j['result']['execution_block']==17923026
assert j['result']['state_witness']==sj['result']['state_witness']
assert j['result']['previous_store_commitment']=='76084610a9e4dfe1aea101c87bf9734b3d7a5a423d0d63f222c2b75ecfdc3aac'
print('PASS same verified result: stock vs official acceleration; invalid inputs rejected; incremental state matches full replay.')
print(f"Measured cycle speedup: {stock['user_cycles']/accelerated['user_cycles']:.3f}x; incremental cycles: {r['user_cycles']:,}.")
print('Execution only. These reports do not establish zk proofs, LEZ gas, or a successful node deployment.')
