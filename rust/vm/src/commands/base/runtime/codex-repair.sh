set -eu

system_prefix=${1:-/usr/local}
installer_override=${2:-}
case "$system_prefix" in
  /*) ;;
  *)
    printf '%s\n' 'Codex repair requires an absolute installation prefix' >&2
    exit 1
    ;;
esac
root="$system_prefix/lib/vm-ai-tools"
bin_root="$system_prefix/bin"
target="$root/codex-package"
user_home=$HOME
user_bin="$user_home/.local/bin"

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

run_install() {
  if test -d "$system_prefix" && test -w "$system_prefix"; then
    "$@"
  elif test ! -e "$system_prefix" && test -w "$(dirname "$system_prefix")"; then
    "$@"
  elif test "$(id -u)" -eq 0; then
    "$@"
  elif command -v sudo >/dev/null 2>&1; then
    sudo -n "$@"
  else
    printf '%s\n' 'Codex repair requires root or passwordless sudo' >&2
    return 1
  fi
}

path_exists() {
  run_install test -e "$1" || run_install test -L "$1"
}

require_managed_launcher() {
  launcher=$1
  shift
  if ! path_exists "$launcher"; then
    return 0
  fi
  if ! run_install test -L "$launcher"; then
    printf 'Refusing to replace unmanaged launcher: %s\n' "$launcher" >&2
    return 1
  fi
  launcher_target="$(run_install readlink "$launcher")"
  for managed_target in "$@"; do
    if test "$launcher_target" = "$managed_target"; then
      return 0
    fi
  done
  printf 'Refusing to replace unmanaged launcher: %s -> %s\n' \
    "$launcher" "$launcher_target" >&2
  return 1
}

legacy_nvm_codex_launcher() {
  launcher=$1
  if ! run_install test -L "$launcher"; then
    return 1
  fi
  launcher_target="$(run_install readlink "$launcher")"
  case "$launcher_target" in
    "$user_home"/.nvm/versions/node/v*/bin/codex)
      run_install test -x "$launcher_target"
      ;;
    *) return 1 ;;
  esac
}

legacy_user_codex_launcher() {
  launcher=$1
  if ! test -L "$launcher"; then
    return 1
  fi
  launcher_target="$(readlink "$launcher")"
  test "$launcher_target" = \
    "$user_home/.codex/packages/standalone/current/bin/codex"
}

if ! legacy_nvm_codex_launcher "$bin_root/codex"; then
  require_managed_launcher \
    "$bin_root/codex" "$root/codex" "$target/bin/codex"
fi
require_managed_launcher \
  "$bin_root/codex-code-mode-host" \
  "$root/codex-code-mode-host" "$target/bin/codex-code-mode-host"
mkdir -p "$user_bin"
if ! legacy_user_codex_launcher "$user_bin/codex"; then
  require_managed_launcher \
    "$user_bin/codex" \
    "$root/codex" "$target/bin/codex" "$bin_root/codex"
fi
require_managed_launcher \
  "$user_bin/codex-code-mode-host" \
  "$root/codex-code-mode-host" "$target/bin/codex-code-mode-host" \
  "$bin_root/codex-code-mode-host"

temporary="$(mktemp -d "${TMPDIR:-/tmp}/vm-codex-reconcile.XXXXXX")"
stage=""
backup=""
rollback_needed=no

backup_path() {
  source_path=$1
  backup_name=$2
  if path_exists "$source_path"; then
    run_install mv "$source_path" "$backup/$backup_name"
  else
    run_install touch "$backup/.absent-$backup_name"
  fi
}

restore_path() {
  backup_name=$1
  destination=$2
  if path_exists "$backup/$backup_name"; then
    run_install rm -rf "$destination" >/dev/null 2>&1 || true
    run_install mv "$backup/$backup_name" "$destination" \
      >/dev/null 2>&1 || true
  elif path_exists "$backup/.absent-$backup_name"; then
    run_install rm -rf "$destination" >/dev/null 2>&1 || true
  fi
}

cleanup() {
  status=$?
  trap - EXIT HUP INT TERM
  if test "$rollback_needed" = yes; then
    restore_path package "$target"
    restore_path root-codex "$root/codex"
    restore_path root-code-mode "$root/codex-code-mode-host"
    restore_path bin-codex "$bin_root/codex"
    restore_path bin-code-mode "$bin_root/codex-code-mode-host"
    restore_path user-codex "$user_bin/codex"
    restore_path user-code-mode "$user_bin/codex-code-mode-host"
  fi
  rm -rf "$temporary"
  if test -n "$stage"; then
    run_install rm -rf "$stage" >/dev/null 2>&1 || true
  fi
  if test -n "$backup"; then
    run_install rm -rf "$backup" >/dev/null 2>&1 || true
  fi
  exit "$status"
}
trap cleanup EXIT HUP INT TERM

export HOME="$temporary/home"
mkdir -p "$HOME/.local/bin"
export PATH="$HOME/.local/bin:$PATH"
installer="$temporary/install-codex.sh"
if test -n "$installer_override"; then
  cp "$installer_override" "$installer"
else
  curl --fail --silent --show-error --location \
    --connect-timeout 10 --max-time 600 --retry 2 \
    --output "$installer" https://chatgpt.com/codex/install.sh
fi
sh "$installer"
hash -r

codex_path="$(command -v codex)"
resolved="$(resolve_path "$codex_path")"
bin_dir="$(dirname "$resolved")"
package_source="$(dirname "$bin_dir")"
test -f "$package_source/codex-package.json"
test -x "$bin_dir/codex-code-mode-host"
"$resolved" --version >/dev/null

run_install install -d -m 0755 "$root" "$bin_root"
stage="$(run_install mktemp -d "$root/.codex-stage.XXXXXX")"
run_install cp -R "$package_source/." "$stage/"
run_install chmod -R go-w,a+rX "$stage"
test -f "$stage/codex-package.json"
test -x "$stage/bin/codex"
test -x "$stage/bin/codex-code-mode-host"
"$stage/bin/codex" --version >/dev/null

backup="$(run_install mktemp -d "$root/.codex-backup.XXXXXX")"
rollback_needed=yes
backup_path "$target" package
backup_path "$root/codex" root-codex
backup_path "$root/codex-code-mode-host" root-code-mode
backup_path "$bin_root/codex" bin-codex
backup_path "$bin_root/codex-code-mode-host" bin-code-mode
backup_path "$user_bin/codex" user-codex
backup_path "$user_bin/codex-code-mode-host" user-code-mode

run_install mv "$stage" "$target"
stage=""

run_install ln -s "$target/bin/codex" "$root/codex"
run_install ln -s "$target/bin/codex-code-mode-host" "$root/codex-code-mode-host"
run_install ln -s "$root/codex" "$bin_root/codex"
run_install ln -s "$root/codex-code-mode-host" "$bin_root/codex-code-mode-host"
ln -s "$root/codex" "$user_bin/codex"
ln -s "$root/codex-code-mode-host" "$user_bin/codex-code-mode-host"
test -x "$bin_root/codex-code-mode-host"
test -x "$user_bin/codex-code-mode-host"
"$bin_root/codex" --version >/dev/null
rollback_needed=no
run_install rm -rf "$backup"
backup=""
