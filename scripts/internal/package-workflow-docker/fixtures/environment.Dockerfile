FROM ubuntu:24.04
ARG DEBIAN_FRONTEND=noninteractive
RUN apt-get update && apt-get install -y --no-install-recommends \
      ansible-core ca-certificates curl git nodejs npm python3 sudo bash tar gzip && \
    rm -rf /var/lib/apt/lists/* && \
    groupadd --gid 11000 acceptance && \
    useradd --create-home --uid 11000 --gid 11000 --shell /bin/bash acceptance && \
    printf '%s\n' 'acceptance ALL=(ALL) NOPASSWD:ALL' > /etc/sudoers.d/acceptance && \
    chmod 0440 /etc/sudoers.d/acceptance && \
    install -d -o acceptance -g acceptance /workspace && \
    sudo -Hu acceptance git config --global user.name 'VM Acceptance' && \
    sudo -Hu acceptance git config --global user.email 'vm-acceptance@example.invalid'
USER acceptance
WORKDIR /workspace
CMD ["tail", "-f", "/dev/null"]
