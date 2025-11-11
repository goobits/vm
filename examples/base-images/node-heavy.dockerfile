# Base image with Node.js and common heavy dependencies pre-installed
# Reduces VM creation time from ~3 minutes to ~30 seconds
# Last updated: 2025-01-11

FROM ubuntu:24.04

# Install Node.js and system dependencies
RUN apt-get update && apt-get install -y \
    curl \
    git \
    build-essential \
    python3 \
    ca-certificates \
    && rm -rf /var/lib/apt/lists/*

# Install Node.js 22.x
RUN curl -fsSL https://deb.nodesource.com/setup_22.x | bash - \
    && apt-get install -y nodejs \
    && rm -rf /var/lib/apt/lists/*

# Install common heavy global packages
# These are time-consuming installations we're pre-caching
RUN npm install -g \
    typescript@5.3.3 \
    ts-node@10.9.2 \
    eslint@8.56.0 \
    prettier@3.1.1 \
    webpack@5.89.0 \
    webpack-cli@5.1.4 \
    vite@5.0.8 \
    turbo@1.11.2 \
    pnpm@8.14.0

# Install common build tools and dependencies
# Pre-cache native modules that take time to compile
RUN mkdir -p /tmp/prebuild && cd /tmp/prebuild \
    && npm init -y \
    && npm install --no-save \
        esbuild@0.19.11 \
        @swc/core@1.3.101 \
        sharp@0.33.1 \
    && rm -rf /tmp/prebuild

# Create non-root user
RUN useradd -m -s /bin/bash developer
USER developer
WORKDIR /workspace

# Verify installation
RUN node --version && npm --version && pnpm --version
