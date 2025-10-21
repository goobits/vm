# Minimal Node.js Base Image
#
# Lightweight base image with just Node.js and essential tools.
# Fast to build, small image size (~500MB vs 2-3GB for full images).
#
# Build:
#   docker build -f minimal-node.dockerfile -t my-minimal-base:latest .
#
# Use in vm.yaml:
#   vm:
#     box_name: my-minimal-base:latest

FROM ubuntu:24.04

LABEL maintainer="VM Tool"
LABEL description="Minimal Node.js base image - lightweight and fast"
LABEL version="1.0"

ENV DEBIAN_FRONTEND=noninteractive

# Install only essential tools
RUN apt-get update && apt-get install -y \
    curl \
    git \
    ca-certificates \
    && rm -rf /var/lib/apt/lists/*

# Install Node.js LTS
RUN curl -fsSL https://deb.nodesource.com/setup_22.x | bash - && \
    apt-get install -y nodejs && \
    rm -rf /var/lib/apt/lists/*

# Install only essential global packages
RUN npm install -g \
    pnpm \
    typescript

WORKDIR /workspace

CMD ["/bin/bash"]
