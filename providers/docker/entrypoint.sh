#!/bin/bash
# Docker entrypoint script with full Vagrant parity

# Source NVM
export NVM_DIR="/home/${USER}/.nvm"
[ -s "$NVM_DIR/nvm.sh" ] && \. "$NVM_DIR/nvm.sh"

# Set environment variables from config
export DISPLAY=${DISPLAY:-:99}
export PUPPETEER_EXECUTABLE_PATH=${PUPPETEER_EXECUTABLE_PATH:-/usr/bin/chromium-browser}

# Install global npm packages if specified
if [ -n "$NPM_PACKAGES" ]; then
    echo "Installing global npm packages..."
    for package in $NPM_PACKAGES; do
        npm install -g "$package" || echo "Failed to install $package"
    done
fi

# Setup environment from template
if [ -n "$ENV_TEMPLATE_PATH" ]; then
    /vm-tool/providers/docker/scripts/setup-env.sh
fi

# Install project dependencies
if [ -f "/vm-tool/providers/docker/scripts/install-dependencies.sh" ]; then
    /vm-tool/providers/docker/scripts/install-dependencies.sh
fi

# Restore database backup if PostgreSQL is enabled
if [ "$POSTGRES_ENABLED" = "true" ] && [ -f "/vm-tool/providers/docker/scripts/restore-backup.sh" ]; then
    /vm-tool/providers/docker/scripts/restore-backup.sh
fi

# Copy Claude settings if available
if [ -f /vm-tool/shared/claude-settings/settings.json ]; then
    cp /vm-tool/shared/claude-settings/settings.json ~/.claude/settings.json
else
    # Use default Claude settings
    cat > ~/.claude/settings.json << 'EOF'
{
  "allow": [
    "Read(/workspace/**)",
    "Write(/workspace/**)",
    "Edit(/workspace/**)",
    "MultiEdit(/workspace/**)",
    "Bash",
    "WebSearch",
    "WebFetch"
  ],
  "deny": [
    "Read(**/.env*)",
    "Read(**/*.key)",
    "Bash(sudo *)",
    "Bash(rm -rf /)"
  ]
}
EOF
fi

# Set up Git safe directory
git config --global --add safe.directory /workspace

# Setup project-specific aliases
if [ -n "$PROJECT_ALIASES" ]; then
    echo "" >> ~/.zshrc
    echo "# Project aliases (from vm.json)" >> ~/.zshrc
    echo "$PROJECT_ALIASES" | tr ';' '\n' | while IFS='=' read -r name cmd; do
        if [ -n "$name" ] && [ -n "$cmd" ]; then
            echo "alias $name='$cmd'" >> ~/.zshrc
        fi
    done
fi

# Setup database aliases with correct ports
if [ -n "$POSTGRES_PORT" ]; then
    echo "alias psql='PGPASSWORD=$POSTGRES_PASSWORD psql -h postgres -p $POSTGRES_PORT -U $POSTGRES_USER'" >> ~/.zshrc
fi
if [ -n "$REDIS_PORT" ]; then
    echo "alias redis='redis-cli -h redis -p $REDIS_PORT'" >> ~/.zshrc
fi
if [ -n "$MONGODB_PORT" ]; then
    echo "alias mongo='mongosh --host mongodb --port $MONGODB_PORT'" >> ~/.zshrc
fi

# Run any project-specific setup
if [ -f /workspace/.vm/setup.sh ]; then
    echo "Running project-specific setup..."
    bash /workspace/.vm/setup.sh
fi

# Start Xvfb if headless browser is enabled
if [ "$HEADLESS_BROWSER_ENABLED" = "true" ]; then
    Xvfb :99 -screen 0 1024x768x24 -ac &
fi

# Execute the command passed to docker run
exec "$@"