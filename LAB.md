# Private interactive lab

This lab uses two disposable regtest wallets, Alice and Bob. It has no connection
to the pool's node or to a production Lightning node. The wallet service holds the
test keys on the lab machine. This is not a browser-only wallet.

Build both repositories. Set absolute binary paths, then start the lab:

```sh
export XBT_BITCOIND=/absolute/path/to/knots/bin/bitcoind
export PAPERCLIP_ASP_BIN=/absolute/path/to/paperclip-asp/target/debug/paperclip-asp
export PAPERCLIP_WALLET_BIN=/absolute/path/to/paperclip-wallet/target/debug/paperclip-wallet
export PAPERCLIP_WALLETD_BIN=/absolute/path/to/paperclip-wallet/target/debug/paperclip-walletd
nix develop --command python3 scripts/lab.py
```

Use the pinned Knots v29.4.2.knots20260508 Linux x86-64 binary. The launcher checks
its SHA-256 before use. The verified release archive hash is
`b59d0445a317e21a03dc29425db3aba79b27d5125230b1a2b1dce62e120827c5`.

The launcher creates owner-only state under `.state/lab`, a PostgreSQL Unix socket
inside that private directory, an isolated regtest node, an ASP and Alice's wallet
API. It creates and funds Alice and Bob with test coins on first setup. Subsequent
starts reuse the data. Do not delete the directory to stop services.

All network listeners are loopback-only:

| Service | Port |
| --- | --- |
| Alice wallet web interface and authenticated API | 38180 |
| Knots test RPC | 38443 |
| ASP wallet-facing RPC | 38535 |
| ASP administrative RPC | 38536 |

The admin RPC is privileged and unauthenticated. Keep it on loopback. The database
uses a private Unix socket, with no TCP listener. Nothing is enabled at boot.

For a remote machine, forward only the wallet port:

```sh
ssh -N -L 38180:127.0.0.1:38180 YOUR_USER@YOUR_TEST_HOST
```

Open `http://127.0.0.1:38180`. Read `.state/lab/alice/auth_token` privately on the
test host and enter it into the page. Do not paste it into chat or commit it.
Use the same data directory with `paperclip-walletd secret show` if preferred.

Blocks are mined only when requested; there is no unattended accelerated mining.
After an on-chain operation or a refresh, mine confirmations:

```sh
python3 scripts/lab.py --mine 3
```

Do not mine through VTXO expiry without first refreshing or exiting. This lab is
also suitable for explicitly testing expiry behavior with disposable balances.

Run the API checks (moves 10,000 disposable sats from Alice to Bob):

```sh
nix develop --command python3 scripts/check-lab.py
```

Stop cleanly from another terminal:

```sh
python3 scripts/lab.py --stop
```

The launcher stops only the processes it created. Keep a copy of the full stopped
`.state/lab` directory if preserving test positions. Logs and generated credentials
stay there and are excluded from Git. A partially interrupted first initialization
may need manual reconciliation; this launcher is a test tool, not production
provisioning or financial automation.
