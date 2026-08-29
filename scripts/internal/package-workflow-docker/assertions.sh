capture_runtime_state() {
  local ids=$1
  local volumes=$2
  local volume_names=$acceptance_root/volume-names
  : > "$volume_names"
  for container in "${stable_containers[@]}"; do
    docker container inspect --format '{{.Name}} {{.Id}}' "$container"
    docker container inspect --format '{{range .Mounts}}{{if eq .Type "volume"}}{{println .Name}}{{end}}{{end}}' \
      "$container" >> "$volume_names"
  done | sort > "$ids"
  : > "$volumes"
  sort -u "$volume_names" | while IFS= read -r volume; do
    test -z "$volume" || docker volume inspect \
      --format '{{.Name}} {{.CreatedAt}} {{.Mountpoint}}' "$volume"
  done | sort > "$volumes"
}

wait_for_nonempty_log() {
  local log=$1
  local timeout_seconds=$2
  local deadline=$((SECONDS + timeout_seconds))
  while ((SECONDS <= deadline)); do
    test -s "$log" && return 0
    sleep 0.05
  done
  return 1
}

wait_for_guest_vm() {
  local environment=$1
  local deadline=$((SECONDS + 60))
  while ((SECONDS <= deadline)); do
    if docker exec --user acceptance "$environment" \
      sh -lc 'command -v vm >/dev/null 2>&1'; then
      return 0
    fi
    sleep 0.2
  done
  echo "Managed VM client did not reconcile in $environment" >&2
  return 1
}

assert_builder_workspaces_clean() {
  docker exec "$compose_project-builder-1" sh -ec '
    root=${PKG_BUILD_WORK_ROOT:?}
    test -d "$root"
    test -z "$(find "$root" -mindepth 1 -maxdepth 1 -print -quit)"
  '
}

assert_project_dependency_files_unchanged() {
  test "$(docker exec --user acceptance "$environment_name" \
    git -C /workspace hash-object package.json)" = "$project_manifest_digest"
  test "$(docker exec --user acceptance "$environment_name" \
    git -C /workspace hash-object package-lock.json)" = "$project_lock_digest"
  test -z "$(docker exec --user acceptance "$environment_name" \
    git -C /workspace status --porcelain --untracked-files=all)"
}

checkout_source_from_log() {
  local log=$1
  local source
  source=$(sed -n 's/^Source: //p' "$log")
  if test "$(printf '%s\n' "$source" | sed '/^$/d' | wc -l | tr -d ' ')" != 1; then
    cat "$log" >&2
    echo "Package checkout did not report exactly one source" >&2
    exit 3
  fi
  case "$source" in
    /home/acceptance/.local/share/vm/package-checkouts/*/source) ;;
    *)
      cat "$log" >&2
      echo "Managed checkout escaped guest storage: $source" >&2
      exit 3
      ;;
  esac
  printf '%s\n' "$source"
}

workflow_state() {
  docker exec "$compose_project-work-1" cat /data/state/workflows.json
}

assert_release_published_once() {
  local release_version=$1
  workflow_state | python3 -c '
import json, sys

version = sys.argv[1]
state = json.load(sys.stdin)
releases = [item for item in state["releases"].values()
            if item["package"] == "release-tool" and item["version"] == version]
artifacts = [item for item in state["tool_artifacts"].values()
             if item["tool"] == "release-tool" and item["version"] == version]
activations = [item for item in state["tool_activations"].values()
               if item["tool"] == "release-tool" and item["version"] == version]
if (len(releases), len(artifacts), len(activations)) != (1, 2, 1):
    raise SystemExit(
        f"expected one release, two artifacts, and one activation for {version}; "
        f"found {len(releases)}, {len(artifacts)}, {len(activations)}"
    )
submission = state["submissions"][releases[0]["submission_id"]]
progress = submission.get("build_progress") or {}
if progress.get("phase") != "complete" or not progress.get("attempt"):
    raise SystemExit(f"release {version} has no durable completed build progress: {progress}")
' "$release_version"
}

activation_worker_pid() {
  local pid_file=$acceptance_home/.vm/infrastructure/packages/activation-worker.pid
  local attempt pid
  for attempt in $(seq 1 100); do
    if test -s "$pid_file"; then
      pid=$(tr -d '[:space:]' < "$pid_file")
      if test -n "$pid" && kill -0 "$pid" 2>/dev/null; then
        printf '%s\n' "$pid"
        return 0
      fi
    fi
    sleep 0.1
  done
  echo 'Tool activation worker did not start' >&2
  return 1
}

wait_for_queued_activation() {
  local attempt
  for attempt in $(seq 1 400); do
    if workflow_state 2>/dev/null | python3 -c '
import json, sys

state = json.load(sys.stdin)
queued = [item for item in state["tool_activations"].values()
          if item["tool"] == "release-tool"
          and item["version"] == "1.1.0"
          and not item["targets"]]
raise SystemExit(0 if queued else 1)
'; then
      return 0
    fi
    sleep 0.1
  done
  echo 'Tool activation was not persisted before rollout' >&2
  return 1
}

wait_for_package_controller() {
  local attempt
  for attempt in $(seq 1 120); do
    if docker info >/dev/null 2>&1 \
      && test "$(docker container inspect --format '{{.State.Status}}' \
        "$compose_project-work-1" 2>/dev/null)" = running; then
      return 0
    fi
    sleep 0.5
  done
  echo 'Docker package controller did not recover after restart' >&2
  return 1
}

assert_guest_only_checkout() {
  local container
  for container in "${workflow_containers[@]}"; do
    docker exec "$container" sh -ec \
      'test ! -e "$1" && test ! -e "$2"' sh \
      "$checkout_source" "/data/agents/$checkout_id/source"
  done
}
