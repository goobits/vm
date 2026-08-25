set -eu

root=$1
expected=$2
environment=$3
mode=$4
action=$5
name=$6
primary=$7
installed_path=$8
layout=$9
marker=${10}
required=${11}
shift 11
lock="$root/$name.lock"
reaper="$root/$name.lock-reaper"
owns_lock=no
owns_reaper=no

case "$name" in
  ''|*[!a-z0-9-]*) exit 2 ;;
esac

delete_tree() {
  path=$1
  test -e "$path" || test -L "$path" || return 0
  find "$path" -depth -delete
}

cleanup() {
  status=$?
  trap - EXIT HUP INT TERM
  if test "$owns_lock" = yes; then
    owner=$(cat "$lock/pid" 2>/dev/null || true)
    if test "$owner" = "$$"; then
      delete_tree "$lock"
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

  owner=$(cat "$lock/pid" 2>/dev/null || true)
  if owner_is_running "$owner"; then
    return 1
  fi
  if ! mkdir "$reaper" 2>/dev/null; then
    return 1
  fi
  owns_reaper=yes

  owner=$(cat "$lock/pid" 2>/dev/null || true)
  if test -z "$owner"; then
    sleep 1
    owner=$(cat "$lock/pid" 2>/dev/null || true)
  fi
  if owner_is_running "$owner"; then
    rmdir "$reaper"
    owns_reaper=no
    return 1
  fi

  stale="$root/$name.lock-stale.$$"
  if mv "$lock" "$stale" 2>/dev/null; then
    delete_tree "$stale"
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
    printf 'Timed out waiting for another %s reconciliation\n' "$name" >&2
    exit 1
  fi
  sleep 1
done

probe_state() {
  sh "$root/vendor-probe.sh" "$name" "$primary" "$layout" "$marker" "$required" \
    | sed -n 's/^VM_VENDOR_TOOL_STATE=//p'
}

mark_success() {
  completed=$(date +%s 2>/dev/null || true)
  case "$completed" in
    ''|*[!0-9]*) return 0 ;;
  esac
  success_marker="$root/.$name.last-success.$$"
  if printf '%s\n' "$completed" > "$success_marker"; then
    mv "$success_marker" "$root/$name.last-success"
  else
    find "$success_marker" -depth -delete >/dev/null 2>&1 || true
  fi
}

state=$(probe_state)
case "$state:$action" in
  consumable:repair)
    mark_success
    exit 0
    ;;
  absent:repair)
    if test "$expected" != yes; then
      exit 0
    fi
    ;;
  absent:update|incomplete:repair|incomplete:update|consumable:update) ;;
  *)
    printf "Vendor tool '%s' returned unknown state/action '%s:%s'\n" \
      "$name" "$state" "$action" >&2
    exit 1
    ;;
esac

if test "$action" = update; then
  printf "Updating vendor tool '%s' in '%s'...\n" "$name" "$environment"
else
  printf "Repairing vendor tool '%s' in '%s'...\n" "$name" "$environment"
fi
if ! sh "$root/vendor-repair.sh" \
  "$name" "$primary" "$installed_path" "$layout" "$marker" "$required" "$@"; then
  printf "Vendor tool '%s' failed. Run on the host: vm tools update %s --to %s\n" \
    "$name" "$name" "$environment" >&2
  exit 1
fi
if test "$(probe_state)" != consumable; then
  printf "Vendor tool '%s' is not consumable. Run on the host: vm tools update %s --to %s\n" \
    "$name" "$name" "$environment" >&2
  exit 1
fi
mark_success
printf "Vendor tool '%s' is consumable\n" "$name"
