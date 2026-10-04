# Deployed version evidence

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

The running ASP now reports `paperclip-asp 0.0.0-beta+e1d3d451df6f7316b69ac2e2a9f50c98831fd9f4`.
Registry manifest digest: `sha256:10e9de86fc016202e16dd8e79f53495c115deb8da92b7947aa1809534e7e6281`.
See [release validation](small-anchor-validation.md) for test evidence and limitations.
