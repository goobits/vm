# ---- Builder Stage ----
FROM rust:1.78 as builder

WORKDIR /usr/src/goobits-pkg-server
COPY . .

# Build the release binary with optimizations
RUN cargo build --release && \
    cp target/release/pkg-server /usr/local/bin/pkg-server

# ---- Final Stage ----
FROM debian:bullseye-slim

# Install necessary runtime dependencies
RUN apt-get update && apt-get install -y \
    ca-certificates \
    curl \
    && rm -rf /var/lib/apt/lists/*

# Copy the binary from the builder stage
COPY --from=builder /usr/local/bin/pkg-server /usr/local/bin/pkg-server

# Create a non-root user
RUN useradd --create-home appuser

# Set up data directory with proper permissions
WORKDIR /home/appuser
RUN mkdir -p data && chown -R appuser:appuser /home/appuser

USER appuser

# Expose the default port
EXPOSE 3080

# Health check to ensure server is running
HEALTHCHECK --interval=30s --timeout=3s --start-period=5s --retries=3 \
    CMD curl -f http://localhost:3080/api/status || exit 1

# Define the entrypoint
ENTRYPOINT ["pkg-server"]

# Default command: start server on all interfaces
CMD ["start", "--host", "0.0.0.0", "--port", "3080", "--data", "./data"]