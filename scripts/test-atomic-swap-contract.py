"""Fresh isolated XBT regtest: actual unified-sighash HTLC spends, not Ark VTXOs."""
import base64
import hashlib
import json
import os
from pathlib import Path
import socket
import subprocess
import tempfile
import time
import urllib.request


def main():
    node = Path(os.environ['XBT_BITCOIND']).resolve()
    fixture = Path(os.environ['SWAP_CONTRACT_BIN']).resolve()
    expected = 'd04cd8211e711af989a7a62d0b8b55a8cfe496694392518da0ccb848469b3799'
    if hashlib.sha256(node.read_bytes()).hexdigest() != expected:
        raise RuntimeError('Unexpected XBT Knots binary')
    with socket.socket() as s:
        s.bind(('127.0.0.1', 0))
        port = s.getsockname()[1]
    with tempfile.TemporaryDirectory(prefix='paperclip-swap-contract-') as directory:
        root = Path(directory)
        with (root / 'node.log').open('w') as log:
            process = subprocess.Popen([str(node), '-regtest', f'-datadir={root}', '-server',
                f'-rpcport={port}', '-rpcbind=127.0.0.1', '-listen=0', '-connect=0',
                '-dnsseed=0', '-discover=0', '-listenonion=0', '-natpmp=0', '-upnp=0',
                '-testactivationheight=blake2b@100', '-acceptnonstdtxn=0', '-mempooltruc=reject',
                '-fallbackfee=0.00002', '-dbcache=64', '-par=1'], stdout=log, stderr=log)

            def rpc(method, *params, wallet=False):
                cookie = (root / 'regtest/.cookie').read_bytes().strip()
                url = f'http://127.0.0.1:{port}' + ('/wallet/fixture' if wallet else '')
                request = urllib.request.Request(url, json.dumps({'id': 1, 'method': method,
                    'params': params}).encode(), headers={'Authorization': 'Basic ' +
                    base64.b64encode(cookie).decode(), 'Content-Type': 'application/json'})
                with urllib.request.urlopen(request, timeout=15) as response:
                    result = json.load(response)
                if result.get('error'): raise RuntimeError(result['error'])
                return result['result']

            def tool(*args):
                return subprocess.check_output([str(fixture), *map(str, args)], text=True).strip()

            try:
                for _ in range(100):
                    if process.poll() is not None: raise RuntimeError('test node exited')
                    try:
                        if rpc('getblockchaininfo')['chain'] == 'regtest': break
                    except (OSError, ValueError): pass
                    time.sleep(0.1)
                else: raise RuntimeError('test node did not start')
                rpc('createwallet', 'fixture')
                miner = rpc('getnewaddress', wallet=True)
                rpc('generatetoaddress', 201, miner)
                height = rpc('getblockcount')
                deadline = height + 20
                address = tool('address', deadline, 'destination')
                destination = rpc('getnewaddress', wallet=True)
                outpoints = []
                for _ in range(3):
                    txid = rpc('sendtoaddress', address, 0.0005, wallet=True)
                    tx = rpc('decoderawtransaction', rpc('gettransaction', txid, wallet=True)['hex'])
                    output = next(x for x in tx['vout'] if x['scriptPubKey'].get('address') == address)
                    outpoints.append((txid, output['n']))
                rpc('generatetoaddress', 1, miner)

                def spend(index, mode, lock=0):
                    return tool('spend', deadline, *outpoints[index], 50_000,
                                destination, mode, lock, 'PUBLIC-REGTEST-KEYS', 'destination')

                def check(raw, allowed):
                    result = rpc('testmempoolaccept', [raw])[0]
                    if result['allowed'] != allowed: raise AssertionError(result)
                    return result

                checks = {}
                for mode in ('wrong-secret', 'wrong-key'):
                    checks[mode] = check(spend(0, mode), False).get('reject-reason')
                success = spend(0, 'success')
                check(success, True)
                claim_txid = rpc('sendrawtransaction', success)
                rpc('generatetoaddress', 1, miner)
                assert rpc('gettxout', claim_txid, 0)['confirmations'] == 1
                checks['success_confirmed'] = True
                refund = spend(1, 'refund', deadline)
                checks['early_refund'] = check(refund, False).get('reject-reason')
                rpc('generatetoaddress', deadline-rpc('getblockcount'), miner)
                # Even with CLTV now mature, a confirmed claim cannot also refund.
                checks['refund_after_claim_rejected'] = not check(spend(0, 'refund', deadline), False)['allowed']
                check(refund, True)
                refund_txid = rpc('sendrawtransaction', refund)
                rpc('generatetoaddress', 1, miner)
                assert rpc('gettxout', refund_txid, 0)['confirmations'] == 1
                checks['refund_confirmed'] = True
                # Expiry enables refund; it does not disable the success branch.
                late = spend(2, 'success')
                check(late, True)
                late_txid = rpc('sendrawtransaction', late)
                rpc('generatetoaddress', 1, miner)
                assert rpc('gettxout', late_txid, 0)['confirmations'] == 1
                checks['late_success_confirmed'] = True
                checks['double_spend_rejected'] = not rpc('testmempoolaccept', [spend(1, 'success')])[0]['allowed']
                assert checks['double_spend_rejected']
                # Two linked contracts, distinct Alice/provider/Bob keys.
                # These are directly funded UTXOs, not two Ark servers.
                now = rpc('getblockcount')
                pair = {}
                for side, expiry in [('source', now+40), ('destination', now+20)]:
                    addr = tool('address', expiry, side)
                    txid = rpc('sendtoaddress', addr, 0.0005, wallet=True)
                    tx = rpc('decoderawtransaction', rpc('gettransaction', txid, wallet=True)['hex'])
                    vout = next(x['n'] for x in tx['vout'] if x['scriptPubKey'].get('address') == addr)
                    pair[side] = (expiry, txid, vout)
                rpc('generatetoaddress', 1, miner)
                for side in ('destination', 'source'):
                    raw = tool('spend', *pair[side], 50_000, destination, 'success', 0,
                               'PUBLIC-REGTEST-KEYS', side)
                    check(raw, True)
                    decoded = rpc('decoderawtransaction', raw)
                    revealed = bytes.fromhex(decoded['vin'][0]['txinwitness'][1])
                    assert hashlib.sha256(revealed).digest() == hashlib.sha256(bytes([42])*32).digest()
                    txid = rpc('sendrawtransaction', raw)
                    rpc('generatetoaddress', 1, miner)
                    assert rpc('gettxout', txid, 0)['confirmations'] == 1
                checks['two_linked_contracts_confirmed'] = True
                # Candidate cooperative policy: both participant and ASP sign.
                # Recovery needs no ASP signature, but enforces CSV and CLTV.
                expiry = rpc('getblockcount') + 12
                candidate_address = tool('candidate-address', expiry, 'source', 3)
                candidates = []
                for _ in range(4):
                    txid = rpc('sendtoaddress', candidate_address, 0.0005, wallet=True)
                    tx = rpc('decoderawtransaction', rpc('gettransaction', txid, wallet=True)['hex'])
                    vout = next(x['n'] for x in tx['vout'] if x['scriptPubKey'].get('address') == candidate_address)
                    candidates.append((txid, vout))
                rpc('generatetoaddress', 1, miner)

                def candidate(index, mode):
                    lock = expiry if mode in ('refund', 'recover-refund') else 0
                    return tool('candidate-spend', expiry, *candidates[index], 50_000,
                                destination, mode, lock, 'PUBLIC-REGTEST-KEYS', 'source', 3)

                for mode in ('wrong-secret', 'wrong-key', 'wrong-server'):
                    checks['candidate_' + mode] = check(candidate(0, mode), False).get('reject-reason')
                check(candidate(0, 'success'), True)
                checks['candidate_recovery_immature'] = check(candidate(1, 'recover-claim'), False).get('reject-reason')
                check(candidate(2, 'refund'), False)
                rpc('sendrawtransaction', candidate(0, 'success'))
                rpc('generatetoaddress', 2, miner)
                check(candidate(1, 'recover-claim'), True)
                rpc('sendrawtransaction', candidate(1, 'recover-claim'))
                rpc('generatetoaddress', expiry-rpc('getblockcount'), miner)
                check(candidate(0, 'refund'), False)
                check(candidate(1, 'recover-refund'), False)
                for index, mode in [(2, 'refund'), (3, 'recover-refund')]:
                    raw = candidate(index, mode)
                    check(raw, True)
                    txid = rpc('sendrawtransaction', raw)
                    rpc('generatetoaddress', 1, miner)
                    assert rpc('gettxout', txid, 0)['confirmations'] == 1
                    check(candidate(index, 'success'), False)
                checks['candidate_all_four_paths_and_conflicts'] = True
                # Synthetic delayed ancestry: root -> parent -> claim. This
                # models delay accumulation, NOT the real Ark checkpoint graph.
                csv = 3
                expiry = rpc('getblockcount') + 30
                delayed_address = tool('address', expiry, 'source', csv)
                root_txid = rpc('sendtoaddress', delayed_address, 0.0006, wallet=True)
                root_tx = rpc('decoderawtransaction', rpc('gettransaction', root_txid, wallet=True)['hex'])
                root_vout = next(x['n'] for x in root_tx['vout'] if x['scriptPubKey'].get('address') == delayed_address)
                rpc('generatetoaddress', 1, miner)
                parent = tool('spend', expiry, root_txid, root_vout, 60_000,
                              delayed_address, 'success', 0, 'PUBLIC-REGTEST-KEYS', 'source', csv)
                checks['immature_parent'] = check(parent, False).get('reject-reason')
                rpc('generatetoaddress', csv-1, miner)
                check(parent, True)
                parent_txid = rpc('sendrawtransaction', parent)
                child = tool('spend', expiry, parent_txid, 0, 59_000,
                             destination, 'success', 0, 'PUBLIC-REGTEST-KEYS', 'source', csv)
                checks['unconfirmed_ancestor'] = check(child, False).get('reject-reason')
                rpc('generatetoaddress', 1, miner)
                checks['immature_child'] = check(child, False).get('reject-reason')
                # Persist the exact signed recovery transaction, restart the
                # isolated node, then recover without constructing another spend.
                saved = root / 'recovery-child.hex'
                saved.write_text(child)
                restart_command = process.args
                rpc('stop')
                process.wait(timeout=20)
                process = subprocess.Popen(restart_command, stdout=log, stderr=log)
                for _ in range(100):
                    if process.poll() is not None: raise RuntimeError('restart failed')
                    try:
                        if rpc('getblockchaininfo')['chain'] == 'regtest': break
                    except (OSError, ValueError): pass
                    time.sleep(0.1)
                else: raise RuntimeError('restart timed out')
                child = saved.read_text()
                checks['restart_retains_immaturity'] = check(child, False).get('reject-reason')
                blocks = rpc('generatetoaddress', csv-1, miner)
                check(child, True)
                # A one-block reorg can make a previously acceptable exit immature.
                rpc('invalidateblock', blocks[-1])
                checks['reorg_maturity_rechecked'] = check(child, False).get('reject-reason')
                rpc('reconsiderblock', blocks[-1])
                check(child, True)
                child_txid = rpc('sendrawtransaction', child)
                rpc('generatetoaddress', 1, miner)
                assert rpc('gettxout', child_txid, 0)['confirmations'] == 1
                checks['delayed_ancestry_claim_confirmed'] = True
                print(json.dumps({'scope': 'HTLC leaves and synthetic delayed ancestry, not Ark recovery', 'checks': checks}, indent=2))
            except Exception:
                print((root / 'node.log').read_text()[-4000:])
                raise
            finally:
                if process.poll() is None:
                    process.terminate()
                    try: process.wait(timeout=20)
                    except subprocess.TimeoutExpired:
                        process.kill()
                        process.wait()


if __name__ == '__main__':
    main()
