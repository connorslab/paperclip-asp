#!/usr/bin/env python3
"""Private real-node test. Never connects to a production RPC or existing datadir.

Uses the pinned covenant node's functional framework via PYTHONPATH.
The wallet and ASP are separate Rust executables, invoked as separate processes.
The test harness passes user secrets only to wallet commands. The ASP commands receive permits.
"""
from decimal import Decimal
import json
import os
from pathlib import Path
import secrets
import subprocess

from test_framework.test_framework import BitcoinTestFramework
from test_framework.wallet import MiniWallet
from test_framework.messages import tx_from_hex, CTxOut
from test_framework.script import CScript, TaprootSignatureHash
from test_framework.key import sign_schnorr

PROFILE = 'paperclip-signet-offline-refresh-v1'
CHALLENGE = '2102396d38e3ff703be31a2d97317835f4e9645b5ae5ea2d2d1ba406afa7ab185b5fac'


class OfflineRefreshTest(BitcoinTestFramework):
    def set_test_params(self):
        self.num_nodes = 1
        self.setup_clean_chain = True
        self.extra_args = [['-xbtcovtest', '-testactivationheight=blake2b@1', '-rdtsexpiry=2147483647']]

    def run_test(self):
        node = self.nodes[0]
        faucet = MiniWallet(node)
        self.generate(faucet, 105)
        bins = Path(os.environ.get('COVENANT_BIN', 'target/debug/examples'))
        self.wallet_offline = False
        self.asp_offline = False
        evidence = {'network': 'private-regtest', 'profile': PROFILE, 'checks': [], 'transactions': []}
        # All generated keys are disposable. Store privately, never in evidence.
        keyfile = Path(self.options.tmpdir) / 'test-keys.json'
        keys = [secrets.token_hex(32) for _ in range(6)]
        with open(keyfile, 'x', opener=lambda p,f: os.open(p,f,0o600)) as f:
            json.dump(keys, f)

        def call(role, command, **params):
            assert not (role == 'wallet' and self.wallet_offline), 'wallet must remain offline'
            assert not (role == 'asp' and self.asp_offline), 'ASP must remain offline'
            data = {'profile': PROFILE, 'challenge': CHALLENGE, 'command': command, **params}
            binary = os.environ.get('COVENANT_' + role.upper())
            cmd = [binary, 'covenant-lab', '--experimental-signet'] if binary else [str(bins / ('covenant-' + role)), '--experimental-signet']
            result = subprocess.run(cmd,
                                    input=json.dumps(data), text=True, capture_output=True)
            assert result.returncode == 0, result.stderr
            return json.loads(result.stdout)

        pubkeys = [call('wallet', 'pubkey', secret=k)['pubkey'] for k in keys]

        def state(i, expiry, amount=100000):
            return {'owner': pubkeys[i], 'server': pubkeys[5], 'amount_sat': amount,
                    'expiry': expiry, 'exit_delay': 12}

        def mine_to(height):
            if node.getblockcount() < height:
                self.generate(faucet, height - node.getblockcount())

        def allow(raw):
            verdict = node.testmempoolaccept([raw])[0]
            assert verdict['allowed'], verdict

        def reject(raw, name):
            verdict = node.testmempoolaccept([raw])[0]
            assert not verdict['allowed'], name
            evidence['checks'].append({'test': name, 'rejection': verdict.get('reject-reason')})

        def broadcast(raw, name):
            allow(raw)
            txid = node.sendrawtransaction(raw)
            block = self.generate(faucet, 1)[0]
            assert txid in node.getblock(block)['tx']
            evidence['transactions'].append({'step': name, 'txid': txid, 'block': block})
            return txid

        def round_fund(s, name):
            info = call('asp', 'round', state=s)
            funded = faucet.send_to(from_node=node, scriptPubKey=CScript(bytes.fromhex(info['script_pubkey'])),
                                    amount=info['funding_sat'])
            block = self.generate(faucet, 1)[0]
            evidence['transactions'].append({'step':name, 'txid':funded['txid'], 'block':block})
            return call('asp', 'round', state=s, funding=f"{funded['txid']}:{funded['sent_vout']}")

        states = [state(0, 160), state(1, 200, 99000), state(2, 240, 98000)]
        original = round_fund(states[0], 'initial round')
        # Authorize two future refunds BEFORE their round funding txids exist.
        permits = [call('wallet','authorize', old=states[i],new=states[i+1],not_before=120+30*i,
                        old_secret=keys[i],new_secret=keys[i+1]) for i in range(2)]
        stale_claim = call('wallet','claim', state=states[0],outpoint=original['txid']+':0',
                           destination=faucet.get_output_script().hex(),secret=keys[0])
        # Persist authorization before going offline. A restarted server consumes
        # only this file, not a wallet database or a process with the wallet seed.
        journal = Path(self.options.tmpdir) / 'permits.json'
        journal.write_text(json.dumps(permits))
        self.wallet_offline = True
        permits = json.loads(journal.read_text())
        old_exit = original
        old_point = None
        recoveries = []
        for i, permit in enumerate(permits):
            call('asp','verify',permit=permit)
            next_exit = round_fund(states[i+1], f'offline refresh {i+1} round')
            if old_point is None:
                old_point = broadcast(old_exit['hex'], 'old exit attempt') + ':0'
            new_point = broadcast(next_exit['hex'], f'refresh {i+1} unroll') + ':0'
            refund = call('asp','refund',permit=permit,old_outpoint=old_point,
                          new_outpoint=new_point,secret=keys[5])
            reject(refund['hex'], f'refresh {i+1} cannot accelerate fee schedule')
            mine_to(permit['not_before'])
            allow(refund['hex'])
            mutated = tx_from_hex(refund['hex'])
            mutated.vout[0].nValue -= 1
            reject(mutated.serialize().hex(), f'refresh {i+1} reduced user output')
            mutated = tx_from_hex(refund['hex'])
            mutated.vout[0].scriptPubKey = faucet.get_output_script()
            reject(mutated.serialize().hex(), f'refresh {i+1} redirected output')
            mutated = tx_from_hex(refund['hex'])
            mutated.wit.vtxinwit[0].scriptWitness.stack[1] = bytes(64)
            reject(mutated.serialize().hex(), f'refresh {i+1} missing user authorization')
            # Rebinding is safe only within the exact template. A third input or
            # swapped input positions changes it and must not pass CSFS.
            mutated = tx_from_hex(refund['hex'])
            mutated.vin.reverse()
            mutated.wit.vtxinwit.reverse()
            reject(mutated.serialize().hex(), f'refresh {i+1} swapped inputs')
            txid = broadcast(refund['hex'], f'offline refresh {i+1} refund')
            assert node.gettxout(old_point.split(':')[0],int(old_point.split(':')[1])) is None
            assert node.gettxout(new_point.split(':')[0],0) is None
            assert node.gettxout(txid,0)['value'] == Decimal(states[i+1]['amount_sat']) / 100000000
            recoveries.append({'state':states[i+1], 'outpoint':txid+':0', 'refund':refund['hex']})
            old_point = txid + ':0'
        mine_to(states[0]['expiry'] + states[0]['exit_delay'])
        stale_verdict = node.testmempoolaccept([stale_claim['hex']])[0]
        assert stale_verdict.get('reject-reason') in ('missing-inputs', 'bad-txns-inputs-missingorspent'), stale_verdict
        reject(stale_claim['hex'], 'mature old claim cannot double-spend after refresh')
        evidence['checks'].append({'test':'two refreshes completed with wallet process unavailable','passed':True})

        recovery = Path(self.options.tmpdir) / 'recovery.json'
        recovery.write_text(json.dumps(recoveries))
        self.restart_node(0)
        node = self.nodes[0]
        faucet = MiniWallet(node)
        recovered = json.loads(recovery.read_text())[-1]
        assert node.gettxout(recovered['outpoint'].split(':')[0],0) is not None
        self.asp_offline = True
        self.wallet_offline = False
        claim = call('wallet','claim', state=recovered['state'],outpoint=recovered['outpoint'],
                     destination=faucet.get_output_script().hex(), secret=keys[2])
        reject(claim['hex'],'unilateral claim respects reaction window')
        mine_to(states[2]['expiry'] + states[2]['exit_delay'])
        tx = tx_from_hex(claim['hex'])
        prev = CTxOut(states[2]['amount_sat'], CScript(bytes.fromhex(next_exit['exit_script_pubkey'])))
        script = CScript(tx.wit.vtxinwit[0].scriptWitness.stack[-2])
        legacy_hash = TaprootSignatureHash(tx,[prev],1,input_index=0,scriptpath=True,leaf_script=script)
        tx.wit.vtxinwit[0].scriptWitness.stack[0] = sign_schnorr(bytes.fromhex(keys[2]), legacy_hash) + b'\x01'
        allow(tx.serialize().hex())
        evidence['checks'].append({'test':'consensus still accepts correctly generated legacy signatures; lab signer only emits unified','passed':True})
        tx = tx_from_hex(claim['hex'])
        assert tx.wit.vtxinwit[0].scriptWitness.stack[0][-1] == 0x21
        tx.wit.vtxinwit[0].scriptWitness.stack[0] = tx.wit.vtxinwit[0].scriptWitness.stack[0][:-1] + b'\x01'
        reject(tx.serialize().hex(),'unified signature cannot be relabeled as legacy')
        claim_txid = broadcast(claim['hex'],'wallet recovery with ASP offline')
        assert node.gettxout(claim_txid,0) is not None
        evidence['checks'].append({'test':'recovery survived node restart and ASP outage','passed':True})
        block = node.getbestblockhash()
        node.invalidateblock(block)
        node.setmocktime(node.getblockheader(block)['time'] + 1)
        allow(claim['hex']) if claim_txid not in node.getrawmempool() else None
        self.generate(faucet,1)
        assert node.gettxout(claim_txid,0) is not None
        evidence['checks'].append({'test':'claim reconfirmed after one-block reorg','passed':True})

        # Server never refreshes: the original timed user exit remains usable.
        self.asp_offline = False
        s = state(3,node.getblockcount()+20)
        original = round_fund(s,'unrefreshed fallback round')
        self.asp_offline = True
        broadcast(original['hex'],'unrefreshed unilateral unroll')
        fallback = call('wallet','claim',state=s,outpoint=original['txid']+':0',
                        destination=faucet.get_output_script().hex(),secret=keys[3])
        mine_to(s['expiry']+s['exit_delay'])
        broadcast(fallback['hex'],'unrefreshed unilateral claim')
        assert node.verifychain()
        evidence['checks'].append({'test':'no-refresh fallback exit works without ASP','passed':True})
        evidence['status'] = 'passed'
        Path(os.environ['COVENANT_RESULTS']).write_text(json.dumps(evidence,indent=2)+'\n')
        self.log.info('Offline refresh, invalid spends, unified signature handling and recovery passed')


if __name__ == '__main__':
    OfflineRefreshTest(__file__).main()
