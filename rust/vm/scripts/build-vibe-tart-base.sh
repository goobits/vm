#!/usr/bin/env bash

set -euo pipefail

GUEST_OS="${GUEST_OS:-macos}"
BASE_NAME="${BASE_NAME:-}"
BASE_IMAGE="${BASE_IMAGE:-}"
NODE_VERSION="${NODE_VERSION:-22.23.2}"
NVM_COMMIT="${NVM_COMMIT:-d025499c7f5466d0dc0a324dc98eab72cce8377d}"
RUST_TOOLCHAIN="${RUST_TOOLCHAIN:-1.98.0}"
WAIT_SECONDS="${WAIT_SECONDS:-120}"

usage() {
  cat <<'EOF'
Build a local Tart-native vibe base VM.

Usage:
  vm system base build vibe --provider tart [--guest-os macos|linux]

Environment overrides:
  GUEST_OS       Guest OS type to build (default: macos)
  BASE_NAME       Target Tart VM name (default depends on guest OS)
  BASE_IMAGE      Source Tart image (default depends on guest OS)
  NODE_VERSION    Default Node version to preinstall (default: 22.23.2)
  NVM_COMMIT      Pinned NVM installer commit
  RUST_TOOLCHAIN  Pinned Rust toolchain (default: 1.98.0)
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
    : "${BASE_IMAGE:=ghcr.io/cirruslabs/macos-sequoia-base@sha256:785c3acb40fa5af6dd5aab96cd60408372c26125e173c14ea417498d086f829c}"
    : "${BASE_NAME:=vibe-tart-sequoia-base}"
    ;;
  linux)
    : "${BASE_IMAGE:=ghcr.io/cirruslabs/ubuntu@sha256:e018055c421f9d594da78c8a9a5f4c45683e4509ccbdc03091aa928c172c0135}"
    : "${BASE_NAME:=vibe-tart-linux-base}"
    ;;
  *)
    echo "Unsupported guest OS: ${GUEST_OS}. Use 'macos' or 'linux'." >&2
    exit 1
    ;;
esac

if [[ ! "$BASE_NAME" =~ ^[A-Za-z0-9][A-Za-z0-9._-]{0,127}$ ]]; then
  echo "Invalid Tart base name: $BASE_NAME" >&2
  exit 1
fi
if [[ ! "$NODE_VERSION" =~ ^[0-9]+([.][0-9]+){2}$ ]] || \
   [[ ! "$RUST_TOOLCHAIN" =~ ^[0-9]+([.][0-9]+){2}$ ]] || \
   [[ ! "$NVM_COMMIT" =~ ^[0-9a-f]{40}$ ]]; then
  echo "Invalid pinned toolchain version" >&2
  exit 1
fi

require_tool() {
  if ! command -v "$1" >/dev/null 2>&1; then
    echo "Missing required tool: $1" >&2
    exit 1
  fi
}

require_tool tart
require_tool curl

if [[ -z "${VIBE_AI_TOOLS_INSTALLER:-}" ]]; then
  script_dir="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
  installer_path="${script_dir}/install-vibe-ai-tools.sh"
  if [[ ! -f "${installer_path}" ]]; then
    echo "Missing Vibe AI tool installer: ${installer_path}" >&2
    exit 1
  fi
  VIBE_AI_TOOLS_INSTALLER="$(<"${installer_path}")"
fi

cleanup_running_vm() {
  if [[ "${started_vm:-false}" == true ]]; then
    tart stop "$BASE_NAME" >/dev/null
  fi
}

started_vm=false
trap cleanup_running_vm EXIT

log_dir="${XDG_STATE_HOME:-$HOME/.local/state}/vm/tart"
if [[ -L "$log_dir" ]]; then
  echo "Refusing symlinked Tart log directory: $log_dir" >&2
  exit 1
fi
mkdir -p "$log_dir"
chmod 700 "$log_dir"
run_log=$(mktemp "$log_dir/build.XXXXXX.log")
chmod 600 "$run_log"

echo "[1/5] Creating staged Tart base '${BASE_NAME}' from '${BASE_IMAGE}'..."
if tart list | awk '{print $1}' | grep -Fxq "$BASE_NAME"; then
  echo "Refusing to overwrite existing staged base: ${BASE_NAME}" >&2
  exit 1
fi
tart clone "$BASE_IMAGE" "$BASE_NAME"

echo "[2/5] Starting '${BASE_NAME}'..."
nohup tart run --no-graphics "$BASE_NAME" >"$run_log" 2>&1 &
started_vm=true

echo "[3/5] Waiting for guest shell..."
deadline=$((SECONDS + WAIT_SECONDS))
until tart exec "$BASE_NAME" bash -lc 'echo ready' >/dev/null 2>&1; do
  if (( SECONDS >= deadline )); then
    echo "Timed out waiting for Tart guest readiness. See $run_log" >&2
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
      curl -fsSL https://raw.githubusercontent.com/nvm-sh/nvm/${NVM_COMMIT}/install.sh -o /tmp/install-nvm.sh
      bash /tmp/install-nvm.sh
      rm -f /tmp/install-nvm.sh
    fi
    export NVM_DIR=\"\$HOME/.nvm\"
    . \"\$NVM_DIR/nvm.sh\"
    nvm install ${NODE_VERSION}
    nvm alias default ${NODE_VERSION}
    nvm use ${NODE_VERSION}

    for pkg in git-filter-repo httpie tldr; do
      if ! pipx list --short 2>/dev/null | grep -Fxq \"\$pkg\"; then
        case \"\$pkg\" in
          git-filter-repo) version=2.47.0 ;;
          httpie) version=3.2.4 ;;
          tldr) version=3.4.4 ;;
        esac
        pipx install \"\$pkg==\$version\"
      fi
    done

    if [ ! -x \"\$HOME/.cargo/bin/cargo\" ]; then
      rust_arch=\$(uname -m)
      [ \"\$rust_arch\" = arm64 ] && rust_arch=aarch64
      rustup_url=\"https://static.rust-lang.org/rustup/dist/\${rust_arch}-apple-darwin/rustup-init\"
      curl --proto '=https' --tlsv1.2 -fsSL \"\$rustup_url\" -o /tmp/rustup-init
      curl --proto '=https' --tlsv1.2 -fsSL \"\$rustup_url.sha256\" -o /tmp/rustup-init.sha256
      expected=\$(awk '{print \$1}' /tmp/rustup-init.sha256)
      actual=\$(shasum -a 256 /tmp/rustup-init | awk '{print \$1}')
      [ \"\$actual\" = \"\$expected\" ]
      chmod 700 /tmp/rustup-init
      /tmp/rustup-init -y --default-toolchain ${RUST_TOOLCHAIN}
      rm -f /tmp/rustup-init /tmp/rustup-init.sha256
    fi
    export PATH=\"\$HOME/.cargo/bin:\$PATH\"
    rustup default ${RUST_TOOLCHAIN}

    if ! command -v go >/dev/null 2>&1; then
      brew install go
    fi

    npm install -g \
      eslint@10.9.1 \
      npm-check-updates@23.1.0 \
      prettier@3.9.6
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
      curl -fsSL https://raw.githubusercontent.com/nvm-sh/nvm/${NVM_COMMIT}/install.sh -o /tmp/install-nvm.sh
      bash /tmp/install-nvm.sh
      rm -f /tmp/install-nvm.sh
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
        case \"\$pkg\" in
          git-filter-repo) version=2.47.0 ;;
          httpie) version=3.2.4 ;;
          tldr) version=3.4.4 ;;
        esac
        pipx install \"\$pkg==\$version\"
      fi
    done

    if [ ! -x \"\$HOME/.cargo/bin/cargo\" ]; then
      rust_arch=\$(uname -m)
      [ \"\$rust_arch\" = x86_64 ] || rust_arch=aarch64
      rustup_url=\"https://static.rust-lang.org/rustup/dist/\${rust_arch}-unknown-linux-gnu/rustup-init\"
      curl --proto '=https' --tlsv1.2 -fsSL \"\$rustup_url\" -o /tmp/rustup-init
      curl --proto '=https' --tlsv1.2 -fsSL \"\$rustup_url.sha256\" -o /tmp/rustup-init.sha256
      expected=\$(awk '{print \$1}' /tmp/rustup-init.sha256)
      echo \"\$expected  /tmp/rustup-init\" | sha256sum --check -
      chmod 700 /tmp/rustup-init
      /tmp/rustup-init -y --default-toolchain ${RUST_TOOLCHAIN}
      rm -f /tmp/rustup-init /tmp/rustup-init.sha256
    fi
    export PATH=\"\$HOME/.cargo/bin:\$PATH\"
    rustup default ${RUST_TOOLCHAIN}

    if ! command -v go >/dev/null 2>&1; then
      sudo apt-get install -y golang-go
    fi

    npm install -g \
      eslint@10.9.1 \
      npm-check-updates@23.1.0 \
      prettier@3.9.6
  "
fi

tart exec "$BASE_NAME" bash -lc "$VIBE_AI_TOOLS_INSTALLER" -- \
  antigravity claude codex

echo "[5/5] Stopping '${BASE_NAME}'..."
tart stop "$BASE_NAME" >/dev/null

if [[ "${GUEST_OS}" == "macos" ]]; then
  cat <<EOF

Local Tart vibe base is ready: ${BASE_NAME}

Next steps:
  1. Apply the macOS Tart vibe preset in your project:
       vm config preset vibe-tart

  2. Start Tart from the same project directory:
       vm run mac

  3. Docker inside the macOS guest uses Colima with QEMU software emulation and
     is much slower than Docker in the Linux Tart profile:
       tart:
         install_docker: true

This script is the backend for:
  vm system base build vibe --provider tart

EOF
else
  cat <<EOF

Local Tart Linux base is ready: ${BASE_NAME}

Next steps:
  1. Apply the Tart vibe preset in your project:
       vm config preset vibe-tart

  2. Start it with:
       vm run linux --provider tart

  3. The vibe-tart preset enables Docker inside the Linux guest:
       tart:
         install_docker: true

This script is the backend for:
  vm system base build vibe --provider tart

EOF
fi
