# VM Tool Docker Provider - Base Development Container
# This Dockerfile creates a fully-featured development environment with pre-installed tools

FROM ubuntu:24.04

# Build arguments
ARG PROJECT_USER=developer
ARG PROJECT_UID=1000
ARG PROJECT_GID=1000

# Prevent interactive prompts during package installation
ENV DEBIAN_FRONTEND=noninteractive

# Set locale to avoid encoding issues
ENV LANG=en_US.UTF-8
ENV LANGUAGE=en_US:en
ENV LC_ALL=en_US.UTF-8

# Install system packages
RUN apt-get update && apt-get install -y \
    locales \
    python3 \
    python3-pip \
    python3-dev \
    ansible \
    supervisor \
    openssh-server \
    docker.io \
    sudo \
    zsh \
    tzdata \
    && locale-gen en_US.UTF-8 \
    && update-locale LANG=en_US.UTF-8 \
    && rm -rf /var/lib/apt/lists/*

# Set timezone via environment variable (will be provided at runtime)
ENV TZ=UTC

# Create render group for GPU access
RUN groupadd -f render

# Create user with specific UID to match host for file permissions
# SECURITY: Safe handling of existing users/groups across platforms
RUN set -e && \
    # First, handle the "ubuntu" user that exists in Ubuntu base images
    if id ubuntu >/dev/null 2>&1 && [ "${PROJECT_USER}" != "ubuntu" ]; then \
        # Rename the existing ubuntu user to avoid conflicts
        usermod -l ${PROJECT_USER} ubuntu && \
        usermod -d /home/${PROJECT_USER} -m ${PROJECT_USER} && \
        groupmod -n ${PROJECT_USER} ubuntu 2>/dev/null || true; \
    fi && \
    # Now handle UID/GID mapping
    if id ${PROJECT_USER} >/dev/null 2>&1; then \
        # User already exists, update UID if needed
        CURRENT_UID=$(id -u ${PROJECT_USER}); \
        if [ "$CURRENT_UID" != "${PROJECT_UID}" ]; then \
            usermod -u ${PROJECT_UID} ${PROJECT_USER}; \
        fi; \
    else \
        # User doesn't exist, create it
        if getent group ${PROJECT_GID} >/dev/null 2>&1; then \
            # GID exists - reuse it
            EXISTING_GROUP=$(getent group ${PROJECT_GID} | cut -d: -f1); \
            useradd -m -u ${PROJECT_UID} -g ${EXISTING_GROUP} -s /bin/zsh ${PROJECT_USER}; \
        else \
            # GID doesn't exist - create new group
            groupadd -g ${PROJECT_GID} ${PROJECT_USER} && \
            useradd -m -u ${PROJECT_UID} -g ${PROJECT_USER} -s /bin/zsh ${PROJECT_USER}; \
        fi; \
    fi && \
    # Add user to relevant groups for sudo and docker access
    usermod -aG sudo,docker ${PROJECT_USER} && \
    # Set up passwordless sudo using a sudoers.d file for better practice
    echo "${PROJECT_USER} ALL=(ALL) NOPASSWD:ALL" > /etc/sudoers.d/90-nopasswd-user && \
    chmod 0440 /etc/sudoers.d/90-nopasswd-user && \
    # Create directories with proper permissions
    mkdir -p /home/${PROJECT_USER}/.local/state/pipx/log && \
    mkdir -p /home/${PROJECT_USER}/.local/bin && \
    # Set ownership using UID:GID directly
    chown -R ${PROJECT_UID}:${PROJECT_GID} /home/${PROJECT_USER}

# Configure SSH for Ansible access (optional - can use docker connection plugin instead)
# SECURITY: Passwords should be provided via environment variables, not hardcoded
RUN mkdir /var/run/sshd && \
    sed -i 's/#PermitRootLogin prohibit-password/PermitRootLogin no/' /etc/ssh/sshd_config && \
    sed -i 's/#PasswordAuthentication yes/PasswordAuthentication no/' /etc/ssh/sshd_config

# Create workspace directory (will be volume mounted)
RUN mkdir -p /workspace && chown ${PROJECT_UID}:${PROJECT_GID} /workspace

# Create directory for VM tool (will be volume mounted)
RUN mkdir -p /vm-tool

# Switch to the project user
USER ${PROJECT_USER}
WORKDIR /home/${PROJECT_USER}

# Set default shell to zsh
SHELL ["/bin/zsh", "-c"]

# The workspace and vm-tool directories will be mounted at runtime
VOLUME ["/workspace", "/vm-tool"]

# Default to a shell session
CMD ["/bin/zsh"]