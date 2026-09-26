#!/usr/bin/env bash
# Limit controller-wide discovery to this acceptance run. Named operations are
# forwarded unchanged to the real daemon, exercising the actual provider paths.
set -euo pipefail
if test "${1:-}" = ps; then
  for argument in "$@"; do
    case "$argument" in
      label=com.vm.managed=true|--filter=label=com.vm.managed=true)
        exec "${VM_ACCEPTANCE_REAL_DOCKER:?}" "$@" \
          --filter "name=${VM_ACCEPTANCE_DOCKER_NAME_FILTER:?}"
        ;;
    esac
  done
fi
exec "${VM_ACCEPTANCE_REAL_DOCKER:?}" "$@"
