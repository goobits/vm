#!/bin/bash
# Install cargo package with link detection
# Usage: install-cargo-package.sh <project_user> <package>

set -e

PROJECT_USER="$1"
PACKAGE="$2"

if [[ -z "$PROJECT_USER" || -z "$PACKAGE" ]]; then
    echo "Usage: $0 <project_user> <package>"
    exit 1
fi

# Source cargo env
export CARGO_HOME="/home/$PROJECT_USER/.cargo"
if [ -s "$CARGO_HOME/env" ]; then
    . "$CARGO_HOME/env"
else
    echo "Warning: Cargo env not found. This may fail if cargo is not in the default PATH." >&2
fi

# Check if this is a linked local package
if [ -d "/home/${PROJECT_USER}/.links/cargo/$PACKAGE" ]; then
    echo "🔗 Linking local cargo package: $PACKAGE"
    cd "/home/${PROJECT_USER}/.links/cargo/$PACKAGE"
    cargo install --path .
else
    echo "📦 Installing cargo package from registry: $PACKAGE"
    cargo install "$PACKAGE"
fi
