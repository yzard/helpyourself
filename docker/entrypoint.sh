#!/bin/sh
set -eu
for identifier in "$PUID" "$PGID"; do
    case "$identifier" in ''|*[!0-9]*) echo 'PUID and PGID must be numeric' >&2; exit 1;; esac
done
if [ "$PUID" -eq 0 ] || [ "$PGID" -eq 0 ]; then
    echo 'Use non-root PUID and PGID' >&2
    exit 1
fi
case "$UMASK" in [0-7][0-7][0-7]|[0-7][0-7][0-7][0-7]) ;; *) echo 'Invalid UMASK' >&2; exit 1;; esac
umask "$UMASK"
if [ "$(id -u)" -eq 0 ]; then
    if [ "$1" = "/app/helpyourself" ]; then
        mkdir -p /data
        chown -R "$PUID:$PGID" /data
    fi
    exec gosu "$PUID:$PGID" "$@"
fi
exec "$@"
