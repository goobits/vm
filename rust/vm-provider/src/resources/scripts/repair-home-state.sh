#!/usr/bin/env bash

set -euo pipefail

REPAIR_VERSION=3
home_dir="${1:?home directory is required}"
user_name="${2:?user name is required}"

case "$user_name" in
  ''|*[!a-zA-Z0-9_-]*|-*)
    echo "Invalid user name: $user_name" >&2
    exit 2
    ;;
esac
case "$home_dir" in
  /|''|*..*|*[![:print:]]*)
    echo "Unsafe home directory: $home_dir" >&2
    exit 2
    ;;
  /*) ;;
  *)
    echo "Home directory must be absolute: $home_dir" >&2
    exit 2
    ;;
esac
if [ ! -d "$home_dir" ] || [ -L "$home_dir" ]; then
  echo "Home directory must be a real directory: $home_dir" >&2
  exit 2
fi

user_uid="$(id -u "$user_name")"
user_gid="$(id -g "$user_name")"
marker_dir="$home_dir/.vm/state"
marker="$marker_dir/home-repair"
package_checkout_root="$home_dir/.local/share/vm/package-checkouts"

as_root() {
  if [ "$(id -u)" -eq 0 ]; then
    "$@"
  elif "$@" 2>/dev/null; then
    return 0
  elif command -v sudo >/dev/null 2>&1; then
    sudo "$@"
  else
    echo "Home repair requires root or sudo" >&2
    return 1
  fi
}

# Older images launched this on every interactive shell. Remove only the exact
# generated hook; retain any administrator-owned file that merely shares its
# historical name.
legacy_profile=/etc/profile.d/vm-worktree-repair.sh
if [ -f "$legacy_profile" ] && [ ! -L "$legacy_profile" ]; then
  if grep -Fq 'VM Git Worktree Auto-Repair' "$legacy_profile" \
    && grep -Fq 'git worktree repair' "$legacy_profile"; then
    as_root rm -f -- "$legacy_profile"
  else
    echo "Retaining unrecognized legacy profile: $legacy_profile" >&2
  fi
fi

stat_identity() {
  stat -c '%d:%u:%g:%a' "$1" 2>/dev/null || stat -f '%d:%u:%g:%Lp' "$1" 2>/dev/null
}

stat_uid() {
  stat -c '%u' "$1" 2>/dev/null || stat -f '%u' "$1" 2>/dev/null
}

stat_gid() {
  stat -c '%g' "$1" 2>/dev/null || stat -f '%g' "$1" 2>/dev/null
}

managed_paths=(
  "$home_dir/.vm"
  "$home_dir/.nvm"
  "$home_dir/.cargo"
  "$home_dir/.rustup"
  "$home_dir/.npm"
  "$home_dir/.cache"
  "$home_dir/.local"
  "$home_dir/.config"
  "$home_dir/.shell_history"
  "$home_dir/.claude"
  "$home_dir/.gemini"
  "$home_dir/.codex"
)

is_mountpoint() {
  local path="$1"
  if command -v mountpoint >/dev/null 2>&1 && mountpoint -q "$path"; then
    return 0
  fi
  mount 2>/dev/null | grep -F "on $path " >/dev/null 2>&1
}

state_fingerprint() {
  local path identity
  printf 'v%s:%s:%s' "$REPAIR_VERSION" "$user_uid" "$user_gid"
  for path in "${managed_paths[@]}" "$package_checkout_root"; do
    if [ -e "$path" ] && identity="$(stat_identity "$path")"; then
      printf '|%s=%s' "${path#$home_dir/}" "$identity"
    else
      printf '|%s=missing' "${path#$home_dir/}"
    fi
  done
}

home_is_writable() {
  local probe
  if [ "$(id -u)" -eq "$user_uid" ]; then
    probe="$(mktemp "$home_dir/.vm-home-write-test.XXXXXX")" || return 1
    rm -f -- "$probe"
  elif command -v sudo >/dev/null 2>&1; then
    probe="$(sudo -H -u "$user_name" mktemp "$home_dir/.vm-home-write-test.XXXXXX")" || return 1
    sudo -H -u "$user_name" rm -f -- "$probe"
  elif command -v runuser >/dev/null 2>&1; then
    probe="$(runuser -u "$user_name" -- mktemp "$home_dir/.vm-home-write-test.XXXXXX")" || return 1
    runuser -u "$user_name" -- rm -f -- "$probe"
  else
    return 1
  fi
}

current=""
if [ -L "$home_dir/.vm" ] || [ -L "$marker_dir" ] || [ -L "$marker" ] \
  || { [ -e "$marker" ] && [ ! -f "$marker" ]; }; then
  echo "Unsafe home repair marker path: $marker" >&2
  exit 1
fi
if [ -f "$marker" ]; then
  IFS= read -r current <"$marker" || true
fi
expected="$(state_fingerprint)"
generation="v$REPAIR_VERSION:$user_uid:$user_gid"
full_repair="${VM_HOME_REPAIR_FORCE:-0}"
if [ "${VM_HOME_REPAIR_FORCE:-0}" != 1 ] && [ "$current" = "$expected" ] && home_is_writable; then
  exit 0
fi

as_root chown -h "$user_uid:$user_gid" "$home_dir"
as_root chmod u+rwx,go+rx "$home_dir"
as_root mkdir -p \
  "$home_dir/.local/bin" \
  "$home_dir/.shell_history" \
  "$marker_dir"
if [ -L "$marker_dir" ] || [ ! -d "$marker_dir" ]; then
  echo "Unsafe home repair state directory: $marker_dir" >&2
  exit 1
fi
as_root chmod 700 "$marker_dir"
if [ ! -L "$home_dir/.claude" ] && ! is_mountpoint "$home_dir/.claude"; then
  as_root mkdir -p "$home_dir/.claude/projects" "$home_dir/.claude/sessions"
fi
if [ ! -L "$home_dir/.codex" ] && ! is_mountpoint "$home_dir/.codex"; then
  as_root mkdir -p \
    "$home_dir/.codex/bin" \
    "$home_dir/.codex/log" \
    "$home_dir/.codex/sessions" \
    "$home_dir/.codex/rollout"
fi
as_root touch "$home_dir/.shell_history/zsh_history"

if command -v chflags >/dev/null 2>&1; then
  as_root chflags nouchg,noschg "$home_dir/.zshrc" "$home_dir/.bashrc" 2>/dev/null || true
  if [ ! -L "$home_dir/.codex" ] && ! is_mountpoint "$home_dir/.codex"; then
    as_root chflags -R nouchg,noschg "$home_dir/.codex" 2>/dev/null || true
  fi
fi

for path in "${managed_paths[@]}"; do
  [ -d "$path" ] || continue
  [ -L "$path" ] && continue
  is_mountpoint "$path" && continue
  path_uid="$(stat_uid "$path")"
  path_gid="$(stat_gid "$path")"
  # A receipt-version change updates exact managed roots but must not turn into
  # a recursive walk of large, already-owned dependency caches. Recursion is
  # reserved for an incorrectly owned root or an explicit forced repair.
  if [ "$full_repair" = 1 ] || [ "$path_uid" != "$user_uid" ] || [ "$path_gid" != "$user_gid" ]; then
    if command -v mountpoint >/dev/null 2>&1; then
      # Prune nested mounts even when a bind mount shares the parent device;
      # -xdev alone does not protect that case.
      as_root find "$path" -xdev -mindepth 1 \
        \( -type d -exec mountpoint -q {} \; -prune \) -o \
        \( \( ! -uid "$user_uid" -o ! -gid "$user_gid" \) \
        -exec chown -h "$user_uid:$user_gid" {} + \)
    else
      echo "Skipping recursive ownership repair without mountpoint: $path" >&2
    fi
  fi
  as_root chown -h "$user_uid:$user_gid" "$path"
  as_root chmod u+rwx "$path"
done

# The checkout root may be a dedicated Docker volume nested below .local.
# Home repair deliberately prunes nested mounts above, so repair this exact
# VM-owned path separately without traversing any host workspace mount.
if [ -L "$package_checkout_root" ] \
  || { [ -e "$package_checkout_root" ] && [ ! -d "$package_checkout_root" ]; }; then
  echo "Unsafe package checkout root: $package_checkout_root" >&2
  exit 1
fi
if [ -d "$package_checkout_root" ]; then
  checkout_uid="$(stat_uid "$package_checkout_root")"
  checkout_gid="$(stat_gid "$package_checkout_root")"
  if [ "$full_repair" = 1 ] \
    || [ "$checkout_uid" != "$user_uid" ] \
    || [ "$checkout_gid" != "$user_gid" ]; then
    as_root find "$package_checkout_root" -xdev \
      \( ! -uid "$user_uid" -o ! -gid "$user_gid" \) \
      -exec chown -h "$user_uid:$user_gid" {} +
  fi
  as_root chown -h "$user_uid:$user_gid" "$package_checkout_root"
  as_root chmod 700 "$package_checkout_root"
fi

for path in \
  "$home_dir/.zshrc" \
  "$home_dir/.bashrc" \
  "$home_dir/.profile" \
  "$home_dir/.claude.json" \
  "$home_dir/.codex/auth.json" \
  "$home_dir/.codex/config.toml" \
  "$home_dir/.shell_history/zsh_history"; do
  [ -e "$path" ] && [ ! -L "$path" ] || continue
  case "$path" in
    "$home_dir/.codex/"*) is_mountpoint "$home_dir/.codex" && continue ;;
  esac
  as_root chown -h "$user_uid:$user_gid" "$path"
done

for path in "$home_dir/.shell_history" "$home_dir/.claude" "$home_dir/.codex"; do
  [ -L "$path" ] || is_mountpoint "$path" || as_root chmod 700 "$path"
done
if [ ! -L "$home_dir/.claude" ] && ! is_mountpoint "$home_dir/.claude"; then
  as_root chmod 700 "$home_dir/.claude/projects" "$home_dir/.claude/sessions"
fi
if [ ! -L "$home_dir/.codex" ] && ! is_mountpoint "$home_dir/.codex"; then
  as_root chmod 700 \
    "$home_dir/.codex/bin" "$home_dir/.codex/log" \
    "$home_dir/.codex/sessions" "$home_dir/.codex/rollout"
fi
as_root chmod 600 "$home_dir/.shell_history/zsh_history"
for path in "$home_dir/.claude.json" "$home_dir/.codex/auth.json" "$home_dir/.codex/config.toml"; do
  [ -f "$path" ] && [ ! -L "$path" ] || continue
  as_root chmod 600 "$path"
done
for path in "$home_dir/.zshrc" "$home_dir/.bashrc" "$home_dir/.profile"; do
  [ -f "$path" ] && [ ! -L "$path" ] || continue
  as_root chmod u+rw,go+r "$path"
done

quarantine_file() {
  local source="$1" quarantine_dir quarantine
  [ -f "$source" ] && [ ! -L "$source" ] || return 0
  quarantine_dir="$marker_dir/quarantine"
  as_root mkdir -p "$quarantine_dir"
  if [ -L "$quarantine_dir" ] || [ ! -d "$quarantine_dir" ]; then
    echo "Unsafe home repair quarantine: $quarantine_dir" >&2
    return 1
  fi
  as_root chmod 700 "$quarantine_dir"
  quarantine="$(as_root mktemp "$quarantine_dir/$(basename "$source").XXXXXX")"
  as_root mv -f -- "$source" "$quarantine"
  as_root chown -h "$user_uid:$user_gid" "$quarantine"
  as_root chmod 600 "$quarantine"
  echo "Quarantined corrupt state: $source -> $quarantine" >&2
}

# Only these known machine-managed state files are eligible for automatic
# quarantine. Never recursively delete arbitrary JSON from tool directories.
[ -s "$home_dir/.claude.json" ] || quarantine_file "$home_dir/.claude.json"
if [ ! -L "$home_dir/.claude" ] && ! is_mountpoint "$home_dir/.claude"; then
  [ -s "$home_dir/.claude/settings.json" ] || quarantine_file "$home_dir/.claude/settings.json"
fi
if [ ! -L "$home_dir/.gemini" ] && ! is_mountpoint "$home_dir/.gemini"; then
  [ -s "$home_dir/.gemini/settings.json" ] || quarantine_file "$home_dir/.gemini/settings.json"
fi
if [ ! -L "$home_dir/.codex" ] && ! is_mountpoint "$home_dir/.codex"; then
  [ -s "$home_dir/.codex/auth.json" ] || quarantine_file "$home_dir/.codex/auth.json"
fi

codex_auth="$home_dir/.codex/auth.json"
if [ -s "$codex_auth" ] && [ ! -L "$codex_auth" ] && [ ! -L "$home_dir/.codex" ] && ! is_mountpoint "$home_dir/.codex"; then
  if command -v python3 >/dev/null 2>&1; then
    python3 -c 'import json, sys; json.load(open(sys.argv[1], encoding="utf-8"))' "$codex_auth" \
      >/dev/null 2>&1 || quarantine_file "$codex_auth"
  elif command -v plutil >/dev/null 2>&1; then
    plutil -lint "$codex_auth" >/dev/null 2>&1 || quarantine_file "$codex_auth"
  elif command -v node >/dev/null 2>&1; then
    node -e 'JSON.parse(require("fs").readFileSync(process.argv[1], "utf8"))' "$codex_auth" \
      >/dev/null 2>&1 || quarantine_file "$codex_auth"
  fi
fi

if ! home_is_writable; then
  echo "HOME is not writable by $user_name: $home_dir" >&2
  ls -ld "$home_dir" >&2 || true
  exit 1
fi

expected="$(state_fingerprint)"
marker_tmp="$(as_root mktemp "$marker_dir/.home-repair.XXXXXX")"
cleanup_marker_tmp() {
  as_root rm -f -- "$marker_tmp" >/dev/null 2>&1 || true
}
trap cleanup_marker_tmp EXIT
as_root sh -c 'umask 077; printf "%s\n" "$1" > "$2"' sh "$expected" "$marker_tmp"
as_root chown -h "$user_uid:$user_gid" "$marker_tmp"
as_root chmod 600 "$marker_tmp"
as_root mv -f -- "$marker_tmp" "$marker"
trap - EXIT
