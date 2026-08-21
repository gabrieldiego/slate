#![forbid(unsafe_code)]

use slate_broadwebd::{
    LocalProfileSyncFixture, PluginRegistry, ProfileSyncObjectRequest, ProfileSyncPutObjectRequest,
    ProfileSyncRequest, ProfileSyncResponse, ProfileSyncRootRequest, ProfileSyncRootUpdate,
    ResourceBudget,
};
use slate_storage::{
    DEFAULT_DATABASE_FILE_NAME, DEFAULT_PROFILE_ID, DEFAULT_PROFILE_SYNC_MEMBERSHIP_EPOCH,
    EncryptedSyncObject, IncomingSyncSettingText, PROFILE_SYNC_CONTENT_KEY_BYTES,
    PROFILE_SYNC_MANIFEST_OBJECT_KIND, PROFILE_SYNC_MANIFEST_SCHEMA_VERSION,
    PROFILE_SYNC_SETTING_CHANGE_OBJECT_KIND, PROFILE_SYNC_SETTINGS_SNAPSHOT_OBJECT_KIND,
    PROFILE_SYNC_SETTINGS_SNAPSHOT_SCHEMA_VERSION, ProfileSyncContentKey,
    ProfileSyncDeviceFrontier, ProfileSyncDevicePublicKey, ProfileSyncDeviceSigner,
    ProfileSyncManifest, ProfileSyncRetentionPolicy, ProfileSyncSettingsSnapshot,
    SYNC_DOMAIN_SETTINGS, SlateProfileDatabase, SyncChangeRecord, SyncRevisionRecord,
    SyncSnapshotRegistration, VerifiedProfileSyncSettingsSnapshot,
    VerifiedProfileSyncSettingsTailChange, open_signed_profile_sync_manifest,
    open_signed_profile_sync_settings_snapshot, open_signed_sync_setting_text,
};
use std::path::PathBuf;
use std::time::{SystemTime, UNIX_EPOCH};

const FIXTURE_CONTENT_KEY_ID: &str = "content-key-epoch-1";
const SETTINGS_ROOT_ID: &str = "settings/latest";

#[test]
fn two_local_slate_settings_databases_sync_through_profile_fixture() {
    let fixture = LocalProfileSyncFixture::new();
    let mut device_a_broadweb = PluginRegistry::new();
    let mut device_b_broadweb = PluginRegistry::new();
    let budget = ResourceBudget::default();

    device_a_broadweb.register_service(fixture.service_for_device("device-a"));
    device_b_broadweb.register_service(fixture.service_for_device("device-b"));

    let device_a_root = test_dir("device-a");
    let device_b_root = test_dir("device-b");
    let device_a_db = SlateProfileDatabase::open_resolved_with_device_id(
        device_a_root.join(DEFAULT_DATABASE_FILE_NAME),
        "device-a",
    )
    .expect("open device a slate-settings.db");
    let device_b_db = SlateProfileDatabase::open_resolved_with_device_id(
        device_b_root.join(DEFAULT_DATABASE_FILE_NAME),
        "device-b",
    )
    .expect("open device b slate-settings.db");
    let device_a_signer =
        ProfileSyncDeviceSigner::generate("device-a").expect("create device a signing key");
    let trusted_device_a_key = device_a_signer
        .public_key()
        .expect("read device a public key");

    let local_change = device_a_db
        .set_sync_setting_text(DEFAULT_PROFILE_ID, SYNC_DOMAIN_SETTINGS, "ui.theme", "teal")
        .expect("device a writes local setting");
    assert_eq!(local_change.device_id, "device-a");

    let content_key = fixture_content_key();
    let change_bytes = sign_encrypted_setting_change(&local_change, &content_key, &device_a_signer);
    assert!(
        !std::str::from_utf8(change_bytes.as_slice())
            .expect("fixture object is JSON envelope")
            .contains("teal")
    );
    let change_object_id = put_object(
        &device_a_broadweb,
        DEFAULT_PROFILE_ID,
        change_bytes,
        &budget,
    );
    let manifest_bytes = sign_encrypted_manifest(
        SETTINGS_ROOT_ID,
        change_object_id.as_str(),
        &local_change,
        &content_key,
        &device_a_signer,
    );
    assert!(
        !std::str::from_utf8(manifest_bytes.as_slice())
            .expect("fixture manifest object is JSON envelope")
            .contains(change_object_id.as_str())
    );
    let manifest_object_id = put_and_publish_object(
        &device_a_broadweb,
        DEFAULT_PROFILE_ID,
        SETTINGS_ROOT_ID,
        manifest_bytes,
        &budget,
    );

    let fetched = fetch_published_object(
        &device_b_broadweb,
        DEFAULT_PROFILE_ID,
        SETTINGS_ROOT_ID,
        &budget,
    );
    assert_eq!(fetched.object_id, manifest_object_id);

    let manifest = verify_and_decrypt_manifest(
        fetched.bytes.as_slice(),
        &content_key,
        &trusted_device_a_key,
    );
    assert_eq!(manifest.profile, DEFAULT_PROFILE_ID);
    assert_eq!(manifest.root_id, SETTINGS_ROOT_ID);
    assert_eq!(
        manifest.schema_version,
        PROFILE_SYNC_MANIFEST_SCHEMA_VERSION
    );
    assert_eq!(
        manifest.membership_epoch,
        DEFAULT_PROFILE_SYNC_MEMBERSHIP_EPOCH
    );
    assert_eq!(
        manifest.retention_policy,
        ProfileSyncRetentionPolicy::default()
    );
    assert_eq!(
        manifest.tail_change_object_ids,
        vec![change_object_id.clone()]
    );
    assert_eq!(manifest.current_snapshot_object_id, None);

    device_b_db
        .set_profile_sync_root(
            DEFAULT_PROFILE_ID,
            SETTINGS_ROOT_ID,
            manifest_object_id.as_str(),
        )
        .expect("device b stores verified manifest root");
    let stored_root = device_b_db
        .profile_sync_root(DEFAULT_PROFILE_ID, SETTINGS_ROOT_ID)
        .expect("read stored sync root")
        .expect("stored sync root");
    assert_eq!(stored_root.object_id, manifest_object_id);

    let change_object = fetch_object(
        &device_b_broadweb,
        DEFAULT_PROFILE_ID,
        manifest.tail_change_object_ids[0].as_str(),
        &budget,
    );
    let incoming = verify_and_decrypt_setting_change(
        change_object.bytes.as_slice(),
        &content_key,
        &trusted_device_a_key,
    );
    let applied = device_b_db
        .apply_sync_setting_text(&incoming)
        .expect("device b applies incoming setting");

    assert_eq!(applied.device_id, "device-a");
    assert_eq!(
        device_b_db
            .get_setting_text("ui.theme")
            .expect("read device b setting")
            .as_deref(),
        Some("teal")
    );

    let value = device_b_db
        .get_sync_setting_text(DEFAULT_PROFILE_ID, SYNC_DOMAIN_SETTINGS, "ui.theme")
        .expect("read device b sync value")
        .expect("device b sync value exists");
    let duplicate = device_b_db
        .apply_sync_setting_text(&incoming_setting_from_change(&local_change))
        .expect("duplicate fixture replay is idempotent");
    assert_eq!(duplicate.id, applied.id);
    assert_eq!(
        device_b_db
            .sync_revisions_after(DEFAULT_PROFILE_ID, value.revision)
            .expect("read revisions after duplicate"),
        Vec::<SyncRevisionRecord>::new()
    );

    let device_b_known_devices = device_b_db
        .sync_devices(DEFAULT_PROFILE_ID)
        .expect("read device b known devices");
    assert!(
        device_b_known_devices
            .iter()
            .any(|device| device.device_id == "device-a")
    );
    assert!(
        device_b_known_devices
            .iter()
            .any(|device| device.device_id == "device-b")
    );

    let _ = std::fs::remove_dir_all(device_a_root);
    let _ = std::fs::remove_dir_all(device_b_root);
}

#[test]
fn two_local_devices_keep_newer_setting_when_fixture_replays_stale_change() {
    let fixture = LocalProfileSyncFixture::new();
    let mut device_a_broadweb = PluginRegistry::new();
    let mut device_b_broadweb = PluginRegistry::new();
    let budget = ResourceBudget::default();

    device_a_broadweb.register_service(fixture.service_for_device("device-a"));
    device_b_broadweb.register_service(fixture.service_for_device("device-b"));

    let device_a_root = test_dir("stale-device-a");
    let device_b_root = test_dir("stale-device-b");
    let device_a_db = SlateProfileDatabase::open_resolved_with_device_id(
        device_a_root.join(DEFAULT_DATABASE_FILE_NAME),
        "device-a",
    )
    .expect("open stale fixture device a slate-settings.db");
    let device_b_db = SlateProfileDatabase::open_resolved_with_device_id(
        device_b_root.join(DEFAULT_DATABASE_FILE_NAME),
        "device-b",
    )
    .expect("open stale fixture device b slate-settings.db");
    let device_a_signer =
        ProfileSyncDeviceSigner::generate("device-a").expect("create device a signing key");
    let trusted_device_a_key = device_a_signer
        .public_key()
        .expect("read device a public key");

    let stale_change = device_a_db
        .set_sync_setting_text(
            DEFAULT_PROFILE_ID,
            SYNC_DOMAIN_SETTINGS,
            "ui.theme",
            "stale",
        )
        .expect("device a writes stale setting");
    let _ = device_b_db
        .set_sync_setting_text(DEFAULT_PROFILE_ID, SYNC_DOMAIN_SETTINGS, "ui.theme", "seed")
        .expect("device b writes seed setting");
    let winning_local_change = device_b_db
        .set_sync_setting_text(
            DEFAULT_PROFILE_ID,
            SYNC_DOMAIN_SETTINGS,
            "ui.theme",
            "local",
        )
        .expect("device b writes newer local setting");
    assert!(winning_local_change.logical_clock > stale_change.logical_clock);
    let winning_value = device_b_db
        .get_sync_setting_text(DEFAULT_PROFILE_ID, SYNC_DOMAIN_SETTINGS, "ui.theme")
        .expect("read winning device b sync value")
        .expect("winning device b sync value exists");

    let content_key = fixture_content_key();
    let change_bytes = sign_encrypted_setting_change(&stale_change, &content_key, &device_a_signer);
    let change_object_id = put_object(
        &device_a_broadweb,
        DEFAULT_PROFILE_ID,
        change_bytes,
        &budget,
    );
    let manifest_bytes = sign_encrypted_manifest(
        SETTINGS_ROOT_ID,
        change_object_id.as_str(),
        &stale_change,
        &content_key,
        &device_a_signer,
    );
    let manifest_object_id = put_and_publish_object(
        &device_a_broadweb,
        DEFAULT_PROFILE_ID,
        SETTINGS_ROOT_ID,
        manifest_bytes,
        &budget,
    );

    let fetched = fetch_published_object(
        &device_b_broadweb,
        DEFAULT_PROFILE_ID,
        SETTINGS_ROOT_ID,
        &budget,
    );
    assert_eq!(fetched.object_id, manifest_object_id);
    let manifest = verify_and_decrypt_manifest(
        fetched.bytes.as_slice(),
        &content_key,
        &trusted_device_a_key,
    );
    let change_object = fetch_object(
        &device_b_broadweb,
        DEFAULT_PROFILE_ID,
        manifest.tail_change_object_ids[0].as_str(),
        &budget,
    );
    let incoming = verify_and_decrypt_setting_change(
        change_object.bytes.as_slice(),
        &content_key,
        &trusted_device_a_key,
    );
    let replayed = device_b_db
        .apply_sync_setting_text(&incoming)
        .expect("device b records stale incoming setting");

    assert_eq!(replayed.payload, "stale");
    assert_eq!(replayed.applied_at, None);
    assert_eq!(
        device_b_db
            .get_setting_text("ui.theme")
            .expect("read device b legacy setting")
            .as_deref(),
        Some("local")
    );
    assert_eq!(
        device_b_db
            .get_sync_setting_text(DEFAULT_PROFILE_ID, SYNC_DOMAIN_SETTINGS, "ui.theme")
            .expect("read device b sync value after stale replay")
            .expect("device b sync value remains present"),
        winning_value
    );
    assert_eq!(
        device_b_db
            .sync_revisions_after(DEFAULT_PROFILE_ID, winning_value.revision)
            .expect("read revisions after stale replay"),
        Vec::<SyncRevisionRecord>::new()
    );

    let duplicate = device_b_db
        .apply_sync_setting_text(&incoming)
        .expect("duplicate stale fixture replay is idempotent");
    assert_eq!(duplicate.id, replayed.id);

    let _ = std::fs::remove_dir_all(device_a_root);
    let _ = std::fs::remove_dir_all(device_b_root);
}

#[test]
fn two_local_devices_transfer_compacted_settings_snapshot_through_profile_fixture() {
    let fixture = LocalProfileSyncFixture::new();
    let mut device_a_broadweb = PluginRegistry::new();
    let mut device_b_broadweb = PluginRegistry::new();
    let budget = ResourceBudget::default();

    device_a_broadweb.register_service(fixture.service_for_device("snapshot-device-a"));
    device_b_broadweb.register_service(fixture.service_for_device("snapshot-device-b"));

    let device_a_root = test_dir("snapshot-device-a");
    let device_b_root = test_dir("snapshot-device-b");
    let device_a_db = SlateProfileDatabase::open_resolved_with_device_id(
        device_a_root.join(DEFAULT_DATABASE_FILE_NAME),
        "snapshot-device-a",
    )
    .expect("open snapshot fixture device a slate-settings.db");
    let device_b_db = SlateProfileDatabase::open_resolved_with_device_id(
        device_b_root.join(DEFAULT_DATABASE_FILE_NAME),
        "snapshot-device-b",
    )
    .expect("open snapshot fixture device b slate-settings.db");
    let device_a_signer = ProfileSyncDeviceSigner::generate("snapshot-device-a")
        .expect("create snapshot device a signing key");
    let trusted_device_a_key = device_a_signer
        .public_key()
        .expect("read snapshot device a public key");

    let latest_change = device_a_db
        .set_sync_setting_text(DEFAULT_PROFILE_ID, SYNC_DOMAIN_SETTINGS, "ui.theme", "teal")
        .expect("snapshot fixture device a writes local setting");
    let covers_revision = device_a_db
        .latest_sync_revision(DEFAULT_PROFILE_ID)
        .expect("read snapshot fixture latest revision");
    let included_domains = vec![SYNC_DOMAIN_SETTINGS.to_string()];
    let snapshot = device_a_db
        .settings_sync_snapshot_payload(DEFAULT_PROFILE_ID, covers_revision, &included_domains)
        .expect("build settings snapshot payload");

    assert_eq!(snapshot.profile, DEFAULT_PROFILE_ID);
    assert_eq!(
        snapshot.schema_version,
        PROFILE_SYNC_SETTINGS_SNAPSHOT_SCHEMA_VERSION
    );
    assert_eq!(snapshot.covers_revision, covers_revision);
    assert_eq!(snapshot.included_domains, included_domains);
    let snapshot_value = snapshot
        .values
        .iter()
        .find(|value| value.domain == SYNC_DOMAIN_SETTINGS && value.key == "ui.theme")
        .expect("snapshot contains theme value");
    assert_eq!(snapshot_value.value, "teal");

    let content_key = fixture_content_key();
    let snapshot_bytes =
        sign_encrypted_settings_snapshot(&snapshot, &content_key, &device_a_signer);
    assert!(
        !std::str::from_utf8(snapshot_bytes.as_slice())
            .expect("fixture snapshot object is JSON envelope")
            .contains("teal")
    );
    let snapshot_object_id = put_object(
        &device_a_broadweb,
        DEFAULT_PROFILE_ID,
        snapshot_bytes,
        &budget,
    );
    let snapshot_id = format!("settings-snapshot-r{}", snapshot.covers_revision);
    device_a_db
        .record_sync_snapshot(&SyncSnapshotRegistration {
            profile: DEFAULT_PROFILE_ID.to_string(),
            snapshot_id: snapshot_id.clone(),
            backend_object_id: Some(snapshot_object_id.clone()),
            covers_revision: snapshot.covers_revision,
            included_domains: snapshot.included_domains.clone(),
        })
        .expect("device a records published snapshot");

    let manifest_bytes = sign_encrypted_manifest_with_snapshot(
        SETTINGS_ROOT_ID,
        snapshot_object_id.as_str(),
        &snapshot,
        &latest_change,
        &content_key,
        &device_a_signer,
    );
    let manifest_object_id = put_and_publish_object(
        &device_a_broadweb,
        DEFAULT_PROFILE_ID,
        SETTINGS_ROOT_ID,
        manifest_bytes,
        &budget,
    );

    let fetched_manifest = fetch_published_object(
        &device_b_broadweb,
        DEFAULT_PROFILE_ID,
        SETTINGS_ROOT_ID,
        &budget,
    );
    assert_eq!(fetched_manifest.object_id, manifest_object_id);
    let manifest = verify_and_decrypt_manifest(
        fetched_manifest.bytes.as_slice(),
        &content_key,
        &trusted_device_a_key,
    );
    assert_eq!(
        manifest.current_snapshot_object_id,
        Some(snapshot_object_id.clone())
    );
    assert_eq!(manifest.tail_change_object_ids, Vec::<String>::new());
    assert_eq!(manifest.included_domains, included_domains);
    assert_eq!(manifest.device_frontiers.len(), 1);
    assert_eq!(
        manifest.device_frontiers[0].latest_sequence,
        latest_change.device_sequence
    );
    assert_eq!(manifest.device_frontiers[0].latest_change_object_id, None);

    device_b_db
        .set_profile_sync_root(
            DEFAULT_PROFILE_ID,
            SETTINGS_ROOT_ID,
            manifest_object_id.as_str(),
        )
        .expect("device b stores verified snapshot manifest root");

    let fetched_snapshot = fetch_object(
        &device_b_broadweb,
        DEFAULT_PROFILE_ID,
        snapshot_object_id.as_str(),
        &budget,
    );
    let verified_snapshot = verify_and_decrypt_settings_snapshot(
        fetched_snapshot.bytes.as_slice(),
        &content_key,
        &trusted_device_a_key,
    );
    assert_eq!(verified_snapshot, snapshot);

    let applied_snapshot_changes = device_b_db
        .apply_settings_snapshot(&verified_snapshot)
        .expect("device b applies verified incoming snapshot");
    let applied_theme = applied_snapshot_changes
        .iter()
        .find(|change| change.domain == SYNC_DOMAIN_SETTINGS && change.entity_key == "ui.theme")
        .expect("snapshot application includes theme change");
    assert_eq!(applied_theme.payload, "teal");
    assert!(applied_theme.applied_at.is_some());
    assert_eq!(
        device_b_db
            .get_setting_text("ui.theme")
            .expect("read applied device b legacy setting")
            .as_deref(),
        Some("teal")
    );
    let snapshot_value = device_b_db
        .get_sync_setting_text(DEFAULT_PROFILE_ID, SYNC_DOMAIN_SETTINGS, "ui.theme")
        .expect("read applied device b snapshot setting")
        .expect("device b snapshot setting exists");
    assert_eq!(snapshot_value.value, "teal");

    device_b_db
        .record_sync_snapshot(&SyncSnapshotRegistration {
            profile: DEFAULT_PROFILE_ID.to_string(),
            snapshot_id: snapshot_id.clone(),
            backend_object_id: Some(snapshot_object_id.clone()),
            covers_revision: verified_snapshot.covers_revision,
            included_domains: verified_snapshot.included_domains.clone(),
        })
        .expect("device b records verified incoming snapshot");
    let latest_snapshot = device_b_db
        .latest_sync_snapshot(DEFAULT_PROFILE_ID)
        .expect("read device b latest snapshot")
        .expect("device b latest snapshot exists");
    assert_eq!(latest_snapshot.snapshot_id, snapshot_id);
    assert_eq!(
        latest_snapshot.backend_object_id.as_deref(),
        Some(snapshot_object_id.as_str())
    );
    let duplicate_snapshot_changes = device_b_db
        .apply_settings_snapshot(&verified_snapshot)
        .expect("duplicate snapshot application is idempotent");
    assert_eq!(
        device_b_db
            .sync_revisions_after(DEFAULT_PROFILE_ID, snapshot_value.revision)
            .expect("read revisions after duplicate snapshot"),
        Vec::<SyncRevisionRecord>::new()
    );
    let duplicate_theme = duplicate_snapshot_changes
        .iter()
        .find(|change| change.domain == SYNC_DOMAIN_SETTINGS && change.entity_key == "ui.theme")
        .expect("duplicate snapshot includes theme change");
    assert_eq!(duplicate_theme.id, applied_theme.id);
    assert_eq!(duplicate_theme.applied_at, applied_theme.applied_at);

    let _ = std::fs::remove_dir_all(device_a_root);
    let _ = std::fs::remove_dir_all(device_b_root);
}

#[test]
fn two_local_devices_apply_snapshot_then_manifest_tail_changes() {
    let fixture = LocalProfileSyncFixture::new();
    let mut device_a_broadweb = PluginRegistry::new();
    let mut device_b_broadweb = PluginRegistry::new();
    let budget = ResourceBudget::default();

    device_a_broadweb.register_service(fixture.service_for_device("tail-device-a"));
    device_b_broadweb.register_service(fixture.service_for_device("tail-device-b"));

    let device_a_root = test_dir("tail-device-a");
    let device_b_root = test_dir("tail-device-b");
    let device_a_db = SlateProfileDatabase::open_resolved_with_device_id(
        device_a_root.join(DEFAULT_DATABASE_FILE_NAME),
        "tail-device-a",
    )
    .expect("open tail fixture device a slate-settings.db");
    let device_b_db = SlateProfileDatabase::open_resolved_with_device_id(
        device_b_root.join(DEFAULT_DATABASE_FILE_NAME),
        "tail-device-b",
    )
    .expect("open tail fixture device b slate-settings.db");
    let device_a_signer =
        ProfileSyncDeviceSigner::generate("tail-device-a").expect("create tail device a key");
    let trusted_device_a_key = device_a_signer
        .public_key()
        .expect("read tail device a public key");
    let content_key = fixture_content_key();

    device_a_db
        .set_sync_setting_text(DEFAULT_PROFILE_ID, SYNC_DOMAIN_SETTINGS, "ui.theme", "teal")
        .expect("tail fixture device a writes snapshot setting");
    let snapshot_revision = device_a_db
        .latest_sync_revision(DEFAULT_PROFILE_ID)
        .expect("read tail fixture snapshot revision");
    let snapshot = device_a_db
        .settings_sync_snapshot_payload(
            DEFAULT_PROFILE_ID,
            snapshot_revision,
            &[SYNC_DOMAIN_SETTINGS.to_string()],
        )
        .expect("build tail fixture snapshot payload");
    let tail_change = device_a_db
        .set_sync_setting_text(
            DEFAULT_PROFILE_ID,
            SYNC_DOMAIN_SETTINGS,
            "ui.theme",
            "slate",
        )
        .expect("tail fixture device a writes retained tail setting");

    let snapshot_object_id = put_object(
        &device_a_broadweb,
        DEFAULT_PROFILE_ID,
        sign_encrypted_settings_snapshot(&snapshot, &content_key, &device_a_signer),
        &budget,
    );
    let tail_change_object_id = put_object(
        &device_a_broadweb,
        DEFAULT_PROFILE_ID,
        sign_encrypted_setting_change(&tail_change, &content_key, &device_a_signer),
        &budget,
    );
    let manifest_object_id = put_and_publish_object(
        &device_a_broadweb,
        DEFAULT_PROFILE_ID,
        SETTINGS_ROOT_ID,
        sign_encrypted_manifest_with_snapshot_tail(
            SETTINGS_ROOT_ID,
            snapshot_object_id.as_str(),
            tail_change_object_id.as_str(),
            &snapshot,
            &tail_change,
            &content_key,
            &device_a_signer,
        ),
        &budget,
    );

    let fetched_manifest = fetch_published_object(
        &device_b_broadweb,
        DEFAULT_PROFILE_ID,
        SETTINGS_ROOT_ID,
        &budget,
    );
    assert_eq!(fetched_manifest.object_id, manifest_object_id);
    let manifest = verify_and_decrypt_manifest(
        fetched_manifest.bytes.as_slice(),
        &content_key,
        &trusted_device_a_key,
    );
    assert_eq!(
        manifest.current_snapshot_object_id,
        Some(snapshot_object_id.clone())
    );
    assert_eq!(
        manifest.tail_change_object_ids,
        vec![tail_change_object_id.clone()]
    );
    assert_eq!(manifest.device_frontiers.len(), 1);
    assert_eq!(
        manifest.device_frontiers[0]
            .latest_change_object_id
            .as_deref(),
        Some(tail_change_object_id.as_str())
    );

    let fetched_snapshot = fetch_object(
        &device_b_broadweb,
        DEFAULT_PROFILE_ID,
        snapshot_object_id.as_str(),
        &budget,
    );
    let verified_snapshot = verify_and_decrypt_settings_snapshot(
        fetched_snapshot.bytes.as_slice(),
        &content_key,
        &trusted_device_a_key,
    );
    let fetched_tail = fetch_object(
        &device_b_broadweb,
        DEFAULT_PROFILE_ID,
        tail_change_object_id.as_str(),
        &budget,
    );
    let incoming_tail = verify_and_decrypt_setting_change(
        fetched_tail.bytes.as_slice(),
        &content_key,
        &trusted_device_a_key,
    );
    let applied_manifest = device_b_db
        .apply_verified_settings_manifest(
            manifest_object_id.as_str(),
            &manifest,
            Some(&VerifiedProfileSyncSettingsSnapshot {
                object_id: snapshot_object_id.clone(),
                snapshot: verified_snapshot,
            }),
            &[VerifiedProfileSyncSettingsTailChange {
                object_id: tail_change_object_id.clone(),
                change: incoming_tail,
            }],
        )
        .expect("device b applies verified snapshot and manifest tail");
    assert_eq!(applied_manifest.manifest_object_id, manifest_object_id);
    assert_eq!(
        applied_manifest
            .snapshot
            .as_ref()
            .and_then(|snapshot| snapshot.backend_object_id.as_deref()),
        Some(snapshot_object_id.as_str())
    );
    let applied_tail = applied_manifest
        .tail_changes
        .iter()
        .find(|change| change.entity_key == "ui.theme")
        .expect("manifest application includes theme tail");
    assert_eq!(applied_tail.payload, "slate");
    assert!(applied_tail.applied_at.is_some());
    assert_eq!(
        device_b_db
            .get_setting_text("ui.theme")
            .expect("read device b theme after tail")
            .as_deref(),
        Some("slate")
    );
    assert_eq!(
        device_b_db
            .profile_sync_root(DEFAULT_PROFILE_ID, SETTINGS_ROOT_ID)
            .expect("read device b manifest root")
            .expect("device b manifest root exists")
            .object_id,
        manifest_object_id
    );

    let _ = std::fs::remove_dir_all(device_a_root);
    let _ = std::fs::remove_dir_all(device_b_root);
}

fn incoming_setting_from_change(change: &SyncChangeRecord) -> IncomingSyncSettingText {
    assert_eq!(change.operation, "set_text");
    IncomingSyncSettingText::new(
        change.profile.clone(),
        change.domain.clone(),
        change.entity_key.clone(),
        change.payload.clone(),
        change.device_id.clone(),
        change.device_sequence,
        change.logical_clock,
    )
}

fn sign_encrypted_setting_change(
    change: &SyncChangeRecord,
    content_key: &ProfileSyncContentKey,
    signer: &ProfileSyncDeviceSigner,
) -> Vec<u8> {
    let incoming = incoming_setting_from_change(change);
    let payload = serde_json::to_vec(&incoming).expect("encode fixture sync payload");
    let encrypted_object = EncryptedSyncObject::seal(
        incoming.profile.as_str(),
        incoming.domain.as_str(),
        PROFILE_SYNC_SETTING_CHANGE_OBJECT_KIND,
        FIXTURE_CONTENT_KEY_ID,
        payload.as_slice(),
        content_key,
    )
    .expect("encrypt fixture sync object");
    let encrypted_bytes = encrypted_object
        .to_bytes()
        .expect("encode fixture encrypted sync object");
    signer
        .sign(encrypted_bytes.as_slice())
        .expect("sign fixture encrypted sync object")
        .to_bytes()
        .expect("encode fixture signed sync object")
}

fn sign_encrypted_settings_snapshot(
    snapshot: &ProfileSyncSettingsSnapshot,
    content_key: &ProfileSyncContentKey,
    signer: &ProfileSyncDeviceSigner,
) -> Vec<u8> {
    let payload = serde_json::to_vec(snapshot).expect("encode fixture snapshot payload");
    let encrypted_object = EncryptedSyncObject::seal(
        snapshot.profile.as_str(),
        SYNC_DOMAIN_SETTINGS,
        PROFILE_SYNC_SETTINGS_SNAPSHOT_OBJECT_KIND,
        FIXTURE_CONTENT_KEY_ID,
        payload.as_slice(),
        content_key,
    )
    .expect("encrypt fixture snapshot object");
    let encrypted_bytes = encrypted_object
        .to_bytes()
        .expect("encode fixture encrypted snapshot object");
    signer
        .sign(encrypted_bytes.as_slice())
        .expect("sign fixture encrypted snapshot object")
        .to_bytes()
        .expect("encode fixture signed snapshot object")
}

fn sign_encrypted_manifest(
    root_id: &str,
    change_object_id: &str,
    change: &SyncChangeRecord,
    content_key: &ProfileSyncContentKey,
    signer: &ProfileSyncDeviceSigner,
) -> Vec<u8> {
    let manifest = ProfileSyncManifest {
        profile: change.profile.clone(),
        root_id: root_id.to_string(),
        schema_version: PROFILE_SYNC_MANIFEST_SCHEMA_VERSION,
        membership_epoch: DEFAULT_PROFILE_SYNC_MEMBERSHIP_EPOCH,
        current_snapshot_object_id: None,
        tail_change_object_ids: vec![change_object_id.to_string()],
        included_domains: vec![change.domain.clone()],
        device_frontiers: vec![ProfileSyncDeviceFrontier {
            device_id: change.device_id.clone(),
            latest_sequence: change.device_sequence,
            latest_change_object_id: Some(change_object_id.to_string()),
        }],
        retention_policy: ProfileSyncRetentionPolicy::default(),
        created_at: change.created_at,
    };
    sign_encrypted_manifest_payload(&manifest, content_key, signer)
}

fn sign_encrypted_manifest_with_snapshot(
    root_id: &str,
    snapshot_object_id: &str,
    snapshot: &ProfileSyncSettingsSnapshot,
    latest_change: &SyncChangeRecord,
    content_key: &ProfileSyncContentKey,
    signer: &ProfileSyncDeviceSigner,
) -> Vec<u8> {
    let manifest = ProfileSyncManifest {
        profile: snapshot.profile.clone(),
        root_id: root_id.to_string(),
        schema_version: PROFILE_SYNC_MANIFEST_SCHEMA_VERSION,
        membership_epoch: DEFAULT_PROFILE_SYNC_MEMBERSHIP_EPOCH,
        current_snapshot_object_id: Some(snapshot_object_id.to_string()),
        tail_change_object_ids: Vec::new(),
        included_domains: snapshot.included_domains.clone(),
        device_frontiers: vec![ProfileSyncDeviceFrontier {
            device_id: latest_change.device_id.clone(),
            latest_sequence: latest_change.device_sequence,
            latest_change_object_id: None,
        }],
        retention_policy: ProfileSyncRetentionPolicy::default(),
        created_at: snapshot.created_at,
    };
    sign_encrypted_manifest_payload(&manifest, content_key, signer)
}

fn sign_encrypted_manifest_with_snapshot_tail(
    root_id: &str,
    snapshot_object_id: &str,
    tail_change_object_id: &str,
    snapshot: &ProfileSyncSettingsSnapshot,
    tail_change: &SyncChangeRecord,
    content_key: &ProfileSyncContentKey,
    signer: &ProfileSyncDeviceSigner,
) -> Vec<u8> {
    let mut included_domains = snapshot.included_domains.clone();
    included_domains.push(tail_change.domain.clone());
    included_domains.sort();
    included_domains.dedup();
    let manifest = ProfileSyncManifest {
        profile: snapshot.profile.clone(),
        root_id: root_id.to_string(),
        schema_version: PROFILE_SYNC_MANIFEST_SCHEMA_VERSION,
        membership_epoch: DEFAULT_PROFILE_SYNC_MEMBERSHIP_EPOCH,
        current_snapshot_object_id: Some(snapshot_object_id.to_string()),
        tail_change_object_ids: vec![tail_change_object_id.to_string()],
        included_domains,
        device_frontiers: vec![ProfileSyncDeviceFrontier {
            device_id: tail_change.device_id.clone(),
            latest_sequence: tail_change.device_sequence,
            latest_change_object_id: Some(tail_change_object_id.to_string()),
        }],
        retention_policy: ProfileSyncRetentionPolicy::default(),
        created_at: tail_change.created_at,
    };
    sign_encrypted_manifest_payload(&manifest, content_key, signer)
}

fn sign_encrypted_manifest_payload(
    manifest: &ProfileSyncManifest,
    content_key: &ProfileSyncContentKey,
    signer: &ProfileSyncDeviceSigner,
) -> Vec<u8> {
    let payload = serde_json::to_vec(&manifest).expect("encode fixture manifest payload");
    let encrypted_object = EncryptedSyncObject::seal(
        manifest.profile.as_str(),
        SYNC_DOMAIN_SETTINGS,
        PROFILE_SYNC_MANIFEST_OBJECT_KIND,
        FIXTURE_CONTENT_KEY_ID,
        payload.as_slice(),
        content_key,
    )
    .expect("encrypt fixture manifest");
    let encrypted_bytes = encrypted_object
        .to_bytes()
        .expect("encode fixture encrypted manifest");
    signer
        .sign(encrypted_bytes.as_slice())
        .expect("sign fixture encrypted manifest")
        .to_bytes()
        .expect("encode fixture signed manifest")
}

fn verify_and_decrypt_setting_change(
    bytes: &[u8],
    content_key: &ProfileSyncContentKey,
    public_key: &ProfileSyncDevicePublicKey,
) -> IncomingSyncSettingText {
    open_signed_sync_setting_text(
        bytes,
        content_key,
        public_key,
        DEFAULT_PROFILE_ID,
        SYNC_DOMAIN_SETTINGS,
        FIXTURE_CONTENT_KEY_ID,
    )
    .expect("verify and decrypt fixture sync payload")
}

fn verify_and_decrypt_settings_snapshot(
    bytes: &[u8],
    content_key: &ProfileSyncContentKey,
    public_key: &ProfileSyncDevicePublicKey,
) -> ProfileSyncSettingsSnapshot {
    open_signed_profile_sync_settings_snapshot(
        bytes,
        content_key,
        public_key,
        DEFAULT_PROFILE_ID,
        FIXTURE_CONTENT_KEY_ID,
    )
    .expect("verify and decrypt fixture snapshot payload")
}

fn verify_and_decrypt_manifest(
    bytes: &[u8],
    content_key: &ProfileSyncContentKey,
    public_key: &ProfileSyncDevicePublicKey,
) -> ProfileSyncManifest {
    open_signed_profile_sync_manifest(
        bytes,
        content_key,
        public_key,
        DEFAULT_PROFILE_ID,
        FIXTURE_CONTENT_KEY_ID,
    )
    .expect("verify and decrypt fixture manifest payload")
}

fn fixture_content_key() -> ProfileSyncContentKey {
    ProfileSyncContentKey::from_bytes([11; PROFILE_SYNC_CONTENT_KEY_BYTES])
}

struct FetchedObject {
    object_id: String,
    bytes: Vec<u8>,
}

fn put_object(
    broadweb: &PluginRegistry,
    profile: &str,
    bytes: Vec<u8>,
    budget: &ResourceBudget,
) -> String {
    let put = broadweb
        .profile_sync(
            ProfileSyncRequest::PutEncryptedObject(ProfileSyncPutObjectRequest::new(
                profile, bytes,
            )),
            budget,
        )
        .expect("put fixture object");
    let ProfileSyncResponse::PutEncryptedObject { object_id } = put else {
        panic!("unexpected put response");
    };

    broadweb
        .profile_sync(
            ProfileSyncRequest::RetainObject(ProfileSyncObjectRequest::new(
                profile,
                object_id.clone(),
            )),
            budget,
        )
        .expect("retain fixture object");

    object_id
}

fn put_and_publish_object(
    broadweb: &PluginRegistry,
    profile: &str,
    root_id: &str,
    bytes: Vec<u8>,
    budget: &ResourceBudget,
) -> String {
    let object_id = put_object(broadweb, profile, bytes, budget);
    broadweb
        .profile_sync(
            ProfileSyncRequest::PublishRoot(ProfileSyncRootUpdate::new(
                profile,
                root_id,
                object_id.clone(),
            )),
            budget,
        )
        .expect("publish fixture root");

    object_id
}

fn fetch_published_object(
    broadweb: &PluginRegistry,
    profile: &str,
    root_id: &str,
    budget: &ResourceBudget,
) -> FetchedObject {
    let resolved = broadweb
        .profile_sync(
            ProfileSyncRequest::ResolveRoot(ProfileSyncRootRequest::new(profile, root_id)),
            budget,
        )
        .expect("resolve fixture root");
    let ProfileSyncResponse::Root {
        object_id: Some(object_id),
        ..
    } = resolved
    else {
        panic!("fixture root did not resolve to an object");
    };

    fetch_object(broadweb, profile, object_id.as_str(), budget)
}

fn fetch_object(
    broadweb: &PluginRegistry,
    profile: &str,
    object_id: &str,
    budget: &ResourceBudget,
) -> FetchedObject {
    let fetched = broadweb
        .profile_sync(
            ProfileSyncRequest::GetEncryptedObject(ProfileSyncObjectRequest::new(
                profile,
                object_id.to_string(),
            )),
            budget,
        )
        .expect("fetch fixture object");
    let ProfileSyncResponse::GetEncryptedObject { bytes, .. } = fetched else {
        panic!("unexpected get response");
    };

    FetchedObject {
        object_id: object_id.to_string(),
        bytes,
    }
}

fn test_dir(name: &str) -> PathBuf {
    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("time should move forward")
        .as_nanos();
    std::env::temp_dir().join(format!(
        "slate-profile-sync-fixture-{name}-{}-{nanos}",
        std::process::id()
    ))
}
