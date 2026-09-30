# Source build for each native architecture; do not copy Nix-linked binaries into Debian.
FROM rust:1.90.0-bookworm AS build
RUN apt-get update && apt-get install -y --no-install-recommends clang libclang-dev cmake pkg-config protobuf-compiler libssl-dev libsqlite3-dev libpq-dev && rm -rf /var/lib/apt/lists/*
WORKDIR /src
ENV CARGO_BUILD_JOBS=2
ARG GIT_HASH=unknown
ARG SERVER_VERSION=0.0.0-dev
ENV GIT_HASH=${GIT_HASH} SERVER_VERSION=${SERVER_VERSION}
COPY . .
RUN cargo build --locked --release -p bark-server --bin paperclip-asp --bin paperclip-watchman

FROM debian:bookworm-slim
RUN apt-get update && apt-get install -y --no-install-recommends ca-certificates libssl3 libsqlite3-0 libpq5 libgcc-s1 python3 tini && rm -rf /var/lib/apt/lists/* && useradd --uid 1000 --create-home paperclip
COPY --from=build /src/target/release/paperclip-asp /src/target/release/paperclip-watchman /usr/local/bin/
COPY deployment/prepare-config.py /opt/paperclip/deployment/prepare-config.py
COPY deployment/pruned_rpc.py /opt/paperclip/deployment/pruned_rpc.py
COPY deployment/managed-chain.py /opt/paperclip/deployment/managed-chain.py
COPY server/captaind.default.toml server/watchmand.default.toml /opt/paperclip/server/
COPY deployment/asp_operator.py /opt/paperclip/asp_operator.py
COPY deployment/operator-web /opt/paperclip/operator-web
USER 1000:1000
EXPOSE 3535
ENTRYPOINT ["/usr/bin/tini", "--"]
CMD ["paperclip-asp", "--config", "/config/asp.json", "start"]
