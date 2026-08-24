#!/usr/bin/env bash

set -euo pipefail

export PATH="$HOME/.local/bin:$PATH"

if [[ $# -eq 0 ]]; then
  echo "Usage: install-vibe-ai-tools.sh <antigravity|claude|codex> [...]" >&2
  exit 2
fi

install_tool() {
  local executable installer shell shell_arg
  shell_arg=""

  case "$1" in
    antigravity)
      executable=agy
      installer=https://antigravity.google/cli/install.sh
      shell=bash
      ;;
    claude)
      executable=claude
      installer=https://claude.ai/install.sh
      shell=bash
      shell_arg=stable
      ;;
    codex)
      executable=codex
      installer=https://chatgpt.com/codex/install.sh
      shell=sh
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

  # Codex's standalone installer places its canonical runtime below ~/.codex,
  # which is intentionally replaced by host-synced state in Vibe environments.
  # Preserve the complete package outside that state directory: code-mode and
  # sandbox helpers must remain beside the main executable.
  if [[ "$1" == "codex" ]]; then
    local codex_bin_dir codex_package_dir resolved_executable
    resolved_executable="$(python3 -c 'import os, sys; print(os.path.realpath(sys.argv[1]))' "$(command -v codex)")"
    codex_bin_dir="$(dirname "$resolved_executable")"
    codex_package_dir="$(dirname "$codex_bin_dir")"
    if [[ ! -f "$codex_package_dir/codex-package.json" || \
          ! -x "$codex_bin_dir/codex-code-mode-host" ]]; then
      echo "Codex installer did not provide the complete standalone runtime" >&2
      exit 1
    fi

    sudo install -d -m 0755 \
      /usr/local/lib/vm-ai-tools \
      /usr/local/lib/vm-ai-tools/codex-package
    sudo cp -R "$codex_package_dir/." /usr/local/lib/vm-ai-tools/codex-package/
    sudo ln -sfn \
      /usr/local/lib/vm-ai-tools/codex-package/bin/codex \
      /usr/local/lib/vm-ai-tools/codex
    sudo ln -sfn \
      /usr/local/lib/vm-ai-tools/codex-package/bin/codex-code-mode-host \
      /usr/local/lib/vm-ai-tools/codex-code-mode-host
    sudo ln -sfn /usr/local/lib/vm-ai-tools/codex /usr/local/bin/codex
    sudo ln -sfn \
      /usr/local/lib/vm-ai-tools/codex-code-mode-host \
      /usr/local/bin/codex-code-mode-host
    test -x /usr/local/bin/codex-code-mode-host
    /usr/local/bin/codex --version >/dev/null
  fi
}

for tool in "$@"; do
  install_tool "$tool"
done
