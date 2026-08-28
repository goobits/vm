accept_tool_workflows() {
  docker exec --user acceptance "$environment_name" \
    vm packages checkout agent-skills >"$checkout_log" 2>&1
  checkout_source=$(checkout_source_from_log "$checkout_log")
  checkout_id=$(basename "$(dirname "$checkout_source")")
  docker exec --user acceptance "$environment_name" test -d "$checkout_source/.git"

  assert_guest_only_checkout

  docker exec --user acceptance "$environment_name" sh -ec '
    source=$1
    mkdir -p "$source/skills/acceptance"
    printf "%s\n" "# Guest-owned acceptance skill" > "$source/skills/acceptance/SKILL.md"
    cd "$source"
    test -f skills/acceptance/SKILL.md
    git add skills/acceptance/SKILL.md
    git commit -m "feat: add guest-owned acceptance skill"
  ' sh "$checkout_source"
  checkout_commit=$(docker exec --user acceptance "$environment_name" \
    git -C "$checkout_source" rev-parse HEAD)

  docker restart "$environment_name" >/dev/null
  docker exec --user acceptance "$environment_name" \
    vm packages checkout agent-skills >"$checkout_log.resume" 2>&1
  resumed_source=$(sed -n 's/^Source: //p' "$checkout_log.resume")
  test "$resumed_source" = "$checkout_source"
  test "$(docker exec --user acceptance "$environment_name" \
    git -C "$checkout_source" rev-parse HEAD)" = "$checkout_commit"
  test -z "$(docker exec --user acceptance "$environment_name" \
    git -C "$checkout_source" status --porcelain --untracked-files=all)"
  assert_guest_only_checkout

  set +e
  docker exec --user acceptance "$environment_name" sh -ec \
    'cd "$1" && vm packages release' sh "$checkout_source" \
    >"$workspace_log" 2>&1
  release_status=$?
  set -e
  test "$release_status" -ne 0 || {
    cat "$workspace_log" >&2
    echo "Guest release unexpectedly skipped the version rework" >&2
    echo "Repair: rerun this acceptance script with package review enabled" >&2
    exit 4
  }
  grep -Eq 'Package (review|release) requested changes' "$workspace_log"
  assert_guest_only_checkout

  docker exec --user acceptance "$environment_name" sh -ec '
    source=$1
    sed -i '\''s/"version": "1.0.0"/"version": "1.0.1"/'\'' "$source/package.json"
    cd "$source"
    test "$(sed -n '\''s/.*"version": "\([^"]*\)".*/\1/p'\'' package.json)" = 1.0.1
    git add package.json
    git commit -m "fix: apply requested collection version bump"
    vm packages release
  ' sh "$checkout_source" >>"$workspace_log" 2>&1
  docker exec --user acceptance "$environment_name" test ! -e "$checkout_source"
  assert_guest_only_checkout

  for container in "${workflow_containers[@]}"; do
    if docker container inspect --format '{{range .Mounts}}{{println .Source}}{{end}}' "$container" | \
      grep -Fx "$project_root" >/dev/null; then
      echo "Package infrastructure mounted the canonical project workspace in $container" >&2
      exit 5
    fi
    if docker top "$container" -eo args | grep -E '[c]odex|[c]laude|[a]ntigravity' >/dev/null; then
      echo "Package appliance launched an AI agent in $container" >&2
      echo "Repair: remove agent launchers from package infrastructure" >&2
      exit 5
    fi
  done
  docker exec --user acceptance "$environment_name" sh -ec '
    if env | grep -Eq "^(PKG_WORK_CONTROLLER|PKG_WORK_GIT|PKG_BUILD|PKG_RELEASE|PKG_SERVER_PUBLISH|GIT_ASKPASS|GITHUB_TOKEN|GH_TOKEN|NPM_TOKEN)[^=]*="; then
      exit 1
    fi
    for secret in \
      /run/secrets/git_token \
      /run/secrets/controller_token \
      /run/secrets/build_token \
      /run/build-secrets/build-token \
      /run/secrets/release_token \
      /run/secrets/publish_token; do
      test ! -e "$secret"
    done
  '
  if docker container inspect --format '{{range .Mounts}}{{println .Name}}{{end}}' "$environment_name" | \
    grep -F "${compose_project}_" >/dev/null; then
    echo "Producer guest received writable package-appliance storage" >&2
    echo "Repair: remove appliance volume mounts from managed guests" >&2
    exit 6
  fi

  docker exec --user acceptance "$environment_name" sh -ec '
    clone=/home/acceptance/unregistered-release-clone
    test ! -e "$clone" || find "$clone" -depth -delete
    git clone /workspace "$clone" >/dev/null 2>&1
    cd "$clone"
    if vm packages release >/tmp/unregistered-release.log 2>&1; then
      echo "Unregistered same-origin clone unexpectedly received release authority" >&2
      exit 1
    fi
    grep -F "not the configured canonical workspace" /tmp/unregistered-release.log >/dev/null
    find "$clone" -depth -delete
    rm -f /tmp/unregistered-release.log
  '

  run_project_vm "$consumer_root" tools update --to "$consumer_environment"
  docker exec --user acceptance "$consumer_environment" \
    test -L /home/acceptance/.codex/skills/acceptance
  docker exec --user acceptance "$consumer_environment" \
    grep -F 'Guest-owned acceptance skill' \
    /home/acceptance/.codex/skills/acceptance/SKILL.md >/dev/null
  run_vm tools show agent-skills | grep -F '1.0.1' >/dev/null
  docker run --rm --user 10001:10001 \
    --volume "${compose_project}_source-mirrors:/data/sources:ro" \
    --entrypoint git "$server_image" \
    --git-dir=/data/sources/acceptance-agent-skills.git show main:package.json | \
    grep -F '"version": "1.0.1"' >/dev/null

  if ! docker exec --user acceptance "$environment_name" sh -ec '
    cd /workspace
    test -z "$(git status --porcelain --untracked-files=all)"
    initial=$(git rev-parse HEAD)
    vm packages release
    test "$(git rev-parse HEAD)" = "$initial"
    test -z "$(git status --porcelain --untracked-files=all)"
    test -z "$(git tag --list)"
  ' >"$workspace_log" 2>&1; then
    cat "$workspace_log" >&2
    echo "Automatic canonical workspace release failed" >&2
    echo "Repair: rerun this acceptance script and inspect the package releaser logs" >&2
    exit 4
  fi
  grep -F 'Released release-tool@1.0.0' "$workspace_log" >/dev/null
  grep -F 'Activated in 2 of 2 running environments' "$workspace_log" >/dev/null
  grep -F '1 stopped environment will update when started' "$workspace_log" >/dev/null
  grep -F 'No environments or volumes recreated' "$workspace_log" >/dev/null
  assert_release_published_once 1.0.0
  assert_builder_workspaces_clean

  direct_open_log=$acceptance_root/direct-open.log
  direct_marker=$project_root/.vm-direct-open-acceptance
  checkout_state_before=$(workflow_state | python3 -c \
    'import json,sys; print(json.dumps(json.load(sys.stdin)["checkouts"], sort_keys=True))')
  if ! printf '%s\n' \
    'test "$PWD" = /workspace || exit 41' \
    'printf "%s\n" direct-owner > .vm-direct-open-acceptance' \
    'exit' | run_vm packages open release-tool >"$direct_open_log" 2>&1; then
    cat "$direct_open_log" >&2
    echo "Direct package workspace open failed" >&2
    exit 4
  fi
  grep -F "Opening original workspace for release-tool" "$direct_open_log" >/dev/null
  grep -F "Host source: $project_root" "$direct_open_log" >/dev/null
  grep -F "Mode: direct workspace (no checkout)" "$direct_open_log" >/dev/null
  test "$(cat "$direct_marker")" = direct-owner
  rm "$direct_marker"
  checkout_state_after=$(workflow_state | python3 -c \
    'import json,sys; print(json.dumps(json.load(sys.stdin)["checkouts"], sort_keys=True))')
  test "$checkout_state_after" = "$checkout_state_before" || {
    echo "Direct package workspace open created managed checkout state" >&2
    exit 4
  }

  test "$(docker exec --user acceptance "$environment_name" \
    /home/acceptance/.local/bin/release-tool --version)" = 1.0.0
  test "$(docker exec --user acceptance "$consumer_environment" \
    /home/acceptance/.local/bin/release-tool --version)" = 1.0.0
  test "$(docker container inspect --format '{{.State.Status}}' "$stopped_environment")" = exited

  if ! docker exec --user acceptance "$environment_name" sh -ec '
    cd /workspace
    printf "%s\n" "workspace release change" >> README.md
    git add README.md
    git commit -m "feat: update binary behavior"
    unchanged=$(git rev-parse HEAD)
    if vm packages release; then
      echo "Later workspace release unexpectedly succeeded without a version bump" >&2
      exit 42
    fi
    test "$(git rev-parse HEAD)" = "$unchanged"
    test -z "$(git status --porcelain --untracked-files=all)"
    test -z "$(git tag --list)"
    sed -i "s/version: 1.0.0/version: 1.0.1/" vm-tool.yaml
    git add vm-tool.yaml
    git commit -m "fix: apply requested binary version bump"
    test -z "$(git status --porcelain --untracked-files=all)"
    test -z "$(git tag --list)"
    test "$(git remote get-url origin)" = "https://127.0.0.1:1/release-tool.git"
  ' >>"$workspace_log" 2>&1; then
    cat "$workspace_log" >&2
    echo "Canonical workspace version rework failed" >&2
    exit 4
  fi
  grep -Eq 'Package (review|release) requested changes' "$workspace_log"
  test "$(git -C "$project_root" hash-object package.json)" = "$project_manifest_digest"
  test ! -e "$project_root/package-lock.json"
  if GIT_TERMINAL_PROMPT=0 git -C "$project_root" ls-remote origin >/dev/null 2>&1; then
    echo "Acceptance source origin unexpectedly became reachable" >&2
    echo "Repair: use an unreachable fixture origin and rerun this acceptance script" >&2
    exit 5
  fi

  docker exec --user acceptance "$environment_name" sh -ec '
    destination=/home/acceptance/.local/bin/release-tool
    rm -f "$destination"
    printf "%s\n" "#!/bin/sh" "printf '\''%s\\n'\'' '\''1.0.1'\''" > "$destination"
    chmod 0755 "$destination"
  '
  docker exec --user acceptance "$consumer_environment" sh -ec '
    destination=/home/acceptance/.local/bin/release-tool
    rm -f "$destination"
    printf "%s\n" "#!/bin/sh" "printf '\''%s\\n'\'' '\''unmanaged'\''" > "$destination"
    chmod 0755 "$destination"
  '

  worker_pid=$(activation_worker_pid)
  kill -STOP "$worker_pid"
  docker exec --user acceptance "$environment_name" sh -ec \
    'cd /workspace && vm packages release' >"$activation_log" 2>&1 &
  release_process=$!
  if ! wait_for_nonempty_log "$activation_log" 2; then
    kill -CONT "$worker_pid" 2>/dev/null || true
    wait "$release_process" || true
    cat "$activation_log" >&2
    echo "Release produced no output within two seconds" >&2
    exit 4
  fi
  if ! wait_for_queued_activation; then
    kill -CONT "$worker_pid" 2>/dev/null || true
    wait "$release_process" || true
    cat "$activation_log" >&2
    exit 4
  fi
  kill -CONT "$worker_pid"
  if ! wait_for_pending_activation_plan; then
    wait "$release_process" || true
    cat "$activation_log" >&2
    exit 4
  fi
  kill -STOP "$worker_pid"

  if ! wait_for_log_count "$activation_log" 'Phase: activating privately' 2 12; then
    kill -CONT "$worker_pid" 2>/dev/null || true
    wait "$release_process" || true
    cat "$activation_log" >&2
    echo "Release emitted no activation heartbeat within twelve seconds" >&2
    exit 4
  fi

  if test -n "$docker_restart_command"; then
    bash -lc "$docker_restart_command"
    restart_scope='Docker daemon'
  else
    docker restart "$compose_project-work-1" >/dev/null
    restart_scope='package controller (set VM_ACCEPTANCE_DOCKER_RESTART_COMMAND for a daemon restart)'
  fi
  kill -KILL "$worker_pid" 2>/dev/null || true
  wait_for_package_controller
  run_vm packages doctor --fix >>"$activation_log" 2>&1
  set +e
  wait "$release_process"
  interrupted_release_status=$?
  set -e
  docker exec --user acceptance "$environment_name" sh -ec \
    'cd /workspace && vm packages release' >>"$activation_log" 2>&1
  printf 'Resumable activation restart scope: %s; interrupted release status: %s\n' \
    "$restart_scope" "$interrupted_release_status"

  tool_inventory=$(run_vm tools show release-tool)
  grep -F '1.0.1 linux-arm64' <<< "$tool_inventory" >/dev/null
  test "$(grep -c '  1.0.1 linux-' <<< "$tool_inventory")" = 2
  assert_release_published_once 1.0.1
  assert_builder_workspaces_clean
  test "$(docker exec --user acceptance "$environment_name" \
    /home/acceptance/.local/bin/release-tool --version)" = '1.0.1'
  test "$(docker exec --user acceptance "$consumer_environment" \
    /home/acceptance/.local/bin/release-tool --version)" = '1.0.1'
  docker exec --user acceptance "$environment_name" sh -ec '
    receipt=$(grep -l "^complete$(printf "\t")matched$(printf "\t")" \
      /home/acceptance/.local/share/vm-tools/migrations/release-tool-*.receipt)
    test -n "$receipt"
  '
  docker exec --user acceptance "$consumer_environment" sh -ec '
    receipt=$(grep -l "^complete$(printf "\t")backed_up$(printf "\t")" \
      /home/acceptance/.local/share/vm-tools/migrations/release-tool-*.receipt)
    backup=$(cut -f4 "$receipt")
    test -x "$backup"
    test "$("$backup")" = unmanaged
  '

  test "$(docker container inspect --format '{{.State.Status}}' "$stopped_environment")" = exited
  run_project_vm "$stopped_root" start
  test "$(docker exec --user acceptance "$stopped_environment" \
    /home/acceptance/.local/bin/release-tool --version)" = '1.0.1'

  installed_state=$(docker exec --user acceptance "$consumer_environment" \
    cat /home/acceptance/.local/share/vm-tools/state/release-tool.state)
  IFS=$'\t' read -r installed_tool installed_version installed_target installed_digest <<< "$installed_state"
  test "$installed_tool" = release-tool
  test "$installed_version" = 1.0.1
  test "$installed_target" = linux-amd64
  catalog_file=$(find "$acceptance_home" -type f -name 'index-linux-amd64.json' -print -quit)
  test -n "$catalog_file"
  catalog_digest=$(python3 -c \
    'import json,sys; print(json.load(open(sys.argv[1]))["tools"]["release-tool"]["artifact_digest"])' \
    "$catalog_file")
  test "$installed_digest" = "$catalog_digest"

  inventory_before=$(run_vm tools show release-tool)
  repeat_started=$(date +%s)
  docker exec --user acceptance "$environment_name" sh -ec \
    'cd /workspace && vm packages release'
  repeat_elapsed=$(($(date +%s) - repeat_started))
  test "$repeat_elapsed" -le 10 || {
    echo "Repeated release was not an immediate no-op (${repeat_elapsed}s)" >&2
    exit 4
  }
  assert_release_published_once 1.0.1
  test "$(run_vm tools show release-tool)" = "$inventory_before"
  test "$(docker exec --user acceptance "$consumer_environment" \
    cat /home/acceptance/.local/share/vm-tools/state/release-tool.state)" = "$installed_state"

  capture_runtime_state "$after_ids" "$after_volumes"
  cmp "$before_ids" "$after_ids"
  cmp "$before_volumes" "$after_volumes"

}
