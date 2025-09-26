#!/bin/bash

# Setup script for configuring Docker containers to use the Goobits Package Server
# Usage: ./setup-client.sh [python|node|rust] [container-name]

set -e

PKG_SERVER_URL="${PKG_SERVER_URL:-pkg-server}"
PKG_SERVER_PORT="${PKG_SERVER_PORT:-3080}"
PKG_NETWORK="${PKG_NETWORK:-pkg-network}"

function print_usage() {
    echo "Usage: $0 [python|node|rust] [container-name]"
    echo ""
    echo "Examples:"
    echo "  $0 python my-python-app    # Setup Python container"
    echo "  $0 node my-node-app        # Setup Node.js container"
    echo "  $0 rust my-rust-app        # Setup Rust container"
    echo ""
    echo "Environment variables:"
    echo "  PKG_SERVER_URL  - Package server hostname (default: pkg-server)"
    echo "  PKG_SERVER_PORT - Package server port (default: 8080)"
    echo "  PKG_NETWORK     - Docker network name (default: pkg-network)"
}

function setup_python() {
    local container_name=$1
    echo "Setting up Python container: $container_name"

    docker run -d \
        --name "$container_name" \
        --network "$PKG_NETWORK" \
        -e "PIP_INDEX_URL=http://${PKG_SERVER_URL}:${PKG_SERVER_PORT}/pypi/simple/" \
        -e "PIP_TRUSTED_HOST=${PKG_SERVER_URL}" \
        python:3.11-slim \
        sleep infinity

    echo "Python container '$container_name' created and configured."
    echo "To test: docker exec -it $container_name pip install requests"
}

function setup_node() {
    local container_name=$1
    echo "Setting up Node.js container: $container_name"

    docker run -d \
        --name "$container_name" \
        --network "$PKG_NETWORK" \
        -e "NPM_CONFIG_REGISTRY=http://${PKG_SERVER_URL}:${PKG_SERVER_PORT}/npm/" \
        node:18-slim \
        sleep infinity

    echo "Node.js container '$container_name' created and configured."
    echo "To test: docker exec -it $container_name npm install express"
}

function setup_rust() {
    local container_name=$1
    echo "Setting up Rust container: $container_name"

    # Create a temporary Dockerfile for Rust with cargo config
    cat > /tmp/rust-client.dockerfile <<EOF
FROM rust:1.78
RUN mkdir -p /usr/local/cargo && \
    echo '[registries.local]' > /usr/local/cargo/config.toml && \
    echo 'index = "sparse+http://${PKG_SERVER_URL}:${PKG_SERVER_PORT}/cargo/"' >> /usr/local/cargo/config.toml
WORKDIR /workspace
CMD ["sleep", "infinity"]
EOF

    docker build -t rust-client-configured -f /tmp/rust-client.dockerfile /tmp/
    docker run -d \
        --name "$container_name" \
        --network "$PKG_NETWORK" \
        rust-client-configured

    rm /tmp/rust-client.dockerfile

    echo "Rust container '$container_name' created and configured."
    echo "To test: docker exec -it $container_name cargo search --registry local"
}

function ensure_network() {
    if ! docker network ls | grep -q "$PKG_NETWORK"; then
        echo "Creating Docker network: $PKG_NETWORK"
        docker network create "$PKG_NETWORK"
    else
        echo "Using existing network: $PKG_NETWORK"
    fi
}

function ensure_server() {
    if ! docker ps | grep -q "$PKG_SERVER_URL"; then
        echo "Warning: Package server '$PKG_SERVER_URL' is not running!"
        echo "Start it with: docker-compose up -d"
        read -p "Continue anyway? (y/N) " -n 1 -r
        echo
        if [[ ! $REPLY =~ ^[Yy]$ ]]; then
            exit 1
        fi
    fi
}

# Main script
if [ $# -lt 2 ]; then
    print_usage
    exit 1
fi

LANG_TYPE=$1
CONTAINER_NAME=$2

# Ensure prerequisites
ensure_network
ensure_server

case "$LANG_TYPE" in
    python)
        setup_python "$CONTAINER_NAME"
        ;;
    node)
        setup_node "$CONTAINER_NAME"
        ;;
    rust)
        setup_rust "$CONTAINER_NAME"
        ;;
    *)
        echo "Error: Unknown language type '$LANG_TYPE'"
        print_usage
        exit 1
        ;;
esac

echo ""
echo "Setup complete! Container '$CONTAINER_NAME' is now configured to use the package server."
echo ""
echo "Quick commands:"
echo "  Enter container:  docker exec -it $CONTAINER_NAME bash"
echo "  View logs:        docker logs $CONTAINER_NAME"
echo "  Stop container:   docker stop $CONTAINER_NAME"
echo "  Remove container: docker rm -f $CONTAINER_NAME"