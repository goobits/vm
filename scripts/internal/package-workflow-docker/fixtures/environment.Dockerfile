FROM node:22-bookworm@sha256:8a34c4ab3ea2c5cd194f07e317b2a8f09461d3c8b05c4e34c8ccd56d56024c4d
ARG DEBIAN_FRONTEND=noninteractive
ARG PROJECT_UID=11000
ARG PROJECT_GID=11000
RUN apt-get update && apt-get install -y --no-install-recommends \
      ansible-core ca-certificates curl git python3 sudo bash tar gzip && \
    rm -rf /var/lib/apt/lists/* && \
    groupadd --non-unique --gid "$PROJECT_GID" acceptance && \
    useradd --non-unique --create-home --uid "$PROJECT_UID" --gid "$PROJECT_GID" --shell /bin/bash acceptance && \
    printf '%s\n' 'acceptance ALL=(ALL) NOPASSWD:ALL' > /etc/sudoers.d/acceptance && \
    chmod 0440 /etc/sudoers.d/acceptance && \
    install -d -o acceptance -g acceptance /workspace && \
    sudo -Hu acceptance git config --global user.name 'VM Acceptance' && \
    sudo -Hu acceptance git config --global user.email 'vm-acceptance@example.invalid'
# Provisioning reconciles detected Node projects even when dependency bootstrap is
# disabled. Bake the real toolchain once so acceptance never downloads it on start.
RUN corepack disable && npm install --global pnpm@10.12.3 && \
    npm cache clean --force && \
    git clone --depth 1 --branch v0.40.3 https://github.com/nvm-sh/nvm.git /home/acceptance/.nvm && \
    mkdir -p /home/acceptance/.nvm/versions/node /home/acceptance/.nvm/alias && \
    cp -a /usr/local "/home/acceptance/.nvm/versions/node/$(node --version)" && \
    printf '%s\n' '22' > /home/acceptance/.nvm/alias/default && \
    chown -R acceptance:acceptance /home/acceptance/.nvm
USER acceptance
ENV PATH="/home/acceptance/.local/bin:${PATH}"
WORKDIR /workspace
CMD ["tail", "-f", "/dev/null"]
