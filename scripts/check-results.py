#!/usr/bin/env python3
"""Compare independently executed native and RISC0 outputs; never authorizes consumption."""
from pathlib import Path
import json

root = Path(__file__).resolve().parent.parent
native = json.loads((root / 'rust/native-report.json').read_text())
execution = json.loads((root / 'rust/executor-report.json').read_text())
journal = json.loads(execution['journal'])
assert journal['accepted'] is True, journal
assert journal['result'] == native, 'RISC0 and native verifier outputs differ'
assert execution['mode'] == 'risc0_executor_only_no_proof'
assert execution['user_cycles'] > 0
for case in ('bad-state', 'bad-signature'):
    negative = json.loads((root / f'rust/{case}-executor-report.json').read_text())
    negative_journal = json.loads(negative['journal'])
    assert negative_journal['accepted'] is False, (case, negative_journal)
    assert negative_journal['error'], case
consumer = json.loads((root / 'rust/consumer-native-report.json').read_text())
assert consumer['bad_proof']['state_unchanged']
assert consumer['replay']['state_unchanged']
assert consumer['false_predicate']['state_unchanged']
print('PASS: native/RISC0 outputs identical; measured execution cycles; consumer rejection leaves state unchanged.')
print('PASS: bad state value and bad BLS signature rejected inside the actual RISC0 guest.')
print('This checks execution evidence. It does not create or verify a zk proof or a LEZ transaction.')
