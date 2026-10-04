# Sideflash implementation plan

Status: experimental address codec and authentication implemented. Sideflash is
not enabled in production. See [the development wire profile](sideflash-wire-v1.md).

The `feature/sideflash` branch starts from `main` at `4287d6a`. Its purpose is
Sideflash compatibility between independent Ark servers. Existing Ark addresses,
wallet APIs, BOLT12 offers and recovery rules retain their current behavior.

## Payment model

A Sideflash address contains a native Ark destination and a reusable BOLT12 offer.
Its human-readable prefix is `sfl`; `1` is the separator in `sfl1...`, not a version.
The signed payload identifies the protocol version and chain explicitly.

For the same server identity, the wallet uses the native Ark route. For different
server identities, it verifies the destination binding and uses Lightning. The
recipient receives Ark value. Compare full cryptographic server identities, not
hostnames or short fingerprints. Do not retry through another route while the
first payment outcome is unknown.

Lightning transfers channel balance between servers. The destination server
still needs Ark inventory. Sideflash does not move channel backing directly into
Ark backing, remove recovery costs, or guarantee zero fees.

## Implementation stages

1. Specify the address codec. Assign canonical field identifiers, chain context,
   signature domains, size limits, feature bits and version rules. Use a bounded
   Bech32m envelope and deterministic CBOR. Publish positive and negative test
   vectors before applications depend on the encoding.
2. Bind both destinations. Verify recipient authorization and the destination
   server acknowledgment over the exact Ark destination and offer bytes. Support
   restricted delegation only with a verifiable recipient-policy certificate.
3. Add recipient registration and authenticated payment status. Pin server identity,
   check fresh nonces, expiry and revisions. Address extraction stays offline.
   Do not expose a public directory of recipients.
4. Add durable payment preparation. Bind the invoice, payment hash, destination,
   net receipt, fees and inventory reservation to an immutable payment intent.
   Require idempotent retries and separate Lightning and Ark delivery states.
5. Connect the existing Lightning receive path. Specify and verify the conditions
   that protect recipient delivery before irreversible Lightning settlement.
   A signed address or Lightning preimage alone is not proof of Ark delivery.
   Do not advertise atomic delivery until the conditional VTXO and recovery
   construction is implemented and tested.
6. Integrate wallet route selection, fee approval and status reconciliation.
   Keep the feature opt-in until two independent servers pass the release tests.

## Release tests

- Decode and authenticate shared test vectors across independent implementations.
- Reject malformed encodings, wrong chains, substituted offers, invalid signatures,
  expired bindings, stale revisions and unsupported required features.
- Complete native local payments and Lightning-backed remote payments to the same
  address format. Verify the exact recipient and net amount.
- Reject concurrent double-spends and duplicate intent execution.
- Restart both servers at each durable state transition. Recover lost responses
  without a second payment or a second recipient credit.
- Test expired invoices, unavailable recipients, insufficient inventory, route
  failures, reservation cleanup and conditional recovery.
- Verify old wallet versions and ordinary Ark and Lightning payments still work.
- Review authentication, payment request restrictions, amount arithmetic and
  resource limits before public activation.

No server deployment, wallet release or monetary test is part of this branch.
Server registration, delivery coordination and wallet integration remain to be
implemented. The library supports verified offer extraction for Lightning-only
clients as well as native Ark route selection.
