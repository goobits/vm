#!/bin/bash
set -e

# Start supervisor, which will in turn start all configured services.
exec /usr/bin/supervisord -n -c /etc/supervisor/supervisord.conf