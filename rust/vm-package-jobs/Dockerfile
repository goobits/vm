# syntax=docker/dockerfile:1
# Build with the Rust workspace as context:
# docker build -f vm-package-jobs/Dockerfile .
FROM rust:1.97-bookworm AS builder

WORKDIR /source
ENV CARGO_TARGET_DIR=/tmp/vm-rust-target
COPY . .
RUN --mount=type=cache,id=vm-packages-cargo-registry,target=/usr/local/cargo/registry,sharing=locked \
    --mount=type=cache,id=vm-packages-cargo-git,target=/usr/local/cargo/git,sharing=locked \
    --mount=type=cache,id=vm-packages-rust-target,target=/tmp/vm-rust-target,sharing=locked \
    cargo build --locked --release --package vm-package-jobs --bins \
      --package vm-package-work --bin pkg-git-askpass && \
    install -m 0755 "$CARGO_TARGET_DIR/release/pkg-review" /usr/local/bin/pkg-review && \
    install -m 0755 "$CARGO_TARGET_DIR/release/pkg-release" /usr/local/bin/pkg-release && \
    install -m 0755 "$CARGO_TARGET_DIR/release/pkg-rollout" /usr/local/bin/pkg-rollout && \
    install -m 0755 "$CARGO_TARGET_DIR/release/pkg-git-askpass" /usr/local/bin/pkg-git-askpass

FROM node:22-bookworm

RUN apt-get update && apt-get install -y --no-install-recommends \
      build-essential ca-certificates curl git libssl-dev pkg-config \
      python3 python3-pip python3-venv && \
    rm -rf /var/lib/apt/lists/* && \
    python3 -m venv /opt/package-tools && \
    /opt/package-tools/bin/pip install --no-cache-dir build twine uv && \
    corepack disable && \
    npm install --global pnpm@10.34.5 yarn@1.22.22 && \
    npm cache clean --force && \
    groupadd --gid 10001 package-job && \
    useradd --uid 10001 --gid package-job --create-home package-job

COPY --from=builder /usr/local/bin/pkg-review /usr/local/bin/pkg-review
COPY --from=builder /usr/local/bin/pkg-release /usr/local/bin/pkg-release
COPY --from=builder /usr/local/bin/pkg-rollout /usr/local/bin/pkg-rollout
COPY --from=builder /usr/local/bin/pkg-git-askpass /usr/local/bin/pkg-git-askpass
COPY --from=builder /usr/local/cargo /usr/local/cargo
COPY --from=builder /usr/local/rustup /usr/local/rustup
COPY vm-package-jobs/pkg-client-env.sh /usr/local/lib/pkg-client-env.sh

USER 10001:10001
ENV PATH="/opt/package-tools/bin:/usr/local/cargo/bin:${PATH}" \
    RUSTUP_HOME=/usr/local/rustup \
    CARGO_HOME=/tmp/cargo-home
WORKDIR /tmp
