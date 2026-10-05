#!/usr/bin/env python3
"""Small private-regtest probes; never publish payload probes on the public signet.

This is a policy characterization, not a comprehensive spam/DoS review.
"""
import hashlib
import json
import os
from pathlib import Path

from test_framework.test_framework import BitcoinTestFramework
from test_framework.wallet import MiniWallet
from test_framework.key import compute_xonly_pubkey, ORDER, secp256k1, TaggedHash
from test_framework.messages import CTransaction, CTxIn, CTxOut, COutPoint, CTxInWitness
from test_framework.script import CScript, CScriptOp, OP_EQUAL, OP_SHA256, taproot_construct


def sign_message(key, message):
    # BIP340 reference algorithm from Bitcoin's test_framework/key.py, with
    # arbitrary-length messages as supported by the experimental CSFS verifier.
    sec = int.from_bytes(key, 'big')
    public = sec * secp256k1.G
    if not public.y.is_even():
        sec = ORDER - sec
    t = (sec ^ int.from_bytes(TaggedHash('BIP0340/aux', bytes(32)), 'big')).to_bytes(32, 'big')
    nonce = int.from_bytes(TaggedHash('BIP0340/nonce', t + public.to_bytes_xonly() + message), 'big') % ORDER
    assert nonce
    point = nonce * secp256k1.G
    k = nonce if point.y.is_even() else ORDER - nonce
    e = int.from_bytes(TaggedHash('BIP0340/challenge', point.to_bytes_xonly() + public.to_bytes_xonly() + message), 'big') % ORDER
    return point.to_bytes_xonly() + ((k + e * sec) % ORDER).to_bytes(32, 'big')


class DataPolicyTest(BitcoinTestFramework):
    def set_test_params(self):
        self.num_nodes = 1
        self.setup_clean_chain = True
        self.extra_args = [['-xbtcovtest', '-testactivationheight=blake2b@1', '-rdtsexpiry=2147483647']]

    def run_test(self):
        node = self.nodes[0]
        wallet = MiniWallet(node)
        self.generate(wallet, 105)
        key = (42).to_bytes(32, 'big')
        public = compute_xonly_pubkey(key)[0]
        evidence = dict(network='private-regtest', probes=[], scope='default policy, small samples only')
        cases = [('csfs-witness', n) for n in (32, 80, 81)]
        cases += [('csfs-script', n) for n in (80, 256, 257)]
        cases += [('sha256-witness', n) for n in (32, 80, 81)]
        cases += [('sha256-script', n) for n in (80, 256, 257)]
        for mode, size in cases:
            message = (b'Private policy test data. '*20)[:size]
            if mode == 'csfs-witness':
                script = CScript([public, CScriptOp(0xcc)])
                witness = [sign_message(key, message), message]
            elif mode == 'csfs-script':
                script = CScript([message, public, CScriptOp(0xcc)])
                witness = [sign_message(key, message)]
            elif mode == 'sha256-witness':
                script = CScript([OP_SHA256, hashlib.sha256(message).digest(), OP_EQUAL])
                witness = [message]
            else:
                script = CScript([message, OP_SHA256, hashlib.sha256(message).digest(), OP_EQUAL])
                witness = []
            tap = taproot_construct(public, [('probe', script)])
            funded = wallet.send_to(from_node=node, scriptPubKey=tap.scriptPubKey, amount=100000)
            self.generate(wallet, 1)
            tx = CTransaction()
            tx.version = 2
            tx.vin = [CTxIn(COutPoint(int(funded['txid'],16),funded['sent_vout']), nSequence=0xfffffffd)]
            tx.vout = [CTxOut(99000,wallet.get_output_script())]
            control = bytes([0xc0 | tap.negflag])+public+tap.leaves['probe'].merklebranch
            tx.wit.vtxinwit = [CTxInWitness()]
            tx.wit.vtxinwit[0].scriptWitness.stack = witness+[script,control]
            raw = tx.serialize().hex()
            result = node.testmempoolaccept([raw])[0]
            expected = size <= (256 if mode.endswith('-script') else 80)
            assert result['allowed'] == expected, (mode,size,result)
            evidence['probes'].append(dict(mode=mode,message_bytes=size,result=result))
            self.log.info('%s %s bytes: %s', mode,size,result['allowed'])
        evidence['passed'] = True
        Path(os.environ['COVENANT_RESULTS']).write_text(json.dumps(evidence,indent=2,default=str)+'\n')


if __name__ == '__main__':
    DataPolicyTest(__file__).main()
