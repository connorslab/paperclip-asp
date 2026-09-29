#!/usr/bin/env python3
"""Exercise the authenticated API using only the private lab's disposable wallets."""
import json
import os
from pathlib import Path
import subprocess
import urllib.request
import urllib.error

root = Path(__file__).resolve().parents[1] / '.state/lab'
assert (root / 'identity').read_text() == 'paperclip-private-regtest-v1'
token = (root / 'alice/auth_token').read_text().strip()
wallet = os.environ['PAPERCLIP_WALLET_BIN']

def api(path, body=None, auth=token):
    headers = {'Content-Type': 'application/json'}
    if auth: headers['Authorization'] = 'Bearer ' + auth
    req = urllib.request.Request('http://127.0.0.1:38180/api/v1/' + path,
        None if body is None else json.dumps(body).encode(), headers)
    try:
        with urllib.request.urlopen(req, timeout=60) as response:
            return response.status, json.load(response)
    except urllib.error.HTTPError as error:
        return error.code, None

def bob(*args):
    result = subprocess.run([wallet, '--datadir', str(root / 'bob'), *args],
        check=True, capture_output=True, text=True, timeout=60)
    if args == ('address',): return result.stdout.strip()
    return json.loads(result.stdout) if result.stdout.strip() else None

assert api('wallet/balance', auth=None)[0] == 401
assert api('wallet/balance', auth='invalid')[0] == 401
assert api('wallet/mnemonic')[0] == 404
assert api('wallet/balance')[0] == 200
assert token not in (root / 'wallet-api.log').read_text()
before = bob('balance')['spendable_sat']
destination = bob('address')
code, result = api('wallet/send', {'destination': destination, 'amount_sat': 10000})
assert code == 200, (code, result)
bob('maintain')
after = bob('balance')['spendable_sat']
assert after == before + 10000, (before, after)
assert api('wallet/addresses/next', {})[0] == 200
report = {'missing_token': 401, 'invalid_token': 401, 'mnemonic_disabled': 404,
          'authenticated_balance': 200, 'token_absent_from_logs': True,
          'ark_transfer_received_sat': after - before, 'receive_address': 'PASS'}
(root / 'api-test-report.json').write_text(json.dumps(report, indent=2))
print(json.dumps(report))
