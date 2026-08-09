#!/usr/bin/env bash

set -euo pipefail

export PATH="$HOME/.local/bin:$PATH"

install_tool() {
  case "$1" in
    antigravity)
      curl -fsSL https://antigravity.google/cli/install.sh | bash
      command -v agy >/dev/null
      if command -v npm >/dev/null 2>&1; then
        npm uninstall -g @google/gemini-cli >/dev/null 2>&1 || true
      fi
      ;;
    claude)
      curl -fsSL https://claude.ai/install.sh | bash
      command -v claude >/dev/null
      ;;
    codex)
      if ! command -v npm >/dev/null 2>&1; then
        echo "Codex installation requires npm" >&2
        exit 1
      fi
      npm install -g @openai/codex@latest
      hash -r
      command -v codex >/dev/null
      ;;
    *)
      echo "Unsupported AI tool: $1" >&2
      exit 2
      ;;
  esac
}

if [[ $# -eq 0 ]]; then
  echo "Usage: install-ai-tools.sh <antigravity|claude|codex> [...]" >&2
  exit 2
fi

for tool in "$@"; do
  install_tool "$tool"
done
