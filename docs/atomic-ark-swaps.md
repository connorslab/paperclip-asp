# Direct atomic Ark swaps

Decision: selected on 2026-10-04 UTC for `feature/inter-asp-openark`.
Status: executable coordination model and standalone XBT regtest contracts.
No Ark payment capability is implemented.

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
- Connect two independent test Ark servers and wallets; measure actual costs.
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

Ten tests cover settlement, replay, crash after destination claim, independent
refunds, exhausted inventory, partial preparation, incorrect secrets, quote
substitution, fee floors, timing, and provider monitoring. One adversarial test
intentionally demonstrates loss when the provider misses its source window and
the destination success path remains available. This is an expected negative
result, not proof of unconditional atomicity.

The model assumes each SQLite transition is final. Real chain confirmations,
reorgs, VTXO ancestry, signatures, default relay policy, and emergency-exit delays
are not modeled. Transaction-level validation remains the next prerequisite.

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
