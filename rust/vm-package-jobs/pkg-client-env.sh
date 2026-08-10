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

export NPM_CONFIG_REGISTRY="${authenticated}/npm/"
export PIP_INDEX_URL="${authenticated}/pypi/simple/"
export UV_INDEX_URL="$PIP_INDEX_URL"
export CARGO_REGISTRIES_VM_INDEX="sparse+${gateway}/cargo/index/"
export CARGO_REGISTRIES_VM_TOKEN="$read_token"
export CARGO_SOURCE_CRATES_IO_REPLACE_WITH=vm
export CARGO_SOURCE_VM_REGISTRY="$CARGO_REGISTRIES_VM_INDEX"
export CARGO_REGISTRY_GLOBAL_CREDENTIAL_PROVIDERS=cargo:token
