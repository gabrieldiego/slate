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
export CARGO_INCREMENTAL=${CARGO_INCREMENTAL:-0}

mkdir -p "$TMPDIR"

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
    'SettingsSyncRootObjectProviderIssue' \
    'profile-sync health reports must expose structured root-object provider issues.'
require_text \
    crates/storage/src/lib.rs \
    'pub app_domains: Vec<AppSyncDomainRecord>' \
    'profile-sync local readiness must carry app sync domain records for settings preview status.'
require_text \
    crates/storage/src/lib.rs \
    'pub storage_providers: Vec<StorageProviderRecord>' \
    'profile-sync local readiness must carry storage provider records for settings preview status.'
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
    docs/roadmap.md \
    'Socketless broadweb models must stay behind transport shims' \
    'Roadmap must record that protocol models only replace socket IO, not protocol implementation logic.'

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
    resources/resource_protocol/slate-settings.html \
    'id="profile-sync-handoff-create" hidden' \
    'Slate settings page must keep low-level handoff creation hidden behind the file download action.'
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
    crates/chrome/src/desktop/protocols/slate.rs \
    'root_object_provider_issues' \
    'Slate settings profile-sync JSON must expose root-object provider issue summaries.'
require_text \
    crates/chrome/src/desktop/protocols/slate.rs \
    'retention_provider_selection_issues' \
    'Slate settings profile-sync JSON must expose retention provider selection issue summaries.'
require_text \
    crates/chrome/src/desktop/protocols/slate.rs \
    'stored_provider_metadata_issues' \
    'Slate settings profile-sync JSON must expose stored provider metadata issue summaries.'
require_text \
    crates/chrome/src/desktop/protocols/slate.rs \
    'profile_sync_app_domains_json' \
    'Slate settings profile-sync JSON must expose app sync domain records.'
require_text \
    crates/chrome/src/desktop/protocols/slate.rs \
    'profile_sync_storage_providers_json' \
    'Slate settings profile-sync JSON must expose storage provider records.'
require_text \
    crates/chrome/src/desktop/protocols/slate.rs \
    '"endpoint_ref": provider.endpoint_ref.as_deref\(\)' \
    'Slate settings profile-sync JSON must expose provider endpoint refs as profile metadata.'
require_text \
    crates/chrome/src/desktop/protocols/slate.rs \
    'selected_endpoint_pending_protocol_provider_count' \
    'Slate settings profile-sync JSON must expose selected endpoint materialization summary counts.'
require_text \
    crates/chrome/src/desktop/protocols/slate.rs \
    'retention_issues: Vec<LocalSettingsSyncRetentionIssueSummary>' \
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
    'Active providers' \
    'Slate settings page must show active profile-sync storage providers.'
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
    'retention_issues' \
    'Slate settings page must include retained-object issues in sync issue details.'
require_text \
    resources/resource_protocol/slate-settings.html \
    'Issue details' \
    'Slate settings page must expose protocol-neutral profile-sync issue details.'

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
run_test_with_features slate-broadwebd test-fixtures kubo_profile_sync_get_uses_transport_bytes_not_local_upload_metadata
run_test_with_features slate-broadwebd test-fixtures kubo_profile_sync_model_round_trips_state_without_canned_responses
run_test_with_features slate-broadwebd test-fixtures kubo_profile_sync_model_release_updates_retention_health
run_test slate-broadwebd local_fixture_root_health_reports_stale_latest_object_holders
run_test slate-broadwebd local_fixture_root_health_reports_offline_latest_object_holders
run_test slate-broadwebd local_fixture_retained_claim_requires_provider_bytes
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
run_test slate-profile-sync broadwebd_settings_compaction_retains_objects_before_strict_root_policy_check
run_test slate-profile-sync broadwebd_settings_sync_scheduler_compacts_with_selected_retention_provider_handles
run_test slate-profile-sync broadwebd_settings_sync_scheduler_compacts_with_stored_retention_provider_handles
run_test slate-profile-sync broadwebd_settings_sync_scheduler_stored_compaction_derives_active_key_from_sync_secret
run_test slate-profile-sync scheduler_stored_fixture_compaction_derives_active_key_from_sync_secret
run_test slate-profile-sync scheduler_protocol_stored_compaction_derives_active_key_from_sync_secret
run_test slate-profile-sync broadwebd_settings_sync_scheduler_runs_with_kubo_profile_sync_materialized_provider
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
