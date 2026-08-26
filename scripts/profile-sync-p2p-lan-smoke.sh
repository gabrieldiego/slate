#!/usr/bin/env sh
set -eu

binary=${SLATE_P2P_LAN_SMOKE_BINARY:-target/debug/slate-broadwebd-net-probe}
ssh_target=${1:-${SLATE_P2P_LAN_SMOKE_SSH:-}}
frame_max_bytes=${SLATE_P2P_LAN_FRAME_MAX_BYTES:-1048576}
remote_memory_mb=${SLATE_P2P_LAN_REMOTE_MEMORY_MB:-256}
local_memory_mb=${SLATE_P2P_LAN_LOCAL_MEMORY_MB:-256}
payload=${SLATE_P2P_LAN_PAYLOAD:-slate broadwebd p2p LAN profile sync smoke}
network_id=${SLATE_P2P_LAN_NETWORK_ID:-slate_p2p_$$}
multicast_group=${SLATE_P2P_LAN_MULTICAST_GROUP:-239.255.85.83}
discovery_port=${SLATE_P2P_LAN_DISCOVERY_PORT:-47883}
discovery_target=${SLATE_P2P_LAN_DISCOVERY_TARGET:-$multicast_group:$discovery_port}

if [ -z "$ssh_target" ]; then
    printf 'usage: %s <ssh-target>\n' "$0" >&2
    printf 'example: SLATE_P2P_LAN_SMOKE_BINARY=target/debug/slate-broadwebd-net-probe %s user@host\n' "$0" >&2
    exit 2
fi

if [ ! -x "$binary" ]; then
    printf 'p2p LAN smoke binary is missing or not executable: %s\n' "$binary" >&2
    printf 'build it first with the low-memory wrapper, for example:\n' >&2
    printf '  SLATE_BUILD_MEMORY_LIMIT_MB=2048 CARGO_BUILD_JOBS=1 CARGO_INCREMENTAL=0 scripts/with-build-limits.sh cargo build -j 1 -p slate-broadwebd --bin slate-broadwebd-net-probe\n' >&2
    exit 2
fi

remote_dir=$(
    ssh "$ssh_target" 'set -eu; base=${TMPDIR:-/tmp}; dir=$(mktemp -d "$base/slate-broadwebd-p2p-lan.XXXXXX"); printf "%s\n" "$dir"'
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

ssh "$ssh_target" "set -eu; ulimit -v $((remote_memory_mb * 1024)) 2>/dev/null || true; '$remote_dir/slate-broadwebd-net-probe' serve --bind 0.0.0.0:0 --state-root '$remote_dir/state' --ready-file '$remote_dir/ready' --max-requests 16 --frame-max-bytes '$frame_max_bytes' --discovery-bind 0.0.0.0:'$discovery_port' --discovery-ready-file '$remote_dir/discovery-ready' --discovery-network '$network_id' --discovery-node remote_probe --discovery-multicast '$multicast_group' > '$remote_dir/server.log' 2>&1 & printf '%s\n' "'$!'" > '$remote_dir/server.pid'"
remote_pid=$(ssh "$ssh_target" "set -eu; cat '$remote_dir/server.pid'")

attempt=0
while [ "$attempt" -lt 50 ]; do
    if ssh "$ssh_target" "set +e; test -s '$remote_dir/ready' && test -s '$remote_dir/discovery-ready'"; then
        break
    fi
    attempt=$((attempt + 1))
    sleep 0.1
done

if ! ssh "$ssh_target" "set +e; test -s '$remote_dir/ready' && test -s '$remote_dir/discovery-ready'"; then
    ssh "$ssh_target" "set +e; cat '$remote_dir/server.log'" >&2 || true
    printf 'remote broadwebd p2p LAN smoke server did not become ready\n' >&2
    exit 1
fi

if ! SLATE_BUILD_MEMORY_LIMIT_MB=$local_memory_mb scripts/with-build-limits.sh \
    "$binary" discover-probe \
    --discovery-target "$discovery_target" \
    --network-id "$network_id" \
    --node-id local_probe \
    --payload "$payload" \
    --frame-max-bytes "$frame_max_bytes"; then
    ssh "$ssh_target" "set +e; cat '$remote_dir/server.log'" >&2 || true
    exit 1
fi

ssh "$ssh_target" "set +e; kill '$remote_pid' 2>/dev/null; rm -rf -- '$remote_dir'"
remote_dir=
remote_pid=
