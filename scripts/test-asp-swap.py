"""Two real isolated ASP processes, separate PostgreSQL DBs, XBT regtest only."""
import base64
from concurrent.futures import ThreadPoolExecutor
import getpass
import hashlib
import json
import os
from pathlib import Path
import socket
import subprocess
import tempfile
import time
import tomllib
import urllib.request

ROOT = Path(__file__).resolve().parents[1]


def main():
    if not os.environ.get('IN_NIX_SHELL'):
        raise RuntimeError('Run through just int-asp-swap inside nix develop')
    node = Path(os.environ['XBT_BITCOIND']).resolve()
    if hashlib.sha256(node.read_bytes()).hexdigest() != 'd04cd8211e711af989a7a62d0b8b55a8cfe496694392518da0ccb848469b3799':
        raise RuntimeError('Unexpected XBT Knots binary')
    target = Path(os.environ.get('CARGO_TARGET_DIR', ROOT / 'target')).resolve()
    asp, client = target / 'debug/paperclip-asp', target / 'debug/examples/swap_client'
    os.umask(0o077)
    state = Path(tempfile.mkdtemp(prefix='paperclip-asp-swap-', dir='/tmp'))
    children = []
    env = {k: v for k, v in os.environ.items() if not k.startswith(('BARK_', 'BARKD_'))}

    def port():
        with socket.socket() as s:
            s.bind(('127.0.0.1', 0))
            return s.getsockname()[1]

    def start(args, name):
        with (state / (name + '.log')).open('a') as log:
            process = subprocess.Popen(list(map(str, args)), stdout=log, stderr=log, env=env)
        children.append(process)
        return process

    def run(args):
        return subprocess.run(list(map(str, args)), capture_output=True, text=True, timeout=90, env=env)

    def checked(args):
        result = run(args)
        if result.returncode: raise RuntimeError(result.stderr[-4000:])
        return result.stdout

    def wait(check):
        last = None
        for _ in range(120):
            try:
                value = check()
                if value: return value
            except Exception as e: last = e
            time.sleep(0.25)
        raise RuntimeError(f'Service wait failed: {last}')

    rpc_port, pg_port = port(), port()

    def rpc(method, *params, wallet=False):
        cookie = (state / 'chain/regtest/.cookie').read_bytes().strip()
        request = urllib.request.Request(f'http://127.0.0.1:{rpc_port}/' + ('wallet/fixture' if wallet else ''),
            json.dumps({'id': 1, 'method': method, 'params': params}).encode(),
            {'Content-Type': 'application/json', 'Authorization': 'Basic ' + base64.b64encode(cookie).decode()})
        with urllib.request.urlopen(request, timeout=15) as response: result = json.load(response)
        if result.get('error'): raise RuntimeError(result['error'])
        return result['result']

    def mine(n):
        assert rpc('getblockchaininfo')['chain'] == 'regtest'
        rpc('generatetoaddress', n, miner)

    def cli(side, action, name, *args, reject=False):
        command = [client, 'PUBLIC-REGTEST-KEYS', action, endpoints[side], state / name, *args]
        result = run(command)
        if reject:
            assert result.returncode != 0, f'Unexpected acceptance: {action}'
            return result.stderr[-500:]
        if result.returncode: raise RuntimeError(result.stderr[-4000:])
        return json.loads(result.stdout)

    configs, servers, endpoints = {}, {}, {}
    try:
        (state / 'chain').mkdir(); (state / 'pgsocket').mkdir()
        checked(['initdb', '-D', state / 'postgres', '--auth-local=trust', '--auth-host=reject', '--no-locale'])
        start(['postgres', '-D', state / 'postgres', '-k', state / 'pgsocket', '-h', '', '-p', pg_port], 'postgres')
        wait(lambda: run(['pg_isready', '-h', state / 'pgsocket', '-p', pg_port]).returncode == 0)
        start([node, '-regtest', f'-datadir={state / "chain"}', '-server', f'-rpcport={rpc_port}',
            '-rpcbind=127.0.0.1', '-listen=0', '-connect=0', '-dnsseed=0', '-discover=0',
            '-listenonion=0', '-natpmp=0', '-upnp=0', '-testactivationheight=blake2b@100',
            '-acceptnonstdtxn=0', '-mempooltruc=reject', '-fallbackfee=0.00002',
            '-dbcache=64', '-par=1', '-txindex=1'], 'knots')
        wait(lambda: rpc('getblockchaininfo')['chain'] == 'regtest')
        rpc('createwallet', 'fixture'); miner = rpc('getnewaddress', wallet=True); mine(201)
        for side in ('a', 'b'):
            checked(['createdb', '-h', state / 'pgsocket', '-p', pg_port, 'swap_' + side])
            config = tomllib.loads((ROOT / 'server/captaind.default.toml').read_text())
            public, admin = port(), port()
            config.update(data_dir=str(state / ('asp_' + side)), network='regtest', vtxo_exit_delta=12,
                round_interval='600s', experimental_swaps=True, cln_array=[],
                max_ln_send_amount='0 sat', max_ln_receive_amount='0 sat')
            config['rpc'] = {'public_address': f'127.0.0.1:{public}', 'admin_address': f'127.0.0.1:{admin}'}
            config['postgres'].update(host=str(state / 'pgsocket'), port=pg_port, name='swap_' + side, user=getpass.getuser())
            config['bitcoind'] = {'url': f'http://127.0.0.1:{rpc_port}', 'cookie': str(state / 'chain/regtest/.cookie')}
            config['vtxopool']['vtxo_targets'] = []
            config['otel_tracing_sampler'] = 0.0
            config['otel_deployment_name'] = 'isolated-asp-swap-' + side
            path = state / (side + '.json'); path.write_text(json.dumps(config)); configs[side] = path
            checked([asp, '--config', path, 'create'])
            servers[side] = start([asp, '--config', path, 'start'], 'asp_' + side)
            endpoints[side] = f'http://127.0.0.1:{public}'
            wait(lambda: run([asp, 'rpc', '--addr', f'127.0.0.1:{admin}', 'wallet']).returncode == 0)

        def board(side, name, key):
            address = cli(side, 'init', name, key, 3000)['address']
            txid = rpc('sendtoaddress', address, 0.005, wallet=True)
            (state / name / 'funding.hex').write_text(rpc('gettransaction', txid, wallet=True)['hex'])
            mine(3)
            return wait(lambda: cli(side, 'board', name))

        report = {'scope': 'two captaind processes, separate databases, funded XBT regtest VTXOs'}
        report['source_board'] = board('a', 'alice', 1)
        report['destination_inventory'] = board('b', 'provider', 2)
        tip = rpc('getblockcount'); source_deadline, destination_deadline = tip + 160, tip + 80
        # Receiver gets 20k. Source pays the provider's exact destination lock
        # and settlement allocations too: no negative provider balance.
        report['destination_lock'] = cli('b', 'lock', 'provider', 3, destination_deadline, 21_330)
        report['source_lock'] = cli('a', 'lock', 'alice', 2, source_deadline, 26_650)
        cli('a', 'lock', 'alice', 2, source_deadline, 26_651, reject=True)
        cli('b', 'refund', 'provider', 2, 2, reject=True)
        # Independent client processes race identical claims; any transient
        # input-in-use response can retry, but successes must have one VTXO ID.
        command = [client, 'PUBLIC-REGTEST-KEYS', 'settle', endpoints['b'], state / 'provider', 3, 3]
        with ThreadPoolExecutor(max_workers=2) as executor:
            attempts = list(executor.map(lambda _: run(command), range(2)))
        accepted = [json.loads(a.stdout) for a in attempts if a.returncode == 0]
        assert accepted and len({a['id'] for a in accepted}) == 1
        report['destination_claim'] = cli('b', 'settle', 'provider', 3, 3)
        assert report['destination_claim'] == accepted[0]
        report['concurrent_duplicate_claims_single_output'] = True
        report['source_claim'] = cli('a', 'settle', 'alice', 2, 2)
        assert report['destination_claim']['settled_sats'] == 20_000
        assert report['source_claim']['settled_sats'] == 25_320
        assert 25_320 - 21_330 - report['destination_lock']['lock_reserve'] == 0
        cli('b', 'settle', 'provider', 3, 4, reject=True)
        # Restart restores the committed spend; an identical retry returns the
        # same VTXO, a different destination still cannot spend it again.
        servers['b'].terminate(); servers['b'].wait(timeout=20)
        servers['b'] = start([asp, '--config', configs['b'], 'start'], 'asp_b')
        retry = wait(lambda: cli('b', 'settle', 'provider', 3, 3))
        assert retry == report['destination_claim']
        cli('b', 'settle', 'provider', 3, 4, reject=True)
        report['recipient_spends_normally'] = cli('b', 'spend', 'provider', 3)
        report['provider_spends_normally'] = cli('a', 'spend', 'alice', 2)
        board('a', 'refund', 6)
        deadline = rpc('getblockcount') + 80
        cli('a', 'lock', 'refund', 7, deadline, 21_330)
        cli('a', 'refund', 'refund', 6, 6, reject=True)
        mine(deadline - rpc('getblockcount'))
        report['refund'] = wait(lambda: cli('a', 'refund', 'refund', 6, 6))
        cli('a', 'settle', 'refund', 7, 7, reject=True)
        report['refunded_funds_spend_normally'] = cli('a', 'spend', 'refund', 6)
        # The destination's deadline has passed; retry still returns the
        # committed claim, including after its resulting VTXO was spent.
        assert cli('b', 'settle', 'provider', 3, 3) == report['destination_claim']
        cli('b', 'refund', 'provider', 2, 2, reject=True)
        report['committed_claim_replay_after_deadline'] = True
        servers['a'].terminate(); servers['a'].wait(timeout=20)
        disabled = json.loads(configs['a'].read_text()); disabled['experimental_swaps'] = False
        configs['a'].write_text(json.dumps(disabled))
        servers['a'] = start([asp, '--config', configs['a'], 'start'], 'asp_a')
        wait(lambda: cli('a', 'init', 'disabled-probe', 8, 3000))
        rejection = cli('a', 'refund', 'refund', 6, 6, reject=True)
        assert 'experimental swaps require explicitly enabled regtest' in rejection
        report['disabled_endpoint_rejected'] = True
        for server in servers.values(): server.terminate(); server.wait(timeout=20)
        # Replay the real recipient ancestry through XBT Knots with both ASPs
        # stopped. No synthetic ancestry or default-policy overrides here.
        recovery = json.loads((state / 'provider/recovery.json').read_text())
        for raw in recovery:
            acceptance = rpc('testmempoolaccept', [raw])[0]
            assert acceptance['allowed'], acceptance
            rpc('sendrawtransaction', raw); mine(1)
        report['actual_recipient_ancestry_confirmed'] = len(recovery)
        report['conflicts_rejected_and_restart_replay_passed'] = True
        (state / 'report.json').write_text(json.dumps(report, indent=2))
        print(json.dumps(report, indent=2)); print('Evidence directory:', state)
    except Exception:
        print('Failure evidence directory:', state)
        for path in state.glob('asp_*.log'): print(path.name, path.read_text()[-3000:])
        raise
    finally:
        for process in reversed(children):
            if process.poll() is None:
                process.terminate()
                try: process.wait(timeout=20)
                except subprocess.TimeoutExpired: process.kill(); process.wait()


if __name__ == '__main__': main()
