#!/bin/bash
# Docker wrapper to handle sudo requirements

# Check if we can run docker without sudo
if docker version &>/dev/null 2>&1; then
    # Docker works without sudo
    exec docker "$@"
else
    # Need sudo for docker
    exec sudo docker "$@"
fi