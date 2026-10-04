# Bark-like recovery migration: relay gate

Status: research only, 2026-10-03. No server restart, production configuration
change, mainnet transaction, or reinterpretation of existing signed funds.

## Isolated relay results

`python3 scripts/probe-exit-relay.py --bin-dir /path/to/knots/bin` starts fresh
regtest nodes with no peers and disposable wallets. It activates XBT at height
100 and signs with unified sighash. The child supplies 10,000 regtest sats in
package fees. This is a relay probe, not a full Ark exit or security test.

| Construction and node policy | Result |
| --- | --- |
| v3 zero-fee parent, zero-value P2A; default policy | Rejected: dust policy |
| Same; mempooltruc=reject | Rejected: version |
| Same; mempooltruc=enforce, default dust penalty | Rejected: dust policy |
| Same; mempooltruc=enforce, subdustfeepenalty=0 | Package accepted |
| v2 zero-fee parent, 330-sat P2WSH anchor; mempooltruc=reject | Rejected: parent relay fee |

See `bark-relay-probe.jsonl` for raw results. The tested binary is the existing
isolated XBT Knots lab binary. These results do not establish compatibility with
all releases or network-wide relay. Package acceptance alone does not prove
reliable propagation, miner inclusion, or safe recovery against an adversary.

## Backward compatibility requirements

- Keep profile 2 admission, reconstruction, signatures and exits unchanged.
- Introduce a distinct negotiated profile. Do not repurpose profile 2 or the
  existing legacy-v3 encoding as a new protocol identity.
- Keep legacy clients on their supported profile; reject unsupported requests
  before funds are committed. Do not switch a global advertised profile and
  strand older clients.
- Persist the profile with each signed tree and operation. Mixed-profile inputs
  require explicit handling; never assume global configuration describes history.
- Migration uses an explicit cooperative refresh after a wallet shows the new
  recovery requirements. No automatic migration of existing positions.
- A wallet must estimate exit costs and have an independently usable source of
  confirmed fee funds. Recovery cannot depend on the ASP or watchman sponsoring it.
- Retain old binaries and backups for rollback. A rollback must still decode and
  service every profile issued before the rollback; otherwise stop new issuance.

## Release gates

1. Resolve relay compatibility on independent nodes without relying exclusively
   on an operator exception. Document supported policy and a tested fallback.
2. Validate new signatures and fee sponsorship, including anchor theft/pinning,
   partial exits, descendant chains, and watchman protection.
3. Test interrupted exits, restart, eviction, reorg, mixed old/new wallets,
   transfers, refresh, offboard, and both Lightning directions.
4. Build revision-labeled immutable images and rehearse migration/rollback on a
   copied isolated database. Never run lab tests against production data.
5. Conduct a separately authorized tiny mainnet pilot, then stage issuance while
   keeping current service available. Only restart for a validated binary rollout.

The relay gate currently fails for the broad default-policy target. Do not enable
this model in production yet. Smaller weight-based funded budgets remain a
separate possible route to lower costs with ordinary version-2 transactions.
