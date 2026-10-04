# Compact Sideflash v1

New recipient authorizations and server acknowledgments use compact v1. The
reader accepts both v1 and legacy v0. `SideflashAddress::version()` identifies
the decoded format. Re-encoding a v0 address retains its original wire format
and signature domains. Conversion requires fresh signatures from both parties.

See the [protocol v1 specification](https://github.com/connorslab/sideflash/blob/main/docs/address-v1.md)
for the exact array positions, immutable local network codes and signature
digests. All payment details remain embedded. No address lookup is required.
The binary Ark destination, full server key, complete offer and both signatures
remain present. The limits remain 633 payload bytes and 1023 text characters.

The shared frozen fixtures measure 785 mainnet characters and 841 regtest
characters. Legacy fixtures measure 911 and 967. The codec rejects unknown
profiles, wrong container/version combinations and copied v0 signatures.
Network codes expand to the full genesis and fork discriminator before the
existing native-network, service-identity and BOLT12 checks.

This is feature-branch code. It does not enable a registration endpoint, complete
cross-server payments or production use. Existing stable wallet releases do not
support v1. Negotiate support explicitly; never silently downgrade an address.
