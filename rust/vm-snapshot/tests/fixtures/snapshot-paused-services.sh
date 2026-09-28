#!/bin/sh
mode=$(cat "$(dirname "$0")/mode")
if [ "$1" = compose ]; then
    # Model the runtime API: its default ps query excludes paused containers.
    case " $* " in *' --all '*) ;; *) exit 0 ;; esac
    case " $* " in
      *' --services '*) [ "$mode" = empty ] || printf 'dev\n' ;;
      *) [ "$mode" = disappeared ] || printf 'paused-container\n' ;;
    esac
    exit 0
fi
if [ "$1" = commit ]; then
    [ "$2" = paused-container ] || exit 1
    printf 'sha256:aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa'
    exit 0
fi
if [ "$1" = save ]; then
    printf 'captured-rootfs' > "$4"
    exit 0
fi
if [ "$1" = image ] && [ "$2" = rm ]; then
    case "$3" in vm-snapshot/owned/dev:clean-*) exit 0 ;; esac
fi
exit 1
