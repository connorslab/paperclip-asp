# Deployed version evidence

Checked 2026-10-03. The running Paperclip ASP reports:

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
