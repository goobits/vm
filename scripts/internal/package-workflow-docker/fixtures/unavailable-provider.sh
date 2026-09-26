#!/bin/sh
# A Docker acceptance controller must never discover another host provider.
echo 'Provider excluded from isolated Docker package acceptance' >&2
exit 127
