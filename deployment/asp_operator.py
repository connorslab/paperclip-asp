#!/usr/bin/env python3
"""Private test-app supervisor and authenticated, narrowly scoped operator API."""
import hmac
from http.server import BaseHTTPRequestHandler, ThreadingHTTPServer
import json
import os
from pathlib import Path
import signal
import socket
import subprocess
import threading
import time

DATA = Path('/var/lib/paperclip-asp')
CONFIG = Path('/config')
WEB = Path('/opt/paperclip/operator-web')
children = {}
lock = threading.Lock()
initializing = False
failed = False
stopping = threading.Event()
password = os.environ.pop('APP_PASSWORD', '')


def ready(port):
    try:
        with socket.create_connection(('127.0.0.1', port), timeout=1):
            return True
    except OSError:
        return False


def start_services():
    for name, binary, config in [('asp', 'paperclip-asp', 'asp.json'),
                                 ('watchman', 'paperclip-watchman', 'watchman.json')]:
        with (DATA / (name + '.log')).open('a') as log:
            children[name] = subprocess.Popen([binary, '--config', str(CONFIG / config), 'start'],
                                               stdout=log, stderr=log)


def initialize():
    global initializing, failed
    try:
        with (DATA / 'initialize.log').open('a') as log:
            result = subprocess.run(['paperclip-asp', '--config', str(CONFIG / 'asp.json'), 'create'],
                                    stdout=log, stderr=log, timeout=120)
        with lock:
            if result.returncode != 0:
                failed = True
            elif not stopping.is_set():
                start_services()
    except (OSError, subprocess.TimeoutExpired):
        failed = True
    finally:
        initializing = False


def snapshot():
    with lock:
        processes = {name: proc.poll() is None for name, proc in children.items()}
    try:
        config = json.loads((CONFIG / 'asp.json').read_text())
        lightning = bool(config.get('experimental_funded_lightning') and config.get('cln_array'))
    except (OSError, ValueError):
        lightning = False
    return {'initialized': (DATA / 'mnemonic').is_file(), 'initializing': initializing,
            'initialization_failed': failed, 'processes': processes,
            'asp_rpc_ready': ready(3536), 'lightning_enabled': lightning and processes.get('asp', False),
            'notice': 'Experimental XBT ASP. Lightning is configured; payments require channel and pool liquidity.' if lightning else 'Experimental XBT ASP. Lightning is disabled.'}


class Handler(BaseHTTPRequestHandler):
    def log_message(self, *_):
        pass

    def send(self, code, body, mime='application/json'):
        body = body if isinstance(body, bytes) else json.dumps(body).encode()
        self.send_response(code)
        self.send_header('Content-Type', mime)
        self.send_header('Content-Length', str(len(body)))
        self.send_header('Cache-Control', 'no-store')
        self.send_header('X-Content-Type-Options', 'nosniff')
        self.send_header('Referrer-Policy', 'no-referrer')
        self.send_header('Content-Security-Policy', "default-src 'self'; script-src 'self'; style-src 'self'; connect-src 'self'; object-src 'none'; base-uri 'none'; frame-ancestors 'self'")
        self.end_headers()
        self.wfile.write(body)

    def authenticated(self):
        actual = self.headers.get('Authorization', '')
        return hmac.compare_digest(actual.encode(), ('Bearer ' + password).encode())

    def do_GET(self):
        assets = {'/': ('index.html', 'text/html; charset=utf-8'),
                  '/app.js': ('app.js', 'text/javascript; charset=utf-8'),
                  '/style.css': ('style.css', 'text/css; charset=utf-8'),
                  '/paperclip-logo.jpg': ('paperclip-logo.jpg', 'image/jpeg')}
        if self.path in assets:
            filename, mime = assets[self.path]
            return self.send(200, (WEB / filename).read_bytes(), mime)
        if not self.authenticated():
            return self.send(401, {'error': 'Authentication required'})
        if self.path == '/api/status':
            return self.send(200, snapshot())
        return self.send(404, {'error': 'Not found'})

    def do_POST(self):
        global initializing
        if not self.authenticated():
            return self.send(401, {'error': 'Authentication required'})
        if self.path != '/api/initialize':
            return self.send(404, {'error': 'Not found'})
        try:
            length = int(self.headers.get('Content-Length', '0'))
            if not 0 < length < 128:
                raise ValueError()
            body = json.loads(self.rfile.read(length))
            if body != {'confirm': 'CREATE NEW ASP'}:
                raise ValueError()
        except (ValueError, json.JSONDecodeError):
            return self.send(400, {'error': 'Explicit new-ASP confirmation required'})
        with lock:
            if initializing or failed or (DATA / 'mnemonic').exists() or children:
                return self.send(409, {'error': 'Already initialized, in progress, or needs operator recovery'})
            initializing = True
            threading.Thread(target=initialize, daemon=True).start()
        return self.send(202, {'initializing': True})


def main():
    if len(password) < 32:
        raise SystemExit('A platform app password is required')
    os.umask(0o077)
    DATA.mkdir(mode=0o700, parents=True, exist_ok=True)
    os.chmod(DATA, 0o700)
    for config in ['asp.json', 'watchman.json']:
        if not (CONFIG / config).is_file():
            raise SystemExit('Private ASP and watchman configuration is required')
    if (DATA / 'mnemonic').is_file():
        start_services()
    server = ThreadingHTTPServer(('0.0.0.0', 3000), Handler)
    server.timeout = 1
    def stop(*_):
        stopping.set()
    signal.signal(signal.SIGTERM, stop)
    signal.signal(signal.SIGINT, stop)
    try:
        while not stopping.is_set():
            server.handle_request()
    finally:
        server.server_close()
        for proc in children.values():
            if proc.poll() is None:
                proc.terminate()
        deadline = time.monotonic() + 90
        for proc in children.values():
            try:
                proc.wait(timeout=max(1, deadline - time.monotonic()))
            except subprocess.TimeoutExpired:
                proc.kill()
                proc.wait()


if __name__ == '__main__':
    main()
