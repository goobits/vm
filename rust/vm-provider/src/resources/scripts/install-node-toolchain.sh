#!/usr/bin/env bash
set -euo pipefail

: "${HOME:?HOME must be set}"

node_target="${VM_NODE_VERSION:-22}"
nvm_target="${VM_NVM_VERSION:-v0.40.3}"
npm_target="${VM_NPM_VERSION:-}"
pnpm_target="${VM_PNPM_VERSION:-10.12.3}"
export NVM_DIR="${NVM_DIR:-$HOME/.nvm}"

changed=0
installer=""
cleanup() {
  if [ -n "$installer" ]; then
    rm -f -- "$installer"
  fi
}
trap cleanup EXIT

if [ ! -s "$NVM_DIR/nvm.sh" ]; then
  installer="$(mktemp)"
  curl -fsSL \
    "https://raw.githubusercontent.com/nvm-sh/nvm/$nvm_target/install.sh" \
    -o "$installer"
  PROFILE=/dev/null bash "$installer"
  changed=1
fi

# shellcheck disable=SC1090
. "$NVM_DIR/nvm.sh"

node_version="$(nvm version "$node_target")"
if [ "$node_version" = "N/A" ]; then
  nvm install "$node_target"
  node_version="$(nvm version "$node_target")"
  changed=1
fi

default_version="$(nvm version default 2>/dev/null || true)"
if [ "$default_version" != "$node_version" ]; then
  nvm alias default "$node_target" >/dev/null
  changed=1
fi
nvm use --silent "$node_target"

if [ -n "$npm_target" ]; then
  if [ "$npm_target" = "latest" ]; then
    npm_target="$(npm view npm@latest version)"
  fi
  if [ "$(npm --version)" != "$npm_target" ]; then
    npm install -g "npm@$npm_target"
    changed=1
  fi
fi

if [ -n "$pnpm_target" ]; then
  if [ "$pnpm_target" = "latest" ]; then
    pnpm_target="$(npm view pnpm@latest version)"
  fi
  if ! command -v pnpm >/dev/null 2>&1 || [ "$(pnpm --version)" != "$pnpm_target" ]; then
    if command -v corepack >/dev/null 2>&1 \
      && corepack enable \
      && corepack prepare "pnpm@$pnpm_target" --activate; then
      :
    else
      npm install -g "pnpm@$pnpm_target"
    fi
    changed=1
  fi
fi

if [ "$changed" = 1 ]; then
  echo 'VM_NODE_TOOLCHAIN_CHANGED=1'
else
  echo 'VM_NODE_TOOLCHAIN_CURRENT=1'
fi
