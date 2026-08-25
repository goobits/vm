set -eu
umask 077

probe_contents=$1
repair_contents=$2
worker_contents=$3
mode=$4
action=$5
expected=$6
environment=$7
name=$8
shift 8
root="${XDG_STATE_HOME:-$HOME/.local/state}/vm-runtime"
mkdir -p "$root"
temporary=

case "$name" in
  ''|*[!a-z0-9-]*) exit 2 ;;
esac

owner_is_running() {
  owner=$(cat "$root/$name.lock/pid" 2>/dev/null || true)
  case "$owner" in
    ''|*[!0-9]*) return 1 ;;
    *) kill -0 "$owner" >/dev/null 2>&1 ;;
  esac
}

recently_completed() {
  completed=$(cat "$root/$name.last-success" 2>/dev/null || true)
  now=$(date +%s 2>/dev/null || true)
  case "$completed" in
    ''|*[!0-9]*) return 1 ;;
  esac
  case "$now" in
    ''|*[!0-9]*) return 1 ;;
  esac
  age=$((now - completed))
  test "$age" -ge 0 && test "$age" -lt 60
}

if test "$mode" = background && owner_is_running; then
  exit 0
fi
if test "$mode" = background && test "$action" = repair && recently_completed; then
  exit 0
fi

cleanup() {
  status=$?
  trap - EXIT HUP INT TERM
  if test -n "$temporary"; then
    find "$temporary" -depth -delete >/dev/null 2>&1 || true
  fi
  exit "$status"
}
trap cleanup EXIT HUP INT TERM

write_script() {
  script_name=$1
  contents=$2
  destination="$root/$script_name"
  temporary=$(mktemp "$root/.$script_name.XXXXXX")
  printf '%s\n' "$contents" > "$temporary"
  chmod 0700 "$temporary"
  mv "$temporary" "$destination"
  temporary=
}

write_script vendor-probe.sh "$probe_contents"
write_script vendor-repair.sh "$repair_contents"
write_script vendor-reconcile.sh "$worker_contents"

case "$mode" in
  wait)
    exec "$root/vendor-reconcile.sh" \
      "$root" "$expected" "$environment" "$mode" "$action" "$name" "$@"
    ;;
  background)
    nohup "$root/vendor-reconcile.sh" \
      "$root" "$expected" "$environment" "$mode" "$action" "$name" "$@" \
      >> "$root/$name.log" 2>&1 </dev/null &
    ;;
  *)
    printf "Unknown vendor-tool reconciliation mode '%s'\n" "$mode" >&2
    exit 1
    ;;
esac
