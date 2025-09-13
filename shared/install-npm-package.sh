#!/bin/bash
# Install npm package with link detection
# Usage: install-npm-package.sh <project_user> <package>

set -e

PROJECT_USER="$1"
PACKAGE="$2"

if [[ -z "$PROJECT_USER" || -z "$PACKAGE" ]]; then
    echo "Usage: $0 <project_user> <package>"
    exit 1
fi

# Source nvm
export NVM_DIR="/home/$PROJECT_USER/.nvm"
if [ -s "$NVM_DIR/nvm.sh" ]; then
    . "$NVM_DIR/nvm.sh"
else
    echo "Warning: NVM not found. This may fail if node is not in the default PATH." >&2
fi


# Check if this is a linked local package
if [ -d "/home/${PROJECT_USER}/.links/npm/$PACKAGE" ]; then
    echo "🔗 Linking local npm package: $PACKAGE"
    cd "/home/${PROJECT_USER}/.links/npm/$PACKAGE"
    npm link
else
    echo "📦 Installing npm package from registry: $PACKAGE"
    npm install -g "$PACKAGE"
fi
