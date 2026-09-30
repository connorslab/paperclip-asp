import json
from pathlib import Path
import subprocess
import sys
import tempfile
import unittest


class ConfigurationTest(unittest.TestCase):
    def test_container_backend_selection_and_private_admin(self):
        with tempfile.TemporaryDirectory() as directory:
            data = Path(directory)
            password = data / 'secret'
            password.write_text('local-test-secret\n')
            for watchman in (False, True):
                out = data / ('watchman.json' if watchman else 'asp.json')
                cmd = [sys.executable, str(Path(__file__).with_name('prepare-config.py')),
                    '--network', 'regtest', '--container', '--rpc-url', 'http://chosen-xbt-node:18443',
                    '--rpc-user', 'rpcuser', '--rpc-password-file', str(password),
                    '--postgres-host', 'postgres', '--postgres-password-file', str(password),
                    '--data-dir', str(data / 'state'), '--output', str(out)]
                if watchman:
                    cmd.append('--watchman')
                result = subprocess.run(cmd, capture_output=True, text=True)
                self.assertEqual(result.returncode, 0, result.stderr)
                self.assertNotIn('local-test-secret', result.stdout + result.stderr)
                cfg = json.loads(out.read_text())
                self.assertEqual(cfg['bitcoind']['url'], 'http://chosen-xbt-node:18443')
                self.assertEqual(cfg['postgres']['host'], 'postgres')
                if not watchman:
                    self.assertTrue(cfg['require_board_funding_tx'])
                    self.assertEqual(cfg['rpc']['admin_address'], '127.0.0.1:3536')
                    self.assertEqual(cfg['rpc']['public_address'], '0.0.0.0:3535')
                    self.assertEqual(cfg['cln_array'], [])
                    self.assertEqual(cfg['max_ln_send_amount'], '0 sat')
                self.assertNotEqual(subprocess.run(cmd, capture_output=True).returncode, 0,
                    'Never overwrite an existing operator configuration')
                rejected = cmd.copy()
                rejected.remove('--container')
                self.assertNotEqual(subprocess.run(rejected, capture_output=True).returncode, 0)


if __name__ == '__main__':
    unittest.main()
