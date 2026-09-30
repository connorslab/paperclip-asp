# Bundled pruned XBT backend

The Umbrel ASP image includes the same private history adapter as Paperclip Wallet.
Native unpruned, indexed XBT nodes remain supported without adapter setup.

For a pruned node, run as the container's uid 1000:

```
python3 /opt/paperclip/deployment/managed-chain.py configure
```

Supply the private node RPC URL and credentials at the prompts. Configuration
verifies the selected chain and XBT header format before saving credentials.
Use `status` to inspect progress, or `connection` privately to obtain the local
RPC credentials. Do not publish that output. Configure **both** `asp.json` and
`watchman.json` to use the returned `bitcoind.url`, `rpc_user`, and `rpc_pass`.
Stop the app before editing an existing backend configuration.

The adapter listens only on 127.0.0.1:18336 inside the operator container.
History and protected cookies persist under `/var/lib/paperclip-asp/chain`,
inside the existing ASP data volume. Back up that directory along with the
complete ASP state, private configuration, and PostgreSQL database.

The managed supervisor starts the adapter, restarts it if it exits, and waits
for a complete index before launching the operator, ASP, and watchman when
either configured backend uses the bundled endpoint. The operator UI is not
available during initial indexing. Missing history or unavailable full peers
keeps startup blocked; it does not initialize replacement keys or bypass
validation. The application RPC guard also rejects an incomplete adapter.

Start the app through `python3 /opt/paperclip/deployment/managed-chain.py`.
The Umbrel generator sets this command and `PAPERCLIP_PRUNED_RPC=1`.
The separate-process Compose template still requires its documented external
private adapter; loopback cannot be shared between separate containers.

The adapter is optional and is never silently substituted for an existing node.
Use a private network or encrypted tunnel for remote node RPC access.
