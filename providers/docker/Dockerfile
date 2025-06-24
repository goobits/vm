FROM ubuntu:22.04

# Build arguments
ARG PROJECT_USER=vagrant

# Prevent interactive prompts during installation
ENV DEBIAN_FRONTEND=noninteractive

# Set locale environment variables to fix locale warnings
ENV LANG=en_US.UTF-8
ENV LC_ALL=en_US.UTF-8
ENV LANGUAGE=en_US:en

# Set default timezone (will be overridden by docker-compose)
ENV TZ=America/Los_Angeles

# Install minimal base packages needed for Ansible
RUN apt-get update && apt-get install -y \
    python3 \
    python3-pip \
    python3-dev \
    ansible \
    supervisor \
    openssh-server \
    sudo \
    && rm -rf /var/lib/apt/lists/*

# Create user with sudo privileges
RUN useradd -m -s /bin/bash -G sudo ${PROJECT_USER} && \
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

# Set entrypoint
ENTRYPOINT ["/usr/local/bin/entrypoint.sh"]