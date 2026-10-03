#!/usr/bin/env python3
"""Isolated regtest probe. Never connects to existing wallets or nodes."""
import argparse
import hashlib
import json
from pathlib import Path
import socket
import struct
import subprocess
import tempfile
import time
from decimal import Decimal

def free_port():
    with socket.socket() as sock:
        sock.bind(("127.0.0.1", 0))
        return sock.getsockname()[1]

def raw_tx(version, inputs, outputs):
    raw = struct.pack("<i", version) + bytes([len(inputs)])
    for txid, vout in inputs:
        raw += bytes.fromhex(txid)[::-1] + struct.pack("<I", vout) + b"\x00" + bytes.fromhex("fdffffff")
    raw += bytes([len(outputs)])
    for value, script in outputs:
        script = bytes.fromhex(script)
        raw += struct.pack("<Q", value) + bytes([len(script)]) + script
    return (raw + bytes(4)).hex()

def probe(bin_dir, policy, zero_penalty=False, standard_anchor=False):
    with tempfile.TemporaryDirectory(prefix="paperclip-relay-") as directory:
        port = free_port()
        common = ["-regtest", "-datadir=" + directory, "-rpcport=" + str(port)]
        daemon = subprocess.Popen([str(bin_dir / "bitcoind"), *common,
            "-testactivationheight=blake2b@100", "-acceptnonstdtxn=0",
            "-listen=0", "-connect=0", "-dnsseed=0", "-discover=0", "-listenonion=0",
            "-server=1", *(["-mempooltruc=" + policy] if policy != "default" else []),
            *(["-subdustfeepenalty=0"] if zero_penalty else [])],
            stdout=subprocess.DEVNULL, stderr=subprocess.DEVNULL)
        def rpc(method, *args):
            result = subprocess.run([str(bin_dir / "bitcoin-cli"), *common, method,
                *[json.dumps(a) if not isinstance(a, str) else a for a in args]],
                capture_output=True, text=True)
            if result.returncode: raise RuntimeError(result.stderr)
            try: return json.loads(result.stdout, parse_float=Decimal)
            except json.JSONDecodeError: return result.stdout.strip()
        try:
            for _ in range(100):
                try: rpc("getblockchaininfo"); break
                except RuntimeError:
                    if daemon.poll() is not None: raise RuntimeError("Isolated node failed to start")
                    time.sleep(0.1)
            rpc("createwallet", "relay-probe")
            address = rpc("getnewaddress", "", "bech32")
            rpc("generatetoaddress", 110, address)
            coin = rpc("listunspent")[0]
            value = int(coin["amount"] * 100_000_000)
            script = rpc("getaddressinfo", address)["scriptPubKey"]
            # Zero-fee parent, zero-value P2A anchor; child supplies package fees.
            version = 2 if standard_anchor else 3
            anchor_value = 330 if standard_anchor else 0
            anchor_script = "0020" + hashlib.sha256(bytes.fromhex("51")).hexdigest() if standard_anchor else "51024e73"
            parent = raw_tx(version, [(coin["txid"], coin["vout"])], [(value - anchor_value, script), (anchor_value, anchor_script)])
            signed = rpc("signrawtransactionwithwallet", parent, [], "ALL|UNIFIED")
            assert signed["complete"]
            parent = signed["hex"]
            txid = rpc("decoderawtransaction", parent)["txid"]
            child = raw_tx(version, [(txid, 0), (txid, 1)], [(value - 10_000, script)])
            child = rpc("signrawtransactionwithwallet", child, [
                {"txid":txid,"vout":0,"scriptPubKey":script,"amount":float(Decimal(value-anchor_value)/100_000_000)},
                {"txid":txid,"vout":1,"scriptPubKey":anchor_script,"amount":float(Decimal(anchor_value)/100_000_000), **({"witnessScript":"51"} if standard_anchor else {})}], "ALL|UNIFIED")["hex"]
            info = rpc("getmempoolinfo")
            return {"standard_anchor":standard_anchor,"requested_policy":policy,"zero_subdust_penalty":zero_penalty,"reported_policy":info.get("truc_policy"),
                "parent":rpc("testmempoolaccept", [parent]),
                "package":rpc("submitpackage", [parent, child])}
        finally:
            try: rpc("stop")
            except Exception: daemon.terminate()
            daemon.wait(timeout=30)

if __name__ == "__main__":
    parser=argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--bin-dir", type=Path, required=True)
    args=parser.parse_args()
    for policy in ["default", "reject", "enforce"]:
        print(json.dumps(probe(args.bin_dir, policy), default=str), flush=True)
    print(json.dumps(probe(args.bin_dir, "enforce", True), default=str), flush=True)

    print(json.dumps(probe(args.bin_dir, "reject", standard_anchor=True), default=str), flush=True)
