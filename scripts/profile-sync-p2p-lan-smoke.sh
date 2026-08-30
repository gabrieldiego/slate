#!/usr/bin/env sh
set -eu

binary=${SLATE_P2P_LAN_SMOKE_BINARY:-target/debug/slate-broadwebd-net-probe}
advertisement_binary=${SLATE_P2P_LAN_ADVERTISEMENT_BINARY:-target/debug/slate-profile-sync-advertisement}
discovery_check_binary=${SLATE_P2P_LAN_DISCOVERY_CHECK_BINARY:-target/debug/slate-profile-sync-discovery-check}
ssh_target=${1:-${SLATE_P2P_LAN_SMOKE_SSH:-}}
frame_max_bytes=${SLATE_P2P_LAN_FRAME_MAX_BYTES:-1048576}
remote_memory_mb=${SLATE_P2P_LAN_REMOTE_MEMORY_MB:-256}
local_memory_mb=${SLATE_P2P_LAN_LOCAL_MEMORY_MB:-256}
payload=${SLATE_P2P_LAN_PAYLOAD:-slate broadwebd p2p LAN profile sync smoke}
network_id=${SLATE_P2P_LAN_NETWORK_ID:-slate_p2p_$$}
server_bind=${SLATE_P2P_LAN_SERVER_BIND:-0.0.0.0:0}
discovery_service_addr=${SLATE_P2P_LAN_DISCOVERY_SERVICE_ADDR:-}
multicast_group=${SLATE_P2P_LAN_MULTICAST_GROUP:-239.255.85.83}
discovery_port=${SLATE_P2P_LAN_DISCOVERY_PORT:-47883}
discovery_target=${SLATE_P2P_LAN_DISCOVERY_TARGET:-$multicast_group:$discovery_port}
runtime_profile_sync=${SLATE_P2P_LAN_RUNTIME_PROFILE_SYNC:-0}
discovery_advertisement_file=${SLATE_P2P_LAN_DISCOVERY_ADVERTISEMENT_FILE:-}
discovery_key_file=${SLATE_P2P_LAN_DISCOVERY_KEY_FILE:-}
discovery_profile=${SLATE_P2P_LAN_DISCOVERY_PROFILE:-}
discovery_node_id=${SLATE_P2P_LAN_DISCOVERY_NODE_ID:-remote_probe}
discovery_provider_id=${SLATE_P2P_LAN_DISCOVERY_PROVIDER_ID:-remote_probe_provider}
discovery_membership_epoch=${SLATE_P2P_LAN_DISCOVERY_MEMBERSHIP_EPOCH:-1}
discovery_sequence=${SLATE_P2P_LAN_DISCOVERY_SEQUENCE:-$(date +%s)}
discovery_check_db=${SLATE_P2P_LAN_DISCOVERY_CHECK_DB:-}
discovery_check_profile=${SLATE_P2P_LAN_DISCOVERY_CHECK_PROFILE:-}
discovery_check_local_device_id=${SLATE_P2P_LAN_DISCOVERY_CHECK_LOCAL_DEVICE_ID:-}
discovery_check_protocol=${SLATE_P2P_LAN_DISCOVERY_CHECK_PROTOCOL:-local-simulation}
discovery_check_report=${SLATE_P2P_LAN_DISCOVERY_CHECK_REPORT:-}
require_signed_discovery=${SLATE_P2P_LAN_REQUIRE_SIGNED_DISCOVERY:-0}
local_tmp_dir=${SLATE_P2P_LAN_LOCAL_TMPDIR:-target/tmp}
remote_dir=
remote_pid=
generated_discovery_advertisement_file=

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

cleanup() {
    if [ -n "${generated_discovery_advertisement_file:-}" ]; then
        rm -f -- "$generated_discovery_advertisement_file" >/dev/null 2>&1 || true
    fi
    if [ -n "${remote_dir:-}" ]; then
        ssh "$ssh_target" "set +e; if [ -n '${remote_pid:-}' ]; then kill '${remote_pid:-}' 2>/dev/null; fi; rm -rf -- '$remote_dir'" >/dev/null 2>&1 || true
    fi
}
trap cleanup EXIT INT TERM

runtime_profile_sync_arg=
case "$runtime_profile_sync" in
    1 | true | yes)
        runtime_profile_sync_arg="--runtime-profile-sync"
        ;;
    0 | false | no | "")
        ;;
    *)
        printf 'SLATE_P2P_LAN_RUNTIME_PROFILE_SYNC must be 1, true, yes, 0, false, no, or empty\n' >&2
        exit 2
        ;;
esac

require_signed_discovery_arg=
case "$require_signed_discovery" in
    1 | true | yes)
        require_signed_discovery_arg="--require-signed-discovery"
        ;;
    0 | false | no | "")
        ;;
    *)
        printf 'SLATE_P2P_LAN_REQUIRE_SIGNED_DISCOVERY must be 1, true, yes, 0, false, no, or empty\n' >&2
        exit 2
        ;;
esac

if [ -n "$discovery_advertisement_file" ] && [ ! -f "$discovery_advertisement_file" ]; then
    printf 'SLATE_P2P_LAN_DISCOVERY_ADVERTISEMENT_FILE does not exist: %s\n' "$discovery_advertisement_file" >&2
    exit 2
fi

if [ -n "$discovery_advertisement_file" ] && [ -n "$discovery_key_file" ]; then
    printf 'set either SLATE_P2P_LAN_DISCOVERY_ADVERTISEMENT_FILE or SLATE_P2P_LAN_DISCOVERY_KEY_FILE, not both\n' >&2
    exit 2
fi

if [ -n "$discovery_key_file" ]; then
    if [ ! -x "$advertisement_binary" ]; then
        printf 'profile-sync advertisement binary is missing or not executable: %s\n' "$advertisement_binary" >&2
        printf 'build it first with the low-memory wrapper, for example:\n' >&2
        printf '  make profile-sync-advertisement-tool\n' >&2
        exit 2
    fi
    if [ ! -f "$discovery_key_file" ]; then
        printf 'SLATE_P2P_LAN_DISCOVERY_KEY_FILE does not exist: %s\n' "$discovery_key_file" >&2
        exit 2
    fi
    if [ -z "$discovery_service_addr" ]; then
        printf 'SLATE_P2P_LAN_DISCOVERY_SERVICE_ADDR is required when generating a signed discovery advertisement from an enrollment key\n' >&2
        exit 2
    fi
    mkdir -p "$local_tmp_dir"
    generated_discovery_advertisement_file=$(mktemp "$local_tmp_dir/slate-profile-sync-advertisement.XXXXXX.json")
    if [ -n "$discovery_profile" ]; then
        SLATE_BUILD_MEMORY_LIMIT_MB=$local_memory_mb scripts/with-build-limits.sh \
            "$advertisement_binary" \
            --key-file "$discovery_key_file" \
            --profile "$discovery_profile" \
            --network-id "$network_id" \
            --device-id "$discovery_node_id" \
            --provider-id "$discovery_provider_id" \
            --service-addr "$discovery_service_addr" \
            --membership-epoch "$discovery_membership_epoch" \
            --sequence "$discovery_sequence" \
            --output "$generated_discovery_advertisement_file"
    else
        SLATE_BUILD_MEMORY_LIMIT_MB=$local_memory_mb scripts/with-build-limits.sh \
            "$advertisement_binary" \
            --key-file "$discovery_key_file" \
            --network-id "$network_id" \
            --device-id "$discovery_node_id" \
            --provider-id "$discovery_provider_id" \
            --service-addr "$discovery_service_addr" \
            --membership-epoch "$discovery_membership_epoch" \
            --sequence "$discovery_sequence" \
            --output "$generated_discovery_advertisement_file"
    fi
    discovery_advertisement_file=$generated_discovery_advertisement_file
fi

if [ -n "$discovery_check_db" ]; then
    if [ ! -x "$discovery_check_binary" ]; then
        printf 'profile-sync discovery check binary is missing or not executable: %s\n' "$discovery_check_binary" >&2
        printf 'build it first with the low-memory wrapper, for example:\n' >&2
        printf '  make profile-sync-discovery-check-tool\n' >&2
        exit 2
    fi
    if [ ! -f "$discovery_check_db" ]; then
        printf 'SLATE_P2P_LAN_DISCOVERY_CHECK_DB does not exist: %s\n' "$discovery_check_db" >&2
        exit 2
    fi
    if [ -z "$discovery_advertisement_file" ]; then
        printf 'SLATE_P2P_LAN_DISCOVERY_CHECK_DB requires SLATE_P2P_LAN_DISCOVERY_ADVERTISEMENT_FILE or SLATE_P2P_LAN_DISCOVERY_KEY_FILE\n' >&2
        exit 2
    fi
    set -- \
        "$discovery_check_binary" \
        --settings-db "$discovery_check_db" \
        --network-id "$network_id" \
        --protocol "$discovery_check_protocol" \
        --advertisement-file "$discovery_advertisement_file" \
        --require-trusted
    if [ -n "$discovery_check_profile" ]; then
        set -- "$@" --profile "$discovery_check_profile"
    fi
    if [ -n "$discovery_check_local_device_id" ]; then
        set -- "$@" --local-device-id "$discovery_check_local_device_id"
    fi
    if [ -n "$discovery_check_report" ]; then
        set -- "$@" --output "$discovery_check_report"
    fi
    SLATE_BUILD_MEMORY_LIMIT_MB=$local_memory_mb scripts/with-build-limits.sh "$@"
fi

remote_dir=$(
    ssh "$ssh_target" 'set -eu; base=${TMPDIR:-/tmp}; dir=$(mktemp -d "$base/slate-broadwebd-p2p-lan.XXXXXX"); printf "%s\n" "$dir"'
)

scp -q "$binary" "$ssh_target:$remote_dir/slate-broadwebd-net-probe"
ssh "$ssh_target" "set -eu; chmod 700 '$remote_dir/slate-broadwebd-net-probe'"

discovery_advertisement_arg=
if [ -n "$discovery_advertisement_file" ]; then
    scp -q "$discovery_advertisement_file" "$ssh_target:$remote_dir/discovery-advertisement.json"
    discovery_advertisement_arg="--discovery-advertisement-file '$remote_dir/discovery-advertisement.json'"
fi

ssh "$ssh_target" "set -eu; ulimit -v $((remote_memory_mb * 1024)) 2>/dev/null || true; '$remote_dir/slate-broadwebd-net-probe' serve --bind '$server_bind' --state-root '$remote_dir/state' --ready-file '$remote_dir/ready' --max-requests 16 --frame-max-bytes '$frame_max_bytes' $runtime_profile_sync_arg --discovery-bind 0.0.0.0:'$discovery_port' --discovery-ready-file '$remote_dir/discovery-ready' --discovery-network '$network_id' --discovery-node remote_probe --discovery-membership-epoch '$discovery_membership_epoch' $discovery_advertisement_arg --discovery-multicast '$multicast_group' > '$remote_dir/server.log' 2>&1 & printf '%s\n' "'$!'" > '$remote_dir/server.pid'"
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
    --frame-max-bytes "$frame_max_bytes" \
    $require_signed_discovery_arg; then
    ssh "$ssh_target" "set +e; cat '$remote_dir/server.log'" >&2 || true
    exit 1
fi

ssh "$ssh_target" "set +e; kill '$remote_pid' 2>/dev/null; rm -rf -- '$remote_dir'"
remote_dir=
remote_pid=
