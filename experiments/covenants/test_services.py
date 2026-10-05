#!/usr/bin/env python3
"""Native scheduler/watchman lifecycle test on a fresh private chain, without txindex."""
import json
import os
from pathlib import Path
import secrets
import subprocess
from urllib.parse import urlsplit

from test_framework.test_framework import BitcoinTestFramework
from test_framework.wallet import MiniWallet
from test_framework.script import CScript

PROFILE = 'paperclip-signet-offline-refresh-v1'
CHALLENGE = '2102396d38e3ff703be31a2d97317835f4e9645b5ae5ea2d2d1ba406afa7ab185b5fac'


class ServiceTest(BitcoinTestFramework):
    def add_options(self, parser):
        self.add_wallet_options(parser)

    def skip_test_if_missing_module(self):
        self.skip_if_no_wallet()

    def set_test_params(self):
        self.num_nodes = 1
        self.setup_clean_chain = True
        self.wallet_names = [False]
        self.extra_args = [['-xbtcovtest', '-testactivationheight=blake2b@1', '-rdtsexpiry=2147483647']]

    def run_test(self):
        n = self.nodes[0]
        faucet = MiniWallet(n)
        self.generate(faucet, 105)
        root = Path(self.options.tmpdir) / 'services'
        root.mkdir(mode=0o700)
        procs = []
        evidence = {'network': 'private-regtest', 'checks': [], 'transactions': []}
        bins = {r: os.environ['COVENANT_' + r.upper()] for r in ('asp', 'wallet', 'watchman')}

        def lab(role, command, **kw):
            p = subprocess.run([bins[role], 'covenant-lab', '--experimental-signet'],
                input=json.dumps(dict(profile=PROFILE, challenge=CHALLENGE, command=command, **kw)),
                text=True, capture_output=True)
            assert p.returncode == 0, p.stderr
            return json.loads(p.stdout)

        def write(name, obj):
            path = root / name
            with open(path, 'w', opener=lambda p, f: os.open(p, f, 0o600)) as f:
                f.write(obj if isinstance(obj, str) else json.dumps(obj))
            return path

        def stop(p):
            if p.poll() is None:
                p.terminate()
                p.wait(timeout=15)

        def check(name):
            evidence['checks'].append(name)
            self.log.info(name)

        try:
            keys = [secrets.token_hex(32) for _ in range(4)]
            pubs = [lab('wallet', 'pubkey', secret=k)['pubkey'] for k in keys]
            write('keys.json', keys)
            keyfile = write('server.key', keys[3])
            states = [dict(owner=pubs[i], server=pubs[3], amount_sat=100000-i*1000,
                           expiry=180+i*40, exit_delay=6) for i in range(3)]
            permits = [lab('wallet', 'authorize', old=states[i], new=states[i+1],
                          not_before=115+i*15, old_secret=keys[i], new_secret=keys[i+1]) for i in range(2)]
            info = lab('asp', 'round', state=states[0])
            original = faucet.send_to(from_node=n, scriptPubKey=CScript(bytes.fromhex(info['script_pubkey'])), amount=info['funding_sat'])
            n.createwallet('covenant-test-service')
            w = n.get_wallet_rpc('covenant-test-service')
            address = w.getnewaddress()
            faucet.send_to(from_node=n, scriptPubKey=CScript(bytes.fromhex(n.validateaddress(address)['scriptPubKey'])), amount=1000000)
            self.generate(faucet, 1)
            funding = original['txid'] + ':' + str(original['sent_vout'])
            unroll = lab('asp', 'round', state=states[0], funding=funding)
            url = urlsplit(n.url)
            config = dict(experimental_test_only=True, network='regtest', state_dir=str(root/'state'),
                rpc_url=f'http://127.0.0.1:{url.port}/', cookie_file=str(n.datadir_path/'regtest/.cookie'),
                funding_wallet='covenant-test-service', server_key_file=str(keyfile), max_funding_sat=500000, poll_ms=200)
            cfg = write('config.json', config)
            asp_cfg = write('asp.json', dict(config, test_failpoint='after-funding-journal'))
            watch_cfg = write('watchman.json', dict(config, test_failpoint='after-refund-journal'))
            req = write('enroll.json', dict(funding=funding, permits=permits))

            def enroll(path=req, c=cfg):
                return subprocess.run([bins['asp'], 'covenant-enroll', '--service-config', str(c), '--request', str(path)], capture_output=True, text=True)

            bad = dict(config, network='main')
            assert enroll(c=write('bad-config.json', bad)).returncode != 0
            for _ in range(2):
                result = enroll()
                assert result.returncode == 0, result.stderr
            bad_request = dict(funding=funding, permits=[dict(permits[0], binding='00'*32)])
            assert enroll(write('bad-enroll.json', bad_request)).returncode != 0
            check('mainnet guard, invalid permit rejection, and idempotent enrollment')

            def start(role, config_path):
                log = open(root/(role+'-'+str(len(procs))+'.log'), 'w')
                p = subprocess.Popen([bins[role], 'covenant-service', '--service-config', str(config_path)], stdout=log, stderr=log)
                log.close()
                procs.append(p)
                return p

            def journal():
                return json.loads((root/'state/journal.json').read_text())

            def steps():
                return journal()['jobs'][0]['steps']

            def txid(raw):
                return n.decoderawtransaction(raw)['txid']

            def wait(fn):
                self.wait_until(fn, timeout=90)

            def mine_to(h):
                if n.getblockcount() < h:
                    self.generate(faucet, h-n.getblockcount())

            asp = start('asp', asp_cfg)
            watch = start('watchman', watch_cfg)
            mine_to(115)
            wait(lambda: asp.poll() is not None)
            assert asp.returncode == 86
            prepared = steps()[0]['funding_hex']
            assert txid(prepared) not in n.getrawmempool()
            asp = start('asp', asp_cfg)
            duplicate = start('asp', cfg)
            wait(lambda: txid(prepared) in n.getrawmempool())
            self.generate(faucet, 1)
            wait(lambda: steps()[0]['funding_confirmations'] >= 1)
            assert len(steps()) == 1 and steps()[0]['funding_hex'] == prepared
            check('funding crash replay and concurrent scheduler deduplication')
            mine_to(130)
            wait(lambda: len(steps()) == 2 and txid(steps()[1]['funding_hex']) in n.getrawmempool())
            self.generate(faucet, 1)
            wait(lambda: steps()[1]['funding_confirmations'] >= 1)
            assert all(s['refund_hex'] is None for s in steps())
            check('two independently scheduled refreshes while wallet is offline; watchman waits for stale exit')
            stop(asp)
            stop(duplicate)
            n.sendrawtransaction(unroll['hex'])
            wait(lambda: watch.poll() is not None)
            assert watch.returncode == 86
            refund0 = steps()[0]['refund_hex']
            assert txid(refund0) not in n.getrawmempool()
            watch = start('watchman', watch_cfg)
            wait(lambda: steps()[1]['refund_hex'] is not None and txid(steps()[1]['refund_hex']) in n.getrawmempool())
            assert steps()[0]['refund_hex'] == refund0
            block = self.generate(faucet, 1)[0]
            wait(lambda: steps()[1]['refund_confirmations'] >= 1)
            check('watchman detects stale exit and completes both refunds with ASP stopped; crash replay preserves signatures')
            snapshot = [(s['funding_hex'], s['refund_hex']) for s in steps()]
            n.invalidateblock(block)
            wait(lambda: steps()[1]['refund_confirmations'] == 0)
            n.setmocktime(n.getblockheader(block)['time'] + 1)
            self.generate(faucet, 2)
            wait(lambda: steps()[1]['refund_confirmations'] >= 1)
            assert snapshot == [(s['funding_hex'], s['refund_hex']) for s in steps()]
            check('reorg reconfirmation does not issue replacement funding or refunds')
            stop(watch)
            recovery = json.loads((root/'state/recovery.json').read_text())
            latest = recovery['jobs'][0]['steps'][-1]
            claim = lab('wallet', 'claim', state=states[-1], outpoint=txid(latest['refund_hex'])+':0',
                        destination=faucet.get_output_script().hex(), secret=keys[2])
            assert not n.testmempoolaccept([claim['hex']])[0]['allowed']
            mine_to(states[-1]['expiry']+states[-1]['exit_delay'])
            assert n.testmempoolaccept([claim['hex']])[0]['allowed']
            claim_id = n.sendrawtransaction(claim['hex'])
            self.generate(faucet, 1)
            assert n.gettxout(claim_id, 0) is not None
            check('wallet recovers latest allocation after timelock with both services stopped')
            evidence['transactions'] = [dict(kind='recovery', txid=claim_id)]
            for s in steps():
                for name in ('funding_hex', 'unroll_hex', 'refund_hex'):
                    evidence['transactions'].append(dict(kind=name, txid=txid(s[name])))
            evidence['txindex'] = n.getindexinfo()
            evidence['passed'] = True
            Path(os.environ['COVENANT_RESULTS']).write_text(json.dumps(evidence, indent=2)+'\n')
        finally:
            for p in procs:
                stop(p)


if __name__ == '__main__':
    ServiceTest(__file__).main()
