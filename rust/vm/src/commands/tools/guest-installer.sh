#!/bin/sh
set -eu
umask 077

root="${XDG_DATA_HOME:-$HOME/.local/share}/vm-tools"
releases="$root/releases"
states="$root/state"
temporary="$root/tmp"
mkdir -p "$releases" "$states" "$temporary"

verify_digest() {
  archive=$1
  expected=$2
  if command -v sha256sum >/dev/null 2>&1; then
    printf '%s  %s\n' "$expected" "$archive" | sha256sum -c - >/dev/null
  elif command -v shasum >/dev/null 2>&1; then
    actual=$(shasum -a 256 "$archive" | awk '{print $1}')
    test "$actual" = "$expected"
  else
    echo "No SHA-256 verifier is installed" >&2
    return 1
  fi
}

safe_archive() {
  archive=$1
  listing=$2
  tar -tzf "$archive" > "$listing"
  while IFS= read -r entry; do
    case "$entry" in
      /*|..|../*|*/..|*/../*)
        echo "Unsafe archive entry: $entry" >&2
        return 1
        ;;
    esac
  done < "$listing"
}

resolved_below() {
  root_path=$1
  candidate=$2
  if command -v realpath >/dev/null 2>&1; then
    root_resolved=$(realpath "$root_path") || return 1
    resolved=$(realpath "$candidate") || return 1
  else
    root_resolved=$(readlink -f "$root_path") || return 1
    resolved=$(readlink -f "$candidate") || return 1
  fi
  case "$resolved" in
    "$root_resolved"|"$root_resolved"/*) return 0 ;;
    *) return 1 ;;
  esac
}

managed_link() {
  managed_root=$1
  candidate=$2
  resolved_below "$managed_root" "$candidate" && return 0
  test -L "$candidate" || return 1
  raw_target=$(readlink "$candidate") || return 1
  case "$raw_target" in
    "$managed_root"|"$managed_root"/*) return 0 ;;
    *) return 1 ;;
  esac
}

install_tool() (
  manifest=$1
  task=$(mktemp -d "$temporary/task.XXXXXX")
  prepared="$task/prepared"
  cleanup() {
    if test -f "$prepared"; then
      while IFS="$(printf '\t')" read -r pending _destination; do
        test -z "$pending" || rm -f "$pending"
      done < "$prepared"
    fi
    rm -rf "$task"
  }
  trap cleanup EXIT HUP INT TERM

  manifest_file="$task/manifest"
  links="$task/links"
  new_links="$task/new-links"
  printf '%s\n' "$manifest" > "$manifest_file"
  tab=$(printf '\t')
  IFS="$tab" read -r name version target kind digest url < "$manifest_file"
  test -n "$name" && test -n "$version" && test -n "$target"
  test "$kind" = binary || test "$kind" = collection
  test "${#digest}" -eq 64
  tail -n +2 "$manifest_file" > "$links"

  release="$releases/$name/$version-$digest"
  if test ! -d "$release"; then
    archive="$task/artifact.tar.gz"
    curl --fail --silent --show-error --location \
      --connect-timeout 5 --max-time 600 --retry 2 \
      --header "Authorization: Bearer ${CARGO_REGISTRIES_VM_TOKEN:?package read token unavailable}" \
      --output "$archive" "$url"
    verify_digest "$archive" "$digest"
    safe_archive "$archive" "$task/archive.list"
    extracted="$task/release"
    mkdir -p "$extracted"
    tar -xzf "$archive" -C "$extracted"

    while IFS="$tab" read -r destination source; do
      test -n "$destination" && test -n "$source"
      test -e "$extracted/$source" || {
        echo "$name archive is missing activation source: $source" >&2
        exit 1
      }
      resolved_below "$extracted" "$extracted/$source" || {
        echo "$name activation source escapes its release: $source" >&2
        exit 1
      }
    done < "$links"

    mkdir -p "$(dirname "$release")"
    if ! mv "$extracted" "$release" 2>/dev/null; then
      test -d "$release" || exit 1
    fi
  fi

  : > "$prepared"
  : > "$new_links"
  sequence=0

  prepare_link() {
    link_source=$1
    link_destination=$2
    if test -e "$link_destination" || test -L "$link_destination"; then
      if ! managed_link "$releases/$name" "$link_destination"; then
        echo "Refusing to replace unmanaged path: $link_destination" >&2
        exit 1
      fi
    fi
    link_parent=$(dirname "$link_destination")
    mkdir -p "$link_parent"
    sequence=$((sequence + 1))
    pending="$link_parent/.vm-tool-$name-$$-$sequence"
    rm -f "$pending"
    ln -s "$link_source" "$pending"
    printf '%s\t%s\n' "$pending" "$link_destination" >> "$prepared"
    printf '%s\n' "$link_destination" >> "$new_links"
  }

  while IFS="$tab" read -r destination source; do
    resolved_below "$release" "$release/$source" || {
      echo "$name activation source is unavailable: $source" >&2
      exit 1
    }
    destination_path="$HOME/$destination"
    if test "$kind" = collection && test -d "$destination_path" && test ! -L "$destination_path"; then
      found_skill=false
      for skill_path in "$release/$source"/*; do
        test -d "$skill_path" && test -f "$skill_path/SKILL.md" || continue
        found_skill=true
        prepare_link "$skill_path" "$destination_path/$(basename "$skill_path")"
      done
      if test "$found_skill" = false; then
        echo "$name collection has no skill directories below: $source" >&2
        exit 1
      fi
      continue
    fi
    prepare_link "$release/$source" "$destination_path"
  done < "$links"

  while IFS="$tab" read -r pending destination_path; do
    mv -f "$pending" "$destination_path"
  done < "$prepared"
  : > "$prepared"

  old_links="$states/$name.links"
  if test -f "$old_links"; then
    while IFS= read -r old_destination; do
      test -n "$old_destination" || continue
      if ! grep -Fqx "$old_destination" "$new_links" \
        && test -L "$old_destination" \
        && managed_link "$releases/$name" "$old_destination"; then
        rm -f "$old_destination"
      fi
    done < "$old_links"
  fi

  state_temp="$states/.$name.$$.tmp"
  links_temp="$states/.$name.links.$$.tmp"
  printf '%s\t%s\t%s\t%s\n' "$name" "$version" "$target" "$digest" > "$state_temp"
  cp "$new_links" "$links_temp"
  mv -f "$state_temp" "$states/$name.state"
  mv -f "$links_temp" "$states/$name.links"
  printf '%s %s is active\n' "$name" "$version"
)

pids=""
for manifest in "$@"; do
  install_tool "$manifest" &
  pids="$pids $!"
done

result=0
for pid in $pids; do
  wait "$pid" || result=1
done
exit "$result"
