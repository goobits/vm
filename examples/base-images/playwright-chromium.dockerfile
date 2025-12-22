# Base image with Playwright and Chromium pre-installed
# Reduces VM creation time from ~5 minutes to ~30 seconds
# Last updated: 2025-12-21

FROM ubuntu:24.04

# Avoid prompts from apt
ENV DEBIAN_FRONTEND=noninteractive

# Install tini for proper zombie process reaping
# Without an init system, zombie processes accumulate in containers
RUN apt-get update && apt-get install -y tini && rm -rf /var/lib/apt/lists/*

# Install Node.js and system dependencies
RUN apt-get update && apt-get install -y \
    curl \
    git \
    build-essential \
    ca-certificates \
    # Playwright/Chromium dependencies (from official Playwright deps list)
    libnss3 \
    libnspr4 \
    libatk1.0-0t64 \
    libatk-bridge2.0-0t64 \
    libatspi2.0-0t64 \
    libcups2t64 \
    libdrm2 \
    libdbus-1-3 \
    libxkbcommon0 \
    libxcomposite1 \
    libxdamage1 \
    libxfixes3 \
    libxrandr2 \
    libgbm1 \
    libasound2t64 \
    libpango-1.0-0 \
    libcairo2 \
    libx11-6 \
    libxcb1 \
    libxext6 \
    libglib2.0-0t64 \
    # WebGL/WebGPU support - Mesa EGL/GL libraries for SwiftShader/SwANGLE backend
    libegl1 \
    libgl1-mesa-dri \
    libgles2-mesa \
    # Vulkan support for WebGPU and SwANGLE (software Vulkan via llvmpipe)
    libvulkan1 \
    mesa-vulkan-drivers \
    # Virtual framebuffer for headless display
    xvfb \
    && rm -rf /var/lib/apt/lists/*

# Install Node.js 22.x
RUN curl -fsSL https://deb.nodesource.com/setup_22.x | bash - \
    && apt-get install -y nodejs \
    && rm -rf /var/lib/apt/lists/*

# Install Playwright with Chromium + Firefox + Chrome for Testing (latest stable)
# - chromium: Default browser (WebGL works on all architectures)
# - firefox: Firefox (WebGL + WebGPU on all architectures including ARM64)
# - chrome: Chrome for Testing (WebGL + WebGPU, x86_64 only - not available for ARM64)
RUN npm install -g playwright \
    && npx playwright install chromium firefox \
    && ([ "$(uname -m)" = "x86_64" ] && npx playwright install chrome || echo "Skipping Chrome (not available for $(uname -m))") \
    && npx playwright install-deps chromium firefox \
    && ([ "$(uname -m)" = "x86_64" ] && npx playwright install-deps chrome || true)

# Install common testing tools
RUN npm install -g \
    @playwright/test \
    typescript

# Create non-root user
RUN useradd -m -s /bin/bash developer

# Environment variables for WebGL/WebGPU software rendering
ENV DISPLAY=:99 \
    # Vulkan ICD for Mesa's llvmpipe software renderer (architecture-aware)
    VK_ICD_FILENAMES=/usr/share/vulkan/icd.d/lvp_icd.$(uname -m).json \
    LIBGL_ALWAYS_SOFTWARE=1

USER developer
WORKDIR /workspace

# Verify installation
RUN npx playwright --version

# Use tini as init to reap zombie processes
ENTRYPOINT ["/usr/bin/tini", "--"]
CMD ["/bin/bash"]
