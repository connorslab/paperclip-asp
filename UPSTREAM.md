# Source provenance

Based on Bark by Second and the Bark contributors (MIT).
Original: https://gitlab.com/ark-bitcoin/bark
XBT source: https://github.com/connorslab/bark-xbt
Source commit: `e107d09e7fd173fc5895a2b9eec500082f48d4cd`

Only the application and its local dependency closure are copied. Shared crates
remain source-compatible and are vendored in each repository. Keep their protocol
changes synchronized. Original copyright and dependency licenses are retained.

## Board funding validation

The board funding-output check is backported from upstream commit
`14d92d427c9cc886fdfe8f071c6d6d4e57257ecd` in `server-0.7.1-patch.4`.
Registration must verify both the funding script and its amount. The script must
commit to the user and server keys. The Paperclip adaptation includes the miner
fee reserve in the expected amount, preserving existing funded board formats.
The regression covers legacy and funded formats, incorrect amounts, and an
out-of-range funding output index. This is a targeted backport, not a merge of
all changes in that upstream bundle.
