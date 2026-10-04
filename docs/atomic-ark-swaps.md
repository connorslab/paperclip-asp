# Direct atomic Ark swaps

Decision: selected on 2026-10-04 UTC for `feature/inter-asp-openark`.
Status: experimental settlement tested between two real regtest ASP processes.
Disabled by default; enabling it outside regtest is rejected. Not a public release.

See the [plain-language walkthrough](atomic-swap-walkthrough.md) for the payment
flow, liquidity movement, and failure cases.

## Run the two-ASP integration test

```sh
nix develop
XBT_BITCOIND=/absolute/path/to/verified/xbt/bitcoind just int-asp-swap
```

This starts an isolated XBT Knots node, private PostgreSQL with separate ASP
databases, and two `paperclip-asp` processes. All RPC listeners use loopback;
the node has no peers. The fixture client uses PUBLIC deterministic user keys.
Never use its addresses or keys for real funds. Each run creates new state in
`/tmp/paperclip-asp-swap-*`; `report.json` records results. All child processes
stop when the test finishes, including on failure.

The test boards real regtest UTXOs, creates one conditional VTXO on each ASP,
claims on B and then A with the same secret, and validates the complete signed
ancestry. The recipient receives 20,000 sats backed by B; the provider receives
25,320 sats backed by A. Both then make ordinary 5,000-sat Ark payments.
It also checks duplicate/concurrent claims, conflicting destinations, reuse of
locked inventory, restart replay, premature refunds, mature refunds, and
disabled endpoint rejection. With both ASPs stopped, it broadcasts and confirms
the recipient's six-transaction ancestry through XBT Knots under default policy.
This last check validates real ancestry and settlement signatures, but is not
a complete adversarial unilateral-exit or reorg campaign.

Current test economics, excluding initial boarding and the later 5,000-sat
demonstration payments:

| Item | Sats |
|---|---:|
| Recipient's net payment | 20,000 |
| Lock allocation per side, with change | 3,990 |
| Settlement allocation per side | 1,330 |
| Total allocation across both sides | 10,640 |
| Provider's destination costs reimbursed by sender | 5,320 |
| Provider's net margin in this fixture | 0 |

These are recovery allocations, not fees already paid to miners. This first
end-to-end path is more expensive than a single local Ark transfer. It does not
yet satisfy the low-fee objective; batching or reducing the number of recovery
steps requires further design and testing. The fixture does not subsidize the
provider from ASP funds.

The test client coordinates both servers with predetermined participants and
a public fixture secret. Production still needs authenticated quotes, recipient
authorization, private secret generation and disclosure, automatic monitoring,
and peer discovery. No production wallet UI or automatic ASP peering is enabled.

## Candidate swap policy

`lib/src/experimental_swap.rs` now implements an independent claimant/refund
contract. Cooperative claim requires participant and ASP signatures plus the
preimage. Cooperative refund requires participant and ASP signatures after CLTV.
Unilateral claim/refund omit the ASP signature but require CSV; refund also
requires CLTV. A NUMS internal key prevents an unconditional key-path bypass.
Keys must be distinct even when compressed-key parity differs. Heights and
delays use the library's bounded policy validation.

`just unit inter_asp_candidate` locks a real signed funded VTXO into this policy,
validates its complete ancestry separately under two ASP keys, accounts for the
added recovery allocation, and rejects a modified deadline. This test holds the
fixture signing keys in one process. It does not demonstrate two running ASPs,
settlement into a normal wallet VTXO, or broadcasts of that ancestry.

`just unit swap_candidate` checks parameter bounds and commitment to every key,
hash and timing term. `just int-swap-contract` tests all four spend paths on
isolated XBT Knots, including wrong participant/server signatures, wrong secret,
premature recovery/refund and conflicting spends. These chain tests use directly
funded UTXOs, not the signed VTXO ancestry fixture.

The branch now adds experimental policy and genesis discriminants (`0x7e`),
version-one contract encoding, and regtest-only `ExperimentalSwapLock` and
`ExperimentalSwapSettle` RPCs. Existing policy encodings remain unchanged.
Old wallets cannot decode experimental swap ancestry; use only the fixture
client or a future explicitly compatible wallet for these outputs. Ordinary
existing wallet traffic retains its current format.

Admission binds the contract's ASP key and CSV delay to its VTXO, restricts
amounts to 1,000,000 sats, requires funded ancestry/checkpoints, and checks
deadline headroom. Settlement checks the participant signature and secret,
persists the spent input and signed output in one database transaction, and
returns the committed result on replay, even after its deadline. Ordinary send,
round and offboard admission do not accept conditional swap inputs.

No final cross-ASP fee quote or production safety claim follows from these tests.

## Actual VTXO boundary test

Run `just unit inter_asp_vtxo` in the development shell. This uses real funded
board and arkoor builders, two distinct server keypairs, and a shared payment
hash. It validates complete signed VTXOs against their respective funding
transactions. It rejects an incorrect server signer, incorrect funding ancestry,
and an incorrect receive preimage. The existing funded tests also pass after the
test helper is parameterized by server key.

This is a cryptographic library integration test. The funding transactions are
fixtures, not broadcasts by two running Ark servers. The test explicitly verifies
that the source HTLC's success claimant remains the local server. Consequently,
the current policies cannot express the independent provider's source claim.
Do not route swaps through these APIs until a new policy or proved composition
supports that role. The new experiment uses `ExperimentalSwap`, not those
Lightning policies.

## Standalone transaction fixture

```sh
XBT_BITCOIND=/absolute/path/to/verified/xbt/bitcoind just int-swap-contract
```

Run inside `nix develop`. The runner checks the same Knots binary hash as the
existing XBT integration harness. It starts its own temporary regtest node with
no peer connections, standard transaction enforcement, and TRUC rejection.
It cannot attach to a live node. The fixture uses PUBLIC deterministic keys;
never send real value to its addresses.

`lib/examples/atomic_swap_contract.rs` constructs a Taproot output with separate
hashlock-success and absolute-height-refund leaves. A NUMS internal key removes
the known key-spend bypass. Success requires a 32-byte secret and the claimant's
signature. Refund requires the refund key and a mature CLTV lock. Both paths use
XBT unified signatures. The source and destination fixtures use distinct Alice,
provider, and Bob keys and share a payment hash.

The runner checks invalid secrets and signatures, an early refund, confirmed
success and refund spends, double-spend rejection, a late success spend, and two
linked success spends. The fixed 1,000-sat transaction fee is test provisioning,
not an optimized recovery allocation or a proposed customer fee.

The fixture also supports an optional CSV block delay on both spend leaves.
A synthetic root -> parent -> claim chain tests accumulated confirmation delays.
The runner verifies rejection before the parent and child become mature, saves
the signed child, restarts the isolated node, and completes recovery with that
same child. It also invalidates and reconsiders one block to verify that maturity
is checked again after a reorg. These tests pass with a three-block CSV delay;
three blocks is a test parameter, not an approved production recovery margin.

These are directly funded on-chain UTXOs and synthetic ancestry. The test does not
include actual Ark ancestry, checkpoint revocation, server failure, or concurrent chain
races. The secret is deterministic and public in this fixture. This demonstrates
script enforcement and paired claims, not a secure end-to-end Ark swap.

## Production release gates

- Define a versioned Ark swap policy and validate its complete ancestry.
- Prove timeout margins for both success/refund graphs, including late disclosure.
- Verify each participant's independent recovery under server failure and reorgs.
- Add authenticated peer/recipient bindings and immutable signed quotes.
- Reserve inventory durably and reconcile unknown outcomes without duplicate debit.
- Extend the passing two-ASP fixture to production wallet integration and
  adversarial monitoring, recovery, partition, and reorg tests.
- Keep old VTXO policies and wallet APIs functional and retain rollback procedures.

The current fixtures do not meet these gates. No production swap capability is
advertised or enabled by this branch.

## Run the laboratory model

```sh
python3 -m unittest discover -s tests -p test_atomic_swap_model.py -v
```

`scripts/experimental_atomic_swap.py` uses two independent SQLite ledgers with
temporary simulated balances. It implements inventory reservations, hashlocked
claims, refunds, immutable operation identifiers, and restart reconciliation.
The provider monitor reconciles claims before it attempts refunds. Destination
refunds precede source refunds. No networking, wallet access, or real sats exist
in this model. Do not connect it to production.

Thirteen tests cover settlement, replay, crash after destination claim, independent
refunds, exhausted inventory, partial preparation, incorrect secrets, quote
substitution, fee floors, timing, and provider monitoring. One adversarial test
intentionally demonstrates loss when the provider misses its source window and
the destination success path remains available. This is an expected negative
result, not proof of unconditional atomicity.

Concurrent tests use independent SQLite connections to race claim against
refund, duplicate claims, and two reservations against the same inventory.
Exactly one conflicting transition succeeds; duplicate claims credit only once.
The isolated XBT contract test also rejects a refund after a confirmed claim
and a claim after a confirmed refund.

The separate two-ASP integration test now also races duplicate claims through
the real RPC and database paths. Source review shows
`cosign_oor_with_builder` locks inputs and persists the spend before signing;
`do_oor_spend_updates` conditionally updates spendable rows and permits only
the same transaction ID on replay. Before enabling production swaps, expand testing
through the actual server with competing swap, ordinary-send, refresh, offboard,
and HTLC requests, including restarts. The experimental swap policy uses these guards.

The model assumes each SQLite transition is final. Real chain confirmations,
reorgs, VTXO ancestry, signatures, default relay policy, and emergency-exit delays
are not modeled by that SQLite model. The separate two-ASP test validates actual
VTXO ancestry and transactions; reorg and comprehensive adversarial testing remain required.

## Outcome and participants

Alice has value on server A. Bob wants value on server B. A liquidity provider
has spendable inventory on B and accepts an enforceable claim on A in exchange.
The provider can be an operator or a separate participant. Alice and Bob do not
need Lightning channels. No Lightning routing is required for the direct path.

After success, Bob holds a recoverable claim backed by B. The provider holds a
claim backed by A. Inventory changes location; backing does not teleport between
servers. Opposite-direction payments can restore inventory. Sustained one-way
flow requires priced rebalancing. No unsecured inter-operator credit is allowed.

Both legs use the same explicit XBT network and payment hash. Bob generates the
secret and retains it until he verifies and saves his complete recovery package.
The quote binds both full server keys, participant keys, recipient address,
amounts, payment hash, deadlines, recovery profiles, and a unique operation ID.
The existing address-to-BOLT12 binding is fallback work. It is not swap consent.

## Required contract properties

The source leg transfers Alice's value to the provider with the secret, or lets
Alice recover it after the refund boundary. The destination leg transfers the
provider's value to Bob with that same secret, or lets the provider recover it.

Before the provider commits destination inventory, it must verify the source
claim, its ancestry, signatures, availability window, and exit funding. Before
Bob releases the secret, he must verify his destination claim to the same level.
Do not accept a server database entry or a promise to sign later as proof.

Persist recovery packages and operation state before each irreversible action.
An unknown RPC outcome requires reconciliation. It does not authorize another
swap. Revealing the secret prevents an unconditional cancellation promise.

An Ark swap retains the underlying Ark assumptions, including server behavior
for out-of-round transfers, monitoring, expiry, and timely chain access. Atomic
settlement does not remove these assumptions or make either server unnecessary.

## Timeout design is a release prerequisite

The source recovery window must permit the provider to claim after Bob exercises
the destination success path. Derive the window from the latest possible
destination preimage disclosure, ancestor confirmation delays, CSV delays, CLTV
boundaries, reorg allowance, and a confirmation/fee-bump margin. Use block heights,
not estimated wall-clock dates. Reject VTXOs whose lifetimes cannot cover it.

Do not assume `source expiry > destination expiry` proves this property. A
hashlock success branch can remain spendable after a refund branch becomes
available. Refund and success can race. The proof and tests must cover late
disclosure and the exact transaction graph, not just nominal invoice deadlines.

Current `ServerHtlcSendVtxoPolicy` and `ServerHtlcRecvVtxoPolicy` use the local
server as a privileged counterparty. The send policy also combines relative
delays with its refund boundary. Replacing a server key with a peer key is not
an implementation. A separate versioned policy or a proved composition is needed.
Old policy encodings and existing claims must remain unchanged.

## Cost and liquidity rules

Measure actual source and destination transaction graphs under default XBT relay
policy. Ordinary pubkey-transfer small-anchor budgets are not evidence that a
swap has the same cost. Test each emergency success and refund path independently.

The quote separates recipient net value, each recovery allocation, service fees,
and liquidity/rebalancing charges. Reserve destination inventory atomically and
persist the reservation. Never count a pending source claim as spendable
destination inventory. Bound concurrent reservations and reject exhausted peers.

Aim for low fees through simple graphs, input selection, inventory reuse, and
batched rebalancing. Do not quote a guaranteed local-transfer price or depend on
an ASP subsidy. Refresh, recovery monitoring, and rebalancing remain real costs.
Record realized costs so the operator can detect a negative margin.

## Prototype sequence

1. Construct both transaction graphs and enumerate every spend branch. Specify
   who signs each transaction and who can recover after each possible interruption.
2. Validate scripts and fees with XBT regtest nodes at default policy. Demonstrate
   independent source refund, destination refund, and both success paths.
3. Test adversarial timing, especially late secret disclosure and an offline
   server. Establish the timeout rules from these graphs.
4. Add versioned capability negotiation, recipient authorization, and signed
   quotes. Peers without the exact capability remain unsupported.
5. Add durable reservations, idempotency, restart reconciliation, and watchman
   support. Exercise two independent Paperclip servers and separate wallet keys.
6. Add wallet review and status UI. Report destination-backed receipt explicitly.
7. Test OpenArk through an adapter only after its actual XBT transaction graphs
   satisfy the same contract. Shared HTLC terminology alone is insufficient.

No production activation, public compatibility claim, or mainnet swap is part of
the design milestone. Lightning fallback requires its own quote and user approval.
