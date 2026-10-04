# Hybrid address proof of concept

This experiment creates a signed address and selects a payment route. It does not send payments, resolve servers, reserve liquidity or prove Ark delivery. The `hyark` prefix is private to this experiment. Existing wallets do not support it.

## Run

In `nix develop`, run:

```sh
cargo run --locked -p ark-lib --example hybrid_address -- demo
just unit hybrid_
```

The demo creates a native regtest Ark address and a syntactically valid regtest BOLT12 offer. It signs the binding with the recipient key and countersigns it with the destination server key. It prints the hybrid address and both routes. All fixture keys are public. Time 100 and expiry 200 are synthetic Unix times. There is no live receiving service. Do not fund these destinations.

Decode that sample with:

```sh
cargo run --locked -p ark-lib --example hybrid_address -- decode ADDRESS TRUSTED_DESTINATION_KEY LOCAL_ASP_KEY 100
```

Use the keys printed by the demo. A matching local server returns `Ark(native_address)`. Another local server returns `Bolt12(offer)`. Pass time 200 to verify expiration rejection. Decoding alone does not authenticate an address; route selection verifies both signatures.

## Experimental binary format

The Bech32m envelope uses prefix `hyark`. It contains version 1, a one-byte network profile, a 33-byte destination server key, an eight-byte big-endian expiry, two CompactSize-prefixed UTF-8 strings (native address and offer), a 64-byte recipient signature and a 64-byte server acknowledgment. Fields are ordered as listed. Strings are limited to 1,024 bytes each. Encoded text is limited to 4,096 characters. Unknown versions, malformed lengths, trailing bytes and noncanonical payloads are rejected. This codec is an experiment, not the CBOR wire proposal in the whitepaper.

The recipient signature reuses `RecipientBinding` and binds the network profile, full server key, address, exact offer text and expiry. The server acknowledgment signs a separate domain and includes the recipient signature. Mapping lifetime is at most 24 hours. There is no key delegation, revision or revocation mechanism yet. A future profile must define those before reusable production deployment.

## Trust and payment boundary

Callers supply the trusted destination key separately. Never obtain that trust solely from an untrusted decoder result. Native Ark addresses have a four-byte server identifier; this is not a substitute for a trusted full key. Bootstrap trust from a recipient-authenticated destination or a pinned server descriptor.

Only simple public-key receiving policies are supported. Route selection verifies address network, full server identity, both signatures, mapping expiry, offer parsing, offer chain and offer expiry. The outer profile distinguishes the XBT fork; chain hashes alone do not distinguish forks with shared genesis. Feature-bit and live node compatibility checks still belong to invoice/payment negotiation. The fixture offer does not exercise XBT-specific feature negotiation.

Both local and remote routes require a valid complete binding. A stale remote offer does not silently fall back to another route. Before live use, implement authenticated freshness, invoice-to-recipient binding, fee approval, replay-safe payment intents and conditional Ark delivery. The output of `route` is a destination suggestion, not payment authorization.
