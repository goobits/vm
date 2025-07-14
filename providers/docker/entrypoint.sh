#!/bin/bash
set -e

# Get the project user from environment (set by docker-compose)
PROJECT_USER="${PROJECT_USER:-developer}"

# Ensure proper ownership of mounted Claude directory if it exists
if [ -d "/home/$PROJECT_USER/.claude" ]; then
    chown -R $PROJECT_USER:$(id -gn $PROJECT_USER) /home/$PROJECT_USER/.claude || true
    chmod -R 755 /home/$PROJECT_USER/.claude || true
fi

# Copy shared Claude settings and CLAUDE.md if they exist
if [ -f "/vm-tool/shared/claude-settings/settings.json" ]; then
    mkdir -p "/home/$PROJECT_USER/.claude"
    cp "/vm-tool/shared/claude-settings/settings.json" "/home/$PROJECT_USER/.claude/" || true
    chown $PROJECT_USER:$(id -gn $PROJECT_USER) "/home/$PROJECT_USER/.claude/settings.json" || true
fi

if [ -f "/vm-tool/shared/claude-settings/CLAUDE.md" ]; then
    mkdir -p "/home/$PROJECT_USER/.claude"
    cp "/vm-tool/shared/claude-settings/CLAUDE.md" "/home/$PROJECT_USER/.claude/" || true
    chown $PROJECT_USER:$(id -gn $PROJECT_USER) "/home/$PROJECT_USER/.claude/CLAUDE.md" || true
fi

# Run shell setup if config exists
if [ -f /tmp/vm-config.json ]; then
    echo "📄 Found config file, setting up shell..."
    /usr/local/bin/setup-shell.sh
else
    echo "⚠️  No config file found at /tmp/vm-config.json"
fi

# Start supervisor, which will in turn start all configured services.
exec /usr/bin/supervisord -n -c /etc/supervisor/supervisord.conf