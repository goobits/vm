FROM ubuntu:22.04

# Build arguments
ARG NODE_VERSION=22
ARG NVM_VERSION=v0.39.7
ARG PNPM_VERSION=9.15.4
ARG PROJECT_NAME=myproject
ARG PROJECT_USER=vagrant
ARG TERMINAL_THEME=dracula
ARG TERMINAL_EMOJI=🚀
ARG TERMINAL_USERNAME=dev

# Prevent interactive prompts during installation
ENV DEBIAN_FRONTEND=noninteractive
ENV LANG=en_US.UTF-8
ENV LANGUAGE=en_US:en
ENV LC_ALL=en_US.UTF-8

# Install base system packages
RUN apt-get update && apt-get install -y \
    locales \
    locales-all \
    curl \
    wget \
    git \
    git-lfs \
    build-essential \
    python3-pip \
    python3-dev \
    python3-venv \
    python3-psycopg2 \
    jq \
    htop \
    tree \
    ncdu \
    zsh \
    zsh-syntax-highlighting \
    ripgrep \
    sudo \
    openssh-server \
    netcat \
    net-tools \
    postgresql-client \
    && rm -rf /var/lib/apt/lists/*

# Generate locale
RUN locale-gen en_US.UTF-8

# Create user with sudo privileges
RUN useradd -m -s /usr/bin/zsh -G sudo ${PROJECT_USER} && \
    echo "${PROJECT_USER} ALL=(ALL) NOPASSWD:ALL" >> /etc/sudoers

# Install headless browser tools if enabled (build arg)
ARG HEADLESS_BROWSER_ENABLED=false
RUN if [ "$HEADLESS_BROWSER_ENABLED" = "true" ]; then \
        apt-get update && \
        apt-get install -y chromium-browser xvfb && \
        rm -rf /var/lib/apt/lists/*; \
    fi

# Switch to user for NVM installation
USER ${PROJECT_USER}
WORKDIR /home/${PROJECT_USER}

# Install NVM and Node.js
RUN curl -o- https://raw.githubusercontent.com/nvm-sh/nvm/${NVM_VERSION}/install.sh | bash && \
    export NVM_DIR="$HOME/.nvm" && \
    [ -s "$NVM_DIR/nvm.sh" ] && \. "$NVM_DIR/nvm.sh" && \
    nvm install ${NODE_VERSION} && \
    nvm use ${NODE_VERSION} && \
    nvm alias default ${NODE_VERSION}

# Setup shell configuration
RUN echo 'export NVM_DIR="$HOME/.nvm"' >> ~/.zshrc && \
    echo '[ -s "$NVM_DIR/nvm.sh" ] && \. "$NVM_DIR/nvm.sh"' >> ~/.zshrc && \
    echo '[ -s "$NVM_DIR/bash_completion" ] && \. "$NVM_DIR/bash_completion"' >> ~/.zshrc

# Setup locale in shell
RUN echo 'export LANG=en_US.UTF-8' >> ~/.zshrc && \
    echo 'export LANGUAGE=en_US:en' >> ~/.zshrc && \
    echo 'export LC_ALL=en_US.UTF-8' >> ~/.zshrc && \
    echo 'export LC_CTYPE=en_US.UTF-8' >> ~/.zshrc

# Install pnpm
RUN export NVM_DIR="$HOME/.nvm" && \
    [ -s "$NVM_DIR/nvm.sh" ] && \. "$NVM_DIR/nvm.sh" && \
    corepack enable && \
    corepack prepare pnpm@${PNPM_VERSION} --activate

# Switch back to root to copy files
USER root

# Copy theme file and terminal setup script
COPY shared/themes.json /tmp/themes.json
COPY providers/docker/scripts/setup-terminal.sh /tmp/setup-terminal.sh
RUN chmod +x /tmp/setup-terminal.sh

# Switch back to user
USER ${PROJECT_USER}

# Setup terminal with full theme support
ARG SHOW_GIT_BRANCH=true
ARG SHOW_TIMESTAMP=false
RUN /tmp/setup-terminal.sh "${TERMINAL_THEME}" "${TERMINAL_EMOJI}" "${TERMINAL_USERNAME}" "${SHOW_GIT_BRANCH}" "${SHOW_TIMESTAMP}"

# Cleanup
USER root
RUN rm -f /tmp/setup-terminal.sh

# Switch back to user
USER ${PROJECT_USER}

# Create Claude settings directory
RUN mkdir -p ~/.claude

# Switch back to root for final setup
USER root

# Copy entrypoint and other scripts
COPY providers/docker/entrypoint.sh /usr/local/bin/entrypoint.sh
COPY providers/docker/scripts/*.sh /vm-tool/providers/docker/scripts/
RUN chmod +x /usr/local/bin/entrypoint.sh /vm-tool/providers/docker/scripts/*.sh

# Set working directory
WORKDIR /workspace

# Set the user back to PROJECT_USER
USER ${PROJECT_USER}

# Set entrypoint
ENTRYPOINT ["/usr/local/bin/entrypoint.sh"]
CMD ["/bin/zsh"]