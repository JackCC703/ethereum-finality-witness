#!/usr/bin/env python3
"""Native, in-process example consumer. No token minting or LEZ transaction claim."""
from copy import deepcopy
from dataclasses import dataclass
import hashlib
import json
from pathlib import Path

import demo

DEPOSIT_CONTRACT = '0x00000000219ab540356cbb839cbe05303d7705fa'


@dataclass(frozen=True)
class Policy:
    address: str = DEPOSIT_CONTRACT
    slot: int = 0
    minimum: int = 1
    chain_id: int = 1


class Consumer:
    """One ticket per authenticated (chain, block, account, storage-slot) fact.

    The example's bootstrap is internal: no caller-supplied report or state root
    can authorize an action. Native process memory is its trust boundary; this
    is not a cross-process proof protocol or a deployed smart contract.
    """
    def __init__(self, policy=Policy()):
        verified = demo.replay()
        self._header = deepcopy(verified.header['execution'])
        self._policy = policy
        demo.need(policy.chain_id == 1, 'POLICY_NETWORK')
        demo.need(policy.minimum >= 0 and 0 <= policy.slot < 2**256, 'POLICY_RANGE')
        self._state = {'tickets': 0, 'consumed': []}

    @property
    def state(self):
        return deepcopy(self._state)

    def consume(self, response, chain_id=1):
        # No state write occurs before all checks succeed.
        demo.need(chain_id == self._policy.chain_id, 'CHAIN_MISMATCH')
        fact = demo.verify_state(demo.raw(self._header['state_root']), response)
        demo.need(fact['address'].lower() == self._policy.address.lower(), 'SUBJECT_MISMATCH')
        matches = [s for s in fact['storage'] if int(s['slot'], 16) == self._policy.slot]
        demo.need(len(matches) == 1, 'SLOT_MISMATCH')
        demo.need(int(matches[0]['value'], 16) >= self._policy.minimum, 'PREDICATE_FALSE')
        identity = (b'finality-witness/consumer-fact/v1\0'
                    + chain_id.to_bytes(8, 'big')
                    + demo.raw(self._header['block_hash'])
                    + demo.raw(fact['address'])
                    + self._policy.slot.to_bytes(32, 'big'))
        statement_id = hashlib.sha256(identity).hexdigest()
        demo.need(statement_id not in self._state['consumed'], 'ALREADY_CONSUMED')
        next_state = deepcopy(self._state)
        next_state['consumed'].append(statement_id)
        next_state['tickets'] += 1
        self._state = next_state
        return {'statement_id': statement_id, 'action': 'ticket_granted',
                'execution_block': int(self._header['block_number']),
                'account': fact['address'], 'storage': matches[0]}


def run():
    consumer = Consumer()
    witness = demo.load('finalized-state-proof.json')
    checks = []
    corrupt = deepcopy(witness)
    node = demo.raw(corrupt['result']['storageProof'][0]['proof'][0])
    corrupt['result']['storageProof'][0]['proof'][0] = demo.hx(node[:-1] + bytes([node[-1] ^ 1]))
    before = consumer.state
    check = demo.expect_reject('invalid proof before success', lambda: consumer.consume(corrupt))
    demo.need(consumer.state == before, 'FAILED_PROOF_CHANGED_STATE')
    checks.append(check)
    action = consumer.consume(witness)
    checks.append({'test': 'valid proof grants one ticket', 'result': 'PASS'})
    before = consumer.state
    checks.append(demo.expect_reject('same fact cannot be consumed twice', lambda: consumer.consume(witness)))
    demo.need(consumer.state == before, 'REPLAY_CHANGED_STATE')
    checks.append(demo.expect_reject('wrong chain rejected', lambda: consumer.consume(witness, chain_id=11155111)))
    demo.need(consumer.state == before, 'WRONG_CHAIN_CHANGED_STATE')
    report = {'mode': 'native_in_process_consumer', 'policy': 'deposit contract slot 0 >= 1',
              'action': action, 'checks': checks, 'final_state': consumer.state,
              'limitations': 'Demonstration tickets only; no assets, LEZ execution, concurrency, or crash-atomic persistence.'}
    Path(__file__).with_name('consumer-report.json').write_text(json.dumps(report, indent=2) + '\n')
    print(json.dumps(report, indent=2))


if __name__ == '__main__':
    run()
