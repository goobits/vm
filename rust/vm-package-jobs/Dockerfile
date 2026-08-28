# syntax=docker/dockerfile:1
# Build with the repository root as context:
# docker build -f rust/vm-package-jobs/Dockerfile .
FROM rust:1.98-bookworm@sha256:82150a52ec202c1b14d7817e14516c392bb7f5cfebd88f1ed531cb37ebd39922 AS builder

ARG VM_PACKAGE_BUILD_PROFILE=release
WORKDIR /source
ENV CARGO_TARGET_DIR=/tmp/vm-rust-target
COPY rust/ .
RUN --mount=type=cache,id=vm-packages-cargo-registry,target=/usr/local/cargo/registry,sharing=locked \
    --mount=type=cache,id=vm-packages-cargo-git,target=/usr/local/cargo/git,sharing=locked \
    --mount=type=cache,id=vm-packages-rust-target,target=/tmp/vm-rust-target,sharing=locked \
    cargo build --locked --profile "$VM_PACKAGE_BUILD_PROFILE" \
      --package vm-package-jobs --bins \
      --package vm-package-git-askpass --bin pkg-git-askpass && \
    install -m 0755 "$CARGO_TARGET_DIR/$VM_PACKAGE_BUILD_PROFILE/pkg-review" /usr/local/bin/pkg-review && \
    install -m 0755 "$CARGO_TARGET_DIR/$VM_PACKAGE_BUILD_PROFILE/pkg-build" /usr/local/bin/pkg-build && \
    install -m 0755 "$CARGO_TARGET_DIR/$VM_PACKAGE_BUILD_PROFILE/pkg-release" /usr/local/bin/pkg-release && \
    install -m 0755 "$CARGO_TARGET_DIR/$VM_PACKAGE_BUILD_PROFILE/pkg-rollout" /usr/local/bin/pkg-rollout && \
    install -m 0755 "$CARGO_TARGET_DIR/$VM_PACKAGE_BUILD_PROFILE/pkg-git-askpass" /usr/local/bin/pkg-git-askpass

FROM node:22-bookworm@sha256:8a34c4ab3ea2c5cd194f07e317b2a8f09461d3c8b05c4e34c8ccd56d56024c4d

RUN apt-get update && apt-get install -y --no-install-recommends \
      build-essential ca-certificates curl git libssl-dev pkg-config \
      python3 python3-pip python3-venv && \
    rm -rf /var/lib/apt/lists/* && \
    python3 -m venv /opt/package-tools && \
    /opt/package-tools/bin/pip install --no-cache-dir \
      build==1.6.0 twine==7.0.0 uv==0.12.7 && \
    corepack disable && \
    npm install --global pnpm@10.34.5 yarn@1.22.22 && \
    npm cache clean --force && \
    groupadd --gid 10001 package-job && \
    useradd --uid 10001 --gid package-job --create-home package-job && \
    groupadd --gid 10002 package-build && \
    useradd --uid 10002 --gid package-build --no-create-home package-build && \
    install -d -o root -g root -m 0700 /run/build-secrets

COPY --from=builder /usr/local/bin/pkg-review /usr/local/bin/pkg-review
COPY --from=builder /usr/local/bin/pkg-build /usr/local/bin/pkg-build
COPY --from=builder /usr/local/bin/pkg-release /usr/local/bin/pkg-release
COPY --from=builder /usr/local/bin/pkg-rollout /usr/local/bin/pkg-rollout
COPY --from=builder /usr/local/bin/pkg-git-askpass /usr/local/bin/pkg-git-askpass
COPY --from=builder /usr/local/cargo /usr/local/cargo
COPY --from=builder /usr/local/rustup /usr/local/rustup
COPY rust/vm-package-jobs/pkg-client-env.sh /usr/local/lib/pkg-client-env.sh

USER 10001:10001
ENV PATH="/opt/package-tools/bin:/usr/local/cargo/bin:${PATH}" \
    RUSTUP_HOME=/usr/local/rustup \
    CARGO_HOME=/tmp/cargo-home
WORKDIR /tmp
