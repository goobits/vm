set -eu

root=$1
expected=$2
environment=$3
mode=$4
lock="$root/codex.lock"
reaper="$root/codex.lock-reaper"
owns_lock=no
owns_reaper=no

cleanup() {
  status=$?
  trap - EXIT HUP INT TERM
  if test "$owns_lock" = yes; then
    owner="$(cat "$lock/pid" 2>/dev/null || true)"
    if test "$owner" = "$$"; then
      rm -rf "$lock"
    fi
  fi
  if test "$owns_reaper" = yes; then
    rmdir "$reaper" >/dev/null 2>&1 || true
  fi
  exit "$status"
}
trap cleanup EXIT HUP INT TERM

owner_is_running() {
  owner=$1
  case "$owner" in
    ''|*[!0-9]*) return 1 ;;
    *) kill -0 "$owner" >/dev/null 2>&1 ;;
  esac
}

acquire_lock() {
  if mkdir "$lock" 2>/dev/null; then
    printf '%s\n' "$$" > "$lock/pid"
    owns_lock=yes
    return 0
  fi

  owner="$(cat "$lock/pid" 2>/dev/null || true)"
  if owner_is_running "$owner"; then
    return 1
  fi
  if ! mkdir "$reaper" 2>/dev/null; then
    return 1
  fi
  owns_reaper=yes

  owner="$(cat "$lock/pid" 2>/dev/null || true)"
  if test -z "$owner"; then
    sleep 1
    owner="$(cat "$lock/pid" 2>/dev/null || true)"
  fi
  if owner_is_running "$owner"; then
    rmdir "$reaper"
    owns_reaper=no
    return 1
  fi

  stale="$root/codex.lock-stale.$$"
  if mv "$lock" "$stale" 2>/dev/null; then
    rm -rf "$stale"
  fi
  rmdir "$reaper"
  owns_reaper=no

  if mkdir "$lock" 2>/dev/null; then
    printf '%s\n' "$$" > "$lock/pid"
    owns_lock=yes
    return 0
  fi
  return 1
}

attempt=0
until acquire_lock; do
  if test "$mode" = background; then
    exit 0
  fi
  attempt=$((attempt + 1))
  if test "$attempt" -ge 900; then
    printf '%s\n' 'Timed out waiting for another Codex reconciliation' >&2
    exit 1
  fi
  sleep 1
done

probe_state() {
  sh "$root/codex-probe.sh" | sed -n 's/^VM_CODEX_STATE=//p'
}

mark_success() {
  completed="$(date +%s 2>/dev/null || true)"
  case "$completed" in
    ''|*[!0-9]*) return 0 ;;
  esac
  marker="$root/.codex.last-success.$$"
  if printf '%s\n' "$completed" > "$marker"; then
    mv "$marker" "$root/codex.last-success"
  else
    rm -f "$marker"
  fi
}

state="$(probe_state)"
case "$state" in
  consumable)
    mark_success
    exit 0
    ;;
  absent)
    if test "$expected" != yes; then
      exit 0
    fi
    ;;
  incomplete) ;;
  *)
    printf "Codex runtime probe returned an unknown state '%s'\n" "$state" >&2
    exit 1
    ;;
esac

printf "Repairing the Codex standalone runtime in '%s'...\n" "$environment"
if ! sh "$root/codex-repair.sh"; then
  printf "Codex repair failed. Run on the host: vm tools update --to %s\n" \
    "$environment" >&2
  exit 1
fi
if test "$(probe_state)" != consumable; then
  printf "Codex repair did not produce a consumable runtime. Run on the host: vm tools update --to %s\n" \
    "$environment" >&2
  exit 1
fi
mark_success
printf '%s\n' 'Codex standalone runtime is consumable'
