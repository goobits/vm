#!/usr/bin/env bash

set -euo pipefail

REBUILD_DOCKER_BASE=false
BUILD_TART_BASE=false
PROVIDER="${PROVIDER:-all}"
TART_BASE_NAME="${TART_BASE_NAME:-vibe-tart-sequoia-base}"

usage() {
  cat <<'EOF'
Validate the shared Docker plus macOS-Tart vibe workflow for the current project.

Usage:
  ./scripts/internal/validate-vibe-providers.sh [--provider docker|tart|all] [--rebuild-docker-base] [--build-tart-base]

Flags:
  --provider docker|tart|all  Limit validation guidance to one provider (default: all)
  --rebuild-docker-base   Rebuild @vibe-box from Dockerfile.vibe before validation
  --build-tart-base       Build the local macOS Tart vibe base before validation

Environment:
  PROVIDER                Provider focus for validation output (default: all)
  TART_BASE_NAME          Tart base name to write into guidance (default: vibe-tart-sequoia-base)
EOF
}

while [[ $# -gt 0 ]]; do
  case "$1" in
    --provider)
      PROVIDER="$2"
      shift 2
      ;;
    --rebuild-docker-base)
      REBUILD_DOCKER_BASE=true
      shift
      ;;
    --build-tart-base)
      BUILD_TART_BASE=true
      shift
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

case "${PROVIDER}" in
  docker|tart|all)
    ;;
  *)
    echo "Invalid provider: ${PROVIDER}" >&2
    usage >&2
    exit 1
    ;;
esac

require_tool() {
  if ! command -v "$1" >/dev/null 2>&1; then
    echo "Missing required tool: $1" >&2
    exit 1
  fi
}

require_tool vm

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
REPO_ROOT="$(cd "${SCRIPT_DIR}/../.." && pwd)"
PROJECT_DIR="$(pwd)"

run_step() {
  local label="$1"
  shift
  echo
  echo "==> ${label}"
  "$@"
}

if [[ "${REBUILD_DOCKER_BASE}" == "true" ]]; then
  require_tool docker
  run_step "Rebuilding Docker vibe base" \
    vm system base build vibe --provider docker
fi

if [[ "${BUILD_TART_BASE}" == "true" ]]; then
  require_tool tart
  run_step "Building Tart vibe base" \
    "${REPO_ROOT}/scripts/internal/build-vibe-tart-base.sh" --name "${TART_BASE_NAME}"
fi

echo
echo "==> Applying provider-ready vibe preset in current project"
(cd "${PROJECT_DIR}" && vm config preset vibe-tart)

cat <<EOF

Validation setup is ready for this project.

Run the provider smoke tests from this project directory:

EOF

if [[ "${PROVIDER}" == "docker" || "${PROVIDER}" == "all" ]]; then
  cat <<'EOF'
  1. Docker provider path
     time vm run linux --provider docker

EOF
fi

if [[ "${PROVIDER}" == "tart" || "${PROVIDER}" == "all" ]]; then
  cat <<'EOF'
  2. Tart default path
     time vm run mac

EOF
fi

cat <<'EOF'
Suggested checks after each start:

  vm exec -- which claude
  vm exec -- which gemini
  vm exec -- which codex
  vm exec -- git config --global user.name
  vm exec -- printenv | grep -E 'EDITOR|PATH' || true
  vm shell

If both paths work from the same repo and the expected tools are present, the shared vibe workflow is validated.

This script is the backend for:
  vm system base validate vibe

EOF
