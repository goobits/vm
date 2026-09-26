# Exercise controller metadata changes and appliance maintenance independently of
# publication. Every resource belongs to this workflow's disposable HOME/project.
accept_tool_selection() {
  local before_state selection_log
  selection_log=$acceptance_root/tool-selection.log
  before_state=$(docker exec --user acceptance "$consumer_environment" \
    cat /home/acceptance/.local/share/vm-tools/state/release-tool.state)
  run_project_vm "$consumer_root" tools refresh release-tool >"$selection_log" 2>&1
  grep -F 'Tool catalog refreshed for release-tool' "$selection_log" >/dev/null
  test "$(docker exec --user acceptance "$consumer_environment" \
    cat /home/acceptance/.local/share/vm-tools/state/release-tool.state)" = "$before_state"
  if run_project_vm "$consumer_root" tools refresh no-such-acceptance-tool \
    >>"$selection_log" 2>&1; then
    echo 'Named tool refresh accepted an unregistered tool' >&2
    return 1
  fi

  run_project_vm "$stopped_root" stop
  run_vm tools enable vm-acceptance-skills >>"$selection_log" 2>&1
  run_vm config show --scope user --json | python3 -c '
import json,sys
assert "vm-acceptance-skills" in json.load(sys.stdin)["data"]["config"].get("tools", {})
'
  run_vm tools disable vm-acceptance-skills >>"$selection_log" 2>&1
  run_vm config show --scope user --json | python3 -c '
import json,sys
assert "vm-acceptance-skills" not in json.load(sys.stdin)["data"]["config"].get("tools", {})
'
  test "$(docker container inspect --format '{{.State.Status}}' "$stopped_environment")" = exited
  # Disabling global selection preserves installed files and project selections.
  docker exec --user acceptance "$consumer_environment" \
    test -f /home/acceptance/.codex/skills/acceptance/SKILL.md
  test "$(docker exec --user acceptance "$consumer_environment" \
    /home/acceptance/.local/bin/release-tool --version)" = 1.1.0
  run_project_vm "$stopped_root" start
  test "$(docker exec --user acceptance "$stopped_environment" \
    /home/acceptance/.local/bin/release-tool --version)" = 1.1.0
  docker exec --user acceptance "$stopped_environment" \
    test ! -e /home/acceptance/.local/share/vm-tools/state/vm-acceptance-skills.state
}

accept_package_backups() {
  local backup_log automatic_backup backup_name marker damage
  backup_log=$acceptance_root/package-backups.log
  backup_name=acceptance-state
  marker=/data/state/acceptance-backup-marker
  docker exec "$compose_project-work-1" sh -ec \
    'printf "%s\n" original > "$1"' sh "$marker"
  run_vm packages service backups create "$backup_name" >"$backup_log" 2>&1
  run_vm packages service backups list | grep -Fx "$backup_name" >/dev/null
  if run_vm packages service backups create "$backup_name" >>"$backup_log" 2>&1; then
    echo 'Package backup creation overwrote an existing backup' >&2
    return 1
  fi
  docker exec "$compose_project-work-1" sh -ec \
    'printf "%s\n" changed > "$1"' sh "$marker"
  if run_vm packages service backups restore no-such-acceptance-backup --yes \
    >>"$backup_log" 2>&1; then
    echo 'Package restore accepted a missing backup' >&2
    return 1
  fi
  test "$(docker exec "$compose_project-work-1" cat "$marker")" = changed
  # Reject incompatible metadata and checksum damage before pausing services or
  # replacing any data, then repair the test archive and restore it successfully.
  for damage in format checksum; do
    docker run --rm --user 0:0 \
      --volume "${compose_project}_infrastructure-backups:/backups" \
      --entrypoint /bin/sh "$jobs_image" -ec '
        receipt=/backups/$1/receipt.txt
        cp "$receipt" "$receipt.original"
        if test "$2" = format; then
          sed -i "s/format_version=1/format_version=999/" "$receipt"
        else
          printf "%s\n" checksum-damage >> "$receipt"
        fi
      ' sh "$backup_name" "$damage"
    if run_vm packages service backups restore "$backup_name" --yes \
      >>"$backup_log" 2>&1; then
      echo "Package restore accepted $damage damage" >&2
      return 1
    fi
    test "$(docker exec "$compose_project-work-1" cat "$marker")" = changed
    docker run --rm --user 0:0 \
      --volume "${compose_project}_infrastructure-backups:/backups" \
      --entrypoint /bin/sh "$jobs_image" -ec \
      'mv "/backups/$1/receipt.txt.original" "/backups/$1/receipt.txt"' sh "$backup_name"
  done
  run_vm packages service backups restore "$backup_name" --yes >>"$backup_log" 2>&1
  test "$(docker exec "$compose_project-work-1" cat "$marker")" = original
  run_vm packages service backups create >"$backup_log.automatic" 2>&1
  automatic_backup=$(sed -n 's/^Backup: //p' "$backup_log.automatic")
  case "$automatic_backup" in
    backup-*) ;;
    *) cat "$backup_log.automatic" >&2; return 1 ;;
  esac
  run_vm packages service backups remove "$backup_name" --yes >>"$backup_log" 2>&1
  run_vm packages service backups list | grep -Fx "$automatic_backup" >/dev/null
  if run_vm packages service backups list | grep -Fx "$backup_name" >/dev/null; then
    echo 'Removed backup is still listed' >&2
    return 1
  fi
  run_vm packages service backups remove "$automatic_backup" --yes >>"$backup_log" 2>&1
  run_vm packages service backups list >"$backup_log.final-list"
  grep -Fx 'No package infrastructure backups' "$backup_log.final-list" >/dev/null
  docker exec "$compose_project-work-1" rm "$marker"
  capture_runtime_state "$after_ids" "$after_volumes"
  cmp "$before_ids" "$after_ids"
  cmp "$before_volumes" "$after_volumes"
}
