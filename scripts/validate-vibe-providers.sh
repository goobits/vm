#!/usr/bin/env bash

set -euo pipefail

REBUILD_DOCKER_BASE=false
BUILD_TART_BASE=false
TART_BASE_NAME="${TART_BASE_NAME:-vibe-tart-base}"

usage() {
  cat <<'EOF'
Validate the shared Docker/Tart vibe workflow for the current project.

Usage:
  ./scripts/validate-vibe-providers.sh [--rebuild-docker-base] [--build-tart-base]

Flags:
  --rebuild-docker-base   Rebuild @vibe-box from Dockerfile.vibe before validation
  --build-tart-base       Build the local Tart vibe base before validation

Environment:
  TART_BASE_NAME          Tart base name to write into guidance (default: vibe-tart-base)
EOF
}

while [[ $# -gt 0 ]]; do
  case "$1" in
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

require_tool() {
  if ! command -v "$1" >/dev/null 2>&1; then
    echo "Missing required tool: $1" >&2
    exit 1
  fi
}

require_tool vm

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
REPO_ROOT="$(cd "${SCRIPT_DIR}/.." && pwd)"
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
    vm snapshot create @vibe-box --from-dockerfile "${REPO_ROOT}/Dockerfile.vibe" --force
fi

if [[ "${BUILD_TART_BASE}" == "true" ]]; then
  require_tool tart
  run_step "Building Tart vibe base" \
    "${REPO_ROOT}/scripts/build-vibe-tart-base.sh" --name "${TART_BASE_NAME}"
fi

echo
echo "==> Applying mixed-provider preset in current project"
(cd "${PROJECT_DIR}" && vm config preset vibe-tart)

cat <<EOF

Validation setup is ready for this project.

If you built a Tart base, make sure your vm.yaml points the Tart profile at it:

profiles:
  tart:
    provider: tart
    vm:
      box: ${TART_BASE_NAME}

Run the provider smoke tests from this project directory:

  1. Docker default path
     time vm start

  2. Tart profile path
     time vm --profile tart start

Suggested checks after each start:

  vm exec -- which claude
  vm exec -- which gemini
  vm exec -- which codex
  vm exec -- git config --global user.name
  vm exec -- printenv | grep -E 'EDITOR|PATH' || true
  vm ssh

If both paths work from the same repo and the expected tools are present, the shared vibe workflow is validated.

EOF
