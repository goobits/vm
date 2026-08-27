#!/usr/bin/env bash

set -euo pipefail

export PATH="$HOME/.local/bin:$PATH"

if [[ $# -eq 0 ]]; then
  echo "Usage: install-vibe-ai-tools.sh <antigravity|claude|codex> [...]" >&2
  exit 2
fi

install_tool() {
  local checksum executable installer shell shell_arg
  shell_arg=""

  case "$1" in
    antigravity)
      executable=agy
      installer=https://antigravity.google/cli/install.sh
      checksum=ee1ea43ce4e9e56356c4ab6dad907ef357ae4bdfcaadb682735909fb57c9c640
      shell=bash
      ;;
    claude)
      executable=claude
      installer=https://claude.ai/install.sh
      checksum=3a68d3406cf674e17bed1733a4dcf37805e2e47d87417700007d7e1aa766a944
      shell=bash
      shell_arg=stable
      ;;
    codex)
      executable=codex
      installer=https://chatgpt.com/codex/install.sh
      checksum=ba92dd27e5c06f0d3bbc58bfa4b9cfb6599cd2742fbb1f92a2765e6c07dedb5a
      shell=sh
      ;;
    *)
      echo "Unsupported Vibe AI tool: $1" >&2
      exit 2
      ;;
  esac

  echo "Installing $1 into the Vibe base..."
  local actual installer_file
  installer_file=$(mktemp "${TMPDIR:-/tmp}/vm-ai-installer.XXXXXX")
  trap 'rm -f "$installer_file"' RETURN
  curl --proto '=https' --tlsv1.2 -fsSL "$installer" -o "$installer_file"
  if command -v sha256sum >/dev/null 2>&1; then
    echo "$checksum  $installer_file" | sha256sum --check -
  else
    actual=$(shasum -a 256 "$installer_file" | awk '{print $1}')
    [[ "$actual" == "$checksum" ]]
  fi
  if [[ -n "$shell_arg" ]]; then
    "$shell" "$installer_file" "$shell_arg"
  else
    "$shell" "$installer_file"
  fi
  rm -f "$installer_file"
  trap - RETURN
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
