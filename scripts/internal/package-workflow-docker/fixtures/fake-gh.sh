#!/bin/sh
case "$*" in
  "auth status --hostname github.com") exit 0 ;;
  "auth token --hostname github.com") printf '%s\n' acceptance-token-not-used-for-local-git ;;
  *) exit 2 ;;
esac
