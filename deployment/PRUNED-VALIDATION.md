# Pruned backend validation — 2026-09-30

Executed on the isolated FLYNN lab. No mainnet transactions or live node
configuration changes were part of these tests.

- ASP and wallet: `just checks` and workspace builds passed.
- Shared protocol source hashes matched in both repositories.
- Ten adapter tests passed: authentication, method restrictions, wrong block
  rejection, reorg rollback, persistent restart, unavailable peers, coverage
  errors, proved recovery ancestry, and a block arriving during lookup.
- A subsequent bootstrap-status regression test also passed (11 adapter tests
  total): health checks report incomplete coverage without starting a history
  fetch. The deployed worker releases its lock between historical blocks.
- Four real Knots scenarios passed: retrieval of an actually pruned block from
  a full peer, index/cache restart, active-chain replacement, and missing peer
  failure. Both fixture nodes ran with native txindex disabled.
- The Rust coverage-handshake test passed with `rpc-async` enabled.
- Full lifecycle passed with the ASP and wallets routed through the adapter:
  board, Ark transfer, BOLT11 send, BOLT12 send, incoming Lightning settlement,
  and a funded emergency exit with the ASP stopped.

Final Nextest run: `64a23c7b-a507-40d3-93f8-70ee1a83bf62`.
Result: one lifecycle test passed in 23.734 seconds.
An earlier coarse one-block-at-a-time fixture hit its 180-second timeout; the
final fixture uses the established exit helper to advance to required heights.

Knots binary SHA-256:
`d04cd8211e711af989a7a62d0b8b55a8cfe496694392518da0ccb848469b3799`.

The tests establish local compatibility, not production readiness. Production
requires an authenticated private node connection, complete initial indexing,
monitoring and backups, and deployment verification. A peer that retains the
requested historical blocks is required for cache misses. No public RPC
exposure is needed. Native Umbrel/StartOS adapter wiring is a separate
packaging step.
