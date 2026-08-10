# Build with the Rust workspace as context:
# docker build -f vm-package-server/docker/server/Dockerfile .
FROM rust:1.97-bookworm AS builder

WORKDIR /source
COPY . .
RUN cargo build --locked --release \
      --package vm-package-server \
      --features standalone-binary \
      --bin pkg-server \
      --package vm-package-work \
      --bin pkg-work \
      --bin pkg-git-askpass && \
    install -m 0755 target/release/pkg-server /usr/local/bin/pkg-server && \
    install -m 0755 target/release/pkg-work /usr/local/bin/pkg-work && \
    install -m 0755 target/release/pkg-git-askpass /usr/local/bin/pkg-git-askpass

FROM debian:bookworm-slim

RUN apt-get update && apt-get install -y --no-install-recommends ca-certificates curl git && \
    rm -rf /var/lib/apt/lists/* && \
    groupadd --gid 10001 packages && \
    useradd --uid 10001 --gid packages --create-home packages && \
    install -d -o packages -g packages \
      /data/npm /data/cargo /data/pypi /data/cache /data/metadata \
      /data/state /data/receipts /data/logs /data/agents

COPY --from=builder /usr/local/bin/pkg-server /usr/local/bin/pkg-server
COPY --from=builder /usr/local/bin/pkg-work /usr/local/bin/pkg-work
COPY --from=builder /usr/local/bin/pkg-git-askpass /usr/local/bin/pkg-git-askpass

USER 10001:10001
WORKDIR /data
EXPOSE 3080 3091
HEALTHCHECK --interval=10s --timeout=3s --start-period=5s --retries=6 \
  CMD curl --fail --silent http://127.0.0.1:3080/health || exit 1

ENTRYPOINT ["pkg-server"]
CMD ["start", "--host", "0.0.0.0", "--port", "3080", "--data", "/data"]
