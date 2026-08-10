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
  resolved=$(readlink -f "$candidate") || return 1
  case "$resolved" in
    "$root_path"|"$root_path"/*) return 0 ;;
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
  printf '%s\n' "$manifest" > "$manifest_file"
  tab=$(printf '\t')
  IFS="$tab" read -r name version target digest url < "$manifest_file"
  test -n "$name" && test -n "$version" && test -n "$target"
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
  sequence=0
  while IFS="$tab" read -r destination source; do
    resolved_below "$release" "$release/$source" || {
      echo "$name activation source is unavailable: $source" >&2
      exit 1
    }
    destination_path="$HOME/$destination"
    if test -e "$destination_path" && test ! -L "$destination_path"; then
      echo "Refusing to replace unmanaged path: $destination_path" >&2
      exit 1
    fi
    parent=$(dirname "$destination_path")
    mkdir -p "$parent"
    sequence=$((sequence + 1))
    pending="$parent/.vm-tool-$name-$$-$sequence"
    rm -f "$pending"
    ln -s "$release/$source" "$pending"
    printf '%s\t%s\n' "$pending" "$destination_path" >> "$prepared"
  done < "$links"

  while IFS="$tab" read -r pending destination_path; do
    mv -f "$pending" "$destination_path"
  done < "$prepared"
  : > "$prepared"

  state_temp="$states/.$name.$$.tmp"
  printf '%s\t%s\t%s\t%s\n' "$name" "$version" "$target" "$digest" > "$state_temp"
  mv -f "$state_temp" "$states/$name.state"
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
