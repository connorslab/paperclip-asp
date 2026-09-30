from pathlib import Path
import json
import os
import subprocess
import sys
import tempfile
import unittest


class ProvisionTest(unittest.TestCase):
    @unittest.skipUnless(os.name == 'posix', 'Umbrel provisioning uses Linux container paths')
    def test_new_app_only_and_node_credentials_unchanged(self):
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            node = root / 'node.env'
            original = 'export APP_BITCOIN_KNOTS_RPC_USER="test"\nexport APP_BITCOIN_KNOTS_RPC_PASS="private-test-only"\n'
            node.write_text(original)
            data = root / 'app'
            cmd = [sys.executable, str(Path(__file__).with_name('provision-umbrel.py')),
                '--app-data', str(data), '--node-env', str(node),
                '--rpc-url', 'http://selected-node:9332', '--postgres-host', 'private-postgres',
                '--network', 'regtest']
            result = subprocess.run(cmd, capture_output=True, text=True)
            self.assertEqual(result.returncode, 0, result.stderr)
            self.assertNotIn('private-test-only', result.stdout + result.stderr)
            self.assertEqual(node.read_text(), original)
            config = data / 'data/config/asp.json'
            before = config.read_bytes()
            self.assertEqual(json.loads(before)['bitcoind']['rpc_pass'], 'private-test-only')
            self.assertNotEqual(subprocess.run(cmd, capture_output=True).returncode, 0)
            self.assertEqual(config.read_bytes(), before)
            self.assertEqual(node.read_text(), original)


if __name__ == '__main__':
    unittest.main()
