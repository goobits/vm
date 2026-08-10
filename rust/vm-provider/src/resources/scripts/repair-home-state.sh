#!/usr/bin/env bash

set -euo pipefail

REPAIR_VERSION=1
home_dir="${1:?home directory is required}"
user_name="${2:?user name is required}"
user_uid="$(id -u "$user_name")"
user_gid="$(id -g "$user_name")"
marker_dir="$home_dir/.vm/state"
marker="$marker_dir/home-repair"

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
  for path in "${managed_paths[@]}"; do
    if [ -e "$path" ] && identity="$(stat_identity "$path")"; then
      printf '|%s=%s' "${path#$home_dir/}" "$identity"
    else
      printf '|%s=missing' "${path#$home_dir/}"
    fi
  done
}

home_is_writable() {
  local probe="$home_dir/.vm-home-write-test"
  if [ "$(id -u)" -eq "$user_uid" ]; then
    touch "$probe" && rm -f "$probe"
  elif command -v sudo >/dev/null 2>&1; then
    sudo -H -u "$user_name" sh -c 'touch "$1" && rm -f "$1"' sh "$probe"
  elif command -v runuser >/dev/null 2>&1; then
    runuser -u "$user_name" -- sh -c 'touch "$1" && rm -f "$1"' sh "$probe"
  else
    return 1
  fi
}

current=""
if [ -f "$marker" ]; then
  IFS= read -r current <"$marker" || true
fi
expected="$(state_fingerprint)"
generation="v$REPAIR_VERSION:$user_uid:$user_gid"
full_repair=0
if [ "${current%%|*}" != "$generation" ] || [ "${VM_HOME_REPAIR_FORCE:-0}" = 1 ]; then
  full_repair=1
fi
if [ "${VM_HOME_REPAIR_FORCE:-0}" != 1 ] && [ "$current" = "$expected" ] && home_is_writable; then
  exit 0
fi

as_root chown -h "$user_uid:$user_gid" "$home_dir"
as_root chmod u+rwx,go+rx "$home_dir"
as_root mkdir -p \
  "$home_dir/.local/bin" \
  "$home_dir/.shell_history" \
  "$marker_dir"
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
  if [ "$full_repair" = 1 ] || [ "$path_uid" != "$user_uid" ] || [ "$path_gid" != "$user_gid" ]; then
    as_root find "$path" -xdev \( ! -uid "$user_uid" -o ! -gid "$user_gid" \) \
      -exec chown -h "$user_uid:$user_gid" {} +
  fi
  as_root chown -h "$user_uid:$user_gid" "$path"
  as_root chmod u+rwx "$path"
done

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

for tool_home in "$home_dir/.claude" "$home_dir/.gemini" "$home_dir/.codex"; do
  [ -d "$tool_home" ] && [ ! -L "$tool_home" ] || continue
  is_mountpoint "$tool_home" && continue
  as_root find "$tool_home" -xdev -type f -name '*.json' -size 0 -delete 2>/dev/null || true
done

codex_auth="$home_dir/.codex/auth.json"
if [ -s "$codex_auth" ] && [ ! -L "$codex_auth" ] && [ ! -L "$home_dir/.codex" ] && ! is_mountpoint "$home_dir/.codex"; then
  if command -v python3 >/dev/null 2>&1; then
    python3 -c 'import json, sys; json.load(open(sys.argv[1], encoding="utf-8"))' "$codex_auth" \
      >/dev/null 2>&1 || as_root rm -f "$codex_auth"
  elif command -v plutil >/dev/null 2>&1; then
    plutil -lint "$codex_auth" >/dev/null 2>&1 || as_root rm -f "$codex_auth"
  elif command -v node >/dev/null 2>&1; then
    node -e 'JSON.parse(require("fs").readFileSync(process.argv[1], "utf8"))' "$codex_auth" \
      >/dev/null 2>&1 || as_root rm -f "$codex_auth"
  fi
fi

if ! home_is_writable; then
  echo "HOME is not writable by $user_name: $home_dir" >&2
  ls -ld "$home_dir" >&2 || true
  exit 1
fi

expected="$(state_fingerprint)"
marker_tmp="$marker.tmp.$$"
as_root sh -c 'umask 077; printf "%s\n" "$1" > "$2"' sh "$expected" "$marker_tmp"
as_root chown -h "$user_uid:$user_gid" "$marker_tmp"
as_root chmod 600 "$marker_tmp"
as_root mv -f "$marker_tmp" "$marker"
