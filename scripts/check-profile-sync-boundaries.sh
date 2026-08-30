#!/usr/bin/env sh
set -eu

script_dir=$(CDPATH= cd -- "$(dirname -- "$0")" && pwd)
repo_root=$(dirname -- "$script_dir")

cargo_bin=${CARGO_BIN:-cargo}
jobs=${CARGO_BUILD_JOBS:-1}
threads=${SLATE_TEST_THREADS:-1}
memory_mb=${SLATE_BUILD_MEMORY_LIMIT_MB:-2048}
check_chrome=${SLATE_PROFILE_SYNC_CHECK_CHROME:-0}

export RUSTUP_HOME=${RUSTUP_HOME:-"$repo_root/.rustup"}
export CARGO_HOME=${CARGO_HOME:-"$repo_root/.cargo"}
export TMPDIR=${TMPDIR:-"$repo_root/target/tmp"}
export XDG_CACHE_HOME=${XDG_CACHE_HOME:-"$repo_root/target/cache/xdg"}
export UV_CACHE_DIR=${UV_CACHE_DIR:-"$repo_root/target/cache/uv"}
export CARGO_INCREMENTAL=${CARGO_INCREMENTAL:-0}

mkdir -p "$TMPDIR" "$XDG_CACHE_HOME" "$UV_CACHE_DIR"

run_test() {
    package=$1
    filter=$2
    printf '\n==> %s :: %s\n' "$package" "$filter"
    SLATE_BUILD_MEMORY_LIMIT_MB="$memory_mb" \
        "$repo_root/scripts/with-build-limits.sh" \
        "$cargo_bin" test -p "$package" "$filter" -j "$jobs" -- --test-threads="$threads"
}

run_test_with_features() {
    package=$1
    features=$2
    filter=$3
    printf '\n==> %s [%s] :: %s\n' "$package" "$features" "$filter"
    SLATE_BUILD_MEMORY_LIMIT_MB="$memory_mb" \
        "$repo_root/scripts/with-build-limits.sh" \
        "$cargo_bin" test -p "$package" --features "$features" "$filter" -j "$jobs" -- --test-threads="$threads"
}

reject_protocol_model_leak() {
    file=$1
    pattern=$2
    message=$3
    if rg -n "$pattern" "$file"; then
        printf '\nprofile-sync boundary violation: %s\n' "$message" >&2
        exit 1
    fi
}

require_text() {
    file=$1
    pattern=$2
    message=$3
    if ! rg -q "$pattern" "$file"; then
        printf '\nprofile-sync boundary violation: %s\n' "$message" >&2
        printf 'missing pattern %s in %s\n' "$pattern" "$file" >&2
        exit 1
    fi
}

reject_cargo_dependency_feature_in_section() {
    file=$1
    section=$2
    dependency=$3
    feature=$4
    message=$5
    if awk -v section="$section" -v dependency="$dependency" -v feature="$feature" '
        $0 == section {
            in_section = 1
            next
        }
        in_section && /^\[/ {
            in_section = 0
        }
        in_section && index($0, dependency) && index($0, feature) {
            printf "%s:%d:%s\n", FILENAME, FNR, $0
            found = 1
        }
        END {
            exit found ? 0 : 1
        }
    ' "$file"; then
        printf '\nprofile-sync boundary violation: %s\n' "$message" >&2
        exit 1
    fi
}

cd "$repo_root"

reject_cargo_dependency_feature_in_section \
    crates/profile-sync/Cargo.toml \
    '[dependencies]' \
    'slate-broadwebd' \
    'test-fixtures' \
    'slate-profile-sync must not enable broadwebd test fixtures for normal consumers; gate local preview helpers behind a feature.'

require_text \
    crates/profile-sync/Cargo.toml \
    '^local-preview-fixtures = \["slate-broadwebd/test-fixtures"\]' \
    'slate-profile-sync must expose an explicit fixture feature for local preview helpers.'

require_text \
    crates/chrome/Cargo.toml \
    'slate-profile-sync = \{ workspace = true, features = \["local-preview-fixtures"\] \}' \
    'Slate chrome must opt into profile-sync local preview fixtures explicitly.'

reject_protocol_model_leak \
    crates/broadwebd/src/protocols/ipfs/kubo.rs \
    'InProcessBroadwebNetwork|InternalKubo(ProfileSyncModel|RpcFixture)|fetch_internal_kubo|internal_kubo_rpc_fixtures|register_internal_kubo|take_internal_kubo' \
    'Kubo protocol code must not call fixture models directly; route through transport executors or shims.'

reject_protocol_model_leak \
    crates/broadwebd/src/protocols/ipfs/kubo.rs \
    'kubo_fixtures|InternalKuboRpcTransportShim|is_internal_kubo_rpc_fixture_url|slate-fixture-kubo|in-process-fixture|socketless-fixture' \
    'Kubo protocol transport must stay fixture-blind; install fixture transports from the in-process network layer.'

reject_protocol_model_leak \
    crates/broadwebd/src/protocols/ipfs/mod.rs \
    'InternalKuboRpc|register_internal_kubo|take_internal_kubo|internal_kubo_rpc_url' \
    'The IPFS protocol module must not re-export Kubo fixture shims; expose socket substitutes only through test_fixtures.'

reject_protocol_model_leak \
    crates/broadwebd/src/protocols/ipfs/config.rs \
    'is_internal_fixture_http_url|internal_fixture_http|slate-fixture-http|in-process-fixture|socketless-fixture|InProcessBroadwebNetwork|InternalFixture|ProfileSyncModel|Simulation|simulated' \
    'IPFS gateway config must stay fixture-blind; in-process tests may inject prevalidated endpoints from the fixture layer.'

reject_protocol_model_leak \
    crates/broadwebd/src/protocols/ipfs/gateway.rs \
    'is_internal_fixture_http_url|internal_fixture_http|slate-fixture-http|in-process-fixture|socketless-fixture|InProcessBroadwebNetwork|InternalFixture|ProfileSyncModel|Simulation|simulated' \
    'IPFS gateway transport must keep real gateway semantics; fixture behavior belongs in the injected HTTP transport.'

reject_protocol_model_leak \
    crates/broadwebd/src/protocols/ipfs/gateway.rs \
    'fetch_http_url\(' \
    'IPFS gateway transport must not call the fixture-aware shared HTTP helper; use an executor so fixtures only swap socket IO.'

reject_protocol_model_leak \
    crates/broadwebd/src/http.rs \
    'return fetch_internal_fixture_http_url' \
    'Default HTTP fetching must not branch into fixture storage; in-process transports must call fixture helpers explicitly.'
reject_protocol_model_leak \
    crates/broadwebd/src/http.rs \
    'fn fetch_http_url\(' \
    'The ambiguous shared HTTP helper name must stay removed; use fetch_http_url_over_network or explicit fixture helpers.'
reject_protocol_model_leak \
    crates/broadwebd/src/transports/direct_http.rs \
    'is_internal_fixture_http_url|internal_fixture_http|slate-fixture-http|InProcessBroadwebNetwork|socketless-fixture|in-process-fixture' \
    'Direct HTTP transport must stay fixture-blind; fixture URLs require the in-process fixture transport.'
reject_protocol_model_leak \
    crates/broadwebd/src/registry.rs \
    'is_internal_fixture_http_url|internal_fixture_http|slate-fixture-http|InProcessBroadwebNetwork|socketless-fixture|in-process-fixture' \
    'Default plugin registry must stay fixture-blind; in-process fixtures install their own protocol service.'

reject_protocol_model_leak \
    crates/broadwebd/src/services/profile_sync.rs \
    'InProcessBroadwebNetwork|InternalKubo(ProfileSyncModel|RpcFixture)|fetch_internal_kubo|internal_kubo_rpc_fixtures|register_internal_kubo|take_internal_kubo' \
    'broadwebd profile-sync service code must not call Kubo fixture models directly; route through transport executors or shims.'
reject_protocol_model_leak \
    crates/broadwebd/src/services/profile_sync.rs \
    'InternalKuboRpcTransportShim|KuboProfileSyncTransport|kubo_fixture|from_prevalidated_api_base_url|ipfs-kubo-fixture|socketless-fixture' \
    'broadwebd profile-sync service must stay fixture-blind; in-process Kubo fixtures inject executor factories from the fixture layer.'
reject_protocol_model_leak \
    crates/broadwebd/src/services/profile_sync.rs \
    'profile-sync/fake|local-fake|local in-memory fake|local test backend' \
    'runtime-visible local profile-sync service must be named as a local preview backend, not a fake/test fixture.'
reject_protocol_model_leak \
    crates/broadwebd/src/lib.rs \
    'pub use services::profile_sync::LocalProfileSyncFixture|profile_sync::\{LocalProfileSyncFixture|LocalProfileSyncFixture, ProfileSyncRuntime' \
    'LocalProfileSyncFixture must not be exported from broadwebd root API; use test_fixtures instead.'
require_text \
    crates/broadwebd/src/daemon.rs \
    'pub trait BroadwebdClient' \
    'broadwebd must keep an IPC-neutral client trait around the in-process daemon boundary.'
require_text \
    crates/broadwebd/src/daemon.rs \
    'impl BroadwebdClient for BroadwebDaemon' \
    'BroadwebDaemon must implement the IPC-neutral broadwebd client boundary.'
require_text \
    crates/broadwebd/src/daemon.rs \
    'dispatch_service_request' \
    'BroadwebdClient must expose one request-envelope dispatch path for future IPC clients.'
require_text \
    crates/broadwebd/src/http.rs \
    'pub enum ServiceRequest' \
    'broadwebd must keep the IPC-shaped service request envelope as a public DTO.'
require_text \
    crates/broadwebd/src/service_frame.rs \
    'pub const DEFAULT_SERVICE_FRAME_MAX_BYTES' \
    'broadwebd must keep service-envelope IPC framing bounded by a named byte limit.'
require_text \
    crates/broadwebd/src/service_frame.rs \
    'pub struct ServiceFrameCodec' \
    'broadwebd must expose a bounded service-envelope frame codec for future IPC clients.'
require_text \
    crates/broadwebd/src/service_frame.rs \
    'pub struct ServiceFrameBroadwebdClient' \
    'broadwebd must expose an in-process framed client adapter for socketless IPC-boundary tests.'
require_text \
    crates/broadwebd/src/service_frame.rs \
    'impl BroadwebdClient for ServiceFrameBroadwebdClient' \
    'broadwebd framed client adapter must implement the normal client trait.'
require_text \
    crates/broadwebd/src/peer_discovery.rs \
    'pub struct ProfileSyncPeerAdvertisement' \
    'broadwebd must keep a typed profile-sync peer advertisement DTO for discovery adapters.'
require_text \
    crates/broadwebd/src/peer_discovery.rs \
    'pub enum ProfileSyncPeerDiscoveryMessage' \
    'broadwebd must keep bounded solicit/advertisement messages for peer discovery adapters.'
require_text \
    crates/broadwebd/src/peer_discovery.rs \
    'pub trait ProfileSyncPeerDiscoveryProvider' \
    'broadwebd must keep profile-sync discovery behind a protocol-neutral provider trait.'
require_text \
    crates/broadwebd/src/peer_discovery.rs \
    'pub enum ProfileSyncPeerDiscoveryProtocol' \
    'broadwebd must model discovery protocols without coupling profile sync to a socket transport.'
require_text \
    crates/broadwebd/src/peer_discovery.rs \
    'SimulatedProfileSyncPeerDiscoveryNetwork' \
    'broadwebd must keep a socketless simulated discovery network for deterministic p2p fixture tests.'
require_text \
    crates/broadwebd/tests/in_process_network_fixture.rs \
    'in_process_profile_sync_peer_discovery_models_p2p_networks_without_sockets' \
    'in-process broadweb fixtures must cover p2p-shaped profile-sync discovery without loopback sockets.'
require_text \
    crates/broadwebd/src/protocols/ipfs/discovery.rs \
    'pub struct IpnsProfileSyncPeerDiscoveryProvider' \
    'broadwebd must keep a concrete IPNS-backed profile-sync discovery provider at the IPFS adapter boundary.'
require_text \
    crates/broadwebd/tests/in_process_network_fixture.rs \
    'ipns_profile_sync_peer_discovery_round_trips_through_kubo_model_without_sockets' \
    'IPNS-backed profile-sync discovery must be covered through the socketless Kubo model.'
require_text \
    crates/broadwebd/tests/peer_discovery_socket.rs \
    'SLATE_LOCAL_SOCKET_TESTS' \
    'local socket peer-discovery coverage must remain opt-in instead of binding sockets in default tests.'
require_text \
    crates/broadwebd/src/lib.rs \
    'daemon_dispatches_profile_sync_through_service_request_envelope' \
    'broadwebd must cover profile-sync dispatch through the IPC-shaped service request envelope.'
require_text \
    crates/broadwebd/src/lib.rs \
    'service_request_response_envelopes_round_trip_for_ipc_framing' \
    'broadwebd must cover service request/response envelope serialization for future IPC framing.'
require_text \
    crates/broadwebd/src/lib.rs \
    'encode_request\(&request\)' \
    'broadwebd serialization coverage must encode the service request envelope through the bounded frame codec.'
require_text \
    crates/broadwebd/src/lib.rs \
    'decode_request\(request_bytes.as_slice\(\)\)' \
    'broadwebd serialization coverage must decode the service request envelope through the bounded frame codec.'
require_text \
    crates/broadwebd/src/lib.rs \
    'service_frame_codec_rejects_oversized_encoded_request' \
    'broadwebd bounded frame codec must reject oversized encoded requests.'
require_text \
    crates/broadwebd/src/lib.rs \
    'service_frame_codec_rejects_oversized_decoded_request_before_json_parse' \
    'broadwebd bounded frame codec must reject oversized incoming frames before JSON parsing.'
require_text \
    crates/broadwebd/src/lib.rs \
    'service_frame_broadwebd_client_dispatches_profile_sync_through_byte_frames' \
    'broadwebd must prove profile-sync dispatch can run through byte-framed service envelopes.'
require_text \
    crates/profile-sync/src/lib.rs \
    'daemon: &.*dyn BroadwebdClient' \
    'slate-profile-sync provider handles must depend on the IPC-neutral broadwebd client trait.'
require_text \
    crates/profile-sync/src/lib.rs \
    '^mod discovery_trust;' \
    'slate-profile-sync must keep discovery trust filtering outside the monolithic lib.rs.'
require_text \
    crates/profile-sync/src/lib.rs \
    '^mod errors;' \
    'slate-profile-sync must keep shared error types outside the monolithic lib.rs.'
require_text \
    crates/profile-sync/src/lib.rs \
    '^mod health;' \
    'slate-profile-sync must keep scheduler-facing health DTO logic outside the monolithic lib.rs.'
require_text \
    crates/profile-sync/src/lib.rs \
    '^mod membership_log;' \
    'slate-profile-sync must keep membership-log DTO and plan helpers outside the monolithic lib.rs.'
require_text \
    crates/profile-sync/src/lib.rs \
    '^mod object_ids;' \
    'slate-profile-sync must keep object-id aggregation helpers outside the monolithic lib.rs.'
require_text \
    crates/profile-sync/src/lib.rs \
    '^mod root_ids;' \
    'slate-profile-sync must keep root-id naming helpers outside the monolithic lib.rs.'
require_text \
    crates/profile-sync/src/object_ids.rs \
    'push_unique_object_id_keeps_first_seen_order' \
    'slate-profile-sync object-id helper module must own focused unit tests.'
require_text \
    crates/profile-sync/src/errors.rs \
    'policy_error_display_includes_profile_role_and_counts' \
    'slate-profile-sync error module must own focused unit tests.'
require_text \
    crates/profile-sync/src/health.rs \
    'health_report_collects_root_object_provider_issues' \
    'slate-profile-sync health report module must own focused unit tests.'
require_text \
    crates/profile-sync/src/membership_log.rs \
    'membership_log_publication_plan_classifies_record_count_boundaries' \
    'slate-profile-sync membership-log module must own focused unit tests.'
require_text \
    crates/profile-sync/src/root_ids.rs \
    'settings_device_head_root_id_formats_per_device_head' \
    'slate-profile-sync root-id helper module must own focused unit tests.'
require_text \
    crates/profile-sync/src/lib.rs \
    'filter_trusted_profile_sync_peer_discovery_results' \
    'slate-profile-sync must filter broadwebd peer discovery candidates against local trust state before scheduler use.'
require_text \
    crates/profile-sync/src/discovery_trust.rs \
    'trusted_profile_sync_peer_discovery_filters_by_local_trust_state' \
    'slate-profile-sync peer discovery trust filtering must have focused regression coverage.'
require_text \
    docs/architecture/profile-sync.md \
    'Discovery is only candidate discovery' \
    'Profile-sync architecture must document that broadwebd discovery is candidate discovery until profile trust checks accept it.'
require_text \
    docs/roadmap.md \
    'first trust gate above broadwebd discovery' \
    'Roadmap must record the current profile-sync peer discovery trust gate.'
reject_protocol_model_leak \
    crates/profile-sync/src/lib.rs \
    'SettingsSyncRetentionProviderHandle.*BroadwebDaemon|retention_provider_daemons: \&\[\&BroadwebDaemon\]|Vec<\&.*BroadwebDaemon>|daemon: \&.*BroadwebDaemon' \
    'slate-profile-sync scheduler retention-provider APIs must not require the in-process BroadwebDaemon type.'

protocol_model_terms='slate-fixture|InProcessBroadwebNetwork|Internal[A-Za-z0-9_]*Fixture|ProfileSyncModel|socketless-fixture|in-process-fixture|simulated|Simulation|fixture model|internal model'

reject_protocol_model_leak \
    crates/broadwebd/src/protocols/mod.rs \
    "$protocol_model_terms" \
    'Protocol registry modules must stay fixture-blind; internal broadweb models belong below transport shims.'
reject_protocol_model_leak \
    crates/broadwebd/src/protocols/ipfs/address.rs \
    "$protocol_model_terms" \
    'IPFS address parsing must stay production-shaped; internal broadweb models belong below transport shims.'
reject_protocol_model_leak \
    crates/broadwebd/src/protocols/ipfs/config.rs \
    "$protocol_model_terms" \
    'IPFS runtime config must not know simulator endpoints or fixture model state.'
reject_protocol_model_leak \
    crates/broadwebd/src/protocols/ipfs/kubo.rs \
    "$protocol_model_terms" \
    'Kubo protocol implementation must build and parse real Kubo RPC semantics; fixture models may only swap the executor.'
reject_protocol_model_leak \
    crates/broadwebd/src/protocols/ipfs/gateway.rs \
    "$protocol_model_terms" \
    'IPFS gateway implementation must build real gateway requests; fixture models may only swap the HTTP executor.'
reject_protocol_model_leak \
    crates/broadwebd/src/protocols/ipfs/service.rs \
    "$protocol_model_terms" \
    'IPFS protocol service must remain unaware of internal broadweb simulation models.'
reject_protocol_model_leak \
    crates/broadwebd/src/protocols/tor/address.rs \
    "$protocol_model_terms" \
    'Tor address parsing must stay production-shaped; internal broadweb models belong below transport shims.'
reject_protocol_model_leak \
    crates/broadwebd/src/protocols/tor/arti_http.rs \
    "$protocol_model_terms" \
    'Tor transport implementation must not import internal broadweb simulation models.'
reject_protocol_model_leak \
    crates/broadwebd/src/protocols/tor/service.rs \
    "$protocol_model_terms" \
    'Tor protocol service must remain unaware of internal broadweb simulation models.'
reject_protocol_model_leak \
    crates/protocols/src/lib.rs \
    "$protocol_model_terms" \
    'slate-protocols must describe broadweb routing semantics without depending on deterministic fixture models.'
reject_protocol_model_leak \
    crates/routing/src/lib.rs \
    "$protocol_model_terms" \
    'slate-routing must parse and carry real routing plans without depending on deterministic fixture models.'

require_text \
    crates/broadwebd/src/protocols/ipfs/kubo_fixtures.rs \
    'impl IpfsKuboProfileSyncRpcExecutor for InternalKuboRpcTransportShim' \
    'Kubo fixture models must be reachable through the profile-sync RPC executor shim.'
require_text \
    crates/broadwebd/src/protocols/ipfs/kubo_fixtures.rs \
    'The shim only redirects Kubo-shaped requests to an in-process fixture' \
    'Kubo fixture shims must document that they replace socket IO, not protocol semantics.'

require_text \
    crates/broadwebd/src/protocols/ipfs/kubo_fixtures.rs \
    'impl IpfsKuboHttpContentExecutor for InternalKuboRpcTransportShim' \
    'Kubo fixture models must be reachable through the HTTP content executor shim.'

require_text \
    crates/broadwebd/src/protocols/ipfs/kubo_fixtures.rs \
    'impl TransportPlugin for InternalKuboRpcFixtureTransport' \
    'Kubo fixture HTTP transport must be a fixture-side wrapper, not a branch inside Kubo protocol code.'

require_text \
    crates/broadwebd/src/protocols/ipfs/kubo.rs \
    'impl IpfsKuboProfileSyncRpcExecutor for IpfsKuboReqwestProfileSyncRpcExecutor' \
    'Kubo profile-sync must keep a real HTTP executor so fixtures only swap transport.'
require_text \
    crates/broadwebd/src/services/profile_sync.rs \
    'KuboProfileSyncExecutorFactory' \
    'Kubo profile-sync services must choose socket behavior through an injected executor factory.'
require_text \
    crates/broadwebd/src/services/profile_sync.rs \
    'profile-sync/local-preview' \
    'runtime local profile-sync service must advertise the local preview backend capability.'
require_text \
    crates/broadwebd/src/lib.rs \
    'pub use crate::protocols::ipfs::kubo_fixtures::' \
    'In-process Kubo fixture shims must be exported only from the broadwebd test_fixtures module.'
require_text \
    crates/broadwebd/src/lib.rs \
    'impl KuboProfileSyncExecutorFactory for InProcessKuboProfileSyncExecutorFactory' \
    'In-process Kubo profile-sync fixtures must inject the socketless executor from the fixture layer.'
require_text \
    crates/profile-sync/src/lib.rs \
    'slate_broadwebd::test_fixtures::LocalProfileSyncFixture' \
    'profile-sync local preview helpers must import fixture models through broadwebd test_fixtures.'

require_text \
    crates/broadwebd/src/protocols/ipfs/gateway.rs \
    'IpfsGatewayHttpExecutor' \
    'IPFS gateway fetches must go through an executor boundary.'
require_text \
    crates/broadwebd/src/protocols/ipfs/gateway.rs \
    'fetch_http_with_executor' \
    'IPFS gateway transport must expose executor-based fetching for fixture shims.'
require_text \
    crates/broadwebd/src/protocols/ipfs/gateway.rs \
    'fetch_http_url_over_network' \
    'Default IPFS gateway fetching must use the real-network HTTP helper.'
require_text \
    crates/broadwebd/src/lib.rs \
    'impl IpfsGatewayHttpExecutor for InProcessIpfsGatewayFixtureExecutor' \
    'In-process IPFS gateway fixtures must swap socket IO through the gateway HTTP executor.'
require_text \
    crates/broadwebd/src/lib.rs \
    'impl TransportPlugin for InProcessIpfsGatewayFixtureTransport' \
    'In-process IPFS gateway fixtures must be installed as transport wrappers.'
require_text \
    crates/broadwebd/src/lib.rs \
    'fetch_internal_fixture_http_url' \
    'In-process HTTP fixture transports must call explicit fixture fetch helpers.'
require_text \
    crates/broadwebd/src/lib.rs \
    'impl ProtocolService for InProcessFixtureHttpProtocolService' \
    'In-process HTTP fixture URL routing must live in a fixture protocol service.'
require_text \
    crates/broadwebd/src/lib.rs \
    'default_registry_does_not_resolve_in_process_http_fixture_urls' \
    'Default registry must have a regression proving fixture URLs do not resolve without in-process fixtures.'

require_text \
    crates/broadwebd/src/lib.rs \
    'with_prevalidated_local_gateway' \
    'In-process IPFS gateway fixtures must be injected through a prevalidated fixture-layer config path.'

require_text \
    crates/broadwebd/src/registry.rs \
    'with_default_http_and_kubo_profile_sync' \
    'broadwebd must expose an explicit local-Kubo profile-sync registry constructor.'

require_text \
    crates/broadwebd/src/services/profile_sync.rs \
    'pub fn from_environment\(\) -> Result<Self, BroadwebdError>' \
    'profile-sync backend selection must remain explicit runtime configuration.'
require_text \
    crates/broadwebd/src/lib.rs \
    'profile_sync_runtime_options_reject_fixture_backends_and_endpoints' \
    'profile-sync runtime configuration must reject fixture backend names and synthetic endpoint refs.'

require_text \
    crates/profile-sync/src/lib.rs \
    'SettingsSyncProtocolProviderMaterializerBoundary' \
    'profile-sync protocol materializers must distinguish runtime adapters from local simulations.'
require_text \
    crates/profile-sync/src/lib.rs \
    'local_deterministic_simulation' \
    'profile-sync local protocol simulations must be explicit materializer policies.'
require_text \
    crates/profile-sync/src/lib.rs \
    'with_max_materialized_providers' \
    'profile-sync protocol materializers must expose explicit provider materialization caps for constrained runs.'
require_text \
    crates/profile-sync/src/lib.rs \
    'protocol_provider_materializer_enforces_materialized_provider_limit' \
    'profile-sync protocol materializer capacity limits must have focused regression coverage.'
require_text \
    crates/profile-sync/src/lib.rs \
    'scheduler_runs_with_iroh_node_protocol_materialized_provider_through_framed_clients' \
    'Iroh-shaped protocol materialization must run through byte-framed BroadwebdClient handles in local tests.'
require_text \
    crates/profile-sync/src/lib.rs \
    'broadwebd_kubo_materialized_provider_runs_through_framed_client' \
    'Kubo-shaped protocol materialization must run through a byte-framed BroadwebdClient handle in local tests.'
require_text \
    crates/profile-sync/src/lib.rs \
    'scheduler_protocol_stored_compaction_derives_active_key_through_framed_clients' \
    'Secret-backed protocol materialized compaction must run through byte-framed BroadwebdClient handles in local tests.'
require_text \
    crates/profile-sync/src/lib.rs \
    'daemon: &'\''a dyn BroadwebdClient' \
    'profile-sync runtime bridge wrappers must consume the IPC-neutral broadwebd client boundary.'
require_text \
    crates/profile-sync/src/lib.rs \
    'broadwebd_profile_sync_bridges_accept_client_trait_objects' \
    'profile-sync bridge wrappers must have focused regression coverage for BroadwebdClient trait objects.'
require_text \
    crates/profile-sync/src/lib.rs \
    'broadwebd_profile_sync_bridges_accept_envelope_only_clients' \
    'profile-sync bridge wrappers must work through an envelope-only BroadwebdClient implementation.'
require_text \
    crates/profile-sync/src/lib.rs \
    'broadwebd_profile_sync_bridges_accept_framed_clients' \
    'profile-sync bridge wrappers must work through a byte-framed BroadwebdClient implementation.'
require_text \
    crates/profile-sync/src/lib.rs \
    'broadwebd_settings_sync_cycle_runs_through_framed_clients' \
    'settings sync cycles must run through byte-framed BroadwebdClient implementations.'
require_text \
    crates/profile-sync/src/lib.rs \
    'broadwebd_settings_sync_cycle_retains_with_framed_selected_provider' \
    'settings sync selected retention providers must run through byte-framed BroadwebdClient implementations.'
require_text \
    crates/profile-sync/src/lib.rs \
    'SettingsSyncRootObjectProviderIssue' \
    'profile-sync health reports must expose structured root-object provider issues.'
require_text \
    crates/storage/src/lib.rs \
    'pub app_domains: Vec<AppSyncDomainRecord>' \
    'profile-sync local readiness must carry app sync domain records for settings preview status.'
require_text \
    crates/storage/src/lib.rs \
    'pub enabled_sync_content_domain_count: usize' \
    'profile-sync local readiness must expose enabled sync-content app domain counts.'
require_text \
    crates/storage/src/lib.rs \
    'pub app_domain_readiness: Vec<AppSyncDomainReadinessRecord>' \
    'profile-sync local readiness must expose app-domain revision heads.'
require_text \
    crates/storage/src/lib.rs \
    'pub storage_providers: Vec<StorageProviderRecord>' \
    'profile-sync local readiness must carry storage provider records for settings preview status.'
require_text \
    crates/storage/src/lib.rs \
    'pub authorized_retention_provider_ids: Vec<String>' \
    'profile-sync local readiness must expose authorized retention provider ids.'
require_text \
    crates/storage/src/lib.rs \
    'pub provider_authority_device_count: usize' \
    'profile-sync local readiness must expose provider-authority device counts.'
require_text \
    crates/storage/src/lib.rs \
    'pub trusted_provider_authority_device_count: usize' \
    'profile-sync local readiness must expose trusted provider-authority device counts.'
require_text \
    crates/profile-sync/src/lib.rs \
    'root_object_provider_issues: Vec<LocalSettingsSyncRootObjectProviderIssueSummary>' \
    'profile-sync local preview reports must carry root-object provider issue summaries.'
require_text \
    crates/profile-sync/src/lib.rs \
    'retention_provider_selection_issues: Vec<LocalSettingsSyncProviderIssueSummary>' \
    'profile-sync local preview reports must carry retention provider selection issue summaries.'
require_text \
    crates/profile-sync/src/lib.rs \
    'stored_provider_metadata_issues: Vec<LocalSettingsSyncProviderIssueSummary>' \
    'profile-sync local preview reports must carry stored provider metadata issue summaries.'
reject_protocol_model_leak \
    crates/profile-sync/src/lib.rs \
    'socketless_fixture_models' \
    'profile-sync materializer policies should name local simulation boundaries, not fixture models.'

require_text \
    docs/architecture/broadwebd.md \
    'Fixture shims should be exported through `test_fixtures`, not' \
    'broadwebd architecture must document that protocol modules do not export fixture shims.'
require_text \
    docs/roadmap.md \
    'Kubo fixture shims are exported' \
    'Roadmap must record that socketless models stay outside the normal protocol API.'
require_text \
    docs/architecture/broadwebd.md \
    '`BroadwebdClient` trait' \
    'broadwebd architecture must document the current IPC-neutral in-process client trait.'
require_text \
    docs/architecture/broadwebd.md \
    '`dispatch_service_request` entry point over' \
    'broadwebd architecture must document the IPC-shaped service request envelope.'
require_text \
    docs/architecture/broadwebd.md \
    'round-trip through JSON' \
    'broadwebd architecture must document that service envelopes are serializable before IPC.'
require_text \
    docs/architecture/broadwebd.md \
    'bounded JSON service-frame codec' \
    'broadwebd architecture must document the bounded service-frame codec.'
require_text \
    docs/architecture/broadwebd.md \
    'socketless framed-client adapter' \
    'broadwebd architecture must document the socketless framed-client adapter.'
require_text \
    docs/roadmap.md \
    'public `BroadwebdClient` trait' \
    'Roadmap must record the current broadwebd client boundary.'
require_text \
    docs/roadmap.md \
    '`dispatch_service_request` method over `ServiceRequest`' \
    'Roadmap must record broadwebd service-envelope dispatch.'
require_text \
    docs/roadmap.md \
    'round-trip through JSON' \
    'Roadmap must record service-envelope serialization for future IPC framing.'
require_text \
    docs/roadmap.md \
    'bounded JSON service-frame codec' \
    'Roadmap must record the bounded broadwebd service-frame codec.'
require_text \
    docs/roadmap.md \
    'socketless framed-client adapter' \
    'Roadmap must record the broadwebd socketless framed-client adapter.'
require_text \
    docs/architecture/profile-sync.md \
    '`ProfileSyncObjectSource` trait over broadwebd'\''s `BroadwebdClient` boundary' \
    'Profile-sync architecture must document that runtime bridge wrappers use BroadwebdClient.'
require_text \
    docs/architecture/profile-sync.md \
    'envelope-only client that implements `dispatch_service_request`' \
    'Profile-sync architecture must document envelope-only bridge coverage.'
require_text \
    docs/roadmap.md \
    'source, publisher, runner, and scheduler bridge wrappers now hold' \
    'Roadmap must record profile-sync bridge adoption of the BroadwebdClient boundary.'
require_text \
    docs/roadmap.md \
    'envelope-only `BroadwebdClient` regression' \
    'Roadmap must record envelope-only profile-sync bridge coverage.'
require_text \
    docs/architecture/profile-sync.md \
    'checks the chrome synced-settings watcher wiring' \
    'Profile-sync architecture must document the low-memory chrome watcher verification strategy.'
require_text \
    docs/roadmap.md \
    'also cover the synced-settings watcher wiring' \
    'Roadmap must record low-memory static chrome watcher verification.'
require_text \
    docs/roadmap.md \
    'Socketless broadweb models must stay behind transport shims' \
    'Roadmap must record that protocol models only replace socket IO, not protocol implementation logic.'
require_text \
    crates/broadwebd/src/lib.rs \
    'with_profile_sync_capacity' \
    'In-process broadweb fixtures must expose profile-sync capacity controls for local simulations.'
require_text \
    crates/broadwebd/src/protocols/ipfs/kubo_fixtures.rs \
    'InternalKuboProfileSyncModelCapacity' \
    'Socketless Kubo/IPNS profile-sync models must expose fixture-local capacity controls.'
require_text \
    crates/broadwebd/tests/in_process_network_fixture.rs \
    'in_process_kubo_profile_sync_model_enforces_capacity_before_sockets' \
    'Kubo/IPNS profile-sync capacity must be verified through the socketless transport shim.'
require_text \
    docs/roadmap.md \
    'socketless model rejects new encrypted objects' \
    'Roadmap must record bounded Kubo/IPNS profile-sync fixture state.'
require_text \
    crates/profile-sync/src/lib.rs \
    'scheduler_surfaces_iroh_node_materialized_provider_fixture_capacity' \
    'Iroh-shaped protocol materializer coverage must include bounded fixture-state enforcement.'
require_text \
    docs/roadmap.md \
    'Iroh-shaped materialized-provider path now also' \
    'Roadmap must record bounded Iroh-shaped materialized-provider fixture state.'
require_text \
    docs/roadmap.md \
    'capacity-exceeded providers' \
    'Roadmap must record bounded protocol materializer provider capacity.'

require_text \
    crates/chrome/src/desktop/protocols/slate.rs \
    'settings/profile-sync/handoff/create' \
    'Slate settings protocol must expose the profile-sync handoff create route.'
require_text \
    crates/chrome/src/desktop/protocols/slate.rs \
    'settings/profile-sync/handoff/import' \
    'Slate settings protocol must expose the profile-sync handoff import route.'
require_text \
    crates/chrome/src/desktop/protocols/slate.rs \
    'handoff_export_text' \
    'Slate settings profile-sync JSON must include handoff export text for local file trials.'
require_text \
    crates/chrome/src/desktop/protocols/slate.rs \
    'to_json_with_handoff_export' \
    'Slate settings profile-sync JSON must expose secret-bearing enrollment files only on explicit handoff creation responses.'
require_text \
    crates/chrome/src/desktop/protocols/slate.rs \
    'PROFILE_SYNC_SECRET_HANDOFF_BUNDLE_MAX_BYTES' \
    'Slate settings profile-sync import route must use the storage-owned secret handoff bundle size limit.'
require_text \
    crates/storage/src/lib.rs \
    'PROFILE_SYNC_SECRET_HANDOFF_BUNDLE_MAX_BYTES' \
    'Storage must expose a profile-sync secret handoff bundle size limit for all enrollment-file callers.'
require_text \
    crates/storage/src/lib.rs \
    'validate_profile_sync_secret_handoff_bundle_size\(bytes.len\(\)\)' \
    'Storage must reject oversized secret handoff bundles before JSON parsing.'
reject_protocol_model_leak \
    crates/chrome/src/desktop/protocols/slate.rs \
    '"export_text":|"device_request_export_text":|"enrollment_export_text":|"export_filename": "slate-sync-secret.json"' \
    'Slate settings profile-sync public JSON must not expose raw key-file, device-request, or internal enrollment-bundle artifacts.'
reject_protocol_model_leak \
    crates/chrome/src/desktop/protocols/slate.rs \
    'settings/profile-sync/(import|device-request|enrollment)' \
    'Slate settings profile-sync protocol must expose only the single enrollment-file handoff route, not raw key, device-request, or internal enrollment-bundle routes.'
reject_protocol_model_leak \
    resources/resource_protocol/slate-settings.html \
    'id="profile-sync-handoff-create"' \
    'Slate settings page must not expose a separate handoff creation control; download should create the file on demand.'
require_text \
    resources/resource_protocol/slate-settings.html \
    'Download enrollment file' \
    'Slate settings page must expose a single enrollment-file download action.'
require_text \
    resources/resource_protocol/slate-settings.html \
    'id="profile-sync-handoff-import"' \
    'Slate settings page must expose profile-sync enrollment-file import.'
require_text \
    resources/resource_protocol/slate-settings.html \
    'handoff/create' \
    'Slate settings page must call the handoff create route.'
require_text \
    resources/resource_protocol/slate-settings.html \
    'handoff/import' \
    'Slate settings page must call the handoff import route.'
require_text \
    resources/resource_protocol/slate-settings.html \
    'PROFILE_SYNC_HANDOFF_FILE_MAX_BYTES' \
    'Slate settings page must bound enrollment file reads before importing through the protocol URL.'
require_text \
    crates/chrome/src/desktop/settings_watcher.rs \
    'AppSyncDomainWatcher::new' \
    'Chrome synced-settings watcher must use storage raw app-domain watcher instead of ad hoc revision polling.'
require_text \
    crates/chrome/src/desktop/settings_watcher.rs \
    'SYNC_DOMAIN_SETTINGS' \
    'Chrome synced-settings watcher must subscribe only to the settings sync domain.'
require_text \
    crates/chrome/src/desktop/settings_watcher.rs \
    'poll_apply_and_acknowledge' \
    'Chrome synced-settings watcher must advance its cursor only after applying a batch.'
require_text \
    crates/chrome/src/desktop/settings_watcher.rs \
    'apply_synced_chrome_settings_events' \
    'Chrome synced-settings watcher must dispatch storage events through the chrome settings apply path.'
require_text \
    crates/chrome/src/desktop/settings_watcher.rs \
    'initialize_chrome_settings_from_database' \
    'Chrome synced-settings watcher must initialize runtime chrome state from slate-settings.db.'
require_text \
    crates/chrome/src/desktop/app.rs \
    'synced_settings_watcher: SyncedChromeSettingsWatcher' \
    'Chrome app must own the synced settings watcher.'
require_text \
    crates/chrome/src/desktop/app.rs \
    'self.synced_settings_watcher.poll_once_logged\(\);' \
    'Chrome app must poll synced settings during the runtime update loop.'
require_text \
    crates/chrome/src/desktop/protocols/slate.rs \
    'apply_key_binding_setting' \
    'Chrome synced settings must apply keybinding updates through the runtime keybinding path.'
require_text \
    crates/chrome/src/desktop/protocols/slate.rs \
    'set_current_chrome_element_zoom_setting' \
    'Chrome synced settings must apply zoom updates through the runtime zoom path.'
require_text \
    crates/profile-sync/src/preview_json.rs \
    'root_object_provider_issues' \
    'Slate settings profile-sync JSON must expose root-object provider issue summaries.'
require_text \
    crates/profile-sync/src/preview_json.rs \
    'retention_provider_selection_issues' \
    'Slate settings profile-sync JSON must expose retention provider selection issue summaries.'
require_text \
    crates/profile-sync/src/preview_json.rs \
    'stored_provider_metadata_issues' \
    'Slate settings profile-sync JSON must expose stored provider metadata issue summaries.'
require_text \
    crates/chrome/src/desktop/protocols/slate.rs \
    'profile_sync_app_domains_json' \
    'Slate settings profile-sync JSON must expose app sync domain records.'
require_text \
    crates/chrome/src/desktop/protocols/slate.rs \
    'profile_sync_app_domain_readiness_json' \
    'Slate settings profile-sync JSON must expose app-domain readiness records.'
require_text \
    crates/chrome/src/desktop/protocols/slate.rs \
    '"enabled_sync_content_domain_count": readiness.enabled_sync_content_domain_count' \
    'Slate settings profile-sync JSON must expose enabled sync-content app domain counts.'
require_text \
    crates/chrome/src/desktop/protocols/slate.rs \
    '"authorized_retention_provider_ids": readiness.authorized_retention_provider_ids.as_slice\(\)' \
    'Slate settings profile-sync JSON must expose authorized retention provider ids.'
require_text \
    crates/chrome/src/desktop/protocols/slate.rs \
    'profile_sync_storage_providers_json' \
    'Slate settings profile-sync JSON must expose storage provider records.'
require_text \
    crates/chrome/src/desktop/protocols/slate.rs \
    '"provider_authority_device_count": readiness.provider_authority_device_count' \
    'Slate settings profile-sync JSON must expose provider-authority device counts.'
require_text \
    crates/chrome/src/desktop/protocols/slate.rs \
    '"trusted_provider_authority_device_count": readiness.trusted_provider_authority_device_count' \
    'Slate settings profile-sync JSON must expose trusted provider-authority device counts.'
require_text \
    crates/chrome/src/desktop/protocols/slate.rs \
    '"endpoint_ref": provider.endpoint_ref.as_deref\(\)' \
    'Slate settings profile-sync JSON must expose provider endpoint refs as profile metadata.'
require_text \
    crates/profile-sync/src/preview_json.rs \
    'selected_endpoint_pending_protocol_provider_count' \
    'Slate settings profile-sync JSON must expose selected endpoint materialization summary counts.'
require_text \
    crates/profile-sync/src/preview_json.rs \
    'retention_issues' \
    'Slate settings profile-sync JSON must expose retained-object issue summaries.'
require_text \
    resources/resource_protocol/slate-settings.html \
    'Sync issues' \
    'Slate settings page must show profile-sync issue status.'
require_text \
    resources/resource_protocol/slate-settings.html \
    'Enabled domains' \
    'Slate settings page must show enabled app sync domains.'
require_text \
    resources/resource_protocol/slate-settings.html \
    'Domain heads' \
    'Slate settings page must show app-domain sync revision heads.'
require_text \
    resources/resource_protocol/slate-settings.html \
    'profileSyncAppDomainHeadStatus' \
    'Slate settings page must render app-domain sync revision heads.'
require_text \
    resources/resource_protocol/slate-settings.html \
    'Content domains' \
    'Slate settings page must show enabled sync-content app domains.'
require_text \
    resources/resource_protocol/slate-settings.html \
    'profileSyncContentAppDomainStatus' \
    'Slate settings page must render sync-content app domain status.'
require_text \
    resources/resource_protocol/slate-settings.html \
    'Active providers' \
    'Slate settings page must show active profile-sync storage providers.'
require_text \
    resources/resource_protocol/slate-settings.html \
    'Authorized providers' \
    'Slate settings page must show account-authorized profile-sync storage providers.'
require_text \
    resources/resource_protocol/slate-settings.html \
    'profileSyncAuthorizedProviderStatus' \
    'Slate settings page must render authorized provider status.'
require_text \
    resources/resource_protocol/slate-settings.html \
    'Provider-authority devices' \
    'Slate settings page must show trusted/total provider-authority device status.'
require_text \
    resources/resource_protocol/slate-settings.html \
    'Provider endpoints' \
    'Slate settings page must show provider endpoint metadata for sync trials.'
require_text \
    resources/resource_protocol/slate-settings.html \
    'profileSyncProviderEndpointStatus' \
    'Slate settings page must render provider endpoint metadata without scheduler internals.'
require_text \
    resources/resource_protocol/slate-settings.html \
    'Endpoint status' \
    'Slate settings page must show selected endpoint materialization status.'
require_text \
    resources/resource_protocol/slate-settings.html \
    'profileSyncEndpointMaterializationStatus' \
    'Slate settings page must render endpoint materialization status from protocol-neutral reports.'
require_text \
    resources/resource_protocol/slate-settings.html \
    'Sync health' \
    'Slate settings page must show profile-sync health after local sync checks.'
require_text \
    resources/resource_protocol/slate-settings.html \
    'profileSyncHealthStatus' \
    'Slate settings page must distinguish healthy, recovered, and degraded profile-sync health.'
require_text \
    resources/resource_protocol/slate-settings.html \
    'Discovery' \
    'Slate settings page must show profile-sync discovery preflight status.'
require_text \
    resources/resource_protocol/slate-settings.html \
    'profileSyncDiscoveryStatus' \
    'Slate settings page must render trusted-discovery preflight counts.'
require_text \
    resources/resource_protocol/slate-settings.html \
    'discovery_endpoint_pending_protocol_provider_count' \
    'Slate settings page must expose discovered provider endpoint materialization status.'
require_text \
    crates/profile-sync/src/preview_json.rs \
    'pub fn local_settings_sync_preview_cycle_report_json' \
    'Profile-sync reports must own low-memory JSON serialization for Settings preview runs.'
require_text \
    crates/chrome/src/desktop/protocols/slate.rs \
    'local_settings_sync_preview_cycle_report_json' \
    'Slate chrome must delegate profile-sync preview report JSON serialization to slate-profile-sync.'
reject_protocol_model_leak \
    crates/chrome/src/desktop/protocols/slate.rs \
    'fn profile_sync_discovery_rejections_json' \
    'Slate chrome must not duplicate profile-sync discovery JSON serialization.'
require_text \
    resources/resource_protocol/slate-settings.html \
    'discovery_rejections' \
    'Slate settings page must include profile-sync discovery rejections in issue details.'
require_text \
    resources/resource_protocol/slate-settings.html \
    'candidates.push\(state.last_two_device_trial\)' \
    'Slate settings page must include the two-device local trial in profile-sync health and issue summaries.'
require_text \
    resources/resource_protocol/slate-settings.html \
    'retention_issues' \
    'Slate settings page must include retained-object issues in sync issue details.'
require_text \
    resources/resource_protocol/slate-settings.html \
    'Issue details' \
    'Slate settings page must expose protocol-neutral profile-sync issue details.'
require_text \
    resources/resource_protocol/slate-settings.html \
    'profile-sync-handoff-device' \
    'Slate settings page must expose one target-device field for enrollment file handoff.'
require_text \
    resources/resource_protocol/slate-settings.html \
    'Download enrollment file' \
    'Slate settings page must expose one enrollment file export action.'
require_text \
    resources/resource_protocol/slate-settings.html \
    'Import enrollment file' \
    'Slate settings page must expose one enrollment file import action.'
reject_protocol_model_leak \
    resources/resource_protocol/slate-settings.html \
    'id="profile-sync-(secret|download|import|device-request|enrollment)([^a-zA-Z_-]|")' \
    'Settings Profile Sync Preview must not expose obsolete internal key, device-request, or enrollment-bundle file controls.'
reject_protocol_model_leak \
    resources/resource_protocol/slate-settings.html \
    'slate-sync-secret\.json|Download internal key file|Import internal key|internal device request|internal enrollment bundle' \
    'Settings Profile Sync Preview should present a single enrollment-file handoff, not internal sync artifacts.'

run_test slate-apps sync_domains
run_test slate-storage rail_app_sync_domains_match_seeded_storage_domains
run_test slate-storage slate_sync_secret_derives_domain_separated_profile_material
run_test slate-storage slate_sync_secret_derives_stable_profile_device_signers
run_test slate-storage slate_sync_secret_export_import_can_require_profile_match
run_test slate-storage slate_sync_secret_export_round_trips_with_redacted_debug
run_test slate-storage profile_sync_device_enrollment_request_round_trips_without_secret_material
run_test slate-storage profile_sync_enrollment_bundle_can_be_derived_from_device_request
run_test slate-storage profile_sync_enrollment_bundle_can_be_derived_from_sync_secret
run_test slate-storage profile_sync_secret_handoff_bundle
run_test slate-storage profile_sync_local_activation_records_non_secret_metadata
run_test slate-storage profile_sync_secret_activation_trusts_derived_local_signer_without_storing_secret
run_test slate-storage profile_sync_local_readiness_reports_provider_gap
run_test slate-storage profile_sync_local_readiness_reports_authorized_retention_provider
run_test slate-storage profile_sync_local_readiness_counts_enabled_sync_content_domains
run_test slate-storage profile_sync_local_readiness_reports_app_domain_revisions
run_test slate-storage storage_provider_writes_metadata_sync_change_without_secrets_or_local_state
run_test slate-storage tests::app_sync_domain_watcher_polls_and_acknowledges_batches
run_test slate-storage typed_app_sync_domain_poll_decodes_payloads_and_records_cursor
run_test_with_features slate-broadwebd test-fixtures app_domain_metadata_syncs_through_profile_fixture
run_test_with_features slate-broadwebd test-fixtures ipfs_kubo_profile_sync_fixture_executor_rejects_non_fixture_endpoints
run_test_with_features slate-broadwebd test-fixtures kubo_profile_sync_fixture_reports_protocol_semantics_over_fixture_executor
run_test_with_features slate-broadwebd test-fixtures kubo_profile_sync_http_service_advertises_real_http_boundary
run_test_with_features slate-broadwebd test-fixtures profile_sync_runtime_options
run_test_with_features slate-broadwebd test-fixtures ipfs_gateway_fixtures_are_injected_without_runtime_config_backdoor
run_test_with_features slate-broadwebd test-fixtures default_registry_does_not_resolve_in_process_http_fixture_urls
run_test_with_features slate-broadwebd test-fixtures http_fetch_uses_in_process_http_transport
run_test_with_features slate-broadwebd test-fixtures registry_can_opt_into_kubo_profile_sync_without_fixture_transport
run_test_with_features slate-broadwebd test-fixtures registry_can_apply_kubo_profile_sync_runtime_config
run_test_with_features slate-broadwebd test-fixtures registry_rejects_external_kubo_profile_sync_endpoint
run_test slate-broadwebd daemon_dispatches_through_ipc_neutral_client_trait
run_test slate-broadwebd daemon_dispatches_profile_sync_through_service_request_envelope
run_test slate-broadwebd service_request_response_envelopes_round_trip_for_ipc_framing
run_test slate-broadwebd service_frame
run_test slate-broadwebd peer_discovery
run_test_with_features slate-broadwebd test-fixtures in_process_profile_sync_peer_discovery_models_p2p_networks_without_sockets
run_test_with_features slate-broadwebd test-fixtures ipns_profile_sync_peer_discovery_round_trips_through_kubo_model_without_sockets
run_test_with_features slate-broadwebd test-fixtures ipns_profile_sync_peer_discovery_prefers_freshest_sequence_without_sockets
run_test_with_features slate-broadwebd test-fixtures kubo_profile_sync_get_uses_transport_bytes_not_local_upload_metadata
run_test_with_features slate-broadwebd test-fixtures kubo_profile_sync_model_round_trips_state_without_canned_responses
run_test_with_features slate-broadwebd test-fixtures kubo_profile_sync_model_release_updates_retention_health
run_test slate-broadwebd local_fixture_root_health_reports_stale_latest_object_holders
run_test slate-broadwebd local_fixture_root_health_reports_offline_latest_object_holders
run_test slate-broadwebd local_fixture_retained_claim_requires_provider_bytes
run_test slate-profile-sync health::tests
run_test slate-profile-sync membership_log::tests
run_test slate-profile-sync root_ids::tests
run_test slate-profile-sync trusted_profile_sync_peer_discovery_filters_by_local_trust_state
run_test slate-profile-sync trusted_profile_sync_peer_discovery_prefers_fresh_sequence_and_rejects_replays
run_test slate-profile-sync trusted_discovery_endpoint_materialization_keeps_socket_addresses_fail_closed
run_test slate-profile-sync broadwebd_settings_sync_scheduler_materializes_trusted_discovered_multiaddr_provider
run_test slate-profile-sync broadwebd_profile_sync_bridges_accept_envelope_only_clients
run_test slate-profile-sync broadwebd_profile_sync_bridges_accept_client_trait_objects
run_test slate-profile-sync broadwebd_profile_sync_bridges_accept_framed_clients
run_test slate-profile-sync broadwebd_settings_sync_cycle_runs_through_framed_clients
run_test slate-profile-sync broadwebd_settings_sync_cycle_retains_with_framed_selected_provider
run_test slate-profile-sync broadwebd_publisher_and_source_use_stateful_kubo_profile_sync_model
run_test slate-profile-sync broadwebd_stateful_kubo_model_shares_roots_and_objects_across_daemons
run_test slate-profile-sync broadwebd_publisher_publishes_signed_settings_tail_manifest_through_stateful_kubo_model
run_test slate-profile-sync broadwebd_settings_sync_cycle_retains_losing_shared_root_conflict
run_test slate-profile-sync broadwebd_source_ignores_stale_device_head_root_rollback
run_test slate-profile-sync broadwebd_source_records_missing_device_head_after_shared_root_sequence_seen
run_test slate-profile-sync broadwebd_source_ignores_stale_shared_root_candidate_rollback
run_test slate-profile-sync broadwebd_source_rejects_corrupt_shared_root_object_without_mutation
run_test slate-profile-sync broadwebd_source_rejects_provider_authority_shared_root_without_mutation
run_test slate-profile-sync broadwebd_source_rejects_wrong_key_id_shared_root_without_mutation
run_test slate-profile-sync broadwebd_source_rejects_bad_signature_shared_root_without_mutation
run_test slate-profile-sync broadwebd_source_rejects_malformed_app_payload_shared_root_without_mutation
run_test slate-profile-sync broadwebd_source_rejects_missing_tail_object_shared_root_without_mutation
run_test slate-profile-sync broadwebd_settings_sync_health_reports_delayed_object_transfer
run_test slate-profile-sync broadwebd_settings_sync_health_reports_unavailable_retained_provider_bytes
run_test slate-profile-sync broadwebd_settings_sync_health_reports_stale_latest_object_holders
run_test slate-profile-sync broadwebd_settings_sync_health_reports_offline_latest_object_holders
run_test slate-profile-sync iroh_node
run_test slate-profile-sync protocol_provider_materializer_enforces_materialized_provider_limit
run_test slate-profile-sync broadwebd_settings_compaction_retains_objects_before_strict_root_policy_check
run_test slate-profile-sync broadwebd_settings_sync_scheduler_compacts_with_selected_retention_provider_handles
run_test slate-profile-sync broadwebd_settings_sync_scheduler_compacts_with_stored_retention_provider_handles
run_test slate-profile-sync broadwebd_settings_sync_scheduler_stored_compaction_derives_active_key_from_sync_secret
run_test slate-profile-sync scheduler_stored_fixture_compaction_derives_active_key_from_sync_secret
run_test slate-profile-sync scheduler_protocol_stored_compaction_derives_active_key_through_framed_clients
run_test slate-profile-sync broadwebd_kubo_materialized_provider_runs_through_framed_client
run_test slate-profile-sync scheduler_membership_fixture_stored_provider_derives_active_key_from_sync_secret
run_test_with_features slate-profile-sync local-preview-fixtures local_settings_sync_current_cycle_publishes_existing_settings_without_preview_write
run_test_with_features slate-profile-sync local-preview-fixtures local_settings_sync_preview_cycle_publishes_and_retains_without_loopback
run_test_with_features slate-profile-sync local-preview-fixtures local_settings_sync_two_device_preview_cycle_applies_on_receiver_without_loopback
run_test slate-profile-sync broadwebd_settings_sync_scheduler_rejects_stale_fixture_endpoint_provider_refs

case "$check_chrome" in
    1 | true | yes)
        run_test slate-chrome watcher_applies_new_sync_revisions_incrementally
        ;;
    *)
        printf '\n==> slate-chrome :: watcher_applies_new_sync_revisions_incrementally\n'
        printf 'skipped by default: compiling slate-chrome currently pulls Servo script bindings and exceeds the low-memory profile. Set SLATE_PROFILE_SYNC_CHECK_CHROME=1 with a larger SLATE_BUILD_MEMORY_LIMIT_MB to run it.\n'
        ;;
esac
