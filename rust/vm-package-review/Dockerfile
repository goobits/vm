# Build with the Rust workspace as context:
# docker build -f vm-package-review/Dockerfile .
FROM rust:1.97-bookworm AS builder

WORKDIR /source
COPY . .
RUN cargo build --locked --release --package vm-package-review --bin pkg-review && \
    install -m 0755 target/release/pkg-review /usr/local/bin/pkg-review

FROM rust:1.97-bookworm

RUN apt-get update && apt-get install -y --no-install-recommends \
      ca-certificates curl git nodejs npm python3 python3-pip python3-venv && \
    rm -rf /var/lib/apt/lists/* && \
    groupadd --gid 10001 reviewer && \
    useradd --uid 10001 --gid reviewer --create-home reviewer

COPY --from=builder /usr/local/bin/pkg-review /usr/local/bin/pkg-review

USER 10001:10001
WORKDIR /tmp
ENTRYPOINT ["pkg-review"]
