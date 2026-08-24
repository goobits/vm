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
codex_path="$(command -v codex 2>/dev/null || true)"
if test -z "$codex_path"; then
  printf '%s\n' VM_CODEX_STATE=absent
  exit 0
fi
if test ! -x "$codex_path"; then
  printf '%s\n' VM_CODEX_STATE=incomplete
  exit 0
fi
resolved="$(resolve_path "$codex_path" 2>/dev/null || printf '%s' "$codex_path")"
bin_dir="$(dirname "$resolved")"
package_dir="$(dirname "$bin_dir")"
if test -f "$package_dir/codex-package.json" \
  && test -x "$bin_dir/codex-code-mode-host" \
  && "$resolved" --version >/dev/null 2>&1; then
  printf '%s\n' VM_CODEX_STATE=consumable
else
  printf '%s\n' VM_CODEX_STATE=incomplete
fi
