#!/bin/sh
set -eu
umask 077

root="${XDG_DATA_HOME:-$HOME/.local/share}/vm-tools"
releases="$root/releases"
states="$root/state"
temporary="$root/tmp"
migrations="$root/migrations"
backups="$root/backups"
mkdir -p "$releases" "$states" "$temporary" "$migrations" "$backups"
mode=${1:-}
case "$mode" in
  background-if-idle|background|wait) shift ;;
  *)
    printf "Unknown tool reconciliation mode '%s'\n" "$mode" >&2
    exit 1
    ;;
esac
lock="$root/update.lock"
reaper="$root/update.lock-reaper"
owns_lock=no
owns_reaper=no

cleanup() {
  status=$?
  trap - EXIT HUP INT TERM
  if test "$owns_lock" = yes; then
    owner="$(cat "$lock/pid" 2>/dev/null || true)"
    if test "$owner" = "$$"; then
      rm -rf "$lock"
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

  owner="$(cat "$lock/pid" 2>/dev/null || true)"
  if owner_is_running "$owner"; then
    return 1
  fi
  if ! mkdir "$reaper" 2>/dev/null; then
    return 1
  fi
  owns_reaper=yes

  owner="$(cat "$lock/pid" 2>/dev/null || true)"
  if test -z "$owner"; then
    sleep 1
    owner="$(cat "$lock/pid" 2>/dev/null || true)"
  fi
  if owner_is_running "$owner"; then
    rmdir "$reaper"
    owns_reaper=no
    return 1
  fi

  stale="$root/update.lock-stale.$$"
  if mv "$lock" "$stale" 2>/dev/null; then
    rm -rf "$stale"
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
  if test "$mode" != wait; then
    exit 0
  fi
  attempt=$((attempt + 1))
  if test "$attempt" -ge 900; then
    printf '%s\n' 'Timed out waiting for another tool reconciliation' >&2
    exit 1
  fi
  sleep 1
done

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

sha256_text() {
  value=$1
  if command -v sha256sum >/dev/null 2>&1; then
    printf '%s' "$value" | sha256sum | awk '{print $1}'
  elif command -v shasum >/dev/null 2>&1; then
    printf '%s' "$value" | shasum -a 256 | awk '{print $1}'
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
  root_path=$1
  candidate=$2
  root_resolved=$(canonical_path "$root_path") || return 1
  resolved=$(canonical_path "$candidate") || return 1
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

release_is_writable() {
  candidate=$1
  test -n "$(find "$candidate" \( -type d -o -type f \) -perm -u=w -print -quit)"
}

harden_release() {
  candidate=$1
  find "$candidate" -type f -exec chmod a-w {} +
  find "$candidate" -type d -exec chmod a-w {} +
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
  if test -d "$release" && release_is_writable "$release"; then
    chmod -R u+w "$release"
    rm -rf "$release"
  fi
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
    harden_release "$release"
  fi

  : > "$prepared"
  : > "$new_links"
  sequence=0

  write_migration_receipt() {
    receipt=$1
    receipt_state=$2
    receipt_mode=$3
    receipt_destination=$4
    receipt_backup=$5
    receipt_source=$6
    receipt_temp="$receipt.$$.tmp"
    printf '%s\t%s\t%s\t%s\t%s\n' \
      "$receipt_state" "$receipt_mode" "$receipt_destination" \
      "$receipt_backup" "$receipt_source" > "$receipt_temp"
    mv -f "$receipt_temp" "$receipt"
  }

  prepare_adoption() {
    link_source=$1
    link_destination=$2
    destination_id=$(sha256_text "$link_destination")
    receipt="$migrations/$name-$destination_id-$digest.receipt"
    receipt_mode=
    backup=-

    if test -f "$receipt"; then
      IFS="$tab" read -r receipt_state receipt_mode receipt_destination backup receipt_source < "$receipt"
      test "$receipt_destination" = "$link_destination" \
        && test "$receipt_source" = "$link_source" || {
          echo "Tool migration receipt does not match its destination: $receipt" >&2
          exit 1
        }
      if test "$receipt_state" = complete; then
        if managed_link "$releases/$name" "$link_destination"; then
          printf '%s\n' "$receipt"
          return 0
        fi
        # A tool may replace its managed symlink while self-updating. Preserve
        # that new executable and reopen adoption instead of dead-ending on an
        # otherwise valid completed receipt.
        test -f "$link_destination" && test -x "$link_destination" || {
          echo "Completed tool migration destination is no longer adoptable: $link_destination" >&2
          exit 1
        }
        if cmp -s "$link_destination" "$link_source"; then
          receipt_mode=matched
          backup=-
        else
          receipt_mode=backed_up
          backup_dir="$backups/$name/$destination_id-$(date +%s)-$$"
          backup="$backup_dir/$(basename "$link_destination")"
        fi
        write_migration_receipt \
          "$receipt" pending "$receipt_mode" "$link_destination" "$backup" "$link_source"
        receipt_state=pending
      fi
      test "$receipt_state" = pending || {
        echo "Tool migration receipt is incomplete or stale: $receipt" >&2
        exit 1
      }
    else
      test -f "$link_destination" && test -x "$link_destination" || {
        echo "Refusing to replace unmanaged non-executable path: $link_destination" >&2
        exit 1
      }
      if cmp -s "$link_destination" "$link_source"; then
        receipt_mode=matched
      else
        receipt_mode=backed_up
        backup_dir="$backups/$name/$destination_id-$(date +%s)-$$"
        backup="$backup_dir/$(basename "$link_destination")"
      fi
      write_migration_receipt \
        "$receipt" pending "$receipt_mode" "$link_destination" "$backup" "$link_source"
    fi

    if managed_link "$releases/$name" "$link_destination"; then
      printf '%s\n' "$receipt"
      return 0
    fi
    case "$receipt_mode" in
      matched)
        if test -e "$link_destination" || test -L "$link_destination"; then
          cmp -s "$link_destination" "$link_source" || {
            echo "Unmanaged executable changed during matching adoption: $link_destination" >&2
            exit 1
          }
        fi
        ;;
      backed_up)
        if test -e "$link_destination" || test -L "$link_destination"; then
          test ! -e "$backup" && test ! -L "$backup" || {
            echo "Tool migration backup already exists: $backup" >&2
            exit 1
          }
          mkdir -p "$(dirname "$backup")"
          mv "$link_destination" "$backup"
        elif test ! -e "$backup" && test ! -L "$backup"; then
          echo "Interrupted tool migration lost both source and backup: $link_destination" >&2
          exit 1
        fi
        ;;
      *)
        echo "Tool migration receipt has an invalid mode: $receipt" >&2
        exit 1
        ;;
    esac
    printf '%s\n' "$receipt"
  }

  complete_adoption() {
    receipt=$1
    IFS="$tab" read -r _state receipt_mode receipt_destination backup receipt_source < "$receipt"
    write_migration_receipt \
      "$receipt" complete "$receipt_mode" "$receipt_destination" "$backup" "$receipt_source"
  }

  prepare_link() {
    link_source=$1
    link_destination=$2
    receipt=
    destination_exists=no
    if test -e "$link_destination" || test -L "$link_destination"; then
      destination_exists=yes
    fi
    case "$kind:$link_destination" in
      binary:"$HOME"/.local/bin/*)
        destination_id=$(sha256_text "$link_destination")
        existing_receipt="$migrations/$name-$destination_id-$digest.receipt"
        if test -f "$existing_receipt" \
          || { test "$destination_exists" = yes \
            && ! managed_link "$releases/$name" "$link_destination"; }; then
          test -f "$link_source" && test -x "$link_source" || {
            echo "$name activation source is not executable: $link_source" >&2
            exit 1
          }
          receipt=$(prepare_adoption "$link_source" "$link_destination")
        fi
        ;;
      *)
        if test "$destination_exists" = yes \
          && ! managed_link "$releases/$name" "$link_destination"; then
          echo "Refusing to replace unmanaged path: $link_destination" >&2
          exit 1
        fi
        ;;
    esac
    link_parent=$(dirname "$link_destination")
    mkdir -p "$link_parent"
    sequence=$((sequence + 1))
    pending="$link_parent/.vm-tool-$name-$$-$sequence"
    rm -f "$pending"
    ln -s "$link_source" "$pending"
    printf '%s\t%s\t%s\n' "$pending" "$link_destination" "$receipt" >> "$prepared"
    printf '%s\n' "$link_destination" >> "$new_links"
  }

  replace_link() {
    pending=$1
    destination=$2
    if test -L "$destination"; then
      if mv -fT "$pending" "$destination" 2>/dev/null; then
        return 0
      fi
      mv -fh "$pending" "$destination"
      return
    fi
    mv -f "$pending" "$destination"
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

  while IFS="$tab" read -r pending destination_path receipt; do
    replace_link "$pending" "$destination_path"
    test -z "$receipt" || complete_adoption "$receipt"
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
if test "$result" -eq 0; then
  completed="$(date +%s 2>/dev/null || true)"
  case "$completed" in
    ''|*[!0-9]*) ;;
    *)
      marker="$root/.update.last-success.$$"
      if printf '%s\n' "$completed" > "$marker"; then
        mv -f "$marker" "$root/update.last-success"
      else
        rm -f "$marker"
      fi
      ;;
  esac
fi
exit "$result"
