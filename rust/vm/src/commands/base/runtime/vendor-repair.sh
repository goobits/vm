set -eu

name=$1
primary=$2
installed_path=$3
layout=$4
marker=$5
required=$6
system_prefix=$7
installer_override=$8
installer_url=$9
installer_shell=${10}
approved_user_scope=${11}
shift 11

case "$name" in
  ''|*[!a-z0-9-]*) exit 2 ;;
esac
case "$primary" in
  ''|.|..) exit 2 ;;
esac
case "$installed_path" in
  ''|/*|*..*) exit 2 ;;
esac
case "$primary$required" in
  *[!a-zA-Z0-9,._-]*) exit 2 ;;
esac
case "$system_prefix" in
  /*) ;;
  *)
    printf '%s\n' 'Vendor-tool repair requires an absolute installation prefix' >&2
    exit 1
    ;;
esac
case "$installer_url" in
  https://*) ;;
  *)
    printf '%s\n' 'Vendor-tool installer URL must use HTTPS' >&2
    exit 1
    ;;
esac
case "$installer_shell" in
  sh|bash) ;;
  *)
    printf '%s\n' 'Vendor-tool installer shell must be sh or bash' >&2
    exit 1
    ;;
esac
case "$layout" in
  package)
    case "$marker" in
      ''|.|..|*/*) exit 2 ;;
    esac
    ;;
  binary) ;;
  *) exit 2 ;;
esac

root="$system_prefix/lib/vm-ai-tools"
bin_root="$system_prefix/bin"
target="$root/$name-package"
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
    link_target=$(readlink "$candidate") || return 1
    case "$link_target" in
      /*) candidate=$link_target ;;
      *) candidate="$(dirname "$candidate")/$link_target" ;;
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
    printf '%s\n' 'Vendor-tool repair requires root or passwordless sudo' >&2
    return 1
  fi
}

path_exists() {
  run_install test -e "$1" || run_install test -L "$1"
}

delete_installed_path() {
  destination=$1
  path_exists "$destination" || return 0
  run_install find "$destination" -depth -delete
}

approved_user_launcher() {
  launcher=$1
  executable=$2
  test "$launcher" = "$user_bin/$executable" || return 1
  case "$approved_user_scope" in
    symlink:*)
      run_install test -L "$launcher" || return 1
      relative=${approved_user_scope#symlink:}
      case "$relative" in
        /*|*..*) return 1 ;;
      esac
      allowed="$user_home/$relative"
      resolved=$(resolve_path "$launcher" 2>/dev/null || true)
      if test -n "$resolved"; then
        case "$resolved" in
          "$allowed"/*) return 0 ;;
        esac
      fi
      launcher_target=$(run_install readlink "$launcher") || return 1
      case "$launcher_target" in
        "$allowed"/*) return 0 ;;
        *) return 1 ;;
      esac
      ;;
    file:*)
      relative=${approved_user_scope#file:}
      case "$relative" in
        /*|*..*) return 1 ;;
      esac
      test "$launcher" = "$user_home/$relative" && run_install test -f "$launcher"
      ;;
    none) return 1 ;;
    *) return 1 ;;
  esac
}

require_managed_launcher() {
  launcher=$1
  executable=$2
  shift 2
  if ! path_exists "$launcher"; then
    return 0
  fi
  if run_install test -L "$launcher"; then
    launcher_target=$(run_install readlink "$launcher")
    for managed_target in "$@"; do
      if test "$launcher_target" = "$managed_target"; then
        return 0
      fi
    done
  fi
  if test "$launcher" = "$root/$executable" \
    && run_install test -f "$launcher" \
    && run_install test -f "$target/bin/$executable" \
    && run_install cmp -s "$launcher" "$target/bin/$executable"; then
    return 0
  fi
  if approved_user_launcher "$launcher" "$executable"; then
    return 0
  fi
  printf 'Refusing to replace unmanaged launcher: %s\n' "$launcher" >&2
  return 1
}

old_ifs=$IFS
IFS=,
for executable in $required; do
  case "$executable" in
    ''|.|..) IFS=$old_ifs; exit 2 ;;
  esac
  require_managed_launcher \
    "$root/$executable" "$executable" "$target/bin/$executable"
  require_managed_launcher \
    "$bin_root/$executable" "$executable" \
    "$root/$executable" "$target/bin/$executable"
done
IFS=$old_ifs
mkdir -p "$user_bin"
IFS=,
for executable in $required; do
  require_managed_launcher \
    "$user_bin/$executable" "$executable" \
    "$root/$executable" "$target/bin/$executable" "$bin_root/$executable"
done
IFS=$old_ifs

temporary=$(mktemp -d "${TMPDIR:-/tmp}/vm-vendor-$name.XXXXXX")
stage=
backup=
rollback_needed=no

delete_user_tree() {
  destination=$1
  test -e "$destination" || test -L "$destination" || return 0
  find "$destination" -depth -delete
}

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
    delete_installed_path "$destination"
    run_install mv "$backup/$backup_name" "$destination"
  elif path_exists "$backup/.absent-$backup_name"; then
    delete_installed_path "$destination"
  fi
}

cleanup() {
  status=$?
  trap - EXIT HUP INT TERM
  if test "$rollback_needed" = yes; then
    rollback_failed=no
    if ! restore_path package "$target"; then rollback_failed=yes; fi
    IFS=,
    for executable in $required; do
      if ! restore_path "root-$executable" "$root/$executable"; then rollback_failed=yes; fi
      if ! restore_path "bin-$executable" "$bin_root/$executable"; then rollback_failed=yes; fi
      if ! restore_path "user-$executable" "$user_bin/$executable"; then rollback_failed=yes; fi
    done
    IFS=$old_ifs
    if test "$rollback_failed" = yes; then
      printf 'Vendor-tool rollback failed; backup retained at %s\n' "$backup" >&2
      status=1
    fi
  fi
  delete_user_tree "$temporary" >/dev/null 2>&1 || true
  if test -n "$stage"; then
    delete_installed_path "$stage" >/dev/null 2>&1 || true
  fi
  if test -n "$backup" && { test "$rollback_needed" != yes || test "${rollback_failed:-no}" != yes; }; then
    delete_installed_path "$backup" >/dev/null 2>&1 || true
  fi
  exit "$status"
}
trap cleanup EXIT HUP INT TERM

export HOME="$temporary/home"
mkdir -p "$HOME/.local/bin"
export PATH="$HOME/.local/bin:$PATH"
installer="$temporary/install-$name.sh"
if test -n "$installer_override"; then
  cp "$installer_override" "$installer"
else
  curl --fail --silent --show-error --location \
    --connect-timeout 10 --max-time 600 --retry 2 \
    --output "$installer" "$installer_url"
fi
"$installer_shell" "$installer" "$@"
tool_path="$HOME/$installed_path"
test -x "$tool_path"
resolved=$(resolve_path "$tool_path")
staging_home=$(resolve_path "$HOME")
case "$resolved" in
  "$staging_home"/*) ;;
  *)
    printf 'Vendor installer produced an artifact outside its staging home: %s\n' \
      "$resolved" >&2
    exit 1
    ;;
esac
"$resolved" --version >/dev/null

run_install install -d -m 0755 "$root" "$bin_root"
stage=$(run_install mktemp -d "$root/.$name-stage.XXXXXX")
run_install install -d -m 0755 "$stage/bin"
case "$layout" in
  package)
    source_bin=$(dirname "$resolved")
    package_source=$(dirname "$source_bin")
    test -f "$package_source/$marker"
    run_install find "$stage" -depth -delete
    stage=$(run_install mktemp -d "$root/.$name-stage.XXXXXX")
    run_install cp -R "$package_source/." "$stage/"
    ;;
  binary)
    run_install cp "$resolved" "$stage/bin/$primary"
    ;;
esac
run_install chmod -R go-w,a+rX "$stage"
if test "$layout" = package; then
  test -f "$stage/$marker"
fi
IFS=,
for executable in $required; do
  test -x "$stage/bin/$executable"
done
IFS=$old_ifs
"$stage/bin/$primary" --version >/dev/null

backup=$(run_install mktemp -d "$root/.$name-backup.XXXXXX")
rollback_needed=yes
backup_path "$target" package
IFS=,
for executable in $required; do
  backup_path "$root/$executable" "root-$executable"
  backup_path "$bin_root/$executable" "bin-$executable"
  backup_path "$user_bin/$executable" "user-$executable"
done
IFS=$old_ifs

run_install mv "$stage" "$target"
stage=
IFS=,
for executable in $required; do
  run_install ln -s "$target/bin/$executable" "$root/$executable"
  run_install ln -s "$root/$executable" "$bin_root/$executable"
  ln -s "$root/$executable" "$user_bin/$executable"
  test -x "$bin_root/$executable"
  test -x "$user_bin/$executable"
done
IFS=$old_ifs
"$bin_root/$primary" --version >/dev/null
rollback_needed=no
delete_installed_path "$backup"
backup=
