set -eu

name=$1
primary=$2
layout=$3
marker=$4
required=$5

case "$name" in
  ''|*[!a-z0-9-]*) exit 2 ;;
esac
case "$primary" in
  ''|.|..) exit 2 ;;
esac
case "$primary$required" in
  *[!a-zA-Z0-9,._-]*) exit 2 ;;
esac
old_ifs=$IFS
IFS=,
for executable in $required; do
  case "$executable" in
    ''|.|..) IFS=$old_ifs; exit 2 ;;
  esac
done
IFS=$old_ifs

resolve_path() {
  candidate=$1
  if command -v realpath >/dev/null 2>&1; then
    realpath "$candidate" 2>/dev/null && return 0
  fi
  if readlink -f "$candidate" >/dev/null 2>&1; then
    readlink -f "$candidate"
    return
  fi
  depth=0
  while test -L "$candidate"; do
    depth=$((depth + 1))
    test "$depth" -le 40 || return 1
    target=$(readlink "$candidate") || return 1
    case "$target" in
      /*) candidate=$target ;;
      *) candidate="$(dirname "$candidate")/$target" ;;
    esac
  done
  parent=$(CDPATH= cd -P "$(dirname "$candidate")" 2>/dev/null && pwd) || return 1
  printf '%s/%s\n' "$parent" "$(basename "$candidate")"
}

tool_path=$(command -v "$primary" 2>/dev/null || true)
if test -z "$tool_path"; then
  printf '%s\n' VM_VENDOR_TOOL_STATE=absent
  exit 0
fi
if test ! -x "$tool_path"; then
  printf '%s\n' VM_VENDOR_TOOL_STATE=incomplete
  exit 0
fi

resolved=$(resolve_path "$tool_path" 2>/dev/null || printf '%s' "$tool_path")
case "$layout" in
  package)
    case "$marker" in
      ''|.|..|*/*) exit 2 ;;
    esac
    bin_dir=$(dirname "$resolved")
    package_dir=$(dirname "$bin_dir")
    test -n "$marker" && test -f "$package_dir/$marker" || {
      printf '%s\n' VM_VENDOR_TOOL_STATE=incomplete
      exit 0
    }
    IFS=,
    for executable in $required; do
      test -n "$executable" && test -x "$bin_dir/$executable" || {
        IFS=$old_ifs
        printf '%s\n' VM_VENDOR_TOOL_STATE=incomplete
        exit 0
      }
    done
    IFS=$old_ifs
    ;;
  binary) ;;
  *) exit 2 ;;
esac

if version=$("$resolved" --version 2>/dev/null | sed -n '1p') && test -n "$version"; then
  printf '%s\n' VM_VENDOR_TOOL_STATE=consumable
  printf 'VM_VENDOR_TOOL_VERSION=%s\n' "$version"
else
  printf '%s\n' VM_VENDOR_TOOL_STATE=incomplete
fi
