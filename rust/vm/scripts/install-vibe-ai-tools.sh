#!/usr/bin/env bash

set -euo pipefail

export PATH="$HOME/.local/bin:$PATH"

if [[ $# -eq 0 ]]; then
  echo "Usage: install-vibe-ai-tools.sh <antigravity|claude|codex> [...]" >&2
  exit 2
fi

install_tool() {
  local executable installer shell shell_arg legacy_package
  shell_arg=""

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
      shell_arg=stable
      legacy_package=@anthropic-ai/claude-code
      ;;
    codex)
      executable=codex
      installer=https://chatgpt.com/codex/install.sh
      shell=sh
      legacy_package=@openai/codex
      ;;
    *)
      echo "Unsupported Vibe AI tool: $1" >&2
      exit 2
      ;;
  esac

  echo "Installing $1 into the Vibe base..."
  if [[ -n "$shell_arg" ]]; then
    curl -fsSL "$installer" | "$shell" -s -- "$shell_arg"
  else
    curl -fsSL "$installer" | "$shell"
  fi
  hash -r

  if ! command -v "$executable" >/dev/null 2>&1; then
    echo "Installer did not activate required Vibe command: $executable" >&2
    exit 1
  fi

  # Codex's standalone installer places the executable below ~/.codex, which
  # is intentionally replaced by host-synced state in Vibe environments.
  # Keep the immutable executable outside that state directory.
  if [[ "$1" == "codex" ]]; then
    local resolved_executable
    resolved_executable="$(python3 -c 'import os, sys; print(os.path.realpath(sys.argv[1]))' "$(command -v codex)")"
    sudo install -d -m 0755 /usr/local/lib/vm-ai-tools
    sudo install -m 0755 "$resolved_executable" /usr/local/lib/vm-ai-tools/codex
    sudo ln -sfn /usr/local/lib/vm-ai-tools/codex /usr/local/bin/codex
    /usr/local/bin/codex --version >/dev/null
  fi

  if command -v npm >/dev/null 2>&1; then
    npm uninstall -g "$legacy_package" >/dev/null 2>&1 || true
  fi
}

for tool in "$@"; do
  install_tool "$tool"
done
