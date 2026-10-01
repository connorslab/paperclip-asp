import concurrent.futures
from http.server import ThreadingHTTPServer
import importlib.util
import io
import json
from pathlib import Path
import threading
import time
import unittest
from unittest.mock import patch, Mock
import urllib.request
import urllib.error

spec = importlib.util.spec_from_file_location('relay', Path(__file__).with_name('production-rpc-relay.py'))
relay = importlib.util.module_from_spec(spec)
with patch.object(Path, 'read_text', return_value='test-token'), patch('http.server.ThreadingHTTPServer'):
    spec.loader.exec_module(relay)


class RelayTests(unittest.TestCase):
    def test_concurrent_requests_queue_with_bounded_backend_work(self):
        active = 0
        peak = 0
        lock = threading.Lock()
        original_urlopen = urllib.request.urlopen

        def upstream(*args, **kwargs):
            nonlocal active, peak
            with lock:
                active += 1
                peak = max(peak, active)
            time.sleep(0.05)
            with lock:
                active -= 1
            response = io.BytesIO(b'{"result":{"feerate":0.00001},"error":null,"id":1}')
            response.code = 200
            return response

        server = ThreadingHTTPServer(('127.0.0.1', 0), relay.Handler)
        thread = threading.Thread(target=server.serve_forever, daemon=True)
        thread.start()
        def request(_):
            req = urllib.request.Request('http://127.0.0.1:' + str(server.server_port),
                json.dumps({'method': 'estimatesmartfee', 'params': [3], 'id': 1}).encode(),
                {'Authorization': relay.AUTH})
            with original_urlopen(req, timeout=5) as response:
                return json.load(response)
        try:
            with patch.object(Path, 'read_text', return_value='test:cookie'), patch.object(relay.urllib.request, 'urlopen', side_effect=upstream):
                with concurrent.futures.ThreadPoolExecutor(max_workers=6) as pool:
                    results = list(pool.map(request, range(12)))
            self.assertEqual(len(results), 12)
            self.assertTrue(all(r['error'] is None for r in results))
            self.assertLessEqual(peak, 2)
        finally:
            server.shutdown()
            server.server_close()
            thread.join()

    def test_saturated_relay_returns_json_rpc_error(self):
        server = ThreadingHTTPServer(('127.0.0.1', 0), relay.Handler)
        thread = threading.Thread(target=server.serve_forever, daemon=True)
        thread.start()
        req = urllib.request.Request('http://127.0.0.1:' + str(server.server_port), b'{}', {'Authorization': relay.AUTH})
        try:
            with patch.object(relay, 'slots', Mock(acquire=Mock(return_value=False))):
                with self.assertRaises(urllib.error.HTTPError) as error:
                    urllib.request.urlopen(req, timeout=5)
                with error.exception as response:
                    self.assertEqual(response.code, 503)
                    self.assertEqual(json.load(response)['error']['code'], -28)
        finally:
            server.shutdown()
            server.server_close()
            thread.join()

if __name__ == '__main__':
    unittest.main()
