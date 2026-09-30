"""Real, isolated pruned-node retrieval/reorg/restart tests. No live wallets."""
import importlib.util
import json
import os
from pathlib import Path
import socket
import subprocess
import tempfile
import time

spec = importlib.util.spec_from_file_location('adapter', Path(__file__).with_name('pruned_rpc.py'))
adapter = importlib.util.module_from_spec(spec)
spec.loader.exec_module(adapter)


def port():
    with socket.socket() as s:
        s.bind(('127.0.0.1', 0))
        return s.getsockname()[1]


def wait(fn, timeout=60):
    deadline = time.monotonic()+timeout
    while time.monotonic() < deadline:
        try:
            result = fn()
            if result:
                return result
        except Exception:
            pass
        time.sleep(.1)
    raise AssertionError('Timed out waiting for isolated fixture')


def main():
    binary = os.environ['XBT_BITCOIND']
    assert os.environ.get('IN_NIX_SHELL'), 'Run with the repository Nix environment'
    evidence = Path(__file__).resolve().parents[1]/'.state/pruned-tests'
    evidence.mkdir(parents=True, exist_ok=True)
    root = Path(tempfile.mkdtemp(prefix='regtest-', dir=evidence))
    print('Evidence:', root, flush=True)
    processes = []
    index = None
    try:
        nodes = []
        for name, prune in [('archive', 0), ('pruned', 1)]:
            datadir = root/name
            datadir.mkdir(mode=0o700)
            rpc_port, peer_port = port(), port()
            log = open(datadir/'process.log', 'w')
            command = [binary, '-regtest', '-server', '-listen=1', '-bind=127.0.0.1',
                       f'-datadir={datadir}', f'-rpcport={rpc_port}', f'-port={peer_port}',
                       '-rpcbind=127.0.0.1', '-rpcallowip=127.0.0.1', '-dnsseed=0',
                       '-discover=0', '-listenonion=0', '-natpmp=0', '-upnp=0', '-connect=0',
                       '-txindex=0', f'-prune={prune}', '-fastprune=1',
                       '-testactivationheight=blake2b@100', '-acceptnonstdtxn=0', '-mempooltruc=reject']
            processes.append(subprocess.Popen(command, stdin=subprocess.DEVNULL, stdout=log, stderr=subprocess.STDOUT))
            node = adapter.Node(f'http://127.0.0.1:{rpc_port}', datadir/'regtest/.cookie')
            wait(lambda: node.call('getblockchaininfo'))
            nodes.append((node, peer_port))
        archive, backend = nodes[0][0], nodes[1][0]
        backend.call('addnode', [f'127.0.0.1:{nodes[0][1]}', 'onetry'])
        wait(lambda: backend.call('getpeerinfo'))
        descriptor = archive.call('getdescriptorinfo', ['raw(51)'])['descriptor']
        archive.call('generatetodescriptor', [1000, descriptor])
        wait(lambda: backend.call('getblockcount') == 1000)
        assert backend.call('getindexinfo') == {}
        old_hash = backend.call('getblockhash', [150])
        old_txid = backend.call('getblock', [old_hash, 1])['tx'][0]
        index = adapter.Index(root/'index.sqlite', backend, 0)
        while not index.sync()['synced']:
            pass
        backend.call('pruneblockchain', [700])
        try:
            backend.call('getblock', [old_hash])
            raise AssertionError('Test block was not actually pruned')
        except adapter.RpcError as error:
            assert 'pruned' in str(error).lower()
        result = index.transaction(old_txid, True)
        assert result['blockhash'] == old_hash and result['confirmations'] == 851
        print('PASS: confirmed transaction recovered from genuinely pruned block', flush=True)
        index.db.close()
        index = adapter.Index(root/'index.sqlite', backend, 0)
        assert index.transaction(old_txid, True)['confirmations'] == 851
        print('PASS: durable index and transaction cache survive restart', flush=True)
        archive.call('generatetodescriptor', [1, descriptor])
        wait(lambda: backend.call('getblockcount') == 1001)
        stale_hash = backend.call('getblockhash', [1001])
        stale_txid = backend.call('getblock', [stale_hash, 1])['tx'][0]
        assert index.transaction(stale_txid, True)['confirmations'] == 1
        # Disconnect to keep the invalidated branch from being re-announced.
        backend.call('setnetworkactive', [False])
        backend.call('invalidateblock', [stale_hash])
        backend.call('setmocktime', [backend.call('getblockheader', [stale_hash])['time'] + 600])
        replacement = backend.call('getdescriptorinfo', ['raw(52)'])['descriptor']
        backend.call('generatetodescriptor', [2, replacement])
        index.sync()
        try:
            index.transaction(stale_txid, True)
            raise AssertionError('Stale confirmation accepted')
        except adapter.RpcError as error:
            assert error.code == -32004
        print('PASS: real chain reorganization removes stale confirmation', flush=True)
        # Fresh index cannot claim readiness when old blocks and peers are absent.
        backend.call('pruneblockchain', [700])
        unavailable = adapter.Index(root/'unavailable.sqlite', backend, 0, fetch_timeout=.1)
        try:
            try:
                unavailable.sync(limit=1003)
                raise AssertionError('Unavailable history was accepted')
            except adapter.RpcError:
                assert not unavailable.info()['synced']
        finally:
            unavailable.db.close()
        print('PASS: unavailable historical data fails closed', flush=True)
        (root/'result.json').write_text(json.dumps({'passed': 4, 'production_changes': False}))
        print('Evidence:', root, flush=True)
    finally:
        if index:
            index.db.close()
        for process in processes:
            process.terminate()
        for process in processes:
            try:
                process.wait(timeout=30)
            except subprocess.TimeoutExpired:
                process.kill()
                process.wait()


if __name__ == '__main__':
    main()
