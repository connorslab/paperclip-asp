# Inter-server payments

Status: prototype. `ark::interop` implements recipient-signed address-to-offer
authorization. The [swap experiment](atomic-ark-swaps.md) includes a persistent
two-ledger model and a passing two-ASP XBT regtest settlement test using actual
funded VTXOs. Experimental lock/settlement endpoints are disabled by default and
restricted to regtest. No production discovery or payment adapter is enabled.
Production behavior remains unchanged.

Development branch: `feature/inter-asp-openark`.

## Targets

1. Send from a Paperclip wallet to an Ark address on another Paperclip Ark server
   through a direct atomic swap. The recipient receives destination-backed value.
2. Support an OpenArk peer through a separate adapter when the peer implements
   compatible XBT Lightning settlement and authenticated recipient resolution.

The primary design is [direct atomic swaps](atomic-ark-swaps.md). Lightning is an
explicitly quoted fallback, not the primary implementation target. It does not
accept another server's VTXOs as local VTXOs. The second target does not require
Paperclip to replace its transaction format or its recovery model.

Same-server payments retain the existing arkoor path. Old wallet versions retain
their existing APIs. Foreign addresses remain rejected until a complete adapter
is enabled and verified. There is no silent on-chain fallback.

## Alternative: source-backed receipt (not selected)

Distinguish address interoperability from a transfer of backing between servers.
An alternative low-cost design is a multi-server recipient wallet. It resolves
the recipient's public identity to a recipient-authorized address on the source
server. The sender then makes an ordinary local arkoor payment. The recipient
retains a claim on the source server, with that server's availability, expiry,
recovery costs, and security assumptions. This does not move backing to the
recipient's home server.

The recipient must explicitly allow each full source-server identity and its
recovery profile. A home-server signature alone cannot authorize a substituted
recipient key. Source-specific addresses must bind to the recipient's own keys.
The wallet must retain separate server balances, backups, refresh schedules, and
exit data. It must show the backing server even if it presents a combined total.
Do not silently accept arbitrary servers through a global directory.

This approach targets the cost of one ordinary arkoor transfer under the source
server's policy. It adds no per-payment Lightning route or server liquidity swap.
It does not remove the existing recovery allocation or later refresh costs.
Recipients need source-server monitoring and backup support before acceptance.

The selected design requires backing on the recipient's home server. Research a
direct atomic Ark swap with prefunded inventory at both
servers. Require verified hashlock or adaptor-signature construction, safe timeout
margins, and recovery on both sides before implementation. Existing small-anchor
pubkey transfer budgets must not be reused blindly for conditional swap outputs.
Such swaps can avoid Lightning routing fees but cannot promise local-transfer
costs. Price inventory, refresh, rebalancing, and recovery costs explicitly.
Batch rebalancing can amortize costs; unsecured operator IOUs are not settlement.

For OpenArk, source-backed receipt requires the wallet to validate that peer's
VTXO and exit formats. The initial Lightning adapter alone cannot provide this
low-cost mode. Keep these capabilities separate in discovery and user messages.

## Discovery and identity

The current Ark address contains a four-byte server identifier, a recipient
policy, and delivery information. It does not contain an endpoint. The short
identifier is a lookup hint, not an authentication credential.

The initial prototype must use an operator-configured peer allowlist. Each entry
must pin the full server public key, an endpoint, and an explicit XBT network
profile. A genesis hash alone is insufficient if chains share history. Network
identity must include the fork and required signature rules. Do not infer XBT
support from an `ark` address or a Lightning invoice prefix.

Define a versioned discovery response with full server identity, network profile,
protocol versions, payment capabilities, amount limits, and quote endpoint.
Authenticate it against the pinned key. Specify canonical signed bytes, domain
separation, expiry, and key rotation before implementation. A future payment URI
can carry a full identity and endpoint without changing existing addresses.

Never fetch arbitrary recipient-supplied URLs from the server. Peer configuration
must constrain destinations and redirects. Public discovery requires separate
SSRF, DNS rebinding, privacy, and identity review. Tor endpoints need explicit
transport support. HTTPS remains the initial transport.

## Quote and recipient registration

The recipient wallet must authenticate registration of the Ark address and its
receive service. A server signature alone does not prove recipient authorization.
Bind the receive request to the address, full destination identity, network,
recipient authorization, invoice payment hash, amount, and expiry.

The sender requests an exact recipient net amount. The quote must show:

- Recipient net sats and sender total sats.
- Source service fee and recovery allocation.
- Destination service fee and recovery allocation.
- Maximum Lightning routing fee and total maximum debit.
- Quote identifier, expiry, and capability versions.

Use integer sats or msats with explicit conversion rules. Reject overflow,
negative amounts, unsupported precision, and a mismatch between invoice and quote.
The sender must approve a higher price if a quote expires. No operator subsidy is
assumed. Reserve costs must not be described as Lightning miner fees.

## Lightning fallback settlement contract

The flow is source Ark HTLC, XBT Lightning payment, then destination Ark HTLC.
The destination wallet must verify its enforceable claim and persist recovery
data before it releases the preimage. Specify timeout margins that cover both
Ark recovery paths and the Lightning route. Do not assume two successful API
calls establish end-to-end atomicity.

Persist a payment operation before an external side effect. Bind its idempotency
key to the source wallet, recipient, quote, and payment hash. Reuse with different
parameters must fail. Track at least these distinct states:

`quoted -> prepared -> payment_pending -> claimed`

Also track `expired`, `failed`, and `recovery_pending`. An RPC timeout is an unknown
outcome, not a failed payment. Reconcile the original payment hash before retry.
Report success only with settlement evidence and the destination claim complete.
Cancellation cannot override a settled HTLC. Bound all pending resource usage.

The initial receive path requires an online recipient service. Do not promise
offline receipt until a separately reviewed mechanism preserves recipient control.

## OpenArk adapter

Reference: [OpenArk specification at 3e636aea](https://github.com/dukeh3/openark/blob/3e636aea8b88b5bb1e05e2c043457ca9a96cdada/15-open-ark.md).
This is an experimental proposal, not an accepted BOLT or a claim of compatibility.

| OpenArk area | Paperclip target | Release condition |
| --- | --- | --- |
| HTLC semantics and Lightning interconnection | Reuse the settlement boundary above | Paired implementation passes success, timeout, and exit tests |
| Nostr transport and proposed NIP-150 | Optional transport adapter with signed messages | Pin companion encoding; verify replay, authentication, and privacy behavior |
| Peer capabilities and round parameters | Map to discovery and timeout checks | Reject unsupported network and recovery profiles |
| Owner and agent keys | Separate future restricted refresh design | Prove the agent cannot redirect value; test revocation and recovery |
| External liquidity and co-verifiers | Separate research work | Validate signing, abort, capital recovery, and signer availability |
| OpenArk VTXO and recycle formats | No direct import in this milestone | Requires a separate protocol and migration review |

Do not label an ordinary Lightning payment as full OpenArk compatibility. Record
the exact peer version and tested subset. A SHA-256 bitcoin OpenArk deployment is
not an XBT peer. Cross-chain swaps are outside this branch's target.

MuSig2 key aggregation does not itself define a general t-of-n threshold scheme.
Do not assume the website's signing terminology proves the required failure or
custody properties. Nostr transport does not enforce payment settlement.

## Implementation sequence and acceptance

- [ ] Define and verify direct swap success and refund transaction graphs.
- [ ] Measure default-policy recovery costs for both swap legs.
- [ ] Add durable inventory reservations and the direct swap state machine.
- [ ] Define discovery and quote schemas, canonical signatures, and test vectors.
- [ ] Add disabled-by-default configured-peer discovery and capability negotiation.
- [ ] Add authenticated recipient registration and exact-net receive quotes.
- [ ] Add durable payment coordination and recovery reconciliation.
- [ ] Add wallet foreign-address dispatch, fee review, and payment status UI.
- [ ] Run two independent Paperclip servers with separate keys and databases.
- [ ] Implement the OpenArk transport adapter against a pinned companion spec.
- [ ] Run a real OpenArk peer adapted to XBT; document unsupported capabilities.
- [ ] Review and release only the tested capabilities; preserve old-wallet paths.

Required tests cover wrong network, short-ID collision, false server identity,
recipient substitution, quote expiry, integer bounds, duplicate requests, offline
recipient, insufficient pool and channel liquidity, lost RPC responses, server
restart at each persisted stage, HTLC timeout, and emergency exit on both sides.
Assert no duplicate debit, no success without a recipient claim, and no recovery
data loss. Test that an old wallet continues to send and receive on its server.

Use isolated XBT regtest first. Do not enable this branch on production or move
mainnet sats as part of the initial prototype. A later release needs paired-peer
evidence and a documented rollback path for persisted payment operations.

## Prototype validation

On 2026-10-04 UTC, the isolated FLYNN development shell ran `just unit interop`
and `just checks`. Four authorization tests passed and the workspace check passed.
The tests cover an accepted binding, wrong peer, wrong network, expiry, offer
substitution, altered expiry, wrong recipient signer, and an oversized offer.
These checks do not validate settlement, a public API, or an OpenArk peer.
No production service or wallet was changed and no mainnet payment was made.
