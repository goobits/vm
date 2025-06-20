#!/bin/bash
# Install project dependencies if lock files exist

set -e

cd /workspace

# Source NVM
export NVM_DIR="/home/${USER}/.nvm"
[ -s "$NVM_DIR/nvm.sh" ] && \. "$NVM_DIR/nvm.sh"

# Check for lock files and install dependencies
if [ -f "pnpm-lock.yaml" ]; then
    echo "Found pnpm-lock.yaml, installing dependencies..."
    pnpm install
    
    # Check if build script exists
    if pnpm run --silent list 2>/dev/null | grep -q "build"; then
        echo "Running build script..."
        pnpm build
    fi
elif [ -f "package-lock.json" ]; then
    echo "Found package-lock.json, installing dependencies..."
    npm install
    
    # Check if build script exists
    if npm run --silent list 2>/dev/null | grep -q "build"; then
        echo "Running build script..."
        npm run build
    fi
elif [ -f "yarn.lock" ]; then
    echo "Found yarn.lock, installing dependencies..."
    yarn install
    
    # Check if build script exists
    if yarn run --silent list 2>/dev/null | grep -q "build"; then
        echo "Running build script..."
        yarn build
    fi
fi