#!/usr/bin/env bash

set -euo pipefail

export PATH="$HOME/.local/bin:$PATH"

install_tool() {
  local executable installer shell legacy_package

  case "$1" in
    antigravity)
      executable=agy
      installer=https://antigravity.google/cli/install.sh
      shell=bash
      legacy_package=@google/gemini-cli
      ;;
    claude)
      executable=claude
      installer=https://claude.ai/install.sh
      shell=bash
      legacy_package=@anthropic-ai/claude-code
      ;;
    codex)
      executable=codex
      installer=https://chatgpt.com/codex/install.sh
      shell=sh
      legacy_package=@openai/codex
      ;;
    *)
      echo "Unsupported AI tool: $1" >&2
      exit 2
      ;;
  esac

  if command -v npm >/dev/null 2>&1; then
    npm uninstall -g "$legacy_package" >/dev/null 2>&1 || true
  fi

  curl -fsSL "$installer" | "$shell"
  hash -r
  command -v "$executable" >/dev/null
}

if [[ $# -eq 0 ]]; then
  echo "Usage: install-ai-tools.sh <antigravity|claude|codex> [...]" >&2
  exit 2
fi

for tool in "$@"; do
  install_tool "$tool"
done
