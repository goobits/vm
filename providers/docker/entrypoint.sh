#!/bin/bash
# Minimal Docker entrypoint - just keeps container running for Ansible

# Start SSH if configured (optional - for SSH-based Ansible connection)
if [ -f /etc/ssh/sshd_config ]; then
    service ssh start
fi

# Keep container running
exec sleep infinity