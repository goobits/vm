#!/bin/sh
# Invoked through a per-test symlink, so $0 locates that test's isolated data.
cd "$(dirname "$0")" || exit 1
echo "$@" >> commands.log
failure=$(cat failure)
fallback=sha256:bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb
case "$1 $2" in
    'image rm')
        if [ "$(cat cleanup-failure)" = true ]; then
            echo 'cleanup fixture failure' >&2
            exit 23
        fi
        exit 0 ;;
    'image ls')
        if [ "$(cat partial-commit)" = true ]; then printf '%s' "$fallback"; fi
        exit 0 ;;
esac
if [ "$1" = "$failure" ] || { [ "$2" = inspect ] && [ "$failure" = inspect ]; }; then
    echo "$failure fixture failure" >&2
    exit 23
fi
if [ "$1" = commit ]; then cat output; fi
if [ "$2" = inspect ]; then printf '%s' "$fallback"; fi
