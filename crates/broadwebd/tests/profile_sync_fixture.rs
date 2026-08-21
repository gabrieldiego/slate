#![forbid(unsafe_code)]

use slate_broadwebd::{
    LocalProfileSyncFixture, PluginRegistry, ProfileSyncObjectRequest, ProfileSyncPutObjectRequest,
    ProfileSyncRequest, ProfileSyncResponse, ProfileSyncRootRequest, ProfileSyncRootUpdate,
    ResourceBudget,
};
use slate_storage::{
    DEFAULT_DATABASE_FILE_NAME, DEFAULT_PROFILE_ID, IncomingSyncSettingText, SYNC_DOMAIN_SETTINGS,
    SlateProfileDatabase, SyncChangeRecord, SyncRevisionRecord,
};
use std::path::PathBuf;
use std::time::{SystemTime, UNIX_EPOCH};

#[derive(serde::Deserialize, serde::Serialize)]
struct FixtureSettingChangeObject {
    profile: String,
    domain: String,
    key: String,
    value: String,
    device_id: String,
    device_sequence: i64,
    logical_clock: i64,
}

impl FixtureSettingChangeObject {
    fn from_change(change: &SyncChangeRecord) -> Self {
        assert_eq!(change.operation, "set_text");
        Self {
            profile: change.profile.clone(),
            domain: change.domain.clone(),
            key: change.entity_key.clone(),
            value: change.payload.clone(),
            device_id: change.device_id.clone(),
            device_sequence: change.device_sequence,
            logical_clock: change.logical_clock,
        }
    }

    fn into_incoming(self) -> IncomingSyncSettingText {
        IncomingSyncSettingText::new(
            self.profile,
            self.domain,
            self.key,
            self.value,
            self.device_id,
            self.device_sequence,
            self.logical_clock,
        )
    }
}

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

    let local_change = device_a_db
        .set_sync_setting_text(DEFAULT_PROFILE_ID, SYNC_DOMAIN_SETTINGS, "ui.theme", "teal")
        .expect("device a writes local setting");
    assert_eq!(local_change.device_id, "device-a");

    let object_bytes = serde_json::to_vec(&FixtureSettingChangeObject::from_change(&local_change))
        .expect("encode fixture sync object");
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

    let incoming: FixtureSettingChangeObject =
        serde_json::from_slice(fetched.bytes.as_slice()).expect("decode fixture sync object");
    let applied = device_b_db
        .apply_sync_setting_text(&incoming.into_incoming())
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
        .apply_sync_setting_text(
            &FixtureSettingChangeObject::from_change(&local_change).into_incoming(),
        )
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
