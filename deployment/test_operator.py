import importlib.util
import json
from pathlib import Path
import tempfile
import threading
import unittest
from urllib.error import HTTPError
from urllib.request import Request, urlopen

spec = importlib.util.spec_from_file_location('operator_app', Path(__file__).with_name('asp_operator.py'))
app = importlib.util.module_from_spec(spec)
spec.loader.exec_module(app)


class OperatorTest(unittest.TestCase):
    def test_auth_and_existing_key_protection(self):
        with tempfile.TemporaryDirectory() as directory:
            app.DATA = Path(directory)
            app.WEB = Path(__file__).with_name('operator-web')
            app.password = 'test-only-' + 'a' * 64
            server = app.ThreadingHTTPServer(('127.0.0.1', 0), app.Handler)
            worker = threading.Thread(target=server.serve_forever)
            worker.start()
            def request(path, auth=False, body=None):
                headers = {'Authorization': 'Bearer ' + app.password} if auth else {}
                req = Request(f'http://127.0.0.1:{server.server_port}' + path,
                    headers=headers, data=None if body is None else json.dumps(body).encode())
                return urlopen(req, timeout=5)
            try:
                with request('/') as response:
                    self.assertIn(b'Ark server', response.read())
                    self.assertEqual(response.headers['Cache-Control'], 'no-store')
                with request('/paperclip-logo.jpg') as response:
                    self.assertEqual(response.headers['Content-Type'], 'image/jpeg')
                    self.assertTrue(response.read().startswith(b'\xff\xd8'))
                with self.assertRaises(HTTPError) as denied:
                    request('/api/status')
                self.assertEqual(denied.exception.code, 401)
                with request('/api/status', True) as response:
                    state = json.load(response)
                    self.assertFalse(state['lightning_enabled'])
                with self.assertRaises(HTTPError) as invalid:
                    request('/api/initialize', True, {'confirm': 'no'})
                self.assertEqual(invalid.exception.code, 400)
                (app.DATA / 'mnemonic').write_text('existing-private-test-state')
                with self.assertRaises(HTTPError) as existing:
                    request('/api/initialize', True, {'confirm': 'CREATE NEW ASP'})
                self.assertEqual(existing.exception.code, 409)
                self.assertEqual((app.DATA / 'mnemonic').read_text(), 'existing-private-test-state')
                self.assertEqual(app.children, {})
                with self.assertRaises(HTTPError) as missing:
                    request('/../../config/asp.json', True)
                self.assertEqual(missing.exception.code, 404)
            finally:
                server.shutdown()
                server.server_close()
                worker.join()


if __name__ == '__main__':
    unittest.main()
