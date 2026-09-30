#!/usr/bin/env python3
"""Generate private operator configuration without starting services or moving funds."""
import argparse
import json
import os
from pathlib import Path
import tomllib
from urllib.parse import urlsplit

parser = argparse.ArgumentParser()
parser.add_argument('--network', choices=['regtest', 'bitcoin'], required=True)
parser.add_argument('--rpc-url', required=True)
auth = parser.add_mutually_exclusive_group(required=True)
auth.add_argument('--rpc-cookie', type=Path)
auth.add_argument('--rpc-password-file', type=Path)
parser.add_argument('--rpc-user')
parser.add_argument('--output', type=Path, required=True)
parser.add_argument('--watchman', action='store_true')
parser.add_argument('--sweep-address', help='Watchman recovery destination owned by the operator')
parser.add_argument('--container', action='store_true', help='Allow an explicitly selected private-network RPC endpoint')
parser.add_argument('--postgres-host', default='/var/run/postgresql')
parser.add_argument('--postgres-password-file', type=Path)
parser.add_argument('--data-dir', type=Path, default=Path('/var/lib/paperclip-asp'))
args = parser.parse_args()
url = urlsplit(args.rpc_url)
if url.scheme not in ('http', 'https') or not url.hostname or url.username or url.password or url.query or url.fragment:
    parser.error('RPC URL must be HTTP(S), without embedded credentials, query, or fragment')
if not args.container and not args.rpc_url.startswith('http://127.0.0.1:'):
    parser.error('Use a local Knots RPC endpoint or a loopback SSH tunnel')
if not all(path.is_absolute() for path in (args.rpc_cookie, args.output, args.data_dir,
        args.rpc_password_file, args.postgres_password_file) if path is not None):
    parser.error('Use absolute paths for data, secrets, and output')
if bool(args.rpc_password_file) != bool(args.rpc_user):
    parser.error('RPC username and password file must be supplied together')
if not args.postgres_host.startswith('/') and not args.postgres_password_file:
    parser.error('TCP PostgreSQL requires a password file')

def secret(path):
    value = path.read_text().rstrip('\r\n')
    if not value:
        parser.error('Secret file must not be empty')
    return value

root = Path(__file__).resolve().parents[1]
cfg = tomllib.loads((root / ('server/watchmand.default.toml' if args.watchman else 'server/captaind.default.toml')).read_text())
cfg.update(data_dir=str(args.data_dir), network=args.network,
           otel_deployment_name='paperclip-xbt-watchman' if args.watchman else 'paperclip-xbt-asp', otel_tracing_sampler=0.0)
if not args.watchman:
 cfg.update(
           rpc_rich_errors=False, require_board_funding_tx=True,
           max_ln_send_amount='0 sat', max_ln_receive_amount='0 sat',
           cln_array=[], handshake_psa='Paperclip XBT Ark beta. Back up recovery data and refresh before expiry.',
           otel_deployment_name='paperclip-xbt-asp', otel_tracing_sampler=0.0)
 cfg['rpc'] = {'public_address': '0.0.0.0:3535' if args.container else '127.0.0.1:3535', 'admin_address': '127.0.0.1:3536'}
 cfg['vtxopool']['vtxo_targets'] = []
else:
 cfg['watchman']['incremental_relay_fee'] = '1000 sat/kvb'
 if args.sweep_address:
  cfg['sweep_address'] = args.sweep_address
cfg['postgres'].update(host=args.postgres_host, port=5432, name='paperclip_asp', user='paperclip-asp')
if args.postgres_password_file:
 cfg['postgres']['password'] = secret(args.postgres_password_file)
cfg['bitcoind'] = {'url': args.rpc_url}
if args.rpc_cookie:
 cfg['bitcoind']['cookie'] = str(args.rpc_cookie)
else:
 cfg['bitcoind'].update(rpc_user=args.rpc_user, rpc_pass=secret(args.rpc_password_file))
with os.fdopen(os.open(args.output, os.O_WRONLY | os.O_CREAT | os.O_EXCL, 0o600), 'w') as f:
    json.dump(cfg, f, indent=2)
    f.write('\n')
print('Configuration created. Services have not been started and no funds moved.')
