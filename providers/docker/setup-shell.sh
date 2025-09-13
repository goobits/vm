#!/bin/bash
# This script sets up shell configurations dynamically based on project config

echo "🐚 Configuring shell environment..."

CONFIG_FILE="/tmp/vm-config.yaml"

# Detect the correct user (works with any username)
if [ "$USER" = "root" ]; then
    # When running as root, detect the non-root user
    DETECTED_USER=$(getent passwd 1000 | cut -d: -f1)
    USER_HOME="/home/$DETECTED_USER"
else
    DETECTED_USER="$USER"
    USER_HOME="$HOME"
fi

echo "👤 Configuring for user: $DETECTED_USER"

# Default values
EMOJI="🚀"
USERNAME="dev"
SHOW_GIT_BRANCH="true"
SHOW_TIMESTAMP="false"

# Extract values from config if available
if [ -f "$CONFIG_FILE" ]; then
    # vm-config is always available at known location
    VM_CONFIG_BIN="/usr/local/bin/vm-config"

    EMOJI=$("$VM_CONFIG_BIN" query "$CONFIG_FILE" "terminal.emoji" --raw --default "🚀")
    USERNAME=$("$VM_CONFIG_BIN" query "$CONFIG_FILE" "terminal.username" --raw --default "dev")
    SHOW_GIT_BRANCH=$("$VM_CONFIG_BIN" query "$CONFIG_FILE" "terminal.show_git_branch" --raw --default "true")
    SHOW_TIMESTAMP=$("$VM_CONFIG_BIN" query "$CONFIG_FILE" "terminal.show_timestamp" --raw --default "false")
    WORKSPACE=$("$VM_CONFIG_BIN" query "$CONFIG_FILE" "project.workspace_path" --raw --default "/workspace")

    # Extract environment variables using shell loop
    ENV_VARS=""
    env_keys=$("$VM_CONFIG_BIN" transform "$CONFIG_FILE" 'environment | keys | .[]' --format lines)
    if [ -n "$env_keys" ]; then
        while IFS= read -r key; do
            [ -z "$key" ] && continue
            value=$("$VM_CONFIG_BIN" query "$CONFIG_FILE" "environment.$key" --raw --default "")
            ENV_VARS="$ENV_VARS"$'\n'"export $key=\"$value\""
        done <<< "$env_keys"
    fi

    # Extract aliases using shell loop
    ALIASES=""
    alias_keys=$("$VM_CONFIG_BIN" transform "$CONFIG_FILE" 'aliases | keys | .[]' --format lines)
    if [ -n "$alias_keys" ]; then
        while IFS= read -r key; do
            [ -z "$key" ] && continue
            value=$("$VM_CONFIG_BIN" query "$CONFIG_FILE" "aliases.$key" --raw --default "")
            ALIASES="$ALIASES"$'\n'"alias $key='$value'"
        done <<< "$alias_keys"
    fi
fi

# Create .bashrc
cat > "$USER_HOME/.bashrc" << EOF
# Custom prompt functions for bash
git_branch_name() {
$(if [ "$SHOW_GIT_BRANCH" = "true" ]; then
    echo "  git branch 2>/dev/null | grep '^*' | cut -c3- | sed 's/^/ (/' | sed 's/$/)/'"
else
    echo "  true"
fi)
}

format_timestamp() {
$(if [ "$SHOW_TIMESTAMP" = "true" ]; then
    echo "  echo \" [\$(date '+%H:%M:%S')]\""
else
    echo "  true"
fi)
}

# Set custom prompt
PS1='$EMOJI $USERNAME \W\$(git_branch_name)\$(format_timestamp) > '

# Universal aliases
alias ll='ls -la'
alias dev='cd $WORKSPACE && ls'
alias ports='netstat -tulpn | grep LISTEN'
alias services='systemctl list-units --type=service --state=running'

# Search tools
alias rg='rg --smart-case'
alias rgf='rg --files | rg'

# Git shortcuts
alias gs='git status'
alias ga='git add'
alias gc='git commit'
alias gp='git push'
alias gl='git log --oneline'

# Docker shortcuts
alias dps='docker ps'
alias dimg='docker images'

# Project aliases (from vm.yaml)
$ALIASES

# Environment
export DISPLAY=:99
export PYTHONDONTWRITEBYTECODE=1
$ENV_VARS

# Auto-cd to workspace
cd $WORKSPACE 2>/dev/null || true
EOF

# Create .zshrc
cat > "$USER_HOME/.zshrc" << EOF
# Custom prompt functions for zsh
function git_branch_name() {
$(if [ "$SHOW_GIT_BRANCH" = "true" ]; then
    echo "  git branch 2>/dev/null | grep '^*' | cut -c3- | sed 's/^/ (/' | sed 's/$/)/'"
else
    echo "  true"
fi)
}

function format_timestamp() {
$(if [ "$SHOW_TIMESTAMP" = "true" ]; then
    echo "  echo \" [\$(date '+%H:%M:%S')]\""
else
    echo "  true"
fi)
}

# Set custom prompt
setopt PROMPT_SUBST
PROMPT='$EMOJI $USERNAME %c\$(git_branch_name)\$(format_timestamp) > '

# Universal aliases
alias ll='ls -la'
alias dev='cd $WORKSPACE && ls'
alias ports='netstat -tulpn | grep LISTEN'
alias services='systemctl list-units --type=service --state=running'

# Search tools
alias rg='rg --smart-case'
alias rgf='rg --files | rg'

# Git shortcuts
alias gs='git status'
alias ga='git add'
alias gc='git commit'
alias gp='git push'
alias gl='git log --oneline'

# Docker shortcuts
alias dps='docker ps'
alias dimg='docker images'

# Project aliases (from vm.yaml)
$ALIASES

# Environment
export DISPLAY=:99
export PYTHONDONTWRITEBYTECODE=1
$ENV_VARS

# Auto-cd to workspace
cd $WORKSPACE 2>/dev/null || true
EOF

# Set ownership
chown "$DETECTED_USER:$DETECTED_USER" "$USER_HOME/.bashrc" "$USER_HOME/.zshrc"

echo "✨ Shell configured with prompt: $EMOJI $USERNAME"

# Quick check if Node.js is available and install Claude if possible
if [ -f "$USER_HOME/.nvm/nvm.sh" ]; then
    echo "📦 Installing development tools..."

    # Run as detected user to ensure proper environment
    su - "$DETECTED_USER" -c '
        source ~/.nvm/nvm.sh

        # Check if claude is already installed
        if ! which claude > /dev/null 2>&1; then
            echo "🤖 Installing Claude Code CLI..."
            npm install -g @anthropic-ai/claude-code
        else
            echo "✅ Claude Code ready"
        fi

        # Check if gemini is already installed
        if ! which gemini > /dev/null 2>&1; then
            echo "💎 Installing Gemini CLI..."
            npm install -g @google/gemini-cli
        else
            echo "✅ Gemini ready"
        fi

        # Also try to install pnpm directly if corepack fails
        if ! which pnpm > /dev/null 2>&1; then
            echo "📋 Installing pnpm..."
            npm install -g pnpm@10.12.3
        fi
    '
fi

# Fix ownership of shell configuration files
echo "🔒 Setting file permissions..."
chown "$DETECTED_USER:$DETECTED_USER" "$USER_HOME/.bashrc" "$USER_HOME/.zshrc"
chmod 644 "$USER_HOME/.bashrc" "$USER_HOME/.zshrc"