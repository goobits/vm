#!/bin/sh

read_token="$(cat "${PKG_CLIENT_READ_TOKEN_FILE:?required}")"
gateway="${PKG_CLIENT_GATEWAY:-http://gateway:8080}"
gateway="${gateway%/}"

test -n "$read_token" || {
  echo "package read token is empty" >&2
  return 2 2>/dev/null || exit 2
}
case "$gateway" in
  http://*|https://*) ;;
  *)
    echo "package client gateway must use HTTP(S)" >&2
    return 2 2>/dev/null || exit 2
    ;;
esac

scheme="${gateway%%://*}"
authority="${gateway#*://}"
authenticated="${scheme}://reader:${read_token}@${authority}"

# npm follows tarball URLs supplied by registry metadata. URL credentials do not
# follow those clean URLs, so scope the read token to the registry path instead.
NPM_CONFIG_USERCONFIG=$(mktemp /tmp/vm-package-npmrc.XXXXXX)
chmod 600 "$NPM_CONFIG_USERCONFIG"
printf 'registry=%s/npm/\n//%s/npm/:_authToken=%s\n' \
  "$gateway" "$authority" "$read_token" > "$NPM_CONFIG_USERCONFIG"
export NPM_CONFIG_USERCONFIG
export NPM_CONFIG_REGISTRY="${gateway}/npm/"
export PIP_INDEX_URL="${authenticated}/pypi/simple/"
export UV_INDEX_URL="$PIP_INDEX_URL"
export CARGO_REGISTRIES_VM_INDEX="sparse+${gateway}/cargo/index/"
export CARGO_REGISTRIES_VM_TOKEN="$read_token"
export CARGO_SOURCE_CRATES_IO_REPLACE_WITH=vm
export CARGO_SOURCE_VM_REGISTRY="$CARGO_REGISTRIES_VM_INDEX"
export CARGO_REGISTRY_GLOBAL_CREDENTIAL_PROVIDERS=cargo:token
