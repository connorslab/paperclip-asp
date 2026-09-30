#!/usr/bin/env python3
"""Create private configs for a new test install, without changing the selected node."""
import argparse
import os
from pathlib import Path
import secrets
import shlex
import subprocess
import sys
import tempfile

parser = argparse.ArgumentParser(description=__doc__)
parser.add_argument('--app-data', type=Path, required=True)
parser.add_argument('--node-env', type=Path, required=True)
parser.add_argument('--rpc-url', required=True)
parser.add_argument('--postgres-host', required=True)
parser.add_argument('--network', choices=['bitcoin', 'regtest'], required=True)
args = parser.parse_args()
os.umask(0o077)
node = {}
for line in args.node_env.read_text().splitlines():
    words = shlex.split(line, comments=True)
    if words and words[0] == 'export':
        words = words[1:]
    if len(words) == 1 and '=' in words[0]:
        key, value = words[0].split('=', 1)
        node[key] = value
user = node.get('APP_BITCOIN_KNOTS_RPC_USER')
password = node.get('APP_BITCOIN_KNOTS_RPC_PASS')
if not user or not password:
    parser.error('Selected node environment must contain its exported Knots RPC credentials')
data = args.app_data / 'data'
for directory in ('asp', 'config', 'postgres', 'secrets'):
    (data / directory).mkdir(mode=0o700, parents=True, exist_ok=True)
if any((data / 'config').iterdir()) or any((data / 'asp').iterdir()) or any((data / 'postgres').iterdir()):
    parser.error('Only a new, empty app installation may be provisioned')
secret_path = data / 'secrets/postgres_password'
with secret_path.open('x') as handle:
    handle.write(secrets.token_hex(32))
with tempfile.TemporaryDirectory(prefix='paperclip-provision-') as directory:
    rpc_password = Path(directory) / 'rpc-password'
    rpc_password.write_text(password)
    for watchman in (False, True):
        command = [sys.executable, str(Path(__file__).with_name('prepare-config.py')),
            '--network', args.network, '--container', '--rpc-url', args.rpc_url,
            '--rpc-user', user, '--rpc-password-file', str(rpc_password),
            '--postgres-host', args.postgres_host, '--postgres-password-file', str(secret_path),
            '--output', str(data / 'config' / ('watchman.json' if watchman else 'asp.json'))]
        if watchman:
            command.append('--watchman')
        subprocess.run(command, check=True)
print('New private test-app configuration prepared. Node configuration and funds were not changed.')
