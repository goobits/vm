# Super Cool Vibes - Streamlined Development Environment
#
# Essential tools for modern development: Playwright, Chromium, Node.js, Rust, Python
# Lean and fast - only what you actually need!
#
# Build:
#   docker build -f super-cool-vibes.dockerfile -t super-cool-vibes:latest .
#
# Use in vm.yaml:
#   vm:
#     box_name: super-cool-vibes:latest

FROM ubuntu:24.04

LABEL maintainer="VM Tool"
LABEL description="Super Cool Vibes - Streamlined dev environment with Playwright, Node, Rust, Python"
LABEL version="2.0"

ENV DEBIAN_FRONTEND=noninteractive

# ============================================================================
# SYSTEM PACKAGES - Essentials only
# ============================================================================
RUN apt-get update && apt-get install -y \
    # Core utilities
    curl \
    wget \
    git \
    vim \
    ca-certificates \
    # Build essentials (needed for native packages)
    build-essential \
    pkg-config \
    libssl-dev \
    mold \
    # CLI utilities
    tree \
    htop \
    jq \
    # Chromium and browser dependencies (for Playwright)
    chromium-browser \
    fonts-liberation \
    fonts-noto-color-emoji \
    libasound2 \
    libatk-bridge2.0-0 \
    libatk1.0-0 \
    libatspi2.0-0 \
    libcups2 \
    libdbus-1-3 \
    libdrm2 \
    libgbm1 \
    libgtk-3-0 \
    libnspr4 \
    libnss3 \
    libwayland-client0 \
    libxcomposite1 \
    libxdamage1 \
    libxfixes3 \
    libxkbcommon0 \
    libxrandr2 \
    xdg-utils \
    xvfb \
    # Python 3.12
    python3 \
    python3-pip \
    python3-venv \
    && rm -rf /var/lib/apt/lists/*

# ============================================================================
# NODE.JS - Install LTS version
# ============================================================================
RUN curl -fsSL https://deb.nodesource.com/setup_22.x | bash - && \
    apt-get install -y nodejs && \
    rm -rf /var/lib/apt/lists/*

# ============================================================================
# BUN - Modern JavaScript runtime and package manager
# ============================================================================
RUN curl -fsSL https://bun.sh/install | bash && \
    ln -s /root/.bun/bin/bun /usr/local/bin/bun

# ============================================================================
# RUST & CARGO - Install via rustup
# ============================================================================
RUN curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh -s -- -y && \
    . "$HOME/.cargo/env" && \
    rustup default stable

ENV PATH="/root/.cargo/bin:${PATH}"

# ============================================================================
# CARGO TOOLS - Development & Testing Utilities
# ============================================================================
RUN . "$HOME/.cargo/env" && \
    cargo install cargo-nextest && \
    cargo install cargo-watch && \
    cargo install cargo-udeps

# ============================================================================
# PLAYWRIGHT - The slow part (5-10 minutes)
# ============================================================================
RUN npm install -g playwright@latest && \
    npx playwright install --with-deps chromium

# ============================================================================
# NPM PACKAGES - Essentials only
# ============================================================================
RUN npm install -g \
    # Package managers
    pnpm \
    yarn \
    # TypeScript ecosystem
    typescript \
    tsx \
    # Code quality
    prettier \
    eslint \
    # Development tools
    jspcd

# ============================================================================
# ENVIRONMENT SETUP
# ============================================================================
ENV DISPLAY=:99
ENV NODE_ENV=development
ENV PLAYWRIGHT_BROWSERS_PATH=/usr/local/share/playwright
ENV PATH="/workspace/node_modules/.bin:${PATH}"

# Set up git defaults
RUN git config --system init.defaultBranch main && \
    git config --system pull.rebase false

# Create working directory
WORKDIR /workspace

# ============================================================================
# WELCOME MESSAGE
# ============================================================================
RUN echo '#!/bin/bash' > /usr/local/bin/welcome && \
    echo 'cat << "EOF"' >> /usr/local/bin/welcome && \
    echo '╔═══════════════════════════════════════════════════════════╗' >> /usr/local/bin/welcome && \
    echo '║                                                           ║' >> /usr/local/bin/welcome && \
    echo '║              🎭 SUPER COOL VIBES 🎭                       ║' >> /usr/local/bin/welcome && \
    echo '║                                                           ║' >> /usr/local/bin/welcome && \
    echo '║  Node.js ✓  Bun ✓  Rust ✓  Python ✓  Playwright ✓        ║' >> /usr/local/bin/welcome && \
    echo '║  TypeScript ✓  Chromium ✓  Essential Tools ✓             ║' >> /usr/local/bin/welcome && \
    echo '║                                                           ║' >> /usr/local/bin/welcome && \
    echo '║  🦀 Cargo Tools: nextest · watch · udeps · mold          ║' >> /usr/local/bin/welcome && \
    echo '║                                                           ║' >> /usr/local/bin/welcome && \
    echo '╚═══════════════════════════════════════════════════════════╝' >> /usr/local/bin/welcome && \
    echo 'EOF' >> /usr/local/bin/welcome && \
    chmod +x /usr/local/bin/welcome

# Add welcome to .bashrc
RUN echo 'welcome' >> /etc/bash.bashrc

# Health check
HEALTHCHECK --interval=30s --timeout=10s --start-period=5s --retries=3 \
    CMD node -e "console.log('Health:', require('playwright') ? 'OK' : 'FAIL')" || exit 1

CMD ["/bin/bash"]
