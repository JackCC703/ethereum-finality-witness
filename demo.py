#!/usr/bin/env python3
"""Historical Capella witness demo. Native Python; not a production light client."""
import argparse
import copy
import hashlib
import json
import time
from pathlib import Path
from py_ecc.bls import G2ProofOfPossession as BLS
from trie import HexaryTrie
from eth_hash.auto import keccak
import rlp

BASE = Path(__file__).resolve().parent
CHECKPOINT = '0x5afc212a7924789b2bc86acad3ab3a6ffb1f6e97253ea50bee7f4f51422c9275'
GENESIS = '0x4b363db94e286120d76eb905340fdd4e54bfe9f06bf33ff6cf5ad27f511bfe95'
# Source: Helios ethereum/tests/sync.rs and mainnet configuration, pinned in provenance.json.
# Only replay these historical Capella slots. This is NOT a current bootstrap policy.
CURRENT_SLOT = 7109432
ZERO = bytes(32)

class Rejected(ValueError):
    pass

def need(ok, code):
    if not ok:
        raise Rejected(code)

def raw(s): return bytes.fromhex(s.removeprefix('0x'))
def hx(b): return '0x' + b.hex()
def h(a, b): return hashlib.sha256(a+b).digest()
def uint(v): return int(v).to_bytes(32, 'little')

def merkle(values, limit=None):
    limit = limit or max(1, len(values))
    need(len(values) <= limit, 'SSZ_LIMIT')
    width = 1 << (limit-1).bit_length()
    nodes = list(values)
    depth = (width-1).bit_length()
    z = ZERO
    if not nodes: nodes = [z]
    for _ in range(depth):
        if len(nodes) % 2: nodes.append(z)
        nodes = [h(nodes[i], nodes[i+1]) for i in range(0,len(nodes),2)]
        z = h(z,z)
    need(len(nodes)==1, 'SSZ_DEPTH')
    return nodes[0]

def bytevector(v):
    return merkle([v[i:i+32].ljust(32,b'\0') for i in range(0,len(v),32)])

def bytelist(v, limit):
    need(len(v)<=limit,'SSZ_BYTE_LIMIT')
    return h(merkle([v[i:i+32].ljust(32,b'\0') for i in range(0,len(v),32)],(limit+31)//32),uint(len(v)))

def beacon_root(b):
    return merkle([uint(b['slot']),uint(b['proposer_index']),raw(b['parent_root']),raw(b['state_root']),raw(b['body_root'])])

def execution_root(e):
    fields = ['parent_hash','fee_recipient','state_root','receipts_root','logs_bloom','prev_randao','block_number','gas_limit','gas_used','timestamp','extra_data','base_fee_per_gas','block_hash','transactions_root','withdrawals_root']
    out=[]
    for k in fields:
        if k in ['block_number','gas_limit','gas_used','timestamp','base_fee_per_gas']: out.append(uint(e[k]))
        elif k=='extra_data': out.append(bytelist(raw(e[k]),32))
        else: out.append(bytevector(raw(e[k])))
    return merkle(out)

def branch(leaf, siblings, gindex, root, code):
    need(len(siblings)==gindex.bit_length()-1, code+'_DEPTH')
    for sibling in siblings:
        sibling = raw(sibling) if isinstance(sibling,str) else sibling
        leaf = h(sibling,leaf) if gindex&1 else h(leaf,sibling)
        gindex//=2
    need(gindex==1 and leaf==root, code)

def committee_root(c):
    need(len(c['pubkeys'])==512,'COMMITTEE_SIZE')
    return merkle([merkle([bytevector(raw(p)) for p in c['pubkeys']],512),bytevector(raw(c['aggregate_pubkey']))])

def validate_header(header):
    slot=int(header['beacon']['slot'])
    need(194048*32 <= slot < 269568*32,'UNSUPPORTED_FORK')
    branch(execution_root(header['execution']),header['execution_branch'],25,raw(header['beacon']['body_root']),'EXECUTION_BRANCH')

class LightClient:
    def __init__(self,bootstrap,checkpoint=CHECKPOINT,genesis=GENESIS):
        validate_header(bootstrap['header'])
        need(beacon_root(bootstrap['header']['beacon'])==raw(checkpoint),'CHECKPOINT_MISMATCH')
        branch(committee_root(bootstrap['current_sync_committee']),bootstrap['current_sync_committee_branch'],54,raw(bootstrap['header']['beacon']['state_root']),'BOOTSTRAP_COMMITTEE')
        self.header=copy.deepcopy(bootstrap['header'])
        self.current=copy.deepcopy(bootstrap['current_sync_committee'])
        self.next=None
        self.genesis=raw(genesis)
        self.applied=[]

    def update(self,u):
        att=u['attested_header']; fin=u['finalized_header']
        sigslot=int(u['signature_slot']); attslot=int(att['beacon']['slot']); finslot=int(fin['beacon']['slot'])
        oldslot=int(self.header['beacon']['slot']); period=oldslot//8192
        need(CURRENT_SLOT>=sigslot>attslot>=finslot,'SLOT_ORDER')
        need(finslot>oldslot or self.next is None and 'next_sync_committee' in u,'STALE_UPDATE')
        need(sigslot//8192 in (period,period+1),'SIGNATURE_PERIOD')
        c=self.current if sigslot//8192==period else self.next
        need(c is not None,'UNKNOWN_COMMITTEE')
        validate_header(att);validate_header(fin)
        branch(beacon_root(fin['beacon']),u['finality_branch'],105,raw(att['beacon']['state_root']),'FINALITY_BRANCH')
        if 'next_sync_committee' in u:
            branch(committee_root(u['next_sync_committee']),u['next_sync_committee_branch'],55,raw(att['beacon']['state_root']),'NEXT_COMMITTEE_BRANCH')
        bits=raw(u['sync_aggregate']['sync_committee_bits'])
        need(len(bits)==64,'PARTICIPATION_ENCODING')
        keys=[raw(p) for i,p in enumerate(c['pubkeys']) if (bits[i//8]>>(i%8))&1]
        need(len(keys)*3>=512*2,'INSUFFICIENT_PARTICIPATION')
        # DOMAIN_SYNC_COMMITTEE 0x07000000; Capella fork version 0x03000000.
        # All signature slots in this replay are Capella; do not apply to other forks.
        forkroot=merkle([raw('03000000').ljust(32,b'\0'),self.genesis])
        domain=raw('07000000')+forkroot[:28]
        signing=h(beacon_root(att['beacon']),domain)
        need(BLS.FastAggregateVerify(keys,signing,raw(u['sync_aggregate']['sync_committee_signature'])),'INVALID_BLS_SIGNATURE')
        if finslot//8192==period+1:
            need(self.next is not None,'MISSING_HANDOFF')
            self.current=self.next
            self.next=u.get('next_sync_committee')
        else:
            need(finslot//8192==period,'FINALITY_PERIOD')
            if self.next is None and 'next_sync_committee' in u:self.next=u['next_sync_committee']
        if finslot>oldslot:self.header=copy.deepcopy(fin)
        self.applied.append({'slot':finslot,'signers':len(keys),'period':finslot//8192})


def load(name):
    return json.loads((BASE/'fixtures'/name).read_text())

def replay():
    lc=LightClient(load('bootstrap.json')['data'])
    for wrap in load('updates.json'):
        u=wrap['data']
        lc.update(u)
    lc.update(load('finality.json')['data'])
    return lc


def tx_witness(lc):
    payload=load('finalized-block.json')['data']['message']['body']['execution_payload']
    need(payload['block_hash']==lc.header['execution']['block_hash'],'BLOCK_BINDING')
    txs=[raw(t) for t in payload['transactions']]
    leaves=[bytelist(t,2**30) for t in txs]
    root=h(merkle(leaves,2**20),uint(len(txs)))
    need(root==raw(lc.header['execution']['transactions_root']),'TRANSACTION_LIST_ROOT')
    nodes=leaves[:]; idx=0; z=ZERO; proof=[]
    for _ in range(20):
        if len(nodes)%2:nodes.append(z)
        proof.append(nodes[idx^1] if idx^1<len(nodes) else z)
        nodes=[h(nodes[i],nodes[i+1]) for i in range(0,len(nodes),2)]
        idx//=2;z=h(z,z)
    proof.append(uint(len(txs)))
    branch(leaves[0],proof,2**21,root,'TX_INCLUSION')
    return {'transaction_hash':hx(keccak(txs[0])),'transaction_index':0,'transaction_count':len(txs),'ssz_branch':[hx(p) for p in proof],'transaction_root':hx(root)},leaves[0],proof,root


def verify_state(state_root, response):
    """Verify account+storage against a caller-supplied authenticated state root.

    The caller must supply the root from the finalized execution header. No root,
    code hash, balance, or storage value in the RPC response is a trust anchor.
    Returns only values extracted from authenticated trie leaves.
    """
    p = response['result']
    address = raw(p['address'])
    need(len(address) == 20, 'ACCOUNT_ADDRESS')
    try:
        encoded = HexaryTrie.get_from_proof(
            state_root, keccak(address), [rlp.decode(raw(n)) for n in p['accountProof']])
        need(bool(encoded), 'ACCOUNT_ABSENT')
        account = rlp.decode(encoded)
    except Exception as e:
        raise Rejected('ACCOUNT_PROOF_INVALID') from e
    need(len(account) == 4, 'ACCOUNT_ENCODING')
    nonce, balance = (int.from_bytes(v, 'big') for v in account[:2])
    storage_root, code_hash = account[2:]
    need(nonce == int(p['nonce'], 16), 'ACCOUNT_NONCE')
    need(balance == int(p['balance'], 16), 'ACCOUNT_BALANCE')
    need(storage_root == raw(p['storageHash']) and code_hash == raw(p['codeHash']), 'ACCOUNT_FIELDS')
    slots = []
    need(len(p['storageProof']) > 0, 'STORAGE_PROOF_MISSING')
    for witness in p['storageProof']:
        slot = int(witness['key'], 16)
        need(0 <= slot < 2**256, 'STORAGE_SLOT')
        try:
            encoded = HexaryTrie.get_from_proof(
                storage_root, keccak(slot.to_bytes(32, 'big')),
                [rlp.decode(raw(n)) for n in witness['proof']])
            value = int.from_bytes(rlp.decode(encoded), 'big') if encoded else 0
        except Exception as e:
            raise Rejected('STORAGE_PROOF_INVALID') from e
        need(value == int(witness['value'], 16), 'STORAGE_VALUE')
        slots.append({'slot': hx(slot.to_bytes(32, 'big')), 'value': hx(value.to_bytes(32, 'big'))})
    return {'address': hx(address), 'nonce': nonce, 'balance_wei': str(balance),
            'state_root': hx(state_root), 'storage_root': hx(storage_root),
            'code_hash': hx(code_hash), 'storage': slots,
            'anchor': 'replayed_finalized_execution_header_state_root'}


def state_witness(lc):
    result = verify_state(raw(lc.header['execution']['state_root']), load('finalized-state-proof.json'))
    result['execution_block'] = int(lc.header['execution']['block_number'])
    return result


def expect_reject(name,fn):
    try:fn()
    except (Rejected,ValueError,KeyError) as e:return {'test':name,'result':'PASS','reason':str(e)}
    raise AssertionError('negative control accepted: '+name)


def run():
    t=time.perf_counter();lc=replay();elapsed=time.perf_counter()-t
    witness,leaf,proof,root=tx_witness(lc)
    badproof=proof[:];badproof[0]=bytes([badproof[0][0]^1])+badproof[0][1:]
    negatives=[]
    b=load('bootstrap.json')['data']
    negatives.append(expect_reject('wrong checkpoint',lambda:LightClient(b,'0x'+'00'*32)))
    update=load('updates.json')[0]['data']
    bad=copy.deepcopy(update);s=raw(bad['sync_aggregate']['sync_committee_signature']);bad['sync_aggregate']['sync_committee_signature']=hx(bytes([s[0]^1])+s[1:])
    negatives.append(expect_reject('tampered BLS signature',lambda:LightClient(b).update(bad)))
    negatives.append(expect_reject('wrong network signing domain',lambda:LightClient(b,genesis='0x'+'11'*32).update(update)))
    low=copy.deepcopy(update);low['sync_aggregate']['sync_committee_bits']='0x'+'00'*64
    negatives.append(expect_reject('insufficient participation',lambda:LightClient(b).update(low)))
    fake=copy.deepcopy(update);fake['finalized_header']['execution']['state_root']='0x'+'11'*32
    negatives.append(expect_reject('tampered execution state root',lambda:LightClient(b).update(fake)))
    negatives.append(expect_reject('tampered transaction proof',lambda:branch(leaf,badproof,2**21,root,'TX_INCLUSION')))
    negatives.append(expect_reject('tampered transaction value',lambda:branch(bytes([leaf[0]^1])+leaf[1:],proof,2**21,root,'TX_INCLUSION')))
    account=state_witness(lc)
    state_root=raw(lc.header['execution']['state_root'])
    response=load('finalized-state-proof.json')
    def mutated_account():
        bad=copy.deepcopy(response)
        node=raw(bad['result']['accountProof'][0])
        bad['result']['accountProof'][0]=hx(node[:-1]+bytes([node[-1]^1]))
        verify_state(state_root,bad)
    def mutated_storage():
        bad=copy.deepcopy(response)
        node=raw(bad['result']['storageProof'][0]['proof'][0])
        bad['result']['storageProof'][0]['proof'][0]=hx(node[:-1]+bytes([node[-1]^1]))
        verify_state(state_root,bad)
    def mutated_balance():
        bad=copy.deepcopy(response);bad['result']['balance']='0x1'
        verify_state(state_root,bad)
    def mutated_value():
        bad=copy.deepcopy(response);bad['result']['storageProof'][0]['value']='0x1'
        verify_state(state_root,bad)
    negatives.append(expect_reject('tampered finalized account proof',mutated_account))
    negatives.append(expect_reject('tampered finalized storage proof',mutated_storage))
    negatives.append(expect_reject('false RPC account balance',mutated_balance))
    negatives.append(expect_reject('false RPC storage value',mutated_value))
    negatives.append(expect_reject('wrong execution root for account proof',
        lambda:verify_state(bytes([state_root[0]^1])+state_root[1:],response)))
    report={'status':'native_historical_capella_demo','network':'ethereum-mainnet','checkpoint':CHECKPOINT,'historical_evaluation_slot':CURRENT_SLOT,'finalized_slot':int(lc.header['beacon']['slot']),'finalized_execution_block':int(lc.header['execution']['block_number']),'verified_updates':lc.applied,'finality_wall_seconds':round(elapsed,3),'transaction_witness':witness,'finalized_state_witness':account,'negative_controls':negatives,'limitations':['No LEZ execution, no RISC0 guest, no zk proof or cycle measurement.','Historical Capella fixture only, not a current weak-subjectivity bootstrap.','No full fork schedule, live sync, persistence, operator, or audit.']}
    print(json.dumps(report,indent=2))
    return report

if __name__=='__main__':
    parser=argparse.ArgumentParser();parser.add_argument('--report',type=Path);args=parser.parse_args()
    report=run()
    if args.report:args.report.write_text(json.dumps(report,indent=2)+'\n')
