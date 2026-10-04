# Sideflash validation — 2026-10-04

Executed in an isolated Linux workspace under the repository Nix development
environment. No live service configuration changed and no payments were sent.

- `just unit sideflash`: 4 passed, 0 failed.
- `just checks`: passed; existing compiler warnings remain.
- `git diff --check`: passed.
- Full `just unit`: 126 passed, 36 failed, 1 ignored. Repeated against the
  unchanged branch baseline with the Sideflash module absent: the same 36 test
  names failed. Failures include existing script-validation tests. The full
  suite is therefore not a clean release gate in this environment.

The successful focused tests cover canonical encoding, both routes, Lightning-only
extraction, invalid signatures, wrong chain and identity, non-XBT offers, altered
recipients and offers, validity boundaries, malformed CBOR and truncations.

This evidence establishes only an experimental codec and binding verifier.
It does not establish server delivery, recovery safety or monetary interoperability.
