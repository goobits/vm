root="${XDG_DATA_HOME:-$HOME/.local/share}/vm-tools"
states="$root/state"
releases="$root/releases"
canonical_path() {
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
resolved_below() {
  expected=$1
  candidate=$2
  expected="$(canonical_path "$expected" 2>/dev/null || true)"
  candidate="$(canonical_path "$candidate" 2>/dev/null || true)"
  test -n "$expected" && test -n "$candidate" || return 1
  case "$candidate" in
    "$expected"|"$expected"/*) return 0 ;;
    *) return 1 ;;
  esac
}
for state in "$states"/*.state; do
  test -f "$state" || continue
  tab="$(printf '\t')"
  IFS="$tab" read -r name version _target digest < "$state"
  links="$states/$name.links"
  release="$releases/$name/$version-$digest"
  result=yes
  if test ! -d "$release" || test ! -s "$links"; then
    result=no
  else
    while IFS= read -r destination; do
      if test -z "$destination" || test ! -L "$destination" \
        || test ! -e "$destination" \
        || ! resolved_below "$release" "$destination"; then
        result=no
        break
      fi
    done < "$links"
  fi
  printf '%s\t%s\n' "$name" "$result"
done
