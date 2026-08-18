#!/usr/bin/env sh
set -eu

if [ "$#" -eq 0 ]; then
    printf 'usage: %s <command> [args...]\n' "$0" >&2
    exit 2
fi

memory_mb=${SLATE_BUILD_MEMORY_LIMIT_MB:-6144}

case "$memory_mb" in
    "" | 0 | none | unlimited)
        exec "$@"
        ;;
    *[!0-9]*)
        printf 'SLATE_BUILD_MEMORY_LIMIT_MB must be a positive integer, 0, none, or unlimited\n' >&2
        exit 2
        ;;
esac

memory_kb=$((memory_mb * 1024))

if ! ulimit -v "$memory_kb" 2>/dev/null; then
    printf 'warning: could not apply %s MiB build memory limit on this shell\n' "$memory_mb" >&2
fi

exec "$@"
