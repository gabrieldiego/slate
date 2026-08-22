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

cd "$repo_root"

reject_protocol_model_leak \
    crates/broadwebd/src/protocols/ipfs/kubo.rs \
    'InProcessBroadwebNetwork|InternalKubo(ProfileSyncModel|RpcFixture)|fetch_internal_kubo|internal_kubo_rpc_fixtures|register_internal_kubo|take_internal_kubo' \
    'Kubo protocol code must not call fixture models directly; route through transport executors or shims.'

reject_protocol_model_leak \
    crates/broadwebd/src/services/profile_sync.rs \
    'InProcessBroadwebNetwork|InternalKubo(ProfileSyncModel|RpcFixture)|fetch_internal_kubo|internal_kubo_rpc_fixtures|register_internal_kubo|take_internal_kubo' \
    'broadwebd profile-sync service code must not call Kubo fixture models directly; route through transport executors or shims.'

require_text \
    crates/broadwebd/src/protocols/ipfs/kubo_fixtures.rs \
    'impl IpfsKuboProfileSyncRpcExecutor for InternalKuboRpcTransportShim' \
    'Kubo fixture models must be reachable through the profile-sync RPC executor shim.'

require_text \
    crates/broadwebd/src/protocols/ipfs/kubo_fixtures.rs \
    'impl IpfsKuboHttpContentExecutor for InternalKuboRpcTransportShim' \
    'Kubo fixture models must be reachable through the HTTP content executor shim.'

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
    'id="profile-sync-handoff-create"' \
    'Slate settings page must expose profile-sync handoff creation.'
require_text \
    resources/resource_protocol/slate-settings.html \
    'id="profile-sync-handoff-import"' \
    'Slate settings page must expose profile-sync handoff import.'
require_text \
    resources/resource_protocol/slate-settings.html \
    'handoff/create' \
    'Slate settings page must call the handoff create route.'
require_text \
    resources/resource_protocol/slate-settings.html \
    'handoff/import' \
    'Slate settings page must call the handoff import route.'

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
run_test slate-broadwebd app_domain_metadata_syncs_through_profile_fixture
run_test slate-broadwebd ipfs_kubo_profile_sync_fixture_executor_rejects_non_fixture_endpoints
run_test slate-broadwebd kubo_profile_sync_fixture_reports_protocol_semantics_over_fixture_executor
run_test slate-broadwebd kubo_profile_sync_get_uses_transport_bytes_not_local_upload_metadata
run_test slate-broadwebd kubo_profile_sync_model_round_trips_state_without_canned_responses
run_test slate-profile-sync broadwebd_publisher_and_source_use_stateful_kubo_profile_sync_model
run_test slate-profile-sync broadwebd_stateful_kubo_model_shares_roots_and_objects_across_daemons
run_test slate-profile-sync broadwebd_publisher_publishes_signed_settings_tail_manifest_through_stateful_kubo_model
run_test slate-profile-sync broadwebd_source_ignores_stale_device_head_root_rollback
run_test slate-profile-sync broadwebd_source_records_missing_device_head_after_shared_root_sequence_seen
run_test slate-profile-sync broadwebd_source_ignores_stale_shared_root_candidate_rollback
run_test slate-profile-sync broadwebd_source_rejects_corrupt_shared_root_object_without_mutation
run_test slate-profile-sync broadwebd_source_rejects_wrong_key_id_shared_root_without_mutation
run_test slate-profile-sync broadwebd_source_rejects_bad_signature_shared_root_without_mutation
run_test slate-profile-sync broadwebd_source_rejects_malformed_app_payload_shared_root_without_mutation
run_test slate-profile-sync broadwebd_source_rejects_missing_tail_object_shared_root_without_mutation
run_test slate-profile-sync iroh_node
run_test slate-profile-sync broadwebd_settings_compaction_retains_objects_before_strict_root_policy_check
run_test slate-profile-sync broadwebd_settings_sync_scheduler_compacts_with_selected_retention_provider_handles
run_test slate-profile-sync broadwebd_settings_sync_scheduler_compacts_with_stored_retention_provider_handles
run_test slate-profile-sync broadwebd_settings_sync_scheduler_stored_compaction_derives_active_key_from_sync_secret
run_test slate-profile-sync scheduler_stored_fixture_compaction_derives_active_key_from_sync_secret
run_test slate-profile-sync scheduler_protocol_stored_compaction_derives_active_key_from_sync_secret
run_test slate-profile-sync broadwebd_settings_sync_scheduler_runs_with_kubo_profile_sync_materialized_provider
run_test slate-profile-sync scheduler_membership_fixture_stored_provider_derives_active_key_from_sync_secret
run_test slate-profile-sync local_settings_sync_current_cycle_publishes_existing_settings_without_preview_write
run_test slate-profile-sync local_settings_sync_preview_cycle_publishes_and_retains_without_loopback
run_test slate-profile-sync local_settings_sync_two_device_preview_cycle_applies_on_receiver_without_loopback
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
