#![forbid(unsafe_code)]

use slate_broadwebd::{
    LocalProfileSyncFixture, PluginRegistry, ProfileSyncObjectRequest, ProfileSyncPutObjectRequest,
    ProfileSyncRequest, ProfileSyncResponse, ProfileSyncRootRequest, ProfileSyncRootUpdate,
    ResourceBudget,
};
use slate_storage::{
    DEFAULT_DATABASE_FILE_NAME, DEFAULT_PROFILE_ID, EncryptedSyncObject, IncomingSyncSettingText,
    PROFILE_SYNC_CONTENT_KEY_BYTES, ProfileSyncContentKey, ProfileSyncDevicePublicKey,
    ProfileSyncDeviceSigner, SYNC_DOMAIN_SETTINGS, SignedSyncObject, SlateProfileDatabase,
    SyncChangeRecord, SyncRevisionRecord,
};
use std::path::PathBuf;
use std::time::{SystemTime, UNIX_EPOCH};

const SETTINGS_CHANGE_OBJECT_KIND: &str = "setting-change";
const FIXTURE_CONTENT_KEY_ID: &str = "content-key-epoch-1";

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
    let object_bytes = sign_encrypted_setting_change(&local_change, &content_key, &device_a_signer);
    assert!(
        !std::str::from_utf8(object_bytes.as_slice())
            .expect("fixture object is JSON envelope")
            .contains("teal")
    );
    let object_id = put_object(
        &device_a_broadweb,
        DEFAULT_PROFILE_ID,
        "settings/latest",
        object_bytes,
        &budget,
    );

    let fetched = fetch_published_object(
        &device_b_broadweb,
        DEFAULT_PROFILE_ID,
        "settings/latest",
        &budget,
    );
    assert_eq!(fetched.object_id, object_id);

    let incoming = verify_and_decrypt_setting_change(
        fetched.bytes.as_slice(),
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
    root_id: &str,
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

    let fetched = broadweb
        .profile_sync(
            ProfileSyncRequest::GetEncryptedObject(ProfileSyncObjectRequest::new(
                profile,
                object_id.clone(),
            )),
            budget,
        )
        .expect("fetch fixture object");
    let ProfileSyncResponse::GetEncryptedObject { bytes, .. } = fetched else {
        panic!("unexpected get response");
    };

    FetchedObject { object_id, bytes }
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
