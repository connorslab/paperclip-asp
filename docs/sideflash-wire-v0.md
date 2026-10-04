# Legacy Sideflash wire profile v0

This profile remains readable. New bindings use [compact v1](sideflash-wire-v1.md).

This profile targets bitcoin (XBT), not SHA-256 BTC. The development fork
discriminator is SHA256 of ASCII `Sideflash/XBT/blake2b-unified-sighash/v0` (no
terminating NUL). `ChainContext::xbt` accepts XBT mainnet and isolated XBT regtest.
The underlying network enum uses `Bitcoin` and `Regtest`; these names do not
authorize SHA-256 BTC payments. The verifier also requires XBT BOLT12 identity
feature bit 512 and rejects unknown mandatory offer features. Shared genesis or
address prefixes alone are not sufficient network discrimination.

This is an experimental encoding. Do not distribute these addresses as stable
payment destinations. The library does not execute payments or enable a server API.

## Envelope

Use the `sfl` human-readable prefix and Bech32m checksum. Writers produce lowercase
text. Readers accept uppercase but reject mixed case. The `1` is a separator.
Limit input to 1023 ASCII characters and decoded payloads to 633 bytes. This
profile deliberately exceeds the original Bech32 address length limit.

The payload is a deterministic CBOR map. Require ascending unsigned integer keys,
shortest lengths and integers, definite lengths, and no duplicate or extra keys.
Reject trailing bytes. Individual byte strings have a 1024-byte limit.

| Key | Type | Meaning |
| --- | --- | --- |
| 0 | unsigned integer | Version: 0 |
| 1 | unsigned integer | Required features: 0; reject all other values |
| 2 | 32-byte string | Chain genesis hash in Lightning chain-hash byte order |
| 3 | 32-byte string | Fork discriminator from the configured chain profile |
| 4 | 33-byte string | Compressed destination server public key |
| 5 | byte string | Native address: network flag byte, version byte 1, canonical Ark payload |
| 6 | byte string | Exact decoded BOLT12 offer TLV bytes |
| 7 | unsigned integer | Mapping revision |
| 8 | unsigned integer | Inclusive validity start, Unix seconds |
| 9 | unsigned integer | Exclusive expiry, Unix seconds |
| 10 | 64-byte string | Recipient BIP340 signature |
| 11 | 64-byte string | Server BIP340 acknowledgment |

The first profile supports native pubkey policies only. Delegated signing keys,
resolver hints and other receiving policies require a subsequent profile. The
native address field starts with a network flag (0 for mainnet, 1 for regtest),
then a policy-address version byte (1), then the canonical native address payload
without its HRP, five-bit version or checksum. Re-encoding must reproduce the
exact field. This replaces the initial unreleased text-byte prototype.

## Signatures

The recipient signs SHA256 of ASCII `Sideflash/recipient/v0` followed by a NUL
byte and the canonical CBOR map of fields 0 through 9 (map size 10). The server
signs SHA256 of ASCII `Sideflash/server/v0` followed by a NUL byte and the canonical
map of fields 0 through 10 (map size 11). The server signature thus commits to
the recipient signature. Verify BIP340 with the respective x-only public keys.

`Binding::authorize` needs only the recipient key. `SideflashAddress::acknowledge`
verifies that authorization and needs only the server key. No component needs
both secret keys. The wire envelope contains neither secret key.

## Verification and routing

`decode` checks structure and canonical encoding only. `verified_offer` checks
signatures, the externally pinned server key, chain context, native server
fingerprint, network flag, offer chain, offer expiry and envelope validity.
The caller must obtain its chain profile and server pin from trusted context.
Do not use a short native server fingerprint as the sole trust anchor. Internal
signatures cannot detect replacement of the entire address with an attacker's
valid address. Deliver the address through an authenticated recipient channel.

`route` uses the full server public keys to select native Ark or the BOLT12 offer.
It does not contact servers or authorize a payment. Revocation freshness,
delivery preparation, invoice validation and amount approval remain mandatory
application steps. Network-specific invoice feature validation remains part of
the existing Lightning payment path. The fork discriminator is not a substitute
for that validation. Chain-profile identifiers still need interoperable agreement.

## Lightning-only clients

A Lightning wallet can call `decode` and `verified_offer` without a local Ark
server. It receives ordinary BOLT12 TLV bytes and can serialize them as an `lno`
offer using its Lightning library. A standalone implementation needs only the
bounded codec, pubkey-policy parser, SHA256, BIP340 and BOLT12 offer validation.
Existing Lightning software does not automatically recognize `sfl1` addresses.

Safe Ark delivery must still occur on the receiving side before settlement.
The initial implementation does not yet provide that server coordination. A
future receiver-managed profile can hide this coordination from a standard
Lightning sender, but must prove recovery safety before it is advertised.

## Validation

Run `just unit sideflash` and `just checks`. Tests cover both route choices,
Lightning-only extraction, validity boundaries, altered offers and recipients,
signature substitution, wrong identity and fork context, truncations, unknown
versions and features, and noncanonical CBOR. No monetary test is performed by
these unit tests. Cross-implementation vectors and live delivery tests remain
release requirements.
