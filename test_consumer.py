import copy
import unittest
from dataclasses import replace

import consumer
import demo


class ConsumerTests(unittest.TestCase):
    @classmethod
    def setUpClass(cls):
        cls.verified = consumer.Consumer()
        cls.witness = demo.load('finalized-state-proof.json')

    def setUp(self):
        self.c = copy.deepcopy(self.verified)

    def unchanged_rejection(self, fn):
        before = self.c.state
        with self.assertRaises(demo.Rejected):
            fn()
        self.assertEqual(before, self.c.state)

    def test_valid_fact_authorizes_once(self):
        result = self.c.consume(self.witness)
        self.assertEqual(result['execution_block'], 17923026)
        self.assertEqual(self.c.state['tickets'], 1)
        self.unchanged_rejection(lambda: self.c.consume(self.witness))

    def test_bad_proof_then_retry(self):
        bad = copy.deepcopy(self.witness)
        bad['result']['accountProof'][0] = '0xc0'
        self.unchanged_rejection(lambda: self.c.consume(bad))
        self.c.consume(self.witness)
        self.assertEqual(self.c.state['tickets'], 1)

    def test_wrong_chain(self):
        self.unchanged_rejection(lambda: self.c.consume(self.witness, chain_id=11155111))

    def test_false_predicate(self):
        self.c._policy = replace(self.c._policy, minimum=2**256)
        self.unchanged_rejection(lambda: self.c.consume(self.witness))

    def test_wrong_expected_subject(self):
        self.c._policy = replace(self.c._policy, address='0x' + '11' * 20)
        self.unchanged_rejection(lambda: self.c.consume(self.witness))

    def test_wrong_expected_slot(self):
        self.c._policy = replace(self.c._policy, slot=1)
        self.unchanged_rejection(lambda: self.c.consume(self.witness))

    def test_state_snapshot_cannot_authorize_replay(self):
        self.c.consume(self.witness)
        exposed = self.c.state
        exposed['consumed'].clear()
        self.unchanged_rejection(lambda: self.c.consume(self.witness))


if __name__ == '__main__':
    unittest.main()
