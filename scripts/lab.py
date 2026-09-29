#!/usr/bin/env python3
"""Private interactive regtest lab. Never uses an existing production datadir."""
import argparse
import base64
import fcntl
import getpass
import hashlib
import json
import os
from pathlib import Path
import signal
import socket
import subprocess
import time
import tomllib
import urllib.request

ROOT = Path(__file__).resolve().parents[1]
STATE = ROOT / '.state' / 'lab'
RPC_PORT, ASP_PORT, ADMIN_PORT, WEB_PORT = 38443, 38535, 38536, 38180
EXPECTED_KNOTS = 'd04cd8211e711af989a7a62d0b8b55a8cfe496694392518da0ccb8488469b3799'
CHILDREN = []

def run(args, capture=False):
    with (STATE / 'commands.log').open('a') as log:
        result = subprocess.run(list(map(str, args)), stdout=subprocess.PIPE if capture else log,
                                stderr=log, text=True, timeout=120, check=True)
    return json.loads(result.stdout) if capture else None

def start(args, name):
    log = (STATE / (name + '.log')).open('a')
    proc = subprocess.Popen(list(map(str, args)), stdout=log, stderr=subprocess.STDOUT)
    log.close()
    CHILDREN.append(proc)
    return proc

def wait(check):
    for _ in range(120):
        try:
            value = check()
            if value: return value
        except Exception:
            pass
        if any(p.poll() is not None for p in CHILDREN):
            raise RuntimeError('A test service exited. Inspect private .state/lab logs.')
        time.sleep(0.5)
    raise RuntimeError('Private service did not become ready within 60 seconds')

def rpc(method, *params, wallet=False):
    cookie = (STATE / 'chain/regtest/.cookie').read_bytes().strip()
    request = urllib.request.Request(f'http://127.0.0.1:{RPC_PORT}/' + ('wallet/faucet' if wallet else ''),
        json.dumps({'id': 1, 'method': method, 'params': params}).encode(),
        {'Content-Type': 'application/json', 'Authorization': 'Basic ' + base64.b64encode(cookie).decode()})
    with urllib.request.urlopen(request, timeout=30) as response:
        result = json.load(response)
    if result.get('error'): raise RuntimeError(result['error']['message'])
    return result['result']

def mine(count):
    assert rpc('getblockchaininfo')['chain'] == 'regtest'
    return rpc('generatetoaddress', count, (STATE / 'miner-address').read_text().strip())

def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--mine', type=int, help='Mine 1–500 private test blocks while the lab is running')
    parser.add_argument('--stop', action='store_true', help='Ask the running lab to stop cleanly')
    args = parser.parse_args()
    os.umask(0o077)
    STATE.mkdir(parents=True, exist_ok=True, mode=0o700)
    if args.mine is not None or args.stop:
        if (STATE / 'identity').read_text() != 'paperclip-private-regtest-v1':
            raise RuntimeError('Not a Paperclip private test directory')
        if args.stop: (STATE / 'stop').touch(); return
        if not 1 <= args.mine <= 500: raise ValueError('Choose 1–500 blocks')
        print(json.dumps(mine(args.mine))); return
    if not os.environ.get('IN_NIX_SHELL'): raise RuntimeError('Run inside nix develop')
    lock = (STATE / 'lock').open('w')
    fcntl.flock(lock, fcntl.LOCK_EX | fcntl.LOCK_NB)
    if any(STATE.iterdir()) and not (STATE / 'identity').exists():
        if set(p.name for p in STATE.iterdir()) != {'lock'}:
            raise RuntimeError('Refusing unrecognized pre-existing state')
    (STATE / 'identity').write_text('paperclip-private-regtest-v1')
    (STATE / 'stop').unlink(missing_ok=True)
    node = Path(os.environ['XBT_BITCOIND']).resolve()
    asp = Path(os.environ['PAPERCLIP_ASP_BIN']).resolve()
    wallet = Path(os.environ['PAPERCLIP_WALLET_BIN']).resolve()
    walletd = Path(os.environ['PAPERCLIP_WALLETD_BIN']).resolve()
    if hashlib.sha256(node.read_bytes()).hexdigest() != EXPECTED_KNOTS:
        raise RuntimeError('Knots binary hash mismatch')
    for port in [RPC_PORT, ASP_PORT, ADMIN_PORT, WEB_PORT]:
        with socket.socket() as probe: probe.bind(('127.0.0.1', port))
    for folder in ['chain', 'pgsocket']: (STATE / folder).mkdir(exist_ok=True)
    pg = STATE / 'postgres'
    if not pg.exists(): run(['initdb', '-D', pg, '--auth-local=trust', '--auth-host=reject', '--no-locale'])
    # Only a private Unix socket is exposed. No PostgreSQL TCP listener.
    start(['postgres', '-D', pg, '-k', STATE / 'pgsocket', '-h', '', '-p', '38432'], 'postgres')
    wait(lambda: subprocess.run(['pg_isready', '-h', str(STATE / 'pgsocket'), '-p', '38432'],
                               stdout=subprocess.DEVNULL, stderr=subprocess.DEVNULL).returncode == 0)
    if not (STATE / 'database-created').exists():
        run(['createdb', '-h', STATE / 'pgsocket', '-p', '38432', 'paperclip_lab'])
        (STATE / 'database-created').touch()
    start([node, '-regtest', f'-datadir={STATE / "chain"}', '-server', f'-rpcport={RPC_PORT}',
        '-rpcbind=127.0.0.1', '-listen=0', '-connect=0', '-dnsseed=0', '-discover=0',
        '-listenonion=0', '-natpmp=0', '-upnp=0', '-testactivationheight=blake2b@100',
        '-acceptnonstdtxn=0', '-mempooltruc=enforce', '-subdustfeepenalty=0',
        '-fallbackfee=0.00002', '-dbcache=64', '-par=1'], 'knots')
    wait(lambda: rpc('getblockchaininfo')['chain'] == 'regtest')
    if 'faucet' not in rpc('listwallets'):
        if any(w['name'] == 'faucet' for w in rpc('listwalletdir')['wallets']): rpc('loadwallet', 'faucet')
        else: rpc('createwallet', 'faucet')
    if not (STATE / 'miner-address').exists():
        (STATE / 'miner-address').write_text(rpc('getnewaddress', wallet=True))
    if rpc('getblockcount') < 201: mine(201 - rpc('getblockcount'))
    config = tomllib.loads((ROOT / 'server/captaind.default.toml').read_text())
    config.update(data_dir=str(STATE / 'asp'), network='regtest', vtxo_exit_delta=12,
                  round_interval='10s', handshake_psa='Paperclip private regtest: no real funds.',
                  max_ln_send_amount='0 sat', max_ln_receive_amount='0 sat', cln_array=[])
    config['rpc'] = {'public_address': f'127.0.0.1:{ASP_PORT}', 'admin_address': f'127.0.0.1:{ADMIN_PORT}'}
    config['postgres'].update(host=str(STATE / 'pgsocket'), port=38432, name='paperclip_lab', user=getpass.getuser())
    config['bitcoind'] = {'url': f'http://127.0.0.1:{RPC_PORT}', 'cookie': str(STATE / 'chain/regtest/.cookie')}
    config['vtxopool']['vtxo_targets'] = []
    config['otel_tracing_sampler'] = 0.0
    config['otel_deployment_name'] = 'paperclip-private-regtest'
    config_path = STATE / 'asp.json'
    config_path.write_text(json.dumps(config, indent=2))
    if not (STATE / 'asp-created').exists():
        run([asp, '--config', config_path, 'create'])
        (STATE / 'asp-created').touch()
    start([asp, '--config', config_path, 'start'], 'asp')
    status = wait(lambda: run([asp, 'rpc', '--addr', f'127.0.0.1:{ADMIN_PORT}', 'wallet'], True))
    if not (STATE / 'funded').exists():
        rpc('sendtoaddress', status['rounds']['address'], 10, wallet=True); mine(3)
        for name in ['alice', 'bob']:
            path = STATE / name
            run([wallet, '--datadir', path, 'create', '--regtest', '--ark', f'http://127.0.0.1:{ASP_PORT}',
                 '--bitcoind', f'http://127.0.0.1:{RPC_PORT}', '--bitcoind-cookie', STATE / 'chain/regtest/.cookie'])
            address = run([wallet, '--datadir', path, 'onchain', 'address'], True)['address']
            rpc('sendtoaddress', address, 0.02, wallet=True); mine(3)
            run([wallet, '--datadir', path, 'board', '1000000sat']); mine(3)
            run([wallet, '--datadir', path, 'sync'])
        (STATE / 'funded').touch()
    start([walletd, '--datadir', STATE / 'alice', '--host', '127.0.0.1', '--port', str(WEB_PORT)], 'wallet-api')
    print(f'Paperclip test wallet: http://127.0.0.1:{WEB_PORT}', flush=True)
    print(f'Token stays in {STATE / "alice/auth_token"}. Do not share it.', flush=True)
    print('Mine blocks explicitly with scripts/lab.py --mine N. Stop with --stop.', flush=True)
    while not (STATE / 'stop').exists():
        if any(p.poll() is not None for p in CHILDREN): raise RuntimeError('Test service exited; inspect private logs')
        time.sleep(1)

if __name__ == '__main__':
    signal.signal(signal.SIGTERM, lambda *_: (_ for _ in ()).throw(KeyboardInterrupt()))
    try:
        main()
    finally:
        for child in reversed(CHILDREN):
            if child.poll() is None:
                child.terminate()
                try: child.wait(timeout=20)
                except subprocess.TimeoutExpired: child.kill(); child.wait()
