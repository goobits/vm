#!/bin/bash
# Setup terminal with theme support

THEME_NAME="${1:-dracula}"
EMOJI="${2:-🚀}"
USERNAME="${3:-dev}"
SHOW_GIT_BRANCH="${4:-true}"
SHOW_TIMESTAMP="${5:-false}"

# Parse theme colors from themes.json
if [ -f /tmp/themes.json ]; then
    # Extract theme colors using jq
    THEME_JSON=$(jq -r ".${THEME_NAME}" /tmp/themes.json 2>/dev/null || echo "{}")
    
    if [ "$THEME_JSON" != "{}" ] && [ "$THEME_JSON" != "null" ]; then
        FOREGROUND=$(echo "$THEME_JSON" | jq -r '.colors.foreground // "#f8f8f2"')
        BACKGROUND=$(echo "$THEME_JSON" | jq -r '.colors.background // "#282a36"')
        RED=$(echo "$THEME_JSON" | jq -r '.colors.red // "#ff5555"')
        GREEN=$(echo "$THEME_JSON" | jq -r '.colors.green // "#50fa7b"')
        YELLOW=$(echo "$THEME_JSON" | jq -r '.colors.yellow // "#f1fa8c"')
        BLUE=$(echo "$THEME_JSON" | jq -r '.colors.blue // "#bd93f9"')
        MAGENTA=$(echo "$THEME_JSON" | jq -r '.colors.magenta // "#ff79c6"')
        CYAN=$(echo "$THEME_JSON" | jq -r '.colors.cyan // "#8be9fd"')
        BRIGHT_BLACK=$(echo "$THEME_JSON" | jq -r '.colors.bright_black // "#6272a4"')
    else
        # Default to dracula colors
        FOREGROUND="#f8f8f2"
        BACKGROUND="#282a36"
        RED="#ff5555"
        GREEN="#50fa7b"
        YELLOW="#f1fa8c"
        BLUE="#bd93f9"
        MAGENTA="#ff79c6"
        CYAN="#8be9fd"
        BRIGHT_BLACK="#6272a4"
    fi
fi

# Write zsh configuration
cat > ~/.zshrc << EOF
# NVM configuration
export NVM_DIR="\$HOME/.nvm"
[ -s "\$NVM_DIR/nvm.sh" ] && \. "\$NVM_DIR/nvm.sh"
[ -s "\$NVM_DIR/bash_completion" ] && \. "\$NVM_DIR/bash_completion"

# Locale settings
export LANG=en_US.UTF-8
export LANGUAGE=en_US:en
export LC_ALL=en_US.UTF-8
export LC_CTYPE=en_US.UTF-8

# Custom prompt function
function git_branch_name() {
$(if [ "$SHOW_GIT_BRANCH" = "true" ]; then
    echo '  git branch 2>/dev/null | grep "^*" | colrm 1 2 | sed "s/^/ (/" | sed "s/$/)/"'
fi)
}

function format_timestamp() {
$(if [ "$SHOW_TIMESTAMP" = "true" ]; then
    echo '  echo " [$(date +%H:%M:%S)]"'
fi)
}

# Set custom prompt
setopt PROMPT_SUBST
PROMPT='${EMOJI} ${USERNAME} %c\$(git_branch_name)\$(format_timestamp) > '

# Terminal color scheme: ${THEME_NAME}
export TERM=xterm-256color

# Theme-based color scheme for ls and other tools
export LS_COLORS='di=1;34:ln=1;36:so=1;35:pi=1;33:ex=1;32:bd=1;33:cd=1;33:su=0;41:sg=0;46:tw=0;42:ow=0;43:'

# Enable colored output for common commands
alias ls='ls --color=auto'
alias grep='grep --color=auto'
alias fgrep='fgrep --color=auto'
alias egrep='egrep --color=auto'
alias diff='diff --color=auto'

# Theme colors for zsh syntax highlighting
if [[ -r /usr/share/zsh-syntax-highlighting/zsh-syntax-highlighting.zsh ]]; then
  source /usr/share/zsh-syntax-highlighting/zsh-syntax-highlighting.zsh
  # Apply ${THEME_NAME} theme colors
  ZSH_HIGHLIGHT_HIGHLIGHTERS=(main brackets pattern cursor)
  ZSH_HIGHLIGHT_STYLES[default]='fg=${FOREGROUND}'
  ZSH_HIGHLIGHT_STYLES[unknown-token]='fg=${RED}'
  ZSH_HIGHLIGHT_STYLES[reserved-word]='fg=${MAGENTA}'
  ZSH_HIGHLIGHT_STYLES[alias]='fg=${GREEN}'
  ZSH_HIGHLIGHT_STYLES[suffix-alias]='fg=${GREEN}'
  ZSH_HIGHLIGHT_STYLES[builtin]='fg=${GREEN}'
  ZSH_HIGHLIGHT_STYLES[function]='fg=${GREEN}'
  ZSH_HIGHLIGHT_STYLES[command]='fg=${GREEN}'
  ZSH_HIGHLIGHT_STYLES[precommand]='fg=${GREEN}'
  ZSH_HIGHLIGHT_STYLES[commandseparator]='fg=${MAGENTA}'
  ZSH_HIGHLIGHT_STYLES[hashed-command]='fg=${GREEN}'
  ZSH_HIGHLIGHT_STYLES[path]='fg=${FOREGROUND}'
  ZSH_HIGHLIGHT_STYLES[path_pathseparator]='fg=${BRIGHT_BLACK}'
  ZSH_HIGHLIGHT_STYLES[globbing]='fg=${MAGENTA}'
  ZSH_HIGHLIGHT_STYLES[history-expansion]='fg=${BLUE}'
  ZSH_HIGHLIGHT_STYLES[single-hyphen-option]='fg=${YELLOW}'
  ZSH_HIGHLIGHT_STYLES[double-hyphen-option]='fg=${YELLOW}'
  ZSH_HIGHLIGHT_STYLES[back-quoted-argument]='fg=${CYAN}'
  ZSH_HIGHLIGHT_STYLES[single-quoted-argument]='fg=${YELLOW}'
  ZSH_HIGHLIGHT_STYLES[double-quoted-argument]='fg=${YELLOW}'
  ZSH_HIGHLIGHT_STYLES[dollar-quoted-argument]='fg=${YELLOW}'
  ZSH_HIGHLIGHT_STYLES[dollar-double-quoted-argument]='fg=${CYAN}'
  ZSH_HIGHLIGHT_STYLES[back-double-quoted-argument]='fg=${CYAN}'
  ZSH_HIGHLIGHT_STYLES[back-dollar-quoted-argument]='fg=${CYAN}'
  ZSH_HIGHLIGHT_STYLES[assign]='fg=${FOREGROUND}'
  ZSH_HIGHLIGHT_STYLES[redirection]='fg=${MAGENTA}'
  ZSH_HIGHLIGHT_STYLES[comment]='fg=${BRIGHT_BLACK}'
fi

# Universal aliases
alias ll='ls -la'
alias dev='cd /workspace && ls'
alias ports='netstat -tulpn | grep LISTEN'
alias services='ps aux | grep -E "postgres|redis|mongo"'

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

# Auto-cd to workspace
cd /workspace 2>/dev/null || true
EOF