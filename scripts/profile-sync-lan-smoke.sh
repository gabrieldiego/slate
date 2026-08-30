#!/usr/bin/env sh
set -eu

binary=${SLATE_LAN_SMOKE_BINARY:-target/debug/slate-broadwebd-net-probe}
ssh_target=${1:-${SLATE_LAN_SMOKE_SSH:-}}
frame_max_bytes=${SLATE_LAN_SMOKE_FRAME_MAX_BYTES:-1048576}
remote_memory_mb=${SLATE_LAN_SMOKE_REMOTE_MEMORY_MB:-256}
local_memory_mb=${SLATE_LAN_SMOKE_LOCAL_MEMORY_MB:-256}
payload=${SLATE_LAN_SMOKE_PAYLOAD:-slate broadwebd LAN profile sync smoke}
runtime_profile_sync=${SLATE_LAN_SMOKE_RUNTIME_PROFILE_SYNC:-0}

if [ -z "$ssh_target" ]; then
    printf 'usage: %s <ssh-target>\n' "$0" >&2
    printf 'example: SLATE_LAN_SMOKE_BINARY=target/debug/slate-broadwebd-net-probe %s user@host\n' "$0" >&2
    exit 2
fi

if [ ! -x "$binary" ]; then
    printf 'LAN smoke binary is missing or not executable: %s\n' "$binary" >&2
    printf 'build it first with the low-memory wrapper, for example:\n' >&2
    printf '  SLATE_BUILD_MEMORY_LIMIT_MB=2048 CARGO_BUILD_JOBS=1 CARGO_INCREMENTAL=0 scripts/with-build-limits.sh cargo build -j 1 -p slate-broadwebd --bin slate-broadwebd-net-probe\n' >&2
    exit 2
fi

runtime_profile_sync_arg=
case "$runtime_profile_sync" in
    1 | true | yes)
        runtime_profile_sync_arg="--runtime-profile-sync"
        ;;
    0 | false | no | "")
        ;;
    *)
        printf 'SLATE_LAN_SMOKE_RUNTIME_PROFILE_SYNC must be 1, true, yes, 0, false, no, or empty\n' >&2
        exit 2
        ;;
esac

remote_dir=$(
    ssh "$ssh_target" 'set -eu; base=${TMPDIR:-/tmp}; dir=$(mktemp -d "$base/slate-broadwebd-lan.XXXXXX"); printf "%s\n" "$dir"'
)
remote_pid=

cleanup() {
    if [ -n "${remote_dir:-}" ]; then
        ssh "$ssh_target" "set +e; if [ -n '${remote_pid:-}' ]; then kill '${remote_pid:-}' 2>/dev/null; fi; rm -rf -- '$remote_dir'" >/dev/null 2>&1 || true
    fi
}
trap cleanup EXIT INT TERM

scp -q "$binary" "$ssh_target:$remote_dir/slate-broadwebd-net-probe"
ssh "$ssh_target" "set -eu; chmod 700 '$remote_dir/slate-broadwebd-net-probe'"

ssh "$ssh_target" "set -eu; ulimit -v $((remote_memory_mb * 1024)) 2>/dev/null || true; '$remote_dir/slate-broadwebd-net-probe' serve --bind 0.0.0.0:0 --state-root '$remote_dir/state' --ready-file '$remote_dir/ready' --max-requests 16 --frame-max-bytes '$frame_max_bytes' $runtime_profile_sync_arg > '$remote_dir/server.log' 2>&1 & printf '%s\n' "'$!'" > '$remote_dir/server.pid'"
remote_pid=$(ssh "$ssh_target" "set -eu; cat '$remote_dir/server.pid'")

ready_addr=
attempt=0
while [ "$attempt" -lt 50 ]; do
    ready_addr=$(ssh "$ssh_target" "set +e; test -s '$remote_dir/ready' && cat '$remote_dir/ready'")
    if [ -n "$ready_addr" ]; then
        break
    fi
    attempt=$((attempt + 1))
    sleep 0.1
done

if [ -z "$ready_addr" ]; then
    ssh "$ssh_target" "set +e; cat '$remote_dir/server.log'" >&2 || true
    printf 'remote broadwebd LAN smoke server did not become ready\n' >&2
    exit 1
fi

ssh_host=${ssh_target##*@}
ssh_host=${ssh_host%%:*}
connect_host=${SLATE_LAN_SMOKE_CONNECT_HOST:-$ssh_host}
connect_port=${ready_addr##*:}
connect_addr=$connect_host:$connect_port

SLATE_BUILD_MEMORY_LIMIT_MB=$local_memory_mb scripts/with-build-limits.sh \
    "$binary" probe \
    --connect "$connect_addr" \
    --payload "$payload" \
    --frame-max-bytes "$frame_max_bytes"

ssh "$ssh_target" "set +e; kill '$remote_pid' 2>/dev/null; rm -rf -- '$remote_dir'"
remote_dir=
remote_pid=
