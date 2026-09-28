#!/bin/sh
root=$(dirname "$0")
case "$1" in
  compose) printf 'first\nsecond\nstopped\nprior\nprior-podman\n' ;;
  inspect)
    case "$(cat "$root/$4")" in
      running) printf '{"Running":true,"Paused":false}' ;;
      paused) printf '{"Running":false,"Paused":true}' ;;
      paused-running) printf '{"Running":true,"Paused":true}' ;;
      stopped) printf '{"Running":false,"Paused":false}' ;;
      *) exit 1 ;;
    esac ;;
  pause)
    printf paused > "$root/$2"
    if [ -f "$root/fail-pause" ] && [ "$2" = second ]; then echo 'partial pause failed' >&2; exit 1; fi ;;
  unpause)
    if [ "$2" = first ]; then
      if [ -f "$root/fail-unpause" ]; then echo 'unpause failed' >&2; exit 1; fi
      if [ -f "$root/noop-unpause" ]; then exit 0; fi
    fi
    printf running > "$root/$2" ;;
  *) exit 1 ;;
esac
