# syntax=docker/dockerfile:1
# Build with the repository root as context:
# docker build -f rust/vm-package-server/docker/server/Dockerfile .
FROM rust:1.97-bookworm@sha256:0e2bcaef56d041a486784e54104a81aebe0da44bd03019bd70bc0401e42e4a97 AS builder

ARG VM_PACKAGE_BUILD_PROFILE=release
WORKDIR /source
ENV CARGO_TARGET_DIR=/tmp/vm-rust-target
COPY rust/ .
COPY configs/ /configs/
COPY Dockerfile.vibe /Dockerfile.vibe
COPY plugins/vibe-dev/preset.yaml /plugins/vibe-dev/preset.yaml
RUN --mount=type=cache,id=vm-packages-cargo-registry,target=/usr/local/cargo/registry,sharing=locked \
    --mount=type=cache,id=vm-packages-cargo-git,target=/usr/local/cargo/git,sharing=locked \
    --mount=type=cache,id=vm-packages-rust-target,target=/tmp/vm-rust-target,sharing=locked \
    cargo build --locked --profile "$VM_PACKAGE_BUILD_PROFILE" \
      --package vm-package-server \
      --features standalone-binary \
      --bin pkg-server \
      --package vm-package-work \
      --bin pkg-work \
      --package vm-package-git-askpass \
      --bin pkg-git-askpass \
      --package goobits-vm \
      --bin vm && \
    install -m 0755 "$CARGO_TARGET_DIR/$VM_PACKAGE_BUILD_PROFILE/pkg-server" /usr/local/bin/pkg-server && \
    install -m 0755 "$CARGO_TARGET_DIR/$VM_PACKAGE_BUILD_PROFILE/pkg-work" /usr/local/bin/pkg-work && \
    install -m 0755 "$CARGO_TARGET_DIR/$VM_PACKAGE_BUILD_PROFILE/pkg-git-askpass" /usr/local/bin/pkg-git-askpass && \
    install -D -m 0755 "$CARGO_TARGET_DIR/$VM_PACKAGE_BUILD_PROFILE/vm" /usr/local/lib/vm-packages/vm

FROM debian:bookworm-slim@sha256:88200866dfff7ea7f5cbcb6ec7c8a701889efe6fe859fe64d6990e4b07ea4171

RUN apt-get update && apt-get install -y --no-install-recommends \
      ca-certificates curl git && \
    rm -rf /var/lib/apt/lists/* && \
    groupadd --gid 10001 packages && \
    useradd --uid 10001 --gid packages --create-home packages && \
    install -d -o packages -g packages \
      /data/npm /data/cargo /data/pypi /data/tools/artifacts /data/cache \
      /data/state /data/receipts /data/logs /data/agents \
      /data/rollouts /data/sources /data/catalog

COPY --from=builder /usr/local/bin/pkg-server /usr/local/bin/pkg-server
COPY --from=builder /usr/local/bin/pkg-work /usr/local/bin/pkg-work
COPY --from=builder /usr/local/bin/pkg-git-askpass /usr/local/bin/pkg-git-askpass
COPY --from=builder /usr/local/lib/vm-packages/vm /usr/local/lib/vm-packages/vm

USER 10001:10001
WORKDIR /data
EXPOSE 3080 3091
HEALTHCHECK --interval=10s --timeout=3s --start-period=5s --retries=6 \
  CMD curl --fail --silent http://127.0.0.1:3080/health || exit 1

ENTRYPOINT ["pkg-server"]
CMD ["start", "--host", "0.0.0.0", "--port", "3080", "--data", "/data"]
