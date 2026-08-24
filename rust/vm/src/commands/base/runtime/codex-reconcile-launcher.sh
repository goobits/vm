set -eu
umask 077

probe_contents=$1
repair_contents=$2
worker_contents=$3
mode=$4
expected=$5
environment=$6
root="${XDG_STATE_HOME:-$HOME/.local/state}/vm-runtime"
mkdir -p "$root"
temporary=

owner_is_running() {
  owner="$(cat "$root/codex.lock/pid" 2>/dev/null || true)"
  case "$owner" in
    ''|*[!0-9]*) return 1 ;;
    *) kill -0 "$owner" >/dev/null 2>&1 ;;
  esac
}

recently_completed() {
  completed="$(cat "$root/codex.last-success" 2>/dev/null || true)"
  now="$(date +%s 2>/dev/null || true)"
  case "$completed" in
    ''|*[!0-9]*) return 1 ;;
  esac
  case "$now" in
    ''|*[!0-9]*) return 1 ;;
  esac
  age=$((now - completed))
  test "$age" -ge 0 && test "$age" -lt 60
}

if test "$mode" = background && { owner_is_running || recently_completed; }; then
  exit 0
fi

cleanup() {
  status=$?
  trap - EXIT HUP INT TERM
  if test -n "$temporary"; then
    rm -f "$temporary"
  fi
  exit "$status"
}
trap cleanup EXIT HUP INT TERM

write_script() {
  name=$1
  contents=$2
  destination="$root/$name"
  temporary="$(mktemp "$root/.$name.XXXXXX")"
  printf '%s\n' "$contents" > "$temporary"
  chmod 0700 "$temporary"
  mv "$temporary" "$destination"
  temporary=
}

write_script codex-probe.sh "$probe_contents"
write_script codex-repair.sh "$repair_contents"
write_script codex-reconcile.sh "$worker_contents"

case "$mode" in
  wait)
    exec "$root/codex-reconcile.sh" "$root" "$expected" "$environment" "$mode"
    ;;
  background)
    nohup "$root/codex-reconcile.sh" "$root" "$expected" "$environment" "$mode" \
      >> "$root/codex.log" 2>&1 </dev/null &
    ;;
  *)
    printf "Unknown Codex reconciliation mode '%s'\n" "$mode" >&2
    exit 1
    ;;
esac
