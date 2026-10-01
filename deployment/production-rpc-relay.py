#!/usr/bin/env python3
"""Loopback RPC relay for the Ark chain index; no wallet or node-management methods."""
import base64
import hmac
import json
from pathlib import Path
import threading
import urllib.error
import urllib.request
from http.server import BaseHTTPRequestHandler, ThreadingHTTPServer

ALLOWED = {'getblockchaininfo', 'getnetworkinfo', 'getblockhash', 'getblock',
           'getblockheader', 'getpeerinfo', 'getblockfrompeer', 'getmempoolinfo',
           'estimatesmartfee', 'sendrawtransaction', 'gettxout', 'getrawmempool', 'getrawtransaction', 'getbestblockhash', 'getblockcount', 'getindexinfo', 'getmempoolentry', 'getmempoolancestors', 'getmempooldescendants', 'gettxspendingprevout', 'estimaterawfee', 'getdeploymentinfo', 'testmempoolaccept', 'submitpackage', 'decoderawtransaction'}
TOKEN = Path('/etc/paperclip-ark-rpc/token').read_text().strip()
AUTH = 'Basic ' + base64.b64encode(('ark:' + TOKEN).encode()).decode()
slots = threading.BoundedSemaphore(2)

class Handler(BaseHTTPRequestHandler):
    def setup(self):
        super().setup()
        self.connection.settimeout(90)

    def do_POST(self):
        if self.path != '/' or not hmac.compare_digest(self.headers.get('Authorization', ''), AUTH):
            self.send_error(401)
            return
        # Keep backend concurrency bounded, but allow short bursts from CLN,
        # the indexer and wallet fee queries to wait for an available slot.
        if not slots.acquire(timeout=10):
            self.close_connection = True
            data = json.dumps({'result': None, 'error': {
                'code': -28, 'message': 'Chain RPC busy; retry shortly'}, 'id': None}).encode()
            self.send_response(503)
            self.send_header('Content-Type', 'application/json')
            self.send_header('Content-Length', str(len(data)))
            self.send_header('Connection', 'close')
            self.end_headers()
            self.wfile.write(data)
            return
        try:
            length = int(self.headers.get('Content-Length', '0'))
            if not 0 < length <= 4 * 1024 * 1024:
                self.send_error(413)
                return
            request = json.loads(self.rfile.read(length))
            if not isinstance(request, dict) or request.get('method') not in ALLOWED:
                self.send_error(403)
                return
            if not isinstance(request.get('params', []), (list, dict)):
                self.send_error(400)
                return
            # Never forward caller-controlled auth, paths or extra envelope fields.
            body = json.dumps({'jsonrpc': '1.0', 'id': request.get('id'),
                               'method': request['method'], 'params': request.get('params', [])}).encode()
            cookie = Path('/var/lib/bitcoin/.cookie').read_text().strip()
            rpc = urllib.request.Request('http://127.0.0.1:8332/', data=body,
                headers={'Content-Type':'application/json',
                         'Authorization':'Basic '+base64.b64encode(cookie.encode()).decode()})
            try:
                response = urllib.request.urlopen(rpc, timeout=60)
            except urllib.error.HTTPError as error:
                response = error
            with response:
                data = response.read(32 * 1024 * 1024 + 1)
                if len(data) > 32 * 1024 * 1024:
                    self.send_error(502)
                    return
                self.send_response(response.code)
                self.send_header('Content-Type', 'application/json')
                self.send_header('Content-Length', str(len(data)))
                self.end_headers()
                self.wfile.write(data)
        except Exception:
            self.send_error(502)
        finally:
            slots.release()

    def log_message(self, *_):
        pass

ThreadingHTTPServer(('127.0.0.1', 18337), Handler).serve_forever()
