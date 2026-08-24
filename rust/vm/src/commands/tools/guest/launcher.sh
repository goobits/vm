set -eu
umask 077
IFS= read -r CARGO_REGISTRIES_VM_TOKEN
test -n "$CARGO_REGISTRIES_VM_TOKEN"
export CARGO_REGISTRIES_VM_TOKEN
root="${XDG_DATA_HOME:-$HOME/.local/share}/vm-tools"
mkdir -p "$root"
mode=$2
case "$mode" in
  background-if-idle|background|wait) ;;
  *)
    printf "Unknown tool reconciliation mode '%s'\n" "$mode" >&2
    exit 1
    ;;
esac
owner_is_running() {
  owner="$(cat "$root/update.lock/pid" 2>/dev/null || true)"
  case "$owner" in
    ''|*[!0-9]*) return 1 ;;
    *) kill -0 "$owner" >/dev/null 2>&1 ;;
  esac
}

recently_completed() {
  completed="$(cat "$root/update.last-success" 2>/dev/null || true)"
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

if test "$mode" != wait && owner_is_running; then
  exit 0
fi
if test "$mode" = background-if-idle && recently_completed; then
  exit 0
fi

script="$root/installer.sh"
temporary="$root/.installer.$$.tmp"
printf '%s\n' "$1" > "$temporary"
chmod 700 "$temporary"
mv -f "$temporary" "$script"
shift 2
case "$mode" in
  background-if-idle|background)
    nohup "$script" "$mode" "$@" > "$root/update.log" 2>&1 </dev/null &
    ;;
  wait)
    exec "$script" "$mode" "$@"
    ;;
esac
