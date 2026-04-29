#!/usr/bin/env bash

set -euo pipefail

GUEST_OS="${GUEST_OS:-macos}"
BASE_NAME="${BASE_NAME:-}"
BASE_IMAGE="${BASE_IMAGE:-}"
NODE_VERSION="${NODE_VERSION:-22}"
NVM_VERSION="${NVM_VERSION:-v0.40.3}"
WAIT_SECONDS="${WAIT_SECONDS:-120}"

usage() {
  cat <<'EOF'
Build a local Tart-native vibe base VM.

Usage:
  ./scripts/build-vibe-tart-base.sh [--guest-os macos|linux] [--name NAME] [--base-image IMAGE] [--node-version VERSION]

Environment overrides:
  GUEST_OS       Guest OS type to build (default: macos)
  BASE_NAME       Target Tart VM name (default depends on guest OS)
  BASE_IMAGE      Source Tart image (default depends on guest OS)
  NODE_VERSION    Default Node version to preinstall (default: 22)
  NVM_VERSION     NVM installer version (default: v0.40.3)
  WAIT_SECONDS    SSH readiness timeout in seconds (default: 120)
EOF
}

while [[ $# -gt 0 ]]; do
  case "$1" in
    --guest-os)
      GUEST_OS="$2"
      shift 2
      ;;
    --name)
      BASE_NAME="$2"
      shift 2
      ;;
    --base-image)
      BASE_IMAGE="$2"
      shift 2
      ;;
    --node-version)
      NODE_VERSION="$2"
      shift 2
      ;;
    -h|--help)
      usage
      exit 0
      ;;
    *)
      echo "Unknown argument: $1" >&2
      usage >&2
      exit 1
      ;;
  esac
done

case "$GUEST_OS" in
  macos)
    : "${BASE_IMAGE:=ghcr.io/cirruslabs/macos-sonoma-base:latest}"
    : "${BASE_NAME:=vibe-tart-base}"
    ;;
  linux)
    : "${BASE_IMAGE:=ghcr.io/cirruslabs/ubuntu:latest}"
    : "${BASE_NAME:=vibe-tart-linux-base}"
    ;;
  *)
    echo "Unsupported guest OS: ${GUEST_OS}. Use 'macos' or 'linux'." >&2
    exit 1
    ;;
esac

require_tool() {
  if ! command -v "$1" >/dev/null 2>&1; then
    echo "Missing required tool: $1" >&2
    exit 1
  fi
}

require_tool tart
require_tool curl

cleanup_running_vm() {
  if tart list | grep -Eq "^${BASE_NAME}[[:space:]]+running"; then
    tart stop "$BASE_NAME" >/dev/null
  fi
}

trap cleanup_running_vm EXIT

echo "[1/5] Recreating local Tart base '${BASE_NAME}' from '${BASE_IMAGE}'..."
if tart list | awk '{print $1}' | grep -Fxq "$BASE_NAME"; then
  tart delete "$BASE_NAME" >/dev/null
fi
tart clone "$BASE_IMAGE" "$BASE_NAME"

echo "[2/5] Starting '${BASE_NAME}'..."
nohup tart run --no-graphics "$BASE_NAME" >/tmp/"${BASE_NAME}".log 2>&1 &

echo "[3/5] Waiting for guest shell..."
deadline=$((SECONDS + WAIT_SECONDS))
until tart exec "$BASE_NAME" bash -lc 'echo ready' >/dev/null 2>&1; do
  if (( SECONDS >= deadline )); then
    echo "Timed out waiting for Tart guest readiness. See /tmp/${BASE_NAME}.log" >&2
    exit 1
  fi
  sleep 2
done

echo "[4/5] Installing vibe baseline into '${BASE_NAME}'..."
if [[ "${GUEST_OS}" == "macos" ]]; then
  tart exec "$BASE_NAME" bash -lc "
    set -euo pipefail

    if [ -x /opt/homebrew/bin/brew ]; then
      eval \"\$(/opt/homebrew/bin/brew shellenv)\"
    fi

    brew update
    brew install \
      bash \
      git \
      git-lfs \
      htop \
      jq \
      pipx \
      tree \
      wget \
      zsh-syntax-highlighting || true

    export PATH=\"/opt/homebrew/bin:\$HOME/.local/bin:\$PATH\"
    pipx ensurepath >/dev/null 2>&1 || true

    if [ ! -s \"\$HOME/.nvm/nvm.sh\" ]; then
      curl -o- https://raw.githubusercontent.com/nvm-sh/nvm/${NVM_VERSION}/install.sh | bash
    fi
    export NVM_DIR=\"\$HOME/.nvm\"
    . \"\$NVM_DIR/nvm.sh\"
    nvm install ${NODE_VERSION}
    nvm alias default ${NODE_VERSION}
    nvm use ${NODE_VERSION}

    for pkg in git-filter-repo httpie tldr; do
      if ! pipx list --short 2>/dev/null | grep -Fxq \"\$pkg\"; then
        pipx install \"\$pkg\"
      fi
    done

    if [ ! -x \"\$HOME/.cargo/bin/cargo\" ]; then
      curl https://sh.rustup.rs -sSf | sh -s -- -y
    fi
    export PATH=\"\$HOME/.cargo/bin:\$PATH\"
    rustup default stable

    if ! command -v go >/dev/null 2>&1; then
      brew install go
    fi

    if ! command -v claude >/dev/null 2>&1; then
      curl -fsSL https://claude.ai/install.sh | bash
    fi

    npm install -g \
      @google/gemini-cli \
      @openai/codex \
      eslint \
      npm-check-updates \
      prettier
  "
else
  tart exec "$BASE_NAME" bash -lc "
    set -euo pipefail
    export DEBIAN_FRONTEND=noninteractive

    sudo apt-get update
    sudo apt-get install -y \
      apt-transport-https \
      build-essential \
      ca-certificates \
      curl \
      dnsutils \
      git \
      git-lfs \
      htop \
      iputils-ping \
      jq \
      locales \
      lsof \
      nano \
      netcat-openbsd \
      pipx \
      python3 \
      python3-dev \
      python3-pip \
      python3-venv \
      redis-tools \
      ruby-full \
      software-properties-common \
      telnet \
      tree \
      unzip \
      vim \
      wget \
      zip \
      zsh \
      zsh-syntax-highlighting

    sudo locale-gen en_US.UTF-8
    sudo update-locale LANG=en_US.UTF-8

    if [ ! -s \"\$HOME/.nvm/nvm.sh\" ]; then
      curl -o- https://raw.githubusercontent.com/nvm-sh/nvm/${NVM_VERSION}/install.sh | bash
    fi
    export NVM_DIR=\"\$HOME/.nvm\"
    . \"\$NVM_DIR/nvm.sh\"
    nvm install ${NODE_VERSION}
    nvm alias default ${NODE_VERSION}
    nvm use ${NODE_VERSION}

    export PATH=\"\$HOME/.local/bin:\$PATH\"
    pipx ensurepath >/dev/null 2>&1 || true
    for pkg in git-filter-repo httpie tldr; do
      if ! pipx list --short 2>/dev/null | grep -Fxq \"\$pkg\"; then
        pipx install \"\$pkg\"
      fi
    done

    if [ ! -x \"\$HOME/.cargo/bin/cargo\" ]; then
      curl https://sh.rustup.rs -sSf | sh -s -- -y
    fi
    export PATH=\"\$HOME/.cargo/bin:\$PATH\"
    rustup default stable

    if ! command -v go >/dev/null 2>&1; then
      sudo apt-get install -y golang-go
    fi

    if ! command -v claude >/dev/null 2>&1; then
      curl -fsSL https://claude.ai/install.sh | bash
    fi

    npm install -g \
      @google/gemini-cli \
      @openai/codex \
      eslint \
      npm-check-updates \
      prettier
  "
fi

echo "[5/5] Stopping '${BASE_NAME}'..."
tart stop "$BASE_NAME" >/dev/null

if [[ "${GUEST_OS}" == "macos" ]]; then
  cat <<EOF

Local Tart vibe base is ready: ${BASE_NAME}

Next steps:
  1. Apply the macOS Tart vibe preset in your project:
       vm config preset vibe-tart

  2. Start Tart from the same project directory:
       vm start

  3. Docker remains available when the preset defines a Docker profile:
       vm start docker

This script is the backend for:
  vm base build vibe --provider tart

EOF
else
  cat <<EOF

Local Tart Linux base is ready: ${BASE_NAME}

Next steps:
  1. Apply the Tart vibe preset in your project:
       vm config preset vibe-tart

  2. Start it with:
       vm start tart

  3. If a project needs Docker inside Tart, enable it in vm.yaml:
       tart:
         install_docker: true

This script is the backend for:
  vm base build vibe --provider tart

EOF
fi
