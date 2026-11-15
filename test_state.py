"""State-witness security regressions; these tests do not replace consensus replay."""
import copy
import unittest
from demo import load, raw, verify_state, Rejected, hx

class StateProofTests(unittest.TestCase):
    def setUp(self):
        self.root = raw(load('finality.json')['data']['finalized_header']['execution']['state_root'])
        self.witness = load('finalized-state-proof.json')

    def test_valid_account_and_nonzero_storage(self):
        value = verify_state(self.root, self.witness)
        self.assertEqual(value['address'], '0x00000000219ab540356cbb839cbe05303d7705fa')
        self.assertEqual(value['balance_wei'], '28260967194325012575404192')
        self.assertEqual(value['storage'][0]['value'], '0x4e585d72ab3a6cb0387fc65e55a7e9bcf6ed555a04e5c1bed4b0ad1264310301')

    def rejects(self, change):
        witness = copy.deepcopy(self.witness)
        change(witness['result'])
        with self.assertRaises(Rejected): verify_state(self.root, witness)

    def test_wrong_authenticated_root(self):
        with self.assertRaises(Rejected): verify_state(bytes(32), self.witness)

    def test_wrong_address(self): self.rejects(lambda p: p.update(address='0x'+'11'*20))
    def test_account_proof_from_other_block(self): self.rejects(lambda p: p.update(accountProof=load('account-proof.json')['accountProof']))
    def test_forged_storage_root_metadata(self): self.rejects(lambda p: p.update(storageHash='0x'+'11'*32))
    def test_false_balance(self): self.rejects(lambda p: p.update(balance='0x1'))
    def test_wrong_storage_slot(self): self.rejects(lambda p: p['storageProof'][0].update(key='0x1'))
    def test_false_storage_value(self): self.rejects(lambda p: p['storageProof'][0].update(value='0x1'))
    def test_missing_storage_proof(self): self.rejects(lambda p: p.update(storageProof=[]))

    def test_tampered_storage_node(self):
        def change(p):
            proof=p['storageProof'][0]['proof'];node=raw(proof[0]);proof[0]=hx(node[:-1]+bytes([node[-1]^1]))
        self.rejects(change)

    def test_rejection_does_not_mutate_witness(self):
        before=copy.deepcopy(self.witness)
        with self.assertRaises(Rejected):verify_state(bytes(32), self.witness)
        self.assertEqual(self.witness,before)

if __name__=='__main__': unittest.main()
