#!/usr/bin/env bash
set -euo pipefail
umask 077

: "${HOME:?HOME must be set}"
: "${VM_PROJECT_PATH:?VM_PROJECT_PATH must be set}"

export NVM_DIR="${NVM_DIR:-$HOME/.nvm}"
# shellcheck disable=SC1090
[ -s "$NVM_DIR/nvm.sh" ] && . "$NVM_DIR/nvm.sh"
cd "$VM_PROJECT_PATH"

vm_sha256() {
  if command -v sha256sum >/dev/null 2>&1; then
    sha256sum "$@"
  elif command -v shasum >/dev/null 2>&1; then
    shasum -a 256 "$@"
  else
    echo 'A SHA-256 tool (sha256sum or shasum) is required.' >&2
    return 1
  fi
}

vm_digest() {
  vm_sha256 "$@" | awk '{print $1}'
}

vm_platform_libc() {
  local libc
  libc="$(getconf GNU_LIBC_VERSION 2>/dev/null || true)"
  if [ -z "$libc" ] && command -v ldd >/dev/null 2>&1; then
    libc="$(ldd --version 2>&1 | sed -n '1p')"
  fi
  printf '%s' "${libc:-none}"
}

manager="${VM_NODE_DEPENDENCY_MANAGER:-}"
if [ -n "$manager" ]; then
  case "$manager" in
    pnpm)
      lockfile=pnpm-lock.yaml
      ;;
    npm)
      lockfile=package-lock.json
      ;;
    *)
      echo "Unsupported Node package manager: $manager" >&2
      exit 2
      ;;
  esac

  if [ ! -f package.json ] || [ ! -f "$lockfile" ]; then
    echo "The $manager bootstrap plan no longer matches the project files." >&2
    exit 2
  fi
  if ! command -v "$manager" >/dev/null 2>&1; then
    echo "The planned Node package manager is unavailable: $manager" >&2
    exit 2
  fi

  manager_version="$($manager --version)"
  npmrc_digest=absent
  if [ -f .npmrc ]; then
    npmrc_digest="$(
      awk '
        /^[[:space:]]*([#;]|$)/ { next }
        {
          key = $0
          sub(/=.*/, "", key)
          gsub(/^[[:space:]]+|[[:space:]]+$/, "", key)
          secret_key = tolower(key)
          if (secret_key ~ /(^|:)(_auth|_authtoken|token|password|username|email|certfile|keyfile)$/)
            next
          line = $0
          sub(/\/\/[^\/@[:space:]]+@/, "//<redacted>@", line)
          print line
        }
      ' .npmrc | vm_digest
    )"
  fi

  fingerprint="$(
    {
      printf 'format=1\nmanager=%s@%s\n' "$manager" "$manager_version"
      printf 'node=%s\nos=%s\narch=%s\nlibc=%s\nimage=%s\n' \
        "$(node --version)" "$(uname -s)" "$(uname -m)" "$(vm_platform_libc)" \
        "${VM_IMAGE_IDENTITY:-unknown}"
      for file in package.json "$lockfile" pnpm-workspace.yaml; do
        if [ -f "$file" ]; then
          printf '%s=%s\n' "$file" "$(vm_digest "$file")"
        fi
      done
      printf '.npmrc=%s\n' "$npmrc_digest"
    } | vm_digest
  )"

  stamp="$PWD/node_modules/.vm-dependencies.sha256"
  installed_entry=""
  if [ -d node_modules ]; then
    installed_entry="$(
      find node_modules -mindepth 1 -maxdepth 1 \
        ! -name '.vm-dependencies.sha256' -print -quit
    )"
  fi

  if [ -z "$installed_entry" ] || [ ! -f "$stamp" ] \
    || [ "$(cat "$stamp")" != "$fingerprint" ]; then
    export PLAYWRIGHT_SKIP_BROWSER_DOWNLOAD=1
    if [ "$manager" = pnpm ]; then
      if ! pnpm install --frozen-lockfile; then
        echo 'Dependency bootstrap was deferred; resolve the package or lockfile error inside the environment.' >&2
        echo 'VM_BOOTSTRAP_DEPENDENCIES_DEFERRED=1'
        exit 0
      fi
    else
      if ! npm ci; then
        echo 'Dependency bootstrap was deferred; resolve the package or lockfile error inside the environment.' >&2
        echo 'VM_BOOTSTRAP_DEPENDENCIES_DEFERRED=1'
        exit 0
      fi
    fi
    mkdir -p node_modules
    stamp_tmp="${stamp}.tmp.$$"
    printf '%s\n' "$fingerprint" > "$stamp_tmp"
    mv -f "$stamp_tmp" "$stamp"
    echo 'VM_BOOTSTRAP_DEPENDENCIES_CHANGED=1'
  else
    echo 'VM_BOOTSTRAP_DEPENDENCIES_CURRENT=1'
  fi
fi

if [ -n "${VM_PLAYWRIGHT_BROWSERS:-}" ]; then
  playwright_path="${PLAYWRIGHT_BROWSERS_PATH:-$HOME/.cache/ms-playwright}"
  mkdir -p "$playwright_path"
  browsers_file="$(mktemp)"
  metadata="$(mktemp)"
  cleanup() {
    rm -f -- "$browsers_file" "$metadata"
  }
  trap cleanup EXIT

  printf '%s\n' $VM_PLAYWRIGHT_BROWSERS \
    | sed '/^$/d' \
    | LC_ALL=C sort -u > "$browsers_file"

  while IFS= read -r cli; do
    descriptor="$(dirname "$cli")/browsers.json"
    if [ ! -f "$descriptor" ]; then
      continue
    fi
    descriptor_hash="$(vm_digest "$descriptor")"
    version="$(node "$cli" --version)"
    printf '%s|%s|%s\n' "$descriptor_hash" "$version" "$cli" >> "$metadata"
  done < <(find node_modules -type f -path '*/playwright-core/cli.js' -print | LC_ALL=C sort)

  LC_ALL=C sort -u -o "$metadata" "$metadata"
  if [ ! -s "$metadata" ]; then
    echo 'Playwright browsers were requested, but no browser descriptors were found.' >&2
    exit 1
  fi

  dependency_fingerprint="$(cat node_modules/.vm-dependencies.sha256 2>/dev/null || true)"
  browser_names="$(tr '\n' ' ' < "$browsers_file" | sed 's/[[:space:]]*$//')"
  fingerprint="$(
    {
      printf 'format=1\ndependencies=%s\nos=%s\narch=%s\nlibc=%s\nimage=%s\n' \
        "$dependency_fingerprint" "$(uname -s)" "$(uname -m)" \
        "$(vm_platform_libc)" "${VM_IMAGE_IDENTITY:-unknown}"
      printf 'browsers=%s\n' "$browser_names"
      cut -d '|' -f 1,2 "$metadata"
    } | vm_digest
  )"

  stamp="$playwright_path/.vm-browsers.sha256"
  browser_entry="$(
    find "$playwright_path" -mindepth 1 -maxdepth 1 \
      ! -name '.vm-browsers.sha256' -print -quit
  )"
  if [ -z "$browser_entry" ] || [ ! -f "$stamp" ] \
    || [ "$(cat "$stamp")" != "$fingerprint" ]; then
    browsers=()
    while IFS= read -r browser; do
      browsers[${#browsers[@]}]="$browser"
    done < "$browsers_file"

    previous_descriptor=""
    while IFS='|' read -r descriptor_hash _version cli; do
      if [ "$descriptor_hash" = "$previous_descriptor" ]; then
        continue
      fi
      PLAYWRIGHT_SKIP_BROWSER_GC=1 node "$cli" install "${browsers[@]}"
      previous_descriptor="$descriptor_hash"
    done < "$metadata"
    stamp_tmp="${stamp}.tmp.$$"
    printf '%s\n' "$fingerprint" > "$stamp_tmp"
    mv -f "$stamp_tmp" "$stamp"
    echo 'VM_BOOTSTRAP_BROWSERS_CHANGED=1'
  else
    echo 'VM_BOOTSTRAP_BROWSERS_CURRENT=1'
  fi
fi
