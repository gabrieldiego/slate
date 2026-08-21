#![forbid(unsafe_code)]

use slate_broadwebd::{
    LocalProfileSyncFixture, PluginRegistry, ProfileSyncObjectRequest, ProfileSyncPutObjectRequest,
    ProfileSyncRequest, ProfileSyncResponse, ProfileSyncRootRequest, ProfileSyncRootUpdate,
    ResourceBudget,
};
use slate_storage::{
    DEFAULT_DATABASE_FILE_NAME, DEFAULT_PROFILE_ID, EncryptedSyncObject, IncomingSyncSettingText,
    PROFILE_SYNC_CONTENT_KEY_BYTES, ProfileSyncContentKey, ProfileSyncDeviceFrontier,
    ProfileSyncDevicePublicKey, ProfileSyncDeviceSigner, ProfileSyncManifest, SYNC_DOMAIN_SETTINGS,
    SignedSyncObject, SlateProfileDatabase, SyncChangeRecord, SyncRevisionRecord,
};
use std::path::PathBuf;
use std::time::{SystemTime, UNIX_EPOCH};

const SETTINGS_CHANGE_OBJECT_KIND: &str = "setting-change";
const MANIFEST_OBJECT_KIND: &str = "manifest";
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
        SETTINGS_CHANGE_OBJECT_KIND,
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
        current_snapshot_object_id: None,
        tail_change_object_ids: vec![change_object_id.to_string()],
        included_domains: vec![change.domain.clone()],
        device_frontiers: vec![ProfileSyncDeviceFrontier {
            device_id: change.device_id.clone(),
            latest_sequence: change.device_sequence,
            latest_change_object_id: Some(change_object_id.to_string()),
        }],
        created_at: change.created_at,
    };
    let payload = serde_json::to_vec(&manifest).expect("encode fixture manifest payload");
    let encrypted_object = EncryptedSyncObject::seal(
        manifest.profile.as_str(),
        SYNC_DOMAIN_SETTINGS,
        MANIFEST_OBJECT_KIND,
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
    let signed_object = SignedSyncObject::from_bytes(bytes).expect("decode fixture signed object");
    let encrypted_bytes = signed_object
        .verify_with(public_key)
        .expect("verify fixture signed object");
    let encrypted_object = EncryptedSyncObject::from_bytes(encrypted_bytes)
        .expect("decode fixture encrypted sync object");
    assert_eq!(encrypted_object.profile, DEFAULT_PROFILE_ID);
    assert_eq!(encrypted_object.domain, SYNC_DOMAIN_SETTINGS);
    assert_eq!(encrypted_object.object_kind, SETTINGS_CHANGE_OBJECT_KIND);
    assert_eq!(encrypted_object.key_id, FIXTURE_CONTENT_KEY_ID);

    let payload = encrypted_object
        .open(content_key)
        .expect("decrypt fixture sync object");
    serde_json::from_slice(payload.as_slice()).expect("decode fixture sync payload")
}

fn verify_and_decrypt_manifest(
    bytes: &[u8],
    content_key: &ProfileSyncContentKey,
    public_key: &ProfileSyncDevicePublicKey,
) -> ProfileSyncManifest {
    let signed_object =
        SignedSyncObject::from_bytes(bytes).expect("decode fixture signed manifest");
    let encrypted_bytes = signed_object
        .verify_with(public_key)
        .expect("verify fixture signed manifest");
    let encrypted_object = EncryptedSyncObject::from_bytes(encrypted_bytes)
        .expect("decode fixture encrypted manifest");
    assert_eq!(encrypted_object.profile, DEFAULT_PROFILE_ID);
    assert_eq!(encrypted_object.domain, SYNC_DOMAIN_SETTINGS);
    assert_eq!(encrypted_object.object_kind, MANIFEST_OBJECT_KIND);
    assert_eq!(encrypted_object.key_id, FIXTURE_CONTENT_KEY_ID);

    let payload = encrypted_object
        .open(content_key)
        .expect("decrypt fixture manifest");
    serde_json::from_slice(payload.as_slice()).expect("decode fixture manifest payload")
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
