#!/bin/sh
set -eu
case "$1" in
  list) printf '[{"Name":"demo-dev","State":"stopped","Source":"local"}]' ;;
  import) cp "$2" "$TART_HOME/vms/$3" ;;
  rename)
    case "$2" in
      vm-restore-*)
        if [ -f "$TART_HOME/fail-install" ]; then exit 19; fi ;;
    esac
    mv "$TART_HOME/vms/$2" "$TART_HOME/vms/$3" ;;
  delete) rm "$TART_HOME/vms/$2" ;;
  *) exit 20 ;;
esac
