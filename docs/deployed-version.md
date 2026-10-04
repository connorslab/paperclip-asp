# Deployed version evidence

## Current deployment — October 4, 2026

Running version: `paperclip-asp 0.0.0-beta+7ef5666783c79d27ae4e7aa4f6142a1b9b098194`.

- Source: [7ef5666](https://github.com/connorslab/paperclip-asp/commit/7ef5666783c79d27ae4e7aa4f6142a1b9b098194).
- Image ID: `sha256:3958b2e1b19f2bd94ded8e89c3f3acc52d6448e83c835e620f223006b6cc2378`.
- Registry digest: `sha256:70991cd75b98824d1cd5c4253a3e9e1d5ae2f1b3933d6105314a1109232d9baf`.
- Production build: [37224009125](https://github.com/connorslab/paperclip-asp/actions/runs/37224009125).
- End-to-end checks: [37224007222](https://github.com/connorslab/paperclip-asp/actions/runs/37224007222), seven cases passed, including the released 0.8.2 wallet.

Adds invoice preflight for updated wallets, recorded recovery-cost reimbursement
for eligible Lightning failures before dispatch, and ordinary inbox delivery for
compatible older wallets. Migrations V67/V68 preserve existing recovery records;
they do not backfill historical reimbursements. Recovery reserves and safe delay
limits remain intact. See [recovery costs and eligibility](recovery-costs.md).

Wallet companion: [0.8.3 beta](https://github.com/connorslab/paperclip-wallet-app/releases/tag/v0.8.3-beta.1).
ASP/watchman were updated with 66 seconds of public API downtime. CLN and pool
mining were not restarted. Public TLS/gRPC identity and synchronized chain backend
were verified afterward. Tests are targeted regression coverage, not an audit.

## Earlier deployments

Checked 2026-10-03. Previous deployment (before the small-anchor rollout) reported:

`paperclip-asp 0.0.0-beta+b8c57e6452f602a3a15e47a1b03f218515195085`

Its Docker image ID is:

`sha256:3969e9ce913d7885cc2a6b2b2dfa1c79d2e098807d95d22749ecdaa65a6b6abf`

The runtime has `experimental_funded_lightning` and `experimental_bolt12_receive`
enabled. Operator secrets and wallet data are not published.

The embedded revision is a build declaration, not proof of a reproducible build.
The existing image has no OCI revision label. Do not claim that comparing a
version string proves binary/source identity. New source builds carry source,
revision, and version OCI labels. The build must receive the full source commit
through `GIT_HASH`; record the resulting image digest with deployment evidence.

Verify a deployment with `paperclip-asp --version` inside its container and the
image ID and revision labels from Docker. Review source differences before a
rollout. Do not attach today's documentation revision to an older running binary.

## Small-anchor rollout

The previous small-anchor deployment reported `paperclip-asp 0.0.0-beta+e1d3d451df6f7316b69ac2e2a9f50c98831fd9f4`.
Registry manifest digest: `sha256:10e9de86fc016202e16dd8e79f53495c115deb8da92b7947aa1809534e7e6281`.
See [release validation](small-anchor-validation.md) for test evidence and limitations.
