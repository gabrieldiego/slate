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

cd "$repo_root"

run_test slate-apps sync_domains
run_test slate-storage rail_app_sync_domains_match_seeded_storage_domains
run_test slate-storage slate_sync_secret_derives_domain_separated_profile_material
run_test slate-storage slate_sync_secret_derives_stable_profile_device_signers
run_test slate-storage slate_sync_secret_export_import_can_require_profile_match
run_test slate-storage slate_sync_secret_export_round_trips_with_redacted_debug
run_test slate-storage profile_sync_device_enrollment_request_round_trips_without_secret_material
run_test slate-storage profile_sync_enrollment_bundle_can_be_derived_from_device_request
run_test slate-storage profile_sync_enrollment_bundle_can_be_derived_from_sync_secret
run_test slate-storage profile_sync_local_activation_records_non_secret_metadata
run_test slate-storage profile_sync_secret_activation_trusts_derived_local_signer_without_storing_secret
run_test slate-storage profile_sync_local_readiness_reports_provider_gap
run_test slate-storage profile_sync_local_readiness_reports_authorized_retention_provider
run_test slate-storage storage_provider_writes_metadata_sync_change_without_secrets_or_local_state
run_test slate-storage tests::app_sync_domain_watcher_polls_and_acknowledges_batches
run_test slate-storage typed_app_sync_domain_poll_decodes_payloads_and_records_cursor
run_test slate-broadwebd app_domain_metadata_syncs_through_profile_fixture
run_test slate-broadwebd kubo_profile_sync_fixture_reports_protocol_semantics_over_internal_transport
run_test slate-broadwebd kubo_profile_sync_model_round_trips_state_without_canned_responses
run_test slate-profile-sync scheduler_membership_fixture_stored_provider_derives_active_key_from_sync_secret
run_test slate-profile-sync local_settings_sync_current_cycle_publishes_existing_settings_without_preview_write
run_test slate-profile-sync local_settings_sync_preview_cycle_publishes_and_retains_without_loopback
run_test slate-profile-sync local_settings_sync_two_device_preview_cycle_applies_on_receiver_without_loopback

case "$check_chrome" in
    1 | true | yes)
        run_test slate-chrome watcher_applies_new_sync_revisions_incrementally
        ;;
    *)
        printf '\n==> slate-chrome :: watcher_applies_new_sync_revisions_incrementally\n'
        printf 'skipped by default: compiling slate-chrome currently pulls Servo script bindings and exceeds the low-memory profile. Set SLATE_PROFILE_SYNC_CHECK_CHROME=1 with a larger SLATE_BUILD_MEMORY_LIMIT_MB to run it.\n'
        ;;
esac
