#!/usr/bin/env bash
set -euo pipefail
umask 077

: "${HOME:?HOME must be set}"

state_dir="$HOME/.local/state/vm"
status_file="$state_dir/bootstrap-node.status"
log_file="$state_dir/bootstrap-node.log"
mkdir -p "$state_dir"
# Provisioning can be repeated while an earlier asynchronous install is still
# running. Serialize installers before changing either their status or log.
# This wrapper runs in Linux containers; Tart uses bootstrap-node.sh directly.
exec 9> "$state_dir/bootstrap-node.lock"
flock 9
printf 'running\n' > "$status_file"

record_result() {
  result=$?
  if [ "$result" -ne 0 ]; then
    printf 'failed\n' > "$status_file"
  elif grep -q 'VM_BOOTSTRAP_DEPENDENCIES_DEFERRED=1' "$log_file"; then
    printf 'deferred\n' > "$status_file"
  else
    printf 'complete\n' > "$status_file"
  fi
}
trap record_result EXIT

exec > "$log_file" 2>&1
bash "$(dirname "${BASH_SOURCE[0]}")/bootstrap-node.sh" 9>&-
