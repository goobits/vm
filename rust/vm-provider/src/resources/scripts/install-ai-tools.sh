#!/usr/bin/env bash

set -euo pipefail

export PATH="$HOME/.local/bin:$PATH"

if [[ $# -eq 0 ]]; then
  echo "Usage: install-ai-tools.sh <antigravity|claude|codex> [...]" >&2
  exit 2
fi

INSTALLER_STATE_VERSION=1
refresh_key="${VM_AI_TOOLS_REFRESH_KEY:-$(date -u +%Y-%m-%d)}"
state_dir="$HOME/.vm/state/ai-tools"

mkdir -p "$state_dir"
chmod 700 "$state_dir"

install_tool() {
  local executable installer shell shell_arg refresh_scope legacy_package
  local marker expected current version marker_tmp
  local had_existing=false install_failed=false
  shell_arg=""

  case "$1" in
    antigravity)
      executable=agy
      installer=https://antigravity.google/cli/install.sh
      shell=bash
      refresh_scope=automatic
      legacy_package=@google/gemini-cli
      ;;
    claude)
      executable=claude
      installer=https://claude.ai/install.sh
      shell=bash
      shell_arg=stable
      refresh_scope=automatic
      legacy_package=@anthropic-ai/claude-code
      ;;
    codex)
      executable=codex
      installer=https://chatgpt.com/codex/install.sh
      shell=sh
      refresh_scope="$refresh_key"
      legacy_package=@openai/codex
      ;;
    *)
      echo "Unsupported AI tool: $1" >&2
      exit 2
      ;;
  esac

  marker="$state_dir/$1"
  expected="$INSTALLER_STATE_VERSION:$refresh_scope"
  current=""
  if [ -f "$marker" ]; then
    IFS= read -r current <"$marker" || true
  fi

  if [ "${VM_AI_TOOLS_FORCE:-0}" != 1 ] \
    && [ "$current" = "$expected" ] \
    && command -v "$executable" >/dev/null 2>&1; then
    echo "VM_AI_TOOL_CURRENT=$1"
    return 0
  fi

  if command -v "$executable" >/dev/null 2>&1; then
    had_existing=true
  fi

  echo "Refreshing $1 CLI..."
  if [ -n "$shell_arg" ]; then
    curl -fsSL "$installer" | "$shell" -s -- "$shell_arg" || install_failed=true
  else
    curl -fsSL "$installer" | "$shell" || install_failed=true
  fi
  if $install_failed; then
    if $had_existing && command -v "$executable" >/dev/null 2>&1; then
      echo "Warning: could not refresh $1; keeping the installed version" >&2
      echo "VM_AI_TOOL_STALE=$1"
      return 0
    fi
    echo "Failed to install required AI tool: $1" >&2
    return 1
  fi

  hash -r
  if [ ! -x "$HOME/.local/bin/$executable" ]; then
    echo "Installer did not create $HOME/.local/bin/$executable" >&2
    return 1
  fi

  if command -v npm >/dev/null 2>&1; then
    npm uninstall -g "$legacy_package" >/dev/null 2>&1 || true
  fi

  version="$("$HOME/.local/bin/$executable" --version 2>/dev/null || true)"
  version="${version%%$'\n'*}"
  marker_tmp="$marker.tmp.$$"
  umask 077
  printf '%s\n%s\n' "$expected" "$version" >"$marker_tmp"
  mv -f "$marker_tmp" "$marker"
  echo "VM_AI_TOOL_CHANGED=$1"
}

for tool in "$@"; do
  install_tool "$tool"
done
