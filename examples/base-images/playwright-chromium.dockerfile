# Base image with Playwright and Chromium pre-installed
# Reduces VM creation time from ~5 minutes to ~30 seconds
# Last updated: 2025-01-11

FROM ubuntu:24.04

# Install Node.js and system dependencies
RUN apt-get update && apt-get install -y \
    curl \
    git \
    build-essential \
    ca-certificates \
    && rm -rf /var/lib/apt/lists/*

# Install Node.js 22.x
RUN curl -fsSL https://deb.nodesource.com/setup_22.x | bash - \
    && apt-get install -y nodejs \
    && rm -rf /var/lib/apt/lists/*

# Install Playwright with Chromium
# This is the time-consuming step we're pre-caching
RUN npm install -g playwright@1.40.0 \
    && npx playwright install chromium \
    && npx playwright install-deps chromium

# Install common testing tools
RUN npm install -g \
    @playwright/test@1.40.0 \
    typescript@5.3.3

# Create non-root user
RUN useradd -m -s /bin/bash developer
USER developer
WORKDIR /workspace

# Verify installation
RUN npx playwright --version
