# Private test results — 2026-09-29

The separated Paperclip wallet and ASP built successfully and passed the actual
`xbt_lifecycle` integration test (20.177 seconds): board, refresh, transfer,
cooperative withdrawal, server-offline unilateral exit, and claim.

The interactive lab's authenticated API transferred 10,000 regtest sats from
Alice to Bob. Missing and invalid tokens returned 401; the disabled mnemonic
endpoint returned 404. The API token was absent from service logs. The lab
restarted using its existing state. A mainnet wallet creation attempt was rejected.

The fixture adapter changes only the old binary-name expectation, Bash interpreter
paths, and the test ASP bind from all interfaces to loopback. It does not bypass
signature, balance or exit assertions. Shared protocol sources match the wallet
repository's `SHARED-SOURCE.sha256`.

This pass uses the explicit test relay settings in `PROTOCOL.json`. Full
default-Knots-policy integration remains unfinished. This is not an independent
security review, a production release, or complete expiry/reorg/fee testing.
