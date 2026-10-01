# XBT Lightning integration

Status: funded version-2 Lightning is deployed in Paperclip's public beta and
requires explicit opt-in for new installations. A funded channel and server pool
are required for live payments. This code has not been independently audited.

## Architecture

Paperclip uses Bark's existing Ark-to-Lightning contracts. The wallet spends Ark
VTXOs into an outgoing HTLC; the ASP pays through XBT CLN. Incoming Lightning uses
the hold-invoice plugin and ASP pool liquidity to create an HTLC VTXO; the wallet
claims Ark funds. Users keep their keys and do not need their own Lightning node.
On-chain funds remain a separate wallet balance. Lightning uses the Ark balance.

The adapter uses authenticated CLN gRPC, xpay, and the hold-invoice gRPC plugin.
It requires XBT identity bit 512 in init, node, and invoice features, and unified
signature capability bit 515 in init and node features. Optional identity 513,
old 68/70 assignments, and missing feature fields are rejected before a node is
registered. These assignments match
[v26.06.8-blake2b.5](https://github.com/privkeyio/lightning/blob/v26.06.8-blake2b.5/common/features.c).
Feature declarations do not prove channel balances or the node's chain backend;
the isolated integration test must verify those separately.

Admission does not inspect a vendor name or exact release string. Compatible CLN
forks may advertise additional features; regression tests cover these and padded
feature vectors. Retropex and privkeyio distributions are targets subject to the
same RPC/plugin requirements. Paul Lamb's
[Lightning Fork](https://github.com/paulscode/lightning-fork/tree/blake2b) is LND-based,
so it needs a separate backend adapter; CLN gRPC is not interchangeable with LND.
Its peer/invoice interoperability and its use as an ASP backend are separate tests.
Do not weaken XBT identity or signature checks merely to admit an older fork.

## Funded Lightning test profile

Opt in with `experimental_funded_lightning: true`, a verified XBT CLN backend,
and its XBT hold plugin. The ASP advertises `funded_lightning`; wallet controls
use this negotiated capability. Defaults remain off for unconfigured installations.

Outgoing HTLC allocation, cooperative refunds, receive allocations, claims, and
ASP pool trees use funded version-2 recovery paths. Payment amounts are preserved;
recovery reserves are separate wallet costs (4,000–6,000 sats per selected input,
plus 4,000 sats per HTLC input for a cooperative claim/refund). An incoming payment
therefore credits less than its invoice amount after service and claim costs.
Fee estimates use maximum send allocation cost and a single-input receive estimate;
fragmented balances can require more inputs. Too-small or dust-shaped requests fail.

Signed incoming recovery paths are validated before exposing a preimage. Outgoing
refunds wait for both relative and absolute locktimes. CLI and REST exit selection
include locked Lightning contracts and reject unknown/spent selections instead of
silently starting an empty exit. Persistent wallet data is required for recovery;
retain backups and do not replace signed contracts with a different profile.

The ASP pool pays its issuance/allocation reserves. Pool targets automatically
consume the ASP funding wallet when funds arrive; these are separate from CLN
channel liquidity. An empty pool does not settle a hold invoice or disclose the
wallet preimage. Receiving needs CLN inbound capacity and Ark pool inventory;
sending needs a funded Ark wallet and CLN outbound capacity.

## Verification

Run `nix develop --command just checks` in both repositories, funded unit tests
with `just unit funded`, and the wallet UI contract tests with
`node scripts/test-web.mjs`. Shared protocol sources must match
`SHARED-SOURCE.sha256` in both repositories (LF checkout).

`just int-lightning` uses `scripts/test-lightning.sh` and the pinned isolated
integration harness. Set PAPERCLIP_ASP_BIN, PAPERCLIP_WALLET_BIN,
PAPERCLIP_WATCHMAN_BIN, XBT_BITCOIND, PAPERCLIP_CLN_EXEC and
PAPERCLIP_CLN_PLUGIN_DIR to the built test binaries. Run inside the Nix shell;
the CLN wrapper must select the matching XBT bitcoin-cli. Never point these
variables at production data directories.

The 2026-09-30 isolated tests use signed CLN v26.06.8-blake2b.5, the patched
Boltz hold plugin, and XBT Knots with TRUC rejected and standardness enforced.
Coverage includes BOLT11/BOLT12, amountless invoices/offers, duplicates,
failed-payment refunds, ASP restart, depleted pool, incoming offline preimage
claims and outgoing offline timeout refunds. An independent Knots peer verifies
outgoing exit propagation. These are regtest results, not mainnet settlement proof.

## Private Umbrel test

CLN and hold use separate mTLS client credentials. Only their client certificates,
client keys and CA certificates are copied into the ASP's private configuration
mount. CLN gRPC 9737 and hold gRPC 9738 remain internal; the node seed, CA private
keys and server private keys are not mounted in the ASP. Private DNS aliases must
match the certificate SANs (`cln` and `hold`). Preserve Umbrel app authentication.

Use separate wallets for ASP pool funding, CLN channels and the user test wallet.
The private test deployment sets `max_ln_send_amount` and `max_ln_receive_amount`
to `50000 sat` (zero disables a direction), and pool targets to `["100000sat:2"]`.
Fund small amounts, confirm pool issuance and channel balance in both directions,
then perform mainnet payments. Enabling a capability does not create liquidity.
VPS services and the existing FLYNN wallet are outside this private deployment.

Example private CLN entry in `cln_array` (matching certificate SANs):

```json
{"uri":"https://cln:9737","priority":0,
 "server_cert_path":"/config/cln/ca.pem",
 "client_cert_path":"/config/cln/client.pem",
 "client_key_path":"/config/cln/client-key.pem",
 "hold_invoice":{"uri":"https://hold:9738",
  "server_cert_path":"/config/hold/ca.pem",
  "client_cert_path":"/config/hold/client.pem",
  "client_key_path":"/config/hold/client-key.pem"}}
```

Validate before restarting: `paperclip-asp check-config /config/asp.json`.
Each private credential directory is mode 0700; files are mode 0600 owned by the
application UID. Preserve the original configuration and existing wallet data.
