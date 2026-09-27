#!/bin/sh
set -eu
root=${0%/*}
printf '%s\n' "$*" >> "$root/commands.log"
mode=$(cat "$root/mode")
case "$mode:$1" in
  inspect:inspect)
    if [ ! -f "$root/state" ]; then
      echo 'Error: No such object' >&2
      exit 1
    fi
    case "$*" in
      *Config.Image*) echo 'vm-derived:test' ;;
      *package-edge.revision*)
        if [ -f "$root/revision" ]; then
          printf 'running\t%s\n' "$(cat "$root/revision")"
        else
          cat "$root/state"
        fi ;;
      *) cat "$root/state" ;;
    esac ;;
  services:ps)
    if [ -f "$root/inventory-error" ]; then
      cat "$root/inventory-error" >&2
      exit 9
    fi
    case "$*" in
      *--filter*|*-a*) printf 'demo-dev\ndemo-cache\ndemo-db\n' ;;
      *) printf 'demo-cache\n' ;;
    esac ;;
  destroy:ps) printf 'demo-dev\ndemo-postgres\n' ;;
esac
