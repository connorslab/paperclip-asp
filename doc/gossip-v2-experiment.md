# Experimental Ark gossip v2 integration

Status: design only. This branch does not announce outputs or forward payments
through Ark graph endpoints. It has no production deployment or database changes.

## Proposal baseline

Reference: https://github.com/lightning/bolts/commit/fa75f3529667f67576d84978ece0e39c57db9b50

This reference adds a proposal, not an adopted protocol specification. It proposes
node and funding key vectors, endpoint-indexed updates, and a proof discriminator.
Pin the eventual wire implementation to an exact specification revision.

## Existing boundary

The server uses CLN payment RPCs in `server/src/ln/cln/xpay.rs` and hold invoice
RPCs in `server/src/ln/cln/hold.rs`. Keep this path unchanged. A CLN gossip upgrade
does not by itself require Ark to publish its shared outputs.

`lib/src/tree/signed.rs` builds the tree output scripts. Its expiry clause uses
an expiry height. Do not substitute the relative-timeout example from the proposal.
Derive the actual key set, script root, output key, outpoint and expiry from the
committed tree. Verify each construction before it becomes eligible for a proof.

## Implementation stages

1. Add an isolated, default-disabled experimental module for the announcement
   data model. Do not change existing database formats or wallet RPC contracts.
2. Implement bounded key vectors, canonical endpoint indices, and deterministic
   encoding against a pinned complete gossip v2 specification. Require valid
   curve points and reject malformed lengths and out-of-range endpoint indices.
3. Build a read-only export of an eligible committed tree. Require the intended
   chain hash, a confirmed unspent output and exact output-key reconstruction.
   Export public information only. Do not export secrets or nonces.
4. Define a separate announcement-signature session with participating clients.
   Never reuse transaction nonces or treat an existing transaction signature as
   an announcement signature. Persist session state before signing and make
   retries safe. The server cannot sign for client-held funding keys.
5. Add a matching experimental CLN adapter on an isolated test network. Require
   explicit operator opt-in and peer support before any announcement broadcast.
6. Define and test settlement through an Ark endpoint before advertising usable
   routing capacity. Proof of output control is not proof of payment forwarding.

## Unresolved protocol decisions

- Define how routers model multiple endpoints that share one output. Do not
  represent each pair as independent funded channels.
- Define capacity accounting across concurrent payments and endpoints.
- Define expiry, withdrawal, spend and reorganisation handling. Do not rely on
  delayed gossip pruning to protect payments near an Ark output's expiry.
- Define client consent and privacy implications before exposing round key sets.
- Define the payment and failure protocol, fee policy and timelock safety margins.
- Resolve wire compatibility: the proposal's scalar-to-vector TLV replacement
  must not be assumed byte-compatible with an earlier draft.

## Required validation

Use `nix develop` and the repository test commands before implementation commits.
Test two-endpoint and multi-endpoint vectors; malformed points, lengths and TLVs;
unknown proof types; wrong chains and outpoints; script-root mismatch; expired or
spent outputs; reorgs; shared-capacity contention; partial signature sessions;
restart and retry nonce safety; and explicit disabled-mode behavior.

Run the existing Lightning send and receive regressions. Then test the new wire
format with a matching independent Lightning implementation on an isolated
network. No customer invoices, production keys or real payments belong in this
experiment. Unit tests alone cannot establish routing interoperability.

## Current blocker

No repository Nix development shell was available in the inspected local Windows
environment or Ark host. The repository instructions prohibit code changes until
that environment is confirmed. No implementation or compatibility test has run.
