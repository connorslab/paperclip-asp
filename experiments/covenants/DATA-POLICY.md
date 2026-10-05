# Covenant data-policy characterization

These are small **private-regtest** probes against the pinned experimental
Bitcoin node (`2f142fdd75719d23046be5b1577fc9ef280f3dc3`), with active RDTS rules
and default relay policy. Payload spends were submitted to `testmempoolaccept`,
not broadcast on the public signet. This is not an exhaustive spam or denial-of-service review.

## What was observed

| Construction | Message bytes | Accepted | Spend vsize |
| --- | ---: | --- | ---: |
| CSFS message in witness | 32 | Yes | 137 |
| CSFS message in witness | 80 | Yes | 149 |
| CSFS message in witness | 81 | No | — |
| CSFS message in executed script | 80 | Yes | 149 |
| CSFS message in executed script | 256 | Yes | 194 |
| CSFS message in executed script | 257 | No | — |
| Existing SHA256 preimage in witness | 32 | Yes | 121 |
| Existing SHA256 preimage in witness | 80 | Yes | 133 |
| Existing SHA256 preimage in witness | 81 | No | — |
| Existing SHA256 preimage in executed script | 80 | Yes | 133 |
| Existing SHA256 preimage in executed script | 256 | Yes | 178 |
| Existing SHA256 preimage in executed script | 257 | No | — |

Each spend paid a fixed 1,000-sat test fee. These are transaction virtual sizes,
not measured minimum accepted fees, total funding-plus-spend costs, or throughput
benchmarks. The 81-byte witness case failed `bad-witness-taproot-stackitem-size`.
The 257-byte script case failed the push-size rule. Bounds are per item, not a
claim that a transaction can contain only 80 or 256 bytes of data in total.

## Interpretation

CSFS verifies a signature over a supplied byte string. The opcode does not require
that string to be a TEMPLATEHASH result, even though the Ark scripts use it that
way. A valid signature over arbitrary data is therefore permitted within the
existing constraints. Script size, transaction/block weight, fees, RDTS push
limits, Taproot witness policy and per-input signature-validation budgets still
apply. Keeping those rules does not establish that only payment data can be stored.

The equivalent old SHA256 constructions accepted the same tested payload sizes
and were 16 vbytes smaller. Thus these samples do not demonstrate cheaper data
storage from CSFS. They also do not establish the absence of more efficient
combinations, repeated/multiple-item payloads, policy-classification gaps or
computational denial-of-service patterns.

TEMPLATEHASH returns a fixed 32-byte commitment derived from transaction fields;
it does not itself add a free-form data field. General scripts can still combine
it with other opcodes or outputs. A transaction commitment needed to enforce a
payment rule is not automatically equivalent to an arbitrary payload, but that
distinction cannot be established from hash length alone.

No consensus or relay-policy restriction was changed as part of these probes.
In particular, restricting CSFS to 32-byte messages would change its semantics
and would still permit arbitrary 32-byte strings. Such a restriction is not a
complete spam defense and would need separate specification and review.

## Reproduce

Set `COVENANT_NODE_SOURCE`, `COVENANT_NODE_CONFIG` and `COVENANT_RESULTS` as
described in the laboratory README. Run `just covenant-data-policy` in the Nix
development shell. The driver creates a fresh private chain and disposable keys.
See `test_data_policy.py`; evidence is saved as JSON outside the source tree.
