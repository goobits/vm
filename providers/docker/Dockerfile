FROM ubuntu:24.04

# Build arguments
ARG PROJECT_USER=vagrant
ARG PROJECT_UID=1000
ARG PROJECT_GID=1000

# Prevent interactive prompts during installation
ENV DEBIAN_FRONTEND=noninteractive

# Set locale environment variables to fix locale warnings
ENV LANG=en_US.UTF-8
ENV LC_ALL=en_US.UTF-8
ENV LANGUAGE=en_US:en

# Set default timezone (will be overridden by docker-compose)
ENV TZ=America/Los_Angeles

# Prevent Python from creating __pycache__ directories
ENV PYTHONDONTWRITEBYTECODE=1

# Install minimal base packages needed for Ansible
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
    jq \
    && locale-gen en_US.UTF-8 \
    && update-locale LANG=en_US.UTF-8 \
    && rm -rf /var/lib/apt/lists/*

# Create group and user with specific UID/GID to match host
RUN set -e && \
    # Handle existing user with same UID first
    if getent passwd ${PROJECT_UID} >/dev/null 2>&1; then \
        EXISTING_USER=$(getent passwd ${PROJECT_UID} | cut -d: -f1); \
        userdel $EXISTING_USER; \
    fi && \
    # Ensure group with correct GID exists
    if ! getent group ${PROJECT_GID} >/dev/null 2>&1; then \
        groupadd -g ${PROJECT_GID} ${PROJECT_USER}; \
    fi && \
    GROUP_NAME=$(getent group ${PROJECT_GID} | cut -d: -f1) && \
    # Create user with correct UID and group
    useradd -m -u ${PROJECT_UID} -g ${GROUP_NAME} -s /bin/zsh -G sudo ${PROJECT_USER} && \
    echo "${PROJECT_USER} ALL=(ALL) NOPASSWD:ALL" >> /etc/sudoers

# Configure SSH for Ansible access (optional - can use docker connection plugin instead)
RUN mkdir /var/run/sshd && \
    echo 'root:root' | chpasswd && \
    echo "${PROJECT_USER}:${PROJECT_USER}" | chpasswd && \
    sed -i 's/#PermitRootLogin prohibit-password/PermitRootLogin yes/' /etc/ssh/sshd_config && \
    sed -i 's/#PasswordAuthentication yes/PasswordAuthentication yes/' /etc/ssh/sshd_config

# Set working directory
WORKDIR /workspace

# Copy minimal entrypoint
COPY providers/docker/entrypoint.sh /usr/local/bin/entrypoint.sh
RUN chmod +x /usr/local/bin/entrypoint.sh

# Copy shell setup script
COPY providers/docker/setup-shell.sh /usr/local/bin/setup-shell.sh
RUN chmod +x /usr/local/bin/setup-shell.sh

# Set entrypoint
ENTRYPOINT ["/usr/local/bin/entrypoint.sh"]