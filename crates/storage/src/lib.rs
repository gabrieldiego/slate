#![forbid(unsafe_code)]

use base64::Engine as _;
use base64::engine::general_purpose::URL_SAFE_NO_PAD;
use ring::{aead, hkdf, rand, signature};
use serde::{Deserialize, Serialize, de::DeserializeOwned};
use std::collections::{BTreeMap, BTreeSet};
use std::fmt;
use std::marker::PhantomData;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::{SystemTime, UNIX_EPOCH};

use ring::rand::SecureRandom;
use ring::signature::KeyPair;
use rusqlite::types::Type;
use rusqlite::{Connection, OptionalExtension, params};

pub const DEFAULT_DATABASE_FILE_NAME: &str = "slate-settings.db";
pub const DEFAULT_HOME_DIRECTORY_NAME: &str = ".slate";
pub const DEFAULT_PROFILE_ID: &str = "default";
pub const DEFAULT_SYNC_DEVICE_ID: &str = "local-device";
pub const DEFAULT_LOCAL_SYNC_DEVICE_ID_FILE_NAME: &str = "slate-local-device-id";
pub const DEFAULT_PROFILE_SYNC_CONTENT_KEY_ID: &str = "content-key-epoch-1";
pub const DEFAULT_PROFILE_SYNC_ACCOUNT_AUTHORITY_DEVICE_ID: &str = "account-authority";
pub const DEFAULT_PROFILE_SYNC_PREVIEW_PROVIDER_ID: &str = "local-preview-provider";
pub const DEFAULT_PROFILE_SYNC_PREVIEW_PROVIDER_KIND: &str = "local-fixture";
pub const SLATE_SYNC_SECRET_BYTES: usize = 32;
pub const PROFILE_SYNC_DERIVED_SECRET_BYTES: usize = 32;
pub const PROFILE_SYNC_CONTENT_KEY_BYTES: usize = 32;
pub const PROFILE_SYNC_NONCE_BYTES: usize = 12;
pub const SYNC_OBJECT_VERSION: u8 = 1;
pub const PROFILE_SYNC_MANIFEST_SCHEMA_VERSION: u8 = 1;
pub const PROFILE_SYNC_DEVICE_HEAD_SCHEMA_VERSION: u8 = 1;
pub const PROFILE_SYNC_MEMBERSHIP_RECORD_SCHEMA_VERSION: u8 = 1;
pub const PROFILE_SYNC_ENROLLMENT_BUNDLE_SCHEMA_VERSION: u8 = 1;
pub const PROFILE_SYNC_DEVICE_ENROLLMENT_REQUEST_SCHEMA_VERSION: u8 = 1;
pub const PROFILE_SYNC_SECRET_HANDOFF_BUNDLE_SCHEMA_VERSION: u8 = 1;
pub const SLATE_SYNC_SECRET_EXPORT_SCHEMA_VERSION: u8 = 1;
pub const PROFILE_SYNC_SETTINGS_SNAPSHOT_SCHEMA_VERSION: u8 = 1;
pub const PROFILE_SYNC_SETTING_CHANGE_OBJECT_KIND: &str = "setting-change";
pub const PROFILE_SYNC_SETTINGS_SNAPSHOT_OBJECT_KIND: &str = "settings-snapshot";
pub const PROFILE_SYNC_MANIFEST_OBJECT_KIND: &str = "manifest";
pub const PROFILE_SYNC_DEVICE_HEAD_OBJECT_KIND: &str = "device-head";
pub const SLATE_SYNC_SECRET_EXPORT_OBJECT_KIND: &str = "slate-sync-secret-export";
pub const PROFILE_SYNC_MEMBERSHIP_RECORD_KIND_ENROLL_DEVICE: &str = "enroll-device";
pub const PROFILE_SYNC_MEMBERSHIP_RECORD_KIND_ENROLL_PROVIDER: &str = "enroll-provider";
pub const PROFILE_SYNC_MEMBERSHIP_RECORD_KIND_REVOKE_DEVICE: &str = "revoke-device";
pub const PROFILE_SYNC_MEMBERSHIP_RECORD_KIND_ROTATE_DEVICE_KEY: &str = "rotate-device-key";
pub const DEFAULT_PROFILE_SYNC_MEMBERSHIP_EPOCH: i64 = 1;
pub const DEFAULT_PROFILE_SYNC_ENROLLMENT_BUNDLE_MAX_RECORDS: usize = 128;
pub const DEFAULT_PROFILE_SYNC_MIN_TAIL_CHANGE_COUNT: u32 = 32;
pub const DEFAULT_PROFILE_SYNC_CHANGE_RETENTION_SECONDS: i64 = 14 * 24 * 60 * 60;
pub const DEFAULT_PROFILE_SYNC_INACTIVE_DEVICE_GRACE_SECONDS: i64 = 30 * 24 * 60 * 60;
pub const PROFILE_SYNC_CONTENT_KEY_ALGORITHM_CHACHA20_POLY1305: &str = "chacha20-poly1305";
pub const SYNC_DOMAIN_BOOKMARKS: &str = "bookmarks";
pub const SYNC_DOMAIN_CALENDAR: &str = "calendar";
pub const SYNC_DOMAIN_CHAT: &str = "chat";
pub const SYNC_DOMAIN_CONTACTS: &str = "contacts";
pub const SYNC_DOMAIN_DOWNLOADS: &str = "downloads";
pub const SYNC_DOMAIN_FILES: &str = "files";
pub const SYNC_DOMAIN_SETTINGS: &str = "settings";
pub const SYNC_DOMAIN_STORAGE: &str = "storage";

pub const DEFAULT_HOME_BOOKMARKS: [DefaultBookmark; 2] = [
    DefaultBookmark {
        title: "Wikipedia on IPFS",
        url: "ipns://en.wikipedia-on-ipfs.org/wiki/",
    },
    DefaultBookmark {
        title: "OpenStreetMap",
        url: "https://www.openstreetmap.org/",
    },
];

const DEFAULT_BOOKMARKS_SEEDED_SETTING_KEY: &str = "bookmarks.defaults_seeded";
const BOOKMARK_HOME_SLOT_SYNC_KEY_PREFIX: &str = "home.slot.";
const CALENDAR_EVENT_SYNC_KEY_PREFIX: &str = "event.";
const CHAT_CONVERSATION_SYNC_KEY_PREFIX: &str = "conversation.";
const CONTACT_CARD_SYNC_KEY_PREFIX: &str = "contact.";
const DOWNLOAD_METADATA_SYNC_KEY_PREFIX: &str = "download.";
const FILE_ENTRY_SYNC_KEY_PREFIX: &str = "entry.";
const STORAGE_PROVIDER_SYNC_KEY_PREFIX: &str = "provider.";
const APP_SYNC_DOMAIN_CURSOR_KEY_PREFIX: &str = "app_sync.cursor.";
const PROFILE_SYNC_ROOT_KEY_PREFIX: &str = "profile_sync.root.";
const PROFILE_SYNC_SNAPSHOT_DEVICE_ID: &str = "snapshot";

const DEFAULT_APP_SYNC_DOMAINS: [DefaultAppSyncDomain; 8] = [
    DefaultAppSyncDomain {
        domain: SYNC_DOMAIN_SETTINGS,
        schema_version: 1,
        privacy_classification: "low-risk",
        sync_content: false,
        default_enabled: true,
    },
    DefaultAppSyncDomain {
        domain: SYNC_DOMAIN_BOOKMARKS,
        schema_version: 1,
        privacy_classification: "low-risk",
        sync_content: false,
        default_enabled: true,
    },
    DefaultAppSyncDomain {
        domain: SYNC_DOMAIN_CALENDAR,
        schema_version: 1,
        privacy_classification: "sensitive",
        sync_content: false,
        default_enabled: false,
    },
    DefaultAppSyncDomain {
        domain: SYNC_DOMAIN_CONTACTS,
        schema_version: 1,
        privacy_classification: "sensitive",
        sync_content: false,
        default_enabled: false,
    },
    DefaultAppSyncDomain {
        domain: SYNC_DOMAIN_CHAT,
        schema_version: 1,
        privacy_classification: "sensitive",
        sync_content: false,
        default_enabled: false,
    },
    DefaultAppSyncDomain {
        domain: SYNC_DOMAIN_FILES,
        schema_version: 1,
        privacy_classification: "content",
        sync_content: true,
        default_enabled: false,
    },
    DefaultAppSyncDomain {
        domain: SYNC_DOMAIN_DOWNLOADS,
        schema_version: 1,
        privacy_classification: "metadata",
        sync_content: false,
        default_enabled: true,
    },
    DefaultAppSyncDomain {
        domain: SYNC_DOMAIN_STORAGE,
        schema_version: 1,
        privacy_classification: "sensitive",
        sync_content: false,
        default_enabled: false,
    },
];

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ProfileId(String);

impl ProfileId {
    pub fn new(value: impl Into<String>) -> Self {
        Self(value.into())
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct StoragePolicy {
    pub profile: ProfileId,
    pub partition_by_top_level_site: bool,
}

impl Default for StoragePolicy {
    fn default() -> Self {
        Self {
            profile: ProfileId::new("default"),
            partition_by_top_level_site: true,
        }
    }
}

#[derive(Clone, Debug)]
pub struct SlateProfileDatabase {
    path: Arc<PathBuf>,
    local_sync_device_id: Arc<String>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum DatabasePathSource {
    Explicit,
    LaunchDirectoryExisting,
    HomeDirectoryExisting,
    LaunchDirectoryCreated,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ResolvedDatabasePath {
    pub path: PathBuf,
    pub source: DatabasePathSource,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct DefaultBookmark {
    pub title: &'static str,
    pub url: &'static str,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct DefaultAppSyncDomain {
    pub domain: &'static str,
    pub schema_version: i64,
    pub privacy_classification: &'static str,
    pub sync_content: bool,
    pub default_enabled: bool,
}

#[derive(Clone, Eq, PartialEq)]
pub struct SlateSyncSecret {
    bytes: [u8; SLATE_SYNC_SECRET_BYTES],
}

impl SlateSyncSecret {
    pub fn generate() -> Result<Self, SyncObjectError> {
        let mut bytes = [0_u8; SLATE_SYNC_SECRET_BYTES];
        rand::SystemRandom::new()
            .fill(&mut bytes)
            .map_err(|_| SyncObjectError::Random)?;
        Ok(Self { bytes })
    }

    pub fn from_bytes(bytes: [u8; SLATE_SYNC_SECRET_BYTES]) -> Self {
        Self { bytes }
    }

    pub fn export_for_profile(
        &self,
        profile: impl Into<String>,
        created_at: i64,
    ) -> SlateSyncSecretExport {
        SlateSyncSecretExport::new(profile, self, created_at)
    }

    pub fn from_export(export: &SlateSyncSecretExport) -> Result<Self, SyncObjectError> {
        export.to_sync_secret()
    }

    pub fn from_export_for_profile(
        export: &SlateSyncSecretExport,
        expected_profile: &str,
    ) -> Result<Self, SyncObjectError> {
        export.to_sync_secret_for_profile(expected_profile)
    }

    pub fn derive_profile_sync_content_key(
        &self,
        profile: &str,
        key_id: &str,
    ) -> Result<ProfileSyncContentKey, SyncObjectError> {
        let info = format!("content-key/v1/{key_id}");
        Ok(ProfileSyncContentKey::from_bytes(
            self.derive_profile_sync_secret_bytes(profile, info.as_str())?,
        ))
    }

    pub fn derive_profile_sync_account_recovery_secret(
        &self,
        profile: &str,
    ) -> Result<ProfileSyncDerivedSecret, SyncObjectError> {
        Ok(ProfileSyncDerivedSecret::new(
            ProfileSyncDerivedSecretPurpose::AccountRecovery,
            self.derive_profile_sync_secret_bytes(profile, "account-recovery/v1")?,
        ))
    }

    pub fn derive_profile_sync_manifest_signing_secret(
        &self,
        profile: &str,
        device_id: &str,
        membership_epoch: i64,
    ) -> Result<ProfileSyncDerivedSecret, SyncObjectError> {
        if !is_valid_sync_identifier(device_id) {
            return Err(SyncObjectError::InvalidDeviceId(device_id.to_string()));
        }
        let info = format!("manifest-signing/v1/{membership_epoch}/{device_id}");
        Ok(ProfileSyncDerivedSecret::new(
            ProfileSyncDerivedSecretPurpose::ManifestSigning,
            self.derive_profile_sync_secret_bytes(profile, info.as_str())?,
        ))
    }

    pub fn derive_profile_sync_device_signer(
        &self,
        profile: &str,
        device_id: &str,
        membership_epoch: i64,
    ) -> Result<ProfileSyncDeviceSigner, SyncObjectError> {
        let signing_secret =
            self.derive_profile_sync_manifest_signing_secret(profile, device_id, membership_epoch)?;
        ProfileSyncDeviceSigner::from_manifest_signing_secret(device_id, &signing_secret)
    }

    pub fn derive_profile_sync_mutable_root_secret(
        &self,
        profile: &str,
        root_id: &str,
        membership_epoch: i64,
    ) -> Result<ProfileSyncDerivedSecret, SyncObjectError> {
        let info = format!("mutable-root/v1/{membership_epoch}/{root_id}");
        Ok(ProfileSyncDerivedSecret::new(
            ProfileSyncDerivedSecretPurpose::MutableRootPublishing,
            self.derive_profile_sync_secret_bytes(profile, info.as_str())?,
        ))
    }

    pub fn derive_profile_sync_enrollment_secret(
        &self,
        profile: &str,
        target_device_id: &str,
        membership_epoch: i64,
    ) -> Result<ProfileSyncDerivedSecret, SyncObjectError> {
        if !is_valid_sync_identifier(target_device_id) {
            return Err(SyncObjectError::InvalidDeviceId(
                target_device_id.to_string(),
            ));
        }
        let info = format!("enrollment/v1/{membership_epoch}/{target_device_id}");
        Ok(ProfileSyncDerivedSecret::new(
            ProfileSyncDerivedSecretPurpose::DeviceEnrollment,
            self.derive_profile_sync_secret_bytes(profile, info.as_str())?,
        ))
    }

    pub fn derive_profile_sync_device_bootstrap_secret(
        &self,
        profile: &str,
        device_id: &str,
        membership_epoch: i64,
    ) -> Result<ProfileSyncDerivedSecret, SyncObjectError> {
        if !is_valid_sync_identifier(device_id) {
            return Err(SyncObjectError::InvalidDeviceId(device_id.to_string()));
        }
        let info = format!("device-bootstrap/v1/{membership_epoch}/{device_id}");
        Ok(ProfileSyncDerivedSecret::new(
            ProfileSyncDerivedSecretPurpose::DeviceBootstrap,
            self.derive_profile_sync_secret_bytes(profile, info.as_str())?,
        ))
    }

    fn derive_profile_sync_secret_bytes(
        &self,
        profile: &str,
        info_label: &str,
    ) -> Result<[u8; PROFILE_SYNC_DERIVED_SECRET_BYTES], SyncObjectError> {
        let salt_bytes = format!("slate/profile-sync/{profile}").into_bytes();
        let salt = hkdf::Salt::new(hkdf::HKDF_SHA256, salt_bytes.as_slice());
        let prk = salt.extract(self.bytes.as_slice());
        let info = [info_label.as_bytes()];
        let okm = prk
            .expand(&info, hkdf::HKDF_SHA256)
            .map_err(|_| SyncObjectError::Key)?;
        let mut secret = [0_u8; PROFILE_SYNC_DERIVED_SECRET_BYTES];
        okm.fill(&mut secret).map_err(|_| SyncObjectError::Key)?;
        Ok(secret)
    }
}

impl fmt::Debug for SlateSyncSecret {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("SlateSyncSecret")
            .field("bytes", &"<redacted>")
            .finish()
    }
}

#[derive(Clone, Eq, PartialEq, Deserialize, Serialize)]
pub struct SlateSyncSecretExport {
    pub profile: String,
    #[serde(default = "default_slate_sync_secret_export_schema_version")]
    pub schema_version: u8,
    pub secret: String,
    pub created_at: i64,
}

impl SlateSyncSecretExport {
    pub fn new(profile: impl Into<String>, secret: &SlateSyncSecret, created_at: i64) -> Self {
        Self {
            profile: profile.into(),
            schema_version: SLATE_SYNC_SECRET_EXPORT_SCHEMA_VERSION,
            secret: URL_SAFE_NO_PAD.encode(secret.bytes.as_slice()),
            created_at,
        }
    }

    pub fn to_bytes(&self) -> Result<Vec<u8>, SyncObjectError> {
        self.validate_schema()?;
        serde_json::to_vec(self).map_err(SyncObjectError::Encode)
    }

    pub fn from_bytes(bytes: &[u8]) -> Result<Self, SyncObjectError> {
        let export: Self = serde_json::from_slice(bytes).map_err(SyncObjectError::Decode)?;
        export.validate_schema()?;
        Ok(export)
    }

    fn to_sync_secret(&self) -> Result<SlateSyncSecret, SyncObjectError> {
        self.validate_schema()?;
        let decoded = URL_SAFE_NO_PAD
            .decode(self.secret.as_bytes())
            .map_err(|_| SyncObjectError::Key)?;
        let bytes: [u8; SLATE_SYNC_SECRET_BYTES] =
            decoded.try_into().map_err(|_| SyncObjectError::Key)?;
        Ok(SlateSyncSecret::from_bytes(bytes))
    }

    fn to_sync_secret_for_profile(
        &self,
        expected_profile: &str,
    ) -> Result<SlateSyncSecret, SyncObjectError> {
        self.validate_expected_profile(expected_profile)?;
        self.to_sync_secret()
    }

    fn validate_schema(&self) -> Result<(), SyncObjectError> {
        if self.schema_version != SLATE_SYNC_SECRET_EXPORT_SCHEMA_VERSION {
            return Err(SyncObjectError::UnsupportedSchema {
                object_kind: SLATE_SYNC_SECRET_EXPORT_OBJECT_KIND.to_string(),
                schema_version: self.schema_version,
            });
        }
        Ok(())
    }

    fn validate_expected_profile(&self, expected_profile: &str) -> Result<(), SyncObjectError> {
        self.validate_schema()?;
        if self.profile != expected_profile {
            return Err(SyncObjectError::UnexpectedProfile {
                expected: expected_profile.to_string(),
                actual: self.profile.clone(),
            });
        }
        Ok(())
    }
}

impl fmt::Debug for SlateSyncSecretExport {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("SlateSyncSecretExport")
            .field("profile", &self.profile)
            .field("schema_version", &self.schema_version)
            .field("secret", &"<redacted>")
            .field("created_at", &self.created_at)
            .finish()
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ProfileSyncDerivedSecretPurpose {
    AccountRecovery,
    ManifestSigning,
    MutableRootPublishing,
    DeviceEnrollment,
    DeviceBootstrap,
}

#[derive(Clone, Eq, PartialEq)]
pub struct ProfileSyncDerivedSecret {
    purpose: ProfileSyncDerivedSecretPurpose,
    bytes: [u8; PROFILE_SYNC_DERIVED_SECRET_BYTES],
}

impl ProfileSyncDerivedSecret {
    fn new(
        purpose: ProfileSyncDerivedSecretPurpose,
        bytes: [u8; PROFILE_SYNC_DERIVED_SECRET_BYTES],
    ) -> Self {
        Self { purpose, bytes }
    }

    pub fn purpose(&self) -> ProfileSyncDerivedSecretPurpose {
        self.purpose
    }

    #[cfg(test)]
    fn as_bytes(&self) -> &[u8] {
        self.bytes.as_slice()
    }
}

impl fmt::Debug for ProfileSyncDerivedSecret {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("ProfileSyncDerivedSecret")
            .field("purpose", &self.purpose)
            .field("bytes", &"<redacted>")
            .finish()
    }
}

#[derive(Clone, Eq, PartialEq)]
pub struct ProfileSyncContentKey {
    bytes: [u8; PROFILE_SYNC_CONTENT_KEY_BYTES],
}

impl ProfileSyncContentKey {
    pub fn from_bytes(bytes: [u8; PROFILE_SYNC_CONTENT_KEY_BYTES]) -> Self {
        Self { bytes }
    }

    fn as_bytes(&self) -> &[u8] {
        self.bytes.as_slice()
    }
}

impl fmt::Debug for ProfileSyncContentKey {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("ProfileSyncContentKey")
            .field("bytes", &"<redacted>")
            .finish()
    }
}

#[derive(Clone, Eq, PartialEq)]
pub struct ProfileSyncDeviceSigner {
    device_id: String,
    key_material: ProfileSyncDeviceSigningKeyMaterial,
}

#[derive(Clone, Eq, PartialEq)]
enum ProfileSyncDeviceSigningKeyMaterial {
    Pkcs8(Vec<u8>),
    Seed([u8; PROFILE_SYNC_DERIVED_SECRET_BYTES]),
}

impl ProfileSyncDeviceSigner {
    pub fn generate(device_id: impl Into<String>) -> Result<Self, SyncObjectError> {
        let device_id = device_id.into();
        if !is_valid_sync_identifier(device_id.as_str()) {
            return Err(SyncObjectError::InvalidDeviceId(device_id));
        }

        let pkcs8 = signature::Ed25519KeyPair::generate_pkcs8(&rand::SystemRandom::new())
            .map_err(|_| SyncObjectError::Random)?;
        Ok(Self {
            device_id,
            key_material: ProfileSyncDeviceSigningKeyMaterial::Pkcs8(pkcs8.as_ref().to_vec()),
        })
    }

    pub fn from_pkcs8(
        device_id: impl Into<String>,
        pkcs8: impl Into<Vec<u8>>,
    ) -> Result<Self, SyncObjectError> {
        let device_id = device_id.into();
        if !is_valid_sync_identifier(device_id.as_str()) {
            return Err(SyncObjectError::InvalidDeviceId(device_id));
        }
        let signer = Self {
            device_id,
            key_material: ProfileSyncDeviceSigningKeyMaterial::Pkcs8(pkcs8.into()),
        };
        signer.key_pair()?;
        Ok(signer)
    }

    pub fn from_manifest_signing_secret(
        device_id: impl Into<String>,
        signing_secret: &ProfileSyncDerivedSecret,
    ) -> Result<Self, SyncObjectError> {
        if signing_secret.purpose != ProfileSyncDerivedSecretPurpose::ManifestSigning {
            return Err(SyncObjectError::UnexpectedDerivedSecretPurpose {
                expected: ProfileSyncDerivedSecretPurpose::ManifestSigning,
                actual: signing_secret.purpose,
            });
        }
        Self::from_seed(device_id, signing_secret.bytes)
    }

    fn from_seed(
        device_id: impl Into<String>,
        seed: [u8; PROFILE_SYNC_DERIVED_SECRET_BYTES],
    ) -> Result<Self, SyncObjectError> {
        let device_id = device_id.into();
        if !is_valid_sync_identifier(device_id.as_str()) {
            return Err(SyncObjectError::InvalidDeviceId(device_id));
        }
        let signer = Self {
            device_id,
            key_material: ProfileSyncDeviceSigningKeyMaterial::Seed(seed),
        };
        signer.key_pair()?;
        Ok(signer)
    }

    pub fn device_id(&self) -> &str {
        self.device_id.as_str()
    }

    pub fn public_key(&self) -> Result<ProfileSyncDevicePublicKey, SyncObjectError> {
        Ok(ProfileSyncDevicePublicKey {
            device_id: self.device_id.clone(),
            bytes: self.key_pair()?.public_key().as_ref().to_vec(),
        })
    }

    pub fn sign(&self, payload: &[u8]) -> Result<SignedSyncObject, SyncObjectError> {
        let key_pair = self.key_pair()?;
        Ok(SignedSyncObject {
            version: SYNC_OBJECT_VERSION,
            device_id: self.device_id.clone(),
            public_key: key_pair.public_key().as_ref().to_vec(),
            payload: payload.to_vec(),
            signature: key_pair.sign(payload).as_ref().to_vec(),
        })
    }

    fn key_pair(&self) -> Result<signature::Ed25519KeyPair, SyncObjectError> {
        match &self.key_material {
            ProfileSyncDeviceSigningKeyMaterial::Pkcs8(pkcs8) => {
                signature::Ed25519KeyPair::from_pkcs8(pkcs8.as_slice())
                    .map_err(|_| SyncObjectError::Key)
            }
            ProfileSyncDeviceSigningKeyMaterial::Seed(seed) => {
                // The seed is HKDF-derived for this profile/device/epoch and fixed-size by type.
                signature::Ed25519KeyPair::from_seed_unchecked(seed.as_slice())
                    .map_err(|_| SyncObjectError::Key)
            }
        }
    }
}

impl fmt::Debug for ProfileSyncDeviceSigner {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("ProfileSyncDeviceSigner")
            .field("device_id", &self.device_id)
            .field("key_material", &"<redacted>")
            .finish()
    }
}

#[derive(Clone, Debug, Eq, PartialEq, Deserialize, Serialize)]
pub struct ProfileSyncDevicePublicKey {
    pub device_id: String,
    pub bytes: Vec<u8>,
}

#[derive(Clone, Debug, Eq, PartialEq, Deserialize, Serialize)]
pub struct SignedSyncObject {
    pub version: u8,
    pub device_id: String,
    pub public_key: Vec<u8>,
    pub payload: Vec<u8>,
    pub signature: Vec<u8>,
}

#[derive(Clone, Debug, Eq, PartialEq, Deserialize, Serialize)]
pub struct ProfileSyncManifest {
    pub profile: String,
    pub root_id: String,
    #[serde(default = "default_profile_sync_manifest_schema_version")]
    pub schema_version: u8,
    #[serde(default = "default_profile_sync_membership_epoch")]
    pub membership_epoch: i64,
    pub current_snapshot_object_id: Option<String>,
    pub tail_change_object_ids: Vec<String>,
    pub included_domains: Vec<String>,
    pub device_frontiers: Vec<ProfileSyncDeviceFrontier>,
    #[serde(default)]
    pub retention_policy: ProfileSyncRetentionPolicy,
    pub created_at: i64,
}

#[derive(Clone, Debug, Eq, PartialEq, Deserialize, Serialize)]
pub struct ProfileSyncDeviceHead {
    pub profile: String,
    pub device_id: String,
    pub root_id: String,
    #[serde(default = "default_profile_sync_device_head_schema_version")]
    pub schema_version: u8,
    #[serde(default = "default_profile_sync_membership_epoch")]
    pub membership_epoch: i64,
    pub latest_manifest_object_id: String,
    pub latest_change_object_id: Option<String>,
    pub device_sequence: i64,
    pub logical_clock: i64,
    pub created_at: i64,
}

#[derive(Clone, Debug, Eq, PartialEq, Deserialize, Serialize)]
pub struct ProfileSyncMembershipRecord {
    pub profile: String,
    pub record_id: String,
    #[serde(default = "default_profile_sync_membership_record_schema_version")]
    pub schema_version: u8,
    pub membership_epoch: i64,
    pub record_kind: String,
    pub device_id: String,
    pub device_public_key: Option<ProfileSyncDevicePublicKey>,
    pub created_at: i64,
}

#[derive(Clone, Debug, Eq, PartialEq, Deserialize, Serialize)]
pub struct ProfileSyncEnrollmentBundle {
    pub profile: String,
    #[serde(default = "default_profile_sync_enrollment_bundle_schema_version")]
    pub schema_version: u8,
    pub target_device_id: String,
    pub created_at: i64,
    pub signed_membership_records: Vec<Vec<u8>>,
}

#[derive(Clone, Debug, Eq, PartialEq, Deserialize, Serialize)]
pub struct ProfileSyncSecretHandoffBundle {
    pub profile: String,
    #[serde(default = "default_profile_sync_secret_handoff_bundle_schema_version")]
    pub schema_version: u8,
    pub target_device_id: String,
    pub created_at: i64,
    pub sync_secret_export: SlateSyncSecretExport,
    pub enrollment_bundle: ProfileSyncEnrollmentBundle,
}

#[derive(Clone, Debug, Eq, PartialEq, Deserialize, Serialize)]
pub struct ProfileSyncDeviceEnrollmentRequest {
    pub profile: String,
    #[serde(default = "default_profile_sync_device_enrollment_request_schema_version")]
    pub schema_version: u8,
    pub device_id: String,
    pub created_at: i64,
}

#[derive(Clone, Debug, Eq, PartialEq, Deserialize, Serialize)]
pub struct ProfileSyncRetentionPolicy {
    pub min_tail_change_count: u32,
    pub change_retention_seconds: i64,
    pub inactive_device_grace_seconds: i64,
}

impl Default for ProfileSyncRetentionPolicy {
    fn default() -> Self {
        Self {
            min_tail_change_count: DEFAULT_PROFILE_SYNC_MIN_TAIL_CHANGE_COUNT,
            change_retention_seconds: DEFAULT_PROFILE_SYNC_CHANGE_RETENTION_SECONDS,
            inactive_device_grace_seconds: DEFAULT_PROFILE_SYNC_INACTIVE_DEVICE_GRACE_SECONDS,
        }
    }
}

#[derive(Clone, Debug, Eq, PartialEq, Deserialize, Serialize)]
pub struct ProfileSyncSettingsSnapshot {
    pub profile: String,
    pub schema_version: u8,
    pub covers_revision: i64,
    pub included_domains: Vec<String>,
    pub values: Vec<ProfileSyncSettingsSnapshotValue>,
    pub created_at: i64,
}

#[derive(Clone, Debug, Eq, PartialEq, Deserialize, Serialize)]
pub struct ProfileSyncSettingsSnapshotValue {
    pub domain: String,
    pub key: String,
    pub value: String,
    pub value_kind: String,
    pub revision: i64,
}

#[derive(Clone, Debug, Eq, PartialEq, Deserialize, Serialize)]
pub struct ProfileSyncDeviceFrontier {
    pub device_id: String,
    pub latest_sequence: i64,
    pub latest_change_object_id: Option<String>,
}

fn default_profile_sync_manifest_schema_version() -> u8 {
    PROFILE_SYNC_MANIFEST_SCHEMA_VERSION
}

fn default_profile_sync_device_head_schema_version() -> u8 {
    PROFILE_SYNC_DEVICE_HEAD_SCHEMA_VERSION
}

fn default_profile_sync_membership_record_schema_version() -> u8 {
    PROFILE_SYNC_MEMBERSHIP_RECORD_SCHEMA_VERSION
}

fn default_profile_sync_enrollment_bundle_schema_version() -> u8 {
    PROFILE_SYNC_ENROLLMENT_BUNDLE_SCHEMA_VERSION
}

fn default_profile_sync_device_enrollment_request_schema_version() -> u8 {
    PROFILE_SYNC_DEVICE_ENROLLMENT_REQUEST_SCHEMA_VERSION
}

fn default_profile_sync_secret_handoff_bundle_schema_version() -> u8 {
    PROFILE_SYNC_SECRET_HANDOFF_BUNDLE_SCHEMA_VERSION
}

fn default_slate_sync_secret_export_schema_version() -> u8 {
    SLATE_SYNC_SECRET_EXPORT_SCHEMA_VERSION
}

fn default_profile_sync_membership_epoch() -> i64 {
    DEFAULT_PROFILE_SYNC_MEMBERSHIP_EPOCH
}

impl ProfileSyncMembershipRecord {
    pub fn to_bytes(&self) -> Result<Vec<u8>, SyncObjectError> {
        serde_json::to_vec(self).map_err(SyncObjectError::Encode)
    }

    pub fn from_bytes(bytes: &[u8]) -> Result<Self, SyncObjectError> {
        serde_json::from_slice(bytes).map_err(SyncObjectError::Decode)
    }
}

impl ProfileSyncEnrollmentBundle {
    pub fn new_device_enrollment(
        profile: impl Into<String>,
        target_device_id: impl Into<String>,
        signed_membership_records: Vec<Vec<u8>>,
        created_at: i64,
    ) -> Result<Self, StorageError> {
        let bundle = Self {
            profile: profile.into(),
            schema_version: PROFILE_SYNC_ENROLLMENT_BUNDLE_SCHEMA_VERSION,
            target_device_id: target_device_id.into(),
            created_at,
            signed_membership_records,
        };
        validate_profile_sync_enrollment_bundle(&bundle)?;
        Ok(bundle)
    }

    pub fn to_bytes(&self) -> Result<Vec<u8>, StorageError> {
        validate_profile_sync_enrollment_bundle(self)?;
        serde_json::to_vec(self).map_err(StorageError::EncodeSyncPayload)
    }

    pub fn from_bytes(bytes: &[u8]) -> Result<Self, StorageError> {
        let bundle: Self =
            serde_json::from_slice(bytes).map_err(StorageError::DecodeSyncPayload)?;
        validate_profile_sync_enrollment_bundle(&bundle)?;
        Ok(bundle)
    }
}

impl ProfileSyncSecretHandoffBundle {
    pub fn new(
        profile: impl Into<String>,
        target_device_id: impl Into<String>,
        sync_secret_export: SlateSyncSecretExport,
        enrollment_bundle: ProfileSyncEnrollmentBundle,
        created_at: i64,
    ) -> Result<Self, StorageError> {
        let bundle = Self {
            profile: profile.into(),
            schema_version: PROFILE_SYNC_SECRET_HANDOFF_BUNDLE_SCHEMA_VERSION,
            target_device_id: target_device_id.into(),
            created_at,
            sync_secret_export,
            enrollment_bundle,
        };
        validate_profile_sync_secret_handoff_bundle(&bundle)?;
        Ok(bundle)
    }

    pub fn to_bytes(&self) -> Result<Vec<u8>, StorageError> {
        validate_profile_sync_secret_handoff_bundle(self)?;
        serde_json::to_vec(self).map_err(StorageError::EncodeSyncPayload)
    }

    pub fn from_bytes(bytes: &[u8]) -> Result<Self, StorageError> {
        let bundle: Self =
            serde_json::from_slice(bytes).map_err(StorageError::DecodeSyncPayload)?;
        validate_profile_sync_secret_handoff_bundle(&bundle)?;
        Ok(bundle)
    }

    fn to_sync_secret(&self) -> Result<SlateSyncSecret, StorageError> {
        validate_profile_sync_secret_handoff_bundle(self)?;
        SlateSyncSecret::from_export_for_profile(&self.sync_secret_export, self.profile.as_str())
            .map_err(profile_sync_secret_handoff_sync_object_error)
    }
}

impl ProfileSyncDeviceEnrollmentRequest {
    pub fn new(
        profile: impl Into<String>,
        device_id: impl Into<String>,
        created_at: i64,
    ) -> Result<Self, StorageError> {
        let request = Self {
            profile: profile.into(),
            schema_version: PROFILE_SYNC_DEVICE_ENROLLMENT_REQUEST_SCHEMA_VERSION,
            device_id: device_id.into(),
            created_at,
        };
        validate_profile_sync_device_enrollment_request(&request)?;
        Ok(request)
    }

    pub fn to_bytes(&self) -> Result<Vec<u8>, StorageError> {
        validate_profile_sync_device_enrollment_request(self)?;
        serde_json::to_vec(self).map_err(StorageError::EncodeSyncPayload)
    }

    pub fn from_bytes(bytes: &[u8]) -> Result<Self, StorageError> {
        let request: Self =
            serde_json::from_slice(bytes).map_err(StorageError::DecodeSyncPayload)?;
        validate_profile_sync_device_enrollment_request(&request)?;
        Ok(request)
    }

    pub fn from_bytes_for_profile(
        bytes: &[u8],
        expected_profile: &str,
    ) -> Result<Self, StorageError> {
        let request = Self::from_bytes(bytes)?;
        if request.profile != expected_profile {
            return Err(StorageError::InvalidProfileSyncDeviceEnrollmentRequest(
                format!(
                    "expected profile {expected_profile}, got {}",
                    request.profile
                ),
            ));
        }
        Ok(request)
    }
}

fn profile_sync_enroll_device_record(
    profile: &str,
    device_id: &str,
    membership_epoch: i64,
    public_key: ProfileSyncDevicePublicKey,
) -> ProfileSyncMembershipRecord {
    ProfileSyncMembershipRecord {
        profile: profile.to_string(),
        record_id: profile_sync_enroll_device_record_id(membership_epoch, device_id),
        schema_version: PROFILE_SYNC_MEMBERSHIP_RECORD_SCHEMA_VERSION,
        membership_epoch,
        record_kind: PROFILE_SYNC_MEMBERSHIP_RECORD_KIND_ENROLL_DEVICE.to_string(),
        device_id: device_id.to_string(),
        device_public_key: Some(public_key),
        created_at: membership_epoch,
    }
}

fn profile_sync_enroll_provider_record(
    profile: &str,
    provider_id: &str,
    membership_epoch: i64,
    public_key: ProfileSyncDevicePublicKey,
) -> ProfileSyncMembershipRecord {
    ProfileSyncMembershipRecord {
        profile: profile.to_string(),
        record_id: profile_sync_enroll_device_record_id(membership_epoch, provider_id),
        schema_version: PROFILE_SYNC_MEMBERSHIP_RECORD_SCHEMA_VERSION,
        membership_epoch,
        record_kind: PROFILE_SYNC_MEMBERSHIP_RECORD_KIND_ENROLL_PROVIDER.to_string(),
        device_id: provider_id.to_string(),
        device_public_key: Some(public_key),
        created_at: membership_epoch,
    }
}

fn profile_sync_enroll_device_record_id(membership_epoch: i64, device_id: &str) -> String {
    format!("epoch-{membership_epoch}-enroll-{device_id}")
}

fn signed_profile_sync_membership_record_bytes(
    signer: &ProfileSyncDeviceSigner,
    record: &ProfileSyncMembershipRecord,
) -> Result<Vec<u8>, StorageError> {
    let payload = record
        .to_bytes()
        .map_err(profile_sync_membership_record_error)?;
    signer
        .sign(payload.as_slice())
        .and_then(|signed| signed.to_bytes())
        .map_err(profile_sync_membership_record_error)
}

fn profile_sync_membership_record_error(error: SyncObjectError) -> StorageError {
    StorageError::InvalidProfileSyncMembershipRecord(error.to_string())
}

impl SignedSyncObject {
    pub fn verify_with(
        &self,
        public_key: &ProfileSyncDevicePublicKey,
    ) -> Result<&[u8], SyncObjectError> {
        if self.version != SYNC_OBJECT_VERSION {
            return Err(SyncObjectError::UnsupportedVersion(self.version));
        }
        if self.device_id != public_key.device_id || self.public_key != public_key.bytes {
            return Err(SyncObjectError::DeviceKeyMismatch {
                expected_device_id: public_key.device_id.clone(),
                actual_device_id: self.device_id.clone(),
            });
        }

        signature::UnparsedPublicKey::new(&signature::ED25519, public_key.bytes.as_slice())
            .verify(self.payload.as_slice(), self.signature.as_slice())
            .map_err(|_| SyncObjectError::Verify)?;
        Ok(self.payload.as_slice())
    }

    pub fn to_bytes(&self) -> Result<Vec<u8>, SyncObjectError> {
        serde_json::to_vec(self).map_err(SyncObjectError::Encode)
    }

    pub fn from_bytes(bytes: &[u8]) -> Result<Self, SyncObjectError> {
        serde_json::from_slice(bytes).map_err(SyncObjectError::Decode)
    }
}

#[derive(Clone, Debug, Eq, PartialEq, Deserialize, Serialize)]
pub struct EncryptedSyncObject {
    pub version: u8,
    pub profile: String,
    pub domain: String,
    pub object_kind: String,
    pub key_id: String,
    pub nonce: Vec<u8>,
    pub ciphertext: Vec<u8>,
}

#[derive(Debug)]
pub enum SyncObjectError {
    Random,
    Key,
    Encrypt,
    Decrypt,
    Verify,
    UnsupportedVersion(u8),
    UnsupportedSchema {
        object_kind: String,
        schema_version: u8,
    },
    InvalidDeviceId(String),
    DeviceKeyMismatch {
        expected_device_id: String,
        actual_device_id: String,
    },
    InvalidNonceLength {
        actual: usize,
    },
    UnexpectedProfile {
        expected: String,
        actual: String,
    },
    UnexpectedDomain {
        expected: String,
        actual: String,
    },
    UnexpectedObjectKind {
        expected: String,
        actual: String,
    },
    UnexpectedKeyId {
        expected: String,
        actual: String,
    },
    UnexpectedDerivedSecretPurpose {
        expected: ProfileSyncDerivedSecretPurpose,
        actual: ProfileSyncDerivedSecretPurpose,
    },
    UnexpectedRootId {
        expected: String,
        actual: String,
    },
    UnexpectedDeviceFrontier {
        device_id: String,
        expected_sequence: i64,
        actual_sequence: Option<i64>,
        expected_change_object_id: Option<String>,
        actual_change_object_id: Option<String>,
    },
    UnexpectedDeviceHeadManifestEpoch {
        device_id: String,
        head_membership_epoch: i64,
        manifest_membership_epoch: i64,
    },
    Encode(serde_json::Error),
    Decode(serde_json::Error),
}

impl fmt::Display for SyncObjectError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Random => write!(formatter, "failed to generate sync object nonce"),
            Self::Key => write!(formatter, "invalid profile sync signing key"),
            Self::Encrypt => write!(formatter, "failed to encrypt sync object"),
            Self::Decrypt => write!(formatter, "failed to decrypt sync object"),
            Self::Verify => write!(formatter, "failed to verify sync object signature"),
            Self::UnsupportedVersion(version) => {
                write!(formatter, "unsupported sync object version: {version}")
            }
            Self::UnsupportedSchema {
                object_kind,
                schema_version,
            } => write!(
                formatter,
                "unsupported {object_kind} sync object schema version: {schema_version}"
            ),
            Self::InvalidDeviceId(device_id) => {
                write!(formatter, "invalid sync object device id: {device_id}")
            }
            Self::DeviceKeyMismatch {
                expected_device_id,
                actual_device_id,
            } => write!(
                formatter,
                "sync object device key mismatch: expected {expected_device_id}, got {actual_device_id}"
            ),
            Self::InvalidNonceLength { actual } => {
                write!(formatter, "invalid sync object nonce length: {actual}")
            }
            Self::UnexpectedProfile { expected, actual } => write!(
                formatter,
                "unexpected sync object profile: expected {expected}, got {actual}"
            ),
            Self::UnexpectedDomain { expected, actual } => write!(
                formatter,
                "unexpected sync object domain: expected {expected}, got {actual}"
            ),
            Self::UnexpectedObjectKind { expected, actual } => write!(
                formatter,
                "unexpected sync object kind: expected {expected}, got {actual}"
            ),
            Self::UnexpectedKeyId { expected, actual } => write!(
                formatter,
                "unexpected sync object key id: expected {expected}, got {actual}"
            ),
            Self::UnexpectedDerivedSecretPurpose { expected, actual } => write!(
                formatter,
                "unexpected profile sync derived secret purpose: expected {expected:?}, got {actual:?}"
            ),
            Self::UnexpectedRootId { expected, actual } => write!(
                formatter,
                "unexpected sync object root id: expected {expected}, got {actual}"
            ),
            Self::UnexpectedDeviceFrontier {
                device_id,
                expected_sequence,
                actual_sequence,
                expected_change_object_id,
                actual_change_object_id,
            } => write!(
                formatter,
                "unexpected sync manifest frontier for device {device_id}: expected sequence {expected_sequence} and change {expected_change_object_id:?}, got sequence {actual_sequence:?} and change {actual_change_object_id:?}"
            ),
            Self::UnexpectedDeviceHeadManifestEpoch {
                device_id,
                head_membership_epoch,
                manifest_membership_epoch,
            } => write!(
                formatter,
                "unexpected sync manifest membership epoch for device {device_id}: expected head epoch {head_membership_epoch}, got manifest epoch {manifest_membership_epoch}"
            ),
            Self::Encode(error) => write!(formatter, "failed to encode sync object: {error}"),
            Self::Decode(error) => write!(formatter, "failed to decode sync object: {error}"),
        }
    }
}

impl std::error::Error for SyncObjectError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Self::Encode(error) | Self::Decode(error) => Some(error),
            Self::Random
            | Self::Key
            | Self::Encrypt
            | Self::Decrypt
            | Self::Verify
            | Self::UnsupportedVersion(_)
            | Self::UnsupportedSchema { .. }
            | Self::InvalidDeviceId(_)
            | Self::DeviceKeyMismatch { .. }
            | Self::InvalidNonceLength { .. }
            | Self::UnexpectedProfile { .. }
            | Self::UnexpectedDomain { .. }
            | Self::UnexpectedObjectKind { .. }
            | Self::UnexpectedKeyId { .. }
            | Self::UnexpectedDerivedSecretPurpose { .. }
            | Self::UnexpectedRootId { .. }
            | Self::UnexpectedDeviceFrontier { .. }
            | Self::UnexpectedDeviceHeadManifestEpoch { .. } => None,
        }
    }
}

impl EncryptedSyncObject {
    pub fn seal(
        profile: impl Into<String>,
        domain: impl Into<String>,
        object_kind: impl Into<String>,
        key_id: impl Into<String>,
        plaintext: &[u8],
        content_key: &ProfileSyncContentKey,
    ) -> Result<Self, SyncObjectError> {
        let mut nonce = [0_u8; PROFILE_SYNC_NONCE_BYTES];
        rand::SystemRandom::new()
            .fill(&mut nonce)
            .map_err(|_| SyncObjectError::Random)?;
        Self::seal_with_nonce(
            profile,
            domain,
            object_kind,
            key_id,
            plaintext,
            content_key,
            nonce,
        )
    }

    pub fn seal_with_nonce(
        profile: impl Into<String>,
        domain: impl Into<String>,
        object_kind: impl Into<String>,
        key_id: impl Into<String>,
        plaintext: &[u8],
        content_key: &ProfileSyncContentKey,
        nonce: [u8; PROFILE_SYNC_NONCE_BYTES],
    ) -> Result<Self, SyncObjectError> {
        let mut object = Self {
            version: SYNC_OBJECT_VERSION,
            profile: profile.into(),
            domain: domain.into(),
            object_kind: object_kind.into(),
            key_id: key_id.into(),
            nonce: nonce.to_vec(),
            ciphertext: plaintext.to_vec(),
        };
        object.ciphertext = seal_sync_payload(
            object.associated_data().as_bytes(),
            object.ciphertext.as_slice(),
            content_key,
            nonce,
        )?;
        Ok(object)
    }

    pub fn open(&self, content_key: &ProfileSyncContentKey) -> Result<Vec<u8>, SyncObjectError> {
        if self.version != SYNC_OBJECT_VERSION {
            return Err(SyncObjectError::UnsupportedVersion(self.version));
        }

        let nonce: [u8; PROFILE_SYNC_NONCE_BYTES] =
            self.nonce
                .as_slice()
                .try_into()
                .map_err(|_| SyncObjectError::InvalidNonceLength {
                    actual: self.nonce.len(),
                })?;
        open_sync_payload(
            self.associated_data().as_bytes(),
            self.ciphertext.as_slice(),
            content_key,
            nonce,
        )
    }

    pub fn open_expected(
        &self,
        content_key: &ProfileSyncContentKey,
        expected_profile: &str,
        expected_domain: &str,
        expected_object_kind: &str,
        expected_key_id: &str,
    ) -> Result<Vec<u8>, SyncObjectError> {
        self.validate_expected(
            expected_profile,
            expected_domain,
            expected_object_kind,
            expected_key_id,
        )?;
        self.open(content_key)
    }

    pub fn to_bytes(&self) -> Result<Vec<u8>, SyncObjectError> {
        serde_json::to_vec(self).map_err(SyncObjectError::Encode)
    }

    pub fn from_bytes(bytes: &[u8]) -> Result<Self, SyncObjectError> {
        serde_json::from_slice(bytes).map_err(SyncObjectError::Decode)
    }

    fn associated_data(&self) -> String {
        format!(
            "slate-profile-sync:v{}:{}:{}:{}:{}",
            self.version, self.profile, self.domain, self.object_kind, self.key_id
        )
    }

    fn validate_expected(
        &self,
        expected_profile: &str,
        expected_domain: &str,
        expected_object_kind: &str,
        expected_key_id: &str,
    ) -> Result<(), SyncObjectError> {
        if self.profile != expected_profile {
            return Err(SyncObjectError::UnexpectedProfile {
                expected: expected_profile.to_string(),
                actual: self.profile.clone(),
            });
        }
        if self.domain != expected_domain {
            return Err(SyncObjectError::UnexpectedDomain {
                expected: expected_domain.to_string(),
                actual: self.domain.clone(),
            });
        }
        if self.object_kind != expected_object_kind {
            return Err(SyncObjectError::UnexpectedObjectKind {
                expected: expected_object_kind.to_string(),
                actual: self.object_kind.clone(),
            });
        }
        if self.key_id != expected_key_id {
            return Err(SyncObjectError::UnexpectedKeyId {
                expected: expected_key_id.to_string(),
                actual: self.key_id.clone(),
            });
        }
        Ok(())
    }
}

pub fn open_signed_encrypted_sync_payload(
    bytes: &[u8],
    content_key: &ProfileSyncContentKey,
    public_key: &ProfileSyncDevicePublicKey,
    expected_profile: &str,
    expected_domain: &str,
    expected_object_kind: &str,
    expected_key_id: &str,
) -> Result<Vec<u8>, SyncObjectError> {
    let signed_object = SignedSyncObject::from_bytes(bytes)?;
    let encrypted_bytes = signed_object.verify_with(public_key)?;
    let encrypted_object = EncryptedSyncObject::from_bytes(encrypted_bytes)?;
    encrypted_object.open_expected(
        content_key,
        expected_profile,
        expected_domain,
        expected_object_kind,
        expected_key_id,
    )
}

pub fn open_signed_profile_sync_manifest(
    bytes: &[u8],
    content_key: &ProfileSyncContentKey,
    public_key: &ProfileSyncDevicePublicKey,
    profile: &str,
    key_id: &str,
) -> Result<ProfileSyncManifest, SyncObjectError> {
    let payload = open_signed_encrypted_sync_payload(
        bytes,
        content_key,
        public_key,
        profile,
        SYNC_DOMAIN_SETTINGS,
        PROFILE_SYNC_MANIFEST_OBJECT_KIND,
        key_id,
    )?;
    serde_json::from_slice(payload.as_slice()).map_err(SyncObjectError::Decode)
}

pub fn open_signed_profile_sync_device_head(
    bytes: &[u8],
    content_key: &ProfileSyncContentKey,
    public_key: &ProfileSyncDevicePublicKey,
    profile: &str,
    key_id: &str,
) -> Result<ProfileSyncDeviceHead, SyncObjectError> {
    let payload = open_signed_encrypted_sync_payload(
        bytes,
        content_key,
        public_key,
        profile,
        SYNC_DOMAIN_SETTINGS,
        PROFILE_SYNC_DEVICE_HEAD_OBJECT_KIND,
        key_id,
    )?;
    let device_head: ProfileSyncDeviceHead =
        serde_json::from_slice(payload.as_slice()).map_err(SyncObjectError::Decode)?;
    if device_head.schema_version != PROFILE_SYNC_DEVICE_HEAD_SCHEMA_VERSION {
        return Err(SyncObjectError::UnsupportedSchema {
            object_kind: PROFILE_SYNC_DEVICE_HEAD_OBJECT_KIND.to_string(),
            schema_version: device_head.schema_version,
        });
    }
    if device_head.profile != profile {
        return Err(SyncObjectError::UnexpectedProfile {
            expected: profile.to_string(),
            actual: device_head.profile,
        });
    }
    if device_head.device_id != public_key.device_id {
        return Err(SyncObjectError::DeviceKeyMismatch {
            expected_device_id: public_key.device_id.clone(),
            actual_device_id: device_head.device_id,
        });
    }
    Ok(device_head)
}

pub fn open_signed_profile_sync_settings_snapshot(
    bytes: &[u8],
    content_key: &ProfileSyncContentKey,
    public_key: &ProfileSyncDevicePublicKey,
    profile: &str,
    key_id: &str,
) -> Result<ProfileSyncSettingsSnapshot, SyncObjectError> {
    let payload = open_signed_encrypted_sync_payload(
        bytes,
        content_key,
        public_key,
        profile,
        SYNC_DOMAIN_SETTINGS,
        PROFILE_SYNC_SETTINGS_SNAPSHOT_OBJECT_KIND,
        key_id,
    )?;
    serde_json::from_slice(payload.as_slice()).map_err(SyncObjectError::Decode)
}

pub fn open_signed_sync_setting_text(
    bytes: &[u8],
    content_key: &ProfileSyncContentKey,
    public_key: &ProfileSyncDevicePublicKey,
    profile: &str,
    domain: &str,
    key_id: &str,
) -> Result<IncomingSyncSettingText, SyncObjectError> {
    let payload = open_signed_encrypted_sync_payload(
        bytes,
        content_key,
        public_key,
        profile,
        domain,
        PROFILE_SYNC_SETTING_CHANGE_OBJECT_KIND,
        key_id,
    )?;
    serde_json::from_slice(payload.as_slice()).map_err(SyncObjectError::Decode)
}

pub fn open_signed_sync_setting_text_for_profile(
    bytes: &[u8],
    content_key: &ProfileSyncContentKey,
    public_key: &ProfileSyncDevicePublicKey,
    profile: &str,
    key_id: &str,
) -> Result<IncomingSyncSettingText, SyncObjectError> {
    let signed_object = SignedSyncObject::from_bytes(bytes)?;
    let encrypted_bytes = signed_object.verify_with(public_key)?;
    let encrypted_object = EncryptedSyncObject::from_bytes(encrypted_bytes)?;
    if encrypted_object.profile != profile {
        return Err(SyncObjectError::UnexpectedProfile {
            expected: profile.to_string(),
            actual: encrypted_object.profile.clone(),
        });
    }
    if encrypted_object.object_kind != PROFILE_SYNC_SETTING_CHANGE_OBJECT_KIND {
        return Err(SyncObjectError::UnexpectedObjectKind {
            expected: PROFILE_SYNC_SETTING_CHANGE_OBJECT_KIND.to_string(),
            actual: encrypted_object.object_kind.clone(),
        });
    }
    if encrypted_object.key_id != key_id {
        return Err(SyncObjectError::UnexpectedKeyId {
            expected: key_id.to_string(),
            actual: encrypted_object.key_id.clone(),
        });
    }

    let payload = encrypted_object.open(content_key)?;
    let change: IncomingSyncSettingText =
        serde_json::from_slice(payload.as_slice()).map_err(SyncObjectError::Decode)?;
    if change.profile != encrypted_object.profile {
        return Err(SyncObjectError::UnexpectedProfile {
            expected: encrypted_object.profile,
            actual: change.profile,
        });
    }
    if change.domain != encrypted_object.domain {
        return Err(SyncObjectError::UnexpectedDomain {
            expected: encrypted_object.domain,
            actual: change.domain,
        });
    }
    Ok(change)
}

pub fn open_signed_profile_sync_settings_manifest_objects(
    manifest_object: &ProfileSyncObjectBytes,
    snapshot_object: Option<&ProfileSyncObjectBytes>,
    tail_change_objects: &[ProfileSyncObjectBytes],
    content_key: &ProfileSyncContentKey,
    public_key: &ProfileSyncDevicePublicKey,
    profile: &str,
    key_id: &str,
) -> Result<VerifiedProfileSyncSettingsManifestObjects, SyncObjectError> {
    let manifest = open_signed_profile_sync_manifest(
        manifest_object.bytes.as_slice(),
        content_key,
        public_key,
        profile,
        key_id,
    )?;
    let snapshot = snapshot_object
        .map(|snapshot_object| {
            Ok(VerifiedProfileSyncSettingsSnapshot {
                object_id: snapshot_object.object_id.clone(),
                snapshot: open_signed_profile_sync_settings_snapshot(
                    snapshot_object.bytes.as_slice(),
                    content_key,
                    public_key,
                    profile,
                    key_id,
                )?,
            })
        })
        .transpose()?;
    let mut tail_changes = Vec::with_capacity(tail_change_objects.len());
    for tail_object in tail_change_objects {
        tail_changes.push(VerifiedProfileSyncSettingsTailChange {
            object_id: tail_object.object_id.clone(),
            change: open_signed_sync_setting_text_for_profile(
                tail_object.bytes.as_slice(),
                content_key,
                public_key,
                profile,
                key_id,
            )?,
        });
    }

    Ok(VerifiedProfileSyncSettingsManifestObjects {
        manifest_object_id: manifest_object.object_id.clone(),
        manifest,
        snapshot,
        tail_changes,
    })
}

pub fn settings_sync_manifest_for_tail_changes(
    profile: &str,
    root_id: &str,
    tail_changes: &[ProfileSyncSettingsTailChangePublication],
    retention_policy: ProfileSyncRetentionPolicy,
) -> Result<ProfileSyncManifest, StorageError> {
    if root_id.is_empty() {
        return Err(StorageError::InvalidSyncRootId(root_id.to_string()));
    }
    if tail_changes.is_empty() {
        return Err(StorageError::InvalidProfileSyncManifest(
            "settings manifest tail is empty".to_string(),
        ));
    }

    let mut included_domains = Vec::with_capacity(tail_changes.len());
    let mut tail_change_object_ids = Vec::with_capacity(tail_changes.len());
    let mut device_frontiers: BTreeMap<String, ProfileSyncDeviceFrontier> = BTreeMap::new();
    let mut created_at = 0;
    for tail in tail_changes {
        if tail.object_id.is_empty() {
            return Err(StorageError::InvalidProfileSyncManifest(
                "settings manifest tail object id is empty".to_string(),
            ));
        }
        if tail.change.profile != profile {
            return Err(StorageError::InvalidProfileSyncManifest(format!(
                "tail change {} profile {} does not match manifest profile {}",
                tail.object_id, tail.change.profile, profile
            )));
        }
        if tail.change.operation != "set_text" {
            return Err(StorageError::InvalidProfileSyncManifest(format!(
                "tail change {} operation {} is not supported by settings manifests",
                tail.object_id, tail.change.operation
            )));
        }

        included_domains.push(tail.change.domain.clone());
        tail_change_object_ids.push(tail.object_id.clone());
        created_at = created_at.max(tail.change.created_at);
        let next_frontier = ProfileSyncDeviceFrontier {
            device_id: tail.change.device_id.clone(),
            latest_sequence: tail.change.device_sequence,
            latest_change_object_id: Some(tail.object_id.clone()),
        };
        match device_frontiers.get(tail.change.device_id.as_str()) {
            Some(existing)
                if (
                    existing.latest_sequence,
                    existing.latest_change_object_id.as_deref(),
                ) >= (
                    next_frontier.latest_sequence,
                    next_frontier.latest_change_object_id.as_deref(),
                ) => {}
            _ => {
                device_frontiers.insert(tail.change.device_id.clone(), next_frontier);
            }
        }
    }
    included_domains.sort();
    included_domains.dedup();

    Ok(ProfileSyncManifest {
        profile: profile.to_string(),
        root_id: root_id.to_string(),
        schema_version: PROFILE_SYNC_MANIFEST_SCHEMA_VERSION,
        membership_epoch: DEFAULT_PROFILE_SYNC_MEMBERSHIP_EPOCH,
        current_snapshot_object_id: None,
        tail_change_object_ids,
        included_domains,
        device_frontiers: device_frontiers.into_values().collect(),
        retention_policy,
        created_at,
    })
}

pub fn settings_sync_manifest_for_snapshot_and_tail_changes(
    profile: &str,
    root_id: &str,
    snapshot: &ProfileSyncSettingsSnapshotPublication,
    tail_changes: &[ProfileSyncSettingsTailChangePublication],
    retention_policy: ProfileSyncRetentionPolicy,
) -> Result<ProfileSyncManifest, StorageError> {
    if root_id.is_empty() {
        return Err(StorageError::InvalidSyncRootId(root_id.to_string()));
    }
    if snapshot.object_id.is_empty() {
        return Err(StorageError::InvalidProfileSyncManifest(
            "settings manifest snapshot object id is empty".to_string(),
        ));
    }
    if snapshot.snapshot.profile != profile {
        return Err(StorageError::InvalidProfileSyncManifest(format!(
            "snapshot profile {} does not match manifest profile {}",
            snapshot.snapshot.profile, profile
        )));
    }
    if snapshot.snapshot.schema_version != PROFILE_SYNC_SETTINGS_SNAPSHOT_SCHEMA_VERSION {
        return Err(StorageError::UnsupportedSyncSnapshotSchema(
            snapshot.snapshot.schema_version,
        ));
    }

    let mut included_domains =
        normalized_snapshot_domains(snapshot.snapshot.included_domains.as_slice());
    let mut tail_change_object_ids = Vec::with_capacity(tail_changes.len());
    let mut device_frontiers: BTreeMap<String, ProfileSyncDeviceFrontier> = BTreeMap::new();
    let mut created_at = snapshot.snapshot.created_at;
    for change in &snapshot.covered_changes {
        if change.profile != profile {
            return Err(StorageError::InvalidProfileSyncManifest(format!(
                "snapshot-covered change {} profile {} does not match manifest profile {}",
                change.id, change.profile, profile
            )));
        }
        if change.operation != "set_text" {
            return Err(StorageError::InvalidProfileSyncManifest(format!(
                "snapshot-covered change {} operation {} is not supported by settings manifests",
                change.id, change.operation
            )));
        }
        if !included_domains.contains(&change.domain) {
            return Err(StorageError::InvalidProfileSyncManifest(format!(
                "snapshot-covered change {} domain {} is not included in snapshot",
                change.id, change.domain
            )));
        }
        upsert_device_frontier(
            &mut device_frontiers,
            ProfileSyncDeviceFrontier {
                device_id: change.device_id.clone(),
                latest_sequence: change.device_sequence,
                latest_change_object_id: None,
            },
        );
    }

    for tail in tail_changes {
        if tail.object_id.is_empty() {
            return Err(StorageError::InvalidProfileSyncManifest(
                "settings manifest tail object id is empty".to_string(),
            ));
        }
        if tail.change.profile != profile {
            return Err(StorageError::InvalidProfileSyncManifest(format!(
                "tail change {} profile {} does not match manifest profile {}",
                tail.object_id, tail.change.profile, profile
            )));
        }
        if tail.change.operation != "set_text" {
            return Err(StorageError::InvalidProfileSyncManifest(format!(
                "tail change {} operation {} is not supported by settings manifests",
                tail.object_id, tail.change.operation
            )));
        }

        included_domains.push(tail.change.domain.clone());
        tail_change_object_ids.push(tail.object_id.clone());
        created_at = created_at.max(tail.change.created_at);
        upsert_device_frontier(
            &mut device_frontiers,
            ProfileSyncDeviceFrontier {
                device_id: tail.change.device_id.clone(),
                latest_sequence: tail.change.device_sequence,
                latest_change_object_id: Some(tail.object_id.clone()),
            },
        );
    }
    included_domains.sort();
    included_domains.dedup();

    Ok(ProfileSyncManifest {
        profile: profile.to_string(),
        root_id: root_id.to_string(),
        schema_version: PROFILE_SYNC_MANIFEST_SCHEMA_VERSION,
        membership_epoch: DEFAULT_PROFILE_SYNC_MEMBERSHIP_EPOCH,
        current_snapshot_object_id: Some(snapshot.object_id.clone()),
        tail_change_object_ids,
        included_domains,
        device_frontiers: device_frontiers.into_values().collect(),
        retention_policy,
        created_at,
    })
}

fn upsert_device_frontier(
    frontiers: &mut BTreeMap<String, ProfileSyncDeviceFrontier>,
    next: ProfileSyncDeviceFrontier,
) {
    match frontiers.get(next.device_id.as_str()) {
        Some(existing)
            if (
                existing.latest_sequence,
                existing.latest_change_object_id.as_deref(),
            ) >= (
                next.latest_sequence,
                next.latest_change_object_id.as_deref(),
            ) => {}
        _ => {
            frontiers.insert(next.device_id.clone(), next);
        }
    }
}

pub trait ProfileSyncObjectSource {
    type Error;

    fn resolve_profile_sync_root(
        &self,
        profile: &str,
        root_id: &str,
    ) -> Result<Option<String>, Self::Error>;

    fn list_profile_sync_root_candidates(
        &self,
        profile: &str,
        root_id: &str,
    ) -> Result<Vec<ProfileSyncRootCandidate>, Self::Error> {
        Ok(self
            .resolve_profile_sync_root(profile, root_id)?
            .map(ProfileSyncRootCandidate::resolved_root)
            .into_iter()
            .collect())
    }

    fn get_profile_sync_object(
        &self,
        profile: &str,
        object_id: &str,
    ) -> Result<ProfileSyncObjectBytes, Self::Error>;
}

#[derive(Debug)]
pub enum ProfileSyncPullError<SourceError> {
    Source(SourceError),
    SyncObject(SyncObjectError),
    ObjectIdMismatch { expected: String, actual: String },
}

impl<SourceError: fmt::Display> fmt::Display for ProfileSyncPullError<SourceError> {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Source(error) => write!(formatter, "profile sync object source failed: {error}"),
            Self::SyncObject(error) => write!(formatter, "profile sync object failed: {error}"),
            Self::ObjectIdMismatch { expected, actual } => write!(
                formatter,
                "profile sync source returned object id {actual}, expected {expected}"
            ),
        }
    }
}

impl<SourceError> std::error::Error for ProfileSyncPullError<SourceError>
where
    SourceError: std::error::Error + 'static,
{
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Self::Source(error) => Some(error),
            Self::SyncObject(error) => Some(error),
            Self::ObjectIdMismatch { .. } => None,
        }
    }
}

#[derive(Debug)]
pub enum ProfileSyncPullApplyError<SourceError> {
    Pull(ProfileSyncPullError<SourceError>),
    Storage(StorageError),
}

impl<SourceError: fmt::Display> fmt::Display for ProfileSyncPullApplyError<SourceError> {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Pull(error) => write!(formatter, "failed to pull profile sync data: {error}"),
            Self::Storage(error) => write!(formatter, "failed to apply profile sync data: {error}"),
        }
    }
}

impl<SourceError> std::error::Error for ProfileSyncPullApplyError<SourceError>
where
    SourceError: std::error::Error + 'static,
{
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Self::Pull(error) => Some(error),
            Self::Storage(error) => Some(error),
        }
    }
}

#[derive(Debug)]
pub enum ProfileSyncTrustedOpenError {
    Storage(StorageError),
    SyncObject(SyncObjectError),
    UntrustedDevice {
        profile: String,
        device_id: String,
    },
    ProviderAuthoritySigner {
        profile: String,
        device_id: String,
    },
    UnauthorizedDeviceEpoch {
        profile: String,
        device_id: String,
        key_membership_epoch: i64,
        manifest_membership_epoch: i64,
    },
}

impl fmt::Display for ProfileSyncTrustedOpenError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Storage(error) => {
                write!(
                    formatter,
                    "failed to read trusted profile sync key: {error}"
                )
            }
            Self::SyncObject(error) => write!(formatter, "profile sync object failed: {error}"),
            Self::UntrustedDevice { profile, device_id } => write!(
                formatter,
                "profile {profile} has no trusted public key for sync device {device_id}"
            ),
            Self::ProviderAuthoritySigner { profile, device_id } => write!(
                formatter,
                "profile {profile} sync device {device_id} is marked as provider authority and cannot authorize profile sync state"
            ),
            Self::UnauthorizedDeviceEpoch {
                profile,
                device_id,
                key_membership_epoch,
                manifest_membership_epoch,
            } => write!(
                formatter,
                "profile {profile} sync device {device_id} was trusted at membership epoch {key_membership_epoch}, after manifest epoch {manifest_membership_epoch}"
            ),
        }
    }
}

impl std::error::Error for ProfileSyncTrustedOpenError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Self::Storage(error) => Some(error),
            Self::SyncObject(error) => Some(error),
            Self::UntrustedDevice { .. }
            | Self::ProviderAuthoritySigner { .. }
            | Self::UnauthorizedDeviceEpoch { .. } => None,
        }
    }
}

#[derive(Debug)]
pub enum ProfileSyncTrustedPullError<SourceError> {
    Source(SourceError),
    Open(ProfileSyncTrustedOpenError),
    ObjectIdMismatch { expected: String, actual: String },
}

impl<SourceError: fmt::Display> fmt::Display for ProfileSyncTrustedPullError<SourceError> {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Source(error) => write!(formatter, "profile sync object source failed: {error}"),
            Self::Open(error) => write!(formatter, "trusted profile sync object failed: {error}"),
            Self::ObjectIdMismatch { expected, actual } => write!(
                formatter,
                "profile sync source returned object id {actual}, expected {expected}"
            ),
        }
    }
}

impl<SourceError> std::error::Error for ProfileSyncTrustedPullError<SourceError>
where
    SourceError: std::error::Error + 'static,
{
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Self::Source(error) => Some(error),
            Self::Open(error) => Some(error),
            Self::ObjectIdMismatch { .. } => None,
        }
    }
}

#[derive(Debug)]
pub enum ProfileSyncTrustedPullApplyError<SourceError> {
    Pull(ProfileSyncTrustedPullError<SourceError>),
    Storage(StorageError),
}

impl<SourceError: fmt::Display> fmt::Display for ProfileSyncTrustedPullApplyError<SourceError> {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Pull(error) => {
                write!(
                    formatter,
                    "failed to pull trusted profile sync data: {error}"
                )
            }
            Self::Storage(error) => write!(formatter, "failed to apply profile sync data: {error}"),
        }
    }
}

impl<SourceError> std::error::Error for ProfileSyncTrustedPullApplyError<SourceError>
where
    SourceError: std::error::Error + 'static,
{
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Self::Pull(error) => Some(error),
            Self::Storage(error) => Some(error),
        }
    }
}

pub fn pull_signed_profile_sync_settings_manifest_objects<Source>(
    source: &Source,
    profile: &str,
    root_id: &str,
    content_key: &ProfileSyncContentKey,
    public_key: &ProfileSyncDevicePublicKey,
    key_id: &str,
) -> Result<Option<VerifiedProfileSyncSettingsManifestObjects>, ProfileSyncPullError<Source::Error>>
where
    Source: ProfileSyncObjectSource,
{
    let Some(manifest_object_id) = source
        .resolve_profile_sync_root(profile, root_id)
        .map_err(ProfileSyncPullError::Source)?
    else {
        return Ok(None);
    };

    let manifest_object = fetch_profile_sync_object(source, profile, manifest_object_id.as_str())?;
    let manifest = open_signed_profile_sync_manifest(
        manifest_object.bytes.as_slice(),
        content_key,
        public_key,
        profile,
        key_id,
    )
    .map_err(ProfileSyncPullError::SyncObject)?;
    let snapshot_object = manifest
        .current_snapshot_object_id
        .as_deref()
        .map(|object_id| fetch_profile_sync_object(source, profile, object_id))
        .transpose()?;
    let mut tail_change_objects = Vec::with_capacity(manifest.tail_change_object_ids.len());
    for object_id in &manifest.tail_change_object_ids {
        tail_change_objects.push(fetch_profile_sync_object(source, profile, object_id)?);
    }

    open_signed_profile_sync_settings_manifest_objects(
        &manifest_object,
        snapshot_object.as_ref(),
        tail_change_objects.as_slice(),
        content_key,
        public_key,
        profile,
        key_id,
    )
    .map(Some)
    .map_err(ProfileSyncPullError::SyncObject)
}

pub fn pull_signed_profile_sync_device_head<Source>(
    source: &Source,
    profile: &str,
    root_id: &str,
    content_key: &ProfileSyncContentKey,
    public_key: &ProfileSyncDevicePublicKey,
    key_id: &str,
) -> Result<Option<VerifiedProfileSyncDeviceHead>, ProfileSyncPullError<Source::Error>>
where
    Source: ProfileSyncObjectSource,
{
    let Some(device_head_object_id) = source
        .resolve_profile_sync_root(profile, root_id)
        .map_err(ProfileSyncPullError::Source)?
    else {
        return Ok(None);
    };

    let device_head_object =
        fetch_profile_sync_object(source, profile, device_head_object_id.as_str())?;
    let device_head = open_signed_profile_sync_device_head(
        device_head_object.bytes.as_slice(),
        content_key,
        public_key,
        profile,
        key_id,
    )
    .map_err(ProfileSyncPullError::SyncObject)?;
    validate_profile_sync_device_head_root(&device_head, root_id)
        .map_err(ProfileSyncPullError::SyncObject)?;
    Ok(Some(VerifiedProfileSyncDeviceHead {
        object_id: device_head_object.object_id,
        device_head,
    }))
}

fn fetch_profile_sync_object<Source>(
    source: &Source,
    profile: &str,
    object_id: &str,
) -> Result<ProfileSyncObjectBytes, ProfileSyncPullError<Source::Error>>
where
    Source: ProfileSyncObjectSource,
{
    let object = source
        .get_profile_sync_object(profile, object_id)
        .map_err(ProfileSyncPullError::Source)?;
    if object.object_id != object_id {
        return Err(ProfileSyncPullError::ObjectIdMismatch {
            expected: object_id.to_string(),
            actual: object.object_id,
        });
    }
    Ok(object)
}

fn fetch_trusted_profile_sync_object<Source>(
    source: &Source,
    profile: &str,
    object_id: &str,
) -> Result<ProfileSyncObjectBytes, ProfileSyncTrustedPullError<Source::Error>>
where
    Source: ProfileSyncObjectSource,
{
    let object = source
        .get_profile_sync_object(profile, object_id)
        .map_err(ProfileSyncTrustedPullError::Source)?;
    if object.object_id != object_id {
        return Err(ProfileSyncTrustedPullError::ObjectIdMismatch {
            expected: object_id.to_string(),
            actual: object.object_id,
        });
    }
    Ok(object)
}

fn validate_profile_sync_device_head_root(
    device_head: &ProfileSyncDeviceHead,
    root_id: &str,
) -> Result<(), SyncObjectError> {
    if device_head.root_id == root_id {
        Ok(())
    } else {
        Err(SyncObjectError::UnexpectedRootId {
            expected: root_id.to_string(),
            actual: device_head.root_id.clone(),
        })
    }
}

fn validate_profile_sync_device_head_manifest(
    device_head: &ProfileSyncDeviceHead,
    manifest: &ProfileSyncManifest,
) -> Result<(), SyncObjectError> {
    if manifest.membership_epoch != device_head.membership_epoch {
        return Err(SyncObjectError::UnexpectedDeviceHeadManifestEpoch {
            device_id: device_head.device_id.clone(),
            head_membership_epoch: device_head.membership_epoch,
            manifest_membership_epoch: manifest.membership_epoch,
        });
    }

    let frontier = manifest
        .device_frontiers
        .iter()
        .find(|frontier| frontier.device_id == device_head.device_id);
    let Some(frontier) = frontier else {
        return Err(SyncObjectError::UnexpectedDeviceFrontier {
            device_id: device_head.device_id.clone(),
            expected_sequence: device_head.device_sequence,
            actual_sequence: None,
            expected_change_object_id: device_head.latest_change_object_id.clone(),
            actual_change_object_id: None,
        });
    };

    if frontier.latest_sequence != device_head.device_sequence
        || frontier.latest_change_object_id != device_head.latest_change_object_id
    {
        return Err(SyncObjectError::UnexpectedDeviceFrontier {
            device_id: device_head.device_id.clone(),
            expected_sequence: device_head.device_sequence,
            actual_sequence: Some(frontier.latest_sequence),
            expected_change_object_id: device_head.latest_change_object_id.clone(),
            actual_change_object_id: frontier.latest_change_object_id.clone(),
        });
    }
    Ok(())
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct BookmarkUpdate {
    pub profile: String,
    pub url: String,
    pub title: Option<String>,
    pub folder: Option<String>,
    pub position: i64,
    pub favicon_key: Option<String>,
}

#[derive(Clone, Debug, Eq, PartialEq, Deserialize, Serialize)]
pub struct BookmarkSlotSyncPayload {
    pub url: String,
    pub title: Option<String>,
    pub folder: Option<String>,
    pub position: i64,
    pub favicon_key: Option<String>,
    pub replaced_url: Option<String>,
    #[serde(default)]
    pub deleted: bool,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct BookmarkRecord {
    pub profile: String,
    pub url: String,
    pub title: Option<String>,
    pub folder: Option<String>,
    pub position: i64,
    pub favicon_key: Option<String>,
    pub created_at: i64,
    pub updated_at: i64,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct DownloadMetadataUpdate {
    pub profile: String,
    pub download_id: String,
    pub source_url: String,
    pub final_url: String,
    pub route: Option<String>,
    pub transport_id: Option<String>,
    pub filename: String,
    pub content_type: Option<String>,
    pub size_bytes: u64,
    pub integrity: Option<String>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct DownloadMetadataRecord {
    pub profile: String,
    pub download_id: String,
    pub source_url: String,
    pub final_url: String,
    pub route: Option<String>,
    pub transport_id: Option<String>,
    pub filename: String,
    pub content_type: Option<String>,
    pub size_bytes: u64,
    pub integrity: Option<String>,
    pub created_at: i64,
    pub updated_at: i64,
}

#[derive(Clone, Debug, Eq, PartialEq, Deserialize, Serialize)]
pub struct DownloadMetadataSyncPayload {
    pub download_id: String,
    pub source_url: String,
    pub final_url: String,
    pub route: Option<String>,
    pub transport_id: Option<String>,
    pub filename: String,
    pub content_type: Option<String>,
    pub size_bytes: u64,
    pub integrity: Option<String>,
    #[serde(default)]
    pub deleted: bool,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct CalendarEventUpdate {
    pub profile: String,
    pub event_id: String,
    pub calendar_id: Option<String>,
    pub title: String,
    pub starts_at: i64,
    pub ends_at: Option<i64>,
    pub time_zone: Option<String>,
    pub location: Option<String>,
    pub notes: Option<String>,
    pub recurrence_rule: Option<String>,
    pub reminder_minutes: Option<i64>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct CalendarEventRecord {
    pub profile: String,
    pub event_id: String,
    pub calendar_id: Option<String>,
    pub title: String,
    pub starts_at: i64,
    pub ends_at: Option<i64>,
    pub time_zone: Option<String>,
    pub location: Option<String>,
    pub notes: Option<String>,
    pub recurrence_rule: Option<String>,
    pub reminder_minutes: Option<i64>,
    pub created_at: i64,
    pub updated_at: i64,
}

#[derive(Clone, Debug, Eq, PartialEq, Deserialize, Serialize)]
pub struct CalendarEventSyncPayload {
    pub event_id: String,
    pub calendar_id: Option<String>,
    pub title: String,
    pub starts_at: i64,
    pub ends_at: Option<i64>,
    pub time_zone: Option<String>,
    pub location: Option<String>,
    pub notes: Option<String>,
    pub recurrence_rule: Option<String>,
    pub reminder_minutes: Option<i64>,
    #[serde(default)]
    pub deleted: bool,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ChatConversationUpdate {
    pub profile: String,
    pub conversation_id: String,
    pub provider_id: Option<String>,
    pub external_thread_id: Option<String>,
    pub display_name: String,
    pub avatar_key: Option<String>,
    pub last_message_at: Option<i64>,
    pub unread_count: u32,
    pub archived: bool,
    pub muted: bool,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ChatConversationRecord {
    pub profile: String,
    pub conversation_id: String,
    pub provider_id: Option<String>,
    pub external_thread_id: Option<String>,
    pub display_name: String,
    pub avatar_key: Option<String>,
    pub last_message_at: Option<i64>,
    pub unread_count: u32,
    pub archived: bool,
    pub muted: bool,
    pub created_at: i64,
    pub updated_at: i64,
}

#[derive(Clone, Debug, Eq, PartialEq, Deserialize, Serialize)]
pub struct ChatConversationSyncPayload {
    pub conversation_id: String,
    pub provider_id: Option<String>,
    pub external_thread_id: Option<String>,
    pub display_name: String,
    pub avatar_key: Option<String>,
    pub last_message_at: Option<i64>,
    pub unread_count: u32,
    pub archived: bool,
    pub muted: bool,
    #[serde(default)]
    pub deleted: bool,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ContactCardUpdate {
    pub profile: String,
    pub contact_id: String,
    pub display_name: String,
    pub given_name: Option<String>,
    pub family_name: Option<String>,
    pub organization: Option<String>,
    pub primary_email: Option<String>,
    pub primary_phone: Option<String>,
    pub notes: Option<String>,
    pub avatar_key: Option<String>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ContactCardRecord {
    pub profile: String,
    pub contact_id: String,
    pub display_name: String,
    pub given_name: Option<String>,
    pub family_name: Option<String>,
    pub organization: Option<String>,
    pub primary_email: Option<String>,
    pub primary_phone: Option<String>,
    pub notes: Option<String>,
    pub avatar_key: Option<String>,
    pub created_at: i64,
    pub updated_at: i64,
}

#[derive(Clone, Debug, Eq, PartialEq, Deserialize, Serialize)]
pub struct ContactCardSyncPayload {
    pub contact_id: String,
    pub display_name: String,
    pub given_name: Option<String>,
    pub family_name: Option<String>,
    pub organization: Option<String>,
    pub primary_email: Option<String>,
    pub primary_phone: Option<String>,
    pub notes: Option<String>,
    pub avatar_key: Option<String>,
    #[serde(default)]
    pub deleted: bool,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct FileEntryUpdate {
    pub profile: String,
    pub entry_id: String,
    pub sync_set_id: Option<String>,
    pub parent_id: Option<String>,
    pub name: String,
    pub entry_kind: String,
    pub content_ref: Option<String>,
    pub mime_type: Option<String>,
    pub size_bytes: Option<u64>,
    pub modified_at: Option<i64>,
    pub integrity: Option<String>,
    pub retention_policy: Option<String>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct FileEntryRecord {
    pub profile: String,
    pub entry_id: String,
    pub sync_set_id: Option<String>,
    pub parent_id: Option<String>,
    pub name: String,
    pub entry_kind: String,
    pub content_ref: Option<String>,
    pub mime_type: Option<String>,
    pub size_bytes: Option<u64>,
    pub modified_at: Option<i64>,
    pub integrity: Option<String>,
    pub retention_policy: Option<String>,
    pub created_at: i64,
    pub updated_at: i64,
}

#[derive(Clone, Debug, Eq, PartialEq, Deserialize, Serialize)]
pub struct FileEntrySyncPayload {
    pub entry_id: String,
    pub sync_set_id: Option<String>,
    pub parent_id: Option<String>,
    pub name: String,
    pub entry_kind: String,
    pub content_ref: Option<String>,
    pub mime_type: Option<String>,
    pub size_bytes: Option<u64>,
    pub modified_at: Option<i64>,
    pub integrity: Option<String>,
    pub retention_policy: Option<String>,
    #[serde(default)]
    pub deleted: bool,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct StorageProviderUpdate {
    pub profile: String,
    pub provider_id: String,
    pub provider_kind: String,
    pub display_name: String,
    pub endpoint_ref: Option<String>,
    pub discovery: bool,
    pub connectivity: bool,
    pub object_transfer: bool,
    pub availability: bool,
    pub mutable_roots: bool,
    pub quota_bytes: Option<u64>,
    pub max_retained_objects: Option<u32>,
    pub pinning_policy: Option<String>,
    pub enabled: bool,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct StorageProviderRecord {
    pub profile: String,
    pub provider_id: String,
    pub provider_kind: String,
    pub display_name: String,
    pub endpoint_ref: Option<String>,
    pub discovery: bool,
    pub connectivity: bool,
    pub object_transfer: bool,
    pub availability: bool,
    pub mutable_roots: bool,
    pub quota_bytes: Option<u64>,
    pub max_retained_objects: Option<u32>,
    pub pinning_policy: Option<String>,
    pub enabled: bool,
    pub created_at: i64,
    pub updated_at: i64,
}

#[derive(Clone, Debug, Eq, PartialEq, Deserialize, Serialize)]
pub struct StorageProviderSyncPayload {
    pub provider_id: String,
    pub provider_kind: String,
    pub display_name: String,
    pub endpoint_ref: Option<String>,
    pub discovery: bool,
    pub connectivity: bool,
    pub object_transfer: bool,
    pub availability: bool,
    pub mutable_roots: bool,
    pub quota_bytes: Option<u64>,
    pub max_retained_objects: Option<u32>,
    pub pinning_policy: Option<String>,
    pub enabled: bool,
    #[serde(default)]
    pub deleted: bool,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct CookieUpdate {
    pub profile: String,
    pub domain: String,
    pub path: String,
    pub name: String,
    pub value: String,
    pub expires_at: Option<i64>,
    pub is_secure: bool,
    pub is_http_only: bool,
    pub same_site: Option<String>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct CookieRecord {
    pub profile: String,
    pub domain: String,
    pub path: String,
    pub name: String,
    pub value: String,
    pub expires_at: Option<i64>,
    pub is_secure: bool,
    pub is_http_only: bool,
    pub same_site: Option<String>,
    pub created_at: i64,
    pub updated_at: i64,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct HistoryVisitRecord {
    pub profile: String,
    pub url: String,
    pub title: Option<String>,
    pub first_visited_at: i64,
    pub last_visited_at: i64,
    pub visit_count: i64,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct BinaryBlobRecord {
    pub profile: String,
    pub key: String,
    pub media_type: Option<String>,
    pub data: Vec<u8>,
    pub created_at: i64,
    pub updated_at: i64,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct AppSyncDomainRegistration {
    pub profile: String,
    pub domain: String,
    pub schema_version: i64,
    pub enabled: bool,
    pub privacy_classification: String,
    pub sync_content: bool,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct AppSyncDomainRecord {
    pub profile: String,
    pub domain: String,
    pub schema_version: i64,
    pub enabled: bool,
    pub privacy_classification: String,
    pub sync_content: bool,
    pub created_at: i64,
    pub updated_at: i64,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct AppSyncDomainCursorRecord {
    pub profile: String,
    pub domain: String,
    pub latest_revision: i64,
    pub updated_at: i64,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct SyncDeviceRegistration {
    pub profile: String,
    pub device_id: String,
    pub label: Option<String>,
    pub membership_epoch: i64,
    pub provider_authority: bool,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct SyncDeviceRecord {
    pub profile: String,
    pub device_id: String,
    pub label: Option<String>,
    pub membership_epoch: i64,
    pub provider_authority: bool,
    pub created_at: i64,
    pub last_seen_at: i64,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct SyncDevicePublicKeyRegistration {
    pub profile: String,
    pub public_key: ProfileSyncDevicePublicKey,
    pub membership_epoch: i64,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct SyncDevicePublicKeyRecord {
    pub profile: String,
    pub public_key: ProfileSyncDevicePublicKey,
    pub membership_epoch: i64,
    pub trusted: bool,
    pub created_at: i64,
    pub updated_at: i64,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct SyncAccountMembershipRecordRegistration {
    pub profile: String,
    pub record_id: String,
    pub membership_epoch: i64,
    pub record_kind: String,
    pub device_id: String,
    pub signer_device_id: String,
    pub signed_record: Vec<u8>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct SyncAccountMembershipRecord {
    pub profile: String,
    pub record_id: String,
    pub membership_epoch: i64,
    pub record_kind: String,
    pub device_id: String,
    pub signer_device_id: String,
    pub signed_record: Vec<u8>,
    pub created_at: i64,
    pub applied_at: Option<i64>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct SyncAccountMembershipRecordApplication {
    pub membership_record: SyncAccountMembershipRecord,
    pub device_key: Option<SyncDevicePublicKeyRecord>,
    pub bootstrapped: bool,
    pub applied: bool,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct SyncContentKeyEpochRegistration {
    pub profile: String,
    pub key_id: String,
    pub membership_epoch: i64,
    pub algorithm: String,
    pub active: bool,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct SyncContentKeyEpochRecord {
    pub profile: String,
    pub key_id: String,
    pub membership_epoch: i64,
    pub algorithm: String,
    pub active: bool,
    pub created_at: i64,
    pub updated_at: i64,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ProfileSyncLocalActivationRecord {
    pub profile: String,
    pub device_id: String,
    pub content_key_epoch: SyncContentKeyEpochRecord,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ProfileSyncLocalSecretActivationRecord {
    pub activation: ProfileSyncLocalActivationRecord,
    pub account_authority_device_id: String,
    pub local_device_id: String,
    pub membership_applications: Vec<SyncAccountMembershipRecordApplication>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ProfileSyncSecretHandoffApplication {
    pub enrollment_applications: Vec<SyncAccountMembershipRecordApplication>,
    pub activation: ProfileSyncLocalSecretActivationRecord,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ProfileSyncPreviewProviderActivationRecord {
    pub provider: StorageProviderRecord,
    pub membership_application: SyncAccountMembershipRecordApplication,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ProfileSyncLocalReadinessReport {
    pub profile: String,
    pub local_device_id: String,
    pub local_device_registered: bool,
    pub local_device_trusted: bool,
    pub account_authority_trusted: bool,
    pub trusted_device_count: usize,
    pub metadata_ready: bool,
    pub active_key_id: Option<String>,
    pub app_domain_count: usize,
    pub enabled_app_domain_count: usize,
    pub storage_provider_count: usize,
    pub enabled_storage_provider_count: usize,
    pub retention_capable_provider_count: usize,
    pub authorized_retention_provider_count: usize,
    pub ready_for_manual_sync: bool,
    pub blocked_reason: Option<String>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct SyncSettingValueRecord {
    pub profile: String,
    pub domain: String,
    pub key: String,
    pub value: String,
    pub value_kind: String,
    pub revision: i64,
    pub updated_at: i64,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct SyncChangeRecord {
    pub id: i64,
    pub profile: String,
    pub domain: String,
    pub entity_key: String,
    pub operation: String,
    pub payload: String,
    pub device_id: String,
    pub device_sequence: i64,
    pub logical_clock: i64,
    pub created_at: i64,
    pub applied_at: Option<i64>,
}

#[derive(Clone, Debug, Eq, PartialEq, Deserialize, Serialize)]
pub struct IncomingSyncSettingText {
    pub profile: String,
    pub domain: String,
    pub key: String,
    pub value: String,
    pub device_id: String,
    pub device_sequence: i64,
    pub logical_clock: i64,
}

impl IncomingSyncSettingText {
    pub fn new(
        profile: impl Into<String>,
        domain: impl Into<String>,
        key: impl Into<String>,
        value: impl Into<String>,
        device_id: impl Into<String>,
        device_sequence: i64,
        logical_clock: i64,
    ) -> Self {
        Self {
            profile: profile.into(),
            domain: domain.into(),
            key: key.into(),
            value: value.into(),
            device_id: device_id.into(),
            device_sequence,
            logical_clock,
        }
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct SyncRevisionRecord {
    pub revision: i64,
    pub profile: String,
    pub domain: String,
    pub change_id: i64,
    pub created_at: i64,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct SyncSettingTextEvent {
    pub revision: SyncRevisionRecord,
    pub change: SyncChangeRecord,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct SyncSettingTextDomainPoll {
    pub profile: String,
    pub domain: String,
    pub previous_revision: i64,
    pub latest_revision: i64,
    pub events: Vec<SyncSettingTextEvent>,
}

impl SyncSettingTextDomainPoll {
    pub fn advanced(&self) -> bool {
        self.latest_revision > self.previous_revision
    }

    pub fn event_count(&self) -> usize {
        self.events.len()
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct TypedSyncSettingTextEvent<T> {
    pub revision: SyncRevisionRecord,
    pub change: SyncChangeRecord,
    pub value: T,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct TypedSyncSettingTextDomainPoll<T> {
    pub profile: String,
    pub domain: String,
    pub previous_revision: i64,
    pub latest_revision: i64,
    pub events: Vec<TypedSyncSettingTextEvent<T>>,
}

impl<T> TypedSyncSettingTextDomainPoll<T> {
    pub fn advanced(&self) -> bool {
        self.latest_revision > self.previous_revision
    }

    pub fn event_count(&self) -> usize {
        self.events.len()
    }
}

#[derive(Clone, Debug)]
pub struct AppSyncDomainWatcher {
    database: SlateProfileDatabase,
    profile: String,
    domain: String,
    batch_limit: u32,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct AppSyncDomainWatcherApply {
    pub poll: SyncSettingTextDomainPoll,
    pub cursor: AppSyncDomainCursorRecord,
}

#[derive(Debug)]
pub enum AppSyncDomainWatcherApplyError<E> {
    Storage(StorageError),
    Apply(E),
}

impl<E> From<StorageError> for AppSyncDomainWatcherApplyError<E> {
    fn from(error: StorageError) -> Self {
        Self::Storage(error)
    }
}

impl AppSyncDomainWatcher {
    pub fn new(
        database: SlateProfileDatabase,
        profile: impl Into<String>,
        domain: impl Into<String>,
        batch_limit: u32,
    ) -> Result<Self, StorageError> {
        let profile = profile.into();
        let domain = domain.into();
        let batch_limit = batch_limit.max(1);
        database.ensure_app_sync_domain_cursor_at_domain_head(profile.as_str(), domain.as_str())?;

        Ok(Self {
            database,
            profile,
            domain,
            batch_limit,
        })
    }

    pub fn profile(&self) -> &str {
        self.profile.as_str()
    }

    pub fn domain(&self) -> &str {
        self.domain.as_str()
    }

    pub fn batch_limit(&self) -> u32 {
        self.batch_limit
    }

    pub fn current_revision(&self) -> Result<i64, StorageError> {
        Ok(self
            .database
            .app_sync_domain_cursor(self.profile.as_str(), self.domain.as_str())?
            .map(|cursor| cursor.latest_revision)
            .unwrap_or(0))
    }

    pub fn poll_once(&self) -> Result<SyncSettingTextDomainPoll, StorageError> {
        self.database.poll_sync_setting_text_events_for_app_domain(
            self.profile.as_str(),
            self.domain.as_str(),
            self.batch_limit,
        )
    }

    pub fn acknowledge(
        &self,
        poll: &SyncSettingTextDomainPoll,
    ) -> Result<AppSyncDomainCursorRecord, StorageError> {
        self.database.record_app_sync_domain_poll_cursor(poll)
    }

    pub fn poll_apply_and_acknowledge<E>(
        &self,
        apply: impl FnOnce(&SyncSettingTextDomainPoll) -> Result<(), E>,
    ) -> Result<AppSyncDomainWatcherApply, AppSyncDomainWatcherApplyError<E>> {
        let poll = self.poll_once()?;
        apply(&poll).map_err(AppSyncDomainWatcherApplyError::Apply)?;
        let cursor = self.acknowledge(&poll)?;
        Ok(AppSyncDomainWatcherApply { poll, cursor })
    }
}

#[derive(Clone, Debug)]
pub struct TypedAppSyncDomainWatcher<T> {
    database: SlateProfileDatabase,
    profile: String,
    domain: String,
    batch_limit: u32,
    payload: PhantomData<T>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct TypedAppSyncDomainWatcherApply<T> {
    pub poll: TypedSyncSettingTextDomainPoll<T>,
    pub cursor: AppSyncDomainCursorRecord,
}

#[derive(Debug)]
pub enum TypedAppSyncDomainWatcherApplyError<E> {
    Storage(StorageError),
    Apply(E),
}

impl<E> From<StorageError> for TypedAppSyncDomainWatcherApplyError<E> {
    fn from(error: StorageError) -> Self {
        Self::Storage(error)
    }
}

impl<T> TypedAppSyncDomainWatcher<T>
where
    T: DeserializeOwned,
{
    pub fn new(
        database: SlateProfileDatabase,
        profile: impl Into<String>,
        domain: impl Into<String>,
        batch_limit: u32,
    ) -> Result<Self, StorageError> {
        let profile = profile.into();
        let domain = domain.into();
        let batch_limit = batch_limit.max(1);
        database.ensure_app_sync_domain_cursor_at_domain_head(profile.as_str(), domain.as_str())?;

        Ok(Self {
            database,
            profile,
            domain,
            batch_limit,
            payload: PhantomData,
        })
    }

    pub fn profile(&self) -> &str {
        self.profile.as_str()
    }

    pub fn domain(&self) -> &str {
        self.domain.as_str()
    }

    pub fn batch_limit(&self) -> u32 {
        self.batch_limit
    }

    pub fn current_revision(&self) -> Result<i64, StorageError> {
        Ok(self
            .database
            .app_sync_domain_cursor(self.profile.as_str(), self.domain.as_str())?
            .map(|cursor| cursor.latest_revision)
            .unwrap_or(0))
    }

    pub fn poll_once(&self) -> Result<TypedSyncSettingTextDomainPoll<T>, StorageError> {
        self.database
            .poll_typed_sync_setting_text_events_for_app_domain::<T>(
                self.profile.as_str(),
                self.domain.as_str(),
                self.batch_limit,
            )
    }

    pub fn acknowledge(
        &self,
        poll: &TypedSyncSettingTextDomainPoll<T>,
    ) -> Result<AppSyncDomainCursorRecord, StorageError> {
        self.database.record_typed_app_sync_domain_poll_cursor(poll)
    }

    pub fn poll_apply_and_acknowledge<E>(
        &self,
        apply: impl FnOnce(&TypedSyncSettingTextDomainPoll<T>) -> Result<(), E>,
    ) -> Result<TypedAppSyncDomainWatcherApply<T>, TypedAppSyncDomainWatcherApplyError<E>> {
        let poll = self.poll_once()?;
        apply(&poll).map_err(TypedAppSyncDomainWatcherApplyError::Apply)?;
        let cursor = self.acknowledge(&poll)?;
        Ok(TypedAppSyncDomainWatcherApply { poll, cursor })
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct SyncSnapshotRegistration {
    pub profile: String,
    pub snapshot_id: String,
    pub backend_object_id: Option<String>,
    pub covers_revision: i64,
    pub included_domains: Vec<String>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct SyncSnapshotRecord {
    pub profile: String,
    pub snapshot_id: String,
    pub backend_object_id: Option<String>,
    pub covers_revision: i64,
    pub included_domains: Vec<String>,
    pub created_at: i64,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct SyncCompactionTarget {
    pub profile: String,
    pub previous_snapshot_covers_revision: i64,
    pub covers_revision: i64,
    pub covers_change_id: i64,
    pub covered_change_count: usize,
    pub retained_tail_change_count: usize,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ProfileSyncObjectBytes {
    pub object_id: String,
    pub bytes: Vec<u8>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ProfileSyncRootCandidate {
    pub publisher_id: String,
    pub object_id: String,
    pub publish_sequence: u64,
}

impl ProfileSyncRootCandidate {
    pub fn new(
        publisher_id: impl Into<String>,
        object_id: impl Into<String>,
        publish_sequence: u64,
    ) -> Self {
        Self {
            publisher_id: publisher_id.into(),
            object_id: object_id.into(),
            publish_sequence,
        }
    }

    fn resolved_root(object_id: impl Into<String>) -> Self {
        Self::new("resolved-root", object_id, 0)
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct VerifiedProfileSyncSettingsSnapshot {
    pub object_id: String,
    pub snapshot: ProfileSyncSettingsSnapshot,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct VerifiedProfileSyncSettingsTailChange {
    pub object_id: String,
    pub change: IncomingSyncSettingText,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ProfileSyncSettingsTailChangePublication {
    pub object_id: String,
    pub change: SyncChangeRecord,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ProfileSyncSettingsSnapshotPublication {
    pub object_id: String,
    pub snapshot: ProfileSyncSettingsSnapshot,
    pub covered_changes: Vec<SyncChangeRecord>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct VerifiedProfileSyncSettingsManifestObjects {
    pub manifest_object_id: String,
    pub manifest: ProfileSyncManifest,
    pub snapshot: Option<VerifiedProfileSyncSettingsSnapshot>,
    pub tail_changes: Vec<VerifiedProfileSyncSettingsTailChange>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct VerifiedProfileSyncSettingsManifestCandidate {
    pub root_candidate: ProfileSyncRootCandidate,
    pub objects: VerifiedProfileSyncSettingsManifestObjects,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ProfileSyncSettingsManifestCandidateApplication {
    pub root_candidate: ProfileSyncRootCandidate,
    pub application: ProfileSyncSettingsManifestApplication,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct VerifiedProfileSyncDeviceHead {
    pub object_id: String,
    pub device_head: ProfileSyncDeviceHead,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ProfileSyncSettingsManifestApplication {
    pub profile: String,
    pub root_id: String,
    pub manifest_object_id: String,
    pub sync_object_ids: Vec<String>,
    pub snapshot: Option<SyncSnapshotRecord>,
    pub snapshot_changes: Vec<SyncChangeRecord>,
    pub tail_changes: Vec<SyncChangeRecord>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ProfileSyncRootRecord {
    pub profile: String,
    pub root_id: String,
    pub object_id: String,
    pub updated_at: i64,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ProfileSyncRootRegistration {
    pub profile: String,
    pub root_id: String,
    pub object_id: String,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum ProfileSyncSettingsPullApplyStatus {
    NoPublishedRoot {
        profile: String,
        root_id: String,
    },
    Unchanged {
        profile: String,
        root_id: String,
        object_id: String,
    },
    Applied(ProfileSyncSettingsManifestApplication),
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum ProfileSyncSettingsCandidatePullApplyStatus {
    NoPublishedRoot {
        profile: String,
        root_id: String,
    },
    Unchanged {
        profile: String,
        root_id: String,
        object_id: String,
    },
    Applied(Vec<ProfileSyncSettingsManifestCandidateApplication>),
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum ProfileSyncDeviceHeadPullRecordStatus {
    NoPublishedRoot {
        profile: String,
        root_id: String,
    },
    Unchanged {
        profile: String,
        root_id: String,
        object_id: String,
    },
    Updated {
        device_head: VerifiedProfileSyncDeviceHead,
        root: ProfileSyncRootRecord,
    },
}

#[derive(Debug)]
pub enum StorageError {
    CurrentDirectory(std::io::Error),
    CreateDirectory {
        path: PathBuf,
        source: std::io::Error,
    },
    ReadLocalSyncDeviceId {
        path: PathBuf,
        source: std::io::Error,
    },
    WriteLocalSyncDeviceId {
        path: PathBuf,
        source: std::io::Error,
    },
    GenerateLocalSyncDeviceId,
    Database {
        path: PathBuf,
        source: rusqlite::Error,
    },
    EncodeSyncPayload(serde_json::Error),
    DecodeSyncPayload(serde_json::Error),
    EncodeSnapshotDomains(serde_json::Error),
    Clock(std::time::SystemTimeError),
    InvalidSyncDeviceId(String),
    InvalidSyncMembershipRecordId(String),
    InvalidSyncMembershipRecordKind(String),
    InvalidSyncMembershipEpoch(i64),
    InvalidProfileSyncMembershipRecord(String),
    InvalidProfileSyncEnrollmentBundle(String),
    InvalidProfileSyncDeviceEnrollmentRequest(String),
    InvalidProfileSyncSecretHandoffBundle(String),
    UntrustedSyncMembershipSigner {
        profile: String,
        device_id: String,
    },
    InvalidSyncContentKeyId(String),
    InvalidSyncDomain(String),
    InvalidSyncRevision(i64),
    InvalidCalendarEventId(String),
    InvalidChatConversationId(String),
    InvalidChatProviderId(String),
    InvalidContactId(String),
    InvalidDownloadId(String),
    InvalidDownloadSize(u64),
    InvalidFileEntryId(String),
    InvalidFileEntryKind(String),
    InvalidFileSize(u64),
    InvalidStorageProviderId(String),
    InvalidStorageProviderKind(String),
    InvalidStorageProviderQuota(u64),
    InvalidStoragePinningPolicy(String),
    MissingActiveSyncContentKey(String),
    UnsupportedSyncContentKeyAlgorithm {
        key_id: String,
        algorithm: String,
    },
    UnauthorizedSyncContentKeyEpoch {
        profile: String,
        key_id: String,
        key_membership_epoch: i64,
        manifest_membership_epoch: i64,
    },
    InvalidSyncRootId(String),
    InvalidProfileSyncManifest(String),
    UnsupportedProfileSyncManifestSchema(u8),
    UnsupportedProfileSyncMembershipRecordSchema(u8),
    UnsupportedProfileSyncEnrollmentBundleSchema(u8),
    UnsupportedProfileSyncDeviceEnrollmentRequestSchema(u8),
    UnsupportedProfileSyncSecretHandoffBundleSchema(u8),
    UnsupportedSyncSnapshotSchema(u8),
}

impl fmt::Display for StorageError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::CurrentDirectory(error) => {
                write!(formatter, "failed to read current directory: {error}")
            }
            Self::CreateDirectory { path, source } => {
                write!(
                    formatter,
                    "failed to create database directory {}: {source}",
                    path.display()
                )
            }
            Self::ReadLocalSyncDeviceId { path, source } => {
                write!(
                    formatter,
                    "failed to read local sync device id from {}: {source}",
                    path.display()
                )
            }
            Self::WriteLocalSyncDeviceId { path, source } => {
                write!(
                    formatter,
                    "failed to write local sync device id to {}: {source}",
                    path.display()
                )
            }
            Self::GenerateLocalSyncDeviceId => {
                write!(formatter, "failed to generate local sync device id")
            }
            Self::Database { path, source } => {
                write!(
                    formatter,
                    "database operation failed for {}: {source}",
                    path.display()
                )
            }
            Self::EncodeSyncPayload(error) => {
                write!(formatter, "failed to encode sync payload: {error}")
            }
            Self::DecodeSyncPayload(error) => {
                write!(formatter, "failed to decode sync payload: {error}")
            }
            Self::EncodeSnapshotDomains(error) => {
                write!(formatter, "failed to encode sync snapshot domains: {error}")
            }
            Self::Clock(error) => write!(formatter, "failed to read system clock: {error}"),
            Self::InvalidSyncDeviceId(device_id) => {
                write!(formatter, "invalid sync device id: {device_id}")
            }
            Self::InvalidSyncMembershipRecordId(record_id) => {
                write!(formatter, "invalid sync membership record id: {record_id}")
            }
            Self::InvalidSyncMembershipRecordKind(record_kind) => {
                write!(
                    formatter,
                    "invalid sync membership record kind: {record_kind}"
                )
            }
            Self::InvalidSyncMembershipEpoch(membership_epoch) => {
                write!(
                    formatter,
                    "invalid sync membership epoch: {membership_epoch}"
                )
            }
            Self::InvalidProfileSyncMembershipRecord(reason) => {
                write!(
                    formatter,
                    "invalid profile sync membership record: {reason}"
                )
            }
            Self::InvalidProfileSyncEnrollmentBundle(reason) => {
                write!(
                    formatter,
                    "invalid profile sync enrollment bundle: {reason}"
                )
            }
            Self::InvalidProfileSyncDeviceEnrollmentRequest(reason) => {
                write!(
                    formatter,
                    "invalid profile sync device enrollment request: {reason}"
                )
            }
            Self::InvalidProfileSyncSecretHandoffBundle(reason) => {
                write!(
                    formatter,
                    "invalid profile sync secret handoff bundle: {reason}"
                )
            }
            Self::UntrustedSyncMembershipSigner { profile, device_id } => {
                write!(
                    formatter,
                    "profile {profile} has no trusted sync membership signer {device_id}"
                )
            }
            Self::InvalidSyncContentKeyId(key_id) => {
                write!(formatter, "invalid sync content key id: {key_id}")
            }
            Self::InvalidSyncDomain(domain) => {
                write!(formatter, "invalid sync domain: {domain}")
            }
            Self::InvalidSyncRevision(revision) => {
                write!(formatter, "invalid sync revision: {revision}")
            }
            Self::InvalidCalendarEventId(event_id) => {
                write!(formatter, "invalid calendar event id: {event_id}")
            }
            Self::InvalidChatConversationId(conversation_id) => {
                write!(formatter, "invalid chat conversation id: {conversation_id}")
            }
            Self::InvalidChatProviderId(provider_id) => {
                write!(formatter, "invalid chat provider id: {provider_id}")
            }
            Self::InvalidContactId(contact_id) => {
                write!(formatter, "invalid contact id: {contact_id}")
            }
            Self::InvalidDownloadId(download_id) => {
                write!(formatter, "invalid download id: {download_id}")
            }
            Self::InvalidDownloadSize(size_bytes) => {
                write!(
                    formatter,
                    "download metadata size exceeds SQLite integer range: {size_bytes}"
                )
            }
            Self::InvalidFileEntryId(entry_id) => {
                write!(formatter, "invalid file entry id: {entry_id}")
            }
            Self::InvalidFileEntryKind(entry_kind) => {
                write!(formatter, "invalid file entry kind: {entry_kind}")
            }
            Self::InvalidFileSize(size_bytes) => {
                write!(
                    formatter,
                    "file metadata size exceeds SQLite integer range: {size_bytes}"
                )
            }
            Self::InvalidStorageProviderId(provider_id) => {
                write!(formatter, "invalid storage provider id: {provider_id}")
            }
            Self::InvalidStorageProviderKind(provider_kind) => {
                write!(formatter, "invalid storage provider kind: {provider_kind}")
            }
            Self::InvalidStorageProviderQuota(quota_bytes) => {
                write!(
                    formatter,
                    "storage provider quota exceeds SQLite integer range: {quota_bytes}"
                )
            }
            Self::InvalidStoragePinningPolicy(pinning_policy) => {
                write!(
                    formatter,
                    "invalid storage provider pinning policy: {pinning_policy}"
                )
            }
            Self::MissingActiveSyncContentKey(profile) => {
                write!(
                    formatter,
                    "profile {profile} has no active sync content key epoch"
                )
            }
            Self::UnsupportedSyncContentKeyAlgorithm { key_id, algorithm } => write!(
                formatter,
                "unsupported sync content key algorithm for {key_id}: {algorithm}"
            ),
            Self::UnauthorizedSyncContentKeyEpoch {
                profile,
                key_id,
                key_membership_epoch,
                manifest_membership_epoch,
            } => write!(
                formatter,
                "profile {profile} sync content key {key_id} was introduced at membership epoch {key_membership_epoch}, after manifest epoch {manifest_membership_epoch}"
            ),
            Self::InvalidSyncRootId(root_id) => {
                write!(formatter, "invalid sync root id: {root_id}")
            }
            Self::InvalidProfileSyncManifest(reason) => {
                write!(formatter, "invalid profile sync manifest: {reason}")
            }
            Self::UnsupportedProfileSyncManifestSchema(schema_version) => {
                write!(
                    formatter,
                    "unsupported profile sync manifest schema version: {schema_version}"
                )
            }
            Self::UnsupportedProfileSyncMembershipRecordSchema(schema_version) => {
                write!(
                    formatter,
                    "unsupported profile sync membership record schema version: {schema_version}"
                )
            }
            Self::UnsupportedProfileSyncEnrollmentBundleSchema(schema_version) => {
                write!(
                    formatter,
                    "unsupported profile sync enrollment bundle schema version: {schema_version}"
                )
            }
            Self::UnsupportedProfileSyncDeviceEnrollmentRequestSchema(schema_version) => {
                write!(
                    formatter,
                    "unsupported profile sync device enrollment request schema version: {schema_version}"
                )
            }
            Self::UnsupportedProfileSyncSecretHandoffBundleSchema(schema_version) => {
                write!(
                    formatter,
                    "unsupported profile sync secret handoff bundle schema version: {schema_version}"
                )
            }
            Self::UnsupportedSyncSnapshotSchema(schema_version) => {
                write!(
                    formatter,
                    "unsupported sync settings snapshot schema version: {schema_version}"
                )
            }
        }
    }
}

impl std::error::Error for StorageError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Self::CurrentDirectory(error) => Some(error),
            Self::CreateDirectory { source, .. } => Some(source),
            Self::ReadLocalSyncDeviceId { source, .. } => Some(source),
            Self::WriteLocalSyncDeviceId { source, .. } => Some(source),
            Self::GenerateLocalSyncDeviceId => None,
            Self::Database { source, .. } => Some(source),
            Self::EncodeSyncPayload(error) => Some(error),
            Self::DecodeSyncPayload(error) => Some(error),
            Self::EncodeSnapshotDomains(error) => Some(error),
            Self::Clock(error) => Some(error),
            Self::InvalidSyncDeviceId(_) => None,
            Self::InvalidSyncMembershipRecordId(_) => None,
            Self::InvalidSyncMembershipRecordKind(_) => None,
            Self::InvalidSyncMembershipEpoch(_) => None,
            Self::InvalidProfileSyncMembershipRecord(_) => None,
            Self::InvalidProfileSyncEnrollmentBundle(_) => None,
            Self::InvalidProfileSyncDeviceEnrollmentRequest(_) => None,
            Self::InvalidProfileSyncSecretHandoffBundle(_) => None,
            Self::UntrustedSyncMembershipSigner { .. } => None,
            Self::InvalidSyncContentKeyId(_) => None,
            Self::InvalidSyncDomain(_) => None,
            Self::InvalidSyncRevision(_) => None,
            Self::InvalidCalendarEventId(_) => None,
            Self::InvalidChatConversationId(_) => None,
            Self::InvalidChatProviderId(_) => None,
            Self::InvalidContactId(_) => None,
            Self::InvalidDownloadId(_) => None,
            Self::InvalidDownloadSize(_) => None,
            Self::InvalidFileEntryId(_) => None,
            Self::InvalidFileEntryKind(_) => None,
            Self::InvalidFileSize(_) => None,
            Self::InvalidStorageProviderId(_) => None,
            Self::InvalidStorageProviderKind(_) => None,
            Self::InvalidStorageProviderQuota(_) => None,
            Self::InvalidStoragePinningPolicy(_) => None,
            Self::MissingActiveSyncContentKey(_) => None,
            Self::UnsupportedSyncContentKeyAlgorithm { .. }
            | Self::UnauthorizedSyncContentKeyEpoch { .. } => None,
            Self::InvalidSyncRootId(_) => None,
            Self::InvalidProfileSyncManifest(_) => None,
            Self::UnsupportedProfileSyncManifestSchema(_) => None,
            Self::UnsupportedProfileSyncMembershipRecordSchema(_) => None,
            Self::UnsupportedProfileSyncEnrollmentBundleSchema(_) => None,
            Self::UnsupportedProfileSyncDeviceEnrollmentRequestSchema(_) => None,
            Self::UnsupportedProfileSyncSecretHandoffBundleSchema(_) => None,
            Self::UnsupportedSyncSnapshotSchema(_) => None,
        }
    }
}

impl SlateProfileDatabase {
    pub fn open(explicit_path: Option<PathBuf>) -> Result<Self, StorageError> {
        let launch_directory = std::env::current_dir().map_err(StorageError::CurrentDirectory)?;
        let resolved = resolve_database_path(
            explicit_path.as_deref(),
            &launch_directory,
            dirs::home_dir().as_deref(),
        );
        Self::open_resolved_with_persistent_device_id(resolved.path)
    }

    pub fn open_resolved(path: PathBuf) -> Result<Self, StorageError> {
        Self::open_resolved_with_device_id(path, DEFAULT_SYNC_DEVICE_ID)
    }

    pub fn open_resolved_with_persistent_device_id(path: PathBuf) -> Result<Self, StorageError> {
        ensure_database_parent_directory(&path)?;
        let local_sync_device_id = load_or_create_persistent_local_sync_device_id(&path)?;
        Self::open_resolved_with_device_id(path, local_sync_device_id)
    }

    pub fn open_resolved_with_device_id(
        path: PathBuf,
        local_sync_device_id: impl Into<String>,
    ) -> Result<Self, StorageError> {
        let local_sync_device_id = local_sync_device_id.into();
        if !is_valid_sync_identifier(local_sync_device_id.as_str()) {
            return Err(StorageError::InvalidSyncDeviceId(local_sync_device_id));
        }

        ensure_database_parent_directory(&path)?;

        let database = Self {
            path: Arc::new(path),
            local_sync_device_id: Arc::new(local_sync_device_id),
        };
        database.initialize()?;
        database.try_seed_default_sync_state();
        database.try_seed_default_bookmarks();
        Ok(database)
    }

    pub fn path(&self) -> &Path {
        self.path.as_ref()
    }

    pub fn local_sync_device_id(&self) -> &str {
        self.local_sync_device_id.as_str()
    }

    pub fn get_setting_text(&self, key: &str) -> Result<Option<String>, StorageError> {
        let connection = self.connection()?;
        connection
            .query_row("SELECT value FROM settings WHERE key = ?1", [key], |row| {
                row.get(0)
            })
            .optional()
            .map_err(|source| self.database_error(source))
    }

    pub fn set_setting_text(&self, key: &str, value: &str) -> Result<(), StorageError> {
        let mut connection = self.connection()?;
        let transaction = connection
            .transaction()
            .map_err(|source| self.database_error(source))?;
        let now = unix_time_seconds()?;
        transaction
            .execute(
                "INSERT INTO settings (key, value, updated_at)
                 VALUES (?1, ?2, ?3)
                 ON CONFLICT(key) DO UPDATE SET
                   value = excluded.value,
                   updated_at = excluded.updated_at",
                params![key, value, now],
            )
            .map_err(|source| self.database_error(source))?;
        record_sync_setting_text_in_transaction(
            &transaction,
            DEFAULT_PROFILE_ID,
            SYNC_DOMAIN_SETTINGS,
            key,
            value,
            self.local_sync_device_id(),
            now,
        )
        .map_err(|source| self.database_error(source))?;
        transaction
            .commit()
            .map_err(|source| self.database_error(source))?;
        Ok(())
    }

    pub fn ensure_setting_text(&self, key: &str, value: &str) -> Result<String, StorageError> {
        if let Some(existing) = self.get_setting_text(key)? {
            return Ok(existing);
        }

        self.set_setting_text(key, value)?;
        Ok(value.to_string())
    }

    pub fn get_setting_text_or_default(&self, key: &str, value: &str) -> String {
        self.get_setting_text(key)
            .ok()
            .flatten()
            .unwrap_or_else(|| value.to_string())
    }

    pub fn get_setting_f32(&self, key: &str) -> Result<Option<f32>, StorageError> {
        Ok(self
            .get_setting_text(key)?
            .and_then(|value| value.parse::<f32>().ok()))
    }

    pub fn get_setting_f32_or_default(&self, key: &str, value: f32) -> f32 {
        self.get_setting_f32(key).ok().flatten().unwrap_or(value)
    }

    pub fn set_setting_f32(&self, key: &str, value: f32) -> Result<(), StorageError> {
        self.set_setting_text(key, &format!("{value:.2}"))
    }

    pub fn ensure_setting_f32(&self, key: &str, value: f32) -> Result<f32, StorageError> {
        let stored = self.ensure_setting_text(key, &format!("{value:.2}"))?;
        Ok(stored.parse::<f32>().unwrap_or(value))
    }

    pub fn ensure_setting_f32_or_default(&self, key: &str, value: f32) -> f32 {
        self.ensure_setting_f32(key, value).unwrap_or(value)
    }

    pub fn upsert_bookmark(&self, bookmark: &BookmarkUpdate) -> Result<(), StorageError> {
        let connection = self.connection()?;
        let now = unix_time_seconds()?;
        connection
            .execute(
                "INSERT INTO bookmarks
                   (profile, url, title, folder, position, favicon_key, created_at, updated_at)
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?7)
                 ON CONFLICT(profile, url) DO UPDATE SET
                   title = excluded.title,
                   folder = excluded.folder,
                   position = excluded.position,
                   favicon_key = excluded.favicon_key,
                   updated_at = excluded.updated_at",
                params![
                    bookmark.profile.as_str(),
                    bookmark.url.as_str(),
                    bookmark.title.as_deref(),
                    bookmark.folder.as_deref(),
                    bookmark.position,
                    bookmark.favicon_key.as_deref(),
                    now
                ],
            )
            .map_err(|source| self.database_error(source))?;
        Ok(())
    }

    pub fn set_bookmark_slot(
        &self,
        bookmark: &BookmarkUpdate,
        replaced_url: Option<&str>,
    ) -> Result<(), StorageError> {
        let sync_key = bookmark_home_slot_sync_key(bookmark.position);
        let sync_payload = bookmark_home_slot_sync_payload(bookmark, replaced_url)?;
        let mut connection = self.connection()?;
        let transaction = connection
            .transaction()
            .map_err(|source| self.database_error(source))?;
        let now = unix_time_seconds()?;
        transaction
            .execute(
                "DELETE FROM bookmarks
                 WHERE profile = ?1
                   AND (
                     position = ?2
                     OR url = ?3
                     OR (?4 IS NOT NULL AND url = ?4)
                   )",
                params![
                    bookmark.profile.as_str(),
                    bookmark.position,
                    bookmark.url.as_str(),
                    replaced_url
                ],
            )
            .map_err(|source| self.database_error(source))?;
        transaction
            .execute(
                "INSERT INTO bookmarks
                   (profile, url, title, folder, position, favicon_key, created_at, updated_at)
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?7)",
                params![
                    bookmark.profile.as_str(),
                    bookmark.url.as_str(),
                    bookmark.title.as_deref(),
                    bookmark.folder.as_deref(),
                    bookmark.position,
                    bookmark.favicon_key.as_deref(),
                    now
                ],
            )
            .map_err(|source| self.database_error(source))?;
        record_sync_setting_text_in_transaction(
            &transaction,
            bookmark.profile.as_str(),
            SYNC_DOMAIN_BOOKMARKS,
            sync_key.as_str(),
            sync_payload.as_str(),
            self.local_sync_device_id(),
            now,
        )
        .map_err(|source| self.database_error(source))?;
        transaction
            .commit()
            .map_err(|source| self.database_error(source))?;
        Ok(())
    }

    pub fn bookmarks(&self, profile: &str) -> Result<Vec<BookmarkRecord>, StorageError> {
        let connection = self.connection()?;
        let mut statement = connection
            .prepare(
                "SELECT profile, url, title, folder, position, favicon_key, created_at, updated_at
                 FROM bookmarks
                 WHERE profile = ?1
                 ORDER BY folder, position, title, url",
            )
            .map_err(|source| self.database_error(source))?;
        let records = statement
            .query_map([profile], |row| {
                Ok(BookmarkRecord {
                    profile: row.get(0)?,
                    url: row.get(1)?,
                    title: row.get(2)?,
                    folder: row.get(3)?,
                    position: row.get(4)?,
                    favicon_key: row.get(5)?,
                    created_at: row.get(6)?,
                    updated_at: row.get(7)?,
                })
            })
            .map_err(|source| self.database_error(source))?;

        let mut bookmarks = Vec::new();
        for record in records {
            bookmarks.push(record.map_err(|source| self.database_error(source))?);
        }
        Ok(bookmarks)
    }

    pub fn remove_bookmark(&self, profile: &str, url: &str) -> Result<(), StorageError> {
        let mut connection = self.connection()?;
        let transaction = connection
            .transaction()
            .map_err(|source| self.database_error(source))?;
        let now = unix_time_seconds()?;
        let removed = transaction
            .query_row(
                "SELECT title, folder, position, favicon_key
                 FROM bookmarks
                 WHERE profile = ?1 AND url = ?2",
                params![profile, url],
                |row| {
                    Ok(BookmarkSlotSyncPayload {
                        url: url.to_string(),
                        title: row.get(0)?,
                        folder: row.get(1)?,
                        position: row.get(2)?,
                        favicon_key: row.get(3)?,
                        replaced_url: None,
                        deleted: true,
                    })
                },
            )
            .optional()
            .map_err(|source| self.database_error(source))?;
        transaction
            .execute(
                "DELETE FROM bookmarks WHERE profile = ?1 AND url = ?2",
                params![profile, url],
            )
            .map_err(|source| self.database_error(source))?;
        if let Some(payload) = removed {
            let sync_key = bookmark_home_slot_sync_key(payload.position);
            let sync_payload =
                serde_json::to_string(&payload).map_err(StorageError::EncodeSyncPayload)?;
            record_sync_setting_text_in_transaction(
                &transaction,
                profile,
                SYNC_DOMAIN_BOOKMARKS,
                sync_key.as_str(),
                sync_payload.as_str(),
                self.local_sync_device_id(),
                now,
            )
            .map_err(|source| self.database_error(source))?;
        }
        transaction
            .commit()
            .map_err(|source| self.database_error(source))?;
        Ok(())
    }

    pub fn record_download_metadata(
        &self,
        download: &DownloadMetadataUpdate,
    ) -> Result<DownloadMetadataRecord, StorageError> {
        validate_download_metadata_update(download)?;
        let sync_key = download_metadata_sync_key(download.download_id.as_str());
        let sync_payload = download_metadata_sync_payload(download)?;
        let payload = DownloadMetadataSyncPayload {
            download_id: download.download_id.clone(),
            source_url: download.source_url.clone(),
            final_url: download.final_url.clone(),
            route: download.route.clone(),
            transport_id: download.transport_id.clone(),
            filename: download.filename.clone(),
            content_type: download.content_type.clone(),
            size_bytes: download.size_bytes,
            integrity: download.integrity.clone(),
            deleted: false,
        };
        let mut connection = self.connection()?;
        let transaction = connection
            .transaction()
            .map_err(|source| self.database_error(source))?;
        let now = unix_time_seconds()?;
        upsert_download_metadata_in_transaction(
            &transaction,
            download.profile.as_str(),
            &payload,
            now,
        )
        .map_err(|source| self.database_error(source))?;
        record_sync_setting_text_in_transaction(
            &transaction,
            download.profile.as_str(),
            SYNC_DOMAIN_DOWNLOADS,
            sync_key.as_str(),
            sync_payload.as_str(),
            self.local_sync_device_id(),
            now,
        )
        .map_err(|source| self.database_error(source))?;
        let record = download_metadata_record_by_id_in_transaction(
            &transaction,
            download.profile.as_str(),
            download.download_id.as_str(),
        )
        .map_err(|source| self.database_error(source))?;
        transaction
            .commit()
            .map_err(|source| self.database_error(source))?;
        Ok(record)
    }

    pub fn downloads(
        &self,
        profile: &str,
        limit: u32,
    ) -> Result<Vec<DownloadMetadataRecord>, StorageError> {
        let connection = self.connection()?;
        let mut statement = connection
            .prepare(
                "SELECT profile, download_id, source_url, final_url, route, transport_id,
                        filename, content_type, size_bytes, integrity, created_at, updated_at
                 FROM downloads
                 WHERE profile = ?1
                 ORDER BY updated_at DESC, download_id
                 LIMIT ?2",
            )
            .map_err(|source| self.database_error(source))?;
        let records = statement
            .query_map(
                params![profile, i64::from(limit)],
                download_metadata_record_from_row,
            )
            .map_err(|source| self.database_error(source))?;

        let mut downloads = Vec::new();
        for record in records {
            downloads.push(record.map_err(|source| self.database_error(source))?);
        }
        Ok(downloads)
    }

    pub fn remove_download_metadata(
        &self,
        profile: &str,
        download_id: &str,
    ) -> Result<(), StorageError> {
        validate_download_id(download_id)?;
        let mut connection = self.connection()?;
        let transaction = connection
            .transaction()
            .map_err(|source| self.database_error(source))?;
        let now = unix_time_seconds()?;
        let removed = download_metadata_record_by_id_optional_in_transaction(
            &transaction,
            profile,
            download_id,
        )
        .map_err(|source| self.database_error(source))?;
        transaction
            .execute(
                "DELETE FROM downloads WHERE profile = ?1 AND download_id = ?2",
                params![profile, download_id],
            )
            .map_err(|source| self.database_error(source))?;
        if let Some(record) = removed {
            let sync_key = download_metadata_sync_key(record.download_id.as_str());
            let sync_payload = download_metadata_tombstone_sync_payload(&record)?;
            record_sync_setting_text_in_transaction(
                &transaction,
                profile,
                SYNC_DOMAIN_DOWNLOADS,
                sync_key.as_str(),
                sync_payload.as_str(),
                self.local_sync_device_id(),
                now,
            )
            .map_err(|source| self.database_error(source))?;
        }
        transaction
            .commit()
            .map_err(|source| self.database_error(source))?;
        Ok(())
    }

    pub fn upsert_calendar_event(
        &self,
        event: &CalendarEventUpdate,
    ) -> Result<CalendarEventRecord, StorageError> {
        validate_calendar_event_id(event.event_id.as_str())?;
        let sync_key = calendar_event_sync_key(event.event_id.as_str());
        let sync_payload = calendar_event_sync_payload(event)?;
        let payload = CalendarEventSyncPayload {
            event_id: event.event_id.clone(),
            calendar_id: event.calendar_id.clone(),
            title: event.title.clone(),
            starts_at: event.starts_at,
            ends_at: event.ends_at,
            time_zone: event.time_zone.clone(),
            location: event.location.clone(),
            notes: event.notes.clone(),
            recurrence_rule: event.recurrence_rule.clone(),
            reminder_minutes: event.reminder_minutes,
            deleted: false,
        };
        let mut connection = self.connection()?;
        let transaction = connection
            .transaction()
            .map_err(|source| self.database_error(source))?;
        let now = unix_time_seconds()?;
        upsert_calendar_event_in_transaction(&transaction, event.profile.as_str(), &payload, now)
            .map_err(|source| self.database_error(source))?;
        record_sync_setting_text_in_transaction(
            &transaction,
            event.profile.as_str(),
            SYNC_DOMAIN_CALENDAR,
            sync_key.as_str(),
            sync_payload.as_str(),
            self.local_sync_device_id(),
            now,
        )
        .map_err(|source| self.database_error(source))?;
        let record = calendar_event_record_by_id_in_transaction(
            &transaction,
            event.profile.as_str(),
            event.event_id.as_str(),
        )
        .map_err(|source| self.database_error(source))?;
        transaction
            .commit()
            .map_err(|source| self.database_error(source))?;
        Ok(record)
    }

    pub fn calendar_events(
        &self,
        profile: &str,
        limit: u32,
    ) -> Result<Vec<CalendarEventRecord>, StorageError> {
        let connection = self.connection()?;
        let mut statement = connection
            .prepare(
                "SELECT profile, event_id, calendar_id, title, starts_at, ends_at, time_zone,
                        location, notes, recurrence_rule, reminder_minutes, created_at, updated_at
                 FROM calendar_events
                 WHERE profile = ?1
                 ORDER BY starts_at, event_id
                 LIMIT ?2",
            )
            .map_err(|source| self.database_error(source))?;
        let records = statement
            .query_map(
                params![profile, i64::from(limit)],
                calendar_event_record_from_row,
            )
            .map_err(|source| self.database_error(source))?;

        let mut events = Vec::new();
        for record in records {
            events.push(record.map_err(|source| self.database_error(source))?);
        }
        Ok(events)
    }

    pub fn remove_calendar_event(&self, profile: &str, event_id: &str) -> Result<(), StorageError> {
        validate_calendar_event_id(event_id)?;
        let mut connection = self.connection()?;
        let transaction = connection
            .transaction()
            .map_err(|source| self.database_error(source))?;
        let now = unix_time_seconds()?;
        let removed =
            calendar_event_record_by_id_optional_in_transaction(&transaction, profile, event_id)
                .map_err(|source| self.database_error(source))?;
        transaction
            .execute(
                "DELETE FROM calendar_events WHERE profile = ?1 AND event_id = ?2",
                params![profile, event_id],
            )
            .map_err(|source| self.database_error(source))?;
        if let Some(record) = removed {
            let sync_key = calendar_event_sync_key(record.event_id.as_str());
            let sync_payload = calendar_event_tombstone_sync_payload(&record)?;
            record_sync_setting_text_in_transaction(
                &transaction,
                profile,
                SYNC_DOMAIN_CALENDAR,
                sync_key.as_str(),
                sync_payload.as_str(),
                self.local_sync_device_id(),
                now,
            )
            .map_err(|source| self.database_error(source))?;
        }
        transaction
            .commit()
            .map_err(|source| self.database_error(source))?;
        Ok(())
    }

    pub fn upsert_chat_conversation(
        &self,
        conversation: &ChatConversationUpdate,
    ) -> Result<ChatConversationRecord, StorageError> {
        validate_chat_conversation_update(conversation)?;
        let sync_key = chat_conversation_sync_key(conversation.conversation_id.as_str());
        let sync_payload = chat_conversation_sync_payload(conversation)?;
        let payload = ChatConversationSyncPayload {
            conversation_id: conversation.conversation_id.clone(),
            provider_id: conversation.provider_id.clone(),
            external_thread_id: conversation.external_thread_id.clone(),
            display_name: conversation.display_name.clone(),
            avatar_key: conversation.avatar_key.clone(),
            last_message_at: conversation.last_message_at,
            unread_count: conversation.unread_count,
            archived: conversation.archived,
            muted: conversation.muted,
            deleted: false,
        };
        let mut connection = self.connection()?;
        let transaction = connection
            .transaction()
            .map_err(|source| self.database_error(source))?;
        let now = unix_time_seconds()?;
        upsert_chat_conversation_in_transaction(
            &transaction,
            conversation.profile.as_str(),
            &payload,
            now,
        )
        .map_err(|source| self.database_error(source))?;
        record_sync_setting_text_in_transaction(
            &transaction,
            conversation.profile.as_str(),
            SYNC_DOMAIN_CHAT,
            sync_key.as_str(),
            sync_payload.as_str(),
            self.local_sync_device_id(),
            now,
        )
        .map_err(|source| self.database_error(source))?;
        let record = chat_conversation_record_by_id_in_transaction(
            &transaction,
            conversation.profile.as_str(),
            conversation.conversation_id.as_str(),
        )
        .map_err(|source| self.database_error(source))?;
        transaction
            .commit()
            .map_err(|source| self.database_error(source))?;
        Ok(record)
    }

    pub fn chat_conversations(
        &self,
        profile: &str,
        limit: u32,
    ) -> Result<Vec<ChatConversationRecord>, StorageError> {
        let connection = self.connection()?;
        let mut statement = connection
            .prepare(
                "SELECT profile, conversation_id, provider_id, external_thread_id,
                        display_name, avatar_key, last_message_at, unread_count, archived,
                        muted, created_at, updated_at
                 FROM chat_conversations
                 WHERE profile = ?1
                 ORDER BY archived, last_message_at DESC, display_name, conversation_id
                 LIMIT ?2",
            )
            .map_err(|source| self.database_error(source))?;
        let records = statement
            .query_map(
                params![profile, i64::from(limit)],
                chat_conversation_record_from_row,
            )
            .map_err(|source| self.database_error(source))?;

        let mut conversations = Vec::new();
        for record in records {
            conversations.push(record.map_err(|source| self.database_error(source))?);
        }
        Ok(conversations)
    }

    pub fn remove_chat_conversation(
        &self,
        profile: &str,
        conversation_id: &str,
    ) -> Result<(), StorageError> {
        validate_chat_conversation_id(conversation_id)?;
        let mut connection = self.connection()?;
        let transaction = connection
            .transaction()
            .map_err(|source| self.database_error(source))?;
        let now = unix_time_seconds()?;
        let removed = chat_conversation_record_by_id_optional_in_transaction(
            &transaction,
            profile,
            conversation_id,
        )
        .map_err(|source| self.database_error(source))?;
        transaction
            .execute(
                "DELETE FROM chat_conversations WHERE profile = ?1 AND conversation_id = ?2",
                params![profile, conversation_id],
            )
            .map_err(|source| self.database_error(source))?;
        if let Some(record) = removed {
            let sync_key = chat_conversation_sync_key(record.conversation_id.as_str());
            let sync_payload = chat_conversation_tombstone_sync_payload(&record)?;
            record_sync_setting_text_in_transaction(
                &transaction,
                profile,
                SYNC_DOMAIN_CHAT,
                sync_key.as_str(),
                sync_payload.as_str(),
                self.local_sync_device_id(),
                now,
            )
            .map_err(|source| self.database_error(source))?;
        }
        transaction
            .commit()
            .map_err(|source| self.database_error(source))?;
        Ok(())
    }

    pub fn upsert_contact_card(
        &self,
        contact: &ContactCardUpdate,
    ) -> Result<ContactCardRecord, StorageError> {
        validate_contact_id(contact.contact_id.as_str())?;
        let sync_key = contact_card_sync_key(contact.contact_id.as_str());
        let sync_payload = contact_card_sync_payload(contact)?;
        let payload = ContactCardSyncPayload {
            contact_id: contact.contact_id.clone(),
            display_name: contact.display_name.clone(),
            given_name: contact.given_name.clone(),
            family_name: contact.family_name.clone(),
            organization: contact.organization.clone(),
            primary_email: contact.primary_email.clone(),
            primary_phone: contact.primary_phone.clone(),
            notes: contact.notes.clone(),
            avatar_key: contact.avatar_key.clone(),
            deleted: false,
        };
        let mut connection = self.connection()?;
        let transaction = connection
            .transaction()
            .map_err(|source| self.database_error(source))?;
        let now = unix_time_seconds()?;
        upsert_contact_card_in_transaction(&transaction, contact.profile.as_str(), &payload, now)
            .map_err(|source| self.database_error(source))?;
        record_sync_setting_text_in_transaction(
            &transaction,
            contact.profile.as_str(),
            SYNC_DOMAIN_CONTACTS,
            sync_key.as_str(),
            sync_payload.as_str(),
            self.local_sync_device_id(),
            now,
        )
        .map_err(|source| self.database_error(source))?;
        let record = contact_card_record_by_id_in_transaction(
            &transaction,
            contact.profile.as_str(),
            contact.contact_id.as_str(),
        )
        .map_err(|source| self.database_error(source))?;
        transaction
            .commit()
            .map_err(|source| self.database_error(source))?;
        Ok(record)
    }

    pub fn contact_cards(
        &self,
        profile: &str,
        limit: u32,
    ) -> Result<Vec<ContactCardRecord>, StorageError> {
        let connection = self.connection()?;
        let mut statement = connection
            .prepare(
                "SELECT profile, contact_id, display_name, given_name, family_name,
                        organization, primary_email, primary_phone, notes, avatar_key,
                        created_at, updated_at
                 FROM contact_cards
                 WHERE profile = ?1
                 ORDER BY display_name, contact_id
                 LIMIT ?2",
            )
            .map_err(|source| self.database_error(source))?;
        let records = statement
            .query_map(
                params![profile, i64::from(limit)],
                contact_card_record_from_row,
            )
            .map_err(|source| self.database_error(source))?;

        let mut contacts = Vec::new();
        for record in records {
            contacts.push(record.map_err(|source| self.database_error(source))?);
        }
        Ok(contacts)
    }

    pub fn remove_contact_card(&self, profile: &str, contact_id: &str) -> Result<(), StorageError> {
        validate_contact_id(contact_id)?;
        let mut connection = self.connection()?;
        let transaction = connection
            .transaction()
            .map_err(|source| self.database_error(source))?;
        let now = unix_time_seconds()?;
        let removed =
            contact_card_record_by_id_optional_in_transaction(&transaction, profile, contact_id)
                .map_err(|source| self.database_error(source))?;
        transaction
            .execute(
                "DELETE FROM contact_cards WHERE profile = ?1 AND contact_id = ?2",
                params![profile, contact_id],
            )
            .map_err(|source| self.database_error(source))?;
        if let Some(record) = removed {
            let sync_key = contact_card_sync_key(record.contact_id.as_str());
            let sync_payload = contact_card_tombstone_sync_payload(&record)?;
            record_sync_setting_text_in_transaction(
                &transaction,
                profile,
                SYNC_DOMAIN_CONTACTS,
                sync_key.as_str(),
                sync_payload.as_str(),
                self.local_sync_device_id(),
                now,
            )
            .map_err(|source| self.database_error(source))?;
        }
        transaction
            .commit()
            .map_err(|source| self.database_error(source))?;
        Ok(())
    }

    pub fn upsert_file_entry(
        &self,
        entry: &FileEntryUpdate,
    ) -> Result<FileEntryRecord, StorageError> {
        validate_file_entry_update(entry)?;
        let sync_key = file_entry_sync_key(entry.entry_id.as_str());
        let sync_payload = file_entry_sync_payload(entry)?;
        let payload = FileEntrySyncPayload {
            entry_id: entry.entry_id.clone(),
            sync_set_id: entry.sync_set_id.clone(),
            parent_id: entry.parent_id.clone(),
            name: entry.name.clone(),
            entry_kind: entry.entry_kind.clone(),
            content_ref: entry.content_ref.clone(),
            mime_type: entry.mime_type.clone(),
            size_bytes: entry.size_bytes,
            modified_at: entry.modified_at,
            integrity: entry.integrity.clone(),
            retention_policy: entry.retention_policy.clone(),
            deleted: false,
        };
        let mut connection = self.connection()?;
        let transaction = connection
            .transaction()
            .map_err(|source| self.database_error(source))?;
        let now = unix_time_seconds()?;
        upsert_file_entry_in_transaction(&transaction, entry.profile.as_str(), &payload, now)
            .map_err(|source| self.database_error(source))?;
        record_sync_setting_text_in_transaction(
            &transaction,
            entry.profile.as_str(),
            SYNC_DOMAIN_FILES,
            sync_key.as_str(),
            sync_payload.as_str(),
            self.local_sync_device_id(),
            now,
        )
        .map_err(|source| self.database_error(source))?;
        let record = file_entry_record_by_id_in_transaction(
            &transaction,
            entry.profile.as_str(),
            entry.entry_id.as_str(),
        )
        .map_err(|source| self.database_error(source))?;
        transaction
            .commit()
            .map_err(|source| self.database_error(source))?;
        Ok(record)
    }

    pub fn file_entries(
        &self,
        profile: &str,
        limit: u32,
    ) -> Result<Vec<FileEntryRecord>, StorageError> {
        let connection = self.connection()?;
        let mut statement = connection
            .prepare(
                "SELECT profile, entry_id, sync_set_id, parent_id, name, entry_kind,
                        content_ref, mime_type, size_bytes, modified_at, integrity,
                        retention_policy, created_at, updated_at
                 FROM file_entries
                 WHERE profile = ?1
                 ORDER BY sync_set_id, parent_id, name, entry_id
                 LIMIT ?2",
            )
            .map_err(|source| self.database_error(source))?;
        let records = statement
            .query_map(
                params![profile, i64::from(limit)],
                file_entry_record_from_row,
            )
            .map_err(|source| self.database_error(source))?;

        let mut entries = Vec::new();
        for record in records {
            entries.push(record.map_err(|source| self.database_error(source))?);
        }
        Ok(entries)
    }

    pub fn remove_file_entry(&self, profile: &str, entry_id: &str) -> Result<(), StorageError> {
        validate_file_entry_id(entry_id)?;
        let mut connection = self.connection()?;
        let transaction = connection
            .transaction()
            .map_err(|source| self.database_error(source))?;
        let now = unix_time_seconds()?;
        let removed =
            file_entry_record_by_id_optional_in_transaction(&transaction, profile, entry_id)
                .map_err(|source| self.database_error(source))?;
        transaction
            .execute(
                "DELETE FROM file_entries WHERE profile = ?1 AND entry_id = ?2",
                params![profile, entry_id],
            )
            .map_err(|source| self.database_error(source))?;
        if let Some(record) = removed {
            let sync_key = file_entry_sync_key(record.entry_id.as_str());
            let sync_payload = file_entry_tombstone_sync_payload(&record)?;
            record_sync_setting_text_in_transaction(
                &transaction,
                profile,
                SYNC_DOMAIN_FILES,
                sync_key.as_str(),
                sync_payload.as_str(),
                self.local_sync_device_id(),
                now,
            )
            .map_err(|source| self.database_error(source))?;
        }
        transaction
            .commit()
            .map_err(|source| self.database_error(source))?;
        Ok(())
    }

    pub fn upsert_storage_provider(
        &self,
        provider: &StorageProviderUpdate,
    ) -> Result<StorageProviderRecord, StorageError> {
        validate_storage_provider_update(provider)?;
        let sync_key = storage_provider_sync_key(provider.provider_id.as_str());
        let sync_payload = storage_provider_sync_payload(provider)?;
        let payload = StorageProviderSyncPayload {
            provider_id: provider.provider_id.clone(),
            provider_kind: provider.provider_kind.clone(),
            display_name: provider.display_name.clone(),
            endpoint_ref: provider.endpoint_ref.clone(),
            discovery: provider.discovery,
            connectivity: provider.connectivity,
            object_transfer: provider.object_transfer,
            availability: provider.availability,
            mutable_roots: provider.mutable_roots,
            quota_bytes: provider.quota_bytes,
            max_retained_objects: provider.max_retained_objects,
            pinning_policy: provider.pinning_policy.clone(),
            enabled: provider.enabled,
            deleted: false,
        };
        let mut connection = self.connection()?;
        let transaction = connection
            .transaction()
            .map_err(|source| self.database_error(source))?;
        let now = unix_time_seconds()?;
        upsert_storage_provider_in_transaction(
            &transaction,
            provider.profile.as_str(),
            &payload,
            now,
        )
        .map_err(|source| self.database_error(source))?;
        record_sync_setting_text_in_transaction(
            &transaction,
            provider.profile.as_str(),
            SYNC_DOMAIN_STORAGE,
            sync_key.as_str(),
            sync_payload.as_str(),
            self.local_sync_device_id(),
            now,
        )
        .map_err(|source| self.database_error(source))?;
        let record = storage_provider_record_by_id_in_transaction(
            &transaction,
            provider.profile.as_str(),
            provider.provider_id.as_str(),
        )
        .map_err(|source| self.database_error(source))?;
        transaction
            .commit()
            .map_err(|source| self.database_error(source))?;
        Ok(record)
    }

    pub fn storage_providers(
        &self,
        profile: &str,
        limit: u32,
    ) -> Result<Vec<StorageProviderRecord>, StorageError> {
        let connection = self.connection()?;
        let mut statement = connection
            .prepare(
                "SELECT profile, provider_id, provider_kind, display_name, endpoint_ref,
                        discovery, connectivity, object_transfer, availability, mutable_roots,
                        quota_bytes, max_retained_objects, pinning_policy, enabled,
                        created_at, updated_at
                 FROM storage_providers
                 WHERE profile = ?1
                 ORDER BY enabled DESC, provider_kind, display_name, provider_id
                 LIMIT ?2",
            )
            .map_err(|source| self.database_error(source))?;
        let records = statement
            .query_map(
                params![profile, i64::from(limit)],
                storage_provider_record_from_row,
            )
            .map_err(|source| self.database_error(source))?;

        let mut providers = Vec::new();
        for record in records {
            providers.push(record.map_err(|source| self.database_error(source))?);
        }
        Ok(providers)
    }

    pub fn remove_storage_provider(
        &self,
        profile: &str,
        provider_id: &str,
    ) -> Result<(), StorageError> {
        validate_storage_provider_id(provider_id)?;
        let mut connection = self.connection()?;
        let transaction = connection
            .transaction()
            .map_err(|source| self.database_error(source))?;
        let now = unix_time_seconds()?;
        let removed = storage_provider_record_by_id_optional_in_transaction(
            &transaction,
            profile,
            provider_id,
        )
        .map_err(|source| self.database_error(source))?;
        transaction
            .execute(
                "DELETE FROM storage_providers WHERE profile = ?1 AND provider_id = ?2",
                params![profile, provider_id],
            )
            .map_err(|source| self.database_error(source))?;
        if let Some(record) = removed {
            let sync_key = storage_provider_sync_key(record.provider_id.as_str());
            let sync_payload = storage_provider_tombstone_sync_payload(&record)?;
            record_sync_setting_text_in_transaction(
                &transaction,
                profile,
                SYNC_DOMAIN_STORAGE,
                sync_key.as_str(),
                sync_payload.as_str(),
                self.local_sync_device_id(),
                now,
            )
            .map_err(|source| self.database_error(source))?;
        }
        transaction
            .commit()
            .map_err(|source| self.database_error(source))?;
        Ok(())
    }

    pub fn upsert_cookie(&self, cookie: &CookieUpdate) -> Result<(), StorageError> {
        let connection = self.connection()?;
        let now = unix_time_seconds()?;
        connection
            .execute(
                "INSERT INTO cookies
                   (profile, domain, path, name, value, expires_at, is_secure, is_http_only,
                    same_site, created_at, updated_at)
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?10)
                 ON CONFLICT(profile, domain, path, name) DO UPDATE SET
                   value = excluded.value,
                   expires_at = excluded.expires_at,
                   is_secure = excluded.is_secure,
                   is_http_only = excluded.is_http_only,
                   same_site = excluded.same_site,
                   updated_at = excluded.updated_at",
                params![
                    cookie.profile.as_str(),
                    cookie.domain.as_str(),
                    cookie.path.as_str(),
                    cookie.name.as_str(),
                    cookie.value.as_str(),
                    cookie.expires_at,
                    bool_to_integer(cookie.is_secure),
                    bool_to_integer(cookie.is_http_only),
                    cookie.same_site.as_deref(),
                    now
                ],
            )
            .map_err(|source| self.database_error(source))?;
        Ok(())
    }

    pub fn cookies_for_domain(
        &self,
        profile: &str,
        domain: &str,
    ) -> Result<Vec<CookieRecord>, StorageError> {
        let connection = self.connection()?;
        let mut statement = connection
            .prepare(
                "SELECT profile, domain, path, name, value, expires_at, is_secure, is_http_only,
                        same_site, created_at, updated_at
                 FROM cookies
                 WHERE profile = ?1 AND domain = ?2
                 ORDER BY path, name",
            )
            .map_err(|source| self.database_error(source))?;
        let records = statement
            .query_map(params![profile, domain], |row| {
                Ok(CookieRecord {
                    profile: row.get(0)?,
                    domain: row.get(1)?,
                    path: row.get(2)?,
                    name: row.get(3)?,
                    value: row.get(4)?,
                    expires_at: row.get(5)?,
                    is_secure: integer_to_bool(row.get(6)?),
                    is_http_only: integer_to_bool(row.get(7)?),
                    same_site: row.get(8)?,
                    created_at: row.get(9)?,
                    updated_at: row.get(10)?,
                })
            })
            .map_err(|source| self.database_error(source))?;

        let mut cookies = Vec::new();
        for record in records {
            cookies.push(record.map_err(|source| self.database_error(source))?);
        }
        Ok(cookies)
    }

    pub fn delete_cookie(
        &self,
        profile: &str,
        domain: &str,
        path: &str,
        name: &str,
    ) -> Result<(), StorageError> {
        let connection = self.connection()?;
        connection
            .execute(
                "DELETE FROM cookies
                 WHERE profile = ?1 AND domain = ?2 AND path = ?3 AND name = ?4",
                params![profile, domain, path, name],
            )
            .map_err(|source| self.database_error(source))?;
        Ok(())
    }

    pub fn record_history_visit(
        &self,
        profile: &str,
        url: &str,
        title: Option<&str>,
    ) -> Result<(), StorageError> {
        let connection = self.connection()?;
        let now = unix_time_seconds()?;
        connection
            .execute(
                "INSERT INTO browsing_history
                   (profile, url, title, first_visited_at, last_visited_at, visit_count)
                 VALUES (?1, ?2, ?3, ?4, ?4, 1)
                 ON CONFLICT(profile, url) DO UPDATE SET
                   title = COALESCE(excluded.title, browsing_history.title),
                   last_visited_at = excluded.last_visited_at,
                   visit_count = browsing_history.visit_count + 1",
                params![profile, url, title, now],
            )
            .map_err(|source| self.database_error(source))?;
        Ok(())
    }

    pub fn update_history_title(
        &self,
        profile: &str,
        url: &str,
        title: Option<&str>,
    ) -> Result<(), StorageError> {
        let Some(title) = title.filter(|title| !title.trim().is_empty()) else {
            return Ok(());
        };

        let connection = self.connection()?;
        let now = unix_time_seconds()?;
        connection
            .execute(
                "INSERT INTO browsing_history
                   (profile, url, title, first_visited_at, last_visited_at, visit_count)
                 VALUES (?1, ?2, ?3, ?4, ?4, 1)
                 ON CONFLICT(profile, url) DO UPDATE SET
                   title = excluded.title",
                params![profile, url, title, now],
            )
            .map_err(|source| self.database_error(source))?;
        Ok(())
    }

    pub fn recent_history(
        &self,
        profile: &str,
        limit: u32,
    ) -> Result<Vec<HistoryVisitRecord>, StorageError> {
        let connection = self.connection()?;
        let mut statement = connection
            .prepare(
                "SELECT profile, url, title, first_visited_at, last_visited_at, visit_count
                 FROM browsing_history
                 WHERE profile = ?1
                 ORDER BY last_visited_at DESC
                 LIMIT ?2",
            )
            .map_err(|source| self.database_error(source))?;
        let records = statement
            .query_map(params![profile, i64::from(limit)], |row| {
                Ok(HistoryVisitRecord {
                    profile: row.get(0)?,
                    url: row.get(1)?,
                    title: row.get(2)?,
                    first_visited_at: row.get(3)?,
                    last_visited_at: row.get(4)?,
                    visit_count: row.get(5)?,
                })
            })
            .map_err(|source| self.database_error(source))?;

        let mut history = Vec::new();
        for record in records {
            history.push(record.map_err(|source| self.database_error(source))?);
        }
        Ok(history)
    }

    pub fn set_blob(
        &self,
        profile: &str,
        key: &str,
        media_type: Option<&str>,
        data: &[u8],
    ) -> Result<(), StorageError> {
        let connection = self.connection()?;
        let now = unix_time_seconds()?;
        connection
            .execute(
                "INSERT INTO binary_blobs (profile, key, media_type, data, created_at, updated_at)
                 VALUES (?1, ?2, ?3, ?4, ?5, ?5)
                 ON CONFLICT(profile, key) DO UPDATE SET
                   media_type = excluded.media_type,
                   data = excluded.data,
                   updated_at = excluded.updated_at",
                params![profile, key, media_type, data, now],
            )
            .map_err(|source| self.database_error(source))?;
        Ok(())
    }

    pub fn get_blob(
        &self,
        profile: &str,
        key: &str,
    ) -> Result<Option<BinaryBlobRecord>, StorageError> {
        let connection = self.connection()?;
        connection
            .query_row(
                "SELECT profile, key, media_type, data, created_at, updated_at
                 FROM binary_blobs
                 WHERE profile = ?1 AND key = ?2",
                params![profile, key],
                |row| {
                    Ok(BinaryBlobRecord {
                        profile: row.get(0)?,
                        key: row.get(1)?,
                        media_type: row.get(2)?,
                        data: row.get(3)?,
                        created_at: row.get(4)?,
                        updated_at: row.get(5)?,
                    })
                },
            )
            .optional()
            .map_err(|source| self.database_error(source))
    }

    pub fn register_app_sync_domain(
        &self,
        domain: &AppSyncDomainRegistration,
    ) -> Result<(), StorageError> {
        let connection = self.connection()?;
        let now = unix_time_seconds()?;
        connection
            .execute(
                "INSERT INTO app_sync_domains
                   (profile, domain, schema_version, enabled, privacy_classification,
                    sync_content, created_at, updated_at)
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?7)
                 ON CONFLICT(profile, domain) DO UPDATE SET
                   schema_version = excluded.schema_version,
                   enabled = excluded.enabled,
                   privacy_classification = excluded.privacy_classification,
                   sync_content = excluded.sync_content,
                   updated_at = excluded.updated_at",
                params![
                    domain.profile.as_str(),
                    domain.domain.as_str(),
                    domain.schema_version,
                    bool_to_integer(domain.enabled),
                    domain.privacy_classification.as_str(),
                    bool_to_integer(domain.sync_content),
                    now
                ],
            )
            .map_err(|source| self.database_error(source))?;
        Ok(())
    }

    pub fn ensure_default_app_sync_domains(&self, profile: &str) -> Result<(), StorageError> {
        for domain in DEFAULT_APP_SYNC_DOMAINS {
            self.seed_default_app_sync_domain(profile, &domain)?;
        }
        Ok(())
    }

    pub fn app_sync_domains(
        &self,
        profile: &str,
    ) -> Result<Vec<AppSyncDomainRecord>, StorageError> {
        let connection = self.connection()?;
        let mut statement = connection
            .prepare(
                "SELECT profile, domain, schema_version, enabled, privacy_classification,
                        sync_content, created_at, updated_at
                 FROM app_sync_domains
                 WHERE profile = ?1
                 ORDER BY domain",
            )
            .map_err(|source| self.database_error(source))?;
        let records = statement
            .query_map([profile], |row| {
                Ok(AppSyncDomainRecord {
                    profile: row.get(0)?,
                    domain: row.get(1)?,
                    schema_version: row.get(2)?,
                    enabled: integer_to_bool(row.get(3)?),
                    privacy_classification: row.get(4)?,
                    sync_content: integer_to_bool(row.get(5)?),
                    created_at: row.get(6)?,
                    updated_at: row.get(7)?,
                })
            })
            .map_err(|source| self.database_error(source))?;

        let mut domains = Vec::new();
        for record in records {
            domains.push(record.map_err(|source| self.database_error(source))?);
        }
        Ok(domains)
    }

    pub fn enabled_app_sync_domains(
        &self,
        profile: &str,
    ) -> Result<Vec<AppSyncDomainRecord>, StorageError> {
        let connection = self.connection()?;
        let mut statement = connection
            .prepare(
                "SELECT profile, domain, schema_version, enabled, privacy_classification,
                        sync_content, created_at, updated_at
                 FROM app_sync_domains
                 WHERE profile = ?1 AND enabled = 1
                 ORDER BY domain",
            )
            .map_err(|source| self.database_error(source))?;
        let records = statement
            .query_map([profile], |row| {
                Ok(AppSyncDomainRecord {
                    profile: row.get(0)?,
                    domain: row.get(1)?,
                    schema_version: row.get(2)?,
                    enabled: integer_to_bool(row.get(3)?),
                    privacy_classification: row.get(4)?,
                    sync_content: integer_to_bool(row.get(5)?),
                    created_at: row.get(6)?,
                    updated_at: row.get(7)?,
                })
            })
            .map_err(|source| self.database_error(source))?;

        let mut domains = Vec::new();
        for record in records {
            domains.push(record.map_err(|source| self.database_error(source))?);
        }
        Ok(domains)
    }

    pub fn app_sync_domain_cursor(
        &self,
        profile: &str,
        domain: &str,
    ) -> Result<Option<AppSyncDomainCursorRecord>, StorageError> {
        validate_sync_domain(domain)?;
        let connection = self.connection()?;
        let key = app_sync_domain_cursor_key(domain);
        connection
            .query_row(
                "SELECT profile, key, value, updated_at
                 FROM sync_state
                 WHERE profile = ?1 AND key = ?2",
                params![profile, key.as_str()],
                app_sync_domain_cursor_record_from_row,
            )
            .optional()
            .map_err(|source| self.database_error(source))
    }

    pub fn record_app_sync_domain_cursor(
        &self,
        profile: &str,
        domain: &str,
        latest_revision: i64,
    ) -> Result<AppSyncDomainCursorRecord, StorageError> {
        validate_sync_domain(domain)?;
        if latest_revision < 0 {
            return Err(StorageError::InvalidSyncRevision(latest_revision));
        }

        let connection = self.connection()?;
        let now = unix_time_seconds()?;
        let key = app_sync_domain_cursor_key(domain);
        let current_revision = self
            .app_sync_domain_cursor(profile, domain)?
            .map(|cursor| cursor.latest_revision)
            .unwrap_or(0);
        let stored_revision = current_revision.max(latest_revision);
        let stored_revision_value = stored_revision.to_string();
        connection
            .execute(
                "INSERT INTO sync_state (profile, key, value, updated_at)
                 VALUES (?1, ?2, ?3, ?4)
                 ON CONFLICT(profile, key) DO UPDATE SET
                   value = excluded.value,
                   updated_at = excluded.updated_at",
                params![profile, key.as_str(), stored_revision_value.as_str(), now],
            )
            .map_err(|source| self.database_error(source))?;

        Ok(AppSyncDomainCursorRecord {
            profile: profile.to_string(),
            domain: domain.to_string(),
            latest_revision: stored_revision,
            updated_at: now,
        })
    }

    pub fn poll_sync_setting_text_events_for_app_domain(
        &self,
        profile: &str,
        domain: &str,
        limit: u32,
    ) -> Result<SyncSettingTextDomainPoll, StorageError> {
        let previous_revision = self
            .ensure_app_sync_domain_cursor_at_domain_head(profile, domain)?
            .latest_revision;
        self.poll_sync_setting_text_events_for_domain(profile, domain, previous_revision, limit)
    }

    pub fn ensure_app_sync_domain_cursor_at_domain_head(
        &self,
        profile: &str,
        domain: &str,
    ) -> Result<AppSyncDomainCursorRecord, StorageError> {
        if let Some(cursor) = self.app_sync_domain_cursor(profile, domain)? {
            return Ok(cursor);
        }
        let revision = self.latest_sync_revision_for_domain(profile, domain)?;
        self.record_app_sync_domain_cursor(profile, domain, revision)
    }

    pub fn poll_typed_sync_setting_text_events_for_app_domain<T>(
        &self,
        profile: &str,
        domain: &str,
        limit: u32,
    ) -> Result<TypedSyncSettingTextDomainPoll<T>, StorageError>
    where
        T: DeserializeOwned,
    {
        let poll = self.poll_sync_setting_text_events_for_app_domain(profile, domain, limit)?;
        let events = poll
            .events
            .into_iter()
            .map(|event| {
                let value = serde_json::from_str::<T>(event.change.payload.as_str())
                    .map_err(StorageError::DecodeSyncPayload)?;
                Ok(TypedSyncSettingTextEvent {
                    revision: event.revision,
                    change: event.change,
                    value,
                })
            })
            .collect::<Result<Vec<_>, StorageError>>()?;

        Ok(TypedSyncSettingTextDomainPoll {
            profile: poll.profile,
            domain: poll.domain,
            previous_revision: poll.previous_revision,
            latest_revision: poll.latest_revision,
            events,
        })
    }

    pub fn record_app_sync_domain_poll_cursor(
        &self,
        poll: &SyncSettingTextDomainPoll,
    ) -> Result<AppSyncDomainCursorRecord, StorageError> {
        self.record_app_sync_domain_cursor(
            poll.profile.as_str(),
            poll.domain.as_str(),
            poll.latest_revision,
        )
    }

    pub fn record_typed_app_sync_domain_poll_cursor<T>(
        &self,
        poll: &TypedSyncSettingTextDomainPoll<T>,
    ) -> Result<AppSyncDomainCursorRecord, StorageError> {
        self.record_app_sync_domain_cursor(
            poll.profile.as_str(),
            poll.domain.as_str(),
            poll.latest_revision,
        )
    }

    pub fn register_sync_device(
        &self,
        device: &SyncDeviceRegistration,
    ) -> Result<(), StorageError> {
        let connection = self.connection()?;
        let now = unix_time_seconds()?;
        connection
            .execute(
                "INSERT INTO sync_devices
                   (profile, device_id, label, membership_epoch, provider_authority,
                    created_at, last_seen_at)
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?6)
                 ON CONFLICT(profile, device_id) DO UPDATE SET
                   label = excluded.label,
                   membership_epoch = excluded.membership_epoch,
                   provider_authority = excluded.provider_authority,
                   last_seen_at = excluded.last_seen_at",
                params![
                    device.profile.as_str(),
                    device.device_id.as_str(),
                    device.label.as_deref(),
                    device.membership_epoch,
                    bool_to_integer(device.provider_authority),
                    now
                ],
            )
            .map_err(|source| self.database_error(source))?;
        Ok(())
    }

    pub fn sync_devices(&self, profile: &str) -> Result<Vec<SyncDeviceRecord>, StorageError> {
        let connection = self.connection()?;
        let mut statement = connection
            .prepare(
                "SELECT profile, device_id, label, membership_epoch, provider_authority,
                        created_at, last_seen_at
                 FROM sync_devices
                 WHERE profile = ?1
                 ORDER BY device_id",
            )
            .map_err(|source| self.database_error(source))?;
        let records = statement
            .query_map([profile], |row| {
                Ok(SyncDeviceRecord {
                    profile: row.get(0)?,
                    device_id: row.get(1)?,
                    label: row.get(2)?,
                    membership_epoch: row.get(3)?,
                    provider_authority: integer_to_bool(row.get(4)?),
                    created_at: row.get(5)?,
                    last_seen_at: row.get(6)?,
                })
            })
            .map_err(|source| self.database_error(source))?;

        let mut devices = Vec::new();
        for record in records {
            devices.push(record.map_err(|source| self.database_error(source))?);
        }
        Ok(devices)
    }

    pub fn register_sync_device_public_key(
        &self,
        registration: &SyncDevicePublicKeyRegistration,
    ) -> Result<SyncDevicePublicKeyRecord, StorageError> {
        let mut connection = self.connection()?;
        let transaction = connection
            .transaction()
            .map_err(|source| self.database_error(source))?;
        let record =
            self.register_sync_device_public_key_in_transaction(&transaction, registration)?;
        transaction
            .commit()
            .map_err(|source| self.database_error(source))?;
        Ok(record)
    }

    pub fn set_sync_device_public_key_trusted(
        &self,
        profile: &str,
        device_id: &str,
        trusted: bool,
    ) -> Result<Option<SyncDevicePublicKeyRecord>, StorageError> {
        if !is_valid_sync_identifier(device_id) {
            return Err(StorageError::InvalidSyncDeviceId(device_id.to_string()));
        }

        let connection = self.connection()?;
        let now = unix_time_seconds()?;
        let updated = connection
            .execute(
                "UPDATE sync_device_public_keys
                 SET trusted = ?3, updated_at = ?4
                 WHERE profile = ?1 AND device_id = ?2",
                params![profile, device_id, bool_to_integer(trusted), now],
            )
            .map_err(|source| self.database_error(source))?;
        if updated == 0 {
            return Ok(None);
        }
        self.sync_device_public_key(profile, device_id)
    }

    pub fn sync_device_public_key(
        &self,
        profile: &str,
        device_id: &str,
    ) -> Result<Option<SyncDevicePublicKeyRecord>, StorageError> {
        if !is_valid_sync_identifier(device_id) {
            return Err(StorageError::InvalidSyncDeviceId(device_id.to_string()));
        }

        let connection = self.connection()?;
        connection
            .query_row(
                "SELECT profile, device_id, public_key, membership_epoch, trusted, created_at,
                        updated_at
                 FROM sync_device_public_keys
                 WHERE profile = ?1 AND device_id = ?2",
                params![profile, device_id],
                sync_device_public_key_record_from_row,
            )
            .optional()
            .map_err(|source| self.database_error(source))
    }

    pub fn sync_device_public_keys(
        &self,
        profile: &str,
    ) -> Result<Vec<SyncDevicePublicKeyRecord>, StorageError> {
        let connection = self.connection()?;
        let mut statement = connection
            .prepare(
                "SELECT profile, device_id, public_key, membership_epoch, trusted, created_at,
                        updated_at
                 FROM sync_device_public_keys
                 WHERE profile = ?1
                 ORDER BY device_id",
            )
            .map_err(|source| self.database_error(source))?;
        let records = statement
            .query_map([profile], sync_device_public_key_record_from_row)
            .map_err(|source| self.database_error(source))?;

        let mut keys = Vec::new();
        for record in records {
            keys.push(record.map_err(|source| self.database_error(source))?);
        }
        Ok(keys)
    }

    fn register_sync_device_public_key_in_transaction(
        &self,
        transaction: &rusqlite::Transaction<'_>,
        registration: &SyncDevicePublicKeyRegistration,
    ) -> Result<SyncDevicePublicKeyRecord, StorageError> {
        if !is_valid_sync_identifier(registration.public_key.device_id.as_str()) {
            return Err(StorageError::InvalidSyncDeviceId(
                registration.public_key.device_id.clone(),
            ));
        }

        let now = unix_time_seconds()?;
        transaction
            .execute(
                "INSERT INTO sync_device_public_keys
                   (profile, device_id, public_key, membership_epoch, trusted, created_at,
                    updated_at)
                 VALUES (?1, ?2, ?3, ?4, 1, ?5, ?5)
                 ON CONFLICT(profile, device_id) DO UPDATE SET
                   public_key = excluded.public_key,
                   membership_epoch = excluded.membership_epoch,
                   trusted = 1,
                   updated_at = excluded.updated_at",
                params![
                    registration.profile.as_str(),
                    registration.public_key.device_id.as_str(),
                    registration.public_key.bytes.as_slice(),
                    registration.membership_epoch,
                    now
                ],
            )
            .map_err(|source| self.database_error(source))?;
        record_sync_device_roster_entry_in_transaction(
            transaction,
            registration.profile.as_str(),
            registration.public_key.device_id.as_str(),
            registration.membership_epoch,
            now,
        )
        .map_err(|source| self.database_error(source))?;

        self.sync_device_public_key_in_transaction(
            transaction,
            registration.profile.as_str(),
            registration.public_key.device_id.as_str(),
        )?
        .ok_or_else(|| self.database_error(rusqlite::Error::QueryReturnedNoRows))
    }

    fn set_sync_device_public_key_trusted_in_transaction(
        &self,
        transaction: &rusqlite::Transaction<'_>,
        profile: &str,
        device_id: &str,
        trusted: bool,
    ) -> Result<Option<SyncDevicePublicKeyRecord>, StorageError> {
        if !is_valid_sync_identifier(device_id) {
            return Err(StorageError::InvalidSyncDeviceId(device_id.to_string()));
        }

        let now = unix_time_seconds()?;
        let updated = transaction
            .execute(
                "UPDATE sync_device_public_keys
                 SET trusted = ?3, updated_at = ?4
                 WHERE profile = ?1 AND device_id = ?2",
                params![profile, device_id, bool_to_integer(trusted), now],
            )
            .map_err(|source| self.database_error(source))?;
        if updated == 0 {
            return Ok(None);
        }
        self.sync_device_public_key_in_transaction(transaction, profile, device_id)
    }

    fn sync_device_public_key_in_transaction(
        &self,
        transaction: &rusqlite::Transaction<'_>,
        profile: &str,
        device_id: &str,
    ) -> Result<Option<SyncDevicePublicKeyRecord>, StorageError> {
        if !is_valid_sync_identifier(device_id) {
            return Err(StorageError::InvalidSyncDeviceId(device_id.to_string()));
        }

        transaction
            .query_row(
                "SELECT profile, device_id, public_key, membership_epoch, trusted, created_at,
                        updated_at
                 FROM sync_device_public_keys
                 WHERE profile = ?1 AND device_id = ?2",
                params![profile, device_id],
                sync_device_public_key_record_from_row,
            )
            .optional()
            .map_err(|source| self.database_error(source))
    }

    fn sync_device_public_keys_in_transaction(
        &self,
        transaction: &rusqlite::Transaction<'_>,
        profile: &str,
    ) -> Result<Vec<SyncDevicePublicKeyRecord>, StorageError> {
        let mut statement = transaction
            .prepare(
                "SELECT profile, device_id, public_key, membership_epoch, trusted, created_at,
                        updated_at
                 FROM sync_device_public_keys
                 WHERE profile = ?1
                 ORDER BY device_id",
            )
            .map_err(|source| self.database_error(source))?;
        let records = statement
            .query_map([profile], sync_device_public_key_record_from_row)
            .map_err(|source| self.database_error(source))?;

        let mut keys = Vec::new();
        for record in records {
            keys.push(record.map_err(|source| self.database_error(source))?);
        }
        Ok(keys)
    }

    fn sync_device_provider_authority_in_transaction(
        &self,
        transaction: &rusqlite::Transaction<'_>,
        profile: &str,
        device_id: &str,
    ) -> Result<bool, StorageError> {
        if !is_valid_sync_identifier(device_id) {
            return Err(StorageError::InvalidSyncDeviceId(device_id.to_string()));
        }

        let provider_authority = transaction
            .query_row(
                "SELECT provider_authority
                 FROM sync_devices
                 WHERE profile = ?1 AND device_id = ?2",
                params![profile, device_id],
                |row| {
                    let value: i64 = row.get(0)?;
                    Ok(integer_to_bool(value))
                },
            )
            .optional()
            .map_err(|source| self.database_error(source))?;
        Ok(provider_authority.unwrap_or(false))
    }

    fn sync_device_provider_authority(
        &self,
        profile: &str,
        device_id: &str,
    ) -> Result<bool, StorageError> {
        if !is_valid_sync_identifier(device_id) {
            return Err(StorageError::InvalidSyncDeviceId(device_id.to_string()));
        }

        let connection = self.connection()?;
        let provider_authority = connection
            .query_row(
                "SELECT provider_authority
                 FROM sync_devices
                 WHERE profile = ?1 AND device_id = ?2",
                params![profile, device_id],
                |row| {
                    let value: i64 = row.get(0)?;
                    Ok(integer_to_bool(value))
                },
            )
            .optional()
            .map_err(|source| self.database_error(source))?;
        Ok(provider_authority.unwrap_or(false))
    }

    pub fn record_signed_sync_account_membership_record(
        &self,
        signed_record: &[u8],
    ) -> Result<SyncAccountMembershipRecord, StorageError> {
        let (signed_object, membership_record) =
            decode_signed_profile_sync_membership_record(signed_record)?;

        self.record_sync_account_membership_record(&SyncAccountMembershipRecordRegistration {
            profile: membership_record.profile,
            record_id: membership_record.record_id,
            membership_epoch: membership_record.membership_epoch,
            record_kind: membership_record.record_kind,
            device_id: membership_record.device_id,
            signer_device_id: signed_object.device_id,
            signed_record: signed_record.to_vec(),
        })
    }

    pub fn apply_signed_sync_account_membership_record(
        &self,
        signed_record: &[u8],
    ) -> Result<SyncAccountMembershipRecordApplication, StorageError> {
        let signed_records = [signed_record.to_vec()];
        let mut applications =
            self.apply_signed_sync_account_membership_records(&signed_records)?;
        applications
            .pop()
            .ok_or_else(|| self.database_error(rusqlite::Error::QueryReturnedNoRows))
    }

    pub fn apply_profile_sync_enrollment_bundle(
        &self,
        bundle: &ProfileSyncEnrollmentBundle,
    ) -> Result<Vec<SyncAccountMembershipRecordApplication>, StorageError> {
        validate_profile_sync_enrollment_bundle(bundle)?;
        if bundle.target_device_id != self.local_sync_device_id.as_str() {
            return Err(StorageError::InvalidProfileSyncEnrollmentBundle(format!(
                "bundle targets device {}, but this database is for device {}",
                bundle.target_device_id,
                self.local_sync_device_id.as_str()
            )));
        }
        self.apply_signed_sync_account_membership_records(&bundle.signed_membership_records)
    }

    pub fn apply_profile_sync_secret_handoff_bundle(
        &self,
        bundle: &ProfileSyncSecretHandoffBundle,
    ) -> Result<ProfileSyncSecretHandoffApplication, StorageError> {
        validate_profile_sync_secret_handoff_bundle(bundle)?;
        if bundle.target_device_id != self.local_sync_device_id.as_str() {
            return Err(StorageError::InvalidProfileSyncSecretHandoffBundle(
                format!(
                    "handoff targets device {}, but this database is for device {}",
                    bundle.target_device_id,
                    self.local_sync_device_id.as_str()
                ),
            ));
        }
        let sync_secret = bundle.to_sync_secret()?;
        let enrollment_applications =
            self.apply_profile_sync_enrollment_bundle(&bundle.enrollment_bundle)?;
        let activation =
            self.activate_local_profile_sync_from_secret(bundle.profile.as_str(), &sync_secret)?;
        Ok(ProfileSyncSecretHandoffApplication {
            enrollment_applications,
            activation,
        })
    }

    pub fn apply_signed_sync_account_membership_record_and_set_profile_sync_root(
        &self,
        profile: &str,
        root_id: &str,
        object_id: &str,
        signed_record: &[u8],
    ) -> Result<
        (
            ProfileSyncRootRecord,
            SyncAccountMembershipRecordApplication,
        ),
        StorageError,
    > {
        let signed_records = [signed_record.to_vec()];
        let (root, mut applications) = self
            .apply_signed_sync_account_membership_records_and_set_profile_sync_root(
                profile,
                root_id,
                object_id,
                &signed_records,
            )?;
        let application = applications
            .pop()
            .ok_or_else(|| self.database_error(rusqlite::Error::QueryReturnedNoRows))?;
        Ok((root, application))
    }

    pub fn apply_signed_sync_account_membership_records(
        &self,
        signed_records: &[Vec<u8>],
    ) -> Result<Vec<SyncAccountMembershipRecordApplication>, StorageError> {
        let decoded_records = signed_records
            .iter()
            .map(|signed_record| {
                decode_signed_profile_sync_membership_record(signed_record.as_slice())
            })
            .collect::<Result<Vec<_>, _>>()?;
        let mut connection = self.connection()?;
        let transaction = connection
            .transaction()
            .map_err(|source| self.database_error(source))?;
        let mut applications = Vec::with_capacity(signed_records.len());
        for (signed_record, (signed_object, membership_record)) in
            signed_records.iter().zip(decoded_records.iter())
        {
            applications.push(
                self.apply_decoded_sync_account_membership_record_in_transaction(
                    &transaction,
                    signed_object,
                    membership_record,
                    signed_record.as_slice(),
                )?,
            );
        }
        transaction
            .commit()
            .map_err(|source| self.database_error(source))?;
        Ok(applications)
    }

    pub fn apply_signed_sync_account_membership_records_and_set_profile_sync_root(
        &self,
        profile: &str,
        root_id: &str,
        object_id: &str,
        signed_records: &[Vec<u8>],
    ) -> Result<
        (
            ProfileSyncRootRecord,
            Vec<SyncAccountMembershipRecordApplication>,
        ),
        StorageError,
    > {
        let decoded_records = signed_records
            .iter()
            .map(|signed_record| {
                decode_signed_profile_sync_membership_record(signed_record.as_slice())
            })
            .collect::<Result<Vec<_>, _>>()?;
        let mut connection = self.connection()?;
        let transaction = connection
            .transaction()
            .map_err(|source| self.database_error(source))?;
        let mut applications = Vec::with_capacity(signed_records.len());
        for (signed_record, (signed_object, membership_record)) in
            signed_records.iter().zip(decoded_records.iter())
        {
            applications.push(
                self.apply_decoded_sync_account_membership_record_in_transaction(
                    &transaction,
                    signed_object,
                    membership_record,
                    signed_record.as_slice(),
                )?,
            );
        }
        let root =
            self.set_profile_sync_root_in_transaction(&transaction, profile, root_id, object_id)?;
        transaction
            .commit()
            .map_err(|source| self.database_error(source))?;
        Ok((root, applications))
    }

    fn apply_decoded_sync_account_membership_record_in_transaction(
        &self,
        transaction: &rusqlite::Transaction<'_>,
        signed_object: &SignedSyncObject,
        membership_record: &ProfileSyncMembershipRecord,
        signed_record: &[u8],
    ) -> Result<SyncAccountMembershipRecordApplication, StorageError> {
        let bootstrapped = self.authorize_sync_account_membership_record_in_transaction(
            transaction,
            signed_object,
            membership_record,
        )?;
        if let Some(existing_record) = self.sync_account_membership_record_in_transaction(
            transaction,
            membership_record.profile.as_str(),
            membership_record.record_id.as_str(),
        )? {
            if existing_record.signed_record != signed_record {
                return Err(StorageError::InvalidProfileSyncMembershipRecord(format!(
                    "membership record {} already exists with different signed bytes",
                    membership_record.record_id
                )));
            }
            if existing_record.applied_at.is_some() {
                return Ok(SyncAccountMembershipRecordApplication {
                    device_key: self.sync_device_public_key_in_transaction(
                        transaction,
                        existing_record.profile.as_str(),
                        existing_record.device_id.as_str(),
                    )?,
                    membership_record: existing_record,
                    bootstrapped,
                    applied: false,
                });
            }
        }
        self.reject_stale_sync_account_membership_record_in_transaction(
            transaction,
            membership_record,
        )?;
        self.reject_invalid_sync_account_membership_transition_in_transaction(
            transaction,
            membership_record,
        )?;
        let stored_record = self.record_sync_account_membership_record_in_transaction(
            transaction,
            &SyncAccountMembershipRecordRegistration {
                profile: membership_record.profile.clone(),
                record_id: membership_record.record_id.clone(),
                membership_epoch: membership_record.membership_epoch,
                record_kind: membership_record.record_kind.clone(),
                device_id: membership_record.device_id.clone(),
                signer_device_id: signed_object.device_id.clone(),
                signed_record: signed_record.to_vec(),
            },
        )?;

        let device_key = match membership_record.record_kind.as_str() {
            PROFILE_SYNC_MEMBERSHIP_RECORD_KIND_ENROLL_DEVICE
            | PROFILE_SYNC_MEMBERSHIP_RECORD_KIND_ENROLL_PROVIDER
            | PROFILE_SYNC_MEMBERSHIP_RECORD_KIND_ROTATE_DEVICE_KEY => {
                Some(self.register_sync_device_public_key_in_transaction(
                    transaction,
                    &SyncDevicePublicKeyRegistration {
                        profile: membership_record.profile.clone(),
                        public_key: membership_record.device_public_key.clone().ok_or_else(
                            || {
                                StorageError::InvalidProfileSyncMembershipRecord(format!(
                                    "{} requires a device public key",
                                    membership_record.record_kind
                                ))
                            },
                        )?,
                        membership_epoch: membership_record.membership_epoch,
                    },
                )?)
            }
            PROFILE_SYNC_MEMBERSHIP_RECORD_KIND_REVOKE_DEVICE => self
                .set_sync_device_public_key_trusted_in_transaction(
                    transaction,
                    membership_record.profile.as_str(),
                    membership_record.device_id.as_str(),
                    false,
                )?,
            _ => {
                return Err(StorageError::InvalidSyncMembershipRecordKind(
                    membership_record.record_kind.clone(),
                ));
            }
        };
        self.record_sync_membership_device_in_transaction(transaction, membership_record)?;

        let applied_record = self.mark_sync_account_membership_record_applied_in_transaction(
            transaction,
            stored_record.profile.as_str(),
            stored_record.record_id.as_str(),
        )?;
        Ok(SyncAccountMembershipRecordApplication {
            membership_record: applied_record,
            device_key,
            bootstrapped,
            applied: true,
        })
    }

    fn authorize_sync_account_membership_record_in_transaction(
        &self,
        transaction: &rusqlite::Transaction<'_>,
        signed_object: &SignedSyncObject,
        membership_record: &ProfileSyncMembershipRecord,
    ) -> Result<bool, StorageError> {
        let known_keys = self.sync_device_public_keys_in_transaction(
            transaction,
            membership_record.profile.as_str(),
        )?;
        let bootstrapped = known_keys.is_empty()
            && membership_record.record_kind == PROFILE_SYNC_MEMBERSHIP_RECORD_KIND_ENROLL_DEVICE
            && membership_record.device_id == signed_object.device_id
            && membership_record
                .device_public_key
                .as_ref()
                .is_some_and(|public_key| {
                    public_key.device_id == signed_object.device_id
                        && public_key.bytes == signed_object.public_key
                });
        if bootstrapped {
            return Ok(true);
        }

        let trusted_signer = self
            .sync_device_public_key_in_transaction(
                transaction,
                membership_record.profile.as_str(),
                signed_object.device_id.as_str(),
            )?
            .filter(|record| record.trusted)
            .ok_or_else(|| StorageError::UntrustedSyncMembershipSigner {
                profile: membership_record.profile.clone(),
                device_id: signed_object.device_id.clone(),
            })?;
        if self.sync_device_provider_authority_in_transaction(
            transaction,
            membership_record.profile.as_str(),
            signed_object.device_id.as_str(),
        )? {
            return Err(StorageError::InvalidProfileSyncMembershipRecord(format!(
                "membership signer {} is marked as provider authority and cannot authorize account membership records",
                signed_object.device_id
            )));
        }
        if trusted_signer.membership_epoch > membership_record.membership_epoch {
            return Err(StorageError::InvalidProfileSyncMembershipRecord(format!(
                "membership signer {} was trusted at epoch {}, after membership record epoch {}",
                signed_object.device_id,
                trusted_signer.membership_epoch,
                membership_record.membership_epoch
            )));
        }
        signed_object
            .verify_with(&trusted_signer.public_key)
            .map_err(|error| StorageError::InvalidProfileSyncMembershipRecord(error.to_string()))?;
        Ok(false)
    }

    fn record_sync_membership_device_in_transaction(
        &self,
        transaction: &rusqlite::Transaction<'_>,
        membership_record: &ProfileSyncMembershipRecord,
    ) -> Result<(), StorageError> {
        let now = unix_time_seconds()?;
        match membership_record.record_kind.as_str() {
            PROFILE_SYNC_MEMBERSHIP_RECORD_KIND_ENROLL_DEVICE => {
                record_sync_device_roster_entry_with_provider_authority_in_transaction(
                    transaction,
                    membership_record.profile.as_str(),
                    membership_record.device_id.as_str(),
                    membership_record.membership_epoch,
                    false,
                    now,
                )
            }
            PROFILE_SYNC_MEMBERSHIP_RECORD_KIND_ENROLL_PROVIDER => {
                record_sync_device_roster_entry_with_provider_authority_in_transaction(
                    transaction,
                    membership_record.profile.as_str(),
                    membership_record.device_id.as_str(),
                    membership_record.membership_epoch,
                    true,
                    now,
                )
            }
            _ => record_sync_device_roster_entry_in_transaction(
                transaction,
                membership_record.profile.as_str(),
                membership_record.device_id.as_str(),
                membership_record.membership_epoch,
                now,
            ),
        }
        .map_err(|source| self.database_error(source))?;
        Ok(())
    }

    fn reject_stale_sync_account_membership_record_in_transaction(
        &self,
        transaction: &rusqlite::Transaction<'_>,
        membership_record: &ProfileSyncMembershipRecord,
    ) -> Result<(), StorageError> {
        let Some(latest_epoch) = self.latest_applied_sync_account_membership_epoch_in_transaction(
            transaction,
            membership_record.profile.as_str(),
            membership_record.device_id.as_str(),
        )?
        else {
            return Ok(());
        };
        if membership_record.membership_epoch < latest_epoch {
            return Err(StorageError::InvalidProfileSyncMembershipRecord(format!(
                "membership record {} at epoch {} is older than latest applied epoch {} for device {}",
                membership_record.record_id,
                membership_record.membership_epoch,
                latest_epoch,
                membership_record.device_id
            )));
        }
        if membership_record.membership_epoch == latest_epoch
            && let Some(applied_record) = self
                .applied_sync_account_membership_record_for_device_epoch_in_transaction(
                    transaction,
                    membership_record.profile.as_str(),
                    membership_record.device_id.as_str(),
                    membership_record.membership_epoch,
                )?
        {
            return Err(StorageError::InvalidProfileSyncMembershipRecord(format!(
                "membership record {} conflicts with already-applied record {} at epoch {} for device {}",
                membership_record.record_id,
                applied_record.record_id,
                membership_record.membership_epoch,
                membership_record.device_id
            )));
        }
        Ok(())
    }

    fn reject_invalid_sync_account_membership_transition_in_transaction(
        &self,
        transaction: &rusqlite::Transaction<'_>,
        membership_record: &ProfileSyncMembershipRecord,
    ) -> Result<(), StorageError> {
        match membership_record.record_kind.as_str() {
            PROFILE_SYNC_MEMBERSHIP_RECORD_KIND_ENROLL_DEVICE
            | PROFILE_SYNC_MEMBERSHIP_RECORD_KIND_ENROLL_PROVIDER => {
                if self
                    .sync_device_public_key_in_transaction(
                        transaction,
                        membership_record.profile.as_str(),
                        membership_record.device_id.as_str(),
                    )?
                    .is_some_and(|existing_key| existing_key.trusted)
                {
                    return Err(StorageError::InvalidProfileSyncMembershipRecord(format!(
                        "{} record {} cannot replace already trusted device {}; use rotate-device-key",
                        membership_record.record_kind,
                        membership_record.record_id,
                        membership_record.device_id
                    )));
                }
            }
            PROFILE_SYNC_MEMBERSHIP_RECORD_KIND_ROTATE_DEVICE_KEY => {
                let Some(existing_key) = self.sync_device_public_key_in_transaction(
                    transaction,
                    membership_record.profile.as_str(),
                    membership_record.device_id.as_str(),
                )?
                else {
                    return Err(StorageError::InvalidProfileSyncMembershipRecord(format!(
                        "rotate-device-key record {} requires an existing trusted key for device {}",
                        membership_record.record_id, membership_record.device_id
                    )));
                };
                if !existing_key.trusted {
                    return Err(StorageError::InvalidProfileSyncMembershipRecord(format!(
                        "rotate-device-key record {} cannot re-trust revoked device {}; use enroll-device",
                        membership_record.record_id, membership_record.device_id
                    )));
                }
            }
            _ => {}
        }
        Ok(())
    }

    fn applied_sync_account_membership_record_for_device_epoch_in_transaction(
        &self,
        transaction: &rusqlite::Transaction<'_>,
        profile: &str,
        device_id: &str,
        membership_epoch: i64,
    ) -> Result<Option<SyncAccountMembershipRecord>, StorageError> {
        if !is_valid_sync_identifier(device_id) {
            return Err(StorageError::InvalidSyncDeviceId(device_id.to_string()));
        }

        transaction
            .query_row(
                "SELECT profile, record_id, membership_epoch, record_kind, device_id,
                        signer_device_id, signed_record, created_at, applied_at
                 FROM sync_account_membership_records
                 WHERE profile = ?1
                   AND device_id = ?2
                   AND membership_epoch = ?3
                   AND applied_at IS NOT NULL
                 ORDER BY record_id
                 LIMIT 1",
                params![profile, device_id, membership_epoch],
                sync_account_membership_record_from_row,
            )
            .optional()
            .map_err(|source| self.database_error(source))
    }

    fn latest_applied_sync_account_membership_epoch_in_transaction(
        &self,
        transaction: &rusqlite::Transaction<'_>,
        profile: &str,
        device_id: &str,
    ) -> Result<Option<i64>, StorageError> {
        if !is_valid_sync_identifier(device_id) {
            return Err(StorageError::InvalidSyncDeviceId(device_id.to_string()));
        }

        transaction
            .query_row(
                "SELECT MAX(membership_epoch)
                 FROM sync_account_membership_records
                 WHERE profile = ?1 AND device_id = ?2 AND applied_at IS NOT NULL",
                params![profile, device_id],
                |row| row.get::<_, Option<i64>>(0),
            )
            .map_err(|source| self.database_error(source))
    }

    fn record_sync_account_membership_record_in_transaction(
        &self,
        transaction: &rusqlite::Transaction<'_>,
        registration: &SyncAccountMembershipRecordRegistration,
    ) -> Result<SyncAccountMembershipRecord, StorageError> {
        validate_sync_account_membership_registration(registration)?;

        transaction
            .execute(
                "INSERT OR IGNORE INTO sync_account_membership_records
                   (profile, record_id, membership_epoch, record_kind, device_id,
                    signer_device_id, signed_record, created_at, applied_at)
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, NULL)",
                params![
                    registration.profile.as_str(),
                    registration.record_id.as_str(),
                    registration.membership_epoch,
                    registration.record_kind.as_str(),
                    registration.device_id.as_str(),
                    registration.signer_device_id.as_str(),
                    registration.signed_record.as_slice(),
                    unix_time_seconds()?,
                ],
            )
            .map_err(|source| self.database_error(source))?;

        let record = self
            .sync_account_membership_record_in_transaction(
                transaction,
                registration.profile.as_str(),
                registration.record_id.as_str(),
            )?
            .ok_or_else(|| self.database_error(rusqlite::Error::QueryReturnedNoRows))?;
        if record.signed_record != registration.signed_record {
            return Err(StorageError::InvalidProfileSyncMembershipRecord(format!(
                "membership record {} already exists with different signed bytes",
                registration.record_id
            )));
        }
        Ok(record)
    }

    fn mark_sync_account_membership_record_applied_in_transaction(
        &self,
        transaction: &rusqlite::Transaction<'_>,
        profile: &str,
        record_id: &str,
    ) -> Result<SyncAccountMembershipRecord, StorageError> {
        let now = unix_time_seconds()?;
        let updated = transaction
            .execute(
                "UPDATE sync_account_membership_records
                 SET applied_at = COALESCE(applied_at, ?3)
                 WHERE profile = ?1 AND record_id = ?2",
                params![profile, record_id, now],
            )
            .map_err(|source| self.database_error(source))?;
        if updated == 0 {
            return Err(self.database_error(rusqlite::Error::QueryReturnedNoRows));
        }
        self.sync_account_membership_record_in_transaction(transaction, profile, record_id)?
            .ok_or_else(|| self.database_error(rusqlite::Error::QueryReturnedNoRows))
    }

    fn sync_account_membership_record_in_transaction(
        &self,
        transaction: &rusqlite::Transaction<'_>,
        profile: &str,
        record_id: &str,
    ) -> Result<Option<SyncAccountMembershipRecord>, StorageError> {
        if !is_valid_sync_identifier(record_id) {
            return Err(StorageError::InvalidSyncMembershipRecordId(
                record_id.to_string(),
            ));
        }

        transaction
            .query_row(
                "SELECT profile, record_id, membership_epoch, record_kind, device_id,
                        signer_device_id, signed_record, created_at, applied_at
                 FROM sync_account_membership_records
                 WHERE profile = ?1 AND record_id = ?2",
                params![profile, record_id],
                sync_account_membership_record_from_row,
            )
            .optional()
            .map_err(|source| self.database_error(source))
    }

    fn record_sync_account_membership_record(
        &self,
        registration: &SyncAccountMembershipRecordRegistration,
    ) -> Result<SyncAccountMembershipRecord, StorageError> {
        validate_sync_account_membership_registration(registration)?;

        let connection = self.connection()?;
        connection
            .execute(
                "INSERT OR IGNORE INTO sync_account_membership_records
                   (profile, record_id, membership_epoch, record_kind, device_id,
                    signer_device_id, signed_record, created_at, applied_at)
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, NULL)",
                params![
                    registration.profile.as_str(),
                    registration.record_id.as_str(),
                    registration.membership_epoch,
                    registration.record_kind.as_str(),
                    registration.device_id.as_str(),
                    registration.signer_device_id.as_str(),
                    registration.signed_record.as_slice(),
                    unix_time_seconds()?,
                ],
            )
            .map_err(|source| self.database_error(source))?;

        let record = self
            .sync_account_membership_record(
                registration.profile.as_str(),
                registration.record_id.as_str(),
            )?
            .ok_or_else(|| self.database_error(rusqlite::Error::QueryReturnedNoRows))?;
        if record.signed_record != registration.signed_record {
            return Err(StorageError::InvalidProfileSyncMembershipRecord(format!(
                "membership record {} already exists with different signed bytes",
                registration.record_id
            )));
        }
        Ok(record)
    }

    pub fn sync_account_membership_record(
        &self,
        profile: &str,
        record_id: &str,
    ) -> Result<Option<SyncAccountMembershipRecord>, StorageError> {
        if !is_valid_sync_identifier(record_id) {
            return Err(StorageError::InvalidSyncMembershipRecordId(
                record_id.to_string(),
            ));
        }

        let connection = self.connection()?;
        connection
            .query_row(
                "SELECT profile, record_id, membership_epoch, record_kind, device_id,
                        signer_device_id, signed_record, created_at, applied_at
                 FROM sync_account_membership_records
                 WHERE profile = ?1 AND record_id = ?2",
                params![profile, record_id],
                sync_account_membership_record_from_row,
            )
            .optional()
            .map_err(|source| self.database_error(source))
    }

    pub fn sync_account_membership_record_count(
        &self,
        profile: &str,
    ) -> Result<usize, StorageError> {
        let connection = self.connection()?;
        let count = connection
            .query_row(
                "SELECT COUNT(*)
                 FROM sync_account_membership_records
                 WHERE profile = ?1",
                [profile],
                |row| row.get::<_, i64>(0),
            )
            .map_err(|source| self.database_error(source))?;
        usize::try_from(count).map_err(|_| {
            StorageError::InvalidProfileSyncMembershipRecord(format!(
                "membership record count out of range: {count}"
            ))
        })
    }

    pub fn sync_account_membership_records(
        &self,
        profile: &str,
    ) -> Result<Vec<SyncAccountMembershipRecord>, StorageError> {
        let connection = self.connection()?;
        let mut statement = connection
            .prepare(
                "SELECT profile, record_id, membership_epoch, record_kind, device_id,
                        signer_device_id, signed_record, created_at, applied_at
                 FROM sync_account_membership_records
                 WHERE profile = ?1
                 ORDER BY membership_epoch, record_id",
            )
            .map_err(|source| self.database_error(source))?;
        let records = statement
            .query_map([profile], sync_account_membership_record_from_row)
            .map_err(|source| self.database_error(source))?;

        let mut membership_records = Vec::new();
        for record in records {
            membership_records.push(record.map_err(|source| self.database_error(source))?);
        }
        Ok(membership_records)
    }

    pub fn register_sync_content_key_epoch(
        &self,
        registration: &SyncContentKeyEpochRegistration,
    ) -> Result<SyncContentKeyEpochRecord, StorageError> {
        if !is_valid_sync_identifier(registration.key_id.as_str()) {
            return Err(StorageError::InvalidSyncContentKeyId(
                registration.key_id.clone(),
            ));
        }

        let now = unix_time_seconds()?;
        let mut connection = self.connection()?;
        let transaction = connection
            .transaction()
            .map_err(|source| self.database_error(source))?;
        if registration.active {
            transaction
                .execute(
                    "UPDATE sync_content_key_epochs
                     SET active = 0, updated_at = ?2
                     WHERE profile = ?1 AND active = 1",
                    params![registration.profile.as_str(), now],
                )
                .map_err(|source| self.database_error(source))?;
        }
        transaction
            .execute(
                "INSERT INTO sync_content_key_epochs
                   (profile, key_id, membership_epoch, algorithm, active, created_at, updated_at)
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?6)
                 ON CONFLICT(profile, key_id) DO UPDATE SET
                   membership_epoch = excluded.membership_epoch,
                   algorithm = excluded.algorithm,
                   active = excluded.active,
                   updated_at = excluded.updated_at",
                params![
                    registration.profile.as_str(),
                    registration.key_id.as_str(),
                    registration.membership_epoch,
                    registration.algorithm.as_str(),
                    bool_to_integer(registration.active),
                    now
                ],
            )
            .map_err(|source| self.database_error(source))?;
        let record = transaction
            .query_row(
                "SELECT profile, key_id, membership_epoch, algorithm, active,
                        created_at, updated_at
                 FROM sync_content_key_epochs
                 WHERE profile = ?1 AND key_id = ?2",
                params![registration.profile.as_str(), registration.key_id.as_str()],
                sync_content_key_epoch_record_from_row,
            )
            .map_err(|source| self.database_error(source))?;
        transaction
            .commit()
            .map_err(|source| self.database_error(source))?;
        Ok(record)
    }

    pub fn sync_content_key_epoch(
        &self,
        profile: &str,
        key_id: &str,
    ) -> Result<Option<SyncContentKeyEpochRecord>, StorageError> {
        if !is_valid_sync_identifier(key_id) {
            return Err(StorageError::InvalidSyncContentKeyId(key_id.to_string()));
        }

        let connection = self.connection()?;
        connection
            .query_row(
                "SELECT profile, key_id, membership_epoch, algorithm, active,
                        created_at, updated_at
                 FROM sync_content_key_epochs
                 WHERE profile = ?1 AND key_id = ?2",
                params![profile, key_id],
                sync_content_key_epoch_record_from_row,
            )
            .optional()
            .map_err(|source| self.database_error(source))
    }

    pub fn sync_content_key_epochs(
        &self,
        profile: &str,
    ) -> Result<Vec<SyncContentKeyEpochRecord>, StorageError> {
        let connection = self.connection()?;
        let mut statement = connection
            .prepare(
                "SELECT profile, key_id, membership_epoch, algorithm, active,
                        created_at, updated_at
                 FROM sync_content_key_epochs
                 WHERE profile = ?1
                 ORDER BY membership_epoch, key_id",
            )
            .map_err(|source| self.database_error(source))?;
        let records = statement
            .query_map([profile], sync_content_key_epoch_record_from_row)
            .map_err(|source| self.database_error(source))?;

        let mut keys = Vec::new();
        for record in records {
            keys.push(record.map_err(|source| self.database_error(source))?);
        }
        Ok(keys)
    }

    pub fn active_sync_content_key_epoch(
        &self,
        profile: &str,
    ) -> Result<Option<SyncContentKeyEpochRecord>, StorageError> {
        let connection = self.connection()?;
        connection
            .query_row(
                "SELECT profile, key_id, membership_epoch, algorithm, active,
                        created_at, updated_at
                 FROM sync_content_key_epochs
                 WHERE profile = ?1 AND active = 1
                 ORDER BY membership_epoch DESC, updated_at DESC
                 LIMIT 1",
                [profile],
                sync_content_key_epoch_record_from_row,
            )
            .optional()
            .map_err(|source| self.database_error(source))
    }

    pub fn activate_local_profile_sync_metadata(
        &self,
        profile: &str,
    ) -> Result<ProfileSyncLocalActivationRecord, StorageError> {
        self.activate_local_profile_sync_metadata_with_key(
            profile,
            DEFAULT_PROFILE_SYNC_CONTENT_KEY_ID,
            DEFAULT_PROFILE_SYNC_MEMBERSHIP_EPOCH,
        )
    }

    pub fn activate_local_profile_sync_metadata_with_key(
        &self,
        profile: &str,
        key_id: &str,
        membership_epoch: i64,
    ) -> Result<ProfileSyncLocalActivationRecord, StorageError> {
        self.register_sync_device(&SyncDeviceRegistration {
            profile: profile.to_string(),
            device_id: self.local_sync_device_id().to_string(),
            label: Some("Local Device".to_string()),
            membership_epoch,
            provider_authority: false,
        })?;
        self.ensure_default_app_sync_domains(profile)?;
        let content_key_epoch =
            self.register_sync_content_key_epoch(&SyncContentKeyEpochRegistration {
                profile: profile.to_string(),
                key_id: key_id.to_string(),
                membership_epoch,
                algorithm: PROFILE_SYNC_CONTENT_KEY_ALGORITHM_CHACHA20_POLY1305.to_string(),
                active: true,
            })?;

        Ok(ProfileSyncLocalActivationRecord {
            profile: profile.to_string(),
            device_id: self.local_sync_device_id().to_string(),
            content_key_epoch,
        })
    }

    pub fn activate_local_profile_sync_from_secret(
        &self,
        profile: &str,
        sync_secret: &SlateSyncSecret,
    ) -> Result<ProfileSyncLocalSecretActivationRecord, StorageError> {
        self.activate_local_profile_sync_from_secret_with_key(
            profile,
            sync_secret,
            DEFAULT_PROFILE_SYNC_CONTENT_KEY_ID,
            DEFAULT_PROFILE_SYNC_MEMBERSHIP_EPOCH,
        )
    }

    pub fn profile_sync_enrollment_bundle_from_secret(
        profile: &str,
        sync_secret: &SlateSyncSecret,
        target_device_id: &str,
    ) -> Result<ProfileSyncEnrollmentBundle, StorageError> {
        Self::profile_sync_enrollment_bundle_from_secret_with_epoch(
            profile,
            sync_secret,
            target_device_id,
            DEFAULT_PROFILE_SYNC_MEMBERSHIP_EPOCH,
            unix_time_seconds()?,
        )
    }

    pub fn profile_sync_enrollment_bundle_from_device_request(
        profile: &str,
        sync_secret: &SlateSyncSecret,
        request: &ProfileSyncDeviceEnrollmentRequest,
    ) -> Result<ProfileSyncEnrollmentBundle, StorageError> {
        if request.profile != profile {
            return Err(StorageError::InvalidProfileSyncDeviceEnrollmentRequest(
                format!("expected profile {profile}, got {}", request.profile),
            ));
        }
        Self::profile_sync_enrollment_bundle_from_secret(
            profile,
            sync_secret,
            request.device_id.as_str(),
        )
    }

    pub fn profile_sync_secret_handoff_bundle_from_secret(
        profile: &str,
        sync_secret: &SlateSyncSecret,
        target_device_id: &str,
    ) -> Result<ProfileSyncSecretHandoffBundle, StorageError> {
        Self::profile_sync_secret_handoff_bundle_from_secret_with_epoch(
            profile,
            sync_secret,
            target_device_id,
            DEFAULT_PROFILE_SYNC_MEMBERSHIP_EPOCH,
            unix_time_seconds()?,
        )
    }

    pub fn profile_sync_secret_handoff_bundle_from_device_request(
        profile: &str,
        sync_secret: &SlateSyncSecret,
        request: &ProfileSyncDeviceEnrollmentRequest,
    ) -> Result<ProfileSyncSecretHandoffBundle, StorageError> {
        if request.profile != profile {
            return Err(StorageError::InvalidProfileSyncDeviceEnrollmentRequest(
                format!("expected profile {profile}, got {}", request.profile),
            ));
        }
        Self::profile_sync_secret_handoff_bundle_from_secret(
            profile,
            sync_secret,
            request.device_id.as_str(),
        )
    }

    pub fn profile_sync_secret_handoff_bundle_from_secret_with_epoch(
        profile: &str,
        sync_secret: &SlateSyncSecret,
        target_device_id: &str,
        membership_epoch: i64,
        created_at: i64,
    ) -> Result<ProfileSyncSecretHandoffBundle, StorageError> {
        let sync_secret_export = sync_secret.export_for_profile(profile, created_at);
        let enrollment_bundle = Self::profile_sync_enrollment_bundle_from_secret_with_epoch(
            profile,
            sync_secret,
            target_device_id,
            membership_epoch,
            created_at,
        )?;
        ProfileSyncSecretHandoffBundle::new(
            profile,
            target_device_id,
            sync_secret_export,
            enrollment_bundle,
            created_at,
        )
    }

    pub fn profile_sync_enrollment_bundle_from_secret_with_epoch(
        profile: &str,
        sync_secret: &SlateSyncSecret,
        target_device_id: &str,
        membership_epoch: i64,
        created_at: i64,
    ) -> Result<ProfileSyncEnrollmentBundle, StorageError> {
        let account_authority_signer = sync_secret
            .derive_profile_sync_device_signer(
                profile,
                DEFAULT_PROFILE_SYNC_ACCOUNT_AUTHORITY_DEVICE_ID,
                membership_epoch,
            )
            .map_err(profile_sync_membership_record_error)?;
        let account_authority_record = profile_sync_enroll_device_record(
            profile,
            DEFAULT_PROFILE_SYNC_ACCOUNT_AUTHORITY_DEVICE_ID,
            membership_epoch,
            account_authority_signer
                .public_key()
                .map_err(profile_sync_membership_record_error)?,
        );
        let mut signed_records = vec![signed_profile_sync_membership_record_bytes(
            &account_authority_signer,
            &account_authority_record,
        )?];

        if target_device_id != DEFAULT_PROFILE_SYNC_ACCOUNT_AUTHORITY_DEVICE_ID {
            let target_signer = sync_secret
                .derive_profile_sync_device_signer(profile, target_device_id, membership_epoch)
                .map_err(profile_sync_membership_record_error)?;
            let target_record = profile_sync_enroll_device_record(
                profile,
                target_device_id,
                membership_epoch,
                target_signer
                    .public_key()
                    .map_err(profile_sync_membership_record_error)?,
            );
            signed_records.push(signed_profile_sync_membership_record_bytes(
                &account_authority_signer,
                &target_record,
            )?);
        }

        ProfileSyncEnrollmentBundle::new_device_enrollment(
            profile,
            target_device_id,
            signed_records,
            created_at,
        )
    }

    pub fn activate_local_profile_sync_from_secret_with_key(
        &self,
        profile: &str,
        sync_secret: &SlateSyncSecret,
        key_id: &str,
        membership_epoch: i64,
    ) -> Result<ProfileSyncLocalSecretActivationRecord, StorageError> {
        let activation =
            self.activate_local_profile_sync_metadata_with_key(profile, key_id, membership_epoch)?;
        let account_authority_signer = sync_secret
            .derive_profile_sync_device_signer(
                profile,
                DEFAULT_PROFILE_SYNC_ACCOUNT_AUTHORITY_DEVICE_ID,
                membership_epoch,
            )
            .map_err(profile_sync_membership_record_error)?;
        let account_authority_public_key = account_authority_signer
            .public_key()
            .map_err(profile_sync_membership_record_error)?;
        let account_authority_record = profile_sync_enroll_device_record(
            profile,
            DEFAULT_PROFILE_SYNC_ACCOUNT_AUTHORITY_DEVICE_ID,
            membership_epoch,
            account_authority_public_key,
        );
        let account_authority_signed_record = signed_profile_sync_membership_record_bytes(
            &account_authority_signer,
            &account_authority_record,
        )?;
        let mut membership_applications = Vec::new();
        membership_applications.push(self.apply_signed_sync_account_membership_record(
            account_authority_signed_record.as_slice(),
        )?);

        if self.local_sync_device_id() != DEFAULT_PROFILE_SYNC_ACCOUNT_AUTHORITY_DEVICE_ID {
            let local_signer = sync_secret
                .derive_profile_sync_device_signer(
                    profile,
                    self.local_sync_device_id(),
                    membership_epoch,
                )
                .map_err(profile_sync_membership_record_error)?;
            let local_record = profile_sync_enroll_device_record(
                profile,
                self.local_sync_device_id(),
                membership_epoch,
                local_signer
                    .public_key()
                    .map_err(profile_sync_membership_record_error)?,
            );
            let local_signed_record = signed_profile_sync_membership_record_bytes(
                &account_authority_signer,
                &local_record,
            )?;
            membership_applications.push(
                self.apply_signed_sync_account_membership_record(local_signed_record.as_slice())?,
            );
        }

        Ok(ProfileSyncLocalSecretActivationRecord {
            activation,
            account_authority_device_id: DEFAULT_PROFILE_SYNC_ACCOUNT_AUTHORITY_DEVICE_ID
                .to_string(),
            local_device_id: self.local_sync_device_id().to_string(),
            membership_applications,
        })
    }

    pub fn activate_local_profile_sync_preview_provider(
        &self,
        profile: &str,
        endpoint_ref: Option<String>,
    ) -> Result<StorageProviderRecord, StorageError> {
        let provider_signer = ProfileSyncDeviceSigner::generate(
            DEFAULT_PROFILE_SYNC_PREVIEW_PROVIDER_ID,
        )
        .map_err(|error| {
            StorageError::InvalidProfileSyncMembershipRecord(format!(
                "failed to create preview provider key: {error}"
            ))
        })?;
        self.register_sync_device(&SyncDeviceRegistration {
            profile: profile.to_string(),
            device_id: DEFAULT_PROFILE_SYNC_PREVIEW_PROVIDER_ID.to_string(),
            label: Some("Local Preview Provider".to_string()),
            membership_epoch: DEFAULT_PROFILE_SYNC_MEMBERSHIP_EPOCH,
            provider_authority: true,
        })?;
        self.register_sync_device_public_key(&SyncDevicePublicKeyRegistration {
            profile: profile.to_string(),
            public_key: provider_signer.public_key().map_err(|error| {
                StorageError::InvalidProfileSyncMembershipRecord(format!(
                    "failed to read preview provider public key: {error}"
                ))
            })?,
            membership_epoch: DEFAULT_PROFILE_SYNC_MEMBERSHIP_EPOCH,
        })?;
        self.upsert_local_profile_sync_preview_provider(profile, endpoint_ref)
    }

    pub fn activate_local_profile_sync_preview_provider_from_secret(
        &self,
        profile: &str,
        sync_secret: &SlateSyncSecret,
        endpoint_ref: Option<String>,
    ) -> Result<ProfileSyncPreviewProviderActivationRecord, StorageError> {
        let membership_epoch = DEFAULT_PROFILE_SYNC_MEMBERSHIP_EPOCH;
        let account_authority_signer = sync_secret
            .derive_profile_sync_device_signer(
                profile,
                DEFAULT_PROFILE_SYNC_ACCOUNT_AUTHORITY_DEVICE_ID,
                membership_epoch,
            )
            .map_err(profile_sync_membership_record_error)?;
        let provider_signer = sync_secret
            .derive_profile_sync_device_signer(
                profile,
                DEFAULT_PROFILE_SYNC_PREVIEW_PROVIDER_ID,
                membership_epoch,
            )
            .map_err(profile_sync_membership_record_error)?;
        let provider_record = profile_sync_enroll_provider_record(
            profile,
            DEFAULT_PROFILE_SYNC_PREVIEW_PROVIDER_ID,
            membership_epoch,
            provider_signer
                .public_key()
                .map_err(profile_sync_membership_record_error)?,
        );
        let signed_provider_record = signed_profile_sync_membership_record_bytes(
            &account_authority_signer,
            &provider_record,
        )?;
        let membership_application =
            self.apply_signed_sync_account_membership_record(signed_provider_record.as_slice())?;
        let provider = self.upsert_local_profile_sync_preview_provider(profile, endpoint_ref)?;

        Ok(ProfileSyncPreviewProviderActivationRecord {
            provider,
            membership_application,
        })
    }

    fn upsert_local_profile_sync_preview_provider(
        &self,
        profile: &str,
        endpoint_ref: Option<String>,
    ) -> Result<StorageProviderRecord, StorageError> {
        self.upsert_storage_provider(&StorageProviderUpdate {
            profile: profile.to_string(),
            provider_id: DEFAULT_PROFILE_SYNC_PREVIEW_PROVIDER_ID.to_string(),
            provider_kind: DEFAULT_PROFILE_SYNC_PREVIEW_PROVIDER_KIND.to_string(),
            display_name: "Local Preview Provider".to_string(),
            endpoint_ref,
            discovery: true,
            connectivity: true,
            object_transfer: true,
            availability: true,
            mutable_roots: false,
            quota_bytes: None,
            max_retained_objects: Some(128),
            pinning_policy: Some("manual".to_string()),
            enabled: true,
        })
    }

    pub fn profile_sync_local_readiness(
        &self,
        profile: &str,
    ) -> Result<ProfileSyncLocalReadinessReport, StorageError> {
        let local_device_id = self.local_sync_device_id().to_string();
        let active_key_id = self
            .active_sync_content_key_epoch(profile)?
            .map(|content_key_epoch| content_key_epoch.key_id);
        let devices = self.sync_devices(profile)?;
        let local_device_registered = devices
            .iter()
            .any(|device| device.device_id == local_device_id && !device.provider_authority);
        let provider_authority_device_ids = devices
            .iter()
            .filter(|device| device.provider_authority)
            .map(|device| device.device_id.as_str())
            .collect::<BTreeSet<_>>();
        let public_keys = self.sync_device_public_keys(profile)?;
        let trusted_device_count = public_keys.iter().filter(|record| record.trusted).count();
        let local_device_trusted = public_keys
            .iter()
            .any(|record| record.trusted && record.public_key.device_id == local_device_id);
        let account_authority_trusted = public_keys.iter().any(|record| {
            record.trusted
                && record.public_key.device_id == DEFAULT_PROFILE_SYNC_ACCOUNT_AUTHORITY_DEVICE_ID
        });
        let trusted_provider_authority_device_ids = public_keys
            .into_iter()
            .filter(|record| {
                record.trusted
                    && provider_authority_device_ids.contains(record.public_key.device_id.as_str())
            })
            .map(|record| record.public_key.device_id)
            .collect::<BTreeSet<_>>();

        let app_domains = self.app_sync_domains(profile)?;
        let enabled_app_domain_count = app_domains.iter().filter(|domain| domain.enabled).count();
        let storage_providers = self.storage_providers(profile, u32::MAX)?;
        let enabled_storage_provider_count = storage_providers
            .iter()
            .filter(|provider| provider.enabled)
            .count();
        let retention_capable_provider_count = storage_providers
            .iter()
            .filter(|provider| {
                provider.enabled && provider.availability && provider.object_transfer
            })
            .count();
        let authorized_retention_provider_count = storage_providers
            .iter()
            .filter(|provider| {
                provider.enabled
                    && provider.availability
                    && provider.object_transfer
                    && trusted_provider_authority_device_ids.contains(provider.provider_id.as_str())
            })
            .count();

        let metadata_ready =
            active_key_id.is_some() && local_device_registered && enabled_app_domain_count > 0;
        let blocked_reason = if active_key_id.is_none() {
            Some("missing active content-key metadata".to_string())
        } else if !local_device_registered {
            Some("local device is not registered for profile sync".to_string())
        } else if !local_device_trusted {
            Some("local device sync key is not trusted".to_string())
        } else if enabled_app_domain_count == 0 {
            Some("no enabled app sync domains".to_string())
        } else if authorized_retention_provider_count == 0 {
            Some("no authorized retention provider configured".to_string())
        } else {
            None
        };
        let ready_for_manual_sync = metadata_ready && blocked_reason.is_none();

        Ok(ProfileSyncLocalReadinessReport {
            profile: profile.to_string(),
            local_device_id,
            local_device_registered,
            local_device_trusted,
            account_authority_trusted,
            trusted_device_count,
            metadata_ready,
            active_key_id,
            app_domain_count: app_domains.len(),
            enabled_app_domain_count,
            storage_provider_count: storage_providers.len(),
            enabled_storage_provider_count,
            retention_capable_provider_count,
            authorized_retention_provider_count,
            ready_for_manual_sync,
            blocked_reason,
        })
    }

    pub fn set_sync_setting_text(
        &self,
        profile: &str,
        domain: &str,
        key: &str,
        value: &str,
    ) -> Result<SyncChangeRecord, StorageError> {
        let mut connection = self.connection()?;
        let transaction = connection
            .transaction()
            .map_err(|source| self.database_error(source))?;
        let now = unix_time_seconds()?;
        if profile == DEFAULT_PROFILE_ID && domain == SYNC_DOMAIN_SETTINGS {
            transaction
                .execute(
                    "INSERT INTO settings (key, value, updated_at)
                     VALUES (?1, ?2, ?3)
                     ON CONFLICT(key) DO UPDATE SET
                       value = excluded.value,
                       updated_at = excluded.updated_at",
                    params![key, value, now],
                )
                .map_err(|source| self.database_error(source))?;
        }
        let change = record_sync_setting_text_in_transaction(
            &transaction,
            profile,
            domain,
            key,
            value,
            self.local_sync_device_id(),
            now,
        )
        .map_err(|source| self.database_error(source))?;
        transaction
            .commit()
            .map_err(|source| self.database_error(source))?;
        Ok(change)
    }

    pub fn apply_sync_setting_text(
        &self,
        change: &IncomingSyncSettingText,
    ) -> Result<SyncChangeRecord, StorageError> {
        let mut connection = self.connection()?;
        let transaction = connection
            .transaction()
            .map_err(|source| self.database_error(source))?;
        let now = unix_time_seconds()?;

        let applied = apply_sync_setting_text_in_transaction(&transaction, change, now)
            .map_err(|source| self.database_error(source))?;
        transaction
            .commit()
            .map_err(|source| self.database_error(source))?;
        Ok(applied)
    }

    pub fn get_sync_setting_text(
        &self,
        profile: &str,
        domain: &str,
        key: &str,
    ) -> Result<Option<SyncSettingValueRecord>, StorageError> {
        let connection = self.connection()?;
        connection
            .query_row(
                "SELECT profile, domain, key, value, value_kind, revision, updated_at
                 FROM settings_values
                 WHERE profile = ?1 AND domain = ?2 AND key = ?3",
                params![profile, domain, key],
                |row| {
                    Ok(SyncSettingValueRecord {
                        profile: row.get(0)?,
                        domain: row.get(1)?,
                        key: row.get(2)?,
                        value: row.get(3)?,
                        value_kind: row.get(4)?,
                        revision: row.get(5)?,
                        updated_at: row.get(6)?,
                    })
                },
            )
            .optional()
            .map_err(|source| self.database_error(source))
    }

    pub fn sync_changes_after(
        &self,
        profile: &str,
        after_change_id: i64,
        limit: u32,
    ) -> Result<Vec<SyncChangeRecord>, StorageError> {
        let connection = self.connection()?;
        let mut statement = connection
            .prepare(
                "SELECT id, profile, domain, entity_key, operation, payload, device_id,
                        device_sequence, logical_clock, created_at, applied_at
                 FROM settings_changes
                 WHERE profile = ?1 AND id > ?2
                 ORDER BY id
                 LIMIT ?3",
            )
            .map_err(|source| self.database_error(source))?;
        let records = statement
            .query_map(params![profile, after_change_id, i64::from(limit)], |row| {
                Ok(SyncChangeRecord {
                    id: row.get(0)?,
                    profile: row.get(1)?,
                    domain: row.get(2)?,
                    entity_key: row.get(3)?,
                    operation: row.get(4)?,
                    payload: row.get(5)?,
                    device_id: row.get(6)?,
                    device_sequence: row.get(7)?,
                    logical_clock: row.get(8)?,
                    created_at: row.get(9)?,
                    applied_at: row.get(10)?,
                })
            })
            .map_err(|source| self.database_error(source))?;

        let mut changes = Vec::new();
        for record in records {
            changes.push(record.map_err(|source| self.database_error(source))?);
        }
        Ok(changes)
    }

    pub fn latest_sync_device_sequence(
        &self,
        profile: &str,
        device_id: &str,
    ) -> Result<Option<i64>, StorageError> {
        let connection = self.connection()?;
        connection
            .query_row(
                "SELECT MAX(device_sequence)
                 FROM settings_changes
                 WHERE profile = ?1 AND device_id = ?2",
                params![profile, device_id],
                |row| row.get::<_, Option<i64>>(0),
            )
            .map_err(|source| self.database_error(source))
    }

    pub fn sync_revisions_after(
        &self,
        profile: &str,
        after_revision: i64,
    ) -> Result<Vec<SyncRevisionRecord>, StorageError> {
        let connection = self.connection()?;
        let mut statement = connection
            .prepare(
                "SELECT revision, profile, domain, change_id, created_at
                 FROM settings_revisions
                 WHERE profile = ?1 AND revision > ?2
                 ORDER BY revision",
            )
            .map_err(|source| self.database_error(source))?;
        let records = statement
            .query_map(params![profile, after_revision], |row| {
                Ok(SyncRevisionRecord {
                    revision: row.get(0)?,
                    profile: row.get(1)?,
                    domain: row.get(2)?,
                    change_id: row.get(3)?,
                    created_at: row.get(4)?,
                })
            })
            .map_err(|source| self.database_error(source))?;

        let mut revisions = Vec::new();
        for record in records {
            revisions.push(record.map_err(|source| self.database_error(source))?);
        }
        Ok(revisions)
    }

    pub fn latest_sync_revision(&self, profile: &str) -> Result<i64, StorageError> {
        let connection = self.connection()?;
        connection
            .query_row(
                "SELECT COALESCE(MAX(revision), 0)
                 FROM settings_revisions
                 WHERE profile = ?1",
                [profile],
                |row| row.get(0),
            )
            .map_err(|source| self.database_error(source))
    }

    pub fn latest_sync_revision_for_domain(
        &self,
        profile: &str,
        domain: &str,
    ) -> Result<i64, StorageError> {
        let connection = self.connection()?;
        connection
            .query_row(
                "SELECT COALESCE(MAX(revision), 0)
                 FROM settings_revisions
                 WHERE profile = ?1 AND domain = ?2",
                params![profile, domain],
                |row| row.get(0),
            )
            .map_err(|source| self.database_error(source))
    }

    pub fn sync_setting_text_events_after(
        &self,
        profile: &str,
        after_revision: i64,
        limit: u32,
    ) -> Result<Vec<SyncSettingTextEvent>, StorageError> {
        let connection = self.connection()?;
        let mut statement = connection
            .prepare(
                "SELECT revisions.revision, revisions.profile, revisions.domain,
                        revisions.change_id, revisions.created_at,
                        changes.id, changes.profile, changes.domain, changes.entity_key,
                        changes.operation, changes.payload, changes.device_id,
                        changes.device_sequence, changes.logical_clock, changes.created_at,
                        changes.applied_at
                 FROM settings_revisions revisions
                 JOIN settings_changes changes
                   ON changes.id = revisions.change_id
                  AND changes.profile = revisions.profile
                 WHERE revisions.profile = ?1
                   AND revisions.revision > ?2
                   AND changes.operation = 'set_text'
                   AND changes.applied_at IS NOT NULL
                 ORDER BY revisions.revision
                 LIMIT ?3",
            )
            .map_err(|source| self.database_error(source))?;
        let records = statement
            .query_map(params![profile, after_revision, i64::from(limit)], |row| {
                Ok(SyncSettingTextEvent {
                    revision: SyncRevisionRecord {
                        revision: row.get(0)?,
                        profile: row.get(1)?,
                        domain: row.get(2)?,
                        change_id: row.get(3)?,
                        created_at: row.get(4)?,
                    },
                    change: SyncChangeRecord {
                        id: row.get(5)?,
                        profile: row.get(6)?,
                        domain: row.get(7)?,
                        entity_key: row.get(8)?,
                        operation: row.get(9)?,
                        payload: row.get(10)?,
                        device_id: row.get(11)?,
                        device_sequence: row.get(12)?,
                        logical_clock: row.get(13)?,
                        created_at: row.get(14)?,
                        applied_at: row.get(15)?,
                    },
                })
            })
            .map_err(|source| self.database_error(source))?;

        let mut events = Vec::new();
        for record in records {
            events.push(record.map_err(|source| self.database_error(source))?);
        }
        Ok(events)
    }

    pub fn sync_setting_text_events_after_for_domain(
        &self,
        profile: &str,
        domain: &str,
        after_revision: i64,
        limit: u32,
    ) -> Result<Vec<SyncSettingTextEvent>, StorageError> {
        let connection = self.connection()?;
        let mut statement = connection
            .prepare(
                "SELECT revisions.revision, revisions.profile, revisions.domain,
                        revisions.change_id, revisions.created_at,
                        changes.id, changes.profile, changes.domain, changes.entity_key,
                        changes.operation, changes.payload, changes.device_id,
                        changes.device_sequence, changes.logical_clock, changes.created_at,
                        changes.applied_at
                 FROM settings_revisions revisions
                 JOIN settings_changes changes
                   ON changes.id = revisions.change_id
                  AND changes.profile = revisions.profile
                 WHERE revisions.profile = ?1
                   AND revisions.domain = ?2
                   AND changes.domain = ?2
                   AND revisions.revision > ?3
                   AND changes.operation = 'set_text'
                   AND changes.applied_at IS NOT NULL
                 ORDER BY revisions.revision
                 LIMIT ?4",
            )
            .map_err(|source| self.database_error(source))?;
        let records = statement
            .query_map(
                params![profile, domain, after_revision, i64::from(limit)],
                |row| {
                    Ok(SyncSettingTextEvent {
                        revision: SyncRevisionRecord {
                            revision: row.get(0)?,
                            profile: row.get(1)?,
                            domain: row.get(2)?,
                            change_id: row.get(3)?,
                            created_at: row.get(4)?,
                        },
                        change: SyncChangeRecord {
                            id: row.get(5)?,
                            profile: row.get(6)?,
                            domain: row.get(7)?,
                            entity_key: row.get(8)?,
                            operation: row.get(9)?,
                            payload: row.get(10)?,
                            device_id: row.get(11)?,
                            device_sequence: row.get(12)?,
                            logical_clock: row.get(13)?,
                            created_at: row.get(14)?,
                            applied_at: row.get(15)?,
                        },
                    })
                },
            )
            .map_err(|source| self.database_error(source))?;

        let mut events = Vec::new();
        for record in records {
            events.push(record.map_err(|source| self.database_error(source))?);
        }
        Ok(events)
    }

    pub fn poll_sync_setting_text_events_for_domain(
        &self,
        profile: &str,
        domain: &str,
        after_revision: i64,
        limit: u32,
    ) -> Result<SyncSettingTextDomainPoll, StorageError> {
        let events =
            self.sync_setting_text_events_after_for_domain(profile, domain, after_revision, limit)?;
        let latest_revision = events
            .last()
            .map(|event| event.revision.revision)
            .unwrap_or(after_revision);
        Ok(SyncSettingTextDomainPoll {
            profile: profile.to_string(),
            domain: domain.to_string(),
            previous_revision: after_revision,
            latest_revision,
            events,
        })
    }

    pub fn record_sync_snapshot(
        &self,
        snapshot: &SyncSnapshotRegistration,
    ) -> Result<SyncSnapshotRecord, StorageError> {
        let mut connection = self.connection()?;
        let transaction = connection
            .transaction()
            .map_err(|source| self.database_error(source))?;
        let now = unix_time_seconds()?;
        let record = self.record_sync_snapshot_in_transaction(&transaction, snapshot, now)?;
        transaction
            .commit()
            .map_err(|source| self.database_error(source))?;
        Ok(record)
    }

    fn record_sync_snapshot_in_transaction(
        &self,
        transaction: &rusqlite::Transaction<'_>,
        snapshot: &SyncSnapshotRegistration,
        now: i64,
    ) -> Result<SyncSnapshotRecord, StorageError> {
        let normalized_domains = normalized_snapshot_domains(snapshot.included_domains.as_slice());
        let included_domains = encode_snapshot_domains(normalized_domains.as_slice())?;
        transaction
            .execute(
                "INSERT INTO settings_snapshots
                   (profile, snapshot_id, backend_object_id, covers_revision, included_domains,
                    created_at)
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6)
                 ON CONFLICT(profile, snapshot_id) DO UPDATE SET
                   backend_object_id = excluded.backend_object_id,
                   covers_revision = excluded.covers_revision,
                   included_domains = excluded.included_domains",
                params![
                    snapshot.profile.as_str(),
                    snapshot.snapshot_id.as_str(),
                    snapshot.backend_object_id.as_deref(),
                    snapshot.covers_revision,
                    included_domains.as_str(),
                    now
                ],
            )
            .map_err(|source| self.database_error(source))?;

        transaction
            .query_row(
                "SELECT profile, snapshot_id, backend_object_id, covers_revision,
                        included_domains, created_at
                 FROM settings_snapshots
                 WHERE profile = ?1 AND snapshot_id = ?2",
                params![snapshot.profile.as_str(), snapshot.snapshot_id.as_str()],
                sync_snapshot_record_from_row,
            )
            .optional()
            .map_err(|source| self.database_error(source))?
            .ok_or_else(|| self.database_error(rusqlite::Error::QueryReturnedNoRows))
    }

    pub fn record_sync_snapshot_and_set_profile_sync_roots(
        &self,
        snapshot: &SyncSnapshotRegistration,
        roots: &[ProfileSyncRootRegistration],
    ) -> Result<(SyncSnapshotRecord, Vec<ProfileSyncRootRecord>), StorageError> {
        let mut connection = self.connection()?;
        let transaction = connection
            .transaction()
            .map_err(|source| self.database_error(source))?;
        let now = unix_time_seconds()?;
        let snapshot_record =
            self.record_sync_snapshot_in_transaction(&transaction, snapshot, now)?;
        let root_records = self.set_profile_sync_roots_in_transaction(&transaction, roots)?;
        transaction
            .commit()
            .map_err(|source| self.database_error(source))?;
        Ok((snapshot_record, root_records))
    }

    pub fn sync_snapshots_after(
        &self,
        profile: &str,
        after_revision: i64,
    ) -> Result<Vec<SyncSnapshotRecord>, StorageError> {
        let connection = self.connection()?;
        let mut statement = connection
            .prepare(
                "SELECT profile, snapshot_id, backend_object_id, covers_revision,
                        included_domains, created_at
                 FROM settings_snapshots
                 WHERE profile = ?1 AND covers_revision > ?2
                 ORDER BY covers_revision, created_at, snapshot_id",
            )
            .map_err(|source| self.database_error(source))?;
        let records = statement
            .query_map(
                params![profile, after_revision],
                sync_snapshot_record_from_row,
            )
            .map_err(|source| self.database_error(source))?;

        let mut snapshots = Vec::new();
        for record in records {
            snapshots.push(record.map_err(|source| self.database_error(source))?);
        }
        Ok(snapshots)
    }

    pub fn latest_sync_snapshot(
        &self,
        profile: &str,
    ) -> Result<Option<SyncSnapshotRecord>, StorageError> {
        let connection = self.connection()?;
        connection
            .query_row(
                "SELECT profile, snapshot_id, backend_object_id, covers_revision,
                        included_domains, created_at
                 FROM settings_snapshots
                 WHERE profile = ?1
                 ORDER BY covers_revision DESC, created_at DESC, snapshot_id DESC
                 LIMIT 1",
                params![profile],
                sync_snapshot_record_from_row,
            )
            .optional()
            .map_err(|source| self.database_error(source))
    }

    pub fn settings_sync_compaction_target(
        &self,
        profile: &str,
        retention_policy: &ProfileSyncRetentionPolicy,
        now: i64,
    ) -> Result<Option<SyncCompactionTarget>, StorageError> {
        let previous_snapshot_covers_revision = self
            .latest_sync_snapshot(profile)?
            .map(|snapshot| snapshot.covers_revision)
            .unwrap_or(0);
        let events = self.sync_setting_text_events_after(
            profile,
            previous_snapshot_covers_revision,
            u32::MAX,
        )?;
        let Some(target_index) =
            compaction_target_event_index(events.as_slice(), retention_policy, now)
        else {
            return Ok(None);
        };
        let target = &events[target_index];

        Ok(Some(SyncCompactionTarget {
            profile: profile.to_string(),
            previous_snapshot_covers_revision,
            covers_revision: target.revision.revision,
            covers_change_id: target.change.id,
            covered_change_count: target_index + 1,
            retained_tail_change_count: events.len() - target_index - 1,
        }))
    }

    pub fn settings_sync_compaction_target_for_domains(
        &self,
        profile: &str,
        retention_policy: &ProfileSyncRetentionPolicy,
        now: i64,
        included_domains: &[String],
    ) -> Result<Option<SyncCompactionTarget>, StorageError> {
        let included_domains = normalized_snapshot_domains(included_domains);
        if included_domains.is_empty() {
            return Ok(None);
        }
        let previous_snapshot_covers_revision = self
            .latest_sync_snapshot(profile)?
            .map(|snapshot| snapshot.covers_revision)
            .unwrap_or(0);
        let events = self
            .sync_setting_text_events_after(profile, previous_snapshot_covers_revision, u32::MAX)?
            .into_iter()
            .filter(|event| included_domains.contains(&event.change.domain))
            .collect::<Vec<_>>();
        let Some(target_index) =
            compaction_target_event_index(events.as_slice(), retention_policy, now)
        else {
            return Ok(None);
        };
        let target = &events[target_index];

        Ok(Some(SyncCompactionTarget {
            profile: profile.to_string(),
            previous_snapshot_covers_revision,
            covers_revision: target.revision.revision,
            covers_change_id: target.change.id,
            covered_change_count: target_index + 1,
            retained_tail_change_count: events.len() - target_index - 1,
        }))
    }

    pub fn settings_sync_snapshot_payload(
        &self,
        profile: &str,
        covers_revision: i64,
        included_domains: &[String],
    ) -> Result<ProfileSyncSettingsSnapshot, StorageError> {
        let included_domains = normalized_snapshot_domains(included_domains);
        let mut values_by_key: BTreeMap<(String, String), ProfileSyncSettingsSnapshotValue> =
            BTreeMap::new();
        let events = self.sync_setting_text_events_after(profile, 0, u32::MAX)?;
        for event in events {
            if event.revision.revision > covers_revision {
                break;
            }
            if !included_domains.contains(&event.change.domain) {
                continue;
            }
            values_by_key.insert(
                (event.change.domain.clone(), event.change.entity_key.clone()),
                ProfileSyncSettingsSnapshotValue {
                    domain: event.change.domain,
                    key: event.change.entity_key,
                    value: event.change.payload,
                    value_kind: "text".to_string(),
                    revision: event.revision.revision,
                },
            );
        }

        Ok(ProfileSyncSettingsSnapshot {
            profile: profile.to_string(),
            schema_version: PROFILE_SYNC_SETTINGS_SNAPSHOT_SCHEMA_VERSION,
            covers_revision,
            included_domains,
            values: values_by_key.into_values().collect(),
            created_at: unix_time_seconds()?,
        })
    }

    pub fn apply_settings_snapshot(
        &self,
        snapshot: &ProfileSyncSettingsSnapshot,
    ) -> Result<Vec<SyncChangeRecord>, StorageError> {
        let mut connection = self.connection()?;
        let transaction = connection
            .transaction()
            .map_err(|source| self.database_error(source))?;
        let now = unix_time_seconds()?;
        let changes = self.apply_settings_snapshot_in_transaction(&transaction, snapshot, now)?;
        transaction
            .commit()
            .map_err(|source| self.database_error(source))?;
        Ok(changes)
    }

    fn apply_settings_snapshot_in_transaction(
        &self,
        transaction: &rusqlite::Transaction<'_>,
        snapshot: &ProfileSyncSettingsSnapshot,
        now: i64,
    ) -> Result<Vec<SyncChangeRecord>, StorageError> {
        if snapshot.schema_version != PROFILE_SYNC_SETTINGS_SNAPSHOT_SCHEMA_VERSION {
            return Err(StorageError::UnsupportedSyncSnapshotSchema(
                snapshot.schema_version,
            ));
        }

        let included_domains = normalized_snapshot_domains(snapshot.included_domains.as_slice());
        let mut values = snapshot.values.clone();
        values.sort_by(|left, right| {
            (
                left.revision,
                left.domain.as_str(),
                left.key.as_str(),
                left.value.as_str(),
            )
                .cmp(&(
                    right.revision,
                    right.domain.as_str(),
                    right.key.as_str(),
                    right.value.as_str(),
                ))
        });

        let mut changes = Vec::new();
        for value in values {
            if value.value_kind != "text" || !included_domains.contains(&value.domain) {
                continue;
            }
            let change = IncomingSyncSettingText::new(
                snapshot.profile.clone(),
                value.domain,
                value.key,
                value.value,
                PROFILE_SYNC_SNAPSHOT_DEVICE_ID,
                value.revision,
                value.revision,
            );
            let applied = apply_sync_setting_text_in_transaction(transaction, &change, now)
                .map_err(|source| self.database_error(source))?;
            changes.push(applied);
        }
        Ok(changes)
    }

    pub fn apply_verified_settings_manifest(
        &self,
        manifest_object_id: &str,
        manifest: &ProfileSyncManifest,
        snapshot: Option<&VerifiedProfileSyncSettingsSnapshot>,
        tail_changes: &[VerifiedProfileSyncSettingsTailChange],
    ) -> Result<ProfileSyncSettingsManifestApplication, StorageError> {
        let mut connection = self.connection()?;
        let transaction = connection
            .transaction()
            .map_err(|source| self.database_error(source))?;
        let now = unix_time_seconds()?;
        let application = self.apply_verified_settings_manifest_in_transaction(
            &transaction,
            manifest_object_id,
            manifest,
            snapshot,
            tail_changes,
            now,
        )?;
        transaction
            .commit()
            .map_err(|source| self.database_error(source))?;
        Ok(application)
    }

    fn apply_verified_settings_manifest_in_transaction(
        &self,
        transaction: &rusqlite::Transaction<'_>,
        manifest_object_id: &str,
        manifest: &ProfileSyncManifest,
        snapshot: Option<&VerifiedProfileSyncSettingsSnapshot>,
        tail_changes: &[VerifiedProfileSyncSettingsTailChange],
        now: i64,
    ) -> Result<ProfileSyncSettingsManifestApplication, StorageError> {
        validate_settings_manifest_application(manifest, snapshot, tail_changes)?;

        let mut sync_object_ids =
            Vec::with_capacity(1 + snapshot.iter().count() + tail_changes.len());
        sync_object_ids.push(manifest_object_id.to_string());
        if let Some(snapshot) = snapshot {
            sync_object_ids.push(snapshot.object_id.clone());
        }
        sync_object_ids.extend(
            tail_changes
                .iter()
                .map(|tail_change| tail_change.object_id.clone()),
        );

        let mut snapshot_record = None;
        let mut snapshot_changes = Vec::new();
        if let Some(snapshot) = snapshot {
            snapshot_changes =
                self.apply_settings_snapshot_in_transaction(transaction, &snapshot.snapshot, now)?;
            snapshot_record = Some(self.record_sync_snapshot_in_transaction(
                transaction,
                &SyncSnapshotRegistration {
                    profile: manifest.profile.clone(),
                    snapshot_id: settings_sync_snapshot_id(snapshot.snapshot.covers_revision),
                    backend_object_id: Some(snapshot.object_id.clone()),
                    covers_revision: snapshot.snapshot.covers_revision,
                    included_domains: snapshot.snapshot.included_domains.clone(),
                },
                now,
            )?);
        }

        let mut applied_tail_changes = Vec::new();
        for tail_change in tail_changes {
            applied_tail_changes.push(
                apply_sync_setting_text_in_transaction(transaction, &tail_change.change, now)
                    .map_err(|source| self.database_error(source))?,
            );
        }

        self.set_profile_sync_root_in_transaction(
            transaction,
            manifest.profile.as_str(),
            manifest.root_id.as_str(),
            manifest_object_id,
        )?;

        Ok(ProfileSyncSettingsManifestApplication {
            profile: manifest.profile.clone(),
            root_id: manifest.root_id.clone(),
            manifest_object_id: manifest_object_id.to_string(),
            sync_object_ids,
            snapshot: snapshot_record,
            snapshot_changes,
            tail_changes: applied_tail_changes,
        })
    }

    pub fn apply_verified_settings_manifest_objects(
        &self,
        objects: &VerifiedProfileSyncSettingsManifestObjects,
    ) -> Result<ProfileSyncSettingsManifestApplication, StorageError> {
        self.apply_verified_settings_manifest(
            objects.manifest_object_id.as_str(),
            &objects.manifest,
            objects.snapshot.as_ref(),
            objects.tail_changes.as_slice(),
        )
    }

    pub fn apply_verified_settings_manifest_objects_and_set_profile_sync_root(
        &self,
        objects: &VerifiedProfileSyncSettingsManifestObjects,
        profile: &str,
        root_id: &str,
        object_id: &str,
    ) -> Result<
        (
            ProfileSyncRootRecord,
            ProfileSyncSettingsManifestApplication,
        ),
        StorageError,
    > {
        let mut connection = self.connection()?;
        let transaction = connection
            .transaction()
            .map_err(|source| self.database_error(source))?;
        let now = unix_time_seconds()?;
        let application = self.apply_verified_settings_manifest_in_transaction(
            &transaction,
            objects.manifest_object_id.as_str(),
            &objects.manifest,
            objects.snapshot.as_ref(),
            objects.tail_changes.as_slice(),
            now,
        )?;
        let root =
            self.set_profile_sync_root_in_transaction(&transaction, profile, root_id, object_id)?;
        transaction
            .commit()
            .map_err(|source| self.database_error(source))?;
        Ok((root, application))
    }

    pub fn apply_verified_settings_manifest_candidates(
        &self,
        candidates: &[VerifiedProfileSyncSettingsManifestCandidate],
    ) -> Result<Vec<ProfileSyncSettingsManifestCandidateApplication>, StorageError> {
        let mut ordered_candidates = candidates.iter().collect::<Vec<_>>();
        ordered_candidates.sort_by(|left, right| {
            (
                left.root_candidate.publish_sequence,
                left.root_candidate.publisher_id.as_str(),
                left.root_candidate.object_id.as_str(),
            )
                .cmp(&(
                    right.root_candidate.publish_sequence,
                    right.root_candidate.publisher_id.as_str(),
                    right.root_candidate.object_id.as_str(),
                ))
        });

        let mut applications = Vec::with_capacity(ordered_candidates.len());
        for candidate in ordered_candidates {
            applications.push(ProfileSyncSettingsManifestCandidateApplication {
                root_candidate: candidate.root_candidate.clone(),
                application: self.apply_verified_settings_manifest_objects(&candidate.objects)?,
            });
        }
        Ok(applications)
    }

    pub fn open_trusted_signed_encrypted_sync_payload(
        &self,
        bytes: &[u8],
        content_key: &ProfileSyncContentKey,
        expected_profile: &str,
        expected_domain: &str,
        expected_object_kind: &str,
        expected_key_id: &str,
    ) -> Result<Vec<u8>, ProfileSyncTrustedOpenError> {
        let signed_object =
            SignedSyncObject::from_bytes(bytes).map_err(ProfileSyncTrustedOpenError::SyncObject)?;
        let public_key = self
            .trusted_profile_state_public_key_for_signed_object(expected_profile, &signed_object)?;
        let encrypted_bytes = signed_object
            .verify_with(&public_key)
            .map_err(ProfileSyncTrustedOpenError::SyncObject)?;
        let encrypted_object = EncryptedSyncObject::from_bytes(encrypted_bytes)
            .map_err(ProfileSyncTrustedOpenError::SyncObject)?;
        encrypted_object
            .open_expected(
                content_key,
                expected_profile,
                expected_domain,
                expected_object_kind,
                expected_key_id,
            )
            .map_err(ProfileSyncTrustedOpenError::SyncObject)
    }

    pub fn open_trusted_signed_profile_sync_manifest(
        &self,
        bytes: &[u8],
        content_key: &ProfileSyncContentKey,
        profile: &str,
        key_id: &str,
    ) -> Result<ProfileSyncManifest, ProfileSyncTrustedOpenError> {
        let payload = self.open_trusted_signed_encrypted_sync_payload(
            bytes,
            content_key,
            profile,
            SYNC_DOMAIN_SETTINGS,
            PROFILE_SYNC_MANIFEST_OBJECT_KIND,
            key_id,
        )?;
        serde_json::from_slice(payload.as_slice())
            .map_err(SyncObjectError::Decode)
            .map_err(ProfileSyncTrustedOpenError::SyncObject)
    }

    pub fn open_trusted_signed_profile_sync_device_head(
        &self,
        bytes: &[u8],
        content_key: &ProfileSyncContentKey,
        profile: &str,
        key_id: &str,
    ) -> Result<ProfileSyncDeviceHead, ProfileSyncTrustedOpenError> {
        let signed_object =
            SignedSyncObject::from_bytes(bytes).map_err(ProfileSyncTrustedOpenError::SyncObject)?;
        let public_key =
            self.trusted_profile_state_public_key_for_signed_object(profile, &signed_object)?;
        let device_head =
            open_signed_profile_sync_device_head(bytes, content_key, &public_key, profile, key_id)
                .map_err(ProfileSyncTrustedOpenError::SyncObject)?;
        self.validate_signed_object_membership_epoch(bytes, profile, device_head.membership_epoch)?;
        Ok(device_head)
    }

    pub fn open_trusted_signed_profile_sync_settings_snapshot(
        &self,
        bytes: &[u8],
        content_key: &ProfileSyncContentKey,
        profile: &str,
        key_id: &str,
    ) -> Result<ProfileSyncSettingsSnapshot, ProfileSyncTrustedOpenError> {
        let payload = self.open_trusted_signed_encrypted_sync_payload(
            bytes,
            content_key,
            profile,
            SYNC_DOMAIN_SETTINGS,
            PROFILE_SYNC_SETTINGS_SNAPSHOT_OBJECT_KIND,
            key_id,
        )?;
        serde_json::from_slice(payload.as_slice())
            .map_err(SyncObjectError::Decode)
            .map_err(ProfileSyncTrustedOpenError::SyncObject)
    }

    pub fn open_trusted_signed_sync_setting_text_for_profile(
        &self,
        bytes: &[u8],
        content_key: &ProfileSyncContentKey,
        profile: &str,
        key_id: &str,
    ) -> Result<IncomingSyncSettingText, ProfileSyncTrustedOpenError> {
        let signed_object =
            SignedSyncObject::from_bytes(bytes).map_err(ProfileSyncTrustedOpenError::SyncObject)?;
        let public_key =
            self.trusted_profile_state_public_key_for_signed_object(profile, &signed_object)?;
        let encrypted_bytes = signed_object
            .verify_with(&public_key)
            .map_err(ProfileSyncTrustedOpenError::SyncObject)?;
        let encrypted_object = EncryptedSyncObject::from_bytes(encrypted_bytes)
            .map_err(ProfileSyncTrustedOpenError::SyncObject)?;
        if encrypted_object.profile != profile {
            return Err(ProfileSyncTrustedOpenError::SyncObject(
                SyncObjectError::UnexpectedProfile {
                    expected: profile.to_string(),
                    actual: encrypted_object.profile.clone(),
                },
            ));
        }
        if encrypted_object.object_kind != PROFILE_SYNC_SETTING_CHANGE_OBJECT_KIND {
            return Err(ProfileSyncTrustedOpenError::SyncObject(
                SyncObjectError::UnexpectedObjectKind {
                    expected: PROFILE_SYNC_SETTING_CHANGE_OBJECT_KIND.to_string(),
                    actual: encrypted_object.object_kind.clone(),
                },
            ));
        }
        if encrypted_object.key_id != key_id {
            return Err(ProfileSyncTrustedOpenError::SyncObject(
                SyncObjectError::UnexpectedKeyId {
                    expected: key_id.to_string(),
                    actual: encrypted_object.key_id.clone(),
                },
            ));
        }

        let payload = encrypted_object
            .open(content_key)
            .map_err(ProfileSyncTrustedOpenError::SyncObject)?;
        let change: IncomingSyncSettingText = serde_json::from_slice(payload.as_slice())
            .map_err(SyncObjectError::Decode)
            .map_err(ProfileSyncTrustedOpenError::SyncObject)?;
        if change.profile != encrypted_object.profile {
            return Err(ProfileSyncTrustedOpenError::SyncObject(
                SyncObjectError::UnexpectedProfile {
                    expected: encrypted_object.profile,
                    actual: change.profile,
                },
            ));
        }
        if change.domain != encrypted_object.domain {
            return Err(ProfileSyncTrustedOpenError::SyncObject(
                SyncObjectError::UnexpectedDomain {
                    expected: encrypted_object.domain,
                    actual: change.domain,
                },
            ));
        }
        Ok(change)
    }

    pub fn open_trusted_signed_profile_sync_settings_manifest_objects(
        &self,
        manifest_object: &ProfileSyncObjectBytes,
        snapshot_object: Option<&ProfileSyncObjectBytes>,
        tail_change_objects: &[ProfileSyncObjectBytes],
        content_key: &ProfileSyncContentKey,
        profile: &str,
        key_id: &str,
    ) -> Result<VerifiedProfileSyncSettingsManifestObjects, ProfileSyncTrustedOpenError> {
        let manifest = self.open_trusted_signed_profile_sync_manifest(
            manifest_object.bytes.as_slice(),
            content_key,
            profile,
            key_id,
        )?;
        self.validate_signed_object_membership_epoch(
            manifest_object.bytes.as_slice(),
            profile,
            manifest.membership_epoch,
        )?;
        let snapshot = snapshot_object
            .map(|snapshot_object| {
                self.validate_signed_object_membership_epoch(
                    snapshot_object.bytes.as_slice(),
                    profile,
                    manifest.membership_epoch,
                )?;
                Ok(VerifiedProfileSyncSettingsSnapshot {
                    object_id: snapshot_object.object_id.clone(),
                    snapshot: self.open_trusted_signed_profile_sync_settings_snapshot(
                        snapshot_object.bytes.as_slice(),
                        content_key,
                        profile,
                        key_id,
                    )?,
                })
            })
            .transpose()?;
        let mut tail_changes = Vec::with_capacity(tail_change_objects.len());
        for tail_object in tail_change_objects {
            self.validate_signed_object_membership_epoch(
                tail_object.bytes.as_slice(),
                profile,
                manifest.membership_epoch,
            )?;
            tail_changes.push(VerifiedProfileSyncSettingsTailChange {
                object_id: tail_object.object_id.clone(),
                change: self.open_trusted_signed_sync_setting_text_for_profile(
                    tail_object.bytes.as_slice(),
                    content_key,
                    profile,
                    key_id,
                )?,
            });
        }

        Ok(VerifiedProfileSyncSettingsManifestObjects {
            manifest_object_id: manifest_object.object_id.clone(),
            manifest,
            snapshot,
            tail_changes,
        })
    }

    pub fn pull_trusted_signed_profile_sync_device_head<Source>(
        &self,
        source: &Source,
        profile: &str,
        root_id: &str,
        content_key: &ProfileSyncContentKey,
        key_id: &str,
    ) -> Result<Option<VerifiedProfileSyncDeviceHead>, ProfileSyncTrustedPullError<Source::Error>>
    where
        Source: ProfileSyncObjectSource,
    {
        let Some(device_head_object_id) = source
            .resolve_profile_sync_root(profile, root_id)
            .map_err(ProfileSyncTrustedPullError::Source)?
        else {
            return Ok(None);
        };

        let device_head_object =
            fetch_trusted_profile_sync_object(source, profile, device_head_object_id.as_str())?;
        let device_head = self
            .open_trusted_signed_profile_sync_device_head(
                device_head_object.bytes.as_slice(),
                content_key,
                profile,
                key_id,
            )
            .map_err(ProfileSyncTrustedPullError::Open)?;
        validate_profile_sync_device_head_root(&device_head, root_id)
            .map_err(ProfileSyncTrustedOpenError::SyncObject)
            .map_err(ProfileSyncTrustedPullError::Open)?;
        Ok(Some(VerifiedProfileSyncDeviceHead {
            object_id: device_head_object.object_id,
            device_head,
        }))
    }

    pub fn pull_and_record_trusted_signed_profile_sync_device_head_if_changed<Source>(
        &self,
        source: &Source,
        profile: &str,
        root_id: &str,
        content_key: &ProfileSyncContentKey,
        key_id: &str,
    ) -> Result<
        ProfileSyncDeviceHeadPullRecordStatus,
        ProfileSyncTrustedPullApplyError<Source::Error>,
    >
    where
        Source: ProfileSyncObjectSource,
    {
        let Some(device_head_object_id) = source
            .resolve_profile_sync_root(profile, root_id)
            .map_err(|source| {
                ProfileSyncTrustedPullApplyError::Pull(ProfileSyncTrustedPullError::Source(source))
            })?
        else {
            return Ok(ProfileSyncDeviceHeadPullRecordStatus::NoPublishedRoot {
                profile: profile.to_string(),
                root_id: root_id.to_string(),
            });
        };

        if let Some(local_root) = self
            .profile_sync_root(profile, root_id)
            .map_err(ProfileSyncTrustedPullApplyError::Storage)?
        {
            if local_root.object_id == device_head_object_id {
                return Ok(ProfileSyncDeviceHeadPullRecordStatus::Unchanged {
                    profile: profile.to_string(),
                    root_id: root_id.to_string(),
                    object_id: device_head_object_id,
                });
            }
        }

        let device_head_object =
            fetch_trusted_profile_sync_object(source, profile, device_head_object_id.as_str())
                .map_err(ProfileSyncTrustedPullApplyError::Pull)?;
        let device_head = self
            .open_trusted_signed_profile_sync_device_head(
                device_head_object.bytes.as_slice(),
                content_key,
                profile,
                key_id,
            )
            .map_err(ProfileSyncTrustedPullError::Open)
            .map_err(ProfileSyncTrustedPullApplyError::Pull)?;
        validate_profile_sync_device_head_root(&device_head, root_id)
            .map_err(ProfileSyncTrustedOpenError::SyncObject)
            .map_err(ProfileSyncTrustedPullError::Open)
            .map_err(ProfileSyncTrustedPullApplyError::Pull)?;
        let root = self
            .set_profile_sync_root(profile, root_id, device_head_object.object_id.as_str())
            .map_err(ProfileSyncTrustedPullApplyError::Storage)?;
        Ok(ProfileSyncDeviceHeadPullRecordStatus::Updated {
            device_head: VerifiedProfileSyncDeviceHead {
                object_id: device_head_object.object_id,
                device_head,
            },
            root,
        })
    }

    pub fn pull_trusted_signed_profile_sync_settings_manifest_objects_from_device_head<Source>(
        &self,
        source: &Source,
        profile: &str,
        device_head: &VerifiedProfileSyncDeviceHead,
        content_key: &ProfileSyncContentKey,
        key_id: &str,
    ) -> Result<
        VerifiedProfileSyncSettingsManifestObjects,
        ProfileSyncTrustedPullError<Source::Error>,
    >
    where
        Source: ProfileSyncObjectSource,
    {
        if device_head.device_head.profile != profile {
            return Err(ProfileSyncTrustedPullError::Open(
                ProfileSyncTrustedOpenError::SyncObject(SyncObjectError::UnexpectedProfile {
                    expected: profile.to_string(),
                    actual: device_head.device_head.profile.clone(),
                }),
            ));
        }

        let manifest_object = fetch_trusted_profile_sync_object(
            source,
            profile,
            device_head.device_head.latest_manifest_object_id.as_str(),
        )?;
        let manifest = self
            .open_trusted_signed_profile_sync_manifest(
                manifest_object.bytes.as_slice(),
                content_key,
                profile,
                key_id,
            )
            .map_err(ProfileSyncTrustedPullError::Open)?;
        let snapshot_object = manifest
            .current_snapshot_object_id
            .as_deref()
            .map(|object_id| fetch_trusted_profile_sync_object(source, profile, object_id))
            .transpose()?;
        let mut tail_change_objects = Vec::with_capacity(manifest.tail_change_object_ids.len());
        for object_id in &manifest.tail_change_object_ids {
            tail_change_objects.push(fetch_trusted_profile_sync_object(
                source, profile, object_id,
            )?);
        }

        let objects = self
            .open_trusted_signed_profile_sync_settings_manifest_objects(
                &manifest_object,
                snapshot_object.as_ref(),
                tail_change_objects.as_slice(),
                content_key,
                profile,
                key_id,
            )
            .map_err(ProfileSyncTrustedPullError::Open)?;
        validate_profile_sync_device_head_manifest(&device_head.device_head, &objects.manifest)
            .map_err(ProfileSyncTrustedOpenError::SyncObject)
            .map_err(ProfileSyncTrustedPullError::Open)?;
        Ok(objects)
    }

    pub fn pull_and_apply_trusted_signed_settings_manifest_objects_from_device_head<Source>(
        &self,
        source: &Source,
        profile: &str,
        device_head: &VerifiedProfileSyncDeviceHead,
        content_key: &ProfileSyncContentKey,
        key_id: &str,
    ) -> Result<
        ProfileSyncSettingsManifestApplication,
        ProfileSyncTrustedPullApplyError<Source::Error>,
    >
    where
        Source: ProfileSyncObjectSource,
    {
        let objects = self
            .pull_trusted_signed_profile_sync_settings_manifest_objects_from_device_head(
                source,
                profile,
                device_head,
                content_key,
                key_id,
            )
            .map_err(ProfileSyncTrustedPullApplyError::Pull)?;
        self.apply_verified_settings_manifest_objects(&objects)
            .map_err(ProfileSyncTrustedPullApplyError::Storage)
    }

    pub fn pull_trusted_signed_profile_sync_settings_manifest_candidates<Source>(
        &self,
        source: &Source,
        profile: &str,
        root_id: &str,
        content_key: &ProfileSyncContentKey,
        key_id: &str,
    ) -> Result<
        Vec<VerifiedProfileSyncSettingsManifestCandidate>,
        ProfileSyncTrustedPullError<Source::Error>,
    >
    where
        Source: ProfileSyncObjectSource,
    {
        let candidates = source
            .list_profile_sync_root_candidates(profile, root_id)
            .map_err(ProfileSyncTrustedPullError::Source)?;
        let mut verified_candidates = Vec::with_capacity(candidates.len());
        for candidate in candidates {
            let objects = self.pull_trusted_signed_profile_sync_settings_manifest_objects_by_id(
                source,
                profile,
                candidate.object_id.as_str(),
                content_key,
                key_id,
            )?;
            verified_candidates.push(VerifiedProfileSyncSettingsManifestCandidate {
                root_candidate: candidate,
                objects,
            });
        }
        Ok(verified_candidates)
    }

    pub fn pull_and_apply_trusted_signed_profile_sync_settings_manifest_candidates<Source>(
        &self,
        source: &Source,
        profile: &str,
        root_id: &str,
        content_key: &ProfileSyncContentKey,
        key_id: &str,
    ) -> Result<
        Vec<ProfileSyncSettingsManifestCandidateApplication>,
        ProfileSyncTrustedPullApplyError<Source::Error>,
    >
    where
        Source: ProfileSyncObjectSource,
    {
        let candidates = self
            .pull_trusted_signed_profile_sync_settings_manifest_candidates(
                source,
                profile,
                root_id,
                content_key,
                key_id,
            )
            .map_err(ProfileSyncTrustedPullApplyError::Pull)?;
        self.apply_verified_settings_manifest_candidates(candidates.as_slice())
            .map_err(ProfileSyncTrustedPullApplyError::Storage)
    }

    pub fn pull_and_apply_active_trusted_signed_settings_manifest_candidates_if_changed<Source>(
        &self,
        source: &Source,
        profile: &str,
        root_id: &str,
        content_key: &ProfileSyncContentKey,
    ) -> Result<
        ProfileSyncSettingsCandidatePullApplyStatus,
        ProfileSyncTrustedPullApplyError<Source::Error>,
    >
    where
        Source: ProfileSyncObjectSource,
    {
        let key = self
            .active_sync_content_key_epoch(profile)
            .map_err(ProfileSyncTrustedPullApplyError::Storage)?
            .ok_or_else(|| StorageError::MissingActiveSyncContentKey(profile.to_string()))
            .map_err(ProfileSyncTrustedPullApplyError::Storage)?;
        validate_active_sync_content_key_epoch(&key)
            .map_err(ProfileSyncTrustedPullApplyError::Storage)?;

        let candidates = source
            .list_profile_sync_root_candidates(profile, root_id)
            .map_err(|source| {
                ProfileSyncTrustedPullApplyError::Pull(ProfileSyncTrustedPullError::Source(source))
            })?;
        let Some(newest_candidate) = newest_profile_sync_root_candidate(candidates.as_slice())
        else {
            return Ok(
                ProfileSyncSettingsCandidatePullApplyStatus::NoPublishedRoot {
                    profile: profile.to_string(),
                    root_id: root_id.to_string(),
                },
            );
        };

        if let Some(local_root) = self
            .profile_sync_root(profile, root_id)
            .map_err(ProfileSyncTrustedPullApplyError::Storage)?
        {
            if local_root.object_id == newest_candidate.object_id {
                return Ok(ProfileSyncSettingsCandidatePullApplyStatus::Unchanged {
                    profile: profile.to_string(),
                    root_id: root_id.to_string(),
                    object_id: newest_candidate.object_id.clone(),
                });
            }
        }

        let mut verified_candidates = Vec::with_capacity(candidates.len());
        for candidate in candidates {
            let objects = self
                .pull_trusted_signed_profile_sync_settings_manifest_objects_by_id(
                    source,
                    profile,
                    candidate.object_id.as_str(),
                    content_key,
                    key.key_id.as_str(),
                )
                .map_err(ProfileSyncTrustedPullApplyError::Pull)?;
            validate_sync_content_key_epoch_for_manifest(&key, &objects.manifest)
                .map_err(ProfileSyncTrustedPullApplyError::Storage)?;
            verified_candidates.push(VerifiedProfileSyncSettingsManifestCandidate {
                root_candidate: candidate,
                objects,
            });
        }

        self.apply_verified_settings_manifest_candidates(verified_candidates.as_slice())
            .map(ProfileSyncSettingsCandidatePullApplyStatus::Applied)
            .map_err(ProfileSyncTrustedPullApplyError::Storage)
    }

    pub fn pull_trusted_signed_profile_sync_settings_manifest_objects<Source>(
        &self,
        source: &Source,
        profile: &str,
        root_id: &str,
        content_key: &ProfileSyncContentKey,
        key_id: &str,
    ) -> Result<
        Option<VerifiedProfileSyncSettingsManifestObjects>,
        ProfileSyncTrustedPullError<Source::Error>,
    >
    where
        Source: ProfileSyncObjectSource,
    {
        let Some(manifest_object_id) = source
            .resolve_profile_sync_root(profile, root_id)
            .map_err(ProfileSyncTrustedPullError::Source)?
        else {
            return Ok(None);
        };

        self.pull_trusted_signed_profile_sync_settings_manifest_objects_by_id(
            source,
            profile,
            manifest_object_id.as_str(),
            content_key,
            key_id,
        )
        .map(Some)
    }

    fn pull_trusted_signed_profile_sync_settings_manifest_objects_by_id<Source>(
        &self,
        source: &Source,
        profile: &str,
        manifest_object_id: &str,
        content_key: &ProfileSyncContentKey,
        key_id: &str,
    ) -> Result<
        VerifiedProfileSyncSettingsManifestObjects,
        ProfileSyncTrustedPullError<Source::Error>,
    >
    where
        Source: ProfileSyncObjectSource,
    {
        let manifest_object =
            fetch_trusted_profile_sync_object(source, profile, manifest_object_id)?;
        let manifest = self
            .open_trusted_signed_profile_sync_manifest(
                manifest_object.bytes.as_slice(),
                content_key,
                profile,
                key_id,
            )
            .map_err(ProfileSyncTrustedPullError::Open)?;
        let snapshot_object = manifest
            .current_snapshot_object_id
            .as_deref()
            .map(|object_id| fetch_trusted_profile_sync_object(source, profile, object_id))
            .transpose()?;
        let mut tail_change_objects = Vec::with_capacity(manifest.tail_change_object_ids.len());
        for object_id in &manifest.tail_change_object_ids {
            tail_change_objects.push(fetch_trusted_profile_sync_object(
                source, profile, object_id,
            )?);
        }

        self.open_trusted_signed_profile_sync_settings_manifest_objects(
            &manifest_object,
            snapshot_object.as_ref(),
            tail_change_objects.as_slice(),
            content_key,
            profile,
            key_id,
        )
        .map_err(ProfileSyncTrustedPullError::Open)
    }

    pub fn pull_and_apply_trusted_signed_settings_manifest_objects<Source>(
        &self,
        source: &Source,
        profile: &str,
        root_id: &str,
        content_key: &ProfileSyncContentKey,
        key_id: &str,
    ) -> Result<
        Option<ProfileSyncSettingsManifestApplication>,
        ProfileSyncTrustedPullApplyError<Source::Error>,
    >
    where
        Source: ProfileSyncObjectSource,
    {
        let Some(objects) = self
            .pull_trusted_signed_profile_sync_settings_manifest_objects(
                source,
                profile,
                root_id,
                content_key,
                key_id,
            )
            .map_err(ProfileSyncTrustedPullApplyError::Pull)?
        else {
            return Ok(None);
        };
        self.apply_verified_settings_manifest_objects(&objects)
            .map(Some)
            .map_err(ProfileSyncTrustedPullApplyError::Storage)
    }

    pub fn pull_and_apply_active_trusted_signed_settings_manifest_objects<Source>(
        &self,
        source: &Source,
        profile: &str,
        root_id: &str,
        content_key: &ProfileSyncContentKey,
    ) -> Result<
        Option<ProfileSyncSettingsManifestApplication>,
        ProfileSyncTrustedPullApplyError<Source::Error>,
    >
    where
        Source: ProfileSyncObjectSource,
    {
        let key = self
            .active_sync_content_key_epoch(profile)
            .map_err(ProfileSyncTrustedPullApplyError::Storage)?
            .ok_or_else(|| StorageError::MissingActiveSyncContentKey(profile.to_string()))
            .map_err(ProfileSyncTrustedPullApplyError::Storage)?;
        validate_active_sync_content_key_epoch(&key)
            .map_err(ProfileSyncTrustedPullApplyError::Storage)?;
        let Some(objects) = self
            .pull_trusted_signed_profile_sync_settings_manifest_objects(
                source,
                profile,
                root_id,
                content_key,
                key.key_id.as_str(),
            )
            .map_err(ProfileSyncTrustedPullApplyError::Pull)?
        else {
            return Ok(None);
        };
        validate_sync_content_key_epoch_for_manifest(&key, &objects.manifest)
            .map_err(ProfileSyncTrustedPullApplyError::Storage)?;
        self.apply_verified_settings_manifest_objects(&objects)
            .map(Some)
            .map_err(ProfileSyncTrustedPullApplyError::Storage)
    }

    pub fn pull_and_apply_active_trusted_signed_settings_manifest_objects_if_changed<Source>(
        &self,
        source: &Source,
        profile: &str,
        root_id: &str,
        content_key: &ProfileSyncContentKey,
    ) -> Result<ProfileSyncSettingsPullApplyStatus, ProfileSyncTrustedPullApplyError<Source::Error>>
    where
        Source: ProfileSyncObjectSource,
    {
        let Some(published_object_id) = source
            .resolve_profile_sync_root(profile, root_id)
            .map_err(|source| {
                ProfileSyncTrustedPullApplyError::Pull(ProfileSyncTrustedPullError::Source(source))
            })?
        else {
            return Ok(ProfileSyncSettingsPullApplyStatus::NoPublishedRoot {
                profile: profile.to_string(),
                root_id: root_id.to_string(),
            });
        };

        if let Some(local_root) = self
            .profile_sync_root(profile, root_id)
            .map_err(ProfileSyncTrustedPullApplyError::Storage)?
        {
            if local_root.object_id == published_object_id {
                return Ok(ProfileSyncSettingsPullApplyStatus::Unchanged {
                    profile: profile.to_string(),
                    root_id: root_id.to_string(),
                    object_id: published_object_id,
                });
            }
        }

        match self.pull_and_apply_active_trusted_signed_settings_manifest_objects(
            source,
            profile,
            root_id,
            content_key,
        )? {
            Some(application) => Ok(ProfileSyncSettingsPullApplyStatus::Applied(application)),
            None => Ok(ProfileSyncSettingsPullApplyStatus::NoPublishedRoot {
                profile: profile.to_string(),
                root_id: root_id.to_string(),
            }),
        }
    }

    pub fn pull_and_apply_signed_settings_manifest_objects<Source>(
        &self,
        source: &Source,
        profile: &str,
        root_id: &str,
        content_key: &ProfileSyncContentKey,
        public_key: &ProfileSyncDevicePublicKey,
        key_id: &str,
    ) -> Result<
        Option<ProfileSyncSettingsManifestApplication>,
        ProfileSyncPullApplyError<Source::Error>,
    >
    where
        Source: ProfileSyncObjectSource,
    {
        let Some(objects) = pull_signed_profile_sync_settings_manifest_objects(
            source,
            profile,
            root_id,
            content_key,
            public_key,
            key_id,
        )
        .map_err(ProfileSyncPullApplyError::Pull)?
        else {
            return Ok(None);
        };
        self.apply_verified_settings_manifest_objects(&objects)
            .map(Some)
            .map_err(ProfileSyncPullApplyError::Storage)
    }

    fn trusted_profile_state_public_key_for_signed_object(
        &self,
        profile: &str,
        signed_object: &SignedSyncObject,
    ) -> Result<ProfileSyncDevicePublicKey, ProfileSyncTrustedOpenError> {
        let record = self.trusted_public_key_record_for_signed_object(profile, signed_object)?;
        if self
            .sync_device_provider_authority(profile, signed_object.device_id.as_str())
            .map_err(ProfileSyncTrustedOpenError::Storage)?
        {
            return Err(ProfileSyncTrustedOpenError::ProviderAuthoritySigner {
                profile: profile.to_string(),
                device_id: signed_object.device_id.clone(),
            });
        }
        Ok(record.public_key)
    }

    fn trusted_public_key_record_for_signed_object(
        &self,
        profile: &str,
        signed_object: &SignedSyncObject,
    ) -> Result<SyncDevicePublicKeyRecord, ProfileSyncTrustedOpenError> {
        if !is_valid_sync_identifier(signed_object.device_id.as_str()) {
            return Err(ProfileSyncTrustedOpenError::SyncObject(
                SyncObjectError::InvalidDeviceId(signed_object.device_id.clone()),
            ));
        }

        let Some(record) = self
            .sync_device_public_key(profile, signed_object.device_id.as_str())
            .map_err(ProfileSyncTrustedOpenError::Storage)?
        else {
            return Err(ProfileSyncTrustedOpenError::UntrustedDevice {
                profile: profile.to_string(),
                device_id: signed_object.device_id.clone(),
            });
        };
        if !record.trusted {
            return Err(ProfileSyncTrustedOpenError::UntrustedDevice {
                profile: profile.to_string(),
                device_id: signed_object.device_id.clone(),
            });
        }
        Ok(record)
    }

    fn validate_signed_object_membership_epoch(
        &self,
        bytes: &[u8],
        profile: &str,
        manifest_membership_epoch: i64,
    ) -> Result<(), ProfileSyncTrustedOpenError> {
        let signed_object =
            SignedSyncObject::from_bytes(bytes).map_err(ProfileSyncTrustedOpenError::SyncObject)?;
        let record = self.trusted_public_key_record_for_signed_object(profile, &signed_object)?;
        if record.membership_epoch > manifest_membership_epoch {
            return Err(ProfileSyncTrustedOpenError::UnauthorizedDeviceEpoch {
                profile: profile.to_string(),
                device_id: signed_object.device_id,
                key_membership_epoch: record.membership_epoch,
                manifest_membership_epoch,
            });
        }
        Ok(())
    }

    pub fn sync_snapshot(
        &self,
        profile: &str,
        snapshot_id: &str,
    ) -> Result<Option<SyncSnapshotRecord>, StorageError> {
        let connection = self.connection()?;
        connection
            .query_row(
                "SELECT profile, snapshot_id, backend_object_id, covers_revision,
                        included_domains, created_at
                 FROM settings_snapshots
                 WHERE profile = ?1 AND snapshot_id = ?2",
                params![profile, snapshot_id],
                sync_snapshot_record_from_row,
            )
            .optional()
            .map_err(|source| self.database_error(source))
    }

    pub fn set_profile_sync_root(
        &self,
        profile: &str,
        root_id: &str,
        object_id: &str,
    ) -> Result<ProfileSyncRootRecord, StorageError> {
        let mut connection = self.connection()?;
        let transaction = connection
            .transaction()
            .map_err(|source| self.database_error(source))?;
        let root =
            self.set_profile_sync_root_in_transaction(&transaction, profile, root_id, object_id)?;
        transaction
            .commit()
            .map_err(|source| self.database_error(source))?;
        Ok(root)
    }

    pub fn set_profile_sync_roots(
        &self,
        roots: &[ProfileSyncRootRegistration],
    ) -> Result<Vec<ProfileSyncRootRecord>, StorageError> {
        let mut connection = self.connection()?;
        let transaction = connection
            .transaction()
            .map_err(|source| self.database_error(source))?;
        let root_records = self.set_profile_sync_roots_in_transaction(&transaction, roots)?;
        transaction
            .commit()
            .map_err(|source| self.database_error(source))?;
        Ok(root_records)
    }

    fn set_profile_sync_roots_in_transaction(
        &self,
        transaction: &rusqlite::Transaction<'_>,
        roots: &[ProfileSyncRootRegistration],
    ) -> Result<Vec<ProfileSyncRootRecord>, StorageError> {
        let mut records = Vec::with_capacity(roots.len());
        for root in roots {
            records.push(self.set_profile_sync_root_in_transaction(
                transaction,
                root.profile.as_str(),
                root.root_id.as_str(),
                root.object_id.as_str(),
            )?);
        }
        Ok(records)
    }

    fn set_profile_sync_root_in_transaction(
        &self,
        transaction: &rusqlite::Transaction<'_>,
        profile: &str,
        root_id: &str,
        object_id: &str,
    ) -> Result<ProfileSyncRootRecord, StorageError> {
        if root_id.is_empty() {
            return Err(StorageError::InvalidSyncRootId(root_id.to_string()));
        }

        let now = unix_time_seconds()?;
        let key = profile_sync_root_key(root_id);
        transaction
            .execute(
                "INSERT INTO sync_state (profile, key, value, updated_at)
                 VALUES (?1, ?2, ?3, ?4)
                 ON CONFLICT(profile, key) DO UPDATE SET
                   value = excluded.value,
                   updated_at = excluded.updated_at",
                params![profile, key.as_str(), object_id, now],
            )
            .map_err(|source| self.database_error(source))?;

        Ok(ProfileSyncRootRecord {
            profile: profile.to_string(),
            root_id: root_id.to_string(),
            object_id: object_id.to_string(),
            updated_at: now,
        })
    }

    pub fn profile_sync_root(
        &self,
        profile: &str,
        root_id: &str,
    ) -> Result<Option<ProfileSyncRootRecord>, StorageError> {
        if root_id.is_empty() {
            return Err(StorageError::InvalidSyncRootId(root_id.to_string()));
        }

        let connection = self.connection()?;
        let key = profile_sync_root_key(root_id);
        connection
            .query_row(
                "SELECT profile, key, value, updated_at
                 FROM sync_state
                 WHERE profile = ?1 AND key = ?2",
                params![profile, key.as_str()],
                profile_sync_root_record_from_row,
            )
            .optional()
            .map_err(|source| self.database_error(source))
    }

    pub fn profile_sync_roots(
        &self,
        profile: &str,
    ) -> Result<Vec<ProfileSyncRootRecord>, StorageError> {
        let connection = self.connection()?;
        let mut statement = connection
            .prepare(
                "SELECT profile, key, value, updated_at
                 FROM sync_state
                 WHERE profile = ?1 AND key LIKE ?2
                 ORDER BY key",
            )
            .map_err(|source| self.database_error(source))?;
        let records = statement
            .query_map(
                params![profile, format!("{PROFILE_SYNC_ROOT_KEY_PREFIX}%")],
                profile_sync_root_record_from_row,
            )
            .map_err(|source| self.database_error(source))?;

        let mut roots = Vec::new();
        for record in records {
            roots.push(record.map_err(|source| self.database_error(source))?);
        }
        Ok(roots)
    }

    fn initialize(&self) -> Result<(), StorageError> {
        let connection = self.connection()?;
        connection
            .execute_batch(
                "
                PRAGMA foreign_keys = ON;
                PRAGMA journal_mode = DELETE;

                CREATE TABLE IF NOT EXISTS schema_migrations (
                    version INTEGER PRIMARY KEY,
                    applied_at INTEGER NOT NULL
                );

                CREATE TABLE IF NOT EXISTS settings (
                    key TEXT PRIMARY KEY,
                    value TEXT NOT NULL,
                    updated_at INTEGER NOT NULL
                );

                CREATE TABLE IF NOT EXISTS bookmarks (
                    id INTEGER PRIMARY KEY AUTOINCREMENT,
                    profile TEXT NOT NULL,
                    url TEXT NOT NULL,
                    title TEXT,
                    folder TEXT,
                    position INTEGER NOT NULL DEFAULT 0,
                    favicon_key TEXT,
                    created_at INTEGER NOT NULL,
                    updated_at INTEGER NOT NULL,
                    UNIQUE(profile, url)
                );

                CREATE TABLE IF NOT EXISTS downloads (
                    profile TEXT NOT NULL,
                    download_id TEXT NOT NULL,
                    source_url TEXT NOT NULL,
                    final_url TEXT NOT NULL,
                    route TEXT,
                    transport_id TEXT,
                    filename TEXT NOT NULL,
                    content_type TEXT,
                    size_bytes INTEGER NOT NULL,
                    integrity TEXT,
                    created_at INTEGER NOT NULL,
                    updated_at INTEGER NOT NULL,
                    PRIMARY KEY(profile, download_id)
                );

                CREATE INDEX IF NOT EXISTS downloads_updated_at
                    ON downloads(profile, updated_at DESC);

                CREATE TABLE IF NOT EXISTS calendar_events (
                    profile TEXT NOT NULL,
                    event_id TEXT NOT NULL,
                    calendar_id TEXT,
                    title TEXT NOT NULL,
                    starts_at INTEGER NOT NULL,
                    ends_at INTEGER,
                    time_zone TEXT,
                    location TEXT,
                    notes TEXT,
                    recurrence_rule TEXT,
                    reminder_minutes INTEGER,
                    created_at INTEGER NOT NULL,
                    updated_at INTEGER NOT NULL,
                    PRIMARY KEY(profile, event_id)
                );

                CREATE INDEX IF NOT EXISTS calendar_events_starts_at
                    ON calendar_events(profile, starts_at, event_id);

                CREATE TABLE IF NOT EXISTS contact_cards (
                    profile TEXT NOT NULL,
                    contact_id TEXT NOT NULL,
                    display_name TEXT NOT NULL,
                    given_name TEXT,
                    family_name TEXT,
                    organization TEXT,
                    primary_email TEXT,
                    primary_phone TEXT,
                    notes TEXT,
                    avatar_key TEXT,
                    created_at INTEGER NOT NULL,
                    updated_at INTEGER NOT NULL,
                    PRIMARY KEY(profile, contact_id)
                );

                CREATE INDEX IF NOT EXISTS contact_cards_display_name
                    ON contact_cards(profile, display_name, contact_id);

                CREATE TABLE IF NOT EXISTS chat_conversations (
                    profile TEXT NOT NULL,
                    conversation_id TEXT NOT NULL,
                    provider_id TEXT,
                    external_thread_id TEXT,
                    display_name TEXT NOT NULL,
                    avatar_key TEXT,
                    last_message_at INTEGER,
                    unread_count INTEGER NOT NULL,
                    archived INTEGER NOT NULL DEFAULT 0,
                    muted INTEGER NOT NULL DEFAULT 0,
                    created_at INTEGER NOT NULL,
                    updated_at INTEGER NOT NULL,
                    PRIMARY KEY(profile, conversation_id)
                );

                CREATE INDEX IF NOT EXISTS chat_conversations_provider
                    ON chat_conversations(profile, provider_id, conversation_id);

                CREATE INDEX IF NOT EXISTS chat_conversations_activity
                    ON chat_conversations(profile, archived, last_message_at DESC, conversation_id);

                CREATE TABLE IF NOT EXISTS file_entries (
                    profile TEXT NOT NULL,
                    entry_id TEXT NOT NULL,
                    sync_set_id TEXT,
                    parent_id TEXT,
                    name TEXT NOT NULL,
                    entry_kind TEXT NOT NULL,
                    content_ref TEXT,
                    mime_type TEXT,
                    size_bytes INTEGER,
                    modified_at INTEGER,
                    integrity TEXT,
                    retention_policy TEXT,
                    created_at INTEGER NOT NULL,
                    updated_at INTEGER NOT NULL,
                    PRIMARY KEY(profile, entry_id)
                );

                CREATE INDEX IF NOT EXISTS file_entries_parent
                    ON file_entries(profile, parent_id, name, entry_id);

                CREATE INDEX IF NOT EXISTS file_entries_sync_set
                    ON file_entries(profile, sync_set_id, entry_id);

                CREATE TABLE IF NOT EXISTS storage_providers (
                    profile TEXT NOT NULL,
                    provider_id TEXT NOT NULL,
                    provider_kind TEXT NOT NULL,
                    display_name TEXT NOT NULL,
                    endpoint_ref TEXT,
                    discovery INTEGER NOT NULL DEFAULT 0,
                    connectivity INTEGER NOT NULL DEFAULT 0,
                    object_transfer INTEGER NOT NULL DEFAULT 0,
                    availability INTEGER NOT NULL DEFAULT 0,
                    mutable_roots INTEGER NOT NULL DEFAULT 0,
                    quota_bytes INTEGER,
                    max_retained_objects INTEGER,
                    pinning_policy TEXT,
                    enabled INTEGER NOT NULL DEFAULT 0,
                    created_at INTEGER NOT NULL,
                    updated_at INTEGER NOT NULL,
                    PRIMARY KEY(profile, provider_id)
                );

                CREATE INDEX IF NOT EXISTS storage_providers_kind
                    ON storage_providers(profile, provider_kind, provider_id);

                CREATE INDEX IF NOT EXISTS storage_providers_enabled
                    ON storage_providers(profile, enabled, provider_id);

                CREATE TABLE IF NOT EXISTS cookies (
                    profile TEXT NOT NULL,
                    domain TEXT NOT NULL,
                    path TEXT NOT NULL,
                    name TEXT NOT NULL,
                    value TEXT NOT NULL,
                    expires_at INTEGER,
                    is_secure INTEGER NOT NULL DEFAULT 0,
                    is_http_only INTEGER NOT NULL DEFAULT 0,
                    same_site TEXT,
                    created_at INTEGER NOT NULL,
                    updated_at INTEGER NOT NULL,
                    PRIMARY KEY(profile, domain, path, name)
                );

                CREATE TABLE IF NOT EXISTS browsing_history (
                    id INTEGER PRIMARY KEY AUTOINCREMENT,
                    profile TEXT NOT NULL,
                    url TEXT NOT NULL,
                    title TEXT,
                    first_visited_at INTEGER NOT NULL,
                    last_visited_at INTEGER NOT NULL,
                    visit_count INTEGER NOT NULL DEFAULT 1,
                    UNIQUE(profile, url)
                );

                CREATE INDEX IF NOT EXISTS browsing_history_last_visited_at
                    ON browsing_history(profile, last_visited_at DESC);

                CREATE TABLE IF NOT EXISTS binary_blobs (
                    profile TEXT NOT NULL,
                    key TEXT NOT NULL,
                    media_type TEXT,
                    data BLOB NOT NULL,
                    created_at INTEGER NOT NULL,
                    updated_at INTEGER NOT NULL,
                    PRIMARY KEY(profile, key)
                );

                CREATE TABLE IF NOT EXISTS app_sync_domains (
                    profile TEXT NOT NULL,
                    domain TEXT NOT NULL,
                    schema_version INTEGER NOT NULL,
                    enabled INTEGER NOT NULL DEFAULT 1,
                    privacy_classification TEXT NOT NULL,
                    sync_content INTEGER NOT NULL DEFAULT 0,
                    created_at INTEGER NOT NULL,
                    updated_at INTEGER NOT NULL,
                    PRIMARY KEY(profile, domain)
                );

                CREATE TABLE IF NOT EXISTS sync_devices (
                    profile TEXT NOT NULL,
                    device_id TEXT NOT NULL,
                    label TEXT,
                    membership_epoch INTEGER NOT NULL DEFAULT 1,
                    provider_authority INTEGER NOT NULL DEFAULT 0,
                    created_at INTEGER NOT NULL,
                    last_seen_at INTEGER NOT NULL,
                    PRIMARY KEY(profile, device_id)
                );

                CREATE TABLE IF NOT EXISTS sync_device_public_keys (
                    profile TEXT NOT NULL,
                    device_id TEXT NOT NULL,
                    public_key BLOB NOT NULL,
                    membership_epoch INTEGER NOT NULL DEFAULT 1,
                    trusted INTEGER NOT NULL DEFAULT 1,
                    created_at INTEGER NOT NULL,
                    updated_at INTEGER NOT NULL,
                    PRIMARY KEY(profile, device_id)
                );

                CREATE TABLE IF NOT EXISTS sync_account_membership_records (
                    profile TEXT NOT NULL,
                    record_id TEXT NOT NULL,
                    membership_epoch INTEGER NOT NULL,
                    record_kind TEXT NOT NULL,
                    device_id TEXT NOT NULL,
                    signer_device_id TEXT NOT NULL,
                    signed_record BLOB NOT NULL,
                    created_at INTEGER NOT NULL,
                    applied_at INTEGER,
                    PRIMARY KEY(profile, record_id)
                );

                CREATE INDEX IF NOT EXISTS sync_account_membership_records_epoch
                    ON sync_account_membership_records(profile, membership_epoch, record_id);

                CREATE INDEX IF NOT EXISTS sync_account_membership_records_device
                    ON sync_account_membership_records(profile, device_id, membership_epoch);

                CREATE TABLE IF NOT EXISTS sync_content_key_epochs (
                    profile TEXT NOT NULL,
                    key_id TEXT NOT NULL,
                    membership_epoch INTEGER NOT NULL DEFAULT 1,
                    algorithm TEXT NOT NULL,
                    active INTEGER NOT NULL DEFAULT 0,
                    created_at INTEGER NOT NULL,
                    updated_at INTEGER NOT NULL,
                    PRIMARY KEY(profile, key_id)
                );

                CREATE UNIQUE INDEX IF NOT EXISTS sync_content_key_epochs_one_active
                    ON sync_content_key_epochs(profile)
                    WHERE active = 1;

                CREATE TABLE IF NOT EXISTS settings_changes (
                    id INTEGER PRIMARY KEY AUTOINCREMENT,
                    profile TEXT NOT NULL,
                    domain TEXT NOT NULL,
                    entity_key TEXT NOT NULL,
                    operation TEXT NOT NULL,
                    payload TEXT NOT NULL,
                    device_id TEXT NOT NULL,
                    device_sequence INTEGER NOT NULL,
                    logical_clock INTEGER NOT NULL,
                    created_at INTEGER NOT NULL,
                    applied_at INTEGER,
                    UNIQUE(profile, device_id, device_sequence)
                );

                CREATE INDEX IF NOT EXISTS settings_changes_profile_id
                    ON settings_changes(profile, id);

                CREATE INDEX IF NOT EXISTS settings_changes_domain_clock
                    ON settings_changes(profile, domain, logical_clock);

                CREATE TABLE IF NOT EXISTS settings_revisions (
                    revision INTEGER PRIMARY KEY AUTOINCREMENT,
                    profile TEXT NOT NULL,
                    domain TEXT NOT NULL,
                    change_id INTEGER NOT NULL,
                    created_at INTEGER NOT NULL
                );

                CREATE INDEX IF NOT EXISTS settings_revisions_profile_revision
                    ON settings_revisions(profile, revision);

                CREATE TABLE IF NOT EXISTS settings_values (
                    profile TEXT NOT NULL,
                    domain TEXT NOT NULL,
                    key TEXT NOT NULL,
                    value TEXT NOT NULL,
                    value_kind TEXT NOT NULL DEFAULT 'text',
                    revision INTEGER NOT NULL,
                    updated_at INTEGER NOT NULL,
                    PRIMARY KEY(profile, domain, key)
                );

                CREATE TABLE IF NOT EXISTS settings_snapshots (
                    profile TEXT NOT NULL,
                    snapshot_id TEXT NOT NULL,
                    backend_object_id TEXT,
                    covers_revision INTEGER NOT NULL,
                    included_domains TEXT NOT NULL,
                    created_at INTEGER NOT NULL,
                    PRIMARY KEY(profile, snapshot_id)
                );

                CREATE INDEX IF NOT EXISTS settings_snapshots_profile_revision
                    ON settings_snapshots(profile, covers_revision);

                CREATE TABLE IF NOT EXISTS sync_state (
                    profile TEXT NOT NULL,
                    key TEXT NOT NULL,
                    value TEXT NOT NULL,
                    updated_at INTEGER NOT NULL,
                    PRIMARY KEY(profile, key)
                );

                INSERT OR IGNORE INTO schema_migrations(version, applied_at)
                    VALUES (1, CAST(strftime('%s', 'now') AS INTEGER));
                INSERT OR IGNORE INTO schema_migrations(version, applied_at)
                    VALUES (2, CAST(strftime('%s', 'now') AS INTEGER));
                ",
            )
            .map_err(|source| self.database_error(source))?;
        self.ensure_sync_device_public_keys_trusted_column(&connection)?;
        connection
            .execute(
                "INSERT OR IGNORE INTO schema_migrations(version, applied_at)
                 VALUES (4, CAST(strftime('%s', 'now') AS INTEGER))",
                [],
            )
            .map_err(|source| self.database_error(source))?;
        self.ensure_sync_account_membership_records_applied_at_column(&connection)?;
        Ok(())
    }

    fn ensure_sync_account_membership_records_applied_at_column(
        &self,
        connection: &Connection,
    ) -> Result<(), StorageError> {
        let has_applied_at =
            table_has_column(connection, "sync_account_membership_records", "applied_at")
                .map_err(|source| self.database_error(source))?;
        if !has_applied_at {
            connection
                .execute(
                    "ALTER TABLE sync_account_membership_records
                     ADD COLUMN applied_at INTEGER",
                    [],
                )
                .map_err(|source| self.database_error(source))?;
        }
        connection
            .execute(
                "INSERT OR IGNORE INTO schema_migrations(version, applied_at)
                 VALUES (5, CAST(strftime('%s', 'now') AS INTEGER))",
                [],
            )
            .map_err(|source| self.database_error(source))?;
        Ok(())
    }

    fn ensure_sync_device_public_keys_trusted_column(
        &self,
        connection: &Connection,
    ) -> Result<(), StorageError> {
        let has_trusted = table_has_column(connection, "sync_device_public_keys", "trusted")
            .map_err(|source| self.database_error(source))?;
        if !has_trusted {
            connection
                .execute(
                    "ALTER TABLE sync_device_public_keys
                     ADD COLUMN trusted INTEGER NOT NULL DEFAULT 1",
                    [],
                )
                .map_err(|source| self.database_error(source))?;
        }
        connection
            .execute(
                "INSERT OR IGNORE INTO schema_migrations(version, applied_at)
                 VALUES (3, CAST(strftime('%s', 'now') AS INTEGER))",
                [],
            )
            .map_err(|source| self.database_error(source))?;
        Ok(())
    }

    fn try_seed_default_sync_state(&self) {
        let _ = self.seed_default_sync_state();
    }

    fn seed_default_sync_state(&self) -> Result<(), StorageError> {
        self.register_sync_device(&SyncDeviceRegistration {
            profile: DEFAULT_PROFILE_ID.to_string(),
            device_id: self.local_sync_device_id().to_string(),
            label: Some("Local Device".to_string()),
            membership_epoch: 1,
            provider_authority: false,
        })?;

        self.ensure_default_app_sync_domains(DEFAULT_PROFILE_ID)?;
        Ok(())
    }

    fn seed_default_app_sync_domain(
        &self,
        profile: &str,
        domain: &DefaultAppSyncDomain,
    ) -> Result<(), StorageError> {
        let connection = self.connection()?;
        let now = unix_time_seconds()?;
        connection
            .execute(
                "INSERT INTO app_sync_domains
                   (profile, domain, schema_version, enabled, privacy_classification,
                    sync_content, created_at, updated_at)
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?7)
                 ON CONFLICT(profile, domain) DO UPDATE SET
                   schema_version = excluded.schema_version,
                   privacy_classification = excluded.privacy_classification,
                   sync_content = excluded.sync_content,
                   updated_at = excluded.updated_at",
                params![
                    profile,
                    domain.domain,
                    domain.schema_version,
                    bool_to_integer(domain.default_enabled),
                    domain.privacy_classification,
                    bool_to_integer(domain.sync_content),
                    now
                ],
            )
            .map_err(|source| self.database_error(source))?;
        Ok(())
    }

    fn try_seed_default_bookmarks(&self) {
        let _ = self.seed_default_bookmarks_if_needed();
    }

    fn seed_default_bookmarks_if_needed(&self) -> Result<(), StorageError> {
        if self.get_setting_text_or_default(DEFAULT_BOOKMARKS_SEEDED_SETTING_KEY, "false") == "true"
        {
            return Ok(());
        }

        if self.bookmarks(DEFAULT_PROFILE_ID)?.is_empty() {
            self.seed_default_bookmarks()?;
        }

        self.set_setting_text(DEFAULT_BOOKMARKS_SEEDED_SETTING_KEY, "true")
    }

    fn seed_default_bookmarks(&self) -> Result<(), StorageError> {
        for (position, bookmark) in DEFAULT_HOME_BOOKMARKS.iter().enumerate() {
            self.upsert_bookmark(&BookmarkUpdate {
                profile: DEFAULT_PROFILE_ID.to_string(),
                url: bookmark.url.to_string(),
                title: Some(bookmark.title.to_string()),
                folder: None,
                position: position as i64,
                favicon_key: None,
            })?;
        }
        Ok(())
    }

    fn connection(&self) -> Result<Connection, StorageError> {
        Connection::open(self.path()).map_err(|source| self.database_error(source))
    }

    fn database_error(&self, source: rusqlite::Error) -> StorageError {
        StorageError::Database {
            path: self.path().to_path_buf(),
            source,
        }
    }
}

pub fn resolve_database_path(
    explicit_path: Option<&Path>,
    launch_directory: &Path,
    home_directory: Option<&Path>,
) -> ResolvedDatabasePath {
    if let Some(path) = explicit_path {
        return ResolvedDatabasePath {
            path: path.to_path_buf(),
            source: DatabasePathSource::Explicit,
        };
    }

    let launch_database = launch_directory.join(DEFAULT_DATABASE_FILE_NAME);
    if launch_database.is_file() {
        return ResolvedDatabasePath {
            path: launch_database,
            source: DatabasePathSource::LaunchDirectoryExisting,
        };
    }

    if let Some(home_directory) = home_directory {
        let home_database = home_directory
            .join(DEFAULT_HOME_DIRECTORY_NAME)
            .join(DEFAULT_DATABASE_FILE_NAME);
        if home_database.is_file() {
            return ResolvedDatabasePath {
                path: home_database,
                source: DatabasePathSource::HomeDirectoryExisting,
            };
        }
    }

    ResolvedDatabasePath {
        path: launch_database,
        source: DatabasePathSource::LaunchDirectoryCreated,
    }
}

fn ensure_database_parent_directory(path: &Path) -> Result<(), StorageError> {
    if let Some(parent) = path.parent()
        && !parent.as_os_str().is_empty()
    {
        std::fs::create_dir_all(parent).map_err(|source| StorageError::CreateDirectory {
            path: parent.to_path_buf(),
            source,
        })?;
    }
    Ok(())
}

fn local_sync_device_id_path(database_path: &Path) -> PathBuf {
    database_path
        .parent()
        .filter(|parent| !parent.as_os_str().is_empty())
        .map(|parent| parent.join(DEFAULT_LOCAL_SYNC_DEVICE_ID_FILE_NAME))
        .unwrap_or_else(|| PathBuf::from(DEFAULT_LOCAL_SYNC_DEVICE_ID_FILE_NAME))
}

fn load_or_create_persistent_local_sync_device_id(
    database_path: &Path,
) -> Result<String, StorageError> {
    let device_id_path = local_sync_device_id_path(database_path);
    match std::fs::read_to_string(&device_id_path) {
        Ok(device_id) => {
            let device_id = device_id.trim().to_string();
            if is_valid_sync_identifier(device_id.as_str()) {
                Ok(device_id)
            } else {
                Err(StorageError::InvalidSyncDeviceId(device_id))
            }
        }
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
            let device_id = generate_local_sync_device_id()?;
            std::fs::write(&device_id_path, format!("{device_id}\n")).map_err(|source| {
                StorageError::WriteLocalSyncDeviceId {
                    path: device_id_path,
                    source,
                }
            })?;
            Ok(device_id)
        }
        Err(source) => Err(StorageError::ReadLocalSyncDeviceId {
            path: device_id_path,
            source,
        }),
    }
}

fn generate_local_sync_device_id() -> Result<String, StorageError> {
    let mut random_bytes = [0_u8; 16];
    rand::SystemRandom::new()
        .fill(&mut random_bytes)
        .map_err(|_| StorageError::GenerateLocalSyncDeviceId)?;
    Ok(format!("device-{}", URL_SAFE_NO_PAD.encode(random_bytes)))
}

fn unix_time_seconds() -> Result<i64, StorageError> {
    Ok(SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_err(StorageError::Clock)?
        .as_secs() as i64)
}

fn bool_to_integer(value: bool) -> i64 {
    i64::from(value)
}

fn integer_to_bool(value: i64) -> bool {
    value != 0
}

fn normalized_snapshot_domains(domains: &[String]) -> Vec<String> {
    domains
        .iter()
        .filter(|domain| !domain.is_empty())
        .cloned()
        .collect::<std::collections::BTreeSet<_>>()
        .into_iter()
        .collect()
}

pub fn settings_sync_snapshot_id(covers_revision: i64) -> String {
    format!("settings-snapshot-r{covers_revision}")
}

fn validate_active_sync_content_key_epoch(
    key: &SyncContentKeyEpochRecord,
) -> Result<(), StorageError> {
    if key.algorithm != PROFILE_SYNC_CONTENT_KEY_ALGORITHM_CHACHA20_POLY1305 {
        return Err(StorageError::UnsupportedSyncContentKeyAlgorithm {
            key_id: key.key_id.clone(),
            algorithm: key.algorithm.clone(),
        });
    }
    Ok(())
}

fn validate_sync_content_key_epoch_for_manifest(
    key: &SyncContentKeyEpochRecord,
    manifest: &ProfileSyncManifest,
) -> Result<(), StorageError> {
    if key.membership_epoch > manifest.membership_epoch {
        return Err(StorageError::UnauthorizedSyncContentKeyEpoch {
            profile: manifest.profile.clone(),
            key_id: key.key_id.clone(),
            key_membership_epoch: key.membership_epoch,
            manifest_membership_epoch: manifest.membership_epoch,
        });
    }
    Ok(())
}

fn newest_profile_sync_root_candidate(
    candidates: &[ProfileSyncRootCandidate],
) -> Option<&ProfileSyncRootCandidate> {
    candidates.iter().max_by(|left, right| {
        (
            left.publish_sequence,
            left.publisher_id.as_str(),
            left.object_id.as_str(),
        )
            .cmp(&(
                right.publish_sequence,
                right.publisher_id.as_str(),
                right.object_id.as_str(),
            ))
    })
}

fn validate_settings_manifest_application(
    manifest: &ProfileSyncManifest,
    snapshot: Option<&VerifiedProfileSyncSettingsSnapshot>,
    tail_changes: &[VerifiedProfileSyncSettingsTailChange],
) -> Result<(), StorageError> {
    if manifest.schema_version != PROFILE_SYNC_MANIFEST_SCHEMA_VERSION {
        return Err(StorageError::UnsupportedProfileSyncManifestSchema(
            manifest.schema_version,
        ));
    }
    if manifest.profile.is_empty() {
        return Err(StorageError::InvalidProfileSyncManifest(
            "manifest profile is empty".to_string(),
        ));
    }
    if manifest.root_id.is_empty() {
        return Err(StorageError::InvalidSyncRootId(manifest.root_id.clone()));
    }

    match (manifest.current_snapshot_object_id.as_deref(), snapshot) {
        (Some(expected_object_id), Some(snapshot))
            if expected_object_id == snapshot.object_id.as_str() =>
        {
            validate_manifest_snapshot(manifest, snapshot)?
        }
        (Some(expected_object_id), Some(snapshot)) => {
            return Err(StorageError::InvalidProfileSyncManifest(format!(
                "manifest current snapshot object {expected_object_id} does not match verified snapshot object {}",
                snapshot.object_id
            )));
        }
        (Some(expected_object_id), None) => {
            return Err(StorageError::InvalidProfileSyncManifest(format!(
                "manifest references missing snapshot object {expected_object_id}"
            )));
        }
        (None, Some(snapshot)) => {
            return Err(StorageError::InvalidProfileSyncManifest(format!(
                "verified snapshot object {} is not referenced by manifest",
                snapshot.object_id
            )));
        }
        (None, None) => {}
    }

    let actual_tail_object_ids = tail_changes
        .iter()
        .map(|tail| tail.object_id.clone())
        .collect::<Vec<_>>();
    if actual_tail_object_ids != manifest.tail_change_object_ids {
        return Err(StorageError::InvalidProfileSyncManifest(
            "manifest tail object ids do not match verified tail changes".to_string(),
        ));
    }
    for tail in tail_changes {
        if tail.change.profile != manifest.profile {
            return Err(StorageError::InvalidProfileSyncManifest(format!(
                "tail change {} profile {} does not match manifest profile {}",
                tail.object_id, tail.change.profile, manifest.profile
            )));
        }
        if !manifest.included_domains.contains(&tail.change.domain) {
            return Err(StorageError::InvalidProfileSyncManifest(format!(
                "tail change {} domain {} is not included in manifest",
                tail.object_id, tail.change.domain
            )));
        }
    }

    Ok(())
}

fn validate_manifest_snapshot(
    manifest: &ProfileSyncManifest,
    snapshot: &VerifiedProfileSyncSettingsSnapshot,
) -> Result<(), StorageError> {
    if snapshot.snapshot.profile != manifest.profile {
        return Err(StorageError::InvalidProfileSyncManifest(format!(
            "snapshot object {} profile {} does not match manifest profile {}",
            snapshot.object_id, snapshot.snapshot.profile, manifest.profile
        )));
    }
    for domain in &snapshot.snapshot.included_domains {
        if !manifest.included_domains.contains(domain) {
            return Err(StorageError::InvalidProfileSyncManifest(format!(
                "snapshot object {} domain {domain} is not included in manifest",
                snapshot.object_id
            )));
        }
    }
    Ok(())
}

fn encode_snapshot_domains(domains: &[String]) -> Result<String, StorageError> {
    serde_json::to_string(domains).map_err(StorageError::EncodeSnapshotDomains)
}

fn decode_snapshot_domains(value: &str) -> Result<Vec<String>, serde_json::Error> {
    serde_json::from_str(value)
}

fn sync_snapshot_record_from_row(
    row: &rusqlite::Row<'_>,
) -> Result<SyncSnapshotRecord, rusqlite::Error> {
    let included_domains_json: String = row.get(4)?;
    let included_domains =
        decode_snapshot_domains(included_domains_json.as_str()).map_err(|source| {
            rusqlite::Error::FromSqlConversionFailure(4, Type::Text, Box::new(source))
        })?;
    Ok(SyncSnapshotRecord {
        profile: row.get(0)?,
        snapshot_id: row.get(1)?,
        backend_object_id: row.get(2)?,
        covers_revision: row.get(3)?,
        included_domains,
        created_at: row.get(5)?,
    })
}

fn compaction_target_event_index(
    events: &[SyncSettingTextEvent],
    retention_policy: &ProfileSyncRetentionPolicy,
    now: i64,
) -> Option<usize> {
    let min_tail_change_count =
        usize::try_from(retention_policy.min_tail_change_count).unwrap_or(usize::MAX);
    if events.len() <= min_tail_change_count {
        return None;
    }

    let retention_seconds = retention_policy.change_retention_seconds.max(0);
    let retention_cutoff = now.saturating_sub(retention_seconds);
    let newest_old_enough = events
        .iter()
        .rposition(|event| event.change.created_at <= retention_cutoff)?;
    let newest_allowed_by_tail = events.len() - min_tail_change_count - 1;

    Some(newest_old_enough.min(newest_allowed_by_tail))
}

fn profile_sync_root_key(root_id: &str) -> String {
    format!("{PROFILE_SYNC_ROOT_KEY_PREFIX}{root_id}")
}

fn bookmark_home_slot_sync_key(position: i64) -> String {
    format!("{BOOKMARK_HOME_SLOT_SYNC_KEY_PREFIX}{position}")
}

fn bookmark_home_slot_sync_payload(
    bookmark: &BookmarkUpdate,
    replaced_url: Option<&str>,
) -> Result<String, StorageError> {
    serde_json::to_string(&BookmarkSlotSyncPayload {
        url: bookmark.url.clone(),
        title: bookmark.title.clone(),
        folder: bookmark.folder.clone(),
        position: bookmark.position,
        favicon_key: bookmark.favicon_key.clone(),
        replaced_url: replaced_url.map(str::to_string),
        deleted: false,
    })
    .map_err(StorageError::EncodeSyncPayload)
}

fn download_metadata_sync_key(download_id: &str) -> String {
    format!("{DOWNLOAD_METADATA_SYNC_KEY_PREFIX}{download_id}")
}

fn download_metadata_sync_payload(
    download: &DownloadMetadataUpdate,
) -> Result<String, StorageError> {
    serde_json::to_string(&DownloadMetadataSyncPayload {
        download_id: download.download_id.clone(),
        source_url: download.source_url.clone(),
        final_url: download.final_url.clone(),
        route: download.route.clone(),
        transport_id: download.transport_id.clone(),
        filename: download.filename.clone(),
        content_type: download.content_type.clone(),
        size_bytes: download.size_bytes,
        integrity: download.integrity.clone(),
        deleted: false,
    })
    .map_err(StorageError::EncodeSyncPayload)
}

fn download_metadata_tombstone_sync_payload(
    download: &DownloadMetadataRecord,
) -> Result<String, StorageError> {
    serde_json::to_string(&DownloadMetadataSyncPayload {
        download_id: download.download_id.clone(),
        source_url: download.source_url.clone(),
        final_url: download.final_url.clone(),
        route: download.route.clone(),
        transport_id: download.transport_id.clone(),
        filename: download.filename.clone(),
        content_type: download.content_type.clone(),
        size_bytes: download.size_bytes,
        integrity: download.integrity.clone(),
        deleted: true,
    })
    .map_err(StorageError::EncodeSyncPayload)
}

fn calendar_event_sync_key(event_id: &str) -> String {
    format!("{CALENDAR_EVENT_SYNC_KEY_PREFIX}{event_id}")
}

fn calendar_event_sync_payload(event: &CalendarEventUpdate) -> Result<String, StorageError> {
    serde_json::to_string(&CalendarEventSyncPayload {
        event_id: event.event_id.clone(),
        calendar_id: event.calendar_id.clone(),
        title: event.title.clone(),
        starts_at: event.starts_at,
        ends_at: event.ends_at,
        time_zone: event.time_zone.clone(),
        location: event.location.clone(),
        notes: event.notes.clone(),
        recurrence_rule: event.recurrence_rule.clone(),
        reminder_minutes: event.reminder_minutes,
        deleted: false,
    })
    .map_err(StorageError::EncodeSyncPayload)
}

fn calendar_event_tombstone_sync_payload(
    event: &CalendarEventRecord,
) -> Result<String, StorageError> {
    serde_json::to_string(&CalendarEventSyncPayload {
        event_id: event.event_id.clone(),
        calendar_id: event.calendar_id.clone(),
        title: event.title.clone(),
        starts_at: event.starts_at,
        ends_at: event.ends_at,
        time_zone: event.time_zone.clone(),
        location: event.location.clone(),
        notes: event.notes.clone(),
        recurrence_rule: event.recurrence_rule.clone(),
        reminder_minutes: event.reminder_minutes,
        deleted: true,
    })
    .map_err(StorageError::EncodeSyncPayload)
}

fn chat_conversation_sync_key(conversation_id: &str) -> String {
    format!("{CHAT_CONVERSATION_SYNC_KEY_PREFIX}{conversation_id}")
}

fn chat_conversation_sync_payload(
    conversation: &ChatConversationUpdate,
) -> Result<String, StorageError> {
    serde_json::to_string(&ChatConversationSyncPayload {
        conversation_id: conversation.conversation_id.clone(),
        provider_id: conversation.provider_id.clone(),
        external_thread_id: conversation.external_thread_id.clone(),
        display_name: conversation.display_name.clone(),
        avatar_key: conversation.avatar_key.clone(),
        last_message_at: conversation.last_message_at,
        unread_count: conversation.unread_count,
        archived: conversation.archived,
        muted: conversation.muted,
        deleted: false,
    })
    .map_err(StorageError::EncodeSyncPayload)
}

fn chat_conversation_tombstone_sync_payload(
    conversation: &ChatConversationRecord,
) -> Result<String, StorageError> {
    serde_json::to_string(&ChatConversationSyncPayload {
        conversation_id: conversation.conversation_id.clone(),
        provider_id: conversation.provider_id.clone(),
        external_thread_id: conversation.external_thread_id.clone(),
        display_name: conversation.display_name.clone(),
        avatar_key: conversation.avatar_key.clone(),
        last_message_at: conversation.last_message_at,
        unread_count: conversation.unread_count,
        archived: conversation.archived,
        muted: conversation.muted,
        deleted: true,
    })
    .map_err(StorageError::EncodeSyncPayload)
}

fn contact_card_sync_key(contact_id: &str) -> String {
    format!("{CONTACT_CARD_SYNC_KEY_PREFIX}{contact_id}")
}

fn contact_card_sync_payload(contact: &ContactCardUpdate) -> Result<String, StorageError> {
    serde_json::to_string(&ContactCardSyncPayload {
        contact_id: contact.contact_id.clone(),
        display_name: contact.display_name.clone(),
        given_name: contact.given_name.clone(),
        family_name: contact.family_name.clone(),
        organization: contact.organization.clone(),
        primary_email: contact.primary_email.clone(),
        primary_phone: contact.primary_phone.clone(),
        notes: contact.notes.clone(),
        avatar_key: contact.avatar_key.clone(),
        deleted: false,
    })
    .map_err(StorageError::EncodeSyncPayload)
}

fn contact_card_tombstone_sync_payload(
    contact: &ContactCardRecord,
) -> Result<String, StorageError> {
    serde_json::to_string(&ContactCardSyncPayload {
        contact_id: contact.contact_id.clone(),
        display_name: contact.display_name.clone(),
        given_name: contact.given_name.clone(),
        family_name: contact.family_name.clone(),
        organization: contact.organization.clone(),
        primary_email: contact.primary_email.clone(),
        primary_phone: contact.primary_phone.clone(),
        notes: contact.notes.clone(),
        avatar_key: contact.avatar_key.clone(),
        deleted: true,
    })
    .map_err(StorageError::EncodeSyncPayload)
}

fn file_entry_sync_key(entry_id: &str) -> String {
    format!("{FILE_ENTRY_SYNC_KEY_PREFIX}{entry_id}")
}

fn file_entry_sync_payload(entry: &FileEntryUpdate) -> Result<String, StorageError> {
    serde_json::to_string(&FileEntrySyncPayload {
        entry_id: entry.entry_id.clone(),
        sync_set_id: entry.sync_set_id.clone(),
        parent_id: entry.parent_id.clone(),
        name: entry.name.clone(),
        entry_kind: entry.entry_kind.clone(),
        content_ref: entry.content_ref.clone(),
        mime_type: entry.mime_type.clone(),
        size_bytes: entry.size_bytes,
        modified_at: entry.modified_at,
        integrity: entry.integrity.clone(),
        retention_policy: entry.retention_policy.clone(),
        deleted: false,
    })
    .map_err(StorageError::EncodeSyncPayload)
}

fn file_entry_tombstone_sync_payload(entry: &FileEntryRecord) -> Result<String, StorageError> {
    serde_json::to_string(&FileEntrySyncPayload {
        entry_id: entry.entry_id.clone(),
        sync_set_id: entry.sync_set_id.clone(),
        parent_id: entry.parent_id.clone(),
        name: entry.name.clone(),
        entry_kind: entry.entry_kind.clone(),
        content_ref: entry.content_ref.clone(),
        mime_type: entry.mime_type.clone(),
        size_bytes: entry.size_bytes,
        modified_at: entry.modified_at,
        integrity: entry.integrity.clone(),
        retention_policy: entry.retention_policy.clone(),
        deleted: true,
    })
    .map_err(StorageError::EncodeSyncPayload)
}

fn storage_provider_sync_key(provider_id: &str) -> String {
    format!("{STORAGE_PROVIDER_SYNC_KEY_PREFIX}{provider_id}")
}

fn storage_provider_sync_payload(provider: &StorageProviderUpdate) -> Result<String, StorageError> {
    serde_json::to_string(&StorageProviderSyncPayload {
        provider_id: provider.provider_id.clone(),
        provider_kind: provider.provider_kind.clone(),
        display_name: provider.display_name.clone(),
        endpoint_ref: provider.endpoint_ref.clone(),
        discovery: provider.discovery,
        connectivity: provider.connectivity,
        object_transfer: provider.object_transfer,
        availability: provider.availability,
        mutable_roots: provider.mutable_roots,
        quota_bytes: provider.quota_bytes,
        max_retained_objects: provider.max_retained_objects,
        pinning_policy: provider.pinning_policy.clone(),
        enabled: provider.enabled,
        deleted: false,
    })
    .map_err(StorageError::EncodeSyncPayload)
}

fn storage_provider_tombstone_sync_payload(
    provider: &StorageProviderRecord,
) -> Result<String, StorageError> {
    serde_json::to_string(&StorageProviderSyncPayload {
        provider_id: provider.provider_id.clone(),
        provider_kind: provider.provider_kind.clone(),
        display_name: provider.display_name.clone(),
        endpoint_ref: provider.endpoint_ref.clone(),
        discovery: provider.discovery,
        connectivity: provider.connectivity,
        object_transfer: provider.object_transfer,
        availability: provider.availability,
        mutable_roots: provider.mutable_roots,
        quota_bytes: provider.quota_bytes,
        max_retained_objects: provider.max_retained_objects,
        pinning_policy: provider.pinning_policy.clone(),
        enabled: provider.enabled,
        deleted: true,
    })
    .map_err(StorageError::EncodeSyncPayload)
}

fn validate_sync_domain(domain: &str) -> Result<(), StorageError> {
    if domain.is_empty() {
        return Err(StorageError::InvalidSyncDomain(domain.to_string()));
    }
    Ok(())
}

fn validate_calendar_event_id(event_id: &str) -> Result<(), StorageError> {
    if is_valid_sync_identifier(event_id) {
        return Ok(());
    }
    Err(StorageError::InvalidCalendarEventId(event_id.to_string()))
}

fn validate_chat_conversation_update(
    conversation: &ChatConversationUpdate,
) -> Result<(), StorageError> {
    validate_chat_conversation_id(conversation.conversation_id.as_str())?;
    if let Some(provider_id) = conversation.provider_id.as_deref() {
        validate_chat_provider_id(provider_id)?;
    }
    Ok(())
}

fn validate_chat_conversation_id(conversation_id: &str) -> Result<(), StorageError> {
    if is_valid_sync_identifier(conversation_id) {
        return Ok(());
    }
    Err(StorageError::InvalidChatConversationId(
        conversation_id.to_string(),
    ))
}

fn validate_chat_provider_id(provider_id: &str) -> Result<(), StorageError> {
    if is_valid_sync_identifier(provider_id) {
        return Ok(());
    }
    Err(StorageError::InvalidChatProviderId(provider_id.to_string()))
}

fn validate_contact_id(contact_id: &str) -> Result<(), StorageError> {
    if is_valid_sync_identifier(contact_id) {
        return Ok(());
    }
    Err(StorageError::InvalidContactId(contact_id.to_string()))
}

fn validate_download_metadata_update(
    download: &DownloadMetadataUpdate,
) -> Result<(), StorageError> {
    validate_download_id(download.download_id.as_str())?;
    let _ = download_size_to_i64(download.size_bytes)?;
    Ok(())
}

fn validate_download_id(download_id: &str) -> Result<(), StorageError> {
    if is_valid_sync_identifier(download_id) {
        return Ok(());
    }
    Err(StorageError::InvalidDownloadId(download_id.to_string()))
}

fn validate_file_entry_update(entry: &FileEntryUpdate) -> Result<(), StorageError> {
    validate_file_entry_id(entry.entry_id.as_str())?;
    if let Some(parent_id) = entry.parent_id.as_deref() {
        validate_file_entry_id(parent_id)?;
    }
    validate_file_entry_kind(entry.entry_kind.as_str())?;
    if let Some(size_bytes) = entry.size_bytes {
        let _ = file_size_to_i64(size_bytes)?;
    }
    Ok(())
}

fn validate_file_entry_id(entry_id: &str) -> Result<(), StorageError> {
    if is_valid_sync_identifier(entry_id) {
        return Ok(());
    }
    Err(StorageError::InvalidFileEntryId(entry_id.to_string()))
}

fn validate_file_entry_kind(entry_kind: &str) -> Result<(), StorageError> {
    if matches!(entry_kind, "file" | "directory") {
        return Ok(());
    }
    Err(StorageError::InvalidFileEntryKind(entry_kind.to_string()))
}

fn validate_storage_provider_update(provider: &StorageProviderUpdate) -> Result<(), StorageError> {
    validate_storage_provider_id(provider.provider_id.as_str())?;
    validate_storage_provider_kind(provider.provider_kind.as_str())?;
    if let Some(quota_bytes) = provider.quota_bytes {
        let _ = storage_quota_to_i64(quota_bytes)?;
    }
    if let Some(pinning_policy) = provider.pinning_policy.as_deref() {
        validate_storage_pinning_policy(pinning_policy)?;
    }
    Ok(())
}

fn validate_storage_provider_id(provider_id: &str) -> Result<(), StorageError> {
    if is_valid_sync_identifier(provider_id) {
        return Ok(());
    }
    Err(StorageError::InvalidStorageProviderId(
        provider_id.to_string(),
    ))
}

fn validate_storage_provider_kind(provider_kind: &str) -> Result<(), StorageError> {
    if is_valid_sync_identifier(provider_kind) {
        return Ok(());
    }
    Err(StorageError::InvalidStorageProviderKind(
        provider_kind.to_string(),
    ))
}

fn validate_storage_pinning_policy(pinning_policy: &str) -> Result<(), StorageError> {
    if matches!(pinning_policy, "disabled" | "manual" | "auto" | "required") {
        return Ok(());
    }
    Err(StorageError::InvalidStoragePinningPolicy(
        pinning_policy.to_string(),
    ))
}

fn app_sync_domain_cursor_key(domain: &str) -> String {
    format!("{APP_SYNC_DOMAIN_CURSOR_KEY_PREFIX}{domain}")
}

fn app_sync_domain_from_cursor_key(key: &str) -> Result<String, rusqlite::Error> {
    key.strip_prefix(APP_SYNC_DOMAIN_CURSOR_KEY_PREFIX)
        .filter(|domain| !domain.is_empty())
        .map(ToString::to_string)
        .ok_or_else(|| {
            rusqlite::Error::FromSqlConversionFailure(
                1,
                Type::Text,
                Box::new(std::io::Error::new(
                    std::io::ErrorKind::InvalidData,
                    "malformed app sync domain cursor key",
                )),
            )
        })
}

fn profile_sync_root_id_from_key(key: &str) -> Result<String, rusqlite::Error> {
    key.strip_prefix(PROFILE_SYNC_ROOT_KEY_PREFIX)
        .filter(|root_id| !root_id.is_empty())
        .map(ToString::to_string)
        .ok_or_else(|| {
            rusqlite::Error::FromSqlConversionFailure(
                1,
                Type::Text,
                Box::new(std::io::Error::new(
                    std::io::ErrorKind::InvalidData,
                    "malformed profile sync root key",
                )),
            )
        })
}

fn app_sync_domain_cursor_record_from_row(
    row: &rusqlite::Row<'_>,
) -> Result<AppSyncDomainCursorRecord, rusqlite::Error> {
    let key: String = row.get(1)?;
    let value: String = row.get(2)?;
    let latest_revision = value.parse::<i64>().map_err(|source| {
        rusqlite::Error::FromSqlConversionFailure(2, Type::Text, Box::new(source))
    })?;
    Ok(AppSyncDomainCursorRecord {
        profile: row.get(0)?,
        domain: app_sync_domain_from_cursor_key(key.as_str())?,
        latest_revision,
        updated_at: row.get(3)?,
    })
}

fn sync_content_key_epoch_record_from_row(
    row: &rusqlite::Row<'_>,
) -> Result<SyncContentKeyEpochRecord, rusqlite::Error> {
    Ok(SyncContentKeyEpochRecord {
        profile: row.get(0)?,
        key_id: row.get(1)?,
        membership_epoch: row.get(2)?,
        algorithm: row.get(3)?,
        active: integer_to_bool(row.get(4)?),
        created_at: row.get(5)?,
        updated_at: row.get(6)?,
    })
}

fn profile_sync_root_record_from_row(
    row: &rusqlite::Row<'_>,
) -> Result<ProfileSyncRootRecord, rusqlite::Error> {
    let key: String = row.get(1)?;
    Ok(ProfileSyncRootRecord {
        profile: row.get(0)?,
        root_id: profile_sync_root_id_from_key(key.as_str())?,
        object_id: row.get(2)?,
        updated_at: row.get(3)?,
    })
}

fn seal_sync_payload(
    associated_data: &[u8],
    plaintext: &[u8],
    content_key: &ProfileSyncContentKey,
    nonce: [u8; PROFILE_SYNC_NONCE_BYTES],
) -> Result<Vec<u8>, SyncObjectError> {
    let key = sync_aead_key(content_key)?;
    let mut sealed = plaintext.to_vec();
    key.seal_in_place_append_tag(
        aead::Nonce::assume_unique_for_key(nonce),
        aead::Aad::from(associated_data),
        &mut sealed,
    )
    .map_err(|_| SyncObjectError::Encrypt)?;
    Ok(sealed)
}

fn open_sync_payload(
    associated_data: &[u8],
    ciphertext: &[u8],
    content_key: &ProfileSyncContentKey,
    nonce: [u8; PROFILE_SYNC_NONCE_BYTES],
) -> Result<Vec<u8>, SyncObjectError> {
    let key = sync_aead_key(content_key)?;
    let mut plaintext = ciphertext.to_vec();
    let plaintext = key
        .open_in_place(
            aead::Nonce::assume_unique_for_key(nonce),
            aead::Aad::from(associated_data),
            plaintext.as_mut_slice(),
        )
        .map_err(|_| SyncObjectError::Decrypt)?;
    Ok(plaintext.to_vec())
}

fn sync_aead_key(
    content_key: &ProfileSyncContentKey,
) -> Result<aead::LessSafeKey, SyncObjectError> {
    let key = aead::UnboundKey::new(&aead::CHACHA20_POLY1305, content_key.as_bytes())
        .map_err(|_| SyncObjectError::Encrypt)?;
    Ok(aead::LessSafeKey::new(key))
}

fn is_valid_sync_identifier(value: &str) -> bool {
    !value.is_empty()
        && value
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'_'))
}

fn decode_signed_profile_sync_membership_record(
    signed_record: &[u8],
) -> Result<(SignedSyncObject, ProfileSyncMembershipRecord), StorageError> {
    let signed_object = SignedSyncObject::from_bytes(signed_record)
        .map_err(|error| StorageError::InvalidProfileSyncMembershipRecord(error.to_string()))?;
    if !is_valid_sync_identifier(signed_object.device_id.as_str()) {
        return Err(StorageError::InvalidSyncDeviceId(
            signed_object.device_id.clone(),
        ));
    }
    let signer_public_key = ProfileSyncDevicePublicKey {
        device_id: signed_object.device_id.clone(),
        bytes: signed_object.public_key.clone(),
    };
    let payload = signed_object
        .verify_with(&signer_public_key)
        .map_err(|error| StorageError::InvalidProfileSyncMembershipRecord(error.to_string()))?;
    let membership_record = ProfileSyncMembershipRecord::from_bytes(payload)
        .map_err(|error| StorageError::InvalidProfileSyncMembershipRecord(error.to_string()))?;
    validate_profile_sync_membership_record(&membership_record)?;
    Ok((signed_object, membership_record))
}

fn validate_profile_sync_enrollment_bundle(
    bundle: &ProfileSyncEnrollmentBundle,
) -> Result<(), StorageError> {
    if bundle.schema_version != PROFILE_SYNC_ENROLLMENT_BUNDLE_SCHEMA_VERSION {
        return Err(StorageError::UnsupportedProfileSyncEnrollmentBundleSchema(
            bundle.schema_version,
        ));
    }
    if bundle.profile.is_empty() {
        return Err(StorageError::InvalidProfileSyncEnrollmentBundle(
            "bundle profile must not be empty".to_string(),
        ));
    }
    if !is_valid_sync_identifier(bundle.target_device_id.as_str()) {
        return Err(StorageError::InvalidSyncDeviceId(
            bundle.target_device_id.clone(),
        ));
    }
    if bundle.signed_membership_records.is_empty() {
        return Err(StorageError::InvalidProfileSyncEnrollmentBundle(
            "bundle must contain at least one signed membership record".to_string(),
        ));
    }
    if bundle.signed_membership_records.len() > DEFAULT_PROFILE_SYNC_ENROLLMENT_BUNDLE_MAX_RECORDS {
        return Err(StorageError::InvalidProfileSyncEnrollmentBundle(format!(
            "bundle has {} signed membership records, exceeding max {}",
            bundle.signed_membership_records.len(),
            DEFAULT_PROFILE_SYNC_ENROLLMENT_BUNDLE_MAX_RECORDS
        )));
    }

    let mut target_record_seen = false;
    for signed_record in &bundle.signed_membership_records {
        let (_, membership_record) =
            decode_signed_profile_sync_membership_record(signed_record.as_slice())?;
        if membership_record.profile != bundle.profile {
            return Err(StorageError::InvalidProfileSyncEnrollmentBundle(format!(
                "membership record {} belongs to profile {}, not bundle profile {}",
                membership_record.record_id, membership_record.profile, bundle.profile
            )));
        }
        if membership_record.device_id == bundle.target_device_id
            && matches!(
                membership_record.record_kind.as_str(),
                PROFILE_SYNC_MEMBERSHIP_RECORD_KIND_ENROLL_DEVICE
                    | PROFILE_SYNC_MEMBERSHIP_RECORD_KIND_ROTATE_DEVICE_KEY
            )
        {
            target_record_seen = true;
        }
    }
    if !target_record_seen {
        return Err(StorageError::InvalidProfileSyncEnrollmentBundle(format!(
            "bundle does not include an enrollment or key-rotation record for target device {}",
            bundle.target_device_id
        )));
    }
    Ok(())
}

fn validate_profile_sync_secret_handoff_bundle(
    bundle: &ProfileSyncSecretHandoffBundle,
) -> Result<(), StorageError> {
    if bundle.schema_version != PROFILE_SYNC_SECRET_HANDOFF_BUNDLE_SCHEMA_VERSION {
        return Err(
            StorageError::UnsupportedProfileSyncSecretHandoffBundleSchema(bundle.schema_version),
        );
    }
    if bundle.profile.trim().is_empty() {
        return Err(StorageError::InvalidProfileSyncSecretHandoffBundle(
            "missing profile id".to_string(),
        ));
    }
    if !is_valid_sync_identifier(bundle.target_device_id.as_str()) {
        return Err(StorageError::InvalidSyncDeviceId(
            bundle.target_device_id.clone(),
        ));
    }
    if bundle.sync_secret_export.profile != bundle.profile {
        return Err(StorageError::InvalidProfileSyncSecretHandoffBundle(
            format!(
                "secret export profile {} does not match handoff profile {}",
                bundle.sync_secret_export.profile, bundle.profile
            ),
        ));
    }
    if bundle.enrollment_bundle.profile != bundle.profile {
        return Err(StorageError::InvalidProfileSyncSecretHandoffBundle(
            format!(
                "enrollment bundle profile {} does not match handoff profile {}",
                bundle.enrollment_bundle.profile, bundle.profile
            ),
        ));
    }
    if bundle.enrollment_bundle.target_device_id != bundle.target_device_id {
        return Err(StorageError::InvalidProfileSyncSecretHandoffBundle(
            format!(
                "enrollment bundle targets device {}, not handoff target {}",
                bundle.enrollment_bundle.target_device_id, bundle.target_device_id
            ),
        ));
    }
    SlateSyncSecret::from_export_for_profile(&bundle.sync_secret_export, bundle.profile.as_str())
        .map_err(profile_sync_secret_handoff_sync_object_error)?;
    validate_profile_sync_enrollment_bundle(&bundle.enrollment_bundle)
}

fn profile_sync_secret_handoff_sync_object_error(error: SyncObjectError) -> StorageError {
    StorageError::InvalidProfileSyncSecretHandoffBundle(error.to_string())
}

fn validate_profile_sync_device_enrollment_request(
    request: &ProfileSyncDeviceEnrollmentRequest,
) -> Result<(), StorageError> {
    if request.schema_version != PROFILE_SYNC_DEVICE_ENROLLMENT_REQUEST_SCHEMA_VERSION {
        return Err(
            StorageError::UnsupportedProfileSyncDeviceEnrollmentRequestSchema(
                request.schema_version,
            ),
        );
    }
    if request.profile.trim().is_empty() {
        return Err(StorageError::InvalidProfileSyncDeviceEnrollmentRequest(
            "missing profile id".to_string(),
        ));
    }
    if !is_valid_sync_identifier(request.device_id.as_str()) {
        return Err(StorageError::InvalidSyncDeviceId(request.device_id.clone()));
    }
    Ok(())
}

fn validate_profile_sync_membership_record(
    record: &ProfileSyncMembershipRecord,
) -> Result<(), StorageError> {
    if record.schema_version != PROFILE_SYNC_MEMBERSHIP_RECORD_SCHEMA_VERSION {
        return Err(StorageError::UnsupportedProfileSyncMembershipRecordSchema(
            record.schema_version,
        ));
    }
    if record.membership_epoch < 1 {
        return Err(StorageError::InvalidSyncMembershipEpoch(
            record.membership_epoch,
        ));
    }
    if !is_valid_sync_identifier(record.record_id.as_str()) {
        return Err(StorageError::InvalidSyncMembershipRecordId(
            record.record_id.clone(),
        ));
    }
    if !is_valid_sync_identifier(record.device_id.as_str()) {
        return Err(StorageError::InvalidSyncDeviceId(record.device_id.clone()));
    }
    if !is_valid_profile_sync_membership_record_kind(record.record_kind.as_str()) {
        return Err(StorageError::InvalidSyncMembershipRecordKind(
            record.record_kind.clone(),
        ));
    }

    match record.record_kind.as_str() {
        PROFILE_SYNC_MEMBERSHIP_RECORD_KIND_ENROLL_DEVICE
        | PROFILE_SYNC_MEMBERSHIP_RECORD_KIND_ENROLL_PROVIDER
        | PROFILE_SYNC_MEMBERSHIP_RECORD_KIND_ROTATE_DEVICE_KEY => {
            let public_key = record.device_public_key.as_ref().ok_or_else(|| {
                StorageError::InvalidProfileSyncMembershipRecord(format!(
                    "{} requires a device public key",
                    record.record_kind
                ))
            })?;
            validate_membership_record_public_key(record, public_key)?;
        }
        PROFILE_SYNC_MEMBERSHIP_RECORD_KIND_REVOKE_DEVICE => {
            if record.device_public_key.is_some() {
                return Err(StorageError::InvalidProfileSyncMembershipRecord(
                    "revoke-device records must not carry a replacement public key".to_string(),
                ));
            }
        }
        _ => {
            return Err(StorageError::InvalidSyncMembershipRecordKind(
                record.record_kind.clone(),
            ));
        }
    }

    Ok(())
}

fn validate_membership_record_public_key(
    record: &ProfileSyncMembershipRecord,
    public_key: &ProfileSyncDevicePublicKey,
) -> Result<(), StorageError> {
    if !is_valid_sync_identifier(public_key.device_id.as_str()) {
        return Err(StorageError::InvalidSyncDeviceId(
            public_key.device_id.clone(),
        ));
    }
    if public_key.device_id != record.device_id {
        return Err(StorageError::InvalidProfileSyncMembershipRecord(format!(
            "membership public key device id {} does not match target device {}",
            public_key.device_id, record.device_id
        )));
    }
    if public_key.bytes.is_empty() {
        return Err(StorageError::InvalidProfileSyncMembershipRecord(format!(
            "membership record {} has an empty device public key",
            record.record_id
        )));
    }
    Ok(())
}

fn validate_sync_account_membership_registration(
    registration: &SyncAccountMembershipRecordRegistration,
) -> Result<(), StorageError> {
    if registration.membership_epoch < 1 {
        return Err(StorageError::InvalidSyncMembershipEpoch(
            registration.membership_epoch,
        ));
    }
    if !is_valid_sync_identifier(registration.record_id.as_str()) {
        return Err(StorageError::InvalidSyncMembershipRecordId(
            registration.record_id.clone(),
        ));
    }
    if !is_valid_profile_sync_membership_record_kind(registration.record_kind.as_str()) {
        return Err(StorageError::InvalidSyncMembershipRecordKind(
            registration.record_kind.clone(),
        ));
    }
    if !is_valid_sync_identifier(registration.device_id.as_str()) {
        return Err(StorageError::InvalidSyncDeviceId(
            registration.device_id.clone(),
        ));
    }
    if !is_valid_sync_identifier(registration.signer_device_id.as_str()) {
        return Err(StorageError::InvalidSyncDeviceId(
            registration.signer_device_id.clone(),
        ));
    }
    if registration.signed_record.is_empty() {
        return Err(StorageError::InvalidProfileSyncMembershipRecord(
            "membership record signed bytes are empty".to_string(),
        ));
    }
    Ok(())
}

fn is_valid_profile_sync_membership_record_kind(record_kind: &str) -> bool {
    matches!(
        record_kind,
        PROFILE_SYNC_MEMBERSHIP_RECORD_KIND_ENROLL_DEVICE
            | PROFILE_SYNC_MEMBERSHIP_RECORD_KIND_ENROLL_PROVIDER
            | PROFILE_SYNC_MEMBERSHIP_RECORD_KIND_REVOKE_DEVICE
            | PROFILE_SYNC_MEMBERSHIP_RECORD_KIND_ROTATE_DEVICE_KEY
    )
}

fn record_sync_device_seen_in_transaction(
    transaction: &rusqlite::Transaction<'_>,
    profile: &str,
    device_id: &str,
    now: i64,
) -> Result<(), rusqlite::Error> {
    transaction.execute(
        "INSERT INTO sync_devices
           (profile, device_id, label, membership_epoch, provider_authority,
            created_at, last_seen_at)
         VALUES (?1, ?2, NULL, 1, 0, ?3, ?3)
         ON CONFLICT(profile, device_id) DO UPDATE SET
           last_seen_at = excluded.last_seen_at",
        params![profile, device_id, now],
    )?;
    Ok(())
}

fn record_sync_device_roster_entry_in_transaction(
    transaction: &rusqlite::Transaction<'_>,
    profile: &str,
    device_id: &str,
    membership_epoch: i64,
    now: i64,
) -> Result<(), rusqlite::Error> {
    transaction.execute(
        "INSERT INTO sync_devices
           (profile, device_id, label, membership_epoch, provider_authority,
            created_at, last_seen_at)
         VALUES (?1, ?2, NULL, ?3, 0, ?4, ?4)
         ON CONFLICT(profile, device_id) DO UPDATE SET
           membership_epoch = excluded.membership_epoch,
           last_seen_at = excluded.last_seen_at",
        params![profile, device_id, membership_epoch, now],
    )?;
    Ok(())
}

fn record_sync_device_roster_entry_with_provider_authority_in_transaction(
    transaction: &rusqlite::Transaction<'_>,
    profile: &str,
    device_id: &str,
    membership_epoch: i64,
    provider_authority: bool,
    now: i64,
) -> Result<(), rusqlite::Error> {
    transaction.execute(
        "INSERT INTO sync_devices
           (profile, device_id, label, membership_epoch, provider_authority,
            created_at, last_seen_at)
         VALUES (?1, ?2, NULL, ?3, ?4, ?5, ?5)
         ON CONFLICT(profile, device_id) DO UPDATE SET
           membership_epoch = excluded.membership_epoch,
           provider_authority = excluded.provider_authority,
           last_seen_at = excluded.last_seen_at",
        params![
            profile,
            device_id,
            membership_epoch,
            bool_to_integer(provider_authority),
            now
        ],
    )?;
    Ok(())
}

fn sync_change_by_device_sequence_in_transaction(
    transaction: &rusqlite::Transaction<'_>,
    profile: &str,
    device_id: &str,
    device_sequence: i64,
) -> Result<Option<SyncChangeRecord>, rusqlite::Error> {
    transaction
        .query_row(
            "SELECT id, profile, domain, entity_key, operation, payload, device_id,
                    device_sequence, logical_clock, created_at, applied_at
             FROM settings_changes
             WHERE profile = ?1 AND device_id = ?2 AND device_sequence = ?3",
            params![profile, device_id, device_sequence],
            sync_change_record_from_row,
        )
        .optional()
}

fn sync_setting_winner_in_transaction(
    transaction: &rusqlite::Transaction<'_>,
    profile: &str,
    domain: &str,
    key: &str,
) -> Result<Option<SyncChangeRecord>, rusqlite::Error> {
    transaction
        .query_row(
            "SELECT id, profile, domain, entity_key, operation, payload, device_id,
                    device_sequence, logical_clock, created_at, applied_at
             FROM settings_changes
             WHERE profile = ?1
               AND domain = ?2
               AND entity_key = ?3
               AND operation = 'set_text'
               AND applied_at IS NOT NULL
             ORDER BY logical_clock DESC, device_id DESC, device_sequence DESC, id DESC
             LIMIT 1",
            params![profile, domain, key],
            sync_change_record_from_row,
        )
        .optional()
}

fn setting_change_wins(
    incoming: &IncomingSyncSettingText,
    existing: Option<&SyncChangeRecord>,
) -> bool {
    let Some(existing) = existing else {
        return true;
    };

    (
        incoming.logical_clock,
        incoming.device_id.as_str(),
        incoming.device_sequence,
    ) > (
        existing.logical_clock,
        existing.device_id.as_str(),
        existing.device_sequence,
    )
}

fn apply_sync_setting_materialized_view_in_transaction(
    transaction: &rusqlite::Transaction<'_>,
    change: &IncomingSyncSettingText,
    now: i64,
) -> Result<(), rusqlite::Error> {
    if change.profile == DEFAULT_PROFILE_ID && change.domain == SYNC_DOMAIN_SETTINGS {
        transaction.execute(
            "INSERT INTO settings (key, value, updated_at)
             VALUES (?1, ?2, ?3)
             ON CONFLICT(key) DO UPDATE SET
               value = excluded.value,
               updated_at = excluded.updated_at",
            params![change.key.as_str(), change.value.as_str(), now],
        )?;
    }

    if change.domain == SYNC_DOMAIN_BOOKMARKS
        && change
            .key
            .as_str()
            .starts_with(BOOKMARK_HOME_SLOT_SYNC_KEY_PREFIX)
    {
        let payload = bookmark_home_slot_sync_payload_from_text(change.value.as_str())?;
        let expected_key = bookmark_home_slot_sync_key(payload.position);
        if change.key != expected_key {
            return Err(invalid_bookmark_slot_sync_payload_error(format!(
                "bookmark slot sync key {} does not match payload position {}",
                change.key, payload.position
            )));
        }
        apply_bookmark_slot_sync_payload_in_transaction(
            transaction,
            change.profile.as_str(),
            &payload,
            now,
        )?;
    }

    if change.domain == SYNC_DOMAIN_CALENDAR
        && change
            .key
            .as_str()
            .starts_with(CALENDAR_EVENT_SYNC_KEY_PREFIX)
    {
        let payload = calendar_event_sync_payload_from_text(change.value.as_str())?;
        if !is_valid_sync_identifier(payload.event_id.as_str()) {
            return Err(invalid_calendar_event_sync_payload_error(format!(
                "invalid calendar event id: {}",
                payload.event_id
            )));
        }
        let expected_key = calendar_event_sync_key(payload.event_id.as_str());
        if change.key != expected_key {
            return Err(invalid_calendar_event_sync_payload_error(format!(
                "calendar event sync key {} does not match payload id {}",
                change.key, payload.event_id
            )));
        }
        apply_calendar_event_sync_payload_in_transaction(
            transaction,
            change.profile.as_str(),
            &payload,
            now,
        )?;
    }

    if change.domain == SYNC_DOMAIN_CHAT
        && change
            .key
            .as_str()
            .starts_with(CHAT_CONVERSATION_SYNC_KEY_PREFIX)
    {
        let payload = chat_conversation_sync_payload_from_text(change.value.as_str())?;
        validate_chat_conversation_sync_payload_for_sql(&payload)?;
        let expected_key = chat_conversation_sync_key(payload.conversation_id.as_str());
        if change.key != expected_key {
            return Err(invalid_chat_conversation_sync_payload_error(format!(
                "chat conversation sync key {} does not match payload id {}",
                change.key, payload.conversation_id
            )));
        }
        apply_chat_conversation_sync_payload_in_transaction(
            transaction,
            change.profile.as_str(),
            &payload,
            now,
        )?;
    }

    if change.domain == SYNC_DOMAIN_CONTACTS
        && change
            .key
            .as_str()
            .starts_with(CONTACT_CARD_SYNC_KEY_PREFIX)
    {
        let payload = contact_card_sync_payload_from_text(change.value.as_str())?;
        if !is_valid_sync_identifier(payload.contact_id.as_str()) {
            return Err(invalid_contact_card_sync_payload_error(format!(
                "invalid contact id: {}",
                payload.contact_id
            )));
        }
        let expected_key = contact_card_sync_key(payload.contact_id.as_str());
        if change.key != expected_key {
            return Err(invalid_contact_card_sync_payload_error(format!(
                "contact sync key {} does not match payload id {}",
                change.key, payload.contact_id
            )));
        }
        apply_contact_card_sync_payload_in_transaction(
            transaction,
            change.profile.as_str(),
            &payload,
            now,
        )?;
    }

    if change.domain == SYNC_DOMAIN_DOWNLOADS
        && change
            .key
            .as_str()
            .starts_with(DOWNLOAD_METADATA_SYNC_KEY_PREFIX)
    {
        let payload = download_metadata_sync_payload_from_text(change.value.as_str())?;
        validate_download_metadata_sync_payload_for_sql(&payload)?;
        let expected_key = download_metadata_sync_key(payload.download_id.as_str());
        if change.key != expected_key {
            return Err(invalid_download_metadata_sync_payload_error(format!(
                "download metadata sync key {} does not match payload id {}",
                change.key, payload.download_id
            )));
        }
        apply_download_metadata_sync_payload_in_transaction(
            transaction,
            change.profile.as_str(),
            &payload,
            now,
        )?;
    }

    if change.domain == SYNC_DOMAIN_FILES
        && change.key.as_str().starts_with(FILE_ENTRY_SYNC_KEY_PREFIX)
    {
        let payload = file_entry_sync_payload_from_text(change.value.as_str())?;
        validate_file_entry_sync_payload_for_sql(&payload)?;
        let expected_key = file_entry_sync_key(payload.entry_id.as_str());
        if change.key != expected_key {
            return Err(invalid_file_entry_sync_payload_error(format!(
                "file entry sync key {} does not match payload id {}",
                change.key, payload.entry_id
            )));
        }
        apply_file_entry_sync_payload_in_transaction(
            transaction,
            change.profile.as_str(),
            &payload,
            now,
        )?;
    }

    if change.domain == SYNC_DOMAIN_STORAGE
        && change
            .key
            .as_str()
            .starts_with(STORAGE_PROVIDER_SYNC_KEY_PREFIX)
    {
        let payload = storage_provider_sync_payload_from_text(change.value.as_str())?;
        validate_storage_provider_sync_payload_for_sql(&payload)?;
        let expected_key = storage_provider_sync_key(payload.provider_id.as_str());
        if change.key != expected_key {
            return Err(invalid_storage_provider_sync_payload_error(format!(
                "storage provider sync key {} does not match payload id {}",
                change.key, payload.provider_id
            )));
        }
        apply_storage_provider_sync_payload_in_transaction(
            transaction,
            change.profile.as_str(),
            &payload,
            now,
        )?;
    }

    Ok(())
}

fn bookmark_home_slot_sync_payload_from_text(
    value: &str,
) -> Result<BookmarkSlotSyncPayload, rusqlite::Error> {
    serde_json::from_str(value).map_err(|source| {
        rusqlite::Error::FromSqlConversionFailure(3, Type::Text, Box::new(source))
    })
}

fn invalid_bookmark_slot_sync_payload_error(message: String) -> rusqlite::Error {
    rusqlite::Error::FromSqlConversionFailure(
        3,
        Type::Text,
        Box::new(std::io::Error::new(
            std::io::ErrorKind::InvalidData,
            message,
        )),
    )
}

fn download_metadata_sync_payload_from_text(
    value: &str,
) -> Result<DownloadMetadataSyncPayload, rusqlite::Error> {
    serde_json::from_str(value).map_err(|source| {
        rusqlite::Error::FromSqlConversionFailure(3, Type::Text, Box::new(source))
    })
}

fn invalid_download_metadata_sync_payload_error(message: String) -> rusqlite::Error {
    rusqlite::Error::FromSqlConversionFailure(
        3,
        Type::Text,
        Box::new(std::io::Error::new(
            std::io::ErrorKind::InvalidData,
            message,
        )),
    )
}

fn calendar_event_sync_payload_from_text(
    value: &str,
) -> Result<CalendarEventSyncPayload, rusqlite::Error> {
    serde_json::from_str(value).map_err(|source| {
        rusqlite::Error::FromSqlConversionFailure(3, Type::Text, Box::new(source))
    })
}

fn invalid_calendar_event_sync_payload_error(message: String) -> rusqlite::Error {
    rusqlite::Error::FromSqlConversionFailure(
        3,
        Type::Text,
        Box::new(std::io::Error::new(
            std::io::ErrorKind::InvalidData,
            message,
        )),
    )
}

fn chat_conversation_sync_payload_from_text(
    value: &str,
) -> Result<ChatConversationSyncPayload, rusqlite::Error> {
    serde_json::from_str(value).map_err(|source| {
        rusqlite::Error::FromSqlConversionFailure(3, Type::Text, Box::new(source))
    })
}

fn invalid_chat_conversation_sync_payload_error(message: String) -> rusqlite::Error {
    rusqlite::Error::FromSqlConversionFailure(
        3,
        Type::Text,
        Box::new(std::io::Error::new(
            std::io::ErrorKind::InvalidData,
            message,
        )),
    )
}

fn contact_card_sync_payload_from_text(
    value: &str,
) -> Result<ContactCardSyncPayload, rusqlite::Error> {
    serde_json::from_str(value).map_err(|source| {
        rusqlite::Error::FromSqlConversionFailure(3, Type::Text, Box::new(source))
    })
}

fn invalid_contact_card_sync_payload_error(message: String) -> rusqlite::Error {
    rusqlite::Error::FromSqlConversionFailure(
        3,
        Type::Text,
        Box::new(std::io::Error::new(
            std::io::ErrorKind::InvalidData,
            message,
        )),
    )
}

fn file_entry_sync_payload_from_text(value: &str) -> Result<FileEntrySyncPayload, rusqlite::Error> {
    serde_json::from_str(value).map_err(|source| {
        rusqlite::Error::FromSqlConversionFailure(3, Type::Text, Box::new(source))
    })
}

fn invalid_file_entry_sync_payload_error(message: String) -> rusqlite::Error {
    rusqlite::Error::FromSqlConversionFailure(
        3,
        Type::Text,
        Box::new(std::io::Error::new(
            std::io::ErrorKind::InvalidData,
            message,
        )),
    )
}

fn storage_provider_sync_payload_from_text(
    value: &str,
) -> Result<StorageProviderSyncPayload, rusqlite::Error> {
    serde_json::from_str(value).map_err(|source| {
        rusqlite::Error::FromSqlConversionFailure(3, Type::Text, Box::new(source))
    })
}

fn invalid_storage_provider_sync_payload_error(message: String) -> rusqlite::Error {
    rusqlite::Error::FromSqlConversionFailure(
        3,
        Type::Text,
        Box::new(std::io::Error::new(
            std::io::ErrorKind::InvalidData,
            message,
        )),
    )
}

fn apply_bookmark_slot_sync_payload_in_transaction(
    transaction: &rusqlite::Transaction<'_>,
    profile: &str,
    payload: &BookmarkSlotSyncPayload,
    now: i64,
) -> Result<(), rusqlite::Error> {
    if payload.deleted {
        transaction.execute(
            "DELETE FROM bookmarks
             WHERE profile = ?1
               AND (
                 position = ?2
                 OR url = ?3
                 OR (?4 IS NOT NULL AND url = ?4)
               )",
            params![
                profile,
                payload.position,
                payload.url.as_str(),
                payload.replaced_url.as_deref()
            ],
        )?;
        return Ok(());
    }

    transaction.execute(
        "DELETE FROM bookmarks
         WHERE profile = ?1
           AND (
             position = ?2
             OR url = ?3
             OR (?4 IS NOT NULL AND url = ?4)
           )",
        params![
            profile,
            payload.position,
            payload.url.as_str(),
            payload.replaced_url.as_deref()
        ],
    )?;
    transaction.execute(
        "INSERT INTO bookmarks
           (profile, url, title, folder, position, favicon_key, created_at, updated_at)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?7)",
        params![
            profile,
            payload.url.as_str(),
            payload.title.as_deref(),
            payload.folder.as_deref(),
            payload.position,
            payload.favicon_key.as_deref(),
            now
        ],
    )?;
    Ok(())
}

fn apply_calendar_event_sync_payload_in_transaction(
    transaction: &rusqlite::Transaction<'_>,
    profile: &str,
    payload: &CalendarEventSyncPayload,
    now: i64,
) -> Result<(), rusqlite::Error> {
    if payload.deleted {
        transaction.execute(
            "DELETE FROM calendar_events WHERE profile = ?1 AND event_id = ?2",
            params![profile, payload.event_id.as_str()],
        )?;
        return Ok(());
    }

    upsert_calendar_event_in_transaction(transaction, profile, payload, now)
}

fn upsert_calendar_event_in_transaction(
    transaction: &rusqlite::Transaction<'_>,
    profile: &str,
    payload: &CalendarEventSyncPayload,
    now: i64,
) -> Result<(), rusqlite::Error> {
    transaction.execute(
        "INSERT INTO calendar_events
           (profile, event_id, calendar_id, title, starts_at, ends_at, time_zone, location,
            notes, recurrence_rule, reminder_minutes, created_at, updated_at)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?12)
         ON CONFLICT(profile, event_id) DO UPDATE SET
           calendar_id = excluded.calendar_id,
           title = excluded.title,
           starts_at = excluded.starts_at,
           ends_at = excluded.ends_at,
           time_zone = excluded.time_zone,
           location = excluded.location,
           notes = excluded.notes,
           recurrence_rule = excluded.recurrence_rule,
           reminder_minutes = excluded.reminder_minutes,
           updated_at = excluded.updated_at",
        params![
            profile,
            payload.event_id.as_str(),
            payload.calendar_id.as_deref(),
            payload.title.as_str(),
            payload.starts_at,
            payload.ends_at,
            payload.time_zone.as_deref(),
            payload.location.as_deref(),
            payload.notes.as_deref(),
            payload.recurrence_rule.as_deref(),
            payload.reminder_minutes,
            now
        ],
    )?;
    Ok(())
}

fn calendar_event_record_by_id_in_transaction(
    transaction: &rusqlite::Transaction<'_>,
    profile: &str,
    event_id: &str,
) -> Result<CalendarEventRecord, rusqlite::Error> {
    transaction.query_row(
        "SELECT profile, event_id, calendar_id, title, starts_at, ends_at, time_zone,
                location, notes, recurrence_rule, reminder_minutes, created_at, updated_at
         FROM calendar_events
         WHERE profile = ?1 AND event_id = ?2",
        params![profile, event_id],
        calendar_event_record_from_row,
    )
}

fn calendar_event_record_by_id_optional_in_transaction(
    transaction: &rusqlite::Transaction<'_>,
    profile: &str,
    event_id: &str,
) -> Result<Option<CalendarEventRecord>, rusqlite::Error> {
    transaction
        .query_row(
            "SELECT profile, event_id, calendar_id, title, starts_at, ends_at, time_zone,
                    location, notes, recurrence_rule, reminder_minutes, created_at, updated_at
             FROM calendar_events
             WHERE profile = ?1 AND event_id = ?2",
            params![profile, event_id],
            calendar_event_record_from_row,
        )
        .optional()
}

fn calendar_event_record_from_row(
    row: &rusqlite::Row<'_>,
) -> Result<CalendarEventRecord, rusqlite::Error> {
    Ok(CalendarEventRecord {
        profile: row.get(0)?,
        event_id: row.get(1)?,
        calendar_id: row.get(2)?,
        title: row.get(3)?,
        starts_at: row.get(4)?,
        ends_at: row.get(5)?,
        time_zone: row.get(6)?,
        location: row.get(7)?,
        notes: row.get(8)?,
        recurrence_rule: row.get(9)?,
        reminder_minutes: row.get(10)?,
        created_at: row.get(11)?,
        updated_at: row.get(12)?,
    })
}

fn apply_chat_conversation_sync_payload_in_transaction(
    transaction: &rusqlite::Transaction<'_>,
    profile: &str,
    payload: &ChatConversationSyncPayload,
    now: i64,
) -> Result<(), rusqlite::Error> {
    if payload.deleted {
        transaction.execute(
            "DELETE FROM chat_conversations WHERE profile = ?1 AND conversation_id = ?2",
            params![profile, payload.conversation_id.as_str()],
        )?;
        return Ok(());
    }

    upsert_chat_conversation_in_transaction(transaction, profile, payload, now)
}

fn upsert_chat_conversation_in_transaction(
    transaction: &rusqlite::Transaction<'_>,
    profile: &str,
    payload: &ChatConversationSyncPayload,
    now: i64,
) -> Result<(), rusqlite::Error> {
    transaction.execute(
        "INSERT INTO chat_conversations
           (profile, conversation_id, provider_id, external_thread_id, display_name,
            avatar_key, last_message_at, unread_count, archived, muted, created_at,
            updated_at)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?11)
         ON CONFLICT(profile, conversation_id) DO UPDATE SET
           provider_id = excluded.provider_id,
           external_thread_id = excluded.external_thread_id,
           display_name = excluded.display_name,
           avatar_key = excluded.avatar_key,
           last_message_at = excluded.last_message_at,
           unread_count = excluded.unread_count,
           archived = excluded.archived,
           muted = excluded.muted,
           updated_at = excluded.updated_at",
        params![
            profile,
            payload.conversation_id.as_str(),
            payload.provider_id.as_deref(),
            payload.external_thread_id.as_deref(),
            payload.display_name.as_str(),
            payload.avatar_key.as_deref(),
            payload.last_message_at,
            i64::from(payload.unread_count),
            payload.archived,
            payload.muted,
            now
        ],
    )?;
    Ok(())
}

fn chat_conversation_record_by_id_in_transaction(
    transaction: &rusqlite::Transaction<'_>,
    profile: &str,
    conversation_id: &str,
) -> Result<ChatConversationRecord, rusqlite::Error> {
    transaction.query_row(
        "SELECT profile, conversation_id, provider_id, external_thread_id, display_name,
                avatar_key, last_message_at, unread_count, archived, muted, created_at,
                updated_at
         FROM chat_conversations
         WHERE profile = ?1 AND conversation_id = ?2",
        params![profile, conversation_id],
        chat_conversation_record_from_row,
    )
}

fn chat_conversation_record_by_id_optional_in_transaction(
    transaction: &rusqlite::Transaction<'_>,
    profile: &str,
    conversation_id: &str,
) -> Result<Option<ChatConversationRecord>, rusqlite::Error> {
    transaction
        .query_row(
            "SELECT profile, conversation_id, provider_id, external_thread_id, display_name,
                    avatar_key, last_message_at, unread_count, archived, muted, created_at,
                    updated_at
             FROM chat_conversations
             WHERE profile = ?1 AND conversation_id = ?2",
            params![profile, conversation_id],
            chat_conversation_record_from_row,
        )
        .optional()
}

fn chat_conversation_record_from_row(
    row: &rusqlite::Row<'_>,
) -> Result<ChatConversationRecord, rusqlite::Error> {
    let unread_count = u32::try_from(row.get::<_, i64>(7)?).map_err(|source| {
        rusqlite::Error::FromSqlConversionFailure(7, Type::Integer, Box::new(source))
    })?;
    Ok(ChatConversationRecord {
        profile: row.get(0)?,
        conversation_id: row.get(1)?,
        provider_id: row.get(2)?,
        external_thread_id: row.get(3)?,
        display_name: row.get(4)?,
        avatar_key: row.get(5)?,
        last_message_at: row.get(6)?,
        unread_count,
        archived: row.get(8)?,
        muted: row.get(9)?,
        created_at: row.get(10)?,
        updated_at: row.get(11)?,
    })
}

fn validate_chat_conversation_sync_payload_for_sql(
    payload: &ChatConversationSyncPayload,
) -> Result<(), rusqlite::Error> {
    if !is_valid_sync_identifier(payload.conversation_id.as_str()) {
        return Err(invalid_chat_conversation_sync_payload_error(format!(
            "invalid chat conversation id: {}",
            payload.conversation_id
        )));
    }
    if let Some(provider_id) = payload.provider_id.as_deref()
        && !is_valid_sync_identifier(provider_id)
    {
        return Err(invalid_chat_conversation_sync_payload_error(format!(
            "invalid chat provider id: {provider_id}"
        )));
    }
    Ok(())
}

fn apply_contact_card_sync_payload_in_transaction(
    transaction: &rusqlite::Transaction<'_>,
    profile: &str,
    payload: &ContactCardSyncPayload,
    now: i64,
) -> Result<(), rusqlite::Error> {
    if payload.deleted {
        transaction.execute(
            "DELETE FROM contact_cards WHERE profile = ?1 AND contact_id = ?2",
            params![profile, payload.contact_id.as_str()],
        )?;
        return Ok(());
    }

    upsert_contact_card_in_transaction(transaction, profile, payload, now)
}

fn upsert_contact_card_in_transaction(
    transaction: &rusqlite::Transaction<'_>,
    profile: &str,
    payload: &ContactCardSyncPayload,
    now: i64,
) -> Result<(), rusqlite::Error> {
    transaction.execute(
        "INSERT INTO contact_cards
           (profile, contact_id, display_name, given_name, family_name, organization,
            primary_email, primary_phone, notes, avatar_key, created_at, updated_at)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?11)
         ON CONFLICT(profile, contact_id) DO UPDATE SET
           display_name = excluded.display_name,
           given_name = excluded.given_name,
           family_name = excluded.family_name,
           organization = excluded.organization,
           primary_email = excluded.primary_email,
           primary_phone = excluded.primary_phone,
           notes = excluded.notes,
           avatar_key = excluded.avatar_key,
           updated_at = excluded.updated_at",
        params![
            profile,
            payload.contact_id.as_str(),
            payload.display_name.as_str(),
            payload.given_name.as_deref(),
            payload.family_name.as_deref(),
            payload.organization.as_deref(),
            payload.primary_email.as_deref(),
            payload.primary_phone.as_deref(),
            payload.notes.as_deref(),
            payload.avatar_key.as_deref(),
            now
        ],
    )?;
    Ok(())
}

fn contact_card_record_by_id_in_transaction(
    transaction: &rusqlite::Transaction<'_>,
    profile: &str,
    contact_id: &str,
) -> Result<ContactCardRecord, rusqlite::Error> {
    transaction.query_row(
        "SELECT profile, contact_id, display_name, given_name, family_name, organization,
                primary_email, primary_phone, notes, avatar_key, created_at, updated_at
         FROM contact_cards
         WHERE profile = ?1 AND contact_id = ?2",
        params![profile, contact_id],
        contact_card_record_from_row,
    )
}

fn contact_card_record_by_id_optional_in_transaction(
    transaction: &rusqlite::Transaction<'_>,
    profile: &str,
    contact_id: &str,
) -> Result<Option<ContactCardRecord>, rusqlite::Error> {
    transaction
        .query_row(
            "SELECT profile, contact_id, display_name, given_name, family_name, organization,
                    primary_email, primary_phone, notes, avatar_key, created_at, updated_at
             FROM contact_cards
             WHERE profile = ?1 AND contact_id = ?2",
            params![profile, contact_id],
            contact_card_record_from_row,
        )
        .optional()
}

fn contact_card_record_from_row(
    row: &rusqlite::Row<'_>,
) -> Result<ContactCardRecord, rusqlite::Error> {
    Ok(ContactCardRecord {
        profile: row.get(0)?,
        contact_id: row.get(1)?,
        display_name: row.get(2)?,
        given_name: row.get(3)?,
        family_name: row.get(4)?,
        organization: row.get(5)?,
        primary_email: row.get(6)?,
        primary_phone: row.get(7)?,
        notes: row.get(8)?,
        avatar_key: row.get(9)?,
        created_at: row.get(10)?,
        updated_at: row.get(11)?,
    })
}

fn apply_file_entry_sync_payload_in_transaction(
    transaction: &rusqlite::Transaction<'_>,
    profile: &str,
    payload: &FileEntrySyncPayload,
    now: i64,
) -> Result<(), rusqlite::Error> {
    if payload.deleted {
        transaction.execute(
            "DELETE FROM file_entries WHERE profile = ?1 AND entry_id = ?2",
            params![profile, payload.entry_id.as_str()],
        )?;
        return Ok(());
    }

    upsert_file_entry_in_transaction(transaction, profile, payload, now)
}

fn upsert_file_entry_in_transaction(
    transaction: &rusqlite::Transaction<'_>,
    profile: &str,
    payload: &FileEntrySyncPayload,
    now: i64,
) -> Result<(), rusqlite::Error> {
    let size_bytes = match payload.size_bytes {
        Some(size_bytes) => Some(file_size_to_sql_i64(size_bytes)?),
        None => None,
    };
    transaction.execute(
        "INSERT INTO file_entries
           (profile, entry_id, sync_set_id, parent_id, name, entry_kind, content_ref,
            mime_type, size_bytes, modified_at, integrity, retention_policy, created_at,
            updated_at)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?13)
         ON CONFLICT(profile, entry_id) DO UPDATE SET
           sync_set_id = excluded.sync_set_id,
           parent_id = excluded.parent_id,
           name = excluded.name,
           entry_kind = excluded.entry_kind,
           content_ref = excluded.content_ref,
           mime_type = excluded.mime_type,
           size_bytes = excluded.size_bytes,
           modified_at = excluded.modified_at,
           integrity = excluded.integrity,
           retention_policy = excluded.retention_policy,
           updated_at = excluded.updated_at",
        params![
            profile,
            payload.entry_id.as_str(),
            payload.sync_set_id.as_deref(),
            payload.parent_id.as_deref(),
            payload.name.as_str(),
            payload.entry_kind.as_str(),
            payload.content_ref.as_deref(),
            payload.mime_type.as_deref(),
            size_bytes,
            payload.modified_at,
            payload.integrity.as_deref(),
            payload.retention_policy.as_deref(),
            now
        ],
    )?;
    Ok(())
}

fn file_entry_record_by_id_in_transaction(
    transaction: &rusqlite::Transaction<'_>,
    profile: &str,
    entry_id: &str,
) -> Result<FileEntryRecord, rusqlite::Error> {
    transaction.query_row(
        "SELECT profile, entry_id, sync_set_id, parent_id, name, entry_kind, content_ref,
                mime_type, size_bytes, modified_at, integrity, retention_policy, created_at,
                updated_at
         FROM file_entries
         WHERE profile = ?1 AND entry_id = ?2",
        params![profile, entry_id],
        file_entry_record_from_row,
    )
}

fn file_entry_record_by_id_optional_in_transaction(
    transaction: &rusqlite::Transaction<'_>,
    profile: &str,
    entry_id: &str,
) -> Result<Option<FileEntryRecord>, rusqlite::Error> {
    transaction
        .query_row(
            "SELECT profile, entry_id, sync_set_id, parent_id, name, entry_kind, content_ref,
                    mime_type, size_bytes, modified_at, integrity, retention_policy, created_at,
                    updated_at
             FROM file_entries
             WHERE profile = ?1 AND entry_id = ?2",
            params![profile, entry_id],
            file_entry_record_from_row,
        )
        .optional()
}

fn file_entry_record_from_row(row: &rusqlite::Row<'_>) -> Result<FileEntryRecord, rusqlite::Error> {
    let size_bytes = match row.get::<_, Option<i64>>(8)? {
        Some(size_bytes) => Some(file_size_from_sql_i64(size_bytes)?),
        None => None,
    };
    Ok(FileEntryRecord {
        profile: row.get(0)?,
        entry_id: row.get(1)?,
        sync_set_id: row.get(2)?,
        parent_id: row.get(3)?,
        name: row.get(4)?,
        entry_kind: row.get(5)?,
        content_ref: row.get(6)?,
        mime_type: row.get(7)?,
        size_bytes,
        modified_at: row.get(9)?,
        integrity: row.get(10)?,
        retention_policy: row.get(11)?,
        created_at: row.get(12)?,
        updated_at: row.get(13)?,
    })
}

fn validate_file_entry_sync_payload_for_sql(
    payload: &FileEntrySyncPayload,
) -> Result<(), rusqlite::Error> {
    if !is_valid_sync_identifier(payload.entry_id.as_str()) {
        return Err(invalid_file_entry_sync_payload_error(format!(
            "invalid file entry id: {}",
            payload.entry_id
        )));
    }
    if let Some(parent_id) = payload.parent_id.as_deref()
        && !is_valid_sync_identifier(parent_id)
    {
        return Err(invalid_file_entry_sync_payload_error(format!(
            "invalid file parent id: {parent_id}"
        )));
    }
    if !matches!(payload.entry_kind.as_str(), "file" | "directory") {
        return Err(invalid_file_entry_sync_payload_error(format!(
            "invalid file entry kind: {}",
            payload.entry_kind
        )));
    }
    if let Some(size_bytes) = payload.size_bytes {
        let _ = file_size_to_sql_i64(size_bytes)?;
    }
    Ok(())
}

fn file_size_to_i64(size_bytes: u64) -> Result<i64, StorageError> {
    i64::try_from(size_bytes).map_err(|_| StorageError::InvalidFileSize(size_bytes))
}

fn file_size_to_sql_i64(size_bytes: u64) -> Result<i64, rusqlite::Error> {
    i64::try_from(size_bytes).map_err(|_| {
        invalid_file_entry_sync_payload_error(format!(
            "file size exceeds SQLite integer range: {size_bytes}"
        ))
    })
}

fn file_size_from_sql_i64(size_bytes: i64) -> Result<u64, rusqlite::Error> {
    u64::try_from(size_bytes).map_err(|source| {
        rusqlite::Error::FromSqlConversionFailure(8, Type::Integer, Box::new(source))
    })
}

fn apply_storage_provider_sync_payload_in_transaction(
    transaction: &rusqlite::Transaction<'_>,
    profile: &str,
    payload: &StorageProviderSyncPayload,
    now: i64,
) -> Result<(), rusqlite::Error> {
    if payload.deleted {
        transaction.execute(
            "DELETE FROM storage_providers WHERE profile = ?1 AND provider_id = ?2",
            params![profile, payload.provider_id.as_str()],
        )?;
        return Ok(());
    }

    upsert_storage_provider_in_transaction(transaction, profile, payload, now)
}

fn upsert_storage_provider_in_transaction(
    transaction: &rusqlite::Transaction<'_>,
    profile: &str,
    payload: &StorageProviderSyncPayload,
    now: i64,
) -> Result<(), rusqlite::Error> {
    let quota_bytes = match payload.quota_bytes {
        Some(quota_bytes) => Some(storage_quota_to_sql_i64(quota_bytes)?),
        None => None,
    };
    let max_retained_objects = payload.max_retained_objects.map(i64::from);
    transaction.execute(
        "INSERT INTO storage_providers
           (profile, provider_id, provider_kind, display_name, endpoint_ref, discovery,
            connectivity, object_transfer, availability, mutable_roots, quota_bytes,
            max_retained_objects, pinning_policy, enabled, created_at, updated_at)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14, ?15, ?15)
         ON CONFLICT(profile, provider_id) DO UPDATE SET
           provider_kind = excluded.provider_kind,
           display_name = excluded.display_name,
           endpoint_ref = excluded.endpoint_ref,
           discovery = excluded.discovery,
           connectivity = excluded.connectivity,
           object_transfer = excluded.object_transfer,
           availability = excluded.availability,
           mutable_roots = excluded.mutable_roots,
           quota_bytes = excluded.quota_bytes,
           max_retained_objects = excluded.max_retained_objects,
           pinning_policy = excluded.pinning_policy,
           enabled = excluded.enabled,
           updated_at = excluded.updated_at",
        params![
            profile,
            payload.provider_id.as_str(),
            payload.provider_kind.as_str(),
            payload.display_name.as_str(),
            payload.endpoint_ref.as_deref(),
            payload.discovery,
            payload.connectivity,
            payload.object_transfer,
            payload.availability,
            payload.mutable_roots,
            quota_bytes,
            max_retained_objects,
            payload.pinning_policy.as_deref(),
            payload.enabled,
            now
        ],
    )?;
    Ok(())
}

fn storage_provider_record_by_id_in_transaction(
    transaction: &rusqlite::Transaction<'_>,
    profile: &str,
    provider_id: &str,
) -> Result<StorageProviderRecord, rusqlite::Error> {
    transaction.query_row(
        "SELECT profile, provider_id, provider_kind, display_name, endpoint_ref, discovery,
                connectivity, object_transfer, availability, mutable_roots, quota_bytes,
                max_retained_objects, pinning_policy, enabled, created_at, updated_at
         FROM storage_providers
         WHERE profile = ?1 AND provider_id = ?2",
        params![profile, provider_id],
        storage_provider_record_from_row,
    )
}

fn storage_provider_record_by_id_optional_in_transaction(
    transaction: &rusqlite::Transaction<'_>,
    profile: &str,
    provider_id: &str,
) -> Result<Option<StorageProviderRecord>, rusqlite::Error> {
    transaction
        .query_row(
            "SELECT profile, provider_id, provider_kind, display_name, endpoint_ref, discovery,
                    connectivity, object_transfer, availability, mutable_roots, quota_bytes,
                    max_retained_objects, pinning_policy, enabled, created_at, updated_at
             FROM storage_providers
             WHERE profile = ?1 AND provider_id = ?2",
            params![profile, provider_id],
            storage_provider_record_from_row,
        )
        .optional()
}

fn storage_provider_record_from_row(
    row: &rusqlite::Row<'_>,
) -> Result<StorageProviderRecord, rusqlite::Error> {
    let quota_bytes = match row.get::<_, Option<i64>>(10)? {
        Some(quota_bytes) => Some(storage_quota_from_sql_i64(quota_bytes)?),
        None => None,
    };
    let max_retained_objects = match row.get::<_, Option<i64>>(11)? {
        Some(max_retained_objects) => {
            Some(u32::try_from(max_retained_objects).map_err(|source| {
                rusqlite::Error::FromSqlConversionFailure(11, Type::Integer, Box::new(source))
            })?)
        }
        None => None,
    };
    Ok(StorageProviderRecord {
        profile: row.get(0)?,
        provider_id: row.get(1)?,
        provider_kind: row.get(2)?,
        display_name: row.get(3)?,
        endpoint_ref: row.get(4)?,
        discovery: row.get(5)?,
        connectivity: row.get(6)?,
        object_transfer: row.get(7)?,
        availability: row.get(8)?,
        mutable_roots: row.get(9)?,
        quota_bytes,
        max_retained_objects,
        pinning_policy: row.get(12)?,
        enabled: row.get(13)?,
        created_at: row.get(14)?,
        updated_at: row.get(15)?,
    })
}

fn validate_storage_provider_sync_payload_for_sql(
    payload: &StorageProviderSyncPayload,
) -> Result<(), rusqlite::Error> {
    if !is_valid_sync_identifier(payload.provider_id.as_str()) {
        return Err(invalid_storage_provider_sync_payload_error(format!(
            "invalid storage provider id: {}",
            payload.provider_id
        )));
    }
    if !is_valid_sync_identifier(payload.provider_kind.as_str()) {
        return Err(invalid_storage_provider_sync_payload_error(format!(
            "invalid storage provider kind: {}",
            payload.provider_kind
        )));
    }
    if let Some(quota_bytes) = payload.quota_bytes {
        let _ = storage_quota_to_sql_i64(quota_bytes)?;
    }
    if let Some(pinning_policy) = payload.pinning_policy.as_deref()
        && !matches!(pinning_policy, "disabled" | "manual" | "auto" | "required")
    {
        return Err(invalid_storage_provider_sync_payload_error(format!(
            "invalid storage provider pinning policy: {pinning_policy}"
        )));
    }
    Ok(())
}

fn storage_quota_to_i64(quota_bytes: u64) -> Result<i64, StorageError> {
    i64::try_from(quota_bytes).map_err(|_| StorageError::InvalidStorageProviderQuota(quota_bytes))
}

fn storage_quota_to_sql_i64(quota_bytes: u64) -> Result<i64, rusqlite::Error> {
    i64::try_from(quota_bytes).map_err(|_| {
        invalid_storage_provider_sync_payload_error(format!(
            "storage provider quota exceeds SQLite integer range: {quota_bytes}"
        ))
    })
}

fn storage_quota_from_sql_i64(quota_bytes: i64) -> Result<u64, rusqlite::Error> {
    u64::try_from(quota_bytes).map_err(|source| {
        rusqlite::Error::FromSqlConversionFailure(10, Type::Integer, Box::new(source))
    })
}

fn apply_download_metadata_sync_payload_in_transaction(
    transaction: &rusqlite::Transaction<'_>,
    profile: &str,
    payload: &DownloadMetadataSyncPayload,
    now: i64,
) -> Result<(), rusqlite::Error> {
    if payload.deleted {
        transaction.execute(
            "DELETE FROM downloads WHERE profile = ?1 AND download_id = ?2",
            params![profile, payload.download_id.as_str()],
        )?;
        return Ok(());
    }

    upsert_download_metadata_in_transaction(transaction, profile, payload, now)
}

fn upsert_download_metadata_in_transaction(
    transaction: &rusqlite::Transaction<'_>,
    profile: &str,
    payload: &DownloadMetadataSyncPayload,
    now: i64,
) -> Result<(), rusqlite::Error> {
    let size_bytes = download_size_to_sql_i64(payload.size_bytes)?;
    transaction.execute(
        "INSERT INTO downloads
           (profile, download_id, source_url, final_url, route, transport_id, filename,
            content_type, size_bytes, integrity, created_at, updated_at)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?11)
         ON CONFLICT(profile, download_id) DO UPDATE SET
           source_url = excluded.source_url,
           final_url = excluded.final_url,
           route = excluded.route,
           transport_id = excluded.transport_id,
           filename = excluded.filename,
           content_type = excluded.content_type,
           size_bytes = excluded.size_bytes,
           integrity = excluded.integrity,
           updated_at = excluded.updated_at",
        params![
            profile,
            payload.download_id.as_str(),
            payload.source_url.as_str(),
            payload.final_url.as_str(),
            payload.route.as_deref(),
            payload.transport_id.as_deref(),
            payload.filename.as_str(),
            payload.content_type.as_deref(),
            size_bytes,
            payload.integrity.as_deref(),
            now
        ],
    )?;
    Ok(())
}

fn download_metadata_record_by_id_in_transaction(
    transaction: &rusqlite::Transaction<'_>,
    profile: &str,
    download_id: &str,
) -> Result<DownloadMetadataRecord, rusqlite::Error> {
    transaction.query_row(
        "SELECT profile, download_id, source_url, final_url, route, transport_id,
                filename, content_type, size_bytes, integrity, created_at, updated_at
         FROM downloads
         WHERE profile = ?1 AND download_id = ?2",
        params![profile, download_id],
        download_metadata_record_from_row,
    )
}

fn download_metadata_record_by_id_optional_in_transaction(
    transaction: &rusqlite::Transaction<'_>,
    profile: &str,
    download_id: &str,
) -> Result<Option<DownloadMetadataRecord>, rusqlite::Error> {
    transaction
        .query_row(
            "SELECT profile, download_id, source_url, final_url, route, transport_id,
                    filename, content_type, size_bytes, integrity, created_at, updated_at
             FROM downloads
             WHERE profile = ?1 AND download_id = ?2",
            params![profile, download_id],
            download_metadata_record_from_row,
        )
        .optional()
}

fn download_metadata_record_from_row(
    row: &rusqlite::Row<'_>,
) -> Result<DownloadMetadataRecord, rusqlite::Error> {
    let size_bytes = download_size_from_sql_i64(row.get(8)?)?;
    Ok(DownloadMetadataRecord {
        profile: row.get(0)?,
        download_id: row.get(1)?,
        source_url: row.get(2)?,
        final_url: row.get(3)?,
        route: row.get(4)?,
        transport_id: row.get(5)?,
        filename: row.get(6)?,
        content_type: row.get(7)?,
        size_bytes,
        integrity: row.get(9)?,
        created_at: row.get(10)?,
        updated_at: row.get(11)?,
    })
}

fn validate_download_metadata_sync_payload_for_sql(
    payload: &DownloadMetadataSyncPayload,
) -> Result<(), rusqlite::Error> {
    if !is_valid_sync_identifier(payload.download_id.as_str()) {
        return Err(invalid_download_metadata_sync_payload_error(format!(
            "invalid download id: {}",
            payload.download_id
        )));
    }
    let _ = download_size_to_sql_i64(payload.size_bytes)?;
    Ok(())
}

fn download_size_to_i64(size_bytes: u64) -> Result<i64, StorageError> {
    i64::try_from(size_bytes).map_err(|_| StorageError::InvalidDownloadSize(size_bytes))
}

fn download_size_to_sql_i64(size_bytes: u64) -> Result<i64, rusqlite::Error> {
    i64::try_from(size_bytes).map_err(|_| {
        invalid_download_metadata_sync_payload_error(format!(
            "download size exceeds SQLite integer range: {size_bytes}"
        ))
    })
}

fn download_size_from_sql_i64(size_bytes: i64) -> Result<u64, rusqlite::Error> {
    u64::try_from(size_bytes).map_err(|source| {
        rusqlite::Error::FromSqlConversionFailure(8, Type::Integer, Box::new(source))
    })
}

fn apply_sync_setting_text_in_transaction(
    transaction: &rusqlite::Transaction<'_>,
    change: &IncomingSyncSettingText,
    now: i64,
) -> Result<SyncChangeRecord, rusqlite::Error> {
    record_sync_device_seen_in_transaction(
        transaction,
        change.profile.as_str(),
        change.device_id.as_str(),
        now,
    )?;

    if let Some(existing) = sync_change_by_device_sequence_in_transaction(
        transaction,
        change.profile.as_str(),
        change.device_id.as_str(),
        change.device_sequence,
    )? {
        return Ok(existing);
    }

    let existing_winner = sync_setting_winner_in_transaction(
        transaction,
        change.profile.as_str(),
        change.domain.as_str(),
        change.key.as_str(),
    )?;
    let should_apply = setting_change_wins(change, existing_winner.as_ref());

    if should_apply {
        apply_sync_setting_materialized_view_in_transaction(transaction, change, now)?;
        insert_sync_setting_text_change_in_transaction(
            transaction,
            change.profile.as_str(),
            change.domain.as_str(),
            change.key.as_str(),
            change.value.as_str(),
            change.device_id.as_str(),
            change.device_sequence,
            change.logical_clock,
            now,
        )
    } else {
        insert_sync_setting_text_change_record_in_transaction(
            transaction,
            change.profile.as_str(),
            change.domain.as_str(),
            change.key.as_str(),
            change.value.as_str(),
            change.device_id.as_str(),
            change.device_sequence,
            change.logical_clock,
            now,
            None,
        )
    }
}

fn sync_change_record_from_row(
    row: &rusqlite::Row<'_>,
) -> Result<SyncChangeRecord, rusqlite::Error> {
    Ok(SyncChangeRecord {
        id: row.get(0)?,
        profile: row.get(1)?,
        domain: row.get(2)?,
        entity_key: row.get(3)?,
        operation: row.get(4)?,
        payload: row.get(5)?,
        device_id: row.get(6)?,
        device_sequence: row.get(7)?,
        logical_clock: row.get(8)?,
        created_at: row.get(9)?,
        applied_at: row.get(10)?,
    })
}

fn sync_device_public_key_record_from_row(
    row: &rusqlite::Row<'_>,
) -> Result<SyncDevicePublicKeyRecord, rusqlite::Error> {
    Ok(SyncDevicePublicKeyRecord {
        profile: row.get(0)?,
        public_key: ProfileSyncDevicePublicKey {
            device_id: row.get(1)?,
            bytes: row.get(2)?,
        },
        membership_epoch: row.get(3)?,
        trusted: integer_to_bool(row.get(4)?),
        created_at: row.get(5)?,
        updated_at: row.get(6)?,
    })
}

fn sync_account_membership_record_from_row(
    row: &rusqlite::Row<'_>,
) -> Result<SyncAccountMembershipRecord, rusqlite::Error> {
    Ok(SyncAccountMembershipRecord {
        profile: row.get(0)?,
        record_id: row.get(1)?,
        membership_epoch: row.get(2)?,
        record_kind: row.get(3)?,
        device_id: row.get(4)?,
        signer_device_id: row.get(5)?,
        signed_record: row.get(6)?,
        created_at: row.get(7)?,
        applied_at: row.get(8)?,
    })
}

fn table_has_column(
    connection: &Connection,
    table: &str,
    column: &str,
) -> Result<bool, rusqlite::Error> {
    let mut statement = connection.prepare(&format!("PRAGMA table_info({table})"))?;
    let columns = statement.query_map([], |row| row.get::<_, String>(1))?;
    for existing_column in columns {
        if existing_column? == column {
            return Ok(true);
        }
    }
    Ok(false)
}

fn record_sync_setting_text_in_transaction(
    transaction: &rusqlite::Transaction<'_>,
    profile: &str,
    domain: &str,
    key: &str,
    value: &str,
    device_id: &str,
    now: i64,
) -> Result<SyncChangeRecord, rusqlite::Error> {
    let device_sequence = transaction.query_row(
        "SELECT COALESCE(MAX(device_sequence), 0) + 1
         FROM settings_changes
         WHERE profile = ?1 AND device_id = ?2",
        params![profile, device_id],
        |row| row.get::<_, i64>(0),
    )?;
    let logical_clock = transaction.query_row(
        "SELECT COALESCE(MAX(logical_clock), 0) + 1
         FROM settings_changes
         WHERE profile = ?1",
        [profile],
        |row| row.get::<_, i64>(0),
    )?;

    insert_sync_setting_text_change_in_transaction(
        transaction,
        profile,
        domain,
        key,
        value,
        device_id,
        device_sequence,
        logical_clock,
        now,
    )
}

fn insert_sync_setting_text_change_in_transaction(
    transaction: &rusqlite::Transaction<'_>,
    profile: &str,
    domain: &str,
    key: &str,
    value: &str,
    device_id: &str,
    device_sequence: i64,
    logical_clock: i64,
    now: i64,
) -> Result<SyncChangeRecord, rusqlite::Error> {
    let change = insert_sync_setting_text_change_record_in_transaction(
        transaction,
        profile,
        domain,
        key,
        value,
        device_id,
        device_sequence,
        logical_clock,
        now,
        Some(now),
    )?;
    let change_id = change.id;
    transaction.execute(
        "INSERT INTO settings_revisions (profile, domain, change_id, created_at)
         VALUES (?1, ?2, ?3, ?4)",
        params![profile, domain, change_id, now],
    )?;
    let revision = transaction.last_insert_rowid();
    transaction.execute(
        "INSERT INTO settings_values
           (profile, domain, key, value, value_kind, revision, updated_at)
         VALUES (?1, ?2, ?3, ?4, 'text', ?5, ?6)
         ON CONFLICT(profile, domain, key) DO UPDATE SET
           value = excluded.value,
           value_kind = excluded.value_kind,
           revision = excluded.revision,
           updated_at = excluded.updated_at",
        params![profile, domain, key, value, revision, now],
    )?;

    Ok(change)
}

fn insert_sync_setting_text_change_record_in_transaction(
    transaction: &rusqlite::Transaction<'_>,
    profile: &str,
    domain: &str,
    key: &str,
    value: &str,
    device_id: &str,
    device_sequence: i64,
    logical_clock: i64,
    now: i64,
    applied_at: Option<i64>,
) -> Result<SyncChangeRecord, rusqlite::Error> {
    transaction.execute(
        "INSERT INTO settings_changes
           (profile, domain, entity_key, operation, payload, device_id, device_sequence,
            logical_clock, created_at, applied_at)
         VALUES (?1, ?2, ?3, 'set_text', ?4, ?5, ?6, ?7, ?8, ?9)",
        params![
            profile,
            domain,
            key,
            value,
            device_id,
            device_sequence,
            logical_clock,
            now,
            applied_at
        ],
    )?;
    let change_id = transaction.last_insert_rowid();

    Ok(SyncChangeRecord {
        id: change_id,
        profile: profile.to_string(),
        domain: domain.to_string(),
        entity_key: key.to_string(),
        operation: "set_text".to_string(),
        payload: value.to_string(),
        device_id: device_id.to_string(),
        device_sequence,
        logical_clock,
        created_at: now,
        applied_at,
    })
}

#[cfg(test)]
mod tests {
    use base64::Engine as _;
    use std::collections::BTreeMap;
    use std::process;

    use super::*;

    fn test_dir(name: &str) -> PathBuf {
        let nanos = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("time should move forward")
            .as_nanos();
        let path =
            std::env::temp_dir().join(format!("slate-storage-{name}-{}-{nanos}", process::id()));
        std::fs::create_dir_all(&path).expect("create test directory");
        path
    }

    fn sign_test_sync_object(
        profile: &str,
        domain: &str,
        object_kind: &str,
        key_id: &str,
        payload: &[u8],
        content_key: &ProfileSyncContentKey,
        signer: &ProfileSyncDeviceSigner,
        nonce_byte: u8,
    ) -> Vec<u8> {
        let encrypted = EncryptedSyncObject::seal_with_nonce(
            profile,
            domain,
            object_kind,
            key_id,
            payload,
            content_key,
            [nonce_byte; PROFILE_SYNC_NONCE_BYTES],
        )
        .unwrap();
        signer
            .sign(encrypted.to_bytes().unwrap().as_slice())
            .unwrap()
            .to_bytes()
            .unwrap()
    }

    fn signed_membership_record_bytes(
        signer: &ProfileSyncDeviceSigner,
        record: &ProfileSyncMembershipRecord,
    ) -> Vec<u8> {
        signer
            .sign(record.to_bytes().unwrap().as_slice())
            .unwrap()
            .to_bytes()
            .unwrap()
    }

    #[derive(Default)]
    struct InMemoryProfileSyncObjectSource {
        roots: BTreeMap<(String, String), String>,
        root_candidates: BTreeMap<(String, String), Vec<ProfileSyncRootCandidate>>,
        objects: BTreeMap<(String, String), ProfileSyncObjectBytes>,
    }

    impl InMemoryProfileSyncObjectSource {
        fn publish_root(&mut self, profile: &str, root_id: &str, object_id: &str) {
            self.roots.insert(
                (profile.to_string(), root_id.to_string()),
                object_id.to_string(),
            );
        }

        fn publish_root_candidate(
            &mut self,
            profile: &str,
            root_id: &str,
            publisher_id: &str,
            object_id: &str,
            publish_sequence: u64,
        ) {
            self.root_candidates
                .entry((profile.to_string(), root_id.to_string()))
                .or_default()
                .push(ProfileSyncRootCandidate::new(
                    publisher_id,
                    object_id,
                    publish_sequence,
                ));
        }

        fn insert_object(&mut self, profile: &str, object_id: &str, bytes: Vec<u8>) {
            self.objects.insert(
                (profile.to_string(), object_id.to_string()),
                ProfileSyncObjectBytes {
                    object_id: object_id.to_string(),
                    bytes,
                },
            );
        }
    }

    impl ProfileSyncObjectSource for InMemoryProfileSyncObjectSource {
        type Error = String;

        fn resolve_profile_sync_root(
            &self,
            profile: &str,
            root_id: &str,
        ) -> Result<Option<String>, Self::Error> {
            Ok(self
                .roots
                .get(&(profile.to_string(), root_id.to_string()))
                .cloned())
        }

        fn list_profile_sync_root_candidates(
            &self,
            profile: &str,
            root_id: &str,
        ) -> Result<Vec<ProfileSyncRootCandidate>, Self::Error> {
            if let Some(candidates) = self
                .root_candidates
                .get(&(profile.to_string(), root_id.to_string()))
            {
                return Ok(candidates.clone());
            }
            Ok(self
                .resolve_profile_sync_root(profile, root_id)?
                .map(ProfileSyncRootCandidate::resolved_root)
                .into_iter()
                .collect())
        }

        fn get_profile_sync_object(
            &self,
            profile: &str,
            object_id: &str,
        ) -> Result<ProfileSyncObjectBytes, Self::Error> {
            self.objects
                .get(&(profile.to_string(), object_id.to_string()))
                .cloned()
                .ok_or_else(|| format!("missing object {object_id}"))
        }
    }

    #[test]
    fn settings_manifest_builder_uses_tail_change_publications() {
        let first = SyncChangeRecord {
            id: 1,
            profile: DEFAULT_PROFILE_ID.to_string(),
            domain: SYNC_DOMAIN_SETTINGS.to_string(),
            entity_key: "ui.theme".to_string(),
            operation: "set_text".to_string(),
            payload: "teal".to_string(),
            device_id: "device-a".to_string(),
            device_sequence: 1,
            logical_clock: 1,
            created_at: 100,
            applied_at: Some(100),
        };
        let second = SyncChangeRecord {
            id: 2,
            profile: DEFAULT_PROFILE_ID.to_string(),
            domain: SYNC_DOMAIN_CALENDAR.to_string(),
            entity_key: "default_view".to_string(),
            operation: "set_text".to_string(),
            payload: "month".to_string(),
            device_id: "device-a".to_string(),
            device_sequence: 2,
            logical_clock: 2,
            created_at: 120,
            applied_at: Some(120),
        };
        let third = SyncChangeRecord {
            id: 3,
            profile: DEFAULT_PROFILE_ID.to_string(),
            domain: SYNC_DOMAIN_SETTINGS.to_string(),
            entity_key: "ui.zoom".to_string(),
            operation: "set_text".to_string(),
            payload: "110".to_string(),
            device_id: "device-b".to_string(),
            device_sequence: 1,
            logical_clock: 3,
            created_at: 110,
            applied_at: Some(110),
        };

        let manifest = settings_sync_manifest_for_tail_changes(
            DEFAULT_PROFILE_ID,
            "settings/latest",
            &[
                ProfileSyncSettingsTailChangePublication {
                    object_id: "change-object-1".to_string(),
                    change: first.clone(),
                },
                ProfileSyncSettingsTailChangePublication {
                    object_id: "change-object-2".to_string(),
                    change: second.clone(),
                },
                ProfileSyncSettingsTailChangePublication {
                    object_id: "change-object-3".to_string(),
                    change: third.clone(),
                },
            ],
            ProfileSyncRetentionPolicy::default(),
        )
        .unwrap();

        assert_eq!(manifest.profile, DEFAULT_PROFILE_ID);
        assert_eq!(manifest.root_id, "settings/latest");
        assert_eq!(
            manifest.tail_change_object_ids,
            vec!["change-object-1", "change-object-2", "change-object-3"]
        );
        assert_eq!(
            manifest.included_domains,
            vec![SYNC_DOMAIN_CALENDAR, SYNC_DOMAIN_SETTINGS]
        );
        assert_eq!(
            manifest.device_frontiers,
            vec![
                ProfileSyncDeviceFrontier {
                    device_id: "device-a".to_string(),
                    latest_sequence: 2,
                    latest_change_object_id: Some("change-object-2".to_string()),
                },
                ProfileSyncDeviceFrontier {
                    device_id: "device-b".to_string(),
                    latest_sequence: 1,
                    latest_change_object_id: Some("change-object-3".to_string()),
                },
            ]
        );
        assert_eq!(manifest.created_at, 120);

        assert!(matches!(
            settings_sync_manifest_for_tail_changes(
                DEFAULT_PROFILE_ID,
                "settings/latest",
                &[],
                ProfileSyncRetentionPolicy::default(),
            ),
            Err(StorageError::InvalidProfileSyncManifest(_))
        ));
        assert!(matches!(
            settings_sync_manifest_for_tail_changes(
                "other-profile",
                "settings/latest",
                &[ProfileSyncSettingsTailChangePublication {
                    object_id: "change-object-1".to_string(),
                    change: first,
                }],
                ProfileSyncRetentionPolicy::default(),
            ),
            Err(StorageError::InvalidProfileSyncManifest(_))
        ));
    }

    #[test]
    fn settings_manifest_builder_uses_snapshot_publication_and_tail_changes() {
        let compacted_settings_change = SyncChangeRecord {
            id: 1,
            profile: DEFAULT_PROFILE_ID.to_string(),
            domain: SYNC_DOMAIN_SETTINGS.to_string(),
            entity_key: "ui.theme".to_string(),
            operation: "set_text".to_string(),
            payload: "teal".to_string(),
            device_id: "device-a".to_string(),
            device_sequence: 1,
            logical_clock: 1,
            created_at: 100,
            applied_at: Some(100),
        };
        let compacted_calendar_change = SyncChangeRecord {
            id: 2,
            profile: DEFAULT_PROFILE_ID.to_string(),
            domain: SYNC_DOMAIN_CALENDAR.to_string(),
            entity_key: "default_view".to_string(),
            operation: "set_text".to_string(),
            payload: "month".to_string(),
            device_id: "device-b".to_string(),
            device_sequence: 3,
            logical_clock: 3,
            created_at: 105,
            applied_at: Some(105),
        };
        let tail_change = SyncChangeRecord {
            id: 3,
            profile: DEFAULT_PROFILE_ID.to_string(),
            domain: SYNC_DOMAIN_SETTINGS.to_string(),
            entity_key: "ui.theme".to_string(),
            operation: "set_text".to_string(),
            payload: "slate".to_string(),
            device_id: "device-a".to_string(),
            device_sequence: 2,
            logical_clock: 4,
            created_at: 130,
            applied_at: Some(130),
        };
        let snapshot = ProfileSyncSettingsSnapshot {
            profile: DEFAULT_PROFILE_ID.to_string(),
            schema_version: PROFILE_SYNC_SETTINGS_SNAPSHOT_SCHEMA_VERSION,
            covers_revision: 2,
            included_domains: vec![
                SYNC_DOMAIN_SETTINGS.to_string(),
                SYNC_DOMAIN_CALENDAR.to_string(),
            ],
            values: Vec::new(),
            created_at: 120,
        };
        let snapshot_publication = ProfileSyncSettingsSnapshotPublication {
            object_id: "snapshot-object-1".to_string(),
            snapshot: snapshot.clone(),
            covered_changes: vec![
                compacted_settings_change.clone(),
                compacted_calendar_change.clone(),
            ],
        };

        let snapshot_only = settings_sync_manifest_for_snapshot_and_tail_changes(
            DEFAULT_PROFILE_ID,
            "settings/latest",
            &snapshot_publication,
            &[],
            ProfileSyncRetentionPolicy::default(),
        )
        .unwrap();
        assert_eq!(
            snapshot_only.current_snapshot_object_id.as_deref(),
            Some("snapshot-object-1")
        );
        assert_eq!(snapshot_only.tail_change_object_ids, Vec::<String>::new());
        assert_eq!(snapshot_only.created_at, 120);
        assert_eq!(
            snapshot_only.device_frontiers,
            vec![
                ProfileSyncDeviceFrontier {
                    device_id: "device-a".to_string(),
                    latest_sequence: 1,
                    latest_change_object_id: None,
                },
                ProfileSyncDeviceFrontier {
                    device_id: "device-b".to_string(),
                    latest_sequence: 3,
                    latest_change_object_id: None,
                },
            ]
        );

        let snapshot_with_tail = settings_sync_manifest_for_snapshot_and_tail_changes(
            DEFAULT_PROFILE_ID,
            "settings/latest",
            &snapshot_publication,
            &[ProfileSyncSettingsTailChangePublication {
                object_id: "tail-change-object-1".to_string(),
                change: tail_change.clone(),
            }],
            ProfileSyncRetentionPolicy::default(),
        )
        .unwrap();
        assert_eq!(
            snapshot_with_tail.current_snapshot_object_id.as_deref(),
            Some("snapshot-object-1")
        );
        assert_eq!(
            snapshot_with_tail.tail_change_object_ids,
            vec!["tail-change-object-1"]
        );
        assert_eq!(
            snapshot_with_tail.included_domains,
            vec![SYNC_DOMAIN_CALENDAR, SYNC_DOMAIN_SETTINGS]
        );
        assert_eq!(snapshot_with_tail.created_at, 130);
        assert_eq!(
            snapshot_with_tail.device_frontiers,
            vec![
                ProfileSyncDeviceFrontier {
                    device_id: "device-a".to_string(),
                    latest_sequence: 2,
                    latest_change_object_id: Some("tail-change-object-1".to_string()),
                },
                ProfileSyncDeviceFrontier {
                    device_id: "device-b".to_string(),
                    latest_sequence: 3,
                    latest_change_object_id: None,
                },
            ]
        );

        let excluded_domain_snapshot = ProfileSyncSettingsSnapshot {
            included_domains: vec![SYNC_DOMAIN_SETTINGS.to_string()],
            ..snapshot
        };
        assert!(matches!(
            settings_sync_manifest_for_snapshot_and_tail_changes(
                DEFAULT_PROFILE_ID,
                "settings/latest",
                &ProfileSyncSettingsSnapshotPublication {
                    object_id: "snapshot-object-2".to_string(),
                    snapshot: excluded_domain_snapshot,
                    covered_changes: vec![compacted_calendar_change],
                },
                &[],
                ProfileSyncRetentionPolicy::default(),
            ),
            Err(StorageError::InvalidProfileSyncManifest(_))
        ));
    }

    #[test]
    fn database_resolution_prefers_explicit_path() {
        let launch_dir = test_dir("explicit-launch");
        let home_dir = test_dir("explicit-home");
        let explicit = launch_dir.join("custom.db");
        std::fs::write(launch_dir.join(DEFAULT_DATABASE_FILE_NAME), b"").unwrap();
        std::fs::create_dir_all(home_dir.join(DEFAULT_HOME_DIRECTORY_NAME)).unwrap();
        std::fs::write(
            home_dir
                .join(DEFAULT_HOME_DIRECTORY_NAME)
                .join(DEFAULT_DATABASE_FILE_NAME),
            b"",
        )
        .unwrap();

        let resolved = resolve_database_path(Some(&explicit), &launch_dir, Some(&home_dir));

        assert_eq!(resolved.path, explicit);
        assert_eq!(resolved.source, DatabasePathSource::Explicit);
    }

    #[test]
    fn database_resolution_uses_launch_directory_before_home_directory() {
        let launch_dir = test_dir("launch");
        let home_dir = test_dir("home");
        let launch_database = launch_dir.join(DEFAULT_DATABASE_FILE_NAME);
        std::fs::write(&launch_database, b"").unwrap();
        std::fs::create_dir_all(home_dir.join(DEFAULT_HOME_DIRECTORY_NAME)).unwrap();
        std::fs::write(
            home_dir
                .join(DEFAULT_HOME_DIRECTORY_NAME)
                .join(DEFAULT_DATABASE_FILE_NAME),
            b"",
        )
        .unwrap();

        let resolved = resolve_database_path(None, &launch_dir, Some(&home_dir));

        assert_eq!(resolved.path, launch_database);
        assert_eq!(resolved.source, DatabasePathSource::LaunchDirectoryExisting);
    }

    #[test]
    fn database_resolution_falls_back_to_existing_home_database() {
        let launch_dir = test_dir("home-fallback-launch");
        let home_dir = test_dir("home-fallback-home");
        let home_database = home_dir
            .join(DEFAULT_HOME_DIRECTORY_NAME)
            .join(DEFAULT_DATABASE_FILE_NAME);
        std::fs::create_dir_all(home_database.parent().unwrap()).unwrap();
        std::fs::write(&home_database, b"").unwrap();

        let resolved = resolve_database_path(None, &launch_dir, Some(&home_dir));

        assert_eq!(resolved.path, home_database);
        assert_eq!(resolved.source, DatabasePathSource::HomeDirectoryExisting);
    }

    #[test]
    fn database_resolution_creates_in_launch_directory_when_no_database_exists() {
        let launch_dir = test_dir("created-launch");
        let home_dir = test_dir("created-home");

        let resolved = resolve_database_path(None, &launch_dir, Some(&home_dir));

        assert_eq!(resolved.path, launch_dir.join(DEFAULT_DATABASE_FILE_NAME));
        assert_eq!(resolved.source, DatabasePathSource::LaunchDirectoryCreated);
    }

    #[test]
    fn slate_sync_secret_derives_separated_profile_content_keys() {
        let secret = SlateSyncSecret::from_bytes([42; SLATE_SYNC_SECRET_BYTES]);
        let same_key = secret
            .derive_profile_sync_content_key(DEFAULT_PROFILE_ID, "content-key-epoch-1")
            .unwrap();
        let repeated_key = secret
            .derive_profile_sync_content_key(DEFAULT_PROFILE_ID, "content-key-epoch-1")
            .unwrap();
        let other_epoch_key = secret
            .derive_profile_sync_content_key(DEFAULT_PROFILE_ID, "content-key-epoch-2")
            .unwrap();
        let other_profile_key = secret
            .derive_profile_sync_content_key("work", "content-key-epoch-1")
            .unwrap();

        assert_eq!(same_key, repeated_key);
        assert_ne!(same_key, other_epoch_key);
        assert_ne!(same_key, other_profile_key);
        let debug = format!("{secret:?}");
        assert!(debug.contains("<redacted>"));
        assert!(!debug.contains("42"));

        let object = EncryptedSyncObject::seal_with_nonce(
            DEFAULT_PROFILE_ID,
            SYNC_DOMAIN_SETTINGS,
            "setting-change",
            "content-key-epoch-1",
            br#"{"value":"teal"}"#,
            &same_key,
            [6; PROFILE_SYNC_NONCE_BYTES],
        )
        .unwrap();
        assert_eq!(
            object.open(&same_key).unwrap(),
            br#"{"value":"teal"}"#.to_vec()
        );
        assert!(matches!(
            object.open(&other_epoch_key),
            Err(SyncObjectError::Decrypt)
        ));
    }

    #[test]
    fn slate_sync_secret_export_round_trips_with_redacted_debug() {
        let secret = SlateSyncSecret::from_bytes([44; SLATE_SYNC_SECRET_BYTES]);
        let export = secret.export_for_profile(DEFAULT_PROFILE_ID, 1_234);

        assert_eq!(export.profile, DEFAULT_PROFILE_ID);
        assert_eq!(
            export.schema_version,
            SLATE_SYNC_SECRET_EXPORT_SCHEMA_VERSION
        );
        assert_eq!(export.created_at, 1_234);

        let imported = SlateSyncSecret::from_export(&export).unwrap();
        assert_eq!(
            imported
                .derive_profile_sync_content_key(DEFAULT_PROFILE_ID, "content-key-epoch-1")
                .unwrap(),
            secret
                .derive_profile_sync_content_key(DEFAULT_PROFILE_ID, "content-key-epoch-1")
                .unwrap()
        );

        let encoded = export.to_bytes().unwrap();
        let decoded = SlateSyncSecretExport::from_bytes(encoded.as_slice()).unwrap();
        assert_eq!(decoded, export);
        assert_eq!(
            SlateSyncSecret::from_export(&decoded)
                .unwrap()
                .derive_profile_sync_account_recovery_secret(DEFAULT_PROFILE_ID)
                .unwrap(),
            secret
                .derive_profile_sync_account_recovery_secret(DEFAULT_PROFILE_ID)
                .unwrap()
        );

        let debug = format!("{export:?}");
        assert!(debug.contains("<redacted>"));
        assert!(!debug.contains(export.secret.as_str()));
        assert!(!debug.contains("44"));

        let mut unsupported_schema = export.clone();
        unsupported_schema.schema_version = SLATE_SYNC_SECRET_EXPORT_SCHEMA_VERSION + 1;
        assert!(matches!(
            unsupported_schema.to_bytes(),
            Err(SyncObjectError::UnsupportedSchema {
                object_kind,
                schema_version
            }) if object_kind == SLATE_SYNC_SECRET_EXPORT_OBJECT_KIND
                && schema_version == SLATE_SYNC_SECRET_EXPORT_SCHEMA_VERSION + 1
        ));

        let mut malformed_secret = export.clone();
        malformed_secret.secret = "not-base64*".to_string();
        assert!(matches!(
            SlateSyncSecret::from_export(&malformed_secret),
            Err(SyncObjectError::Key)
        ));

        let mut short_secret = export.clone();
        short_secret.secret =
            URL_SAFE_NO_PAD.encode([1_u8; SLATE_SYNC_SECRET_BYTES - 1].as_slice());
        assert!(matches!(
            SlateSyncSecret::from_export(&short_secret),
            Err(SyncObjectError::Key)
        ));
    }

    #[test]
    fn slate_sync_secret_export_import_can_require_profile_match() {
        let secret = SlateSyncSecret::from_bytes([45; SLATE_SYNC_SECRET_BYTES]);
        let export = secret.export_for_profile(DEFAULT_PROFILE_ID, 1_235);

        let imported = SlateSyncSecret::from_export_for_profile(&export, DEFAULT_PROFILE_ID)
            .expect("matching profile export should import");
        assert_eq!(
            imported
                .derive_profile_sync_account_recovery_secret(DEFAULT_PROFILE_ID)
                .unwrap(),
            secret
                .derive_profile_sync_account_recovery_secret(DEFAULT_PROFILE_ID)
                .unwrap()
        );

        assert!(matches!(
            SlateSyncSecret::from_export_for_profile(&export, "work"),
            Err(SyncObjectError::UnexpectedProfile { expected, actual })
                if expected == "work" && actual == DEFAULT_PROFILE_ID
        ));
    }

    #[test]
    fn slate_sync_secret_derives_domain_separated_profile_material() {
        let secret = SlateSyncSecret::from_bytes([43; SLATE_SYNC_SECRET_BYTES]);
        let manifest_secret = secret
            .derive_profile_sync_manifest_signing_secret(DEFAULT_PROFILE_ID, "device-a", 1)
            .unwrap();
        let repeated_manifest_secret = secret
            .derive_profile_sync_manifest_signing_secret(DEFAULT_PROFILE_ID, "device-a", 1)
            .unwrap();
        let other_device_manifest_secret = secret
            .derive_profile_sync_manifest_signing_secret(DEFAULT_PROFILE_ID, "device-b", 1)
            .unwrap();
        let other_epoch_manifest_secret = secret
            .derive_profile_sync_manifest_signing_secret(DEFAULT_PROFILE_ID, "device-a", 2)
            .unwrap();
        let other_profile_manifest_secret = secret
            .derive_profile_sync_manifest_signing_secret("work", "device-a", 1)
            .unwrap();
        let mutable_root_secret = secret
            .derive_profile_sync_mutable_root_secret(DEFAULT_PROFILE_ID, "settings/latest", 1)
            .unwrap();
        let enrollment_secret = secret
            .derive_profile_sync_enrollment_secret(DEFAULT_PROFILE_ID, "device-a", 1)
            .unwrap();
        let bootstrap_secret = secret
            .derive_profile_sync_device_bootstrap_secret(DEFAULT_PROFILE_ID, "device-a", 1)
            .unwrap();
        let recovery_secret = secret
            .derive_profile_sync_account_recovery_secret(DEFAULT_PROFILE_ID)
            .unwrap();
        let content_key = secret
            .derive_profile_sync_content_key(DEFAULT_PROFILE_ID, "content-key-epoch-1")
            .unwrap();

        assert_eq!(manifest_secret, repeated_manifest_secret);
        assert_eq!(
            manifest_secret.purpose(),
            ProfileSyncDerivedSecretPurpose::ManifestSigning
        );
        assert_eq!(
            mutable_root_secret.purpose(),
            ProfileSyncDerivedSecretPurpose::MutableRootPublishing
        );
        assert_eq!(
            enrollment_secret.purpose(),
            ProfileSyncDerivedSecretPurpose::DeviceEnrollment
        );
        assert_eq!(
            bootstrap_secret.purpose(),
            ProfileSyncDerivedSecretPurpose::DeviceBootstrap
        );
        assert_eq!(
            recovery_secret.purpose(),
            ProfileSyncDerivedSecretPurpose::AccountRecovery
        );

        assert_ne!(manifest_secret, other_device_manifest_secret);
        assert_ne!(manifest_secret, other_epoch_manifest_secret);
        assert_ne!(manifest_secret, other_profile_manifest_secret);
        assert_ne!(manifest_secret.as_bytes(), mutable_root_secret.as_bytes());
        assert_ne!(manifest_secret.as_bytes(), enrollment_secret.as_bytes());
        assert_ne!(manifest_secret.as_bytes(), bootstrap_secret.as_bytes());
        assert_ne!(manifest_secret.as_bytes(), recovery_secret.as_bytes());
        assert_ne!(manifest_secret.as_bytes(), content_key.as_bytes());

        let debug = format!("{manifest_secret:?}");
        assert!(debug.contains("ManifestSigning"));
        assert!(debug.contains("<redacted>"));
        assert!(!debug.contains("43"));

        assert!(matches!(
            secret.derive_profile_sync_manifest_signing_secret(DEFAULT_PROFILE_ID, "../bad", 1),
            Err(SyncObjectError::InvalidDeviceId(device_id)) if device_id == "../bad"
        ));
        assert!(matches!(
            secret.derive_profile_sync_enrollment_secret(DEFAULT_PROFILE_ID, "", 1),
            Err(SyncObjectError::InvalidDeviceId(device_id)) if device_id.is_empty()
        ));
    }

    #[test]
    fn slate_sync_secret_derives_stable_profile_device_signers() {
        let secret = SlateSyncSecret::from_bytes([46; SLATE_SYNC_SECRET_BYTES]);
        let signer = secret
            .derive_profile_sync_device_signer(DEFAULT_PROFILE_ID, "device-a", 1)
            .unwrap();
        let repeated_signer = secret
            .derive_profile_sync_device_signer(DEFAULT_PROFILE_ID, "device-a", 1)
            .unwrap();
        let other_device_signer = secret
            .derive_profile_sync_device_signer(DEFAULT_PROFILE_ID, "device-b", 1)
            .unwrap();
        let other_epoch_signer = secret
            .derive_profile_sync_device_signer(DEFAULT_PROFILE_ID, "device-a", 2)
            .unwrap();

        let public_key = signer.public_key().unwrap();
        assert_eq!(signer.device_id(), "device-a");
        assert_eq!(public_key, repeated_signer.public_key().unwrap());
        assert_ne!(public_key, other_device_signer.public_key().unwrap());
        assert_ne!(public_key, other_epoch_signer.public_key().unwrap());

        let payload = b"local settings handoff preview";
        let signed = signer.sign(payload).unwrap();
        let repeated_signed = repeated_signer.sign(payload).unwrap();
        assert_eq!(signed, repeated_signed);
        assert_eq!(signed.verify_with(&public_key).unwrap(), payload);

        let debug = format!("{signer:?}");
        assert!(debug.contains("device-a"));
        assert!(debug.contains("<redacted>"));
        assert!(!debug.contains("46"));

        let recovery_secret = secret
            .derive_profile_sync_account_recovery_secret(DEFAULT_PROFILE_ID)
            .unwrap();
        assert!(matches!(
            ProfileSyncDeviceSigner::from_manifest_signing_secret("device-a", &recovery_secret),
            Err(SyncObjectError::UnexpectedDerivedSecretPurpose { expected, actual })
                if expected == ProfileSyncDerivedSecretPurpose::ManifestSigning
                    && actual == ProfileSyncDerivedSecretPurpose::AccountRecovery
        ));
    }

    #[test]
    fn encrypted_sync_objects_round_trip_and_reject_tampering() {
        let content_key = ProfileSyncContentKey::from_bytes([7; PROFILE_SYNC_CONTENT_KEY_BYTES]);
        let wrong_content_key =
            ProfileSyncContentKey::from_bytes([8; PROFILE_SYNC_CONTENT_KEY_BYTES]);
        let payload = serde_json::to_vec(&IncomingSyncSettingText::new(
            DEFAULT_PROFILE_ID,
            SYNC_DOMAIN_SETTINGS,
            "ui.theme",
            "teal",
            "device-a",
            1,
            1,
        ))
        .unwrap();
        let object = EncryptedSyncObject::seal_with_nonce(
            DEFAULT_PROFILE_ID,
            SYNC_DOMAIN_SETTINGS,
            "setting-change",
            "content-key-epoch-1",
            payload.as_slice(),
            &content_key,
            [3; PROFILE_SYNC_NONCE_BYTES],
        )
        .unwrap();

        assert_eq!(object.version, SYNC_OBJECT_VERSION);
        assert_ne!(object.ciphertext, payload);

        let encoded = object.to_bytes().unwrap();
        assert!(
            !std::str::from_utf8(encoded.as_slice())
                .unwrap()
                .contains("teal")
        );

        let decoded = EncryptedSyncObject::from_bytes(encoded.as_slice()).unwrap();
        let plaintext = decoded.open(&content_key).unwrap();
        assert_eq!(plaintext, payload);
        let change: IncomingSyncSettingText = serde_json::from_slice(plaintext.as_slice()).unwrap();
        assert_eq!(change.value, "teal");

        let mut tampered_ciphertext = decoded.clone();
        tampered_ciphertext.ciphertext[0] ^= 1;
        assert!(matches!(
            tampered_ciphertext.open(&content_key),
            Err(SyncObjectError::Decrypt)
        ));

        let mut tampered_metadata = decoded.clone();
        tampered_metadata.domain = SYNC_DOMAIN_CALENDAR.to_string();
        assert!(matches!(
            tampered_metadata.open(&content_key),
            Err(SyncObjectError::Decrypt)
        ));

        assert!(matches!(
            decoded.open(&wrong_content_key),
            Err(SyncObjectError::Decrypt)
        ));

        let mut invalid_nonce = decoded.clone();
        invalid_nonce.nonce.pop();
        assert!(matches!(
            invalid_nonce.open(&content_key),
            Err(SyncObjectError::InvalidNonceLength { actual })
                if actual == PROFILE_SYNC_NONCE_BYTES - 1
        ));

        let mut unsupported = decoded;
        unsupported.version = SYNC_OBJECT_VERSION + 1;
        assert!(matches!(
            unsupported.open(&content_key),
            Err(SyncObjectError::UnsupportedVersion(version))
                if version == SYNC_OBJECT_VERSION + 1
        ));
    }

    #[test]
    fn signed_sync_objects_verify_encrypted_payloads_against_trusted_device_key() {
        let content_key = ProfileSyncContentKey::from_bytes([9; PROFILE_SYNC_CONTENT_KEY_BYTES]);
        let signer = ProfileSyncDeviceSigner::generate("device-a").unwrap();
        let trusted_public_key = signer.public_key().unwrap();
        let payload = serde_json::to_vec(&IncomingSyncSettingText::new(
            DEFAULT_PROFILE_ID,
            SYNC_DOMAIN_SETTINGS,
            "ui.theme",
            "teal",
            "device-a",
            1,
            1,
        ))
        .unwrap();
        let encrypted_object = EncryptedSyncObject::seal_with_nonce(
            DEFAULT_PROFILE_ID,
            SYNC_DOMAIN_SETTINGS,
            PROFILE_SYNC_SETTING_CHANGE_OBJECT_KIND,
            "content-key-epoch-1",
            payload.as_slice(),
            &content_key,
            [4; PROFILE_SYNC_NONCE_BYTES],
        )
        .unwrap();
        let encrypted_bytes = encrypted_object.to_bytes().unwrap();
        let signed = signer.sign(encrypted_bytes.as_slice()).unwrap();

        let encoded = signed.to_bytes().unwrap();
        assert!(
            !std::str::from_utf8(encoded.as_slice())
                .unwrap()
                .contains("teal")
        );

        let decoded = SignedSyncObject::from_bytes(encoded.as_slice()).unwrap();
        let verified_payload = decoded.verify_with(&trusted_public_key).unwrap();
        assert_eq!(verified_payload, encrypted_bytes.as_slice());

        let encrypted_after_verify = EncryptedSyncObject::from_bytes(verified_payload).unwrap();
        assert_eq!(
            encrypted_after_verify.open(&content_key).unwrap(),
            payload.as_slice()
        );

        let mut tampered_payload = decoded.clone();
        tampered_payload.payload[0] ^= 1;
        assert!(matches!(
            tampered_payload.verify_with(&trusted_public_key),
            Err(SyncObjectError::Verify)
        ));

        let mut tampered_signature = decoded.clone();
        tampered_signature.signature[0] ^= 1;
        assert!(matches!(
            tampered_signature.verify_with(&trusted_public_key),
            Err(SyncObjectError::Verify)
        ));

        let wrong_device_public_key = ProfileSyncDeviceSigner::generate("device-b")
            .unwrap()
            .public_key()
            .unwrap();
        assert!(matches!(
            decoded.verify_with(&wrong_device_public_key),
            Err(SyncObjectError::DeviceKeyMismatch {
                expected_device_id,
                actual_device_id
            }) if expected_device_id == "device-b" && actual_device_id == "device-a"
        ));

        let wrong_key_same_device = ProfileSyncDeviceSigner::generate("device-a")
            .unwrap()
            .public_key()
            .unwrap();
        assert!(matches!(
            decoded.verify_with(&wrong_key_same_device),
            Err(SyncObjectError::DeviceKeyMismatch {
                expected_device_id,
                actual_device_id
            }) if expected_device_id == "device-a" && actual_device_id == "device-a"
        ));

        let mut unsupported = decoded;
        unsupported.version = SYNC_OBJECT_VERSION + 1;
        assert!(matches!(
            unsupported.verify_with(&trusted_public_key),
            Err(SyncObjectError::UnsupportedVersion(version))
                if version == SYNC_OBJECT_VERSION + 1
        ));

        assert!(matches!(
            ProfileSyncDeviceSigner::generate("../device-a"),
            Err(SyncObjectError::InvalidDeviceId(device_id)) if device_id == "../device-a"
        ));
    }

    #[test]
    fn profile_sync_manifest_can_be_signed_and_encrypted() {
        let content_key = ProfileSyncContentKey::from_bytes([10; PROFILE_SYNC_CONTENT_KEY_BYTES]);
        let signer = ProfileSyncDeviceSigner::generate("device-a").unwrap();
        let trusted_public_key = signer.public_key().unwrap();
        let manifest = ProfileSyncManifest {
            profile: DEFAULT_PROFILE_ID.to_string(),
            root_id: "settings/latest".to_string(),
            schema_version: PROFILE_SYNC_MANIFEST_SCHEMA_VERSION,
            membership_epoch: DEFAULT_PROFILE_SYNC_MEMBERSHIP_EPOCH,
            current_snapshot_object_id: Some("snapshot-object-1".to_string()),
            tail_change_object_ids: vec!["change-object-2".to_string()],
            included_domains: vec![SYNC_DOMAIN_SETTINGS.to_string()],
            device_frontiers: vec![ProfileSyncDeviceFrontier {
                device_id: "device-a".to_string(),
                latest_sequence: 7,
                latest_change_object_id: Some("change-object-2".to_string()),
            }],
            retention_policy: ProfileSyncRetentionPolicy::default(),
            created_at: 1234,
        };
        let manifest_payload = serde_json::to_vec(&manifest).unwrap();
        let encrypted_manifest = EncryptedSyncObject::seal_with_nonce(
            DEFAULT_PROFILE_ID,
            SYNC_DOMAIN_SETTINGS,
            PROFILE_SYNC_MANIFEST_OBJECT_KIND,
            "content-key-epoch-1",
            manifest_payload.as_slice(),
            &content_key,
            [5; PROFILE_SYNC_NONCE_BYTES],
        )
        .unwrap();
        let signed_manifest = signer
            .sign(encrypted_manifest.to_bytes().unwrap().as_slice())
            .unwrap();

        let signed_bytes = signed_manifest.to_bytes().unwrap();
        assert!(
            !std::str::from_utf8(signed_bytes.as_slice())
                .unwrap()
                .contains("change-object-2")
        );

        let decoded = SignedSyncObject::from_bytes(signed_bytes.as_slice()).unwrap();
        let encrypted_bytes = decoded.verify_with(&trusted_public_key).unwrap();
        let decoded_encrypted_manifest = EncryptedSyncObject::from_bytes(encrypted_bytes).unwrap();
        assert_eq!(
            decoded_encrypted_manifest.object_kind,
            PROFILE_SYNC_MANIFEST_OBJECT_KIND
        );

        let decoded_payload = decoded_encrypted_manifest.open(&content_key).unwrap();
        let decoded_manifest: ProfileSyncManifest =
            serde_json::from_slice(decoded_payload.as_slice()).unwrap();
        assert_eq!(decoded_manifest, manifest);

        assert_eq!(
            open_signed_profile_sync_manifest(
                signed_bytes.as_slice(),
                &content_key,
                &trusted_public_key,
                DEFAULT_PROFILE_ID,
                "content-key-epoch-1",
            )
            .unwrap(),
            manifest
        );
        assert!(matches!(
            open_signed_profile_sync_settings_snapshot(
                signed_bytes.as_slice(),
                &content_key,
                &trusted_public_key,
                DEFAULT_PROFILE_ID,
                "content-key-epoch-1",
            ),
            Err(SyncObjectError::UnexpectedObjectKind { expected, actual })
                if expected == PROFILE_SYNC_SETTINGS_SNAPSHOT_OBJECT_KIND
                    && actual == PROFILE_SYNC_MANIFEST_OBJECT_KIND
        ));
        assert!(matches!(
            open_signed_profile_sync_manifest(
                signed_bytes.as_slice(),
                &content_key,
                &trusted_public_key,
                DEFAULT_PROFILE_ID,
                "content-key-epoch-2",
            ),
            Err(SyncObjectError::UnexpectedKeyId { expected, actual })
                if expected == "content-key-epoch-2" && actual == "content-key-epoch-1"
        ));
    }

    #[test]
    fn profile_sync_device_head_can_be_signed_and_encrypted() {
        let content_key = ProfileSyncContentKey::from_bytes([24; PROFILE_SYNC_CONTENT_KEY_BYTES]);
        let signer = ProfileSyncDeviceSigner::generate("device-a").unwrap();
        let trusted_public_key = signer.public_key().unwrap();
        let device_head = ProfileSyncDeviceHead {
            profile: DEFAULT_PROFILE_ID.to_string(),
            device_id: "device-a".to_string(),
            root_id: "settings/devices/device-a/head".to_string(),
            schema_version: PROFILE_SYNC_DEVICE_HEAD_SCHEMA_VERSION,
            membership_epoch: DEFAULT_PROFILE_SYNC_MEMBERSHIP_EPOCH,
            latest_manifest_object_id: "manifest-object-3".to_string(),
            latest_change_object_id: Some("change-object-7".to_string()),
            device_sequence: 7,
            logical_clock: 11,
            created_at: 1234,
        };
        let signed_bytes = sign_test_sync_object(
            DEFAULT_PROFILE_ID,
            SYNC_DOMAIN_SETTINGS,
            PROFILE_SYNC_DEVICE_HEAD_OBJECT_KIND,
            "content-key-epoch-1",
            serde_json::to_vec(&device_head).unwrap().as_slice(),
            &content_key,
            &signer,
            24,
        );

        assert!(
            !std::str::from_utf8(signed_bytes.as_slice())
                .unwrap()
                .contains("manifest-object-3")
        );
        assert_eq!(
            open_signed_profile_sync_device_head(
                signed_bytes.as_slice(),
                &content_key,
                &trusted_public_key,
                DEFAULT_PROFILE_ID,
                "content-key-epoch-1",
            )
            .unwrap(),
            device_head
        );
        assert!(matches!(
            open_signed_profile_sync_manifest(
                signed_bytes.as_slice(),
                &content_key,
                &trusted_public_key,
                DEFAULT_PROFILE_ID,
                "content-key-epoch-1",
            ),
            Err(SyncObjectError::UnexpectedObjectKind { expected, actual })
                if expected == PROFILE_SYNC_MANIFEST_OBJECT_KIND
                    && actual == PROFILE_SYNC_DEVICE_HEAD_OBJECT_KIND
        ));

        let mismatched_payload = ProfileSyncDeviceHead {
            device_id: "device-b".to_string(),
            ..device_head
        };
        let mismatched_signed_bytes = sign_test_sync_object(
            DEFAULT_PROFILE_ID,
            SYNC_DOMAIN_SETTINGS,
            PROFILE_SYNC_DEVICE_HEAD_OBJECT_KIND,
            "content-key-epoch-1",
            serde_json::to_vec(&mismatched_payload).unwrap().as_slice(),
            &content_key,
            &signer,
            25,
        );
        assert!(matches!(
            open_signed_profile_sync_device_head(
                mismatched_signed_bytes.as_slice(),
                &content_key,
                &trusted_public_key,
                DEFAULT_PROFILE_ID,
                "content-key-epoch-1",
            ),
            Err(SyncObjectError::DeviceKeyMismatch {
                expected_device_id,
                actual_device_id
            }) if expected_device_id == "device-a" && actual_device_id == "device-b"
        ));

        let unsupported_schema_payload = ProfileSyncDeviceHead {
            schema_version: PROFILE_SYNC_DEVICE_HEAD_SCHEMA_VERSION + 1,
            ..mismatched_payload
        };
        let unsupported_schema_signed_bytes = sign_test_sync_object(
            DEFAULT_PROFILE_ID,
            SYNC_DOMAIN_SETTINGS,
            PROFILE_SYNC_DEVICE_HEAD_OBJECT_KIND,
            "content-key-epoch-1",
            serde_json::to_vec(&unsupported_schema_payload)
                .unwrap()
                .as_slice(),
            &content_key,
            &signer,
            26,
        );
        assert!(matches!(
            open_signed_profile_sync_device_head(
                unsupported_schema_signed_bytes.as_slice(),
                &content_key,
                &trusted_public_key,
                DEFAULT_PROFILE_ID,
                "content-key-epoch-1",
            ),
            Err(SyncObjectError::UnsupportedSchema {
                object_kind,
                schema_version
            }) if object_kind == PROFILE_SYNC_DEVICE_HEAD_OBJECT_KIND
                && schema_version == PROFILE_SYNC_DEVICE_HEAD_SCHEMA_VERSION + 1
        ));
    }

    #[test]
    fn profile_sync_trusted_device_head_uses_stored_signer_membership_epoch() {
        let content_key = ProfileSyncContentKey::from_bytes([25; PROFILE_SYNC_CONTENT_KEY_BYTES]);
        let signer = ProfileSyncDeviceSigner::generate("device-a").unwrap();
        let trusted_public_key = signer.public_key().unwrap();
        let key_id = "content-key-epoch-1";
        let device_head = ProfileSyncDeviceHead {
            profile: DEFAULT_PROFILE_ID.to_string(),
            device_id: "device-a".to_string(),
            root_id: "settings/devices/device-a/head".to_string(),
            schema_version: PROFILE_SYNC_DEVICE_HEAD_SCHEMA_VERSION,
            membership_epoch: DEFAULT_PROFILE_SYNC_MEMBERSHIP_EPOCH,
            latest_manifest_object_id: "manifest-object-3".to_string(),
            latest_change_object_id: None,
            device_sequence: 1,
            logical_clock: 1,
            created_at: 1234,
        };
        let signed_bytes = sign_test_sync_object(
            DEFAULT_PROFILE_ID,
            SYNC_DOMAIN_SETTINGS,
            PROFILE_SYNC_DEVICE_HEAD_OBJECT_KIND,
            key_id,
            serde_json::to_vec(&device_head).unwrap().as_slice(),
            &content_key,
            &signer,
            26,
        );
        let trusted_path = test_dir("sync-trusted-device-head").join(DEFAULT_DATABASE_FILE_NAME);
        let trusted_database =
            SlateProfileDatabase::open_resolved_with_device_id(trusted_path, "device-b").unwrap();
        trusted_database
            .register_sync_device_public_key(&SyncDevicePublicKeyRegistration {
                profile: DEFAULT_PROFILE_ID.to_string(),
                public_key: trusted_public_key.clone(),
                membership_epoch: DEFAULT_PROFILE_SYNC_MEMBERSHIP_EPOCH,
            })
            .unwrap();

        assert_eq!(
            trusted_database
                .open_trusted_signed_profile_sync_device_head(
                    signed_bytes.as_slice(),
                    &content_key,
                    DEFAULT_PROFILE_ID,
                    key_id,
                )
                .unwrap(),
            device_head
        );
        trusted_database
            .set_sync_device_public_key_trusted(DEFAULT_PROFILE_ID, "device-a", false)
            .unwrap()
            .expect("revoked trusted device key");
        assert!(matches!(
            trusted_database.open_trusted_signed_profile_sync_device_head(
                signed_bytes.as_slice(),
                &content_key,
                DEFAULT_PROFILE_ID,
                key_id,
            ),
            Err(ProfileSyncTrustedOpenError::UntrustedDevice { profile, device_id })
                if profile == DEFAULT_PROFILE_ID && device_id == "device-a"
        ));

        let late_path = test_dir("sync-trusted-device-head-late").join(DEFAULT_DATABASE_FILE_NAME);
        let late_database =
            SlateProfileDatabase::open_resolved_with_device_id(late_path, "device-b").unwrap();
        late_database
            .register_sync_device_public_key(&SyncDevicePublicKeyRegistration {
                profile: DEFAULT_PROFILE_ID.to_string(),
                public_key: trusted_public_key,
                membership_epoch: DEFAULT_PROFILE_SYNC_MEMBERSHIP_EPOCH + 1,
            })
            .unwrap();
        assert!(matches!(
            late_database.open_trusted_signed_profile_sync_device_head(
                signed_bytes.as_slice(),
                &content_key,
                DEFAULT_PROFILE_ID,
                key_id,
            ),
            Err(ProfileSyncTrustedOpenError::UnauthorizedDeviceEpoch {
                profile,
                device_id,
                key_membership_epoch,
                manifest_membership_epoch,
            }) if profile == DEFAULT_PROFILE_ID
                && device_id == "device-a"
                && key_membership_epoch == DEFAULT_PROFILE_SYNC_MEMBERSHIP_EPOCH + 1
                && manifest_membership_epoch == DEFAULT_PROFILE_SYNC_MEMBERSHIP_EPOCH
        ));
    }

    #[test]
    fn profile_sync_provider_authority_signers_cannot_authorize_profile_state() {
        let content_key = ProfileSyncContentKey::from_bytes([25; PROFILE_SYNC_CONTENT_KEY_BYTES]);
        let provider_signer = ProfileSyncDeviceSigner::generate("provider-device").unwrap();
        let provider_public_key = provider_signer.public_key().unwrap();
        let key_id = "content-key-epoch-1";
        let root_id = "settings/devices/provider-device/head";
        let device_head_object_id = "provider-device-head-object-1";
        let device_head = ProfileSyncDeviceHead {
            profile: DEFAULT_PROFILE_ID.to_string(),
            device_id: "provider-device".to_string(),
            root_id: root_id.to_string(),
            schema_version: PROFILE_SYNC_DEVICE_HEAD_SCHEMA_VERSION,
            membership_epoch: DEFAULT_PROFILE_SYNC_MEMBERSHIP_EPOCH,
            latest_manifest_object_id: "provider-manifest-object-1".to_string(),
            latest_change_object_id: None,
            device_sequence: 1,
            logical_clock: 1,
            created_at: 1234,
        };
        let signed_device_head = sign_test_sync_object(
            DEFAULT_PROFILE_ID,
            SYNC_DOMAIN_SETTINGS,
            PROFILE_SYNC_DEVICE_HEAD_OBJECT_KIND,
            key_id,
            serde_json::to_vec(&device_head).unwrap().as_slice(),
            &content_key,
            &provider_signer,
            26,
        );
        let database_path =
            test_dir("sync-provider-authority-profile-state").join(DEFAULT_DATABASE_FILE_NAME);
        let database =
            SlateProfileDatabase::open_resolved_with_device_id(database_path, "device-b").unwrap();
        database
            .register_sync_device_public_key(&SyncDevicePublicKeyRegistration {
                profile: DEFAULT_PROFILE_ID.to_string(),
                public_key: provider_public_key,
                membership_epoch: DEFAULT_PROFILE_SYNC_MEMBERSHIP_EPOCH,
            })
            .unwrap();
        database
            .register_sync_device(&SyncDeviceRegistration {
                profile: DEFAULT_PROFILE_ID.to_string(),
                device_id: "provider-device".to_string(),
                label: Some("Availability Provider".to_string()),
                membership_epoch: DEFAULT_PROFILE_SYNC_MEMBERSHIP_EPOCH,
                provider_authority: true,
            })
            .unwrap();

        assert!(matches!(
            database.open_trusted_signed_profile_sync_device_head(
                signed_device_head.as_slice(),
                &content_key,
                DEFAULT_PROFILE_ID,
                key_id,
            ),
            Err(ProfileSyncTrustedOpenError::ProviderAuthoritySigner { profile, device_id })
                if profile == DEFAULT_PROFILE_ID && device_id == "provider-device"
        ));

        let mut source = InMemoryProfileSyncObjectSource::default();
        source.insert_object(
            DEFAULT_PROFILE_ID,
            device_head_object_id,
            signed_device_head.clone(),
        );
        source.publish_root(DEFAULT_PROFILE_ID, root_id, device_head_object_id);
        assert!(matches!(
            database.pull_and_record_trusted_signed_profile_sync_device_head_if_changed(
                &source,
                DEFAULT_PROFILE_ID,
                root_id,
                &content_key,
                key_id,
            ),
            Err(ProfileSyncTrustedPullApplyError::Pull(ProfileSyncTrustedPullError::Open(
                ProfileSyncTrustedOpenError::ProviderAuthoritySigner { profile, device_id }
            ))) if profile == DEFAULT_PROFILE_ID && device_id == "provider-device"
        ));
        assert!(
            database
                .profile_sync_root(DEFAULT_PROFILE_ID, root_id)
                .unwrap()
                .is_none()
        );

        let manifest = ProfileSyncManifest {
            profile: DEFAULT_PROFILE_ID.to_string(),
            root_id: "settings/root".to_string(),
            schema_version: PROFILE_SYNC_MANIFEST_SCHEMA_VERSION,
            membership_epoch: DEFAULT_PROFILE_SYNC_MEMBERSHIP_EPOCH,
            current_snapshot_object_id: None,
            tail_change_object_ids: Vec::new(),
            included_domains: vec![SYNC_DOMAIN_SETTINGS.to_string()],
            device_frontiers: vec![ProfileSyncDeviceFrontier {
                device_id: "provider-device".to_string(),
                latest_sequence: 1,
                latest_change_object_id: None,
            }],
            retention_policy: ProfileSyncRetentionPolicy::default(),
            created_at: 1234,
        };
        let signed_manifest = sign_test_sync_object(
            DEFAULT_PROFILE_ID,
            SYNC_DOMAIN_SETTINGS,
            PROFILE_SYNC_MANIFEST_OBJECT_KIND,
            key_id,
            serde_json::to_vec(&manifest).unwrap().as_slice(),
            &content_key,
            &provider_signer,
            27,
        );
        assert!(matches!(
            database.open_trusted_signed_profile_sync_manifest(
                signed_manifest.as_slice(),
                &content_key,
                DEFAULT_PROFILE_ID,
                key_id,
            ),
            Err(ProfileSyncTrustedOpenError::ProviderAuthoritySigner { profile, device_id })
                if profile == DEFAULT_PROFILE_ID && device_id == "provider-device"
        ));
        assert!(
            database
                .sync_device_public_key(DEFAULT_PROFILE_ID, "provider-device")
                .unwrap()
                .expect("provider key remains trusted")
                .trusted
        );
        assert!(
            database
                .sync_devices(DEFAULT_PROFILE_ID)
                .unwrap()
                .into_iter()
                .find(|device| device.device_id == "provider-device")
                .expect("provider roster entry remains")
                .provider_authority
        );
    }

    #[test]
    fn profile_sync_pull_fetches_device_head_object() {
        let content_key = ProfileSyncContentKey::from_bytes([26; PROFILE_SYNC_CONTENT_KEY_BYTES]);
        let signer = ProfileSyncDeviceSigner::generate("device-a").unwrap();
        let trusted_public_key = signer.public_key().unwrap();
        let key_id = "content-key-epoch-1";
        let root_id = "settings/devices/device-a/head";
        let object_id = "device-head-object-1";
        let device_head = ProfileSyncDeviceHead {
            profile: DEFAULT_PROFILE_ID.to_string(),
            device_id: "device-a".to_string(),
            root_id: root_id.to_string(),
            schema_version: PROFILE_SYNC_DEVICE_HEAD_SCHEMA_VERSION,
            membership_epoch: DEFAULT_PROFILE_SYNC_MEMBERSHIP_EPOCH,
            latest_manifest_object_id: "manifest-object-3".to_string(),
            latest_change_object_id: Some("change-object-7".to_string()),
            device_sequence: 7,
            logical_clock: 11,
            created_at: 1234,
        };
        let mut source = InMemoryProfileSyncObjectSource::default();
        source.insert_object(
            DEFAULT_PROFILE_ID,
            object_id,
            sign_test_sync_object(
                DEFAULT_PROFILE_ID,
                SYNC_DOMAIN_SETTINGS,
                PROFILE_SYNC_DEVICE_HEAD_OBJECT_KIND,
                key_id,
                serde_json::to_vec(&device_head).unwrap().as_slice(),
                &content_key,
                &signer,
                27,
            ),
        );
        source.publish_root(DEFAULT_PROFILE_ID, root_id, object_id);

        let pulled = pull_signed_profile_sync_device_head(
            &source,
            DEFAULT_PROFILE_ID,
            root_id,
            &content_key,
            &trusted_public_key,
            key_id,
        )
        .unwrap()
        .expect("published device head root");
        assert_eq!(
            pulled,
            VerifiedProfileSyncDeviceHead {
                object_id: object_id.to_string(),
                device_head: device_head.clone(),
            }
        );
        assert_eq!(
            pull_signed_profile_sync_device_head(
                &source,
                DEFAULT_PROFILE_ID,
                "settings/devices/device-b/head",
                &content_key,
                &trusted_public_key,
                key_id,
            )
            .unwrap(),
            None
        );

        let mismatched_root_id = "settings/devices/device-a/other-head";
        let mismatched_object_id = "device-head-object-2";
        let mismatched_device_head = ProfileSyncDeviceHead {
            root_id: "settings/devices/device-a/payload-head".to_string(),
            ..device_head.clone()
        };
        source.insert_object(
            DEFAULT_PROFILE_ID,
            mismatched_object_id,
            sign_test_sync_object(
                DEFAULT_PROFILE_ID,
                SYNC_DOMAIN_SETTINGS,
                PROFILE_SYNC_DEVICE_HEAD_OBJECT_KIND,
                key_id,
                serde_json::to_vec(&mismatched_device_head)
                    .unwrap()
                    .as_slice(),
                &content_key,
                &signer,
                29,
            ),
        );
        source.publish_root(DEFAULT_PROFILE_ID, mismatched_root_id, mismatched_object_id);
        assert!(matches!(
            pull_signed_profile_sync_device_head(
                &source,
                DEFAULT_PROFILE_ID,
                mismatched_root_id,
                &content_key,
                &trusted_public_key,
                key_id,
            ),
            Err(ProfileSyncPullError::SyncObject(
                SyncObjectError::UnexpectedRootId { expected, actual }
            )) if expected == mismatched_root_id
                && actual == "settings/devices/device-a/payload-head"
        ));
    }

    #[test]
    fn profile_sync_trusted_pull_fetches_device_head_with_stored_key() {
        let content_key = ProfileSyncContentKey::from_bytes([27; PROFILE_SYNC_CONTENT_KEY_BYTES]);
        let signer = ProfileSyncDeviceSigner::generate("device-a").unwrap();
        let trusted_public_key = signer.public_key().unwrap();
        let key_id = "content-key-epoch-1";
        let root_id = "settings/devices/device-a/head";
        let object_id = "device-head-object-1";
        let device_head = ProfileSyncDeviceHead {
            profile: DEFAULT_PROFILE_ID.to_string(),
            device_id: "device-a".to_string(),
            root_id: root_id.to_string(),
            schema_version: PROFILE_SYNC_DEVICE_HEAD_SCHEMA_VERSION,
            membership_epoch: DEFAULT_PROFILE_SYNC_MEMBERSHIP_EPOCH,
            latest_manifest_object_id: "manifest-object-3".to_string(),
            latest_change_object_id: None,
            device_sequence: 1,
            logical_clock: 1,
            created_at: 1234,
        };
        let mut source = InMemoryProfileSyncObjectSource::default();
        source.insert_object(
            DEFAULT_PROFILE_ID,
            object_id,
            sign_test_sync_object(
                DEFAULT_PROFILE_ID,
                SYNC_DOMAIN_SETTINGS,
                PROFILE_SYNC_DEVICE_HEAD_OBJECT_KIND,
                key_id,
                serde_json::to_vec(&device_head).unwrap().as_slice(),
                &content_key,
                &signer,
                28,
            ),
        );
        source.publish_root(DEFAULT_PROFILE_ID, root_id, object_id);
        let destination_path =
            test_dir("sync-trusted-pull-device-head").join(DEFAULT_DATABASE_FILE_NAME);
        let destination =
            SlateProfileDatabase::open_resolved_with_device_id(destination_path, "device-b")
                .unwrap();
        destination
            .register_sync_device_public_key(&SyncDevicePublicKeyRegistration {
                profile: DEFAULT_PROFILE_ID.to_string(),
                public_key: trusted_public_key,
                membership_epoch: DEFAULT_PROFILE_SYNC_MEMBERSHIP_EPOCH,
            })
            .unwrap();

        let pulled = destination
            .pull_trusted_signed_profile_sync_device_head(
                &source,
                DEFAULT_PROFILE_ID,
                root_id,
                &content_key,
                key_id,
            )
            .unwrap()
            .expect("published trusted device head root");
        assert_eq!(
            pulled,
            VerifiedProfileSyncDeviceHead {
                object_id: object_id.to_string(),
                device_head: device_head.clone(),
            }
        );
        assert_eq!(
            destination
                .pull_trusted_signed_profile_sync_device_head(
                    &source,
                    DEFAULT_PROFILE_ID,
                    "settings/devices/device-b/head",
                    &content_key,
                    key_id,
                )
                .unwrap(),
            None
        );

        let mismatched_root_id = "settings/devices/device-a/other-head";
        let mismatched_object_id = "device-head-object-2";
        let mismatched_device_head = ProfileSyncDeviceHead {
            root_id: "settings/devices/device-a/payload-head".to_string(),
            ..device_head.clone()
        };
        source.insert_object(
            DEFAULT_PROFILE_ID,
            mismatched_object_id,
            sign_test_sync_object(
                DEFAULT_PROFILE_ID,
                SYNC_DOMAIN_SETTINGS,
                PROFILE_SYNC_DEVICE_HEAD_OBJECT_KIND,
                key_id,
                serde_json::to_vec(&mismatched_device_head)
                    .unwrap()
                    .as_slice(),
                &content_key,
                &signer,
                30,
            ),
        );
        source.publish_root(DEFAULT_PROFILE_ID, mismatched_root_id, mismatched_object_id);
        assert!(matches!(
            destination.pull_trusted_signed_profile_sync_device_head(
                &source,
                DEFAULT_PROFILE_ID,
                mismatched_root_id,
                &content_key,
                key_id,
            ),
            Err(ProfileSyncTrustedPullError::Open(
                ProfileSyncTrustedOpenError::SyncObject(
                    SyncObjectError::UnexpectedRootId { expected, actual }
                )
            )) if expected == mismatched_root_id
                && actual == "settings/devices/device-a/payload-head"
        ));
    }

    #[test]
    fn profile_sync_trusted_device_head_record_reports_missing_and_unchanged_roots() {
        let content_key = ProfileSyncContentKey::from_bytes([28; PROFILE_SYNC_CONTENT_KEY_BYTES]);
        let root_id = "settings/devices/device-a/head";
        let object_id = "device-head-object-1";
        let mut source = InMemoryProfileSyncObjectSource::default();
        let destination_path =
            test_dir("sync-trusted-record-device-head-empty").join(DEFAULT_DATABASE_FILE_NAME);
        let destination =
            SlateProfileDatabase::open_resolved_with_device_id(destination_path, "device-b")
                .unwrap();

        let missing = destination
            .pull_and_record_trusted_signed_profile_sync_device_head_if_changed(
                &source,
                DEFAULT_PROFILE_ID,
                root_id,
                &content_key,
                "content-key-epoch-1",
            )
            .unwrap();
        assert_eq!(
            missing,
            ProfileSyncDeviceHeadPullRecordStatus::NoPublishedRoot {
                profile: DEFAULT_PROFILE_ID.to_string(),
                root_id: root_id.to_string(),
            }
        );

        source.publish_root(DEFAULT_PROFILE_ID, root_id, object_id);
        destination
            .set_profile_sync_root(DEFAULT_PROFILE_ID, root_id, object_id)
            .unwrap();
        let unchanged = destination
            .pull_and_record_trusted_signed_profile_sync_device_head_if_changed(
                &source,
                DEFAULT_PROFILE_ID,
                root_id,
                &content_key,
                "content-key-epoch-1",
            )
            .unwrap();
        assert_eq!(
            unchanged,
            ProfileSyncDeviceHeadPullRecordStatus::Unchanged {
                profile: DEFAULT_PROFILE_ID.to_string(),
                root_id: root_id.to_string(),
                object_id: object_id.to_string(),
            }
        );
    }

    #[test]
    fn profile_sync_trusted_device_head_record_updates_verified_root() {
        let content_key = ProfileSyncContentKey::from_bytes([29; PROFILE_SYNC_CONTENT_KEY_BYTES]);
        let signer = ProfileSyncDeviceSigner::generate("device-a").unwrap();
        let trusted_public_key = signer.public_key().unwrap();
        let key_id = "content-key-epoch-1";
        let root_id = "settings/devices/device-a/head";
        let object_id = "device-head-object-1";
        let device_head = ProfileSyncDeviceHead {
            profile: DEFAULT_PROFILE_ID.to_string(),
            device_id: "device-a".to_string(),
            root_id: root_id.to_string(),
            schema_version: PROFILE_SYNC_DEVICE_HEAD_SCHEMA_VERSION,
            membership_epoch: DEFAULT_PROFILE_SYNC_MEMBERSHIP_EPOCH,
            latest_manifest_object_id: "manifest-object-3".to_string(),
            latest_change_object_id: None,
            device_sequence: 1,
            logical_clock: 1,
            created_at: 1234,
        };
        let mut source = InMemoryProfileSyncObjectSource::default();
        source.insert_object(
            DEFAULT_PROFILE_ID,
            object_id,
            sign_test_sync_object(
                DEFAULT_PROFILE_ID,
                SYNC_DOMAIN_SETTINGS,
                PROFILE_SYNC_DEVICE_HEAD_OBJECT_KIND,
                key_id,
                serde_json::to_vec(&device_head).unwrap().as_slice(),
                &content_key,
                &signer,
                31,
            ),
        );
        source.publish_root(DEFAULT_PROFILE_ID, root_id, object_id);
        let destination_path =
            test_dir("sync-trusted-record-device-head").join(DEFAULT_DATABASE_FILE_NAME);
        let destination =
            SlateProfileDatabase::open_resolved_with_device_id(destination_path, "device-b")
                .unwrap();
        destination
            .register_sync_device_public_key(&SyncDevicePublicKeyRegistration {
                profile: DEFAULT_PROFILE_ID.to_string(),
                public_key: trusted_public_key,
                membership_epoch: DEFAULT_PROFILE_SYNC_MEMBERSHIP_EPOCH,
            })
            .unwrap();

        let updated = destination
            .pull_and_record_trusted_signed_profile_sync_device_head_if_changed(
                &source,
                DEFAULT_PROFILE_ID,
                root_id,
                &content_key,
                key_id,
            )
            .unwrap();
        let ProfileSyncDeviceHeadPullRecordStatus::Updated {
            device_head: updated_head,
            root,
        } = updated
        else {
            panic!("expected trusted device head update, got {updated:?}");
        };
        assert_eq!(
            updated_head,
            VerifiedProfileSyncDeviceHead {
                object_id: object_id.to_string(),
                device_head,
            }
        );
        assert_eq!(root.profile, DEFAULT_PROFILE_ID);
        assert_eq!(root.root_id, root_id);
        assert_eq!(root.object_id, object_id);
        assert_eq!(
            destination
                .profile_sync_root(DEFAULT_PROFILE_ID, root_id)
                .unwrap()
                .expect("stored verified device head root")
                .object_id,
            object_id
        );

        let unchanged = destination
            .pull_and_record_trusted_signed_profile_sync_device_head_if_changed(
                &source,
                DEFAULT_PROFILE_ID,
                root_id,
                &content_key,
                key_id,
            )
            .unwrap();
        assert_eq!(
            unchanged,
            ProfileSyncDeviceHeadPullRecordStatus::Unchanged {
                profile: DEFAULT_PROFILE_ID.to_string(),
                root_id: root_id.to_string(),
                object_id: object_id.to_string(),
            }
        );
    }

    #[test]
    fn profile_sync_trusted_pull_and_apply_manifest_from_device_head() {
        let content_key = ProfileSyncContentKey::from_bytes([30; PROFILE_SYNC_CONTENT_KEY_BYTES]);
        let signer = ProfileSyncDeviceSigner::generate("device-a").unwrap();
        let trusted_public_key = signer.public_key().unwrap();
        let key_id = "content-key-epoch-1";
        let manifest_root_id = "settings/latest";
        let manifest_object_id = "manifest-object-1";
        let change_object_id = "change-object-1";
        let change = IncomingSyncSettingText::new(
            DEFAULT_PROFILE_ID,
            SYNC_DOMAIN_SETTINGS,
            "ui.theme",
            "slate",
            "device-a",
            1,
            1,
        );
        let manifest = ProfileSyncManifest {
            profile: DEFAULT_PROFILE_ID.to_string(),
            root_id: manifest_root_id.to_string(),
            schema_version: PROFILE_SYNC_MANIFEST_SCHEMA_VERSION,
            membership_epoch: DEFAULT_PROFILE_SYNC_MEMBERSHIP_EPOCH,
            current_snapshot_object_id: None,
            tail_change_object_ids: vec![change_object_id.to_string()],
            included_domains: vec![SYNC_DOMAIN_SETTINGS.to_string()],
            device_frontiers: vec![ProfileSyncDeviceFrontier {
                device_id: "device-a".to_string(),
                latest_sequence: 1,
                latest_change_object_id: Some(change_object_id.to_string()),
            }],
            retention_policy: ProfileSyncRetentionPolicy::default(),
            created_at: 1234,
        };
        let device_head = VerifiedProfileSyncDeviceHead {
            object_id: "device-head-object-1".to_string(),
            device_head: ProfileSyncDeviceHead {
                profile: DEFAULT_PROFILE_ID.to_string(),
                device_id: "device-a".to_string(),
                root_id: "settings/devices/device-a/head".to_string(),
                schema_version: PROFILE_SYNC_DEVICE_HEAD_SCHEMA_VERSION,
                membership_epoch: DEFAULT_PROFILE_SYNC_MEMBERSHIP_EPOCH,
                latest_manifest_object_id: manifest_object_id.to_string(),
                latest_change_object_id: Some(change_object_id.to_string()),
                device_sequence: 1,
                logical_clock: 1,
                created_at: 1235,
            },
        };
        let mut source = InMemoryProfileSyncObjectSource::default();
        source.insert_object(
            DEFAULT_PROFILE_ID,
            change_object_id,
            sign_test_sync_object(
                DEFAULT_PROFILE_ID,
                SYNC_DOMAIN_SETTINGS,
                PROFILE_SYNC_SETTING_CHANGE_OBJECT_KIND,
                key_id,
                serde_json::to_vec(&change).unwrap().as_slice(),
                &content_key,
                &signer,
                32,
            ),
        );
        source.insert_object(
            DEFAULT_PROFILE_ID,
            manifest_object_id,
            sign_test_sync_object(
                DEFAULT_PROFILE_ID,
                SYNC_DOMAIN_SETTINGS,
                PROFILE_SYNC_MANIFEST_OBJECT_KIND,
                key_id,
                serde_json::to_vec(&manifest).unwrap().as_slice(),
                &content_key,
                &signer,
                33,
            ),
        );
        let destination_path = test_dir("sync-trusted-apply-manifest-from-device-head")
            .join(DEFAULT_DATABASE_FILE_NAME);
        let destination =
            SlateProfileDatabase::open_resolved_with_device_id(destination_path, "device-b")
                .unwrap();
        destination
            .register_sync_device_public_key(&SyncDevicePublicKeyRegistration {
                profile: DEFAULT_PROFILE_ID.to_string(),
                public_key: trusted_public_key,
                membership_epoch: DEFAULT_PROFILE_SYNC_MEMBERSHIP_EPOCH,
            })
            .unwrap();

        let pulled = destination
            .pull_trusted_signed_profile_sync_settings_manifest_objects_from_device_head(
                &source,
                DEFAULT_PROFILE_ID,
                &device_head,
                &content_key,
                key_id,
            )
            .unwrap();
        assert_eq!(pulled.manifest_object_id, manifest_object_id);
        assert_eq!(pulled.manifest, manifest);
        assert_eq!(pulled.tail_changes[0].object_id, change_object_id);

        let applied = destination
            .pull_and_apply_trusted_signed_settings_manifest_objects_from_device_head(
                &source,
                DEFAULT_PROFILE_ID,
                &device_head,
                &content_key,
                key_id,
            )
            .unwrap();
        assert_eq!(applied.manifest_object_id, manifest_object_id);
        assert_eq!(
            destination
                .get_setting_text("ui.theme")
                .expect("read setting applied from device head manifest")
                .as_deref(),
            Some("slate")
        );
        assert_eq!(
            destination
                .profile_sync_root(DEFAULT_PROFILE_ID, manifest_root_id)
                .expect("read manifest root applied from device head")
                .expect("manifest root recorded")
                .object_id,
            manifest_object_id
        );
    }

    #[test]
    fn profile_sync_trusted_pull_manifest_from_device_head_rejects_profile_mismatch() {
        let content_key = ProfileSyncContentKey::from_bytes([31; PROFILE_SYNC_CONTENT_KEY_BYTES]);
        let source = InMemoryProfileSyncObjectSource::default();
        let destination_path =
            test_dir("sync-trusted-device-head-profile-mismatch").join(DEFAULT_DATABASE_FILE_NAME);
        let destination =
            SlateProfileDatabase::open_resolved_with_device_id(destination_path, "device-b")
                .unwrap();
        let device_head = VerifiedProfileSyncDeviceHead {
            object_id: "device-head-object-1".to_string(),
            device_head: ProfileSyncDeviceHead {
                profile: "other-profile".to_string(),
                device_id: "device-a".to_string(),
                root_id: "settings/devices/device-a/head".to_string(),
                schema_version: PROFILE_SYNC_DEVICE_HEAD_SCHEMA_VERSION,
                membership_epoch: DEFAULT_PROFILE_SYNC_MEMBERSHIP_EPOCH,
                latest_manifest_object_id: "manifest-object-1".to_string(),
                latest_change_object_id: None,
                device_sequence: 1,
                logical_clock: 1,
                created_at: 1235,
            },
        };

        assert!(matches!(
            destination.pull_trusted_signed_profile_sync_settings_manifest_objects_from_device_head(
                &source,
                DEFAULT_PROFILE_ID,
                &device_head,
                &content_key,
                "content-key-epoch-1",
            ),
            Err(ProfileSyncTrustedPullError::Open(
                ProfileSyncTrustedOpenError::SyncObject(
                    SyncObjectError::UnexpectedProfile { expected, actual }
                )
            )) if expected == DEFAULT_PROFILE_ID && actual == "other-profile"
        ));
    }

    #[test]
    fn profile_sync_trusted_pull_manifest_from_device_head_rejects_frontier_mismatch() {
        let content_key = ProfileSyncContentKey::from_bytes([32; PROFILE_SYNC_CONTENT_KEY_BYTES]);
        let signer = ProfileSyncDeviceSigner::generate("device-a").unwrap();
        let trusted_public_key = signer.public_key().unwrap();
        let key_id = "content-key-epoch-1";
        let manifest_without_frontier_object_id = "manifest-without-frontier";
        let mismatched_manifest_object_id = "manifest-with-mismatched-frontier";
        let manifest_without_frontier = ProfileSyncManifest {
            profile: DEFAULT_PROFILE_ID.to_string(),
            root_id: "settings/latest".to_string(),
            schema_version: PROFILE_SYNC_MANIFEST_SCHEMA_VERSION,
            membership_epoch: DEFAULT_PROFILE_SYNC_MEMBERSHIP_EPOCH,
            current_snapshot_object_id: None,
            tail_change_object_ids: Vec::new(),
            included_domains: vec![SYNC_DOMAIN_SETTINGS.to_string()],
            device_frontiers: Vec::new(),
            retention_policy: ProfileSyncRetentionPolicy::default(),
            created_at: 1234,
        };
        let mismatched_manifest = ProfileSyncManifest {
            device_frontiers: vec![ProfileSyncDeviceFrontier {
                device_id: "device-a".to_string(),
                latest_sequence: 2,
                latest_change_object_id: None,
            }],
            ..manifest_without_frontier.clone()
        };
        let mut source = InMemoryProfileSyncObjectSource::default();
        source.insert_object(
            DEFAULT_PROFILE_ID,
            manifest_without_frontier_object_id,
            sign_test_sync_object(
                DEFAULT_PROFILE_ID,
                SYNC_DOMAIN_SETTINGS,
                PROFILE_SYNC_MANIFEST_OBJECT_KIND,
                key_id,
                serde_json::to_vec(&manifest_without_frontier)
                    .unwrap()
                    .as_slice(),
                &content_key,
                &signer,
                34,
            ),
        );
        source.insert_object(
            DEFAULT_PROFILE_ID,
            mismatched_manifest_object_id,
            sign_test_sync_object(
                DEFAULT_PROFILE_ID,
                SYNC_DOMAIN_SETTINGS,
                PROFILE_SYNC_MANIFEST_OBJECT_KIND,
                key_id,
                serde_json::to_vec(&mismatched_manifest).unwrap().as_slice(),
                &content_key,
                &signer,
                35,
            ),
        );
        let destination_path =
            test_dir("sync-trusted-device-head-frontier-mismatch").join(DEFAULT_DATABASE_FILE_NAME);
        let destination =
            SlateProfileDatabase::open_resolved_with_device_id(destination_path, "device-b")
                .unwrap();
        destination
            .register_sync_device_public_key(&SyncDevicePublicKeyRegistration {
                profile: DEFAULT_PROFILE_ID.to_string(),
                public_key: trusted_public_key,
                membership_epoch: DEFAULT_PROFILE_SYNC_MEMBERSHIP_EPOCH,
            })
            .unwrap();

        let head_without_frontier = VerifiedProfileSyncDeviceHead {
            object_id: "device-head-object-1".to_string(),
            device_head: ProfileSyncDeviceHead {
                profile: DEFAULT_PROFILE_ID.to_string(),
                device_id: "device-a".to_string(),
                root_id: "settings/devices/device-a/head".to_string(),
                schema_version: PROFILE_SYNC_DEVICE_HEAD_SCHEMA_VERSION,
                membership_epoch: DEFAULT_PROFILE_SYNC_MEMBERSHIP_EPOCH,
                latest_manifest_object_id: manifest_without_frontier_object_id.to_string(),
                latest_change_object_id: Some("change-object-1".to_string()),
                device_sequence: 1,
                logical_clock: 1,
                created_at: 1235,
            },
        };
        assert!(matches!(
            destination.pull_trusted_signed_profile_sync_settings_manifest_objects_from_device_head(
                &source,
                DEFAULT_PROFILE_ID,
                &head_without_frontier,
                &content_key,
                key_id,
            ),
            Err(ProfileSyncTrustedPullError::Open(
                ProfileSyncTrustedOpenError::SyncObject(
                    SyncObjectError::UnexpectedDeviceFrontier {
                        device_id,
                        expected_sequence,
                        actual_sequence: None,
                        expected_change_object_id,
                        actual_change_object_id: None,
                    }
                )
            )) if device_id == "device-a"
                && expected_sequence == 1
                && expected_change_object_id.as_deref() == Some("change-object-1")
        ));

        let head_with_mismatch = VerifiedProfileSyncDeviceHead {
            object_id: "device-head-object-2".to_string(),
            device_head: ProfileSyncDeviceHead {
                latest_manifest_object_id: mismatched_manifest_object_id.to_string(),
                ..head_without_frontier.device_head
            },
        };
        assert!(matches!(
            destination.pull_trusted_signed_profile_sync_settings_manifest_objects_from_device_head(
                &source,
                DEFAULT_PROFILE_ID,
                &head_with_mismatch,
                &content_key,
                key_id,
            ),
            Err(ProfileSyncTrustedPullError::Open(
                ProfileSyncTrustedOpenError::SyncObject(
                    SyncObjectError::UnexpectedDeviceFrontier {
                        device_id,
                        expected_sequence,
                        actual_sequence: Some(2),
                        expected_change_object_id,
                        actual_change_object_id: None,
                    }
                )
            )) if device_id == "device-a"
                && expected_sequence == 1
                && expected_change_object_id.as_deref() == Some("change-object-1")
        ));
    }

    #[test]
    fn profile_sync_trusted_pull_manifest_from_device_head_rejects_epoch_mismatch() {
        let content_key = ProfileSyncContentKey::from_bytes([33; PROFILE_SYNC_CONTENT_KEY_BYTES]);
        let signer = ProfileSyncDeviceSigner::generate("device-a").unwrap();
        let trusted_public_key = signer.public_key().unwrap();
        let key_id = "content-key-epoch-1";
        let manifest_object_id = "manifest-with-mismatched-epoch";
        let manifest = ProfileSyncManifest {
            profile: DEFAULT_PROFILE_ID.to_string(),
            root_id: "settings/latest".to_string(),
            schema_version: PROFILE_SYNC_MANIFEST_SCHEMA_VERSION,
            membership_epoch: DEFAULT_PROFILE_SYNC_MEMBERSHIP_EPOCH + 1,
            current_snapshot_object_id: None,
            tail_change_object_ids: Vec::new(),
            included_domains: vec![SYNC_DOMAIN_SETTINGS.to_string()],
            device_frontiers: vec![ProfileSyncDeviceFrontier {
                device_id: "device-a".to_string(),
                latest_sequence: 1,
                latest_change_object_id: None,
            }],
            retention_policy: ProfileSyncRetentionPolicy::default(),
            created_at: 1234,
        };
        let mut source = InMemoryProfileSyncObjectSource::default();
        source.insert_object(
            DEFAULT_PROFILE_ID,
            manifest_object_id,
            sign_test_sync_object(
                DEFAULT_PROFILE_ID,
                SYNC_DOMAIN_SETTINGS,
                PROFILE_SYNC_MANIFEST_OBJECT_KIND,
                key_id,
                serde_json::to_vec(&manifest).unwrap().as_slice(),
                &content_key,
                &signer,
                36,
            ),
        );
        let destination_path =
            test_dir("sync-trusted-device-head-epoch-mismatch").join(DEFAULT_DATABASE_FILE_NAME);
        let destination =
            SlateProfileDatabase::open_resolved_with_device_id(destination_path, "device-b")
                .unwrap();
        destination
            .register_sync_device_public_key(&SyncDevicePublicKeyRegistration {
                profile: DEFAULT_PROFILE_ID.to_string(),
                public_key: trusted_public_key,
                membership_epoch: DEFAULT_PROFILE_SYNC_MEMBERSHIP_EPOCH,
            })
            .unwrap();
        let device_head = VerifiedProfileSyncDeviceHead {
            object_id: "device-head-object-1".to_string(),
            device_head: ProfileSyncDeviceHead {
                profile: DEFAULT_PROFILE_ID.to_string(),
                device_id: "device-a".to_string(),
                root_id: "settings/devices/device-a/head".to_string(),
                schema_version: PROFILE_SYNC_DEVICE_HEAD_SCHEMA_VERSION,
                membership_epoch: DEFAULT_PROFILE_SYNC_MEMBERSHIP_EPOCH,
                latest_manifest_object_id: manifest_object_id.to_string(),
                latest_change_object_id: None,
                device_sequence: 1,
                logical_clock: 1,
                created_at: 1235,
            },
        };

        assert!(matches!(
            destination.pull_trusted_signed_profile_sync_settings_manifest_objects_from_device_head(
                &source,
                DEFAULT_PROFILE_ID,
                &device_head,
                &content_key,
                key_id,
            ),
            Err(ProfileSyncTrustedPullError::Open(
                ProfileSyncTrustedOpenError::SyncObject(
                    SyncObjectError::UnexpectedDeviceHeadManifestEpoch {
                        device_id,
                        head_membership_epoch,
                        manifest_membership_epoch,
                    }
                )
            )) if device_id == "device-a"
                && head_membership_epoch == DEFAULT_PROFILE_SYNC_MEMBERSHIP_EPOCH
                && manifest_membership_epoch == DEFAULT_PROFILE_SYNC_MEMBERSHIP_EPOCH + 1
        ));
    }

    #[test]
    fn profile_sync_pull_fetches_manifest_snapshot_and_tail_objects() {
        let content_key = ProfileSyncContentKey::from_bytes([12; PROFILE_SYNC_CONTENT_KEY_BYTES]);
        let signer = ProfileSyncDeviceSigner::generate("device-a").unwrap();
        let trusted_public_key = signer.public_key().unwrap();
        let key_id = "content-key-epoch-1";
        let root_id = "settings/latest";
        let snapshot_object_id = "snapshot-object-1";
        let tail_object_id = "tail-object-1";
        let manifest_object_id = "manifest-object-1";
        let snapshot = ProfileSyncSettingsSnapshot {
            profile: DEFAULT_PROFILE_ID.to_string(),
            schema_version: PROFILE_SYNC_SETTINGS_SNAPSHOT_SCHEMA_VERSION,
            covers_revision: 1,
            included_domains: vec![SYNC_DOMAIN_SETTINGS.to_string()],
            values: vec![ProfileSyncSettingsSnapshotValue {
                domain: SYNC_DOMAIN_SETTINGS.to_string(),
                key: "ui.theme".to_string(),
                value: "teal".to_string(),
                value_kind: "text".to_string(),
                revision: 1,
            }],
            created_at: 100,
        };
        let tail_change = IncomingSyncSettingText::new(
            DEFAULT_PROFILE_ID,
            SYNC_DOMAIN_SETTINGS,
            "ui.theme",
            "slate",
            "device-a",
            2,
            2,
        );
        let manifest = ProfileSyncManifest {
            profile: DEFAULT_PROFILE_ID.to_string(),
            root_id: root_id.to_string(),
            schema_version: PROFILE_SYNC_MANIFEST_SCHEMA_VERSION,
            membership_epoch: DEFAULT_PROFILE_SYNC_MEMBERSHIP_EPOCH,
            current_snapshot_object_id: Some(snapshot_object_id.to_string()),
            tail_change_object_ids: vec![tail_object_id.to_string()],
            included_domains: vec![SYNC_DOMAIN_SETTINGS.to_string()],
            device_frontiers: vec![ProfileSyncDeviceFrontier {
                device_id: "device-a".to_string(),
                latest_sequence: 2,
                latest_change_object_id: Some(tail_object_id.to_string()),
            }],
            retention_policy: ProfileSyncRetentionPolicy::default(),
            created_at: 101,
        };
        let snapshot_payload = serde_json::to_vec(&snapshot).unwrap();
        let tail_payload = serde_json::to_vec(&tail_change).unwrap();
        let manifest_payload = serde_json::to_vec(&manifest).unwrap();
        let mut source = InMemoryProfileSyncObjectSource::default();
        source.insert_object(
            DEFAULT_PROFILE_ID,
            snapshot_object_id,
            sign_test_sync_object(
                DEFAULT_PROFILE_ID,
                SYNC_DOMAIN_SETTINGS,
                PROFILE_SYNC_SETTINGS_SNAPSHOT_OBJECT_KIND,
                key_id,
                snapshot_payload.as_slice(),
                &content_key,
                &signer,
                11,
            ),
        );
        source.insert_object(
            DEFAULT_PROFILE_ID,
            tail_object_id,
            sign_test_sync_object(
                DEFAULT_PROFILE_ID,
                SYNC_DOMAIN_SETTINGS,
                PROFILE_SYNC_SETTING_CHANGE_OBJECT_KIND,
                key_id,
                tail_payload.as_slice(),
                &content_key,
                &signer,
                12,
            ),
        );
        source.insert_object(
            DEFAULT_PROFILE_ID,
            manifest_object_id,
            sign_test_sync_object(
                DEFAULT_PROFILE_ID,
                SYNC_DOMAIN_SETTINGS,
                PROFILE_SYNC_MANIFEST_OBJECT_KIND,
                key_id,
                manifest_payload.as_slice(),
                &content_key,
                &signer,
                13,
            ),
        );
        source.publish_root(DEFAULT_PROFILE_ID, root_id, manifest_object_id);

        let pulled = pull_signed_profile_sync_settings_manifest_objects(
            &source,
            DEFAULT_PROFILE_ID,
            root_id,
            &content_key,
            &trusted_public_key,
            key_id,
        )
        .unwrap()
        .expect("published settings root");
        assert_eq!(pulled.manifest_object_id, manifest_object_id);
        assert_eq!(pulled.manifest, manifest);
        assert_eq!(
            pulled
                .snapshot
                .as_ref()
                .map(|snapshot| snapshot.object_id.as_str()),
            Some(snapshot_object_id)
        );
        assert_eq!(pulled.tail_changes[0].object_id, tail_object_id);

        let destination_path = test_dir("sync-pull-destination").join(DEFAULT_DATABASE_FILE_NAME);
        let destination =
            SlateProfileDatabase::open_resolved_with_device_id(destination_path, "device-b")
                .unwrap();
        let applied = destination
            .pull_and_apply_signed_settings_manifest_objects(
                &source,
                DEFAULT_PROFILE_ID,
                root_id,
                &content_key,
                &trusted_public_key,
                key_id,
            )
            .unwrap()
            .expect("applied pulled settings root");

        assert_eq!(applied.manifest_object_id, manifest_object_id);
        assert_eq!(applied.tail_changes.len(), 1);
        assert_eq!(
            destination.get_setting_text("ui.theme").unwrap().as_deref(),
            Some("slate")
        );
        assert_eq!(
            destination
                .profile_sync_root(DEFAULT_PROFILE_ID, root_id)
                .unwrap()
                .expect("stored profile root")
                .object_id,
            manifest_object_id
        );
    }

    #[test]
    fn profile_sync_trusted_pull_uses_stored_device_public_key() {
        let content_key = ProfileSyncContentKey::from_bytes([16; PROFILE_SYNC_CONTENT_KEY_BYTES]);
        let signer = ProfileSyncDeviceSigner::generate("device-a").unwrap();
        let trusted_public_key = signer.public_key().unwrap();
        let key_id = "content-key-epoch-1";
        let root_id = "settings/latest";
        let tail_object_id = "tail-object-1";
        let manifest_object_id = "manifest-object-1";
        let tail_change = IncomingSyncSettingText::new(
            DEFAULT_PROFILE_ID,
            SYNC_DOMAIN_SETTINGS,
            "ui.theme",
            "slate",
            "device-a",
            1,
            1,
        );
        let manifest = ProfileSyncManifest {
            profile: DEFAULT_PROFILE_ID.to_string(),
            root_id: root_id.to_string(),
            schema_version: PROFILE_SYNC_MANIFEST_SCHEMA_VERSION,
            membership_epoch: DEFAULT_PROFILE_SYNC_MEMBERSHIP_EPOCH,
            current_snapshot_object_id: None,
            tail_change_object_ids: vec![tail_object_id.to_string()],
            included_domains: vec![SYNC_DOMAIN_SETTINGS.to_string()],
            device_frontiers: vec![ProfileSyncDeviceFrontier {
                device_id: "device-a".to_string(),
                latest_sequence: 1,
                latest_change_object_id: Some(tail_object_id.to_string()),
            }],
            retention_policy: ProfileSyncRetentionPolicy::default(),
            created_at: 101,
        };
        let mut source = InMemoryProfileSyncObjectSource::default();
        source.insert_object(
            DEFAULT_PROFILE_ID,
            tail_object_id,
            sign_test_sync_object(
                DEFAULT_PROFILE_ID,
                SYNC_DOMAIN_SETTINGS,
                PROFILE_SYNC_SETTING_CHANGE_OBJECT_KIND,
                key_id,
                serde_json::to_vec(&tail_change).unwrap().as_slice(),
                &content_key,
                &signer,
                31,
            ),
        );
        source.insert_object(
            DEFAULT_PROFILE_ID,
            manifest_object_id,
            sign_test_sync_object(
                DEFAULT_PROFILE_ID,
                SYNC_DOMAIN_SETTINGS,
                PROFILE_SYNC_MANIFEST_OBJECT_KIND,
                key_id,
                serde_json::to_vec(&manifest).unwrap().as_slice(),
                &content_key,
                &signer,
                32,
            ),
        );
        source.publish_root(DEFAULT_PROFILE_ID, root_id, manifest_object_id);
        let destination_path = test_dir("sync-trusted-pull").join(DEFAULT_DATABASE_FILE_NAME);
        let destination =
            SlateProfileDatabase::open_resolved_with_device_id(destination_path, "device-b")
                .unwrap();
        destination
            .register_sync_device_public_key(&SyncDevicePublicKeyRegistration {
                profile: DEFAULT_PROFILE_ID.to_string(),
                public_key: trusted_public_key,
                membership_epoch: DEFAULT_PROFILE_SYNC_MEMBERSHIP_EPOCH,
            })
            .unwrap();
        destination
            .register_sync_content_key_epoch(&SyncContentKeyEpochRegistration {
                profile: DEFAULT_PROFILE_ID.to_string(),
                key_id: key_id.to_string(),
                membership_epoch: DEFAULT_PROFILE_SYNC_MEMBERSHIP_EPOCH,
                algorithm: PROFILE_SYNC_CONTENT_KEY_ALGORITHM_CHACHA20_POLY1305.to_string(),
                active: true,
            })
            .unwrap();

        let applied = destination
            .pull_and_apply_active_trusted_signed_settings_manifest_objects_if_changed(
                &source,
                DEFAULT_PROFILE_ID,
                root_id,
                &content_key,
            )
            .unwrap();
        let ProfileSyncSettingsPullApplyStatus::Applied(applied) = applied else {
            panic!("expected trusted settings root to apply, got {applied:?}");
        };

        assert_eq!(applied.manifest_object_id, manifest_object_id);
        assert_eq!(applied.tail_changes.len(), 1);
        assert_eq!(
            destination.get_setting_text("ui.theme").unwrap().as_deref(),
            Some("slate")
        );
        assert_eq!(
            destination
                .profile_sync_root(DEFAULT_PROFILE_ID, root_id)
                .unwrap()
                .expect("stored trusted pull root")
                .object_id,
            manifest_object_id
        );
    }

    #[test]
    fn profile_sync_trusted_pull_lists_competing_manifest_candidates() {
        let content_key = ProfileSyncContentKey::from_bytes([34; PROFILE_SYNC_CONTENT_KEY_BYTES]);
        let signer_a = ProfileSyncDeviceSigner::generate("candidate-device-a").unwrap();
        let signer_b = ProfileSyncDeviceSigner::generate("candidate-device-b").unwrap();
        let public_key_a = signer_a.public_key().unwrap();
        let public_key_b = signer_b.public_key().unwrap();
        let key_id = "content-key-epoch-1";
        let root_id = "settings/latest";
        let change_a_object_id = "candidate-change-a";
        let change_b_object_id = "candidate-change-b";
        let manifest_a_object_id = "candidate-manifest-a";
        let manifest_b_object_id = "candidate-manifest-b";
        let change_a = IncomingSyncSettingText::new(
            DEFAULT_PROFILE_ID,
            SYNC_DOMAIN_SETTINGS,
            "ui.theme",
            "aqua",
            "candidate-device-a",
            1,
            1,
        );
        let change_b = IncomingSyncSettingText::new(
            DEFAULT_PROFILE_ID,
            SYNC_DOMAIN_SETTINGS,
            "ui.theme",
            "teal",
            "candidate-device-b",
            1,
            2,
        );
        let manifest_a = ProfileSyncManifest {
            profile: DEFAULT_PROFILE_ID.to_string(),
            root_id: root_id.to_string(),
            schema_version: PROFILE_SYNC_MANIFEST_SCHEMA_VERSION,
            membership_epoch: DEFAULT_PROFILE_SYNC_MEMBERSHIP_EPOCH,
            current_snapshot_object_id: None,
            tail_change_object_ids: vec![change_a_object_id.to_string()],
            included_domains: vec![SYNC_DOMAIN_SETTINGS.to_string()],
            device_frontiers: vec![ProfileSyncDeviceFrontier {
                device_id: "candidate-device-a".to_string(),
                latest_sequence: 1,
                latest_change_object_id: Some(change_a_object_id.to_string()),
            }],
            retention_policy: ProfileSyncRetentionPolicy::default(),
            created_at: 101,
        };
        let manifest_b = ProfileSyncManifest {
            profile: DEFAULT_PROFILE_ID.to_string(),
            root_id: root_id.to_string(),
            schema_version: PROFILE_SYNC_MANIFEST_SCHEMA_VERSION,
            membership_epoch: DEFAULT_PROFILE_SYNC_MEMBERSHIP_EPOCH,
            current_snapshot_object_id: None,
            tail_change_object_ids: vec![change_b_object_id.to_string()],
            included_domains: vec![SYNC_DOMAIN_SETTINGS.to_string()],
            device_frontiers: vec![ProfileSyncDeviceFrontier {
                device_id: "candidate-device-b".to_string(),
                latest_sequence: 1,
                latest_change_object_id: Some(change_b_object_id.to_string()),
            }],
            retention_policy: ProfileSyncRetentionPolicy::default(),
            created_at: 102,
        };
        let mut source = InMemoryProfileSyncObjectSource::default();
        source.insert_object(
            DEFAULT_PROFILE_ID,
            change_a_object_id,
            sign_test_sync_object(
                DEFAULT_PROFILE_ID,
                SYNC_DOMAIN_SETTINGS,
                PROFILE_SYNC_SETTING_CHANGE_OBJECT_KIND,
                key_id,
                serde_json::to_vec(&change_a).unwrap().as_slice(),
                &content_key,
                &signer_a,
                41,
            ),
        );
        source.insert_object(
            DEFAULT_PROFILE_ID,
            change_b_object_id,
            sign_test_sync_object(
                DEFAULT_PROFILE_ID,
                SYNC_DOMAIN_SETTINGS,
                PROFILE_SYNC_SETTING_CHANGE_OBJECT_KIND,
                key_id,
                serde_json::to_vec(&change_b).unwrap().as_slice(),
                &content_key,
                &signer_b,
                42,
            ),
        );
        source.insert_object(
            DEFAULT_PROFILE_ID,
            manifest_a_object_id,
            sign_test_sync_object(
                DEFAULT_PROFILE_ID,
                SYNC_DOMAIN_SETTINGS,
                PROFILE_SYNC_MANIFEST_OBJECT_KIND,
                key_id,
                serde_json::to_vec(&manifest_a).unwrap().as_slice(),
                &content_key,
                &signer_a,
                43,
            ),
        );
        source.insert_object(
            DEFAULT_PROFILE_ID,
            manifest_b_object_id,
            sign_test_sync_object(
                DEFAULT_PROFILE_ID,
                SYNC_DOMAIN_SETTINGS,
                PROFILE_SYNC_MANIFEST_OBJECT_KIND,
                key_id,
                serde_json::to_vec(&manifest_b).unwrap().as_slice(),
                &content_key,
                &signer_b,
                44,
            ),
        );
        source.publish_root_candidate(
            DEFAULT_PROFILE_ID,
            root_id,
            "provider-device-b",
            manifest_b_object_id,
            2,
        );
        source.publish_root_candidate(
            DEFAULT_PROFILE_ID,
            root_id,
            "provider-device-a",
            manifest_a_object_id,
            1,
        );
        let destination_path =
            test_dir("sync-trusted-candidate-roots").join(DEFAULT_DATABASE_FILE_NAME);
        let destination =
            SlateProfileDatabase::open_resolved_with_device_id(destination_path, "device-c")
                .unwrap();
        destination
            .register_sync_device_public_key(&SyncDevicePublicKeyRegistration {
                profile: DEFAULT_PROFILE_ID.to_string(),
                public_key: public_key_a,
                membership_epoch: DEFAULT_PROFILE_SYNC_MEMBERSHIP_EPOCH,
            })
            .unwrap();
        destination
            .register_sync_device_public_key(&SyncDevicePublicKeyRegistration {
                profile: DEFAULT_PROFILE_ID.to_string(),
                public_key: public_key_b,
                membership_epoch: DEFAULT_PROFILE_SYNC_MEMBERSHIP_EPOCH,
            })
            .unwrap();

        let candidates = destination
            .pull_trusted_signed_profile_sync_settings_manifest_candidates(
                &source,
                DEFAULT_PROFILE_ID,
                root_id,
                &content_key,
                key_id,
            )
            .unwrap();

        assert_eq!(candidates.len(), 2);
        assert_eq!(
            candidates
                .iter()
                .map(|candidate| (
                    candidate.root_candidate.publisher_id.as_str(),
                    candidate.root_candidate.object_id.as_str(),
                    candidate.root_candidate.publish_sequence,
                    candidate.objects.tail_changes[0].change.value.as_str(),
                ))
                .collect::<Vec<_>>(),
            vec![
                ("provider-device-b", manifest_b_object_id, 2, "teal"),
                ("provider-device-a", manifest_a_object_id, 1, "aqua"),
            ]
        );
        assert_eq!(candidates[0].objects.manifest, manifest_b);
        assert_eq!(candidates[1].objects.manifest, manifest_a);
        assert!(
            destination
                .profile_sync_root(DEFAULT_PROFILE_ID, root_id)
                .unwrap()
                .is_none(),
            "listing manifest candidates should not apply or record a winner"
        );

        let applications = destination
            .apply_verified_settings_manifest_candidates(candidates.as_slice())
            .unwrap();
        assert_eq!(
            applications
                .iter()
                .map(|application| {
                    (
                        application.root_candidate.publisher_id.as_str(),
                        application.application.manifest_object_id.as_str(),
                    )
                })
                .collect::<Vec<_>>(),
            vec![
                ("provider-device-a", manifest_a_object_id),
                ("provider-device-b", manifest_b_object_id),
            ],
            "candidate application should run oldest root publication first"
        );
        assert_eq!(
            destination.get_setting_text("ui.theme").unwrap().as_deref(),
            Some("teal")
        );
        assert_eq!(
            destination
                .profile_sync_root(DEFAULT_PROFILE_ID, root_id)
                .unwrap()
                .expect("candidate application records newest verified root")
                .object_id,
            manifest_b_object_id
        );

        let active_destination_path =
            test_dir("sync-active-candidate-roots").join(DEFAULT_DATABASE_FILE_NAME);
        let active_destination =
            SlateProfileDatabase::open_resolved_with_device_id(active_destination_path, "device-d")
                .unwrap();
        active_destination
            .register_sync_device_public_key(&SyncDevicePublicKeyRegistration {
                profile: DEFAULT_PROFILE_ID.to_string(),
                public_key: signer_a.public_key().unwrap(),
                membership_epoch: DEFAULT_PROFILE_SYNC_MEMBERSHIP_EPOCH,
            })
            .unwrap();
        active_destination
            .register_sync_device_public_key(&SyncDevicePublicKeyRegistration {
                profile: DEFAULT_PROFILE_ID.to_string(),
                public_key: signer_b.public_key().unwrap(),
                membership_epoch: DEFAULT_PROFILE_SYNC_MEMBERSHIP_EPOCH,
            })
            .unwrap();
        active_destination
            .register_sync_content_key_epoch(&SyncContentKeyEpochRegistration {
                profile: DEFAULT_PROFILE_ID.to_string(),
                key_id: key_id.to_string(),
                membership_epoch: DEFAULT_PROFILE_SYNC_MEMBERSHIP_EPOCH,
                algorithm: PROFILE_SYNC_CONTENT_KEY_ALGORITHM_CHACHA20_POLY1305.to_string(),
                active: true,
            })
            .unwrap();

        let active_status = active_destination
            .pull_and_apply_active_trusted_signed_settings_manifest_candidates_if_changed(
                &source,
                DEFAULT_PROFILE_ID,
                root_id,
                &content_key,
            )
            .unwrap();
        let ProfileSyncSettingsCandidatePullApplyStatus::Applied(active_applications) =
            active_status
        else {
            panic!("expected active candidate application, got {active_status:?}");
        };
        assert_eq!(
            active_applications
                .iter()
                .map(|application| application.application.manifest_object_id.as_str())
                .collect::<Vec<_>>(),
            vec![manifest_a_object_id, manifest_b_object_id]
        );
        assert_eq!(
            active_destination
                .get_setting_text("ui.theme")
                .unwrap()
                .as_deref(),
            Some("teal")
        );

        let unchanged = active_destination
            .pull_and_apply_active_trusted_signed_settings_manifest_candidates_if_changed(
                &source,
                DEFAULT_PROFILE_ID,
                root_id,
                &content_key,
            )
            .unwrap();
        assert_eq!(
            unchanged,
            ProfileSyncSettingsCandidatePullApplyStatus::Unchanged {
                profile: DEFAULT_PROFILE_ID.to_string(),
                root_id: root_id.to_string(),
                object_id: manifest_b_object_id.to_string(),
            }
        );
    }

    #[test]
    fn profile_sync_active_key_pull_reports_missing_published_root() {
        let content_key = ProfileSyncContentKey::from_bytes([22; PROFILE_SYNC_CONTENT_KEY_BYTES]);
        let source = InMemoryProfileSyncObjectSource::default();
        let destination_path =
            test_dir("sync-active-missing-root").join(DEFAULT_DATABASE_FILE_NAME);
        let destination =
            SlateProfileDatabase::open_resolved_with_device_id(destination_path, "device-b")
                .unwrap();

        let status = destination
            .pull_and_apply_active_trusted_signed_settings_manifest_objects_if_changed(
                &source,
                DEFAULT_PROFILE_ID,
                "settings/latest",
                &content_key,
            )
            .unwrap();

        assert_eq!(
            status,
            ProfileSyncSettingsPullApplyStatus::NoPublishedRoot {
                profile: DEFAULT_PROFILE_ID.to_string(),
                root_id: "settings/latest".to_string(),
            }
        );
    }

    #[test]
    fn profile_sync_active_key_pull_skips_unchanged_root_without_fetching() {
        let content_key = ProfileSyncContentKey::from_bytes([23; PROFILE_SYNC_CONTENT_KEY_BYTES]);
        let root_id = "settings/latest";
        let object_id = "manifest-object-1";
        let mut source = InMemoryProfileSyncObjectSource::default();
        source.publish_root(DEFAULT_PROFILE_ID, root_id, object_id);
        let destination_path =
            test_dir("sync-active-unchanged-root").join(DEFAULT_DATABASE_FILE_NAME);
        let destination =
            SlateProfileDatabase::open_resolved_with_device_id(destination_path, "device-b")
                .unwrap();
        destination
            .set_profile_sync_root(DEFAULT_PROFILE_ID, root_id, object_id)
            .unwrap();

        let status = destination
            .pull_and_apply_active_trusted_signed_settings_manifest_objects_if_changed(
                &source,
                DEFAULT_PROFILE_ID,
                root_id,
                &content_key,
            )
            .unwrap();

        assert_eq!(
            status,
            ProfileSyncSettingsPullApplyStatus::Unchanged {
                profile: DEFAULT_PROFILE_ID.to_string(),
                root_id: root_id.to_string(),
                object_id: object_id.to_string(),
            }
        );
    }

    #[test]
    fn profile_sync_active_key_pull_rejects_unsupported_content_key_algorithm() {
        let content_key = ProfileSyncContentKey::from_bytes([20; PROFILE_SYNC_CONTENT_KEY_BYTES]);
        let source = InMemoryProfileSyncObjectSource::default();
        let destination_path =
            test_dir("sync-active-unsupported-algorithm").join(DEFAULT_DATABASE_FILE_NAME);
        let destination =
            SlateProfileDatabase::open_resolved_with_device_id(destination_path, "device-b")
                .unwrap();
        destination
            .register_sync_content_key_epoch(&SyncContentKeyEpochRegistration {
                profile: DEFAULT_PROFILE_ID.to_string(),
                key_id: "content-key-epoch-1".to_string(),
                membership_epoch: DEFAULT_PROFILE_SYNC_MEMBERSHIP_EPOCH,
                algorithm: "future-aead".to_string(),
                active: true,
            })
            .unwrap();

        let error = destination
            .pull_and_apply_active_trusted_signed_settings_manifest_objects(
                &source,
                DEFAULT_PROFILE_ID,
                "settings/latest",
                &content_key,
            )
            .unwrap_err();

        assert!(matches!(
            error,
            ProfileSyncTrustedPullApplyError::Storage(
                StorageError::UnsupportedSyncContentKeyAlgorithm { key_id, algorithm }
            ) if key_id == "content-key-epoch-1" && algorithm == "future-aead"
        ));
        assert_eq!(
            destination
                .profile_sync_root(DEFAULT_PROFILE_ID, "settings/latest")
                .unwrap(),
            None
        );
    }

    #[test]
    fn profile_sync_active_key_pull_rejects_content_key_after_manifest_epoch() {
        let content_key = ProfileSyncContentKey::from_bytes([21; PROFILE_SYNC_CONTENT_KEY_BYTES]);
        let signer = ProfileSyncDeviceSigner::generate("device-a").unwrap();
        let trusted_public_key = signer.public_key().unwrap();
        let key_id = "content-key-epoch-1";
        let root_id = "settings/latest";
        let manifest_object_id = "manifest-object-1";
        let manifest = ProfileSyncManifest {
            profile: DEFAULT_PROFILE_ID.to_string(),
            root_id: root_id.to_string(),
            schema_version: PROFILE_SYNC_MANIFEST_SCHEMA_VERSION,
            membership_epoch: DEFAULT_PROFILE_SYNC_MEMBERSHIP_EPOCH,
            current_snapshot_object_id: None,
            tail_change_object_ids: Vec::new(),
            included_domains: vec![SYNC_DOMAIN_SETTINGS.to_string()],
            device_frontiers: vec![ProfileSyncDeviceFrontier {
                device_id: "device-a".to_string(),
                latest_sequence: 1,
                latest_change_object_id: None,
            }],
            retention_policy: ProfileSyncRetentionPolicy::default(),
            created_at: 101,
        };
        let mut source = InMemoryProfileSyncObjectSource::default();
        source.insert_object(
            DEFAULT_PROFILE_ID,
            manifest_object_id,
            sign_test_sync_object(
                DEFAULT_PROFILE_ID,
                SYNC_DOMAIN_SETTINGS,
                PROFILE_SYNC_MANIFEST_OBJECT_KIND,
                key_id,
                serde_json::to_vec(&manifest).unwrap().as_slice(),
                &content_key,
                &signer,
                71,
            ),
        );
        source.publish_root(DEFAULT_PROFILE_ID, root_id, manifest_object_id);
        let destination_path =
            test_dir("sync-active-future-key-epoch").join(DEFAULT_DATABASE_FILE_NAME);
        let destination =
            SlateProfileDatabase::open_resolved_with_device_id(destination_path, "device-b")
                .unwrap();
        destination
            .register_sync_device_public_key(&SyncDevicePublicKeyRegistration {
                profile: DEFAULT_PROFILE_ID.to_string(),
                public_key: trusted_public_key,
                membership_epoch: DEFAULT_PROFILE_SYNC_MEMBERSHIP_EPOCH,
            })
            .unwrap();
        destination
            .register_sync_content_key_epoch(&SyncContentKeyEpochRegistration {
                profile: DEFAULT_PROFILE_ID.to_string(),
                key_id: key_id.to_string(),
                membership_epoch: DEFAULT_PROFILE_SYNC_MEMBERSHIP_EPOCH + 1,
                algorithm: PROFILE_SYNC_CONTENT_KEY_ALGORITHM_CHACHA20_POLY1305.to_string(),
                active: true,
            })
            .unwrap();

        let error = destination
            .pull_and_apply_active_trusted_signed_settings_manifest_objects(
                &source,
                DEFAULT_PROFILE_ID,
                root_id,
                &content_key,
            )
            .unwrap_err();

        assert!(matches!(
            error,
            ProfileSyncTrustedPullApplyError::Storage(
                StorageError::UnauthorizedSyncContentKeyEpoch {
                    profile,
                    key_id,
                    key_membership_epoch,
                    manifest_membership_epoch,
                }
            ) if profile == DEFAULT_PROFILE_ID
                && key_id == "content-key-epoch-1"
                && key_membership_epoch == DEFAULT_PROFILE_SYNC_MEMBERSHIP_EPOCH + 1
                && manifest_membership_epoch == DEFAULT_PROFILE_SYNC_MEMBERSHIP_EPOCH
        ));
        assert_eq!(
            destination
                .profile_sync_root(DEFAULT_PROFILE_ID, root_id)
                .unwrap(),
            None
        );
    }

    #[test]
    fn profile_sync_trusted_pull_rejects_unknown_signer() {
        let content_key = ProfileSyncContentKey::from_bytes([17; PROFILE_SYNC_CONTENT_KEY_BYTES]);
        let signer = ProfileSyncDeviceSigner::generate("device-a").unwrap();
        let key_id = "content-key-epoch-1";
        let root_id = "settings/latest";
        let manifest_object_id = "manifest-object-1";
        let manifest = ProfileSyncManifest {
            profile: DEFAULT_PROFILE_ID.to_string(),
            root_id: root_id.to_string(),
            schema_version: PROFILE_SYNC_MANIFEST_SCHEMA_VERSION,
            membership_epoch: DEFAULT_PROFILE_SYNC_MEMBERSHIP_EPOCH,
            current_snapshot_object_id: None,
            tail_change_object_ids: Vec::new(),
            included_domains: vec![SYNC_DOMAIN_SETTINGS.to_string()],
            device_frontiers: vec![ProfileSyncDeviceFrontier {
                device_id: "device-a".to_string(),
                latest_sequence: 1,
                latest_change_object_id: None,
            }],
            retention_policy: ProfileSyncRetentionPolicy::default(),
            created_at: 101,
        };
        let mut source = InMemoryProfileSyncObjectSource::default();
        source.insert_object(
            DEFAULT_PROFILE_ID,
            manifest_object_id,
            sign_test_sync_object(
                DEFAULT_PROFILE_ID,
                SYNC_DOMAIN_SETTINGS,
                PROFILE_SYNC_MANIFEST_OBJECT_KIND,
                key_id,
                serde_json::to_vec(&manifest).unwrap().as_slice(),
                &content_key,
                &signer,
                41,
            ),
        );
        source.publish_root(DEFAULT_PROFILE_ID, root_id, manifest_object_id);
        let destination_path =
            test_dir("sync-trusted-unknown-signer").join(DEFAULT_DATABASE_FILE_NAME);
        let destination =
            SlateProfileDatabase::open_resolved_with_device_id(destination_path, "device-b")
                .unwrap();

        let error = destination
            .pull_and_apply_trusted_signed_settings_manifest_objects(
                &source,
                DEFAULT_PROFILE_ID,
                root_id,
                &content_key,
                key_id,
            )
            .unwrap_err();

        assert!(matches!(
            error,
            ProfileSyncTrustedPullApplyError::Pull(ProfileSyncTrustedPullError::Open(
                ProfileSyncTrustedOpenError::UntrustedDevice { profile, device_id }
            )) if profile == DEFAULT_PROFILE_ID && device_id == "device-a"
        ));
        assert_eq!(
            destination
                .profile_sync_root(DEFAULT_PROFILE_ID, root_id)
                .unwrap(),
            None
        );
    }

    #[test]
    fn profile_sync_trusted_pull_rejects_stored_key_mismatch() {
        let content_key = ProfileSyncContentKey::from_bytes([18; PROFILE_SYNC_CONTENT_KEY_BYTES]);
        let signer = ProfileSyncDeviceSigner::generate("device-a").unwrap();
        let wrong_public_key = ProfileSyncDeviceSigner::generate("device-a")
            .unwrap()
            .public_key()
            .unwrap();
        let key_id = "content-key-epoch-1";
        let root_id = "settings/latest";
        let manifest_object_id = "manifest-object-1";
        let manifest = ProfileSyncManifest {
            profile: DEFAULT_PROFILE_ID.to_string(),
            root_id: root_id.to_string(),
            schema_version: PROFILE_SYNC_MANIFEST_SCHEMA_VERSION,
            membership_epoch: DEFAULT_PROFILE_SYNC_MEMBERSHIP_EPOCH,
            current_snapshot_object_id: None,
            tail_change_object_ids: Vec::new(),
            included_domains: vec![SYNC_DOMAIN_SETTINGS.to_string()],
            device_frontiers: vec![ProfileSyncDeviceFrontier {
                device_id: "device-a".to_string(),
                latest_sequence: 1,
                latest_change_object_id: None,
            }],
            retention_policy: ProfileSyncRetentionPolicy::default(),
            created_at: 101,
        };
        let mut source = InMemoryProfileSyncObjectSource::default();
        source.insert_object(
            DEFAULT_PROFILE_ID,
            manifest_object_id,
            sign_test_sync_object(
                DEFAULT_PROFILE_ID,
                SYNC_DOMAIN_SETTINGS,
                PROFILE_SYNC_MANIFEST_OBJECT_KIND,
                key_id,
                serde_json::to_vec(&manifest).unwrap().as_slice(),
                &content_key,
                &signer,
                51,
            ),
        );
        source.publish_root(DEFAULT_PROFILE_ID, root_id, manifest_object_id);
        let destination_path =
            test_dir("sync-trusted-key-mismatch").join(DEFAULT_DATABASE_FILE_NAME);
        let destination =
            SlateProfileDatabase::open_resolved_with_device_id(destination_path, "device-b")
                .unwrap();
        destination
            .register_sync_device_public_key(&SyncDevicePublicKeyRegistration {
                profile: DEFAULT_PROFILE_ID.to_string(),
                public_key: wrong_public_key,
                membership_epoch: DEFAULT_PROFILE_SYNC_MEMBERSHIP_EPOCH,
            })
            .unwrap();

        let error = destination
            .pull_and_apply_trusted_signed_settings_manifest_objects(
                &source,
                DEFAULT_PROFILE_ID,
                root_id,
                &content_key,
                key_id,
            )
            .unwrap_err();

        assert!(matches!(
            error,
            ProfileSyncTrustedPullApplyError::Pull(ProfileSyncTrustedPullError::Open(
                ProfileSyncTrustedOpenError::SyncObject(SyncObjectError::DeviceKeyMismatch {
                    expected_device_id,
                    actual_device_id,
                })
            )) if expected_device_id == "device-a" && actual_device_id == "device-a"
        ));
        assert_eq!(
            destination
                .profile_sync_root(DEFAULT_PROFILE_ID, root_id)
                .unwrap(),
            None
        );
    }

    #[test]
    fn profile_sync_trusted_pull_rejects_signer_after_manifest_epoch() {
        let content_key = ProfileSyncContentKey::from_bytes([19; PROFILE_SYNC_CONTENT_KEY_BYTES]);
        let signer = ProfileSyncDeviceSigner::generate("device-a").unwrap();
        let trusted_public_key = signer.public_key().unwrap();
        let key_id = "content-key-epoch-1";
        let root_id = "settings/latest";
        let manifest_object_id = "manifest-object-1";
        let manifest = ProfileSyncManifest {
            profile: DEFAULT_PROFILE_ID.to_string(),
            root_id: root_id.to_string(),
            schema_version: PROFILE_SYNC_MANIFEST_SCHEMA_VERSION,
            membership_epoch: DEFAULT_PROFILE_SYNC_MEMBERSHIP_EPOCH,
            current_snapshot_object_id: None,
            tail_change_object_ids: Vec::new(),
            included_domains: vec![SYNC_DOMAIN_SETTINGS.to_string()],
            device_frontiers: vec![ProfileSyncDeviceFrontier {
                device_id: "device-a".to_string(),
                latest_sequence: 1,
                latest_change_object_id: None,
            }],
            retention_policy: ProfileSyncRetentionPolicy::default(),
            created_at: 101,
        };
        let mut source = InMemoryProfileSyncObjectSource::default();
        source.insert_object(
            DEFAULT_PROFILE_ID,
            manifest_object_id,
            sign_test_sync_object(
                DEFAULT_PROFILE_ID,
                SYNC_DOMAIN_SETTINGS,
                PROFILE_SYNC_MANIFEST_OBJECT_KIND,
                key_id,
                serde_json::to_vec(&manifest).unwrap().as_slice(),
                &content_key,
                &signer,
                61,
            ),
        );
        source.publish_root(DEFAULT_PROFILE_ID, root_id, manifest_object_id);
        let destination_path =
            test_dir("sync-trusted-future-key-epoch").join(DEFAULT_DATABASE_FILE_NAME);
        let destination =
            SlateProfileDatabase::open_resolved_with_device_id(destination_path, "device-b")
                .unwrap();
        destination
            .register_sync_device_public_key(&SyncDevicePublicKeyRegistration {
                profile: DEFAULT_PROFILE_ID.to_string(),
                public_key: trusted_public_key,
                membership_epoch: DEFAULT_PROFILE_SYNC_MEMBERSHIP_EPOCH + 1,
            })
            .unwrap();

        let error = destination
            .pull_and_apply_trusted_signed_settings_manifest_objects(
                &source,
                DEFAULT_PROFILE_ID,
                root_id,
                &content_key,
                key_id,
            )
            .unwrap_err();

        assert!(matches!(
            error,
            ProfileSyncTrustedPullApplyError::Pull(ProfileSyncTrustedPullError::Open(
                ProfileSyncTrustedOpenError::UnauthorizedDeviceEpoch {
                    profile,
                    device_id,
                    key_membership_epoch,
                    manifest_membership_epoch,
                }
            )) if profile == DEFAULT_PROFILE_ID
                && device_id == "device-a"
                && key_membership_epoch == DEFAULT_PROFILE_SYNC_MEMBERSHIP_EPOCH + 1
                && manifest_membership_epoch == DEFAULT_PROFILE_SYNC_MEMBERSHIP_EPOCH
        ));
        assert_eq!(
            destination
                .profile_sync_root(DEFAULT_PROFILE_ID, root_id)
                .unwrap(),
            None
        );
    }

    #[test]
    fn profile_sync_pull_and_apply_returns_none_without_published_root() {
        let content_key = ProfileSyncContentKey::from_bytes([14; PROFILE_SYNC_CONTENT_KEY_BYTES]);
        let signer = ProfileSyncDeviceSigner::generate("device-a").unwrap();
        let trusted_public_key = signer.public_key().unwrap();
        let source = InMemoryProfileSyncObjectSource::default();
        let destination_path = test_dir("sync-pull-no-root").join(DEFAULT_DATABASE_FILE_NAME);
        let destination =
            SlateProfileDatabase::open_resolved_with_device_id(destination_path, "device-b")
                .unwrap();

        let applied = destination
            .pull_and_apply_signed_settings_manifest_objects(
                &source,
                DEFAULT_PROFILE_ID,
                "settings/latest",
                &content_key,
                &trusted_public_key,
                "content-key-epoch-1",
            )
            .unwrap();

        assert_eq!(applied, None);
        assert_eq!(
            destination
                .profile_sync_root(DEFAULT_PROFILE_ID, "settings/latest")
                .unwrap(),
            None
        );
    }

    #[test]
    fn profile_sync_pull_and_apply_surfaces_manifest_validation_errors() {
        let content_key = ProfileSyncContentKey::from_bytes([15; PROFILE_SYNC_CONTENT_KEY_BYTES]);
        let signer = ProfileSyncDeviceSigner::generate("device-a").unwrap();
        let trusted_public_key = signer.public_key().unwrap();
        let key_id = "content-key-epoch-1";
        let root_id = "settings/latest";
        let snapshot_object_id = "snapshot-object-1";
        let manifest_object_id = "manifest-object-1";
        let snapshot = ProfileSyncSettingsSnapshot {
            profile: "other-profile".to_string(),
            schema_version: PROFILE_SYNC_SETTINGS_SNAPSHOT_SCHEMA_VERSION,
            covers_revision: 1,
            included_domains: vec![SYNC_DOMAIN_SETTINGS.to_string()],
            values: Vec::new(),
            created_at: 100,
        };
        let manifest = ProfileSyncManifest {
            profile: DEFAULT_PROFILE_ID.to_string(),
            root_id: root_id.to_string(),
            schema_version: PROFILE_SYNC_MANIFEST_SCHEMA_VERSION,
            membership_epoch: DEFAULT_PROFILE_SYNC_MEMBERSHIP_EPOCH,
            current_snapshot_object_id: Some(snapshot_object_id.to_string()),
            tail_change_object_ids: Vec::new(),
            included_domains: vec![SYNC_DOMAIN_SETTINGS.to_string()],
            device_frontiers: Vec::new(),
            retention_policy: ProfileSyncRetentionPolicy::default(),
            created_at: 101,
        };
        let mut source = InMemoryProfileSyncObjectSource::default();
        source.insert_object(
            DEFAULT_PROFILE_ID,
            snapshot_object_id,
            sign_test_sync_object(
                DEFAULT_PROFILE_ID,
                SYNC_DOMAIN_SETTINGS,
                PROFILE_SYNC_SETTINGS_SNAPSHOT_OBJECT_KIND,
                key_id,
                serde_json::to_vec(&snapshot).unwrap().as_slice(),
                &content_key,
                &signer,
                21,
            ),
        );
        source.insert_object(
            DEFAULT_PROFILE_ID,
            manifest_object_id,
            sign_test_sync_object(
                DEFAULT_PROFILE_ID,
                SYNC_DOMAIN_SETTINGS,
                PROFILE_SYNC_MANIFEST_OBJECT_KIND,
                key_id,
                serde_json::to_vec(&manifest).unwrap().as_slice(),
                &content_key,
                &signer,
                22,
            ),
        );
        source.publish_root(DEFAULT_PROFILE_ID, root_id, manifest_object_id);
        let destination_path =
            test_dir("sync-pull-invalid-manifest").join(DEFAULT_DATABASE_FILE_NAME);
        let destination =
            SlateProfileDatabase::open_resolved_with_device_id(destination_path, "device-b")
                .unwrap();

        let error = destination
            .pull_and_apply_signed_settings_manifest_objects(
                &source,
                DEFAULT_PROFILE_ID,
                root_id,
                &content_key,
                &trusted_public_key,
                key_id,
            )
            .unwrap_err();

        assert!(matches!(
            error,
            ProfileSyncPullApplyError::Storage(StorageError::InvalidProfileSyncManifest(_))
        ));
        assert_eq!(
            destination
                .profile_sync_root(DEFAULT_PROFILE_ID, root_id)
                .unwrap(),
            None
        );
    }

    #[test]
    fn profile_sync_pull_rejects_source_object_id_mismatch() {
        let content_key = ProfileSyncContentKey::from_bytes([13; PROFILE_SYNC_CONTENT_KEY_BYTES]);
        let signer = ProfileSyncDeviceSigner::generate("device-a").unwrap();
        let trusted_public_key = signer.public_key().unwrap();
        let mut source = InMemoryProfileSyncObjectSource::default();
        source.publish_root(DEFAULT_PROFILE_ID, "settings/latest", "manifest-object-1");
        source.objects.insert(
            (
                DEFAULT_PROFILE_ID.to_string(),
                "manifest-object-1".to_string(),
            ),
            ProfileSyncObjectBytes {
                object_id: "wrong-object".to_string(),
                bytes: Vec::new(),
            },
        );

        let error = pull_signed_profile_sync_settings_manifest_objects(
            &source,
            DEFAULT_PROFILE_ID,
            "settings/latest",
            &content_key,
            &trusted_public_key,
            "content-key-epoch-1",
        )
        .unwrap_err();

        assert!(matches!(
            error,
            ProfileSyncPullError::ObjectIdMismatch { expected, actual }
                if expected == "manifest-object-1" && actual == "wrong-object"
        ));
    }

    #[test]
    fn profile_sync_manifest_decodes_default_membership_and_retention_metadata() {
        let payload = serde_json::json!({
            "profile": DEFAULT_PROFILE_ID,
            "root_id": "settings/latest",
            "current_snapshot_object_id": null,
            "tail_change_object_ids": ["change-object-1"],
            "included_domains": [SYNC_DOMAIN_SETTINGS],
            "device_frontiers": [{
                "device_id": "device-a",
                "latest_sequence": 1,
                "latest_change_object_id": "change-object-1"
            }],
            "created_at": 1234
        });
        let manifest: ProfileSyncManifest = serde_json::from_value(payload).unwrap();

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
    }

    #[test]
    fn database_initialization_creates_schema_and_persists_profile_state() {
        let database_path = test_dir("db").join(DEFAULT_DATABASE_FILE_NAME);
        let database = SlateProfileDatabase::open_resolved(database_path.clone()).unwrap();

        assert!(database_path.is_file());
        let default_bookmarks = database.bookmarks(DEFAULT_PROFILE_ID).unwrap();
        assert_eq!(default_bookmarks.len(), DEFAULT_HOME_BOOKMARKS.len());
        assert_eq!(
            default_bookmarks[0].title.as_deref(),
            Some("Wikipedia on IPFS")
        );
        assert_eq!(
            default_bookmarks[0].url,
            "ipns://en.wikipedia-on-ipfs.org/wiki/"
        );
        assert_eq!(default_bookmarks[1].title.as_deref(), Some("OpenStreetMap"));
        assert_eq!(default_bookmarks[1].url, "https://www.openstreetmap.org/");
        assert!(
            database
                .sync_setting_text_events_after_for_domain(
                    DEFAULT_PROFILE_ID,
                    SYNC_DOMAIN_BOOKMARKS,
                    0,
                    10
                )
                .unwrap()
                .is_empty()
        );

        assert_eq!(
            database.ensure_setting_f32("chrome.zoom", 0.9).unwrap(),
            0.9
        );
        database.set_setting_f32("chrome.zoom", 1.05).unwrap();
        assert_eq!(database.get_setting_f32("chrome.zoom").unwrap(), Some(1.05));

        database
            .record_history_visit("default", "https://example.com/", Some("Example"))
            .unwrap();
        database
            .record_history_visit("default", "https://example.com/", None)
            .unwrap();

        let history = database.recent_history("default", 10).unwrap();
        assert_eq!(history.len(), 1);
        assert_eq!(history[0].url, "https://example.com/");
        assert_eq!(history[0].title.as_deref(), Some("Example"));
        assert_eq!(history[0].visit_count, 2);

        database
            .update_history_title("default", "https://example.com/", Some("Example Updated"))
            .unwrap();
        let history = database.recent_history("default", 10).unwrap();
        assert_eq!(history[0].title.as_deref(), Some("Example Updated"));
        assert_eq!(history[0].visit_count, 2);

        database
            .upsert_bookmark(&BookmarkUpdate {
                profile: "testing".into(),
                url: "https://example.com/".into(),
                title: Some("Example".into()),
                folder: Some("Research".into()),
                position: 1,
                favicon_key: Some("favicon:example".into()),
            })
            .unwrap();
        let bookmarks = database.bookmarks("testing").unwrap();
        assert_eq!(bookmarks.len(), 1);
        assert_eq!(bookmarks[0].url, "https://example.com/");
        assert_eq!(bookmarks[0].folder.as_deref(), Some("Research"));

        database
            .upsert_cookie(&CookieUpdate {
                profile: "default".into(),
                domain: "example.com".into(),
                path: "/".into(),
                name: "session".into(),
                value: "abc123".into(),
                expires_at: Some(2_000_000_000),
                is_secure: true,
                is_http_only: true,
                same_site: Some("Lax".into()),
            })
            .unwrap();
        let cookies = database
            .cookies_for_domain("default", "example.com")
            .unwrap();
        assert_eq!(cookies.len(), 1);
        assert_eq!(cookies[0].name, "session");
        assert_eq!(cookies[0].value, "abc123");
        assert!(cookies[0].is_secure);
        assert!(cookies[0].is_http_only);
        assert_eq!(cookies[0].same_site.as_deref(), Some("Lax"));

        database
            .set_blob(
                "default",
                "favicon:example",
                Some("image/png"),
                &[1, 2, 3, 4],
            )
            .unwrap();
        let blob = database
            .get_blob("default", "favicon:example")
            .unwrap()
            .unwrap();
        assert_eq!(blob.media_type.as_deref(), Some("image/png"));
        assert_eq!(blob.data, vec![1, 2, 3, 4]);

        database
            .delete_cookie("default", "example.com", "/", "session")
            .unwrap();
        assert!(
            database
                .cookies_for_domain("default", "example.com")
                .unwrap()
                .is_empty()
        );

        database
            .remove_bookmark("testing", "https://example.com/")
            .unwrap();
        assert!(database.bookmarks("testing").unwrap().is_empty());
    }

    #[test]
    fn database_initialization_registers_sync_domains_and_local_device() {
        let database_path = test_dir("sync-domains").join(DEFAULT_DATABASE_FILE_NAME);
        let database = SlateProfileDatabase::open_resolved(database_path).unwrap();

        let domains = database.app_sync_domains(DEFAULT_PROFILE_ID).unwrap();
        assert_eq!(domains.len(), DEFAULT_APP_SYNC_DOMAINS.len());
        assert!(domains.iter().any(|domain| {
            domain.domain == SYNC_DOMAIN_SETTINGS
                && domain.enabled
                && domain.privacy_classification == "low-risk"
        }));
        assert!(domains.iter().any(|domain| {
            domain.domain == SYNC_DOMAIN_FILES
                && !domain.enabled
                && domain.privacy_classification == "content"
                && domain.sync_content
        }));
        assert!(domains.iter().any(|domain| {
            domain.domain == SYNC_DOMAIN_CONTACTS
                && !domain.enabled
                && domain.privacy_classification == "sensitive"
                && !domain.sync_content
        }));
        assert!(domains.iter().any(|domain| {
            domain.domain == SYNC_DOMAIN_DOWNLOADS
                && domain.enabled
                && domain.privacy_classification == "metadata"
        }));

        let enabled_domains = database
            .enabled_app_sync_domains(DEFAULT_PROFILE_ID)
            .unwrap();
        let enabled_domain_names = enabled_domains
            .iter()
            .map(|domain| domain.domain.as_str())
            .collect::<Vec<_>>();
        assert_eq!(
            enabled_domain_names,
            vec![
                SYNC_DOMAIN_BOOKMARKS,
                SYNC_DOMAIN_DOWNLOADS,
                SYNC_DOMAIN_SETTINGS,
            ]
        );

        let devices = database.sync_devices(DEFAULT_PROFILE_ID).unwrap();
        assert_eq!(devices.len(), 1);
        assert_eq!(devices[0].device_id, DEFAULT_SYNC_DEVICE_ID);
        assert_eq!(devices[0].membership_epoch, 1);
        assert!(!devices[0].provider_authority);
    }

    #[test]
    fn rail_app_sync_domains_match_seeded_storage_domains() {
        let database_path = test_dir("rail-sync-domains").join(DEFAULT_DATABASE_FILE_NAME);
        let database = SlateProfileDatabase::open_resolved(database_path).unwrap();
        let seeded_domains = database
            .app_sync_domains(DEFAULT_PROFILE_ID)
            .unwrap()
            .into_iter()
            .map(|domain| (domain.domain.clone(), domain))
            .collect::<BTreeMap<_, _>>();

        for app in slate_apps::default_apps() {
            let seeded = seeded_domains
                .get(app.sync.domain)
                .unwrap_or_else(|| panic!("missing seeded storage sync domain for {}", app.label));
            assert_eq!(seeded.schema_version, 1);
            assert_eq!(
                seeded.privacy_classification,
                app.sync.privacy_classification.as_str()
            );
            assert_eq!(seeded.sync_content, app.sync.sync_content);
            assert_eq!(seeded.enabled, app.sync.default_enabled);
        }

        assert!(
            seeded_domains.contains_key(SYNC_DOMAIN_STORAGE),
            "storage remains seeded as a future sync domain"
        );
        assert!(
            slate_apps::app_for_sync_domain(SYNC_DOMAIN_STORAGE).is_none(),
            "storage should not appear as a visible rail app until its surface exists"
        );
    }

    #[test]
    fn default_app_sync_domains_can_be_seeded_for_custom_profiles() {
        let database_path = test_dir("sync-domain-custom-profile").join(DEFAULT_DATABASE_FILE_NAME);
        let database = SlateProfileDatabase::open_resolved(database_path).unwrap();

        database.ensure_default_app_sync_domains("work").unwrap();

        let domains = database.app_sync_domains("work").unwrap();
        assert_eq!(domains.len(), DEFAULT_APP_SYNC_DOMAINS.len());
        assert!(domains.iter().any(|domain| {
            domain.domain == SYNC_DOMAIN_SETTINGS
                && domain.enabled
                && domain.privacy_classification == "low-risk"
        }));
        assert!(domains.iter().any(|domain| {
            domain.domain == SYNC_DOMAIN_CALENDAR
                && !domain.enabled
                && domain.privacy_classification == "sensitive"
        }));

        let enabled_domain_names = database
            .enabled_app_sync_domains("work")
            .unwrap()
            .into_iter()
            .map(|domain| domain.domain)
            .collect::<Vec<_>>();
        assert_eq!(
            enabled_domain_names,
            vec![
                SYNC_DOMAIN_BOOKMARKS.to_string(),
                SYNC_DOMAIN_DOWNLOADS.to_string(),
                SYNC_DOMAIN_SETTINGS.to_string(),
            ]
        );
    }

    #[test]
    fn default_sync_domain_seeding_preserves_user_enabled_choices() {
        let database_path = test_dir("sync-domain-enabled-choice").join(DEFAULT_DATABASE_FILE_NAME);
        let database = SlateProfileDatabase::open_resolved(database_path.clone()).unwrap();
        database
            .register_app_sync_domain(&AppSyncDomainRegistration {
                profile: DEFAULT_PROFILE_ID.to_string(),
                domain: SYNC_DOMAIN_CALENDAR.to_string(),
                schema_version: 1,
                enabled: true,
                privacy_classification: "sensitive".to_string(),
                sync_content: false,
            })
            .unwrap();
        drop(database);

        let reopened = SlateProfileDatabase::open_resolved(database_path).unwrap();
        let calendar = reopened
            .app_sync_domains(DEFAULT_PROFILE_ID)
            .unwrap()
            .into_iter()
            .find(|domain| domain.domain == SYNC_DOMAIN_CALENDAR)
            .expect("calendar sync domain should be seeded");
        assert!(calendar.enabled);

        let enabled_domain_names = reopened
            .enabled_app_sync_domains(DEFAULT_PROFILE_ID)
            .unwrap()
            .into_iter()
            .map(|domain| domain.domain)
            .collect::<Vec<_>>();
        assert!(enabled_domain_names.contains(&SYNC_DOMAIN_CALENDAR.to_string()));
    }

    #[test]
    fn database_can_use_distinct_local_sync_device_id() {
        let database_path = test_dir("sync-device-id").join(DEFAULT_DATABASE_FILE_NAME);
        let database =
            SlateProfileDatabase::open_resolved_with_device_id(database_path, "device-a").unwrap();

        assert_eq!(database.local_sync_device_id(), "device-a");

        let devices = database.sync_devices(DEFAULT_PROFILE_ID).unwrap();
        assert_eq!(devices.len(), 1);
        assert_eq!(devices[0].device_id, "device-a");

        let change = database.set_setting_text("ui.theme", "teal");
        assert!(change.is_ok());
        let changes = database
            .sync_changes_after(DEFAULT_PROFILE_ID, 0, 100)
            .unwrap();
        assert!(
            changes.iter().any(|change| {
                change.entity_key == "ui.theme" && change.device_id == "device-a"
            })
        );
    }

    #[test]
    fn database_rejects_invalid_local_sync_device_id() {
        let database_path = test_dir("invalid-sync-device-id").join(DEFAULT_DATABASE_FILE_NAME);
        let error =
            SlateProfileDatabase::open_resolved_with_device_id(database_path, "../device-a")
                .unwrap_err();

        assert!(matches!(error, StorageError::InvalidSyncDeviceId(_)));
    }

    #[test]
    fn database_runtime_open_reuses_persistent_local_sync_device_id() {
        let directory = test_dir("persistent-sync-device-id");
        let database_path = directory.join(DEFAULT_DATABASE_FILE_NAME);
        let first =
            SlateProfileDatabase::open_resolved_with_persistent_device_id(database_path.clone())
                .unwrap();
        let first_device_id = first.local_sync_device_id().to_string();
        let sidecar_path = local_sync_device_id_path(&database_path);

        assert!(first_device_id.starts_with("device-"));
        assert_ne!(first_device_id, DEFAULT_SYNC_DEVICE_ID);
        assert_eq!(
            std::fs::read_to_string(&sidecar_path).unwrap().trim(),
            first_device_id
        );

        let reopened =
            SlateProfileDatabase::open_resolved_with_persistent_device_id(database_path).unwrap();
        assert_eq!(reopened.local_sync_device_id(), first_device_id);

        let fixture_database =
            SlateProfileDatabase::open_resolved(directory.join("fixture-settings.db")).unwrap();
        assert_eq!(
            fixture_database.local_sync_device_id(),
            DEFAULT_SYNC_DEVICE_ID
        );
    }

    #[test]
    fn database_runtime_open_rejects_invalid_persistent_local_sync_device_id() {
        let database_path =
            test_dir("invalid-persistent-sync-device-id").join(DEFAULT_DATABASE_FILE_NAME);
        let sidecar_path = local_sync_device_id_path(&database_path);
        std::fs::write(&sidecar_path, "../device-a\n").unwrap();

        let error = SlateProfileDatabase::open_resolved_with_persistent_device_id(database_path)
            .unwrap_err();

        assert!(matches!(error, StorageError::InvalidSyncDeviceId(_)));
    }

    #[test]
    fn sync_device_public_keys_round_trip_and_update() {
        let database_path = test_dir("sync-device-public-key").join(DEFAULT_DATABASE_FILE_NAME);
        let database = SlateProfileDatabase::open_resolved(database_path).unwrap();
        let first_key = ProfileSyncDeviceSigner::generate("device-a")
            .unwrap()
            .public_key()
            .unwrap();
        let first_record = database
            .register_sync_device_public_key(&SyncDevicePublicKeyRegistration {
                profile: DEFAULT_PROFILE_ID.to_string(),
                public_key: first_key.clone(),
                membership_epoch: 1,
            })
            .unwrap();

        assert_eq!(first_record.profile, DEFAULT_PROFILE_ID);
        assert_eq!(first_record.public_key, first_key);
        assert_eq!(first_record.membership_epoch, 1);
        assert!(first_record.trusted);
        assert_eq!(
            database
                .sync_device_public_key(DEFAULT_PROFILE_ID, "device-a")
                .unwrap()
                .expect("trusted device key"),
            first_record
        );
        assert_eq!(
            database
                .sync_device_public_keys(DEFAULT_PROFILE_ID)
                .unwrap()
                .len(),
            1
        );
        let first_device = database
            .sync_devices(DEFAULT_PROFILE_ID)
            .unwrap()
            .into_iter()
            .find(|device| device.device_id == "device-a")
            .expect("trusted key registration records sync device");
        assert_eq!(first_device.membership_epoch, 1);
        assert_eq!(first_device.label, None);
        assert!(!first_device.provider_authority);

        database
            .register_sync_device(&SyncDeviceRegistration {
                profile: DEFAULT_PROFILE_ID.to_string(),
                device_id: "device-a".to_string(),
                label: Some("Pinned provider".to_string()),
                membership_epoch: 1,
                provider_authority: true,
            })
            .unwrap();

        let second_key = ProfileSyncDeviceSigner::generate("device-a")
            .unwrap()
            .public_key()
            .unwrap();
        let second_record = database
            .register_sync_device_public_key(&SyncDevicePublicKeyRegistration {
                profile: DEFAULT_PROFILE_ID.to_string(),
                public_key: second_key.clone(),
                membership_epoch: 2,
            })
            .unwrap();

        assert_eq!(second_record.public_key, second_key);
        assert_eq!(second_record.membership_epoch, 2);
        assert!(second_record.trusted);
        assert_eq!(second_record.created_at, first_record.created_at);
        assert!(second_record.updated_at >= first_record.updated_at);
        assert_eq!(
            database
                .sync_device_public_keys(DEFAULT_PROFILE_ID)
                .unwrap()
                .len(),
            1
        );
        let updated_device = database
            .sync_devices(DEFAULT_PROFILE_ID)
            .unwrap()
            .into_iter()
            .find(|device| device.device_id == "device-a")
            .expect("trusted key update keeps sync device metadata");
        assert_eq!(updated_device.membership_epoch, 2);
        assert_eq!(updated_device.label.as_deref(), Some("Pinned provider"));
        assert!(updated_device.provider_authority);

        let revoked = database
            .set_sync_device_public_key_trusted(DEFAULT_PROFILE_ID, "device-a", false)
            .unwrap()
            .expect("revoked device key");
        assert!(!revoked.trusted);
        assert_eq!(revoked.public_key, second_key);
        assert_eq!(
            database
                .sync_device_public_key(DEFAULT_PROFILE_ID, "device-a")
                .unwrap()
                .expect("revoked trusted device key")
                .trusted,
            false
        );
        let restored = database
            .register_sync_device_public_key(&SyncDevicePublicKeyRegistration {
                profile: DEFAULT_PROFILE_ID.to_string(),
                public_key: second_key,
                membership_epoch: 3,
            })
            .unwrap();
        assert!(restored.trusted);
        assert_eq!(restored.membership_epoch, 3);
        assert!(
            database
                .set_sync_device_public_key_trusted(DEFAULT_PROFILE_ID, "missing-device", false)
                .unwrap()
                .is_none()
        );
    }

    #[test]
    fn sync_device_public_key_trust_column_is_added_to_existing_databases() {
        let database_path =
            test_dir("sync-device-public-key-migration").join(DEFAULT_DATABASE_FILE_NAME);
        {
            let connection = Connection::open(&database_path).unwrap();
            connection
                .execute_batch(
                    "
                    CREATE TABLE sync_device_public_keys (
                        profile TEXT NOT NULL,
                        device_id TEXT NOT NULL,
                        public_key BLOB NOT NULL,
                        membership_epoch INTEGER NOT NULL DEFAULT 1,
                        created_at INTEGER NOT NULL,
                        updated_at INTEGER NOT NULL,
                        PRIMARY KEY(profile, device_id)
                    );
                    INSERT INTO sync_device_public_keys
                        (profile, device_id, public_key, membership_epoch, created_at, updated_at)
                    VALUES ('default', 'device-a', X'010203', 1, 10, 10);
                    ",
                )
                .unwrap();
        }

        let database = SlateProfileDatabase::open_resolved(database_path).unwrap();
        let migrated = database
            .sync_device_public_key(DEFAULT_PROFILE_ID, "device-a")
            .unwrap()
            .expect("migrated trusted device key");

        assert!(migrated.trusted);
        assert_eq!(migrated.created_at, 10);
        assert_eq!(
            database
                .set_sync_device_public_key_trusted(DEFAULT_PROFILE_ID, "device-a", false)
                .unwrap()
                .expect("revoked migrated key")
                .trusted,
            false
        );
    }

    #[test]
    fn sync_device_public_keys_reject_invalid_device_ids() {
        let database_path =
            test_dir("invalid-sync-device-public-key").join(DEFAULT_DATABASE_FILE_NAME);
        let database = SlateProfileDatabase::open_resolved(database_path).unwrap();
        let error = database
            .register_sync_device_public_key(&SyncDevicePublicKeyRegistration {
                profile: DEFAULT_PROFILE_ID.to_string(),
                public_key: ProfileSyncDevicePublicKey {
                    device_id: "../device-a".to_string(),
                    bytes: vec![1, 2, 3],
                },
                membership_epoch: 1,
            })
            .unwrap_err();
        assert!(
            matches!(error, StorageError::InvalidSyncDeviceId(device_id) if device_id == "../device-a")
        );
        assert!(matches!(
            database.sync_device_public_key(DEFAULT_PROFILE_ID, "../device-a"),
            Err(StorageError::InvalidSyncDeviceId(device_id)) if device_id == "../device-a"
        ));
    }

    #[test]
    fn signed_sync_account_membership_records_round_trip() {
        let database_path = test_dir("sync-account-membership").join(DEFAULT_DATABASE_FILE_NAME);
        let database = SlateProfileDatabase::open_resolved(database_path).unwrap();
        let signer = ProfileSyncDeviceSigner::generate("device-a").unwrap();
        let device_public_key = signer.public_key().unwrap();
        let enroll_record = ProfileSyncMembershipRecord {
            profile: DEFAULT_PROFILE_ID.to_string(),
            record_id: "epoch-1-enroll-device-a".to_string(),
            schema_version: PROFILE_SYNC_MEMBERSHIP_RECORD_SCHEMA_VERSION,
            membership_epoch: 1,
            record_kind: PROFILE_SYNC_MEMBERSHIP_RECORD_KIND_ENROLL_DEVICE.to_string(),
            device_id: "device-a".to_string(),
            device_public_key: Some(device_public_key),
            created_at: 10,
        };
        let signed_enroll = signed_membership_record_bytes(&signer, &enroll_record);

        assert_eq!(
            database
                .sync_account_membership_record_count(DEFAULT_PROFILE_ID)
                .unwrap(),
            0
        );
        let first = database
            .record_signed_sync_account_membership_record(signed_enroll.as_slice())
            .unwrap();
        assert_eq!(
            database
                .sync_account_membership_record_count(DEFAULT_PROFILE_ID)
                .unwrap(),
            1
        );
        let second = database
            .record_signed_sync_account_membership_record(signed_enroll.as_slice())
            .unwrap();
        assert_eq!(
            database
                .sync_account_membership_record_count(DEFAULT_PROFILE_ID)
                .unwrap(),
            1
        );

        assert_eq!(first, second);
        assert_eq!(first.profile, DEFAULT_PROFILE_ID);
        assert_eq!(first.record_id, "epoch-1-enroll-device-a");
        assert_eq!(first.membership_epoch, 1);
        assert_eq!(
            first.record_kind,
            PROFILE_SYNC_MEMBERSHIP_RECORD_KIND_ENROLL_DEVICE
        );
        assert_eq!(first.device_id, "device-a");
        assert_eq!(first.signer_device_id, "device-a");
        assert_eq!(first.signed_record, signed_enroll);
        assert_eq!(
            database
                .sync_account_membership_record(DEFAULT_PROFILE_ID, "epoch-1-enroll-device-a")
                .unwrap()
                .expect("membership record"),
            first
        );

        let revoke_record = ProfileSyncMembershipRecord {
            profile: DEFAULT_PROFILE_ID.to_string(),
            record_id: "epoch-2-revoke-device-b".to_string(),
            schema_version: PROFILE_SYNC_MEMBERSHIP_RECORD_SCHEMA_VERSION,
            membership_epoch: 2,
            record_kind: PROFILE_SYNC_MEMBERSHIP_RECORD_KIND_REVOKE_DEVICE.to_string(),
            device_id: "device-b".to_string(),
            device_public_key: None,
            created_at: 20,
        };
        let signed_revoke = signed_membership_record_bytes(&signer, &revoke_record);
        database
            .record_signed_sync_account_membership_record(signed_revoke.as_slice())
            .unwrap();
        assert_eq!(
            database
                .sync_account_membership_record_count(DEFAULT_PROFILE_ID)
                .unwrap(),
            2
        );

        let records = database
            .sync_account_membership_records(DEFAULT_PROFILE_ID)
            .unwrap();
        assert_eq!(records.len(), 2);
        assert_eq!(records[0].record_id, "epoch-1-enroll-device-a");
        assert_eq!(records[1].record_id, "epoch-2-revoke-device-b");
    }

    #[test]
    fn signed_sync_account_membership_records_apply_to_trusted_device_keys() {
        let database_path =
            test_dir("sync-account-membership-apply").join(DEFAULT_DATABASE_FILE_NAME);
        let database = SlateProfileDatabase::open_resolved(database_path).unwrap();
        let signer_a = ProfileSyncDeviceSigner::generate("device-a").unwrap();
        let signer_b = ProfileSyncDeviceSigner::generate("device-b").unwrap();
        let enroll_a = ProfileSyncMembershipRecord {
            profile: DEFAULT_PROFILE_ID.to_string(),
            record_id: "epoch-1-enroll-device-a".to_string(),
            schema_version: PROFILE_SYNC_MEMBERSHIP_RECORD_SCHEMA_VERSION,
            membership_epoch: 1,
            record_kind: PROFILE_SYNC_MEMBERSHIP_RECORD_KIND_ENROLL_DEVICE.to_string(),
            device_id: "device-a".to_string(),
            device_public_key: Some(signer_a.public_key().unwrap()),
            created_at: 10,
        };
        let signed_enroll_a = signed_membership_record_bytes(&signer_a, &enroll_a);

        let bootstrap = database
            .apply_signed_sync_account_membership_record(signed_enroll_a.as_slice())
            .unwrap();
        assert!(bootstrap.bootstrapped);
        assert!(bootstrap.applied);
        assert!(bootstrap.membership_record.applied_at.is_some());
        assert_eq!(
            bootstrap
                .device_key
                .as_ref()
                .expect("bootstrapped key")
                .public_key,
            signer_a.public_key().unwrap()
        );

        let replay = database
            .apply_signed_sync_account_membership_record(signed_enroll_a.as_slice())
            .unwrap();
        assert!(!replay.applied);
        assert!(replay.membership_record.applied_at.is_some());

        let enroll_b = ProfileSyncMembershipRecord {
            profile: DEFAULT_PROFILE_ID.to_string(),
            record_id: "epoch-2-enroll-device-b".to_string(),
            schema_version: PROFILE_SYNC_MEMBERSHIP_RECORD_SCHEMA_VERSION,
            membership_epoch: 2,
            record_kind: PROFILE_SYNC_MEMBERSHIP_RECORD_KIND_ENROLL_DEVICE.to_string(),
            device_id: "device-b".to_string(),
            device_public_key: Some(signer_b.public_key().unwrap()),
            created_at: 20,
        };
        let signed_enroll_b = signed_membership_record_bytes(&signer_a, &enroll_b);
        let enroll_b_application = database
            .apply_signed_sync_account_membership_record(signed_enroll_b.as_slice())
            .unwrap();
        assert!(!enroll_b_application.bootstrapped);
        assert!(enroll_b_application.applied);
        let enrolled_b_key = enroll_b_application.device_key.expect("enrolled device b");
        assert_eq!(enrolled_b_key.public_key, signer_b.public_key().unwrap());
        assert_eq!(enrolled_b_key.membership_epoch, 2);
        assert!(enrolled_b_key.trusted);

        let revoke_b = ProfileSyncMembershipRecord {
            profile: DEFAULT_PROFILE_ID.to_string(),
            record_id: "epoch-3-revoke-device-b".to_string(),
            schema_version: PROFILE_SYNC_MEMBERSHIP_RECORD_SCHEMA_VERSION,
            membership_epoch: 3,
            record_kind: PROFILE_SYNC_MEMBERSHIP_RECORD_KIND_REVOKE_DEVICE.to_string(),
            device_id: "device-b".to_string(),
            device_public_key: None,
            created_at: 30,
        };
        let signed_revoke_b = signed_membership_record_bytes(&signer_a, &revoke_b);
        let revoke_b_application = database
            .apply_signed_sync_account_membership_record(signed_revoke_b.as_slice())
            .unwrap();
        assert!(revoke_b_application.applied);
        assert!(
            !revoke_b_application
                .device_key
                .expect("revoked device b")
                .trusted
        );

        let replay_enroll_b = database
            .apply_signed_sync_account_membership_record(signed_enroll_b.as_slice())
            .unwrap();
        assert!(!replay_enroll_b.applied);
        assert!(
            !replay_enroll_b
                .device_key
                .expect("device b should stay revoked after replay")
                .trusted
        );
        let devices = database.sync_devices(DEFAULT_PROFILE_ID).unwrap();
        let device_a = devices
            .iter()
            .find(|device| device.device_id == "device-a")
            .expect("membership applies device a metadata");
        assert_eq!(device_a.membership_epoch, 1);
        assert!(!device_a.provider_authority);
        let device_b = devices
            .iter()
            .find(|device| device.device_id == "device-b")
            .expect("membership applies device b metadata");
        assert_eq!(device_b.membership_epoch, 3);
        assert!(!device_b.provider_authority);

        let stale_signer_b = ProfileSyncDeviceSigner::generate("device-b").unwrap();
        let stale_rotate_b = ProfileSyncMembershipRecord {
            profile: DEFAULT_PROFILE_ID.to_string(),
            record_id: "epoch-2-rotate-device-b-stale".to_string(),
            schema_version: PROFILE_SYNC_MEMBERSHIP_RECORD_SCHEMA_VERSION,
            membership_epoch: 2,
            record_kind: PROFILE_SYNC_MEMBERSHIP_RECORD_KIND_ROTATE_DEVICE_KEY.to_string(),
            device_id: "device-b".to_string(),
            device_public_key: Some(stale_signer_b.public_key().unwrap()),
            created_at: 40,
        };
        let stale_error = database
            .apply_signed_sync_account_membership_record(
                signed_membership_record_bytes(&signer_a, &stale_rotate_b).as_slice(),
            )
            .unwrap_err();
        assert!(matches!(
            stale_error,
            StorageError::InvalidProfileSyncMembershipRecord(reason)
                if reason.contains("older than latest applied epoch 3")
        ));
        assert!(
            database
                .sync_account_membership_record(DEFAULT_PROFILE_ID, "epoch-2-rotate-device-b-stale")
                .unwrap()
                .is_none()
        );
        assert!(
            !database
                .sync_device_public_key(DEFAULT_PROFILE_ID, "device-b")
                .unwrap()
                .expect("device b key remains present")
                .trusted
        );

        let conflicting_signer_b = ProfileSyncDeviceSigner::generate("device-b").unwrap();
        let conflicting_rotate_b = ProfileSyncMembershipRecord {
            profile: DEFAULT_PROFILE_ID.to_string(),
            record_id: "epoch-3-rotate-device-b-conflict".to_string(),
            schema_version: PROFILE_SYNC_MEMBERSHIP_RECORD_SCHEMA_VERSION,
            membership_epoch: 3,
            record_kind: PROFILE_SYNC_MEMBERSHIP_RECORD_KIND_ROTATE_DEVICE_KEY.to_string(),
            device_id: "device-b".to_string(),
            device_public_key: Some(conflicting_signer_b.public_key().unwrap()),
            created_at: 50,
        };
        let conflict_error = database
            .apply_signed_sync_account_membership_record(
                signed_membership_record_bytes(&signer_a, &conflicting_rotate_b).as_slice(),
            )
            .unwrap_err();
        assert!(matches!(
            conflict_error,
            StorageError::InvalidProfileSyncMembershipRecord(reason)
                if reason.contains("conflicts with already-applied record epoch-3-revoke-device-b")
        ));
        assert!(
            database
                .sync_account_membership_record(
                    DEFAULT_PROFILE_ID,
                    "epoch-3-rotate-device-b-conflict"
                )
                .unwrap()
                .is_none()
        );
        assert!(
            !database
                .sync_device_public_key(DEFAULT_PROFILE_ID, "device-b")
                .unwrap()
                .expect("device b remains present after conflicting same-epoch record")
                .trusted
        );
    }

    #[test]
    fn profile_sync_enrollment_bundle_imports_ordered_membership_chain_for_local_device() {
        let signer_a = ProfileSyncDeviceSigner::generate("device-a").unwrap();
        let signer_b = ProfileSyncDeviceSigner::generate("device-b").unwrap();
        let enroll_a = ProfileSyncMembershipRecord {
            profile: DEFAULT_PROFILE_ID.to_string(),
            record_id: "epoch-1-enroll-device-a".to_string(),
            schema_version: PROFILE_SYNC_MEMBERSHIP_RECORD_SCHEMA_VERSION,
            membership_epoch: 1,
            record_kind: PROFILE_SYNC_MEMBERSHIP_RECORD_KIND_ENROLL_DEVICE.to_string(),
            device_id: "device-a".to_string(),
            device_public_key: Some(signer_a.public_key().unwrap()),
            created_at: 10,
        };
        let enroll_b = ProfileSyncMembershipRecord {
            profile: DEFAULT_PROFILE_ID.to_string(),
            record_id: "epoch-2-enroll-device-b".to_string(),
            schema_version: PROFILE_SYNC_MEMBERSHIP_RECORD_SCHEMA_VERSION,
            membership_epoch: 2,
            record_kind: PROFILE_SYNC_MEMBERSHIP_RECORD_KIND_ENROLL_DEVICE.to_string(),
            device_id: "device-b".to_string(),
            device_public_key: Some(signer_b.public_key().unwrap()),
            created_at: 20,
        };
        let signed_enroll_a = signed_membership_record_bytes(&signer_a, &enroll_a);
        let signed_enroll_b = signed_membership_record_bytes(&signer_a, &enroll_b);
        let bundle = ProfileSyncEnrollmentBundle::new_device_enrollment(
            DEFAULT_PROFILE_ID,
            "device-b",
            vec![signed_enroll_a.clone(), signed_enroll_b.clone()],
            30,
        )
        .unwrap();
        let encoded = bundle.to_bytes().unwrap();
        let decoded = ProfileSyncEnrollmentBundle::from_bytes(encoded.as_slice()).unwrap();
        assert_eq!(decoded, bundle);

        let database_path =
            test_dir("sync-enrollment-bundle-import").join(DEFAULT_DATABASE_FILE_NAME);
        let database =
            SlateProfileDatabase::open_resolved_with_device_id(database_path, "device-b").unwrap();
        let applications = database
            .apply_profile_sync_enrollment_bundle(&decoded)
            .unwrap();

        assert_eq!(applications.len(), 2);
        assert!(applications[0].bootstrapped);
        assert!(applications[0].applied);
        assert_eq!(applications[0].membership_record.device_id, "device-a");
        assert!(!applications[1].bootstrapped);
        assert!(applications[1].applied);
        assert_eq!(applications[1].membership_record.device_id, "device-b");
        assert_eq!(
            database
                .sync_account_membership_record_count(DEFAULT_PROFILE_ID)
                .unwrap(),
            2
        );
        assert_eq!(
            database
                .sync_device_public_key(DEFAULT_PROFILE_ID, "device-b")
                .unwrap()
                .expect("imported target device key")
                .public_key,
            signer_b.public_key().unwrap()
        );

        let replay = database
            .apply_profile_sync_enrollment_bundle(&decoded)
            .unwrap();
        assert_eq!(replay.len(), 2);
        assert!(replay.iter().all(|application| !application.applied));
    }

    #[test]
    fn profile_sync_device_enrollment_request_round_trips_without_secret_material() {
        let request =
            ProfileSyncDeviceEnrollmentRequest::new(DEFAULT_PROFILE_ID, "device-b", 123).unwrap();
        let encoded = request.to_bytes().unwrap();
        let decoded = ProfileSyncDeviceEnrollmentRequest::from_bytes(encoded.as_slice()).unwrap();
        let json: serde_json::Value = serde_json::from_slice(encoded.as_slice()).unwrap();

        assert_eq!(decoded, request);
        assert_eq!(json["profile"], DEFAULT_PROFILE_ID);
        assert_eq!(json["device_id"], "device-b");
        assert_eq!(
            json["schema_version"],
            PROFILE_SYNC_DEVICE_ENROLLMENT_REQUEST_SCHEMA_VERSION
        );
        assert!(json.get("secret").is_none());
        assert!(json.get("signed_membership_records").is_none());
    }

    #[test]
    fn profile_sync_device_enrollment_request_rejects_invalid_shape() {
        assert!(matches!(
            ProfileSyncDeviceEnrollmentRequest::new(DEFAULT_PROFILE_ID, "../device-b", 123),
            Err(StorageError::InvalidSyncDeviceId(device_id)) if device_id == "../device-b"
        ));

        let mut unsupported =
            ProfileSyncDeviceEnrollmentRequest::new(DEFAULT_PROFILE_ID, "device-b", 123).unwrap();
        unsupported.schema_version = PROFILE_SYNC_DEVICE_ENROLLMENT_REQUEST_SCHEMA_VERSION + 1;
        let error = unsupported.to_bytes().unwrap_err();
        assert!(matches!(
            error,
            StorageError::UnsupportedProfileSyncDeviceEnrollmentRequestSchema(schema_version)
                if schema_version == PROFILE_SYNC_DEVICE_ENROLLMENT_REQUEST_SCHEMA_VERSION + 1
        ));

        let missing_profile = serde_json::json!({
            "profile": "",
            "schema_version": PROFILE_SYNC_DEVICE_ENROLLMENT_REQUEST_SCHEMA_VERSION,
            "device_id": "device-b",
            "created_at": 123,
        });
        let error =
            ProfileSyncDeviceEnrollmentRequest::from_bytes(missing_profile.to_string().as_bytes())
                .unwrap_err();
        assert!(matches!(
            error,
            StorageError::InvalidProfileSyncDeviceEnrollmentRequest(reason)
                if reason.contains("missing profile id")
        ));

        let wrong_profile =
            ProfileSyncDeviceEnrollmentRequest::new("work", "device-b", 123).unwrap();
        let wrong_profile_error = ProfileSyncDeviceEnrollmentRequest::from_bytes_for_profile(
            wrong_profile.to_bytes().unwrap().as_slice(),
            DEFAULT_PROFILE_ID,
        )
        .unwrap_err();
        assert!(matches!(
            wrong_profile_error,
            StorageError::InvalidProfileSyncDeviceEnrollmentRequest(reason)
                if reason.contains("expected profile default, got work")
        ));
    }

    #[test]
    fn profile_sync_enrollment_bundle_can_be_derived_from_sync_secret() {
        let secret = SlateSyncSecret::from_bytes([49; SLATE_SYNC_SECRET_BYTES]);
        let bundle = SlateProfileDatabase::profile_sync_enrollment_bundle_from_secret_with_epoch(
            DEFAULT_PROFILE_ID,
            &secret,
            "device-b",
            DEFAULT_PROFILE_SYNC_MEMBERSHIP_EPOCH,
            40,
        )
        .unwrap();

        assert_eq!(bundle.profile, DEFAULT_PROFILE_ID);
        assert_eq!(bundle.target_device_id, "device-b");
        assert_eq!(bundle.created_at, 40);
        assert_eq!(bundle.signed_membership_records.len(), 2);

        let database_path =
            test_dir("sync-secret-enrollment-bundle").join(DEFAULT_DATABASE_FILE_NAME);
        let database =
            SlateProfileDatabase::open_resolved_with_device_id(database_path, "device-b").unwrap();
        database
            .activate_local_profile_sync_metadata(DEFAULT_PROFILE_ID)
            .unwrap();
        let applications = database
            .apply_profile_sync_enrollment_bundle(&bundle)
            .unwrap();

        assert_eq!(applications.len(), 2);
        assert!(applications[0].bootstrapped);
        assert!(!applications[1].bootstrapped);
        assert!(applications.iter().all(|application| application.applied));

        let account_authority_signer = secret
            .derive_profile_sync_device_signer(
                DEFAULT_PROFILE_ID,
                DEFAULT_PROFILE_SYNC_ACCOUNT_AUTHORITY_DEVICE_ID,
                DEFAULT_PROFILE_SYNC_MEMBERSHIP_EPOCH,
            )
            .unwrap();
        let target_signer = secret
            .derive_profile_sync_device_signer(
                DEFAULT_PROFILE_ID,
                "device-b",
                DEFAULT_PROFILE_SYNC_MEMBERSHIP_EPOCH,
            )
            .unwrap();
        assert_eq!(
            database
                .sync_device_public_key(
                    DEFAULT_PROFILE_ID,
                    DEFAULT_PROFILE_SYNC_ACCOUNT_AUTHORITY_DEVICE_ID
                )
                .unwrap()
                .expect("derived account authority key")
                .public_key,
            account_authority_signer.public_key().unwrap()
        );
        assert_eq!(
            database
                .sync_device_public_key(DEFAULT_PROFILE_ID, "device-b")
                .unwrap()
                .expect("derived target key")
                .public_key,
            target_signer.public_key().unwrap()
        );
        assert!(
            database
                .get_sync_setting_text(
                    DEFAULT_PROFILE_ID,
                    SYNC_DOMAIN_SETTINGS,
                    "slate-sync-secret"
                )
                .unwrap()
                .is_none()
        );
    }

    #[test]
    fn profile_sync_enrollment_bundle_can_be_derived_from_device_request() {
        let secret = SlateSyncSecret::from_bytes([51; SLATE_SYNC_SECRET_BYTES]);
        let request =
            ProfileSyncDeviceEnrollmentRequest::new(DEFAULT_PROFILE_ID, "device-b", 30).unwrap();
        let bundle = SlateProfileDatabase::profile_sync_enrollment_bundle_from_device_request(
            DEFAULT_PROFILE_ID,
            &secret,
            &request,
        )
        .unwrap();

        assert_eq!(bundle.profile, DEFAULT_PROFILE_ID);
        assert_eq!(bundle.target_device_id, request.device_id);
        assert_eq!(bundle.signed_membership_records.len(), 2);

        let wrong_profile_request =
            ProfileSyncDeviceEnrollmentRequest::new("work", "device-b", 30).unwrap();
        let error = SlateProfileDatabase::profile_sync_enrollment_bundle_from_device_request(
            DEFAULT_PROFILE_ID,
            &secret,
            &wrong_profile_request,
        )
        .unwrap_err();
        assert!(matches!(
            error,
            StorageError::InvalidProfileSyncDeviceEnrollmentRequest(reason)
                if reason.contains("expected profile default, got work")
        ));
    }

    #[test]
    fn profile_sync_secret_handoff_bundle_imports_secret_and_enrollment_for_local_device() {
        let secret = SlateSyncSecret::from_bytes([52; SLATE_SYNC_SECRET_BYTES]);
        let request =
            ProfileSyncDeviceEnrollmentRequest::new(DEFAULT_PROFILE_ID, "device-b", 30).unwrap();
        let bundle =
            SlateProfileDatabase::profile_sync_secret_handoff_bundle_from_secret_with_epoch(
                DEFAULT_PROFILE_ID,
                &secret,
                request.device_id.as_str(),
                DEFAULT_PROFILE_SYNC_MEMBERSHIP_EPOCH,
                40,
            )
            .unwrap();
        let encoded = bundle.to_bytes().unwrap();
        let decoded = ProfileSyncSecretHandoffBundle::from_bytes(encoded.as_slice()).unwrap();
        let debug = format!("{decoded:?}");

        assert_eq!(decoded, bundle);
        assert_eq!(decoded.profile, DEFAULT_PROFILE_ID);
        assert_eq!(decoded.target_device_id, "device-b");
        assert_eq!(
            decoded.schema_version,
            PROFILE_SYNC_SECRET_HANDOFF_BUNDLE_SCHEMA_VERSION
        );
        assert_eq!(decoded.sync_secret_export.profile, DEFAULT_PROFILE_ID);
        assert_eq!(decoded.enrollment_bundle.target_device_id, "device-b");
        assert!(!debug.contains(decoded.sync_secret_export.secret.as_str()));
        assert!(debug.contains("<redacted>"));

        let database_path = test_dir("sync-secret-handoff-import").join(DEFAULT_DATABASE_FILE_NAME);
        let database =
            SlateProfileDatabase::open_resolved_with_device_id(database_path, "device-b").unwrap();
        let application = database
            .apply_profile_sync_secret_handoff_bundle(&decoded)
            .unwrap();

        assert_eq!(application.enrollment_applications.len(), 2);
        assert!(
            application
                .enrollment_applications
                .iter()
                .all(|application| application.applied)
        );
        assert_eq!(application.activation.local_device_id, "device-b");
        assert_eq!(
            application.activation.activation.content_key_epoch.key_id,
            DEFAULT_PROFILE_SYNC_CONTENT_KEY_ID
        );
        assert_eq!(
            database
                .sync_account_membership_record_count(DEFAULT_PROFILE_ID)
                .unwrap(),
            2
        );

        let target_signer = secret
            .derive_profile_sync_device_signer(
                DEFAULT_PROFILE_ID,
                "device-b",
                DEFAULT_PROFILE_SYNC_MEMBERSHIP_EPOCH,
            )
            .unwrap();
        let target_key = database
            .sync_device_public_key(DEFAULT_PROFILE_ID, "device-b")
            .unwrap()
            .expect("target device key imported");
        assert!(target_key.trusted);
        assert_eq!(target_key.public_key, target_signer.public_key().unwrap());
        assert!(
            database
                .active_sync_content_key_epoch(DEFAULT_PROFILE_ID)
                .unwrap()
                .is_some()
        );
        assert!(
            database
                .get_sync_setting_text(
                    DEFAULT_PROFILE_ID,
                    SYNC_DOMAIN_SETTINGS,
                    "slate-sync-secret"
                )
                .unwrap()
                .is_none()
        );

        let replay = database
            .apply_profile_sync_secret_handoff_bundle(&decoded)
            .unwrap();
        assert!(
            replay
                .enrollment_applications
                .iter()
                .all(|application| !application.applied)
        );
        assert_eq!(
            database
                .sync_account_membership_record_count(DEFAULT_PROFILE_ID)
                .unwrap(),
            2
        );
    }

    #[test]
    fn profile_sync_secret_handoff_bundle_rejects_wrong_local_device_without_mutation() {
        let secret = SlateSyncSecret::from_bytes([53; SLATE_SYNC_SECRET_BYTES]);
        let bundle =
            SlateProfileDatabase::profile_sync_secret_handoff_bundle_from_secret_with_epoch(
                DEFAULT_PROFILE_ID,
                &secret,
                "device-b",
                DEFAULT_PROFILE_SYNC_MEMBERSHIP_EPOCH,
                40,
            )
            .unwrap();
        let database_path =
            test_dir("sync-secret-handoff-wrong-device").join(DEFAULT_DATABASE_FILE_NAME);
        let database =
            SlateProfileDatabase::open_resolved_with_device_id(database_path, "device-c").unwrap();

        let error = database
            .apply_profile_sync_secret_handoff_bundle(&bundle)
            .unwrap_err();

        assert!(matches!(
            error,
            StorageError::InvalidProfileSyncSecretHandoffBundle(reason)
                if reason.contains("handoff targets device device-b")
        ));
        assert_eq!(
            database
                .sync_account_membership_record_count(DEFAULT_PROFILE_ID)
                .unwrap(),
            0
        );
        assert!(
            database
                .active_sync_content_key_epoch(DEFAULT_PROFILE_ID)
                .unwrap()
                .is_none()
        );
    }

    #[test]
    fn profile_sync_enrollment_bundle_rejects_wrong_local_device_without_mutation() {
        let signer_a = ProfileSyncDeviceSigner::generate("device-a").unwrap();
        let signer_b = ProfileSyncDeviceSigner::generate("device-b").unwrap();
        let enroll_a = ProfileSyncMembershipRecord {
            profile: DEFAULT_PROFILE_ID.to_string(),
            record_id: "epoch-1-enroll-device-a".to_string(),
            schema_version: PROFILE_SYNC_MEMBERSHIP_RECORD_SCHEMA_VERSION,
            membership_epoch: 1,
            record_kind: PROFILE_SYNC_MEMBERSHIP_RECORD_KIND_ENROLL_DEVICE.to_string(),
            device_id: "device-a".to_string(),
            device_public_key: Some(signer_a.public_key().unwrap()),
            created_at: 10,
        };
        let enroll_b = ProfileSyncMembershipRecord {
            profile: DEFAULT_PROFILE_ID.to_string(),
            record_id: "epoch-2-enroll-device-b".to_string(),
            schema_version: PROFILE_SYNC_MEMBERSHIP_RECORD_SCHEMA_VERSION,
            membership_epoch: 2,
            record_kind: PROFILE_SYNC_MEMBERSHIP_RECORD_KIND_ENROLL_DEVICE.to_string(),
            device_id: "device-b".to_string(),
            device_public_key: Some(signer_b.public_key().unwrap()),
            created_at: 20,
        };
        let bundle = ProfileSyncEnrollmentBundle::new_device_enrollment(
            DEFAULT_PROFILE_ID,
            "device-b",
            vec![
                signed_membership_record_bytes(&signer_a, &enroll_a),
                signed_membership_record_bytes(&signer_a, &enroll_b),
            ],
            30,
        )
        .unwrap();
        let database_path =
            test_dir("sync-enrollment-bundle-wrong-device").join(DEFAULT_DATABASE_FILE_NAME);
        let database =
            SlateProfileDatabase::open_resolved_with_device_id(database_path, "device-c").unwrap();

        let error = database
            .apply_profile_sync_enrollment_bundle(&bundle)
            .unwrap_err();

        assert!(matches!(
            error,
            StorageError::InvalidProfileSyncEnrollmentBundle(reason)
                if reason.contains("bundle targets device device-b")
        ));
        assert_eq!(
            database
                .sync_account_membership_record_count(DEFAULT_PROFILE_ID)
                .unwrap(),
            0
        );
        assert!(
            database
                .sync_device_public_key(DEFAULT_PROFILE_ID, "device-a")
                .unwrap()
                .is_none()
        );
    }

    #[test]
    fn signed_sync_account_membership_records_and_root_update_are_atomic() {
        let database_path =
            test_dir("sync-account-membership-root-atomic").join(DEFAULT_DATABASE_FILE_NAME);
        let database = SlateProfileDatabase::open_resolved(database_path).unwrap();
        let signer = ProfileSyncDeviceSigner::generate("device-a").unwrap();
        let enroll = ProfileSyncMembershipRecord {
            profile: DEFAULT_PROFILE_ID.to_string(),
            record_id: "epoch-1-enroll-device-a".to_string(),
            schema_version: PROFILE_SYNC_MEMBERSHIP_RECORD_SCHEMA_VERSION,
            membership_epoch: 1,
            record_kind: PROFILE_SYNC_MEMBERSHIP_RECORD_KIND_ENROLL_DEVICE.to_string(),
            device_id: "device-a".to_string(),
            device_public_key: Some(signer.public_key().unwrap()),
            created_at: 10,
        };
        let signed_enroll = signed_membership_record_bytes(&signer, &enroll);
        let signed_records = [signed_enroll.clone()];

        let error = database
            .apply_signed_sync_account_membership_records_and_set_profile_sync_root(
                DEFAULT_PROFILE_ID,
                "",
                "membership-log-object-1",
                &signed_records,
            )
            .unwrap_err();

        assert!(matches!(error, StorageError::InvalidSyncRootId(root_id) if root_id.is_empty()));
        assert!(
            database
                .sync_account_membership_record(DEFAULT_PROFILE_ID, "epoch-1-enroll-device-a")
                .unwrap()
                .is_none()
        );
        assert!(
            database
                .sync_device_public_key(DEFAULT_PROFILE_ID, "device-a")
                .unwrap()
                .is_none()
        );
        assert!(
            database
                .profile_sync_roots(DEFAULT_PROFILE_ID)
                .unwrap()
                .is_empty()
        );
        assert!(
            !database
                .sync_devices(DEFAULT_PROFILE_ID)
                .unwrap()
                .iter()
                .any(|device| device.device_id == "device-a")
        );

        let (root, applications) = database
            .apply_signed_sync_account_membership_records_and_set_profile_sync_root(
                DEFAULT_PROFILE_ID,
                "account/membership/log",
                "membership-log-object-1",
                &signed_records,
            )
            .unwrap();

        assert_eq!(root.profile, DEFAULT_PROFILE_ID);
        assert_eq!(root.root_id, "account/membership/log");
        assert_eq!(root.object_id, "membership-log-object-1");
        assert_eq!(applications.len(), 1);
        assert!(applications[0].applied);
        assert!(
            applications[0]
                .device_key
                .as_ref()
                .expect("trusted device key")
                .trusted
        );
        assert_eq!(
            database
                .profile_sync_root(DEFAULT_PROFILE_ID, "account/membership/log")
                .unwrap()
                .expect("membership log root"),
            root
        );
        let device_a = database
            .sync_devices(DEFAULT_PROFILE_ID)
            .unwrap()
            .into_iter()
            .find(|device| device.device_id == "device-a")
            .expect("atomic membership apply records sync device");
        assert_eq!(device_a.membership_epoch, 1);
        assert!(!device_a.provider_authority);
    }

    #[test]
    fn signed_sync_account_membership_records_reject_untrusted_signers() {
        let database_path =
            test_dir("sync-account-membership-untrusted").join(DEFAULT_DATABASE_FILE_NAME);
        let database = SlateProfileDatabase::open_resolved(database_path).unwrap();
        let signer_a = ProfileSyncDeviceSigner::generate("device-a").unwrap();
        let signer_c = ProfileSyncDeviceSigner::generate("device-c").unwrap();
        let signer_d = ProfileSyncDeviceSigner::generate("device-d").unwrap();
        let enroll_a = ProfileSyncMembershipRecord {
            profile: DEFAULT_PROFILE_ID.to_string(),
            record_id: "epoch-1-enroll-device-a".to_string(),
            schema_version: PROFILE_SYNC_MEMBERSHIP_RECORD_SCHEMA_VERSION,
            membership_epoch: 1,
            record_kind: PROFILE_SYNC_MEMBERSHIP_RECORD_KIND_ENROLL_DEVICE.to_string(),
            device_id: "device-a".to_string(),
            device_public_key: Some(signer_a.public_key().unwrap()),
            created_at: 10,
        };
        database
            .apply_signed_sync_account_membership_record(
                signed_membership_record_bytes(&signer_a, &enroll_a).as_slice(),
            )
            .unwrap();

        let untrusted_enroll_d = ProfileSyncMembershipRecord {
            profile: DEFAULT_PROFILE_ID.to_string(),
            record_id: "epoch-2-enroll-device-d".to_string(),
            schema_version: PROFILE_SYNC_MEMBERSHIP_RECORD_SCHEMA_VERSION,
            membership_epoch: 2,
            record_kind: PROFILE_SYNC_MEMBERSHIP_RECORD_KIND_ENROLL_DEVICE.to_string(),
            device_id: "device-d".to_string(),
            device_public_key: Some(signer_d.public_key().unwrap()),
            created_at: 20,
        };
        let error = database
            .apply_signed_sync_account_membership_record(
                signed_membership_record_bytes(&signer_c, &untrusted_enroll_d).as_slice(),
            )
            .unwrap_err();
        assert!(matches!(
            error,
            StorageError::UntrustedSyncMembershipSigner { profile, device_id }
                if profile == DEFAULT_PROFILE_ID && device_id == "device-c"
        ));
    }

    #[test]
    fn signed_sync_account_membership_records_enroll_provider_authority_devices() {
        let database_path =
            test_dir("sync-account-membership-enroll-provider").join(DEFAULT_DATABASE_FILE_NAME);
        let database = SlateProfileDatabase::open_resolved(database_path).unwrap();
        let signer_a = ProfileSyncDeviceSigner::generate("device-a").unwrap();
        let provider_signer = ProfileSyncDeviceSigner::generate("provider-device").unwrap();
        let signer_c = ProfileSyncDeviceSigner::generate("device-c").unwrap();
        let enroll_a = ProfileSyncMembershipRecord {
            profile: DEFAULT_PROFILE_ID.to_string(),
            record_id: "epoch-1-enroll-device-a".to_string(),
            schema_version: PROFILE_SYNC_MEMBERSHIP_RECORD_SCHEMA_VERSION,
            membership_epoch: 1,
            record_kind: PROFILE_SYNC_MEMBERSHIP_RECORD_KIND_ENROLL_DEVICE.to_string(),
            device_id: "device-a".to_string(),
            device_public_key: Some(signer_a.public_key().unwrap()),
            created_at: 10,
        };
        database
            .apply_signed_sync_account_membership_record(
                signed_membership_record_bytes(&signer_a, &enroll_a).as_slice(),
            )
            .unwrap();

        let enroll_provider = ProfileSyncMembershipRecord {
            profile: DEFAULT_PROFILE_ID.to_string(),
            record_id: "epoch-2-enroll-provider-device".to_string(),
            schema_version: PROFILE_SYNC_MEMBERSHIP_RECORD_SCHEMA_VERSION,
            membership_epoch: 2,
            record_kind: PROFILE_SYNC_MEMBERSHIP_RECORD_KIND_ENROLL_PROVIDER.to_string(),
            device_id: "provider-device".to_string(),
            device_public_key: Some(provider_signer.public_key().unwrap()),
            created_at: 20,
        };
        let provider_application = database
            .apply_signed_sync_account_membership_record(
                signed_membership_record_bytes(&signer_a, &enroll_provider).as_slice(),
            )
            .unwrap();
        assert!(!provider_application.bootstrapped);
        assert!(provider_application.applied);
        assert_eq!(
            provider_application.membership_record.record_kind,
            PROFILE_SYNC_MEMBERSHIP_RECORD_KIND_ENROLL_PROVIDER
        );
        let provider_key = provider_application
            .device_key
            .expect("provider key application");
        assert_eq!(
            provider_key.public_key,
            provider_signer.public_key().unwrap()
        );
        assert_eq!(provider_key.membership_epoch, 2);
        assert!(provider_key.trusted);
        let provider_device = database
            .sync_devices(DEFAULT_PROFILE_ID)
            .unwrap()
            .into_iter()
            .find(|device| device.device_id == "provider-device")
            .expect("provider roster entry");
        assert_eq!(provider_device.membership_epoch, 2);
        assert!(provider_device.provider_authority);

        let provider_enrolls_c = ProfileSyncMembershipRecord {
            profile: DEFAULT_PROFILE_ID.to_string(),
            record_id: "epoch-3-provider-enrolls-device-c".to_string(),
            schema_version: PROFILE_SYNC_MEMBERSHIP_RECORD_SCHEMA_VERSION,
            membership_epoch: 3,
            record_kind: PROFILE_SYNC_MEMBERSHIP_RECORD_KIND_ENROLL_DEVICE.to_string(),
            device_id: "device-c".to_string(),
            device_public_key: Some(signer_c.public_key().unwrap()),
            created_at: 30,
        };
        let error = database
            .apply_signed_sync_account_membership_record(
                signed_membership_record_bytes(&provider_signer, &provider_enrolls_c).as_slice(),
            )
            .unwrap_err();
        assert!(matches!(
            error,
            StorageError::InvalidProfileSyncMembershipRecord(reason)
                if reason.contains("provider-device")
                    && reason.contains("provider authority")
        ));
        assert!(
            database
                .sync_device_public_key(DEFAULT_PROFILE_ID, "device-c")
                .unwrap()
                .is_none()
        );
    }

    #[test]
    fn signed_sync_account_membership_records_reject_provider_authority_signers() {
        let database_path =
            test_dir("sync-account-membership-provider-signer").join(DEFAULT_DATABASE_FILE_NAME);
        let database = SlateProfileDatabase::open_resolved(database_path).unwrap();
        let signer_a = ProfileSyncDeviceSigner::generate("device-a").unwrap();
        let provider_signer = ProfileSyncDeviceSigner::generate("provider-device").unwrap();
        let signer_c = ProfileSyncDeviceSigner::generate("device-c").unwrap();
        let enroll_a = ProfileSyncMembershipRecord {
            profile: DEFAULT_PROFILE_ID.to_string(),
            record_id: "epoch-1-enroll-device-a".to_string(),
            schema_version: PROFILE_SYNC_MEMBERSHIP_RECORD_SCHEMA_VERSION,
            membership_epoch: 1,
            record_kind: PROFILE_SYNC_MEMBERSHIP_RECORD_KIND_ENROLL_DEVICE.to_string(),
            device_id: "device-a".to_string(),
            device_public_key: Some(signer_a.public_key().unwrap()),
            created_at: 10,
        };
        database
            .apply_signed_sync_account_membership_record(
                signed_membership_record_bytes(&signer_a, &enroll_a).as_slice(),
            )
            .unwrap();

        let enroll_provider = ProfileSyncMembershipRecord {
            profile: DEFAULT_PROFILE_ID.to_string(),
            record_id: "epoch-2-enroll-provider-device".to_string(),
            schema_version: PROFILE_SYNC_MEMBERSHIP_RECORD_SCHEMA_VERSION,
            membership_epoch: 2,
            record_kind: PROFILE_SYNC_MEMBERSHIP_RECORD_KIND_ENROLL_DEVICE.to_string(),
            device_id: "provider-device".to_string(),
            device_public_key: Some(provider_signer.public_key().unwrap()),
            created_at: 20,
        };
        database
            .apply_signed_sync_account_membership_record(
                signed_membership_record_bytes(&signer_a, &enroll_provider).as_slice(),
            )
            .unwrap();
        database
            .register_sync_device(&SyncDeviceRegistration {
                profile: DEFAULT_PROFILE_ID.to_string(),
                device_id: "provider-device".to_string(),
                label: Some("Availability Provider".to_string()),
                membership_epoch: 2,
                provider_authority: true,
            })
            .unwrap();

        let provider_enrolls_c = ProfileSyncMembershipRecord {
            profile: DEFAULT_PROFILE_ID.to_string(),
            record_id: "epoch-3-provider-enrolls-device-c".to_string(),
            schema_version: PROFILE_SYNC_MEMBERSHIP_RECORD_SCHEMA_VERSION,
            membership_epoch: 3,
            record_kind: PROFILE_SYNC_MEMBERSHIP_RECORD_KIND_ENROLL_DEVICE.to_string(),
            device_id: "device-c".to_string(),
            device_public_key: Some(signer_c.public_key().unwrap()),
            created_at: 30,
        };
        let error = database
            .apply_signed_sync_account_membership_record(
                signed_membership_record_bytes(&provider_signer, &provider_enrolls_c).as_slice(),
            )
            .unwrap_err();
        assert!(matches!(
            error,
            StorageError::InvalidProfileSyncMembershipRecord(reason)
                if reason.contains("provider-device")
                    && reason.contains("provider authority")
        ));
        assert!(
            database
                .sync_account_membership_record(
                    DEFAULT_PROFILE_ID,
                    "epoch-3-provider-enrolls-device-c"
                )
                .unwrap()
                .is_none()
        );
        assert!(
            database
                .sync_device_public_key(DEFAULT_PROFILE_ID, "device-c")
                .unwrap()
                .is_none()
        );
        let provider_device = database
            .sync_devices(DEFAULT_PROFILE_ID)
            .unwrap()
            .into_iter()
            .find(|device| device.device_id == "provider-device")
            .expect("provider roster entry remains");
        assert!(provider_device.provider_authority);
        assert_eq!(provider_device.membership_epoch, 2);
        assert!(
            database
                .sync_device_public_key(DEFAULT_PROFILE_ID, "provider-device")
                .unwrap()
                .expect("provider key remains trusted")
                .trusted
        );
    }

    #[test]
    fn signed_sync_account_membership_records_reject_future_epoch_signers() {
        let database_path =
            test_dir("sync-account-membership-future-signer").join(DEFAULT_DATABASE_FILE_NAME);
        let database = SlateProfileDatabase::open_resolved(database_path).unwrap();
        let signer_a = ProfileSyncDeviceSigner::generate("device-a").unwrap();
        let signer_b = ProfileSyncDeviceSigner::generate("device-b").unwrap();
        let signer_c = ProfileSyncDeviceSigner::generate("device-c").unwrap();
        let enroll_a = ProfileSyncMembershipRecord {
            profile: DEFAULT_PROFILE_ID.to_string(),
            record_id: "epoch-1-enroll-device-a".to_string(),
            schema_version: PROFILE_SYNC_MEMBERSHIP_RECORD_SCHEMA_VERSION,
            membership_epoch: 1,
            record_kind: PROFILE_SYNC_MEMBERSHIP_RECORD_KIND_ENROLL_DEVICE.to_string(),
            device_id: "device-a".to_string(),
            device_public_key: Some(signer_a.public_key().unwrap()),
            created_at: 10,
        };
        database
            .apply_signed_sync_account_membership_record(
                signed_membership_record_bytes(&signer_a, &enroll_a).as_slice(),
            )
            .unwrap();

        let enroll_b = ProfileSyncMembershipRecord {
            profile: DEFAULT_PROFILE_ID.to_string(),
            record_id: "epoch-4-enroll-device-b".to_string(),
            schema_version: PROFILE_SYNC_MEMBERSHIP_RECORD_SCHEMA_VERSION,
            membership_epoch: 4,
            record_kind: PROFILE_SYNC_MEMBERSHIP_RECORD_KIND_ENROLL_DEVICE.to_string(),
            device_id: "device-b".to_string(),
            device_public_key: Some(signer_b.public_key().unwrap()),
            created_at: 40,
        };
        database
            .apply_signed_sync_account_membership_record(
                signed_membership_record_bytes(&signer_a, &enroll_b).as_slice(),
            )
            .unwrap();

        let stale_enroll_c = ProfileSyncMembershipRecord {
            profile: DEFAULT_PROFILE_ID.to_string(),
            record_id: "epoch-2-enroll-device-c".to_string(),
            schema_version: PROFILE_SYNC_MEMBERSHIP_RECORD_SCHEMA_VERSION,
            membership_epoch: 2,
            record_kind: PROFILE_SYNC_MEMBERSHIP_RECORD_KIND_ENROLL_DEVICE.to_string(),
            device_id: "device-c".to_string(),
            device_public_key: Some(signer_c.public_key().unwrap()),
            created_at: 20,
        };
        let error = database
            .apply_signed_sync_account_membership_record(
                signed_membership_record_bytes(&signer_b, &stale_enroll_c).as_slice(),
            )
            .unwrap_err();

        assert!(matches!(
            error,
            StorageError::InvalidProfileSyncMembershipRecord(reason)
                if reason.contains("trusted at epoch 4, after membership record epoch 2")
        ));
        assert!(
            database
                .sync_account_membership_record(DEFAULT_PROFILE_ID, "epoch-2-enroll-device-c")
                .unwrap()
                .is_none()
        );
        assert!(
            database
                .sync_device_public_key(DEFAULT_PROFILE_ID, "device-c")
                .unwrap()
                .is_none()
        );
    }

    #[test]
    fn signed_sync_account_membership_records_rotate_existing_trusted_device_key() {
        let database_path =
            test_dir("sync-account-membership-valid-rotation").join(DEFAULT_DATABASE_FILE_NAME);
        let database = SlateProfileDatabase::open_resolved(database_path).unwrap();
        let signer_a = ProfileSyncDeviceSigner::generate("device-a").unwrap();
        let signer_b = ProfileSyncDeviceSigner::generate("device-b").unwrap();
        let rotated_signer_b = ProfileSyncDeviceSigner::generate("device-b").unwrap();
        let enroll_a = ProfileSyncMembershipRecord {
            profile: DEFAULT_PROFILE_ID.to_string(),
            record_id: "epoch-1-enroll-device-a".to_string(),
            schema_version: PROFILE_SYNC_MEMBERSHIP_RECORD_SCHEMA_VERSION,
            membership_epoch: 1,
            record_kind: PROFILE_SYNC_MEMBERSHIP_RECORD_KIND_ENROLL_DEVICE.to_string(),
            device_id: "device-a".to_string(),
            device_public_key: Some(signer_a.public_key().unwrap()),
            created_at: 10,
        };
        database
            .apply_signed_sync_account_membership_record(
                signed_membership_record_bytes(&signer_a, &enroll_a).as_slice(),
            )
            .unwrap();
        let enroll_b = ProfileSyncMembershipRecord {
            profile: DEFAULT_PROFILE_ID.to_string(),
            record_id: "epoch-2-enroll-device-b".to_string(),
            schema_version: PROFILE_SYNC_MEMBERSHIP_RECORD_SCHEMA_VERSION,
            membership_epoch: 2,
            record_kind: PROFILE_SYNC_MEMBERSHIP_RECORD_KIND_ENROLL_DEVICE.to_string(),
            device_id: "device-b".to_string(),
            device_public_key: Some(signer_b.public_key().unwrap()),
            created_at: 20,
        };
        database
            .apply_signed_sync_account_membership_record(
                signed_membership_record_bytes(&signer_a, &enroll_b).as_slice(),
            )
            .unwrap();

        let rotate_b = ProfileSyncMembershipRecord {
            profile: DEFAULT_PROFILE_ID.to_string(),
            record_id: "epoch-3-rotate-device-b".to_string(),
            schema_version: PROFILE_SYNC_MEMBERSHIP_RECORD_SCHEMA_VERSION,
            membership_epoch: 3,
            record_kind: PROFILE_SYNC_MEMBERSHIP_RECORD_KIND_ROTATE_DEVICE_KEY.to_string(),
            device_id: "device-b".to_string(),
            device_public_key: Some(rotated_signer_b.public_key().unwrap()),
            created_at: 30,
        };
        let rotated = database
            .apply_signed_sync_account_membership_record(
                signed_membership_record_bytes(&signer_a, &rotate_b).as_slice(),
            )
            .unwrap();

        assert!(rotated.applied);
        assert_eq!(
            rotated.membership_record.record_kind,
            PROFILE_SYNC_MEMBERSHIP_RECORD_KIND_ROTATE_DEVICE_KEY
        );
        let device_b_key = database
            .sync_device_public_key(DEFAULT_PROFILE_ID, "device-b")
            .unwrap()
            .expect("rotated device b key");
        assert!(device_b_key.trusted);
        assert_eq!(device_b_key.membership_epoch, 3);
        assert_eq!(
            device_b_key.public_key,
            rotated_signer_b.public_key().unwrap()
        );
    }

    #[test]
    fn signed_sync_account_membership_records_reject_enrollment_for_trusted_device() {
        let database_path =
            test_dir("sync-account-membership-duplicate-enroll").join(DEFAULT_DATABASE_FILE_NAME);
        let database = SlateProfileDatabase::open_resolved(database_path).unwrap();
        let signer_a = ProfileSyncDeviceSigner::generate("device-a").unwrap();
        let signer_b = ProfileSyncDeviceSigner::generate("device-b").unwrap();
        let replacement_signer_b = ProfileSyncDeviceSigner::generate("device-b").unwrap();
        let enroll_a = ProfileSyncMembershipRecord {
            profile: DEFAULT_PROFILE_ID.to_string(),
            record_id: "epoch-1-enroll-device-a".to_string(),
            schema_version: PROFILE_SYNC_MEMBERSHIP_RECORD_SCHEMA_VERSION,
            membership_epoch: 1,
            record_kind: PROFILE_SYNC_MEMBERSHIP_RECORD_KIND_ENROLL_DEVICE.to_string(),
            device_id: "device-a".to_string(),
            device_public_key: Some(signer_a.public_key().unwrap()),
            created_at: 10,
        };
        database
            .apply_signed_sync_account_membership_record(
                signed_membership_record_bytes(&signer_a, &enroll_a).as_slice(),
            )
            .unwrap();
        let enroll_b = ProfileSyncMembershipRecord {
            profile: DEFAULT_PROFILE_ID.to_string(),
            record_id: "epoch-2-enroll-device-b".to_string(),
            schema_version: PROFILE_SYNC_MEMBERSHIP_RECORD_SCHEMA_VERSION,
            membership_epoch: 2,
            record_kind: PROFILE_SYNC_MEMBERSHIP_RECORD_KIND_ENROLL_DEVICE.to_string(),
            device_id: "device-b".to_string(),
            device_public_key: Some(signer_b.public_key().unwrap()),
            created_at: 20,
        };
        database
            .apply_signed_sync_account_membership_record(
                signed_membership_record_bytes(&signer_a, &enroll_b).as_slice(),
            )
            .unwrap();

        let duplicate_enroll_b = ProfileSyncMembershipRecord {
            profile: DEFAULT_PROFILE_ID.to_string(),
            record_id: "epoch-3-enroll-device-b-again".to_string(),
            schema_version: PROFILE_SYNC_MEMBERSHIP_RECORD_SCHEMA_VERSION,
            membership_epoch: 3,
            record_kind: PROFILE_SYNC_MEMBERSHIP_RECORD_KIND_ENROLL_DEVICE.to_string(),
            device_id: "device-b".to_string(),
            device_public_key: Some(replacement_signer_b.public_key().unwrap()),
            created_at: 30,
        };
        let error = database
            .apply_signed_sync_account_membership_record(
                signed_membership_record_bytes(&signer_a, &duplicate_enroll_b).as_slice(),
            )
            .unwrap_err();

        assert!(matches!(
            error,
            StorageError::InvalidProfileSyncMembershipRecord(reason)
                if reason.contains("cannot replace already trusted device device-b")
        ));
        assert!(
            database
                .sync_account_membership_record(DEFAULT_PROFILE_ID, "epoch-3-enroll-device-b-again")
                .unwrap()
                .is_none()
        );
        let device_b_key = database
            .sync_device_public_key(DEFAULT_PROFILE_ID, "device-b")
            .unwrap()
            .expect("device b key");
        assert!(device_b_key.trusted);
        assert_eq!(device_b_key.membership_epoch, 2);
        assert_eq!(device_b_key.public_key, signer_b.public_key().unwrap());
    }

    #[test]
    fn signed_sync_account_membership_records_reject_invalid_rotation_transitions() {
        let database_path =
            test_dir("sync-account-membership-invalid-rotation").join(DEFAULT_DATABASE_FILE_NAME);
        let database = SlateProfileDatabase::open_resolved(database_path).unwrap();
        let signer_a = ProfileSyncDeviceSigner::generate("device-a").unwrap();
        let signer_b = ProfileSyncDeviceSigner::generate("device-b").unwrap();
        let rotated_signer_b = ProfileSyncDeviceSigner::generate("device-b").unwrap();
        let enroll_a = ProfileSyncMembershipRecord {
            profile: DEFAULT_PROFILE_ID.to_string(),
            record_id: "epoch-1-enroll-device-a".to_string(),
            schema_version: PROFILE_SYNC_MEMBERSHIP_RECORD_SCHEMA_VERSION,
            membership_epoch: 1,
            record_kind: PROFILE_SYNC_MEMBERSHIP_RECORD_KIND_ENROLL_DEVICE.to_string(),
            device_id: "device-a".to_string(),
            device_public_key: Some(signer_a.public_key().unwrap()),
            created_at: 10,
        };
        database
            .apply_signed_sync_account_membership_record(
                signed_membership_record_bytes(&signer_a, &enroll_a).as_slice(),
            )
            .unwrap();

        let rotate_missing_b = ProfileSyncMembershipRecord {
            profile: DEFAULT_PROFILE_ID.to_string(),
            record_id: "epoch-2-rotate-missing-device-b".to_string(),
            schema_version: PROFILE_SYNC_MEMBERSHIP_RECORD_SCHEMA_VERSION,
            membership_epoch: 2,
            record_kind: PROFILE_SYNC_MEMBERSHIP_RECORD_KIND_ROTATE_DEVICE_KEY.to_string(),
            device_id: "device-b".to_string(),
            device_public_key: Some(rotated_signer_b.public_key().unwrap()),
            created_at: 20,
        };
        let missing_error = database
            .apply_signed_sync_account_membership_record(
                signed_membership_record_bytes(&signer_a, &rotate_missing_b).as_slice(),
            )
            .unwrap_err();
        assert!(matches!(
            missing_error,
            StorageError::InvalidProfileSyncMembershipRecord(reason)
                if reason.contains("requires an existing trusted key for device device-b")
        ));
        assert!(
            database
                .sync_account_membership_record(
                    DEFAULT_PROFILE_ID,
                    "epoch-2-rotate-missing-device-b"
                )
                .unwrap()
                .is_none()
        );
        assert!(
            database
                .sync_device_public_key(DEFAULT_PROFILE_ID, "device-b")
                .unwrap()
                .is_none()
        );

        let enroll_b = ProfileSyncMembershipRecord {
            profile: DEFAULT_PROFILE_ID.to_string(),
            record_id: "epoch-2-enroll-device-b".to_string(),
            schema_version: PROFILE_SYNC_MEMBERSHIP_RECORD_SCHEMA_VERSION,
            membership_epoch: 2,
            record_kind: PROFILE_SYNC_MEMBERSHIP_RECORD_KIND_ENROLL_DEVICE.to_string(),
            device_id: "device-b".to_string(),
            device_public_key: Some(signer_b.public_key().unwrap()),
            created_at: 25,
        };
        database
            .apply_signed_sync_account_membership_record(
                signed_membership_record_bytes(&signer_a, &enroll_b).as_slice(),
            )
            .unwrap();
        let revoke_b = ProfileSyncMembershipRecord {
            profile: DEFAULT_PROFILE_ID.to_string(),
            record_id: "epoch-3-revoke-device-b".to_string(),
            schema_version: PROFILE_SYNC_MEMBERSHIP_RECORD_SCHEMA_VERSION,
            membership_epoch: 3,
            record_kind: PROFILE_SYNC_MEMBERSHIP_RECORD_KIND_REVOKE_DEVICE.to_string(),
            device_id: "device-b".to_string(),
            device_public_key: None,
            created_at: 30,
        };
        database
            .apply_signed_sync_account_membership_record(
                signed_membership_record_bytes(&signer_a, &revoke_b).as_slice(),
            )
            .unwrap();

        let rotate_revoked_b = ProfileSyncMembershipRecord {
            profile: DEFAULT_PROFILE_ID.to_string(),
            record_id: "epoch-4-rotate-revoked-device-b".to_string(),
            schema_version: PROFILE_SYNC_MEMBERSHIP_RECORD_SCHEMA_VERSION,
            membership_epoch: 4,
            record_kind: PROFILE_SYNC_MEMBERSHIP_RECORD_KIND_ROTATE_DEVICE_KEY.to_string(),
            device_id: "device-b".to_string(),
            device_public_key: Some(rotated_signer_b.public_key().unwrap()),
            created_at: 40,
        };
        let revoked_error = database
            .apply_signed_sync_account_membership_record(
                signed_membership_record_bytes(&signer_a, &rotate_revoked_b).as_slice(),
            )
            .unwrap_err();
        assert!(matches!(
            revoked_error,
            StorageError::InvalidProfileSyncMembershipRecord(reason)
                if reason.contains("cannot re-trust revoked device device-b")
        ));
        assert!(
            !database
                .sync_device_public_key(DEFAULT_PROFILE_ID, "device-b")
                .unwrap()
                .expect("revoked device b key remains recorded")
                .trusted
        );

        let re_enroll_b = ProfileSyncMembershipRecord {
            record_id: "epoch-4-reenroll-device-b".to_string(),
            record_kind: PROFILE_SYNC_MEMBERSHIP_RECORD_KIND_ENROLL_DEVICE.to_string(),
            created_at: 45,
            ..rotate_revoked_b
        };
        let re_enrolled = database
            .apply_signed_sync_account_membership_record(
                signed_membership_record_bytes(&signer_a, &re_enroll_b).as_slice(),
            )
            .unwrap();
        assert!(re_enrolled.applied);
        let device_b_key = database
            .sync_device_public_key(DEFAULT_PROFILE_ID, "device-b")
            .unwrap()
            .expect("device b key after explicit re-enrollment");
        assert!(device_b_key.trusted);
        assert_eq!(device_b_key.membership_epoch, 4);
        assert_eq!(
            device_b_key.public_key,
            rotated_signer_b.public_key().unwrap()
        );
    }

    #[test]
    fn sync_account_membership_records_reject_reused_ids_with_different_bytes() {
        let database_path =
            test_dir("sync-account-membership-conflict").join(DEFAULT_DATABASE_FILE_NAME);
        let database = SlateProfileDatabase::open_resolved(database_path).unwrap();
        let first = SyncAccountMembershipRecordRegistration {
            profile: DEFAULT_PROFILE_ID.to_string(),
            record_id: "epoch-1-enroll-device-a".to_string(),
            membership_epoch: 1,
            record_kind: PROFILE_SYNC_MEMBERSHIP_RECORD_KIND_ENROLL_DEVICE.to_string(),
            device_id: "device-a".to_string(),
            signer_device_id: "device-a".to_string(),
            signed_record: vec![1, 2, 3],
        };
        let second = SyncAccountMembershipRecordRegistration {
            signed_record: vec![3, 2, 1],
            ..first.clone()
        };

        database
            .record_sync_account_membership_record(&first)
            .unwrap();
        let error = database
            .record_sync_account_membership_record(&second)
            .unwrap_err();

        assert!(matches!(
            error,
            StorageError::InvalidProfileSyncMembershipRecord(reason)
                if reason.contains("already exists with different signed bytes")
        ));
    }

    #[test]
    fn signed_sync_account_membership_records_validate_payload_shape() {
        let database_path =
            test_dir("invalid-sync-account-membership").join(DEFAULT_DATABASE_FILE_NAME);
        let database = SlateProfileDatabase::open_resolved(database_path).unwrap();
        let signer = ProfileSyncDeviceSigner::generate("device-a").unwrap();
        let other_public_key = ProfileSyncDeviceSigner::generate("device-b")
            .unwrap()
            .public_key()
            .unwrap();
        let mismatched_key_record = ProfileSyncMembershipRecord {
            profile: DEFAULT_PROFILE_ID.to_string(),
            record_id: "epoch-1-enroll-device-a".to_string(),
            schema_version: PROFILE_SYNC_MEMBERSHIP_RECORD_SCHEMA_VERSION,
            membership_epoch: 1,
            record_kind: PROFILE_SYNC_MEMBERSHIP_RECORD_KIND_ENROLL_DEVICE.to_string(),
            device_id: "device-a".to_string(),
            device_public_key: Some(other_public_key),
            created_at: 10,
        };
        let error = database
            .record_signed_sync_account_membership_record(
                signed_membership_record_bytes(&signer, &mismatched_key_record).as_slice(),
            )
            .unwrap_err();
        assert!(matches!(
            error,
            StorageError::InvalidProfileSyncMembershipRecord(reason)
                if reason.contains("does not match target device")
        ));

        let invalid_id_record = ProfileSyncMembershipRecord {
            record_id: "../epoch-1".to_string(),
            device_public_key: Some(signer.public_key().unwrap()),
            ..mismatched_key_record
        };
        let error = database
            .record_signed_sync_account_membership_record(
                signed_membership_record_bytes(&signer, &invalid_id_record).as_slice(),
            )
            .unwrap_err();
        assert!(matches!(
            error,
            StorageError::InvalidSyncMembershipRecordId(record_id) if record_id == "../epoch-1"
        ));

        let mut tampered = signed_membership_record_bytes(&signer, &invalid_id_record);
        let last = tampered.last_mut().expect("signed bytes");
        *last = last.wrapping_add(1);
        let error = database
            .record_signed_sync_account_membership_record(tampered.as_slice())
            .unwrap_err();
        assert!(matches!(
            error,
            StorageError::InvalidProfileSyncMembershipRecord(_)
        ));
    }

    #[test]
    fn sync_account_membership_records_schema_is_migrated() {
        let database_path =
            test_dir("sync-account-membership-migration").join(DEFAULT_DATABASE_FILE_NAME);
        {
            let connection = Connection::open(&database_path).unwrap();
            connection
                .execute_batch(
                    "
                    CREATE TABLE schema_migrations (
                        version INTEGER PRIMARY KEY,
                        applied_at INTEGER NOT NULL
                    );
                    INSERT INTO schema_migrations(version, applied_at) VALUES (1, 10);
                    CREATE TABLE sync_account_membership_records (
                        profile TEXT NOT NULL,
                        record_id TEXT NOT NULL,
                        membership_epoch INTEGER NOT NULL,
                        record_kind TEXT NOT NULL,
                        device_id TEXT NOT NULL,
                        signer_device_id TEXT NOT NULL,
                        signed_record BLOB NOT NULL,
                        created_at INTEGER NOT NULL,
                        PRIMARY KEY(profile, record_id)
                    );
                    ",
                )
                .unwrap();
        }

        let database = SlateProfileDatabase::open_resolved(database_path.clone()).unwrap();
        let signer = ProfileSyncDeviceSigner::generate("device-a").unwrap();
        let record = ProfileSyncMembershipRecord {
            profile: DEFAULT_PROFILE_ID.to_string(),
            record_id: "epoch-1-enroll-device-a".to_string(),
            schema_version: PROFILE_SYNC_MEMBERSHIP_RECORD_SCHEMA_VERSION,
            membership_epoch: 1,
            record_kind: PROFILE_SYNC_MEMBERSHIP_RECORD_KIND_ENROLL_DEVICE.to_string(),
            device_id: "device-a".to_string(),
            device_public_key: Some(signer.public_key().unwrap()),
            created_at: 10,
        };
        database
            .record_signed_sync_account_membership_record(
                signed_membership_record_bytes(&signer, &record).as_slice(),
            )
            .unwrap();

        assert_eq!(
            database
                .sync_account_membership_records(DEFAULT_PROFILE_ID)
                .unwrap()
                .len(),
            1
        );
        let connection = Connection::open(database_path).unwrap();
        let migration_count: i64 = connection
            .query_row(
                "SELECT COUNT(*) FROM schema_migrations WHERE version IN (4, 5)",
                [],
                |row| row.get(0),
            )
            .unwrap();
        assert_eq!(migration_count, 2);
        let migrated_applied_at: Option<i64> = connection
            .query_row(
                "SELECT applied_at FROM sync_account_membership_records
                 WHERE profile = 'default' AND record_id = 'epoch-1-enroll-device-a'",
                [],
                |row| row.get(0),
            )
            .unwrap();
        assert_eq!(migrated_applied_at, None);
    }

    #[test]
    fn sync_content_key_epochs_track_one_active_key_per_profile() {
        let database_path = test_dir("sync-content-key-epoch").join(DEFAULT_DATABASE_FILE_NAME);
        let database = SlateProfileDatabase::open_resolved(database_path).unwrap();
        let first = database
            .register_sync_content_key_epoch(&SyncContentKeyEpochRegistration {
                profile: DEFAULT_PROFILE_ID.to_string(),
                key_id: "content-key-epoch-1".to_string(),
                membership_epoch: 1,
                algorithm: PROFILE_SYNC_CONTENT_KEY_ALGORITHM_CHACHA20_POLY1305.to_string(),
                active: true,
            })
            .unwrap();

        assert_eq!(first.profile, DEFAULT_PROFILE_ID);
        assert_eq!(first.key_id, "content-key-epoch-1");
        assert_eq!(first.membership_epoch, 1);
        assert_eq!(
            first.algorithm,
            PROFILE_SYNC_CONTENT_KEY_ALGORITHM_CHACHA20_POLY1305
        );
        assert!(first.active);
        assert_eq!(
            database
                .active_sync_content_key_epoch(DEFAULT_PROFILE_ID)
                .unwrap()
                .expect("active content key"),
            first
        );

        let second = database
            .register_sync_content_key_epoch(&SyncContentKeyEpochRegistration {
                profile: DEFAULT_PROFILE_ID.to_string(),
                key_id: "content-key-epoch-2".to_string(),
                membership_epoch: 2,
                algorithm: PROFILE_SYNC_CONTENT_KEY_ALGORITHM_CHACHA20_POLY1305.to_string(),
                active: true,
            })
            .unwrap();

        let first_after = database
            .sync_content_key_epoch(DEFAULT_PROFILE_ID, "content-key-epoch-1")
            .unwrap()
            .expect("first key metadata");
        assert!(!first_after.active);
        assert!(second.active);
        assert_eq!(
            database
                .active_sync_content_key_epoch(DEFAULT_PROFILE_ID)
                .unwrap()
                .expect("rotated active content key")
                .key_id,
            "content-key-epoch-2"
        );
        let epochs = database
            .sync_content_key_epochs(DEFAULT_PROFILE_ID)
            .unwrap();
        assert_eq!(epochs.len(), 2);
        assert_eq!(epochs.iter().filter(|epoch| epoch.active).count(), 1);
    }

    #[test]
    fn sync_content_key_epochs_reject_invalid_key_ids() {
        let database_path = test_dir("invalid-sync-content-key").join(DEFAULT_DATABASE_FILE_NAME);
        let database = SlateProfileDatabase::open_resolved(database_path).unwrap();

        let error = database
            .register_sync_content_key_epoch(&SyncContentKeyEpochRegistration {
                profile: DEFAULT_PROFILE_ID.to_string(),
                key_id: "../content-key".to_string(),
                membership_epoch: 1,
                algorithm: PROFILE_SYNC_CONTENT_KEY_ALGORITHM_CHACHA20_POLY1305.to_string(),
                active: true,
            })
            .unwrap_err();

        assert!(
            matches!(error, StorageError::InvalidSyncContentKeyId(key_id) if key_id == "../content-key")
        );
        assert!(matches!(
            database.sync_content_key_epoch(DEFAULT_PROFILE_ID, "../content-key"),
            Err(StorageError::InvalidSyncContentKeyId(key_id)) if key_id == "../content-key"
        ));
    }

    #[test]
    fn profile_sync_local_activation_records_non_secret_metadata() {
        let database_path =
            test_dir("profile-sync-local-activation").join(DEFAULT_DATABASE_FILE_NAME);
        let database =
            SlateProfileDatabase::open_resolved_with_device_id(database_path, "device-preview")
                .unwrap();
        let activation = database
            .activate_local_profile_sync_metadata(DEFAULT_PROFILE_ID)
            .unwrap();

        assert_eq!(activation.profile, DEFAULT_PROFILE_ID);
        assert_eq!(activation.device_id, "device-preview");
        assert_eq!(
            activation.content_key_epoch.key_id,
            DEFAULT_PROFILE_SYNC_CONTENT_KEY_ID
        );
        assert_eq!(
            activation.content_key_epoch.algorithm,
            PROFILE_SYNC_CONTENT_KEY_ALGORITHM_CHACHA20_POLY1305
        );
        assert!(activation.content_key_epoch.active);

        let active_key = database
            .active_sync_content_key_epoch(DEFAULT_PROFILE_ID)
            .unwrap()
            .expect("active local preview key metadata");
        assert_eq!(active_key.key_id, DEFAULT_PROFILE_SYNC_CONTENT_KEY_ID);

        let devices = database.sync_devices(DEFAULT_PROFILE_ID).unwrap();
        assert!(
            devices
                .iter()
                .any(|device| device.device_id == "device-preview" && !device.provider_authority)
        );
        assert!(
            database
                .get_blob(DEFAULT_PROFILE_ID, "slate-sync-secret")
                .unwrap()
                .is_none()
        );
        assert!(
            database
                .get_sync_setting_text(
                    DEFAULT_PROFILE_ID,
                    SYNC_DOMAIN_SETTINGS,
                    "slate-sync-secret"
                )
                .unwrap()
                .is_none()
        );
    }

    #[test]
    fn profile_sync_secret_activation_trusts_derived_local_signer_without_storing_secret() {
        let database_path =
            test_dir("profile-sync-secret-activation").join(DEFAULT_DATABASE_FILE_NAME);
        let database =
            SlateProfileDatabase::open_resolved_with_device_id(database_path, "device-preview")
                .unwrap();
        let secret = SlateSyncSecret::from_bytes([47; SLATE_SYNC_SECRET_BYTES]);

        let activation = database
            .activate_local_profile_sync_from_secret(DEFAULT_PROFILE_ID, &secret)
            .unwrap();

        assert_eq!(activation.activation.profile, DEFAULT_PROFILE_ID);
        assert_eq!(activation.local_device_id, "device-preview");
        assert_eq!(
            activation.account_authority_device_id,
            DEFAULT_PROFILE_SYNC_ACCOUNT_AUTHORITY_DEVICE_ID
        );
        assert_eq!(activation.membership_applications.len(), 2);
        assert!(
            activation
                .membership_applications
                .iter()
                .all(|entry| entry.applied)
        );
        assert!(activation.membership_applications[0].bootstrapped);
        assert!(!activation.membership_applications[1].bootstrapped);

        let account_authority_signer = secret
            .derive_profile_sync_device_signer(
                DEFAULT_PROFILE_ID,
                DEFAULT_PROFILE_SYNC_ACCOUNT_AUTHORITY_DEVICE_ID,
                DEFAULT_PROFILE_SYNC_MEMBERSHIP_EPOCH,
            )
            .unwrap();
        let local_signer = secret
            .derive_profile_sync_device_signer(
                DEFAULT_PROFILE_ID,
                "device-preview",
                DEFAULT_PROFILE_SYNC_MEMBERSHIP_EPOCH,
            )
            .unwrap();
        assert_eq!(
            database
                .sync_device_public_key(
                    DEFAULT_PROFILE_ID,
                    DEFAULT_PROFILE_SYNC_ACCOUNT_AUTHORITY_DEVICE_ID
                )
                .unwrap()
                .expect("account authority key")
                .public_key,
            account_authority_signer.public_key().unwrap()
        );
        assert_eq!(
            database
                .sync_device_public_key(DEFAULT_PROFILE_ID, "device-preview")
                .unwrap()
                .expect("local device key")
                .public_key,
            local_signer.public_key().unwrap()
        );
        assert_eq!(
            database
                .sync_account_membership_record_count(DEFAULT_PROFILE_ID)
                .unwrap(),
            2
        );

        let repeated = database
            .activate_local_profile_sync_from_secret(DEFAULT_PROFILE_ID, &secret)
            .unwrap();
        assert_eq!(repeated.membership_applications.len(), 2);
        assert!(
            repeated
                .membership_applications
                .iter()
                .all(|entry| !entry.applied)
        );
        assert_eq!(
            database
                .sync_account_membership_record_count(DEFAULT_PROFILE_ID)
                .unwrap(),
            2
        );
        assert!(
            database
                .get_blob(DEFAULT_PROFILE_ID, "slate-sync-secret")
                .unwrap()
                .is_none()
        );
        assert!(
            database
                .get_sync_setting_text(
                    DEFAULT_PROFILE_ID,
                    SYNC_DOMAIN_SETTINGS,
                    "slate-sync-secret"
                )
                .unwrap()
                .is_none()
        );
    }

    #[test]
    fn profile_sync_local_readiness_reports_provider_gap() {
        let database_path =
            test_dir("profile-sync-local-readiness-gap").join(DEFAULT_DATABASE_FILE_NAME);
        let database =
            SlateProfileDatabase::open_resolved_with_device_id(database_path, "device-preview")
                .unwrap();
        let initial_report = database
            .profile_sync_local_readiness(DEFAULT_PROFILE_ID)
            .unwrap();

        assert!(!initial_report.metadata_ready);
        assert!(!initial_report.ready_for_manual_sync);
        assert_eq!(
            initial_report.blocked_reason.as_deref(),
            Some("missing active content-key metadata")
        );

        database
            .activate_local_profile_sync_metadata(DEFAULT_PROFILE_ID)
            .unwrap();
        let report = database
            .profile_sync_local_readiness(DEFAULT_PROFILE_ID)
            .unwrap();

        assert!(report.metadata_ready);
        assert!(!report.local_device_trusted);
        assert!(!report.account_authority_trusted);
        assert_eq!(report.trusted_device_count, 0);
        assert_eq!(
            report.active_key_id.as_deref(),
            Some(DEFAULT_PROFILE_SYNC_CONTENT_KEY_ID)
        );
        assert!(report.local_device_registered);
        assert!(report.enabled_app_domain_count > 0);
        assert_eq!(report.storage_provider_count, 0);
        assert_eq!(report.authorized_retention_provider_count, 0);
        assert!(!report.ready_for_manual_sync);
        assert_eq!(
            report.blocked_reason.as_deref(),
            Some("local device sync key is not trusted")
        );
    }

    #[test]
    fn profile_sync_local_readiness_reports_authorized_retention_provider() {
        let database_path =
            test_dir("profile-sync-local-readiness-ready").join(DEFAULT_DATABASE_FILE_NAME);
        let database =
            SlateProfileDatabase::open_resolved_with_device_id(database_path, "device-preview")
                .unwrap();
        let secret = SlateSyncSecret::from_bytes([48; SLATE_SYNC_SECRET_BYTES]);

        database
            .activate_local_profile_sync_from_secret(DEFAULT_PROFILE_ID, &secret)
            .unwrap();
        let provider = database
            .activate_local_profile_sync_preview_provider(
                DEFAULT_PROFILE_ID,
                Some("slate-fixture-profile-sync://preview/local-preview-provider".to_string()),
            )
            .unwrap();

        let report = database
            .profile_sync_local_readiness(DEFAULT_PROFILE_ID)
            .unwrap();

        assert_eq!(
            provider.provider_id,
            DEFAULT_PROFILE_SYNC_PREVIEW_PROVIDER_ID
        );
        assert_eq!(
            provider.provider_kind,
            DEFAULT_PROFILE_SYNC_PREVIEW_PROVIDER_KIND
        );
        assert_eq!(
            provider.endpoint_ref.as_deref(),
            Some("slate-fixture-profile-sync://preview/local-preview-provider")
        );
        assert!(!provider.mutable_roots);
        assert!(report.metadata_ready);
        assert!(report.local_device_trusted);
        assert!(report.account_authority_trusted);
        assert_eq!(report.trusted_device_count, 3);
        assert_eq!(report.storage_provider_count, 1);
        assert_eq!(report.enabled_storage_provider_count, 1);
        assert_eq!(report.retention_capable_provider_count, 1);
        assert_eq!(report.authorized_retention_provider_count, 1);
        assert!(report.ready_for_manual_sync);
        assert_eq!(report.blocked_reason, None);
    }

    #[test]
    fn setting_text_writes_materialized_sync_change_and_revision() {
        let database_path = test_dir("sync-setting").join(DEFAULT_DATABASE_FILE_NAME);
        let database = SlateProfileDatabase::open_resolved(database_path).unwrap();
        let baseline_revision = database
            .sync_revisions_after(DEFAULT_PROFILE_ID, 0)
            .unwrap()
            .last()
            .map(|revision| revision.revision)
            .unwrap_or(0);
        let baseline_change = database
            .sync_changes_after(DEFAULT_PROFILE_ID, 0, 100)
            .unwrap()
            .last()
            .map(|change| change.id)
            .unwrap_or(0);

        database.set_setting_text("ui.theme", "slate").unwrap();

        let value = database
            .get_sync_setting_text(DEFAULT_PROFILE_ID, SYNC_DOMAIN_SETTINGS, "ui.theme")
            .unwrap()
            .unwrap();
        assert_eq!(value.value, "slate");
        assert_eq!(value.value_kind, "text");
        assert!(value.revision > baseline_revision);

        let changes = database
            .sync_changes_after(DEFAULT_PROFILE_ID, baseline_change, 10)
            .unwrap();
        assert_eq!(changes.len(), 1);
        assert_eq!(changes[0].domain, SYNC_DOMAIN_SETTINGS);
        assert_eq!(changes[0].entity_key, "ui.theme");
        assert_eq!(changes[0].operation, "set_text");
        assert_eq!(changes[0].payload, "slate");
        assert_eq!(changes[0].device_id, DEFAULT_SYNC_DEVICE_ID);

        let revisions = database
            .sync_revisions_after(DEFAULT_PROFILE_ID, baseline_revision)
            .unwrap();
        assert_eq!(revisions.len(), 1);
        assert_eq!(revisions[0].domain, SYNC_DOMAIN_SETTINGS);
        assert_eq!(revisions[0].change_id, changes[0].id);
    }

    #[test]
    fn app_domain_setting_changes_do_not_touch_legacy_settings_table() {
        let database_path = test_dir("sync-app-domain").join(DEFAULT_DATABASE_FILE_NAME);
        let database = SlateProfileDatabase::open_resolved(database_path).unwrap();

        let change = database
            .set_sync_setting_text(
                DEFAULT_PROFILE_ID,
                SYNC_DOMAIN_CALENDAR,
                "default_view",
                "month",
            )
            .unwrap();

        assert_eq!(change.domain, SYNC_DOMAIN_CALENDAR);
        assert_eq!(change.entity_key, "default_view");
        assert_eq!(change.device_sequence, 2);
        assert_eq!(
            database
                .get_setting_text("default_view")
                .unwrap()
                .as_deref(),
            None
        );

        let value = database
            .get_sync_setting_text(DEFAULT_PROFILE_ID, SYNC_DOMAIN_CALENDAR, "default_view")
            .unwrap()
            .unwrap();
        assert_eq!(value.value, "month");
        assert!(
            database
                .sync_revisions_after(DEFAULT_PROFILE_ID, 0)
                .unwrap()
                .iter()
                .any(|revision| {
                    revision.revision == value.revision && revision.change_id == change.id
                })
        );
    }

    #[test]
    fn incoming_setting_change_updates_materialized_views_idempotently() {
        let database_path = test_dir("incoming-sync-setting").join(DEFAULT_DATABASE_FILE_NAME);
        let database = SlateProfileDatabase::open_resolved(database_path).unwrap();
        let baseline_revision = database
            .sync_revisions_after(DEFAULT_PROFILE_ID, 0)
            .unwrap()
            .last()
            .map(|revision| revision.revision)
            .unwrap_or(0);

        let incoming = IncomingSyncSettingText::new(
            DEFAULT_PROFILE_ID,
            SYNC_DOMAIN_SETTINGS,
            "ui.theme",
            "teal",
            "device-b",
            7,
            42,
        );
        let applied = database.apply_sync_setting_text(&incoming).unwrap();

        assert_eq!(applied.domain, SYNC_DOMAIN_SETTINGS);
        assert_eq!(applied.entity_key, "ui.theme");
        assert_eq!(applied.payload, "teal");
        assert_eq!(applied.device_id, "device-b");
        assert_eq!(applied.device_sequence, 7);
        assert_eq!(applied.logical_clock, 42);
        assert_eq!(
            database.get_setting_text("ui.theme").unwrap().as_deref(),
            Some("teal")
        );

        let value = database
            .get_sync_setting_text(DEFAULT_PROFILE_ID, SYNC_DOMAIN_SETTINGS, "ui.theme")
            .unwrap()
            .unwrap();
        assert_eq!(value.value, "teal");
        assert!(value.revision > baseline_revision);

        let duplicate = database.apply_sync_setting_text(&incoming).unwrap();
        assert_eq!(duplicate.id, applied.id);
        assert_eq!(
            database
                .sync_revisions_after(DEFAULT_PROFILE_ID, value.revision)
                .unwrap(),
            Vec::<SyncRevisionRecord>::new()
        );

        let devices = database.sync_devices(DEFAULT_PROFILE_ID).unwrap();
        assert!(devices.iter().any(|device| device.device_id == "device-b"));
    }

    #[test]
    fn incoming_bookmark_slot_change_updates_bookmark_rows() {
        let database_path = test_dir("incoming-bookmark-slot").join(DEFAULT_DATABASE_FILE_NAME);
        let database = SlateProfileDatabase::open_resolved(database_path).unwrap();
        let payload = BookmarkSlotSyncPayload {
            url: "https://example.com/".to_string(),
            title: Some("Example".to_string()),
            folder: None,
            position: 0,
            favicon_key: Some("favicon:https://example.com/".to_string()),
            replaced_url: Some(DEFAULT_HOME_BOOKMARKS[0].url.to_string()),
            deleted: false,
        };
        let incoming = IncomingSyncSettingText::new(
            DEFAULT_PROFILE_ID,
            SYNC_DOMAIN_BOOKMARKS,
            bookmark_home_slot_sync_key(payload.position),
            serde_json::to_string(&payload).unwrap(),
            "device-b",
            1,
            20,
        );

        let applied = database.apply_sync_setting_text(&incoming).unwrap();

        assert_eq!(applied.domain, SYNC_DOMAIN_BOOKMARKS);
        assert_eq!(applied.entity_key, "home.slot.0");
        assert!(applied.applied_at.is_some());
        let bookmarks = database.bookmarks(DEFAULT_PROFILE_ID).unwrap();
        assert_eq!(bookmarks.len(), DEFAULT_HOME_BOOKMARKS.len());
        assert_eq!(bookmarks[0].url, "https://example.com/");
        assert_eq!(bookmarks[0].title.as_deref(), Some("Example"));
        assert_eq!(bookmarks[0].position, 0);
        assert_eq!(
            bookmarks[0].favicon_key.as_deref(),
            Some("favicon:https://example.com/")
        );
        assert_eq!(bookmarks[1].url, DEFAULT_HOME_BOOKMARKS[1].url);
        let value = database
            .get_sync_setting_text(DEFAULT_PROFILE_ID, SYNC_DOMAIN_BOOKMARKS, "home.slot.0")
            .unwrap()
            .unwrap();
        assert_eq!(value.value, applied.payload);
    }

    #[test]
    fn incoming_bookmark_slot_tombstone_removes_bookmark_row() {
        let database_path =
            test_dir("incoming-bookmark-slot-tombstone").join(DEFAULT_DATABASE_FILE_NAME);
        let database = SlateProfileDatabase::open_resolved(database_path).unwrap();
        database
            .set_bookmark_slot(
                &BookmarkUpdate {
                    profile: DEFAULT_PROFILE_ID.to_string(),
                    url: "https://example.com/".to_string(),
                    title: Some("Example".to_string()),
                    folder: None,
                    position: 0,
                    favicon_key: None,
                },
                Some(DEFAULT_HOME_BOOKMARKS[0].url),
            )
            .unwrap();
        let payload = BookmarkSlotSyncPayload {
            url: "https://example.com/".to_string(),
            title: None,
            folder: None,
            position: 0,
            favicon_key: None,
            replaced_url: None,
            deleted: true,
        };
        let incoming = IncomingSyncSettingText::new(
            DEFAULT_PROFILE_ID,
            SYNC_DOMAIN_BOOKMARKS,
            bookmark_home_slot_sync_key(payload.position),
            serde_json::to_string(&payload).unwrap(),
            "zz-device",
            1,
            100,
        );

        let applied = database.apply_sync_setting_text(&incoming).unwrap();

        assert!(applied.applied_at.is_some());
        let bookmarks = database.bookmarks(DEFAULT_PROFILE_ID).unwrap();
        assert!(
            !bookmarks
                .iter()
                .any(|bookmark| bookmark.url == "https://example.com/")
        );
        assert!(!bookmarks.iter().any(|bookmark| bookmark.position == 0));
        let value = database
            .get_sync_setting_text(DEFAULT_PROFILE_ID, SYNC_DOMAIN_BOOKMARKS, "home.slot.0")
            .unwrap()
            .unwrap();
        let stored: BookmarkSlotSyncPayload = serde_json::from_str(value.value.as_str()).unwrap();
        assert!(stored.deleted);
    }

    #[test]
    fn incoming_losing_bookmark_slot_change_does_not_replace_winner() {
        let database_path =
            test_dir("incoming-bookmark-slot-conflict").join(DEFAULT_DATABASE_FILE_NAME);
        let database = SlateProfileDatabase::open_resolved(database_path).unwrap();
        let winning_payload = BookmarkSlotSyncPayload {
            url: "https://winner.example/".to_string(),
            title: Some("Winner".to_string()),
            folder: None,
            position: 1,
            favicon_key: None,
            replaced_url: Some(DEFAULT_HOME_BOOKMARKS[1].url.to_string()),
            deleted: false,
        };
        let losing_payload = BookmarkSlotSyncPayload {
            url: "https://loser.example/".to_string(),
            title: Some("Loser".to_string()),
            folder: None,
            position: 1,
            favicon_key: None,
            replaced_url: Some(DEFAULT_HOME_BOOKMARKS[1].url.to_string()),
            deleted: false,
        };
        let winning = database
            .apply_sync_setting_text(&IncomingSyncSettingText::new(
                DEFAULT_PROFILE_ID,
                SYNC_DOMAIN_BOOKMARKS,
                bookmark_home_slot_sync_key(1),
                serde_json::to_string(&winning_payload).unwrap(),
                "device-b",
                2,
                40,
            ))
            .unwrap();

        let losing = database
            .apply_sync_setting_text(&IncomingSyncSettingText::new(
                DEFAULT_PROFILE_ID,
                SYNC_DOMAIN_BOOKMARKS,
                bookmark_home_slot_sync_key(1),
                serde_json::to_string(&losing_payload).unwrap(),
                "device-c",
                1,
                30,
            ))
            .unwrap();

        assert!(winning.applied_at.is_some());
        assert_eq!(losing.applied_at, None);
        let bookmarks = database.bookmarks(DEFAULT_PROFILE_ID).unwrap();
        assert_eq!(bookmarks.len(), DEFAULT_HOME_BOOKMARKS.len());
        let winner = bookmarks
            .iter()
            .find(|bookmark| bookmark.position == 1)
            .expect("winner bookmark slot");
        assert_eq!(winner.url, "https://winner.example/");
        assert_eq!(winner.title.as_deref(), Some("Winner"));
        let value = database
            .get_sync_setting_text(DEFAULT_PROFILE_ID, SYNC_DOMAIN_BOOKMARKS, "home.slot.1")
            .unwrap()
            .unwrap();
        assert_eq!(value.value, winning.payload);
    }

    #[test]
    fn download_metadata_writes_sync_change_without_file_bytes() {
        let database_path = test_dir("download-metadata-local").join(DEFAULT_DATABASE_FILE_NAME);
        let database = SlateProfileDatabase::open_resolved(database_path).unwrap();
        let download = DownloadMetadataUpdate {
            profile: DEFAULT_PROFILE_ID.to_string(),
            download_id: "download-1".to_string(),
            source_url: "ipfs://bafybeigdyrzt/picture.png".to_string(),
            final_url: "ipfs://bafybeigdyrzt/picture.png".to_string(),
            route: Some("ipfs://bafybeigdyrzt/picture.png".to_string()),
            transport_id: Some("ipfs-gateway".to_string()),
            filename: "picture.png".to_string(),
            content_type: Some("image/png".to_string()),
            size_bytes: 42,
            integrity: Some("sha256-fixture".to_string()),
        };

        let record = database.record_download_metadata(&download).unwrap();

        assert_eq!(record.profile, DEFAULT_PROFILE_ID);
        assert_eq!(record.download_id, "download-1");
        assert_eq!(record.filename, "picture.png");
        assert_eq!(record.size_bytes, 42);
        assert_eq!(record.transport_id.as_deref(), Some("ipfs-gateway"));
        let downloads = database.downloads(DEFAULT_PROFILE_ID, 10).unwrap();
        assert_eq!(downloads, vec![record]);
        let events = database
            .sync_setting_text_events_after_for_domain(
                DEFAULT_PROFILE_ID,
                SYNC_DOMAIN_DOWNLOADS,
                0,
                10,
            )
            .unwrap();
        assert_eq!(events.len(), 1);
        assert_eq!(events[0].change.entity_key, "download.download-1");
        let payload: DownloadMetadataSyncPayload =
            serde_json::from_str(events[0].change.payload.as_str()).unwrap();
        assert_eq!(payload.download_id, "download-1");
        assert_eq!(payload.source_url, "ipfs://bafybeigdyrzt/picture.png");
        assert_eq!(payload.filename, "picture.png");
        assert!(!payload.deleted);
        let payload_json: serde_json::Value =
            serde_json::from_str(events[0].change.payload.as_str()).unwrap();
        assert!(payload_json.get("path").is_none());
        assert!(payload_json.get("local_path").is_none());
        assert!(payload_json.get("file_bytes").is_none());
        assert!(payload_json.get("contents").is_none());
        let value = database
            .get_sync_setting_text(
                DEFAULT_PROFILE_ID,
                SYNC_DOMAIN_DOWNLOADS,
                "download.download-1",
            )
            .unwrap()
            .unwrap();
        assert_eq!(value.value, events[0].change.payload);
    }

    #[test]
    fn download_metadata_removal_records_tombstone() {
        let database_path =
            test_dir("download-metadata-tombstone").join(DEFAULT_DATABASE_FILE_NAME);
        let database = SlateProfileDatabase::open_resolved(database_path).unwrap();
        database
            .record_download_metadata(&DownloadMetadataUpdate {
                profile: DEFAULT_PROFILE_ID.to_string(),
                download_id: "download-delete-1".to_string(),
                source_url: "ipfs://bafy-download-delete".to_string(),
                final_url: "ipfs://bafy-download-delete".to_string(),
                route: Some("ipfs://bafy-download-delete".to_string()),
                transport_id: Some("ipfs-fixture".to_string()),
                filename: "delete-me.bin".to_string(),
                content_type: Some("application/octet-stream".to_string()),
                size_bytes: 512,
                integrity: Some("sha256-delete-me".to_string()),
            })
            .unwrap();

        database
            .remove_download_metadata(DEFAULT_PROFILE_ID, "download-delete-1")
            .unwrap();

        assert!(
            database
                .downloads(DEFAULT_PROFILE_ID, 10)
                .unwrap()
                .is_empty()
        );
        let events = database
            .sync_setting_text_events_after_for_domain(
                DEFAULT_PROFILE_ID,
                SYNC_DOMAIN_DOWNLOADS,
                0,
                10,
            )
            .unwrap();
        assert_eq!(events.len(), 2);
        let tombstone: DownloadMetadataSyncPayload =
            serde_json::from_str(events[1].change.payload.as_str()).unwrap();
        assert!(tombstone.deleted);
        assert_eq!(tombstone.download_id, "download-delete-1");
        assert_eq!(tombstone.filename, "delete-me.bin");
        assert_eq!(tombstone.size_bytes, 512);
        let value = database
            .get_sync_setting_text(
                DEFAULT_PROFILE_ID,
                SYNC_DOMAIN_DOWNLOADS,
                "download.download-delete-1",
            )
            .unwrap()
            .unwrap();
        assert_eq!(value.value, events[1].change.payload);
    }

    #[test]
    fn incoming_download_metadata_change_updates_download_rows() {
        let database_path = test_dir("incoming-download-metadata").join(DEFAULT_DATABASE_FILE_NAME);
        let database = SlateProfileDatabase::open_resolved(database_path).unwrap();
        let payload = DownloadMetadataSyncPayload {
            download_id: "download-2".to_string(),
            source_url: "https://example.com/report.pdf".to_string(),
            final_url: "https://cdn.example.com/report.pdf".to_string(),
            route: Some("https://cdn.example.com/report.pdf".to_string()),
            transport_id: Some("direct-http".to_string()),
            filename: "report.pdf".to_string(),
            content_type: Some("application/pdf".to_string()),
            size_bytes: 2048,
            integrity: None,
            deleted: false,
        };
        let incoming = IncomingSyncSettingText::new(
            DEFAULT_PROFILE_ID,
            SYNC_DOMAIN_DOWNLOADS,
            download_metadata_sync_key(payload.download_id.as_str()),
            serde_json::to_string(&payload).unwrap(),
            "device-b",
            1,
            20,
        );

        let applied = database.apply_sync_setting_text(&incoming).unwrap();

        assert_eq!(applied.domain, SYNC_DOMAIN_DOWNLOADS);
        assert_eq!(applied.entity_key, "download.download-2");
        assert!(applied.applied_at.is_some());
        let downloads = database.downloads(DEFAULT_PROFILE_ID, 10).unwrap();
        assert_eq!(downloads.len(), 1);
        assert_eq!(downloads[0].download_id, "download-2");
        assert_eq!(downloads[0].source_url, "https://example.com/report.pdf");
        assert_eq!(downloads[0].final_url, "https://cdn.example.com/report.pdf");
        assert_eq!(downloads[0].transport_id.as_deref(), Some("direct-http"));
        assert_eq!(downloads[0].filename, "report.pdf");
        assert_eq!(downloads[0].size_bytes, 2048);
        let value = database
            .get_sync_setting_text(
                DEFAULT_PROFILE_ID,
                SYNC_DOMAIN_DOWNLOADS,
                "download.download-2",
            )
            .unwrap()
            .unwrap();
        assert_eq!(value.value, applied.payload);
    }

    #[test]
    fn incoming_download_metadata_tombstone_removes_row() {
        let database_path =
            test_dir("incoming-download-metadata-tombstone").join(DEFAULT_DATABASE_FILE_NAME);
        let database = SlateProfileDatabase::open_resolved(database_path).unwrap();
        database
            .record_download_metadata(&DownloadMetadataUpdate {
                profile: DEFAULT_PROFILE_ID.to_string(),
                download_id: "download-delete-2".to_string(),
                source_url: "https://example.com/delete.bin".to_string(),
                final_url: "https://example.com/delete.bin".to_string(),
                route: None,
                transport_id: Some("direct-http".to_string()),
                filename: "delete.bin".to_string(),
                content_type: Some("application/octet-stream".to_string()),
                size_bytes: 64,
                integrity: None,
            })
            .unwrap();
        let tombstone = DownloadMetadataSyncPayload {
            download_id: "download-delete-2".to_string(),
            source_url: "https://example.com/delete.bin".to_string(),
            final_url: "https://example.com/delete.bin".to_string(),
            route: None,
            transport_id: Some("direct-http".to_string()),
            filename: "delete.bin".to_string(),
            content_type: Some("application/octet-stream".to_string()),
            size_bytes: 64,
            integrity: None,
            deleted: true,
        };

        let applied = database
            .apply_sync_setting_text(&IncomingSyncSettingText::new(
                DEFAULT_PROFILE_ID,
                SYNC_DOMAIN_DOWNLOADS,
                download_metadata_sync_key("download-delete-2"),
                serde_json::to_string(&tombstone).unwrap(),
                "zz-device",
                1,
                100,
            ))
            .unwrap();

        assert!(applied.applied_at.is_some());
        assert!(
            database
                .downloads(DEFAULT_PROFILE_ID, 10)
                .unwrap()
                .is_empty()
        );
    }

    #[test]
    fn incoming_losing_download_metadata_change_does_not_replace_winner() {
        let database_path =
            test_dir("incoming-download-metadata-conflict").join(DEFAULT_DATABASE_FILE_NAME);
        let database = SlateProfileDatabase::open_resolved(database_path).unwrap();
        let winning_payload = DownloadMetadataSyncPayload {
            download_id: "download-3".to_string(),
            source_url: "https://winner.example/file.zip".to_string(),
            final_url: "https://winner.example/file.zip".to_string(),
            route: None,
            transport_id: Some("direct-http".to_string()),
            filename: "winner.zip".to_string(),
            content_type: Some("application/zip".to_string()),
            size_bytes: 300,
            integrity: Some("sha256-winner".to_string()),
            deleted: false,
        };
        let losing_payload = DownloadMetadataSyncPayload {
            download_id: "download-3".to_string(),
            source_url: "https://loser.example/file.zip".to_string(),
            final_url: "https://loser.example/file.zip".to_string(),
            route: None,
            transport_id: Some("direct-http".to_string()),
            filename: "loser.zip".to_string(),
            content_type: Some("application/zip".to_string()),
            size_bytes: 100,
            integrity: Some("sha256-loser".to_string()),
            deleted: false,
        };

        let winning = database
            .apply_sync_setting_text(&IncomingSyncSettingText::new(
                DEFAULT_PROFILE_ID,
                SYNC_DOMAIN_DOWNLOADS,
                download_metadata_sync_key("download-3"),
                serde_json::to_string(&winning_payload).unwrap(),
                "device-b",
                2,
                40,
            ))
            .unwrap();
        let losing = database
            .apply_sync_setting_text(&IncomingSyncSettingText::new(
                DEFAULT_PROFILE_ID,
                SYNC_DOMAIN_DOWNLOADS,
                download_metadata_sync_key("download-3"),
                serde_json::to_string(&losing_payload).unwrap(),
                "device-c",
                1,
                30,
            ))
            .unwrap();

        assert!(winning.applied_at.is_some());
        assert_eq!(losing.applied_at, None);
        let downloads = database.downloads(DEFAULT_PROFILE_ID, 10).unwrap();
        assert_eq!(downloads.len(), 1);
        assert_eq!(downloads[0].download_id, "download-3");
        assert_eq!(downloads[0].source_url, "https://winner.example/file.zip");
        assert_eq!(downloads[0].filename, "winner.zip");
        assert_eq!(downloads[0].size_bytes, 300);
        let value = database
            .get_sync_setting_text(
                DEFAULT_PROFILE_ID,
                SYNC_DOMAIN_DOWNLOADS,
                "download.download-3",
            )
            .unwrap()
            .unwrap();
        assert_eq!(value.value, winning.payload);
    }

    #[test]
    fn oversized_download_metadata_is_rejected_before_sqlite_insert() {
        let database_path =
            test_dir("download-metadata-oversized").join(DEFAULT_DATABASE_FILE_NAME);
        let database = SlateProfileDatabase::open_resolved(database_path).unwrap();
        let download = DownloadMetadataUpdate {
            profile: DEFAULT_PROFILE_ID.to_string(),
            download_id: "too-large".to_string(),
            source_url: "https://example.com/large.bin".to_string(),
            final_url: "https://example.com/large.bin".to_string(),
            route: None,
            transport_id: Some("direct-http".to_string()),
            filename: "large.bin".to_string(),
            content_type: Some("application/octet-stream".to_string()),
            size_bytes: i64::MAX as u64 + 1,
            integrity: None,
        };

        let error = database.record_download_metadata(&download).unwrap_err();

        assert!(
            matches!(error, StorageError::InvalidDownloadSize(size) if size == i64::MAX as u64 + 1)
        );
        assert!(
            database
                .downloads(DEFAULT_PROFILE_ID, 10)
                .unwrap()
                .is_empty()
        );
    }

    #[test]
    fn calendar_event_writes_sync_change_and_materializes_row() {
        let database_path = test_dir("calendar-event-local").join(DEFAULT_DATABASE_FILE_NAME);
        let database = SlateProfileDatabase::open_resolved(database_path).unwrap();
        let event = CalendarEventUpdate {
            profile: DEFAULT_PROFILE_ID.to_string(),
            event_id: "event-1".to_string(),
            calendar_id: Some("personal".to_string()),
            title: "Design review".to_string(),
            starts_at: 1_788_480_000,
            ends_at: Some(1_788_483_600),
            time_zone: Some("America/Los_Angeles".to_string()),
            location: Some("Slate room".to_string()),
            notes: Some("Review distributed sync milestones".to_string()),
            recurrence_rule: None,
            reminder_minutes: Some(15),
        };

        let record = database.upsert_calendar_event(&event).unwrap();

        assert_eq!(record.profile, DEFAULT_PROFILE_ID);
        assert_eq!(record.event_id, "event-1");
        assert_eq!(record.calendar_id.as_deref(), Some("personal"));
        assert_eq!(record.title, "Design review");
        assert_eq!(record.starts_at, 1_788_480_000);
        assert_eq!(record.ends_at, Some(1_788_483_600));
        assert_eq!(record.reminder_minutes, Some(15));
        assert_eq!(
            database.calendar_events(DEFAULT_PROFILE_ID, 10).unwrap(),
            vec![record]
        );
        let events = database
            .sync_setting_text_events_after_for_domain(
                DEFAULT_PROFILE_ID,
                SYNC_DOMAIN_CALENDAR,
                0,
                10,
            )
            .unwrap();
        assert_eq!(events.len(), 1);
        assert_eq!(events[0].change.entity_key, "event.event-1");
        let payload: CalendarEventSyncPayload =
            serde_json::from_str(events[0].change.payload.as_str()).unwrap();
        assert_eq!(payload.event_id, "event-1");
        assert_eq!(payload.title, "Design review");
        assert!(!payload.deleted);
        let value = database
            .get_sync_setting_text(DEFAULT_PROFILE_ID, SYNC_DOMAIN_CALENDAR, "event.event-1")
            .unwrap()
            .unwrap();
        assert_eq!(value.value, events[0].change.payload);
    }

    #[test]
    fn calendar_event_removal_records_tombstone() {
        let database_path = test_dir("calendar-event-tombstone").join(DEFAULT_DATABASE_FILE_NAME);
        let database = SlateProfileDatabase::open_resolved(database_path).unwrap();
        database
            .upsert_calendar_event(&CalendarEventUpdate {
                profile: DEFAULT_PROFILE_ID.to_string(),
                event_id: "event-2".to_string(),
                calendar_id: Some("work".to_string()),
                title: "Sync checkpoint".to_string(),
                starts_at: 1_788_500_000,
                ends_at: None,
                time_zone: Some("UTC".to_string()),
                location: None,
                notes: None,
                recurrence_rule: None,
                reminder_minutes: None,
            })
            .unwrap();

        database
            .remove_calendar_event(DEFAULT_PROFILE_ID, "event-2")
            .unwrap();

        assert!(
            database
                .calendar_events(DEFAULT_PROFILE_ID, 10)
                .unwrap()
                .is_empty()
        );
        let events = database
            .sync_setting_text_events_after_for_domain(
                DEFAULT_PROFILE_ID,
                SYNC_DOMAIN_CALENDAR,
                0,
                10,
            )
            .unwrap();
        assert_eq!(events.len(), 2);
        let tombstone: CalendarEventSyncPayload =
            serde_json::from_str(events[1].change.payload.as_str()).unwrap();
        assert!(tombstone.deleted);
        assert_eq!(tombstone.event_id, "event-2");
        assert_eq!(tombstone.title, "Sync checkpoint");
        let value = database
            .get_sync_setting_text(DEFAULT_PROFILE_ID, SYNC_DOMAIN_CALENDAR, "event.event-2")
            .unwrap()
            .unwrap();
        assert_eq!(value.value, events[1].change.payload);
    }

    #[test]
    fn incoming_calendar_event_change_updates_event_rows() {
        let database_path = test_dir("incoming-calendar-event").join(DEFAULT_DATABASE_FILE_NAME);
        let database = SlateProfileDatabase::open_resolved(database_path).unwrap();
        let payload = CalendarEventSyncPayload {
            event_id: "event-3".to_string(),
            calendar_id: Some("shared".to_string()),
            title: "Shared planning".to_string(),
            starts_at: 1_788_600_000,
            ends_at: Some(1_788_603_000),
            time_zone: Some("America/Los_Angeles".to_string()),
            location: Some("Remote".to_string()),
            notes: Some("Encrypted in profile sync objects".to_string()),
            recurrence_rule: Some("FREQ=WEEKLY;COUNT=3".to_string()),
            reminder_minutes: Some(30),
            deleted: false,
        };
        let incoming = IncomingSyncSettingText::new(
            DEFAULT_PROFILE_ID,
            SYNC_DOMAIN_CALENDAR,
            calendar_event_sync_key(payload.event_id.as_str()),
            serde_json::to_string(&payload).unwrap(),
            "device-b",
            1,
            20,
        );

        let applied = database.apply_sync_setting_text(&incoming).unwrap();

        assert_eq!(applied.domain, SYNC_DOMAIN_CALENDAR);
        assert_eq!(applied.entity_key, "event.event-3");
        assert!(applied.applied_at.is_some());
        let events = database.calendar_events(DEFAULT_PROFILE_ID, 10).unwrap();
        assert_eq!(events.len(), 1);
        assert_eq!(events[0].event_id, "event-3");
        assert_eq!(events[0].calendar_id.as_deref(), Some("shared"));
        assert_eq!(events[0].title, "Shared planning");
        assert_eq!(
            events[0].recurrence_rule.as_deref(),
            Some("FREQ=WEEKLY;COUNT=3")
        );
        assert_eq!(events[0].reminder_minutes, Some(30));
    }

    #[test]
    fn incoming_calendar_event_tombstone_removes_event_row() {
        let database_path =
            test_dir("incoming-calendar-event-tombstone").join(DEFAULT_DATABASE_FILE_NAME);
        let database = SlateProfileDatabase::open_resolved(database_path).unwrap();
        database
            .upsert_calendar_event(&CalendarEventUpdate {
                profile: DEFAULT_PROFILE_ID.to_string(),
                event_id: "event-4".to_string(),
                calendar_id: None,
                title: "Temporary event".to_string(),
                starts_at: 1_788_700_000,
                ends_at: None,
                time_zone: None,
                location: None,
                notes: None,
                recurrence_rule: None,
                reminder_minutes: None,
            })
            .unwrap();
        let tombstone = CalendarEventSyncPayload {
            event_id: "event-4".to_string(),
            calendar_id: None,
            title: "Temporary event".to_string(),
            starts_at: 1_788_700_000,
            ends_at: None,
            time_zone: None,
            location: None,
            notes: None,
            recurrence_rule: None,
            reminder_minutes: None,
            deleted: true,
        };

        let applied = database
            .apply_sync_setting_text(&IncomingSyncSettingText::new(
                DEFAULT_PROFILE_ID,
                SYNC_DOMAIN_CALENDAR,
                calendar_event_sync_key("event-4"),
                serde_json::to_string(&tombstone).unwrap(),
                "zz-device",
                1,
                100,
            ))
            .unwrap();

        assert!(applied.applied_at.is_some());
        assert!(
            database
                .calendar_events(DEFAULT_PROFILE_ID, 10)
                .unwrap()
                .is_empty()
        );
    }

    #[test]
    fn incoming_losing_calendar_event_change_does_not_replace_winner() {
        let database_path =
            test_dir("incoming-calendar-event-conflict").join(DEFAULT_DATABASE_FILE_NAME);
        let database = SlateProfileDatabase::open_resolved(database_path).unwrap();
        let winning_payload = CalendarEventSyncPayload {
            event_id: "event-5".to_string(),
            calendar_id: None,
            title: "Winner".to_string(),
            starts_at: 1_788_800_000,
            ends_at: None,
            time_zone: None,
            location: None,
            notes: None,
            recurrence_rule: None,
            reminder_minutes: None,
            deleted: false,
        };
        let losing_payload = CalendarEventSyncPayload {
            event_id: "event-5".to_string(),
            calendar_id: None,
            title: "Loser".to_string(),
            starts_at: 1_788_900_000,
            ends_at: None,
            time_zone: None,
            location: None,
            notes: None,
            recurrence_rule: None,
            reminder_minutes: None,
            deleted: false,
        };

        let winning = database
            .apply_sync_setting_text(&IncomingSyncSettingText::new(
                DEFAULT_PROFILE_ID,
                SYNC_DOMAIN_CALENDAR,
                calendar_event_sync_key("event-5"),
                serde_json::to_string(&winning_payload).unwrap(),
                "device-b",
                2,
                40,
            ))
            .unwrap();
        let losing = database
            .apply_sync_setting_text(&IncomingSyncSettingText::new(
                DEFAULT_PROFILE_ID,
                SYNC_DOMAIN_CALENDAR,
                calendar_event_sync_key("event-5"),
                serde_json::to_string(&losing_payload).unwrap(),
                "device-c",
                1,
                30,
            ))
            .unwrap();

        assert!(winning.applied_at.is_some());
        assert_eq!(losing.applied_at, None);
        let events = database.calendar_events(DEFAULT_PROFILE_ID, 10).unwrap();
        assert_eq!(events.len(), 1);
        assert_eq!(events[0].title, "Winner");
        assert_eq!(events[0].starts_at, 1_788_800_000);
        let value = database
            .get_sync_setting_text(DEFAULT_PROFILE_ID, SYNC_DOMAIN_CALENDAR, "event.event-5")
            .unwrap()
            .unwrap();
        assert_eq!(value.value, winning.payload);
    }

    #[test]
    fn calendar_event_ids_must_be_sync_identifiers() {
        let database_path = test_dir("calendar-event-invalid-id").join(DEFAULT_DATABASE_FILE_NAME);
        let database = SlateProfileDatabase::open_resolved(database_path).unwrap();
        let event = CalendarEventUpdate {
            profile: DEFAULT_PROFILE_ID.to_string(),
            event_id: "../event".to_string(),
            calendar_id: None,
            title: "Invalid".to_string(),
            starts_at: 1_788_800_000,
            ends_at: None,
            time_zone: None,
            location: None,
            notes: None,
            recurrence_rule: None,
            reminder_minutes: None,
        };

        let error = database.upsert_calendar_event(&event).unwrap_err();

        assert!(matches!(
            error,
            StorageError::InvalidCalendarEventId(event_id) if event_id == "../event"
        ));
        assert!(
            database
                .calendar_events(DEFAULT_PROFILE_ID, 10)
                .unwrap()
                .is_empty()
        );
    }

    #[test]
    fn chat_conversation_writes_metadata_sync_change_without_messages_or_tokens() {
        let database_path = test_dir("chat-conversation-local").join(DEFAULT_DATABASE_FILE_NAME);
        let database = SlateProfileDatabase::open_resolved(database_path).unwrap();
        let conversation = ChatConversationUpdate {
            profile: DEFAULT_PROFILE_ID.to_string(),
            conversation_id: "chat-1".to_string(),
            provider_id: Some("whatsapp".to_string()),
            external_thread_id: Some("family@example.test".to_string()),
            display_name: "Family".to_string(),
            avatar_key: Some("chat-avatar:chat-1".to_string()),
            last_message_at: Some(1_788_950_000),
            unread_count: 3,
            archived: false,
            muted: true,
        };

        let record = database.upsert_chat_conversation(&conversation).unwrap();

        assert_eq!(record.profile, DEFAULT_PROFILE_ID);
        assert_eq!(record.conversation_id, "chat-1");
        assert_eq!(record.provider_id.as_deref(), Some("whatsapp"));
        assert_eq!(
            record.external_thread_id.as_deref(),
            Some("family@example.test")
        );
        assert_eq!(record.display_name, "Family");
        assert_eq!(record.unread_count, 3);
        assert!(record.muted);
        assert_eq!(
            database.chat_conversations(DEFAULT_PROFILE_ID, 10).unwrap(),
            vec![record]
        );
        let events = database
            .sync_setting_text_events_after_for_domain(DEFAULT_PROFILE_ID, SYNC_DOMAIN_CHAT, 0, 10)
            .unwrap();
        assert_eq!(events.len(), 1);
        assert_eq!(events[0].change.entity_key, "conversation.chat-1");
        let payload: ChatConversationSyncPayload =
            serde_json::from_str(events[0].change.payload.as_str()).unwrap();
        assert_eq!(payload.conversation_id, "chat-1");
        assert_eq!(payload.provider_id.as_deref(), Some("whatsapp"));
        assert_eq!(payload.display_name, "Family");
        assert!(!payload.deleted);
        let payload_json: serde_json::Value =
            serde_json::from_str(events[0].change.payload.as_str()).unwrap();
        assert!(payload_json.get("message").is_none());
        assert!(payload_json.get("message_body").is_none());
        assert!(payload_json.get("messages").is_none());
        assert!(payload_json.get("provider_token").is_none());
        assert!(payload_json.get("sms_secret").is_none());
        assert!(payload_json.get("whatsapp_secret").is_none());
        let value = database
            .get_sync_setting_text(DEFAULT_PROFILE_ID, SYNC_DOMAIN_CHAT, "conversation.chat-1")
            .unwrap()
            .unwrap();
        assert_eq!(value.value, events[0].change.payload);
    }

    #[test]
    fn chat_conversation_removal_records_tombstone() {
        let database_path =
            test_dir("chat-conversation-tombstone").join(DEFAULT_DATABASE_FILE_NAME);
        let database = SlateProfileDatabase::open_resolved(database_path).unwrap();
        database
            .upsert_chat_conversation(&ChatConversationUpdate {
                profile: DEFAULT_PROFILE_ID.to_string(),
                conversation_id: "chat-2".to_string(),
                provider_id: Some("sms".to_string()),
                external_thread_id: Some("+15550101000".to_string()),
                display_name: "Alex".to_string(),
                avatar_key: None,
                last_message_at: Some(1_788_960_000),
                unread_count: 1,
                archived: false,
                muted: false,
            })
            .unwrap();

        database
            .remove_chat_conversation(DEFAULT_PROFILE_ID, "chat-2")
            .unwrap();

        assert!(
            database
                .chat_conversations(DEFAULT_PROFILE_ID, 10)
                .unwrap()
                .is_empty()
        );
        let events = database
            .sync_setting_text_events_after_for_domain(DEFAULT_PROFILE_ID, SYNC_DOMAIN_CHAT, 0, 10)
            .unwrap();
        assert_eq!(events.len(), 2);
        let tombstone: ChatConversationSyncPayload =
            serde_json::from_str(events[1].change.payload.as_str()).unwrap();
        assert!(tombstone.deleted);
        assert_eq!(tombstone.conversation_id, "chat-2");
        assert_eq!(tombstone.display_name, "Alex");
        let value = database
            .get_sync_setting_text(DEFAULT_PROFILE_ID, SYNC_DOMAIN_CHAT, "conversation.chat-2")
            .unwrap()
            .unwrap();
        assert_eq!(value.value, events[1].change.payload);
    }

    #[test]
    fn incoming_chat_conversation_change_updates_rows() {
        let database_path = test_dir("incoming-chat-conversation").join(DEFAULT_DATABASE_FILE_NAME);
        let database = SlateProfileDatabase::open_resolved(database_path).unwrap();
        let payload = ChatConversationSyncPayload {
            conversation_id: "chat-3".to_string(),
            provider_id: Some("whatsapp".to_string()),
            external_thread_id: Some("team@example.test".to_string()),
            display_name: "Project Team".to_string(),
            avatar_key: Some("chat-avatar:chat-3".to_string()),
            last_message_at: Some(1_788_970_000),
            unread_count: 5,
            archived: false,
            muted: false,
            deleted: false,
        };
        let incoming = IncomingSyncSettingText::new(
            DEFAULT_PROFILE_ID,
            SYNC_DOMAIN_CHAT,
            chat_conversation_sync_key(payload.conversation_id.as_str()),
            serde_json::to_string(&payload).unwrap(),
            "device-b",
            1,
            20,
        );

        let applied = database.apply_sync_setting_text(&incoming).unwrap();

        assert_eq!(applied.domain, SYNC_DOMAIN_CHAT);
        assert_eq!(applied.entity_key, "conversation.chat-3");
        assert!(applied.applied_at.is_some());
        let conversations = database.chat_conversations(DEFAULT_PROFILE_ID, 10).unwrap();
        assert_eq!(conversations.len(), 1);
        assert_eq!(conversations[0].conversation_id, "chat-3");
        assert_eq!(conversations[0].provider_id.as_deref(), Some("whatsapp"));
        assert_eq!(conversations[0].display_name, "Project Team");
        assert_eq!(conversations[0].unread_count, 5);
    }

    #[test]
    fn incoming_chat_conversation_tombstone_removes_row() {
        let database_path =
            test_dir("incoming-chat-conversation-tombstone").join(DEFAULT_DATABASE_FILE_NAME);
        let database = SlateProfileDatabase::open_resolved(database_path).unwrap();
        database
            .upsert_chat_conversation(&ChatConversationUpdate {
                profile: DEFAULT_PROFILE_ID.to_string(),
                conversation_id: "chat-4".to_string(),
                provider_id: Some("sms".to_string()),
                external_thread_id: Some("+15550102000".to_string()),
                display_name: "Temporary Thread".to_string(),
                avatar_key: None,
                last_message_at: None,
                unread_count: 0,
                archived: false,
                muted: false,
            })
            .unwrap();
        let tombstone = ChatConversationSyncPayload {
            conversation_id: "chat-4".to_string(),
            provider_id: Some("sms".to_string()),
            external_thread_id: Some("+15550102000".to_string()),
            display_name: "Temporary Thread".to_string(),
            avatar_key: None,
            last_message_at: None,
            unread_count: 0,
            archived: false,
            muted: false,
            deleted: true,
        };

        let applied = database
            .apply_sync_setting_text(&IncomingSyncSettingText::new(
                DEFAULT_PROFILE_ID,
                SYNC_DOMAIN_CHAT,
                chat_conversation_sync_key("chat-4"),
                serde_json::to_string(&tombstone).unwrap(),
                "zz-device",
                1,
                100,
            ))
            .unwrap();

        assert!(applied.applied_at.is_some());
        assert!(
            database
                .chat_conversations(DEFAULT_PROFILE_ID, 10)
                .unwrap()
                .is_empty()
        );
    }

    #[test]
    fn incoming_losing_chat_conversation_change_does_not_replace_winner() {
        let database_path =
            test_dir("incoming-chat-conversation-conflict").join(DEFAULT_DATABASE_FILE_NAME);
        let database = SlateProfileDatabase::open_resolved(database_path).unwrap();
        let winning_payload = ChatConversationSyncPayload {
            conversation_id: "chat-5".to_string(),
            provider_id: Some("whatsapp".to_string()),
            external_thread_id: Some("winner@example.test".to_string()),
            display_name: "Winner Thread".to_string(),
            avatar_key: None,
            last_message_at: Some(1_788_980_000),
            unread_count: 7,
            archived: false,
            muted: true,
            deleted: false,
        };
        let losing_payload = ChatConversationSyncPayload {
            conversation_id: "chat-5".to_string(),
            provider_id: Some("whatsapp".to_string()),
            external_thread_id: Some("loser@example.test".to_string()),
            display_name: "Loser Thread".to_string(),
            avatar_key: None,
            last_message_at: Some(1_788_990_000),
            unread_count: 1,
            archived: true,
            muted: false,
            deleted: false,
        };

        let winning = database
            .apply_sync_setting_text(&IncomingSyncSettingText::new(
                DEFAULT_PROFILE_ID,
                SYNC_DOMAIN_CHAT,
                chat_conversation_sync_key("chat-5"),
                serde_json::to_string(&winning_payload).unwrap(),
                "device-b",
                2,
                40,
            ))
            .unwrap();
        let losing = database
            .apply_sync_setting_text(&IncomingSyncSettingText::new(
                DEFAULT_PROFILE_ID,
                SYNC_DOMAIN_CHAT,
                chat_conversation_sync_key("chat-5"),
                serde_json::to_string(&losing_payload).unwrap(),
                "device-c",
                1,
                30,
            ))
            .unwrap();

        assert!(winning.applied_at.is_some());
        assert_eq!(losing.applied_at, None);
        let conversations = database.chat_conversations(DEFAULT_PROFILE_ID, 10).unwrap();
        assert_eq!(conversations.len(), 1);
        assert_eq!(conversations[0].display_name, "Winner Thread");
        assert_eq!(
            conversations[0].external_thread_id.as_deref(),
            Some("winner@example.test")
        );
        assert_eq!(conversations[0].unread_count, 7);
        assert!(conversations[0].muted);
        let value = database
            .get_sync_setting_text(DEFAULT_PROFILE_ID, SYNC_DOMAIN_CHAT, "conversation.chat-5")
            .unwrap()
            .unwrap();
        assert_eq!(value.value, winning.payload);
    }

    #[test]
    fn chat_conversation_ids_and_provider_ids_are_validated() {
        let database_path = test_dir("chat-conversation-invalid").join(DEFAULT_DATABASE_FILE_NAME);
        let database = SlateProfileDatabase::open_resolved(database_path).unwrap();
        let invalid_conversation_id = ChatConversationUpdate {
            profile: DEFAULT_PROFILE_ID.to_string(),
            conversation_id: "../chat".to_string(),
            provider_id: Some("sms".to_string()),
            external_thread_id: None,
            display_name: "Invalid".to_string(),
            avatar_key: None,
            last_message_at: None,
            unread_count: 0,
            archived: false,
            muted: false,
        };
        let invalid_provider_id = ChatConversationUpdate {
            conversation_id: "chat-6".to_string(),
            provider_id: Some("../provider".to_string()),
            ..invalid_conversation_id.clone()
        };

        let conversation_error = database
            .upsert_chat_conversation(&invalid_conversation_id)
            .unwrap_err();
        let provider_error = database
            .upsert_chat_conversation(&invalid_provider_id)
            .unwrap_err();

        assert!(matches!(
            conversation_error,
            StorageError::InvalidChatConversationId(conversation_id)
                if conversation_id == "../chat"
        ));
        assert!(matches!(
            provider_error,
            StorageError::InvalidChatProviderId(provider_id) if provider_id == "../provider"
        ));
        assert!(
            database
                .chat_conversations(DEFAULT_PROFILE_ID, 10)
                .unwrap()
                .is_empty()
        );
    }

    #[test]
    fn contact_card_writes_sync_change_and_materializes_row() {
        let database_path = test_dir("contact-card-local").join(DEFAULT_DATABASE_FILE_NAME);
        let database = SlateProfileDatabase::open_resolved(database_path).unwrap();
        let contact = ContactCardUpdate {
            profile: DEFAULT_PROFILE_ID.to_string(),
            contact_id: "contact-1".to_string(),
            display_name: "Ada Lovelace".to_string(),
            given_name: Some("Ada".to_string()),
            family_name: Some("Lovelace".to_string()),
            organization: Some("Analytical Engine".to_string()),
            primary_email: Some("ada@example.test".to_string()),
            primary_phone: Some("+15550101000".to_string()),
            notes: Some("Sensitive local address book entry".to_string()),
            avatar_key: Some("contact-avatar:contact-1".to_string()),
        };

        let record = database.upsert_contact_card(&contact).unwrap();

        assert_eq!(record.profile, DEFAULT_PROFILE_ID);
        assert_eq!(record.contact_id, "contact-1");
        assert_eq!(record.display_name, "Ada Lovelace");
        assert_eq!(record.given_name.as_deref(), Some("Ada"));
        assert_eq!(record.primary_email.as_deref(), Some("ada@example.test"));
        assert_eq!(
            record.avatar_key.as_deref(),
            Some("contact-avatar:contact-1")
        );
        assert_eq!(
            database.contact_cards(DEFAULT_PROFILE_ID, 10).unwrap(),
            vec![record]
        );
        let events = database
            .sync_setting_text_events_after_for_domain(
                DEFAULT_PROFILE_ID,
                SYNC_DOMAIN_CONTACTS,
                0,
                10,
            )
            .unwrap();
        assert_eq!(events.len(), 1);
        assert_eq!(events[0].change.entity_key, "contact.contact-1");
        let payload: ContactCardSyncPayload =
            serde_json::from_str(events[0].change.payload.as_str()).unwrap();
        assert_eq!(payload.contact_id, "contact-1");
        assert_eq!(payload.display_name, "Ada Lovelace");
        assert!(!payload.deleted);
        let value = database
            .get_sync_setting_text(
                DEFAULT_PROFILE_ID,
                SYNC_DOMAIN_CONTACTS,
                "contact.contact-1",
            )
            .unwrap()
            .unwrap();
        assert_eq!(value.value, events[0].change.payload);
    }

    #[test]
    fn contact_card_removal_records_tombstone() {
        let database_path = test_dir("contact-card-tombstone").join(DEFAULT_DATABASE_FILE_NAME);
        let database = SlateProfileDatabase::open_resolved(database_path).unwrap();
        database
            .upsert_contact_card(&ContactCardUpdate {
                profile: DEFAULT_PROFILE_ID.to_string(),
                contact_id: "contact-2".to_string(),
                display_name: "Grace Hopper".to_string(),
                given_name: Some("Grace".to_string()),
                family_name: Some("Hopper".to_string()),
                organization: Some("Navy".to_string()),
                primary_email: None,
                primary_phone: None,
                notes: None,
                avatar_key: None,
            })
            .unwrap();

        database
            .remove_contact_card(DEFAULT_PROFILE_ID, "contact-2")
            .unwrap();

        assert!(
            database
                .contact_cards(DEFAULT_PROFILE_ID, 10)
                .unwrap()
                .is_empty()
        );
        let events = database
            .sync_setting_text_events_after_for_domain(
                DEFAULT_PROFILE_ID,
                SYNC_DOMAIN_CONTACTS,
                0,
                10,
            )
            .unwrap();
        assert_eq!(events.len(), 2);
        let tombstone: ContactCardSyncPayload =
            serde_json::from_str(events[1].change.payload.as_str()).unwrap();
        assert!(tombstone.deleted);
        assert_eq!(tombstone.contact_id, "contact-2");
        assert_eq!(tombstone.display_name, "Grace Hopper");
        let value = database
            .get_sync_setting_text(
                DEFAULT_PROFILE_ID,
                SYNC_DOMAIN_CONTACTS,
                "contact.contact-2",
            )
            .unwrap()
            .unwrap();
        assert_eq!(value.value, events[1].change.payload);
    }

    #[test]
    fn incoming_contact_card_change_updates_rows() {
        let database_path = test_dir("incoming-contact-card").join(DEFAULT_DATABASE_FILE_NAME);
        let database = SlateProfileDatabase::open_resolved(database_path).unwrap();
        let payload = ContactCardSyncPayload {
            contact_id: "contact-3".to_string(),
            display_name: "Katherine Johnson".to_string(),
            given_name: Some("Katherine".to_string()),
            family_name: Some("Johnson".to_string()),
            organization: Some("NASA".to_string()),
            primary_email: Some("katherine@example.test".to_string()),
            primary_phone: None,
            notes: Some("Imported from another Slate device".to_string()),
            avatar_key: None,
            deleted: false,
        };
        let incoming = IncomingSyncSettingText::new(
            DEFAULT_PROFILE_ID,
            SYNC_DOMAIN_CONTACTS,
            contact_card_sync_key(payload.contact_id.as_str()),
            serde_json::to_string(&payload).unwrap(),
            "device-b",
            1,
            20,
        );

        let applied = database.apply_sync_setting_text(&incoming).unwrap();

        assert_eq!(applied.domain, SYNC_DOMAIN_CONTACTS);
        assert_eq!(applied.entity_key, "contact.contact-3");
        assert!(applied.applied_at.is_some());
        let contacts = database.contact_cards(DEFAULT_PROFILE_ID, 10).unwrap();
        assert_eq!(contacts.len(), 1);
        assert_eq!(contacts[0].contact_id, "contact-3");
        assert_eq!(contacts[0].display_name, "Katherine Johnson");
        assert_eq!(contacts[0].organization.as_deref(), Some("NASA"));
        assert_eq!(
            contacts[0].primary_email.as_deref(),
            Some("katherine@example.test")
        );
    }

    #[test]
    fn incoming_contact_card_tombstone_removes_row() {
        let database_path =
            test_dir("incoming-contact-card-tombstone").join(DEFAULT_DATABASE_FILE_NAME);
        let database = SlateProfileDatabase::open_resolved(database_path).unwrap();
        database
            .upsert_contact_card(&ContactCardUpdate {
                profile: DEFAULT_PROFILE_ID.to_string(),
                contact_id: "contact-4".to_string(),
                display_name: "Temporary Contact".to_string(),
                given_name: None,
                family_name: None,
                organization: None,
                primary_email: Some("temporary@example.test".to_string()),
                primary_phone: None,
                notes: None,
                avatar_key: None,
            })
            .unwrap();
        let tombstone = ContactCardSyncPayload {
            contact_id: "contact-4".to_string(),
            display_name: "Temporary Contact".to_string(),
            given_name: None,
            family_name: None,
            organization: None,
            primary_email: Some("temporary@example.test".to_string()),
            primary_phone: None,
            notes: None,
            avatar_key: None,
            deleted: true,
        };

        let applied = database
            .apply_sync_setting_text(&IncomingSyncSettingText::new(
                DEFAULT_PROFILE_ID,
                SYNC_DOMAIN_CONTACTS,
                contact_card_sync_key("contact-4"),
                serde_json::to_string(&tombstone).unwrap(),
                "zz-device",
                1,
                100,
            ))
            .unwrap();

        assert!(applied.applied_at.is_some());
        assert!(
            database
                .contact_cards(DEFAULT_PROFILE_ID, 10)
                .unwrap()
                .is_empty()
        );
    }

    #[test]
    fn incoming_losing_contact_card_change_does_not_replace_winner() {
        let database_path =
            test_dir("incoming-contact-card-conflict").join(DEFAULT_DATABASE_FILE_NAME);
        let database = SlateProfileDatabase::open_resolved(database_path).unwrap();
        let winning_payload = ContactCardSyncPayload {
            contact_id: "contact-5".to_string(),
            display_name: "Winner Contact".to_string(),
            given_name: None,
            family_name: None,
            organization: Some("Winner Org".to_string()),
            primary_email: None,
            primary_phone: None,
            notes: None,
            avatar_key: None,
            deleted: false,
        };
        let losing_payload = ContactCardSyncPayload {
            contact_id: "contact-5".to_string(),
            display_name: "Loser Contact".to_string(),
            given_name: None,
            family_name: None,
            organization: Some("Loser Org".to_string()),
            primary_email: None,
            primary_phone: None,
            notes: None,
            avatar_key: None,
            deleted: false,
        };

        let winning = database
            .apply_sync_setting_text(&IncomingSyncSettingText::new(
                DEFAULT_PROFILE_ID,
                SYNC_DOMAIN_CONTACTS,
                contact_card_sync_key("contact-5"),
                serde_json::to_string(&winning_payload).unwrap(),
                "device-b",
                2,
                40,
            ))
            .unwrap();
        let losing = database
            .apply_sync_setting_text(&IncomingSyncSettingText::new(
                DEFAULT_PROFILE_ID,
                SYNC_DOMAIN_CONTACTS,
                contact_card_sync_key("contact-5"),
                serde_json::to_string(&losing_payload).unwrap(),
                "device-c",
                1,
                30,
            ))
            .unwrap();

        assert!(winning.applied_at.is_some());
        assert_eq!(losing.applied_at, None);
        let contacts = database.contact_cards(DEFAULT_PROFILE_ID, 10).unwrap();
        assert_eq!(contacts.len(), 1);
        assert_eq!(contacts[0].display_name, "Winner Contact");
        assert_eq!(contacts[0].organization.as_deref(), Some("Winner Org"));
        let value = database
            .get_sync_setting_text(
                DEFAULT_PROFILE_ID,
                SYNC_DOMAIN_CONTACTS,
                "contact.contact-5",
            )
            .unwrap()
            .unwrap();
        assert_eq!(value.value, winning.payload);
    }

    #[test]
    fn contact_card_ids_must_be_sync_identifiers() {
        let database_path = test_dir("contact-card-invalid-id").join(DEFAULT_DATABASE_FILE_NAME);
        let database = SlateProfileDatabase::open_resolved(database_path).unwrap();
        let contact = ContactCardUpdate {
            profile: DEFAULT_PROFILE_ID.to_string(),
            contact_id: "../contact".to_string(),
            display_name: "Invalid".to_string(),
            given_name: None,
            family_name: None,
            organization: None,
            primary_email: None,
            primary_phone: None,
            notes: None,
            avatar_key: None,
        };

        let error = database.upsert_contact_card(&contact).unwrap_err();

        assert!(matches!(
            error,
            StorageError::InvalidContactId(contact_id) if contact_id == "../contact"
        ));
        assert!(
            database
                .contact_cards(DEFAULT_PROFILE_ID, 10)
                .unwrap()
                .is_empty()
        );
    }

    #[test]
    fn file_entry_writes_metadata_sync_change_without_local_paths_or_bytes() {
        let database_path = test_dir("file-entry-local").join(DEFAULT_DATABASE_FILE_NAME);
        let database = SlateProfileDatabase::open_resolved(database_path).unwrap();
        let entry = FileEntryUpdate {
            profile: DEFAULT_PROFILE_ID.to_string(),
            entry_id: "file-1".to_string(),
            sync_set_id: Some("set-docs".to_string()),
            parent_id: Some("root".to_string()),
            name: "report.pdf".to_string(),
            entry_kind: "file".to_string(),
            content_ref: Some("bafybeigdyrzt-report".to_string()),
            mime_type: Some("application/pdf".to_string()),
            size_bytes: Some(4096),
            modified_at: Some(1_788_900_000),
            integrity: Some("sha256-report".to_string()),
            retention_policy: Some("keep-latest".to_string()),
        };

        let record = database.upsert_file_entry(&entry).unwrap();

        assert_eq!(record.profile, DEFAULT_PROFILE_ID);
        assert_eq!(record.entry_id, "file-1");
        assert_eq!(record.sync_set_id.as_deref(), Some("set-docs"));
        assert_eq!(record.parent_id.as_deref(), Some("root"));
        assert_eq!(record.name, "report.pdf");
        assert_eq!(record.entry_kind, "file");
        assert_eq!(record.content_ref.as_deref(), Some("bafybeigdyrzt-report"));
        assert_eq!(record.size_bytes, Some(4096));
        assert_eq!(
            database.file_entries(DEFAULT_PROFILE_ID, 10).unwrap(),
            vec![record]
        );
        let events = database
            .sync_setting_text_events_after_for_domain(DEFAULT_PROFILE_ID, SYNC_DOMAIN_FILES, 0, 10)
            .unwrap();
        assert_eq!(events.len(), 1);
        assert_eq!(events[0].change.entity_key, "entry.file-1");
        let payload: FileEntrySyncPayload =
            serde_json::from_str(events[0].change.payload.as_str()).unwrap();
        assert_eq!(payload.entry_id, "file-1");
        assert_eq!(payload.content_ref.as_deref(), Some("bafybeigdyrzt-report"));
        assert!(!payload.deleted);
        let payload_json: serde_json::Value =
            serde_json::from_str(events[0].change.payload.as_str()).unwrap();
        assert!(payload_json.get("path").is_none());
        assert!(payload_json.get("local_path").is_none());
        assert!(payload_json.get("file_bytes").is_none());
        assert!(payload_json.get("contents").is_none());
        let value = database
            .get_sync_setting_text(DEFAULT_PROFILE_ID, SYNC_DOMAIN_FILES, "entry.file-1")
            .unwrap()
            .unwrap();
        assert_eq!(value.value, events[0].change.payload);
    }

    #[test]
    fn file_entry_removal_records_tombstone() {
        let database_path = test_dir("file-entry-tombstone").join(DEFAULT_DATABASE_FILE_NAME);
        let database = SlateProfileDatabase::open_resolved(database_path).unwrap();
        database
            .upsert_file_entry(&FileEntryUpdate {
                profile: DEFAULT_PROFILE_ID.to_string(),
                entry_id: "file-2".to_string(),
                sync_set_id: Some("set-docs".to_string()),
                parent_id: None,
                name: "old-note.txt".to_string(),
                entry_kind: "file".to_string(),
                content_ref: Some("bafy-old-note".to_string()),
                mime_type: Some("text/plain".to_string()),
                size_bytes: Some(12),
                modified_at: Some(1_788_910_000),
                integrity: None,
                retention_policy: None,
            })
            .unwrap();

        database
            .remove_file_entry(DEFAULT_PROFILE_ID, "file-2")
            .unwrap();

        assert!(
            database
                .file_entries(DEFAULT_PROFILE_ID, 10)
                .unwrap()
                .is_empty()
        );
        let events = database
            .sync_setting_text_events_after_for_domain(DEFAULT_PROFILE_ID, SYNC_DOMAIN_FILES, 0, 10)
            .unwrap();
        assert_eq!(events.len(), 2);
        let tombstone: FileEntrySyncPayload =
            serde_json::from_str(events[1].change.payload.as_str()).unwrap();
        assert!(tombstone.deleted);
        assert_eq!(tombstone.entry_id, "file-2");
        assert_eq!(tombstone.name, "old-note.txt");
        let value = database
            .get_sync_setting_text(DEFAULT_PROFILE_ID, SYNC_DOMAIN_FILES, "entry.file-2")
            .unwrap()
            .unwrap();
        assert_eq!(value.value, events[1].change.payload);
    }

    #[test]
    fn incoming_file_entry_change_updates_rows() {
        let database_path = test_dir("incoming-file-entry").join(DEFAULT_DATABASE_FILE_NAME);
        let database = SlateProfileDatabase::open_resolved(database_path).unwrap();
        let payload = FileEntrySyncPayload {
            entry_id: "file-3".to_string(),
            sync_set_id: Some("set-media".to_string()),
            parent_id: Some("folder-1".to_string()),
            name: "song.flac".to_string(),
            entry_kind: "file".to_string(),
            content_ref: Some("bafy-song".to_string()),
            mime_type: Some("audio/flac".to_string()),
            size_bytes: Some(65_536),
            modified_at: Some(1_788_920_000),
            integrity: Some("sha256-song".to_string()),
            retention_policy: Some("pin".to_string()),
            deleted: false,
        };
        let incoming = IncomingSyncSettingText::new(
            DEFAULT_PROFILE_ID,
            SYNC_DOMAIN_FILES,
            file_entry_sync_key(payload.entry_id.as_str()),
            serde_json::to_string(&payload).unwrap(),
            "device-b",
            1,
            20,
        );

        let applied = database.apply_sync_setting_text(&incoming).unwrap();

        assert_eq!(applied.domain, SYNC_DOMAIN_FILES);
        assert_eq!(applied.entity_key, "entry.file-3");
        assert!(applied.applied_at.is_some());
        let entries = database.file_entries(DEFAULT_PROFILE_ID, 10).unwrap();
        assert_eq!(entries.len(), 1);
        assert_eq!(entries[0].entry_id, "file-3");
        assert_eq!(entries[0].parent_id.as_deref(), Some("folder-1"));
        assert_eq!(entries[0].name, "song.flac");
        assert_eq!(entries[0].content_ref.as_deref(), Some("bafy-song"));
        assert_eq!(entries[0].size_bytes, Some(65_536));
    }

    #[test]
    fn incoming_file_entry_tombstone_removes_row() {
        let database_path =
            test_dir("incoming-file-entry-tombstone").join(DEFAULT_DATABASE_FILE_NAME);
        let database = SlateProfileDatabase::open_resolved(database_path).unwrap();
        database
            .upsert_file_entry(&FileEntryUpdate {
                profile: DEFAULT_PROFILE_ID.to_string(),
                entry_id: "file-4".to_string(),
                sync_set_id: None,
                parent_id: None,
                name: "temporary.bin".to_string(),
                entry_kind: "file".to_string(),
                content_ref: Some("bafy-temporary".to_string()),
                mime_type: Some("application/octet-stream".to_string()),
                size_bytes: Some(8),
                modified_at: None,
                integrity: None,
                retention_policy: None,
            })
            .unwrap();
        let tombstone = FileEntrySyncPayload {
            entry_id: "file-4".to_string(),
            sync_set_id: None,
            parent_id: None,
            name: "temporary.bin".to_string(),
            entry_kind: "file".to_string(),
            content_ref: Some("bafy-temporary".to_string()),
            mime_type: Some("application/octet-stream".to_string()),
            size_bytes: Some(8),
            modified_at: None,
            integrity: None,
            retention_policy: None,
            deleted: true,
        };

        let applied = database
            .apply_sync_setting_text(&IncomingSyncSettingText::new(
                DEFAULT_PROFILE_ID,
                SYNC_DOMAIN_FILES,
                file_entry_sync_key("file-4"),
                serde_json::to_string(&tombstone).unwrap(),
                "zz-device",
                1,
                100,
            ))
            .unwrap();

        assert!(applied.applied_at.is_some());
        assert!(
            database
                .file_entries(DEFAULT_PROFILE_ID, 10)
                .unwrap()
                .is_empty()
        );
    }

    #[test]
    fn incoming_losing_file_entry_change_does_not_replace_winner() {
        let database_path =
            test_dir("incoming-file-entry-conflict").join(DEFAULT_DATABASE_FILE_NAME);
        let database = SlateProfileDatabase::open_resolved(database_path).unwrap();
        let winning_payload = FileEntrySyncPayload {
            entry_id: "file-5".to_string(),
            sync_set_id: Some("set-docs".to_string()),
            parent_id: None,
            name: "winner.txt".to_string(),
            entry_kind: "file".to_string(),
            content_ref: Some("bafy-winner".to_string()),
            mime_type: Some("text/plain".to_string()),
            size_bytes: Some(300),
            modified_at: Some(1_788_930_000),
            integrity: None,
            retention_policy: Some("keep-latest".to_string()),
            deleted: false,
        };
        let losing_payload = FileEntrySyncPayload {
            entry_id: "file-5".to_string(),
            sync_set_id: Some("set-docs".to_string()),
            parent_id: None,
            name: "loser.txt".to_string(),
            entry_kind: "file".to_string(),
            content_ref: Some("bafy-loser".to_string()),
            mime_type: Some("text/plain".to_string()),
            size_bytes: Some(100),
            modified_at: Some(1_788_940_000),
            integrity: None,
            retention_policy: Some("keep-latest".to_string()),
            deleted: false,
        };

        let winning = database
            .apply_sync_setting_text(&IncomingSyncSettingText::new(
                DEFAULT_PROFILE_ID,
                SYNC_DOMAIN_FILES,
                file_entry_sync_key("file-5"),
                serde_json::to_string(&winning_payload).unwrap(),
                "device-b",
                2,
                40,
            ))
            .unwrap();
        let losing = database
            .apply_sync_setting_text(&IncomingSyncSettingText::new(
                DEFAULT_PROFILE_ID,
                SYNC_DOMAIN_FILES,
                file_entry_sync_key("file-5"),
                serde_json::to_string(&losing_payload).unwrap(),
                "device-c",
                1,
                30,
            ))
            .unwrap();

        assert!(winning.applied_at.is_some());
        assert_eq!(losing.applied_at, None);
        let entries = database.file_entries(DEFAULT_PROFILE_ID, 10).unwrap();
        assert_eq!(entries.len(), 1);
        assert_eq!(entries[0].name, "winner.txt");
        assert_eq!(entries[0].content_ref.as_deref(), Some("bafy-winner"));
        assert_eq!(entries[0].size_bytes, Some(300));
        let value = database
            .get_sync_setting_text(DEFAULT_PROFILE_ID, SYNC_DOMAIN_FILES, "entry.file-5")
            .unwrap()
            .unwrap();
        assert_eq!(value.value, winning.payload);
    }

    #[test]
    fn file_entry_ids_kinds_and_sizes_are_validated() {
        let database_path = test_dir("file-entry-invalid").join(DEFAULT_DATABASE_FILE_NAME);
        let database = SlateProfileDatabase::open_resolved(database_path).unwrap();
        let invalid_id = FileEntryUpdate {
            profile: DEFAULT_PROFILE_ID.to_string(),
            entry_id: "../file".to_string(),
            sync_set_id: None,
            parent_id: None,
            name: "Invalid".to_string(),
            entry_kind: "file".to_string(),
            content_ref: None,
            mime_type: None,
            size_bytes: None,
            modified_at: None,
            integrity: None,
            retention_policy: None,
        };
        let invalid_kind = FileEntryUpdate {
            entry_id: "file-6".to_string(),
            entry_kind: "symlink".to_string(),
            ..invalid_id.clone()
        };
        let invalid_size = FileEntryUpdate {
            entry_id: "file-7".to_string(),
            entry_kind: "file".to_string(),
            size_bytes: Some(i64::MAX as u64 + 1),
            ..invalid_id.clone()
        };

        let id_error = database.upsert_file_entry(&invalid_id).unwrap_err();
        let kind_error = database.upsert_file_entry(&invalid_kind).unwrap_err();
        let size_error = database.upsert_file_entry(&invalid_size).unwrap_err();

        assert!(matches!(
            id_error,
            StorageError::InvalidFileEntryId(entry_id) if entry_id == "../file"
        ));
        assert!(matches!(
            kind_error,
            StorageError::InvalidFileEntryKind(entry_kind) if entry_kind == "symlink"
        ));
        assert!(
            matches!(size_error, StorageError::InvalidFileSize(size) if size == i64::MAX as u64 + 1)
        );
        assert!(
            database
                .file_entries(DEFAULT_PROFILE_ID, 10)
                .unwrap()
                .is_empty()
        );
    }

    #[test]
    fn storage_provider_writes_metadata_sync_change_without_secrets_or_local_state() {
        let database_path = test_dir("storage-provider-local").join(DEFAULT_DATABASE_FILE_NAME);
        let database = SlateProfileDatabase::open_resolved(database_path).unwrap();
        let provider = StorageProviderUpdate {
            profile: DEFAULT_PROFILE_ID.to_string(),
            provider_id: "provider-1".to_string(),
            provider_kind: "ipfs".to_string(),
            display_name: "Home IPFS".to_string(),
            endpoint_ref: Some("/dnsaddr/home.example.test/p2p/provider-1".to_string()),
            discovery: true,
            connectivity: true,
            object_transfer: true,
            availability: true,
            mutable_roots: false,
            quota_bytes: Some(1_048_576),
            max_retained_objects: Some(32),
            pinning_policy: Some("manual".to_string()),
            enabled: true,
        };

        let record = database.upsert_storage_provider(&provider).unwrap();

        assert_eq!(record.profile, DEFAULT_PROFILE_ID);
        assert_eq!(record.provider_id, "provider-1");
        assert_eq!(record.provider_kind, "ipfs");
        assert_eq!(record.display_name, "Home IPFS");
        assert_eq!(record.quota_bytes, Some(1_048_576));
        assert_eq!(record.max_retained_objects, Some(32));
        assert_eq!(record.pinning_policy.as_deref(), Some("manual"));
        assert!(record.enabled);
        assert_eq!(
            database.storage_providers(DEFAULT_PROFILE_ID, 10).unwrap(),
            vec![record]
        );
        let events = database
            .sync_setting_text_events_after_for_domain(
                DEFAULT_PROFILE_ID,
                SYNC_DOMAIN_STORAGE,
                0,
                10,
            )
            .unwrap();
        assert_eq!(events.len(), 1);
        assert_eq!(events[0].change.entity_key, "provider.provider-1");
        let payload: StorageProviderSyncPayload =
            serde_json::from_str(events[0].change.payload.as_str()).unwrap();
        assert_eq!(payload.provider_id, "provider-1");
        assert_eq!(payload.provider_kind, "ipfs");
        assert!(payload.discovery);
        assert!(!payload.mutable_roots);
        assert!(!payload.deleted);
        let payload_json: serde_json::Value =
            serde_json::from_str(events[0].change.payload.as_str()).unwrap();
        assert!(payload_json.get("secret").is_none());
        assert!(payload_json.get("token").is_none());
        assert!(payload_json.get("private_key").is_none());
        assert!(payload_json.get("local_path").is_none());
        assert!(payload_json.get("daemon_state").is_none());
        assert!(payload_json.get("health").is_none());
        let value = database
            .get_sync_setting_text(
                DEFAULT_PROFILE_ID,
                SYNC_DOMAIN_STORAGE,
                "provider.provider-1",
            )
            .unwrap()
            .unwrap();
        assert_eq!(value.value, events[0].change.payload);
    }

    #[test]
    fn storage_provider_removal_records_tombstone() {
        let database_path = test_dir("storage-provider-tombstone").join(DEFAULT_DATABASE_FILE_NAME);
        let database = SlateProfileDatabase::open_resolved(database_path).unwrap();
        database
            .upsert_storage_provider(&StorageProviderUpdate {
                profile: DEFAULT_PROFILE_ID.to_string(),
                provider_id: "provider-2".to_string(),
                provider_kind: "iroh".to_string(),
                display_name: "Laptop mesh".to_string(),
                endpoint_ref: Some("iroh-node:provider-2".to_string()),
                discovery: true,
                connectivity: true,
                object_transfer: true,
                availability: false,
                mutable_roots: false,
                quota_bytes: None,
                max_retained_objects: None,
                pinning_policy: Some("disabled".to_string()),
                enabled: true,
            })
            .unwrap();

        database
            .remove_storage_provider(DEFAULT_PROFILE_ID, "provider-2")
            .unwrap();

        assert!(
            database
                .storage_providers(DEFAULT_PROFILE_ID, 10)
                .unwrap()
                .is_empty()
        );
        let events = database
            .sync_setting_text_events_after_for_domain(
                DEFAULT_PROFILE_ID,
                SYNC_DOMAIN_STORAGE,
                0,
                10,
            )
            .unwrap();
        assert_eq!(events.len(), 2);
        let tombstone: StorageProviderSyncPayload =
            serde_json::from_str(events[1].change.payload.as_str()).unwrap();
        assert!(tombstone.deleted);
        assert_eq!(tombstone.provider_id, "provider-2");
        assert_eq!(tombstone.display_name, "Laptop mesh");
        let value = database
            .get_sync_setting_text(
                DEFAULT_PROFILE_ID,
                SYNC_DOMAIN_STORAGE,
                "provider.provider-2",
            )
            .unwrap()
            .unwrap();
        assert_eq!(value.value, events[1].change.payload);
    }

    #[test]
    fn incoming_storage_provider_change_updates_rows() {
        let database_path = test_dir("incoming-storage-provider").join(DEFAULT_DATABASE_FILE_NAME);
        let database = SlateProfileDatabase::open_resolved(database_path).unwrap();
        let payload = StorageProviderSyncPayload {
            provider_id: "provider-3".to_string(),
            provider_kind: "pinning".to_string(),
            display_name: "Contracted pinning".to_string(),
            endpoint_ref: Some("provider:contracted-pinning".to_string()),
            discovery: false,
            connectivity: true,
            object_transfer: true,
            availability: true,
            mutable_roots: false,
            quota_bytes: Some(9_000),
            max_retained_objects: Some(4),
            pinning_policy: Some("required".to_string()),
            enabled: false,
            deleted: false,
        };
        let incoming = IncomingSyncSettingText::new(
            DEFAULT_PROFILE_ID,
            SYNC_DOMAIN_STORAGE,
            storage_provider_sync_key(payload.provider_id.as_str()),
            serde_json::to_string(&payload).unwrap(),
            "device-b",
            1,
            20,
        );

        let applied = database.apply_sync_setting_text(&incoming).unwrap();

        assert_eq!(applied.domain, SYNC_DOMAIN_STORAGE);
        assert_eq!(applied.entity_key, "provider.provider-3");
        assert!(applied.applied_at.is_some());
        let providers = database.storage_providers(DEFAULT_PROFILE_ID, 10).unwrap();
        assert_eq!(providers.len(), 1);
        assert_eq!(providers[0].provider_id, "provider-3");
        assert_eq!(providers[0].provider_kind, "pinning");
        assert_eq!(providers[0].quota_bytes, Some(9_000));
        assert_eq!(providers[0].max_retained_objects, Some(4));
        assert_eq!(providers[0].pinning_policy.as_deref(), Some("required"));
        assert!(!providers[0].enabled);
    }

    #[test]
    fn incoming_storage_provider_tombstone_removes_row() {
        let database_path =
            test_dir("incoming-storage-provider-tombstone").join(DEFAULT_DATABASE_FILE_NAME);
        let database = SlateProfileDatabase::open_resolved(database_path).unwrap();
        database
            .upsert_storage_provider(&StorageProviderUpdate {
                profile: DEFAULT_PROFILE_ID.to_string(),
                provider_id: "provider-4".to_string(),
                provider_kind: "ipfs".to_string(),
                display_name: "Temporary provider".to_string(),
                endpoint_ref: None,
                discovery: true,
                connectivity: true,
                object_transfer: true,
                availability: true,
                mutable_roots: true,
                quota_bytes: Some(1024),
                max_retained_objects: Some(1),
                pinning_policy: Some("auto".to_string()),
                enabled: true,
            })
            .unwrap();
        let tombstone = StorageProviderSyncPayload {
            provider_id: "provider-4".to_string(),
            provider_kind: "ipfs".to_string(),
            display_name: "Temporary provider".to_string(),
            endpoint_ref: None,
            discovery: true,
            connectivity: true,
            object_transfer: true,
            availability: true,
            mutable_roots: true,
            quota_bytes: Some(1024),
            max_retained_objects: Some(1),
            pinning_policy: Some("auto".to_string()),
            enabled: true,
            deleted: true,
        };

        let applied = database
            .apply_sync_setting_text(&IncomingSyncSettingText::new(
                DEFAULT_PROFILE_ID,
                SYNC_DOMAIN_STORAGE,
                storage_provider_sync_key("provider-4"),
                serde_json::to_string(&tombstone).unwrap(),
                "zz-device",
                1,
                100,
            ))
            .unwrap();

        assert!(applied.applied_at.is_some());
        assert!(
            database
                .storage_providers(DEFAULT_PROFILE_ID, 10)
                .unwrap()
                .is_empty()
        );
    }

    #[test]
    fn incoming_losing_storage_provider_change_does_not_replace_winner() {
        let database_path =
            test_dir("incoming-storage-provider-conflict").join(DEFAULT_DATABASE_FILE_NAME);
        let database = SlateProfileDatabase::open_resolved(database_path).unwrap();
        let winning_payload = StorageProviderSyncPayload {
            provider_id: "provider-5".to_string(),
            provider_kind: "ipfs".to_string(),
            display_name: "Winner provider".to_string(),
            endpoint_ref: Some("provider:winner".to_string()),
            discovery: true,
            connectivity: true,
            object_transfer: true,
            availability: true,
            mutable_roots: true,
            quota_bytes: Some(500),
            max_retained_objects: Some(5),
            pinning_policy: Some("manual".to_string()),
            enabled: true,
            deleted: false,
        };
        let losing_payload = StorageProviderSyncPayload {
            provider_id: "provider-5".to_string(),
            provider_kind: "iroh".to_string(),
            display_name: "Loser provider".to_string(),
            endpoint_ref: Some("provider:loser".to_string()),
            discovery: false,
            connectivity: true,
            object_transfer: true,
            availability: false,
            mutable_roots: false,
            quota_bytes: Some(100),
            max_retained_objects: Some(1),
            pinning_policy: Some("disabled".to_string()),
            enabled: false,
            deleted: false,
        };

        let winning = database
            .apply_sync_setting_text(&IncomingSyncSettingText::new(
                DEFAULT_PROFILE_ID,
                SYNC_DOMAIN_STORAGE,
                storage_provider_sync_key("provider-5"),
                serde_json::to_string(&winning_payload).unwrap(),
                "device-b",
                2,
                40,
            ))
            .unwrap();
        let losing = database
            .apply_sync_setting_text(&IncomingSyncSettingText::new(
                DEFAULT_PROFILE_ID,
                SYNC_DOMAIN_STORAGE,
                storage_provider_sync_key("provider-5"),
                serde_json::to_string(&losing_payload).unwrap(),
                "device-c",
                1,
                30,
            ))
            .unwrap();

        assert!(winning.applied_at.is_some());
        assert_eq!(losing.applied_at, None);
        let providers = database.storage_providers(DEFAULT_PROFILE_ID, 10).unwrap();
        assert_eq!(providers.len(), 1);
        assert_eq!(providers[0].display_name, "Winner provider");
        assert_eq!(providers[0].provider_kind, "ipfs");
        assert_eq!(
            providers[0].endpoint_ref.as_deref(),
            Some("provider:winner")
        );
        assert_eq!(providers[0].quota_bytes, Some(500));
        assert!(providers[0].mutable_roots);
        let value = database
            .get_sync_setting_text(
                DEFAULT_PROFILE_ID,
                SYNC_DOMAIN_STORAGE,
                "provider.provider-5",
            )
            .unwrap()
            .unwrap();
        assert_eq!(value.value, winning.payload);
    }

    #[test]
    fn storage_provider_ids_kinds_quotas_and_pinning_policies_are_validated() {
        let database_path = test_dir("storage-provider-invalid").join(DEFAULT_DATABASE_FILE_NAME);
        let database = SlateProfileDatabase::open_resolved(database_path).unwrap();
        let invalid_id = StorageProviderUpdate {
            profile: DEFAULT_PROFILE_ID.to_string(),
            provider_id: "../provider".to_string(),
            provider_kind: "ipfs".to_string(),
            display_name: "Invalid".to_string(),
            endpoint_ref: None,
            discovery: false,
            connectivity: false,
            object_transfer: false,
            availability: false,
            mutable_roots: false,
            quota_bytes: None,
            max_retained_objects: None,
            pinning_policy: Some("manual".to_string()),
            enabled: false,
        };
        let invalid_kind = StorageProviderUpdate {
            provider_id: "provider-6".to_string(),
            provider_kind: "ipfs/rpc".to_string(),
            ..invalid_id.clone()
        };
        let invalid_quota = StorageProviderUpdate {
            provider_id: "provider-7".to_string(),
            provider_kind: "ipfs".to_string(),
            quota_bytes: Some(i64::MAX as u64 + 1),
            ..invalid_id.clone()
        };
        let invalid_policy = StorageProviderUpdate {
            provider_id: "provider-8".to_string(),
            provider_kind: "ipfs".to_string(),
            quota_bytes: None,
            pinning_policy: Some("secret".to_string()),
            ..invalid_id.clone()
        };

        let id_error = database.upsert_storage_provider(&invalid_id).unwrap_err();
        let kind_error = database.upsert_storage_provider(&invalid_kind).unwrap_err();
        let quota_error = database
            .upsert_storage_provider(&invalid_quota)
            .unwrap_err();
        let policy_error = database
            .upsert_storage_provider(&invalid_policy)
            .unwrap_err();

        assert!(matches!(
            id_error,
            StorageError::InvalidStorageProviderId(provider_id)
                if provider_id == "../provider"
        ));
        assert!(matches!(
            kind_error,
            StorageError::InvalidStorageProviderKind(provider_kind)
                if provider_kind == "ipfs/rpc"
        ));
        assert!(
            matches!(quota_error, StorageError::InvalidStorageProviderQuota(quota) if quota == i64::MAX as u64 + 1)
        );
        assert!(matches!(
            policy_error,
            StorageError::InvalidStoragePinningPolicy(pinning_policy)
                if pinning_policy == "secret"
        ));
        assert!(
            database
                .storage_providers(DEFAULT_PROFILE_ID, 10)
                .unwrap()
                .is_empty()
        );
    }

    #[test]
    fn incoming_setting_conflicts_use_logical_clock_and_device_tiebreak() {
        let database_path = test_dir("incoming-sync-conflict").join(DEFAULT_DATABASE_FILE_NAME);
        let database = SlateProfileDatabase::open_resolved(database_path).unwrap();

        let local = database
            .set_sync_setting_text(
                DEFAULT_PROFILE_ID,
                SYNC_DOMAIN_SETTINGS,
                "ui.theme",
                "local",
            )
            .unwrap();
        let value = database
            .get_sync_setting_text(DEFAULT_PROFILE_ID, SYNC_DOMAIN_SETTINGS, "ui.theme")
            .unwrap()
            .unwrap();

        let older = IncomingSyncSettingText::new(
            DEFAULT_PROFILE_ID,
            SYNC_DOMAIN_SETTINGS,
            "ui.theme",
            "older",
            "device-b",
            1,
            local.logical_clock - 1,
        );
        let losing_change = database.apply_sync_setting_text(&older).unwrap();
        assert_eq!(losing_change.payload, "older");
        assert_eq!(losing_change.applied_at, None);
        assert_eq!(
            database.get_setting_text("ui.theme").unwrap().as_deref(),
            Some("local")
        );
        assert_eq!(
            database
                .get_sync_setting_text(DEFAULT_PROFILE_ID, SYNC_DOMAIN_SETTINGS, "ui.theme")
                .unwrap()
                .unwrap(),
            value
        );
        assert_eq!(
            database
                .sync_revisions_after(DEFAULT_PROFILE_ID, value.revision)
                .unwrap(),
            Vec::<SyncRevisionRecord>::new()
        );

        let tied_higher_device = IncomingSyncSettingText::new(
            DEFAULT_PROFILE_ID,
            SYNC_DOMAIN_SETTINGS,
            "ui.theme",
            "remote",
            "zz-device",
            1,
            local.logical_clock,
        );
        let winning_change = database
            .apply_sync_setting_text(&tied_higher_device)
            .unwrap();
        assert_eq!(winning_change.payload, "remote");
        assert!(winning_change.applied_at.is_some());
        assert_eq!(
            database.get_setting_text("ui.theme").unwrap().as_deref(),
            Some("remote")
        );
        let remote_value = database
            .get_sync_setting_text(DEFAULT_PROFILE_ID, SYNC_DOMAIN_SETTINGS, "ui.theme")
            .unwrap()
            .unwrap();
        assert_eq!(remote_value.value, "remote");
        assert!(remote_value.revision > value.revision);
    }

    #[test]
    fn sync_setting_text_events_follow_applied_revisions_only() {
        let database_path = test_dir("sync-setting-events").join(DEFAULT_DATABASE_FILE_NAME);
        let database = SlateProfileDatabase::open_resolved(database_path).unwrap();
        let baseline_revision = database
            .sync_revisions_after(DEFAULT_PROFILE_ID, 0)
            .unwrap()
            .last()
            .map(|revision| revision.revision)
            .unwrap_or(0);

        let settings_change = database
            .set_sync_setting_text(
                DEFAULT_PROFILE_ID,
                SYNC_DOMAIN_SETTINGS,
                "ui.theme",
                "local",
            )
            .unwrap();
        let calendar_change = database
            .set_sync_setting_text(
                DEFAULT_PROFILE_ID,
                SYNC_DOMAIN_CALENDAR,
                "default_view",
                "month",
            )
            .unwrap();
        let losing_change = database
            .apply_sync_setting_text(&IncomingSyncSettingText::new(
                DEFAULT_PROFILE_ID,
                SYNC_DOMAIN_SETTINGS,
                "ui.theme",
                "older",
                "device-b",
                1,
                settings_change.logical_clock - 1,
            ))
            .unwrap();
        assert_eq!(losing_change.applied_at, None);

        let events = database
            .sync_setting_text_events_after(DEFAULT_PROFILE_ID, baseline_revision, 10)
            .unwrap();
        assert_eq!(events.len(), 2);
        assert_eq!(events[0].revision.change_id, settings_change.id);
        assert_eq!(events[0].change, settings_change);
        assert_eq!(events[1].revision.change_id, calendar_change.id);
        assert_eq!(events[1].change, calendar_change);
        assert!(events.iter().all(|event| event.change.applied_at.is_some()));
        assert!(
            !events
                .iter()
                .any(|event| event.change.id == losing_change.id)
        );

        let first_batch = database
            .sync_setting_text_events_after(DEFAULT_PROFILE_ID, baseline_revision, 1)
            .unwrap();
        assert_eq!(first_batch, vec![events[0].clone()]);

        let after_first = database
            .sync_setting_text_events_after(DEFAULT_PROFILE_ID, events[0].revision.revision, 10)
            .unwrap();
        assert_eq!(after_first, vec![events[1].clone()]);
        assert_eq!(
            database.latest_sync_revision(DEFAULT_PROFILE_ID).unwrap(),
            events[1].revision.revision
        );
    }

    #[test]
    fn sync_setting_text_events_can_be_scoped_to_one_app_domain() {
        let database_path = test_dir("sync-setting-domain-events").join(DEFAULT_DATABASE_FILE_NAME);
        let database = SlateProfileDatabase::open_resolved(database_path).unwrap();
        let baseline_revision = database.latest_sync_revision(DEFAULT_PROFILE_ID).unwrap();

        let settings_change = database
            .set_sync_setting_text(
                DEFAULT_PROFILE_ID,
                SYNC_DOMAIN_SETTINGS,
                "ui.theme",
                "slate",
            )
            .unwrap();
        let first_calendar_change = database
            .set_sync_setting_text(
                DEFAULT_PROFILE_ID,
                SYNC_DOMAIN_CALENDAR,
                "default_view",
                "month",
            )
            .unwrap();
        database
            .set_sync_setting_text(
                DEFAULT_PROFILE_ID,
                SYNC_DOMAIN_DOWNLOADS,
                "last_filter",
                "active",
            )
            .unwrap();
        let second_calendar_change = database
            .set_sync_setting_text(DEFAULT_PROFILE_ID, SYNC_DOMAIN_CALENDAR, "timezone", "UTC")
            .unwrap();

        let settings_events = database
            .sync_setting_text_events_after_for_domain(
                DEFAULT_PROFILE_ID,
                SYNC_DOMAIN_SETTINGS,
                baseline_revision,
                10,
            )
            .unwrap();
        assert_eq!(settings_events.len(), 1);
        assert_eq!(settings_events[0].change, settings_change);
        assert_eq!(
            database
                .latest_sync_revision_for_domain(DEFAULT_PROFILE_ID, SYNC_DOMAIN_SETTINGS)
                .unwrap(),
            settings_events[0].revision.revision
        );

        let calendar_events = database
            .sync_setting_text_events_after_for_domain(
                DEFAULT_PROFILE_ID,
                SYNC_DOMAIN_CALENDAR,
                baseline_revision,
                10,
            )
            .unwrap();
        assert_eq!(
            calendar_events
                .iter()
                .map(|event| event.change.clone())
                .collect::<Vec<_>>(),
            vec![first_calendar_change.clone(), second_calendar_change]
        );
        assert_eq!(
            database
                .latest_sync_revision_for_domain(DEFAULT_PROFILE_ID, SYNC_DOMAIN_CALENDAR)
                .unwrap(),
            calendar_events[1].revision.revision
        );
        assert_eq!(
            database
                .latest_sync_revision_for_domain(DEFAULT_PROFILE_ID, "unknown-domain")
                .unwrap(),
            0
        );

        let first_calendar_batch = database
            .sync_setting_text_events_after_for_domain(
                DEFAULT_PROFILE_ID,
                SYNC_DOMAIN_CALENDAR,
                baseline_revision,
                1,
            )
            .unwrap();
        assert_eq!(first_calendar_batch, vec![calendar_events[0].clone()]);
        let after_first_calendar = database
            .sync_setting_text_events_after_for_domain(
                DEFAULT_PROFILE_ID,
                SYNC_DOMAIN_CALENDAR,
                calendar_events[0].revision.revision,
                10,
            )
            .unwrap();
        assert_eq!(after_first_calendar, vec![calendar_events[1].clone()]);
    }

    #[test]
    fn sync_setting_text_domain_poll_tracks_one_app_cursor() {
        let database_path = test_dir("sync-setting-domain-poll").join(DEFAULT_DATABASE_FILE_NAME);
        let database = SlateProfileDatabase::open_resolved(database_path).unwrap();
        let baseline_revision = database.latest_sync_revision(DEFAULT_PROFILE_ID).unwrap();

        database
            .set_sync_setting_text(
                DEFAULT_PROFILE_ID,
                SYNC_DOMAIN_SETTINGS,
                "ui.theme",
                "slate",
            )
            .unwrap();
        let first_calendar_change = database
            .set_sync_setting_text(
                DEFAULT_PROFILE_ID,
                SYNC_DOMAIN_CALENDAR,
                "default_view",
                "month",
            )
            .unwrap();
        database
            .set_sync_setting_text(
                DEFAULT_PROFILE_ID,
                SYNC_DOMAIN_DOWNLOADS,
                "last_filter",
                "active",
            )
            .unwrap();
        let second_calendar_change = database
            .set_sync_setting_text(DEFAULT_PROFILE_ID, SYNC_DOMAIN_CALENDAR, "timezone", "UTC")
            .unwrap();

        let first_poll = database
            .poll_sync_setting_text_events_for_domain(
                DEFAULT_PROFILE_ID,
                SYNC_DOMAIN_CALENDAR,
                baseline_revision,
                1,
            )
            .unwrap();
        assert_eq!(first_poll.profile, DEFAULT_PROFILE_ID);
        assert_eq!(first_poll.domain, SYNC_DOMAIN_CALENDAR);
        assert_eq!(first_poll.previous_revision, baseline_revision);
        assert!(first_poll.advanced());
        assert_eq!(first_poll.event_count(), 1);
        assert_eq!(first_poll.events[0].change, first_calendar_change);
        assert_eq!(
            first_poll.latest_revision,
            first_poll.events[0].revision.revision
        );

        let second_poll = database
            .poll_sync_setting_text_events_for_domain(
                DEFAULT_PROFILE_ID,
                SYNC_DOMAIN_CALENDAR,
                first_poll.latest_revision,
                8,
            )
            .unwrap();
        assert!(second_poll.advanced());
        assert_eq!(second_poll.event_count(), 1);
        assert_eq!(second_poll.events[0].change, second_calendar_change);

        let idle_poll = database
            .poll_sync_setting_text_events_for_domain(
                DEFAULT_PROFILE_ID,
                SYNC_DOMAIN_CALENDAR,
                second_poll.latest_revision,
                8,
            )
            .unwrap();
        assert!(!idle_poll.advanced());
        assert_eq!(idle_poll.latest_revision, second_poll.latest_revision);
        assert!(idle_poll.events.is_empty());
    }

    #[test]
    fn app_sync_domain_cursors_persist_independent_progress() {
        let database_path = test_dir("app-sync-domain-cursors").join(DEFAULT_DATABASE_FILE_NAME);
        let database = SlateProfileDatabase::open_resolved(database_path).unwrap();
        let baseline_revision = database.latest_sync_revision(DEFAULT_PROFILE_ID).unwrap();

        assert_eq!(
            database
                .app_sync_domain_cursor(DEFAULT_PROFILE_ID, SYNC_DOMAIN_CALENDAR)
                .unwrap(),
            None
        );

        database
            .set_sync_setting_text(
                DEFAULT_PROFILE_ID,
                SYNC_DOMAIN_SETTINGS,
                "ui.theme",
                "slate",
            )
            .unwrap();
        database
            .set_sync_setting_text(
                DEFAULT_PROFILE_ID,
                SYNC_DOMAIN_CALENDAR,
                "default_view",
                "month",
            )
            .unwrap();
        database
            .set_sync_setting_text(DEFAULT_PROFILE_ID, SYNC_DOMAIN_CALENDAR, "timezone", "UTC")
            .unwrap();
        let downloads_change = database
            .set_sync_setting_text(
                DEFAULT_PROFILE_ID,
                SYNC_DOMAIN_DOWNLOADS,
                "last_filter",
                "active",
            )
            .unwrap();

        let first_poll = database
            .poll_sync_setting_text_events_for_domain(
                DEFAULT_PROFILE_ID,
                SYNC_DOMAIN_CALENDAR,
                baseline_revision,
                1,
            )
            .unwrap();
        let first_cursor = database
            .record_app_sync_domain_cursor(
                DEFAULT_PROFILE_ID,
                SYNC_DOMAIN_CALENDAR,
                first_poll.latest_revision,
            )
            .unwrap();
        assert_eq!(first_cursor.profile, DEFAULT_PROFILE_ID);
        assert_eq!(first_cursor.domain, SYNC_DOMAIN_CALENDAR);
        assert_eq!(first_cursor.latest_revision, first_poll.latest_revision);

        let stale_cursor = database
            .record_app_sync_domain_cursor(
                DEFAULT_PROFILE_ID,
                SYNC_DOMAIN_CALENDAR,
                baseline_revision,
            )
            .unwrap();
        assert_eq!(stale_cursor.latest_revision, first_poll.latest_revision);

        let second_poll = database
            .poll_sync_setting_text_events_for_domain(
                DEFAULT_PROFILE_ID,
                SYNC_DOMAIN_CALENDAR,
                stale_cursor.latest_revision,
                8,
            )
            .unwrap();
        let final_cursor = database
            .record_app_sync_domain_cursor(
                DEFAULT_PROFILE_ID,
                SYNC_DOMAIN_CALENDAR,
                second_poll.latest_revision,
            )
            .unwrap();
        assert_eq!(final_cursor.latest_revision, second_poll.latest_revision);
        assert_eq!(
            database
                .app_sync_domain_cursor(DEFAULT_PROFILE_ID, SYNC_DOMAIN_CALENDAR)
                .unwrap(),
            Some(final_cursor.clone())
        );
        assert!(final_cursor.latest_revision < downloads_change.id);
        assert_eq!(
            database.latest_sync_revision(DEFAULT_PROFILE_ID).unwrap(),
            downloads_change.id
        );
    }

    #[test]
    fn app_sync_domain_poll_initializes_missing_cursor_at_domain_head() {
        let database_path =
            test_dir("app-sync-domain-poll-initial").join(DEFAULT_DATABASE_FILE_NAME);
        let database = SlateProfileDatabase::open_resolved(database_path).unwrap();

        database
            .set_sync_setting_text(
                DEFAULT_PROFILE_ID,
                SYNC_DOMAIN_CALENDAR,
                "default_view",
                "month",
            )
            .unwrap();
        database
            .set_sync_setting_text(DEFAULT_PROFILE_ID, SYNC_DOMAIN_CALENDAR, "timezone", "UTC")
            .unwrap();
        database
            .set_sync_setting_text(
                DEFAULT_PROFILE_ID,
                SYNC_DOMAIN_DOWNLOADS,
                "last_filter",
                "active",
            )
            .unwrap();
        let calendar_head_revision = database
            .latest_sync_revision_for_domain(DEFAULT_PROFILE_ID, SYNC_DOMAIN_CALENDAR)
            .unwrap();
        let global_head_revision = database.latest_sync_revision(DEFAULT_PROFILE_ID).unwrap();
        assert!(calendar_head_revision < global_head_revision);

        let poll = database
            .poll_sync_setting_text_events_for_app_domain(
                DEFAULT_PROFILE_ID,
                SYNC_DOMAIN_CALENDAR,
                8,
            )
            .unwrap();
        assert_eq!(poll.profile, DEFAULT_PROFILE_ID);
        assert_eq!(poll.domain, SYNC_DOMAIN_CALENDAR);
        assert_eq!(poll.previous_revision, calendar_head_revision);
        assert_eq!(poll.latest_revision, calendar_head_revision);
        assert!(poll.events.is_empty());
        assert_eq!(
            database
                .app_sync_domain_cursor(DEFAULT_PROFILE_ID, SYNC_DOMAIN_CALENDAR)
                .unwrap()
                .map(|cursor| cursor.latest_revision),
            Some(calendar_head_revision)
        );
        assert_eq!(
            database.latest_sync_revision(DEFAULT_PROFILE_ID).unwrap(),
            global_head_revision
        );
    }

    #[test]
    fn app_sync_domain_poll_resumes_from_persisted_cursor_after_partial_batch() {
        let database_path =
            test_dir("app-sync-domain-poll-resume").join(DEFAULT_DATABASE_FILE_NAME);
        let database = SlateProfileDatabase::open_resolved(database_path).unwrap();

        let first_calendar_change = database
            .set_sync_setting_text(
                DEFAULT_PROFILE_ID,
                SYNC_DOMAIN_CALENDAR,
                "default_view",
                "month",
            )
            .unwrap();
        let second_calendar_change = database
            .set_sync_setting_text(DEFAULT_PROFILE_ID, SYNC_DOMAIN_CALENDAR, "timezone", "UTC")
            .unwrap();
        let calendar_events = database
            .sync_setting_text_events_after_for_domain(
                DEFAULT_PROFILE_ID,
                SYNC_DOMAIN_CALENDAR,
                0,
                8,
            )
            .unwrap();
        assert_eq!(calendar_events.len(), 2);
        assert_eq!(calendar_events[0].change, first_calendar_change);
        assert_eq!(calendar_events[1].change, second_calendar_change);
        let first_revision = calendar_events[0].revision.revision;
        let second_revision = calendar_events[1].revision.revision;
        database
            .record_app_sync_domain_cursor(DEFAULT_PROFILE_ID, SYNC_DOMAIN_CALENDAR, first_revision)
            .unwrap();

        let poll = database
            .poll_sync_setting_text_events_for_app_domain(
                DEFAULT_PROFILE_ID,
                SYNC_DOMAIN_CALENDAR,
                1,
            )
            .unwrap();
        assert_eq!(poll.previous_revision, first_revision);
        assert_eq!(poll.latest_revision, second_revision);
        assert_eq!(poll.events.len(), 1);
        assert_eq!(poll.events[0].change, calendar_events[1].change);
        assert_eq!(
            database
                .app_sync_domain_cursor(DEFAULT_PROFILE_ID, SYNC_DOMAIN_CALENDAR)
                .unwrap()
                .map(|cursor| cursor.latest_revision),
            Some(first_revision)
        );

        let cursor = database.record_app_sync_domain_poll_cursor(&poll).unwrap();
        assert_eq!(cursor.latest_revision, second_revision);
    }

    #[test]
    fn typed_app_sync_domain_poll_decodes_payloads_and_records_cursor() {
        let database_path = test_dir("typed-app-sync-domain-poll").join(DEFAULT_DATABASE_FILE_NAME);
        let database = SlateProfileDatabase::open_resolved(database_path).unwrap();

        let initial_poll = database
            .poll_typed_sync_setting_text_events_for_app_domain::<ChatConversationSyncPayload>(
                DEFAULT_PROFILE_ID,
                SYNC_DOMAIN_CHAT,
                8,
            )
            .unwrap();
        assert!(!initial_poll.advanced());
        assert!(initial_poll.events.is_empty());

        database
            .upsert_chat_conversation(&ChatConversationUpdate {
                profile: DEFAULT_PROFILE_ID.to_string(),
                conversation_id: "chat-watch-1".to_string(),
                provider_id: Some("whatsapp".to_string()),
                external_thread_id: Some("team@example.test".to_string()),
                display_name: "Team".to_string(),
                avatar_key: Some("chat-avatar:chat-watch-1".to_string()),
                last_message_at: Some(1_789_060_000),
                unread_count: 2,
                archived: false,
                muted: false,
            })
            .unwrap();
        database
            .upsert_chat_conversation(&ChatConversationUpdate {
                profile: DEFAULT_PROFILE_ID.to_string(),
                conversation_id: "chat-watch-2".to_string(),
                provider_id: Some("sms".to_string()),
                external_thread_id: Some("+15550101010".to_string()),
                display_name: "SMS".to_string(),
                avatar_key: None,
                last_message_at: Some(1_789_060_100),
                unread_count: 1,
                archived: false,
                muted: true,
            })
            .unwrap();

        let first_poll = database
            .poll_typed_sync_setting_text_events_for_app_domain::<ChatConversationSyncPayload>(
                DEFAULT_PROFILE_ID,
                SYNC_DOMAIN_CHAT,
                1,
            )
            .unwrap();
        assert!(first_poll.advanced());
        assert_eq!(first_poll.previous_revision, initial_poll.latest_revision);
        assert_eq!(first_poll.event_count(), 1);
        assert_eq!(first_poll.events[0].change.domain, SYNC_DOMAIN_CHAT);
        assert_eq!(
            first_poll.events[0].change.entity_key,
            "conversation.chat-watch-1"
        );
        assert_eq!(first_poll.events[0].value.conversation_id, "chat-watch-1");
        assert_eq!(first_poll.events[0].value.display_name, "Team");
        assert_eq!(first_poll.events[0].value.unread_count, 2);
        assert!(!first_poll.events[0].value.deleted);
        assert_eq!(
            database
                .app_sync_domain_cursor(DEFAULT_PROFILE_ID, SYNC_DOMAIN_CHAT)
                .unwrap()
                .map(|cursor| cursor.latest_revision),
            Some(initial_poll.latest_revision)
        );

        let cursor = database
            .record_typed_app_sync_domain_poll_cursor(&first_poll)
            .unwrap();
        assert_eq!(cursor.latest_revision, first_poll.latest_revision);

        let second_poll = database
            .poll_typed_sync_setting_text_events_for_app_domain::<ChatConversationSyncPayload>(
                DEFAULT_PROFILE_ID,
                SYNC_DOMAIN_CHAT,
                8,
            )
            .unwrap();
        assert!(second_poll.advanced());
        assert_eq!(second_poll.previous_revision, first_poll.latest_revision);
        assert_eq!(second_poll.event_count(), 1);
        assert_eq!(second_poll.events[0].value.conversation_id, "chat-watch-2");
        assert_eq!(
            second_poll.events[0].value.provider_id.as_deref(),
            Some("sms")
        );
        assert!(second_poll.events[0].value.muted);
    }

    #[test]
    fn typed_app_sync_domain_poll_decode_error_does_not_advance_cursor() {
        let database_path =
            test_dir("typed-app-sync-domain-poll-decode-error").join(DEFAULT_DATABASE_FILE_NAME);
        let database = SlateProfileDatabase::open_resolved(database_path).unwrap();

        let initial_poll = database
            .poll_typed_sync_setting_text_events_for_app_domain::<ChatConversationSyncPayload>(
                DEFAULT_PROFILE_ID,
                SYNC_DOMAIN_CHAT,
                8,
            )
            .unwrap();
        let initial_revision = initial_poll.latest_revision;
        database
            .set_sync_setting_text(
                DEFAULT_PROFILE_ID,
                SYNC_DOMAIN_CHAT,
                "conversation.bad",
                "{not-json",
            )
            .unwrap();

        let error = database
            .poll_typed_sync_setting_text_events_for_app_domain::<ChatConversationSyncPayload>(
                DEFAULT_PROFILE_ID,
                SYNC_DOMAIN_CHAT,
                8,
            )
            .expect_err("malformed typed payload should fail before cursor advancement");
        assert!(matches!(error, StorageError::DecodeSyncPayload(_)));
        assert_eq!(
            database
                .app_sync_domain_cursor(DEFAULT_PROFILE_ID, SYNC_DOMAIN_CHAT)
                .unwrap()
                .map(|cursor| cursor.latest_revision),
            Some(initial_revision)
        );
    }

    #[test]
    fn app_sync_domain_watcher_polls_and_acknowledges_batches() {
        let database_path = test_dir("app-sync-domain-watcher").join(DEFAULT_DATABASE_FILE_NAME);
        let database = SlateProfileDatabase::open_resolved(database_path).unwrap();
        database
            .set_sync_setting_text(
                DEFAULT_PROFILE_ID,
                SYNC_DOMAIN_SETTINGS,
                "chrome.zoom",
                "1.02",
            )
            .unwrap();
        let settings_head = database
            .latest_sync_revision_for_domain(DEFAULT_PROFILE_ID, SYNC_DOMAIN_SETTINGS)
            .unwrap();
        database
            .set_sync_setting_text(
                DEFAULT_PROFILE_ID,
                SYNC_DOMAIN_DOWNLOADS,
                "last_filter",
                "active",
            )
            .unwrap();
        assert!(
            database.latest_sync_revision(DEFAULT_PROFILE_ID).unwrap() > settings_head,
            "the watcher cursor should use the domain head, not the profile head"
        );

        let watcher = AppSyncDomainWatcher::new(
            database.clone(),
            DEFAULT_PROFILE_ID,
            SYNC_DOMAIN_SETTINGS,
            1,
        )
        .unwrap();
        assert_eq!(watcher.profile(), DEFAULT_PROFILE_ID);
        assert_eq!(watcher.domain(), SYNC_DOMAIN_SETTINGS);
        assert_eq!(watcher.batch_limit(), 1);
        assert_eq!(watcher.current_revision().unwrap(), settings_head);
        let idle = watcher.poll_once().unwrap();
        assert_eq!(idle.previous_revision, settings_head);
        assert_eq!(idle.latest_revision, settings_head);
        assert_eq!(idle.event_count(), 0);

        database
            .set_sync_setting_text(
                DEFAULT_PROFILE_ID,
                SYNC_DOMAIN_SETTINGS,
                "chrome.zoom",
                "1.03",
            )
            .unwrap();
        database
            .set_sync_setting_text(
                DEFAULT_PROFILE_ID,
                SYNC_DOMAIN_SETTINGS,
                "keybindings.next_tab",
                "Alt+ArrowRight",
            )
            .unwrap();

        let first = watcher.poll_once().unwrap();
        assert!(first.advanced());
        assert_eq!(first.previous_revision, settings_head);
        assert_eq!(first.event_count(), 1);
        assert_eq!(first.events[0].change.entity_key, "chrome.zoom");
        assert_eq!(first.events[0].change.payload, "1.03");
        assert_eq!(watcher.current_revision().unwrap(), settings_head);
        let cursor = watcher.acknowledge(&first).unwrap();
        assert_eq!(cursor.latest_revision, first.latest_revision);
        assert_eq!(watcher.current_revision().unwrap(), first.latest_revision);

        let second = watcher.poll_once().unwrap();
        assert!(second.advanced());
        assert_eq!(second.previous_revision, first.latest_revision);
        assert_eq!(second.event_count(), 1);
        assert_eq!(second.events[0].change.entity_key, "keybindings.next_tab");
        assert_eq!(second.events[0].change.payload, "Alt+ArrowRight");
        assert_eq!(watcher.current_revision().unwrap(), first.latest_revision);

        let apply_error = watcher
            .poll_apply_and_acknowledge(|poll| {
                assert_eq!(poll.event_count(), 1);
                assert_eq!(poll.events[0].change.entity_key, "keybindings.next_tab");
                Err("app apply failed")
            })
            .expect_err("failed app apply should not acknowledge the batch");
        assert!(matches!(
            apply_error,
            AppSyncDomainWatcherApplyError::Apply("app apply failed")
        ));
        assert_eq!(watcher.current_revision().unwrap(), first.latest_revision);

        let applied = watcher
            .poll_apply_and_acknowledge(|poll| {
                assert_eq!(poll.previous_revision, first.latest_revision);
                assert_eq!(poll.event_count(), 1);
                assert_eq!(poll.events[0].change.entity_key, "keybindings.next_tab");
                Ok::<(), &'static str>(())
            })
            .unwrap();
        assert_eq!(applied.poll.event_count(), 1);
        assert_eq!(applied.cursor.latest_revision, applied.poll.latest_revision);
        assert_eq!(
            watcher.current_revision().unwrap(),
            applied.poll.latest_revision
        );
    }

    #[test]
    fn typed_app_sync_domain_watcher_polls_and_acknowledges_batches() {
        let database_path =
            test_dir("typed-app-sync-domain-watcher").join(DEFAULT_DATABASE_FILE_NAME);
        let database = SlateProfileDatabase::open_resolved(database_path).unwrap();
        database
            .upsert_chat_conversation(&ChatConversationUpdate {
                profile: DEFAULT_PROFILE_ID.to_string(),
                conversation_id: "chat-watcher-history".to_string(),
                provider_id: Some("whatsapp".to_string()),
                external_thread_id: Some("history@example.test".to_string()),
                display_name: "History".to_string(),
                avatar_key: None,
                last_message_at: Some(1_789_070_000),
                unread_count: 0,
                archived: false,
                muted: false,
            })
            .unwrap();
        let chat_head = database
            .latest_sync_revision_for_domain(DEFAULT_PROFILE_ID, SYNC_DOMAIN_CHAT)
            .unwrap();
        database
            .set_sync_setting_text(
                DEFAULT_PROFILE_ID,
                SYNC_DOMAIN_DOWNLOADS,
                "last_filter",
                "active",
            )
            .unwrap();
        assert!(database.latest_sync_revision(DEFAULT_PROFILE_ID).unwrap() > chat_head);

        let watcher = TypedAppSyncDomainWatcher::<ChatConversationSyncPayload>::new(
            database.clone(),
            DEFAULT_PROFILE_ID,
            SYNC_DOMAIN_CHAT,
            1,
        )
        .unwrap();
        assert_eq!(watcher.profile(), DEFAULT_PROFILE_ID);
        assert_eq!(watcher.domain(), SYNC_DOMAIN_CHAT);
        assert_eq!(watcher.batch_limit(), 1);
        assert_eq!(watcher.current_revision().unwrap(), chat_head);
        let idle = watcher.poll_once().unwrap();
        assert_eq!(idle.previous_revision, chat_head);
        assert_eq!(idle.latest_revision, chat_head);
        assert_eq!(idle.event_count(), 0);

        database
            .upsert_chat_conversation(&ChatConversationUpdate {
                profile: DEFAULT_PROFILE_ID.to_string(),
                conversation_id: "chat-watcher-1".to_string(),
                provider_id: Some("whatsapp".to_string()),
                external_thread_id: Some("one@example.test".to_string()),
                display_name: "One".to_string(),
                avatar_key: None,
                last_message_at: Some(1_789_070_100),
                unread_count: 1,
                archived: false,
                muted: false,
            })
            .unwrap();
        database
            .upsert_chat_conversation(&ChatConversationUpdate {
                profile: DEFAULT_PROFILE_ID.to_string(),
                conversation_id: "chat-watcher-2".to_string(),
                provider_id: Some("sms".to_string()),
                external_thread_id: Some("+15550102020".to_string()),
                display_name: "Two".to_string(),
                avatar_key: None,
                last_message_at: Some(1_789_070_200),
                unread_count: 2,
                archived: false,
                muted: true,
            })
            .unwrap();

        let first = watcher.poll_once().unwrap();
        assert!(first.advanced());
        assert_eq!(first.previous_revision, chat_head);
        assert_eq!(first.event_count(), 1);
        assert_eq!(first.events[0].value.conversation_id, "chat-watcher-1");
        assert_eq!(first.events[0].value.display_name, "One");
        assert_eq!(watcher.current_revision().unwrap(), chat_head);
        let cursor = watcher.acknowledge(&first).unwrap();
        assert_eq!(cursor.latest_revision, first.latest_revision);
        assert_eq!(watcher.current_revision().unwrap(), first.latest_revision);

        let second = watcher.poll_once().unwrap();
        assert!(second.advanced());
        assert_eq!(second.previous_revision, first.latest_revision);
        assert_eq!(second.event_count(), 1);
        assert_eq!(second.events[0].value.conversation_id, "chat-watcher-2");
        assert_eq!(second.events[0].value.provider_id.as_deref(), Some("sms"));
        assert!(second.events[0].value.muted);
        assert_eq!(watcher.current_revision().unwrap(), first.latest_revision);

        let apply_error = watcher
            .poll_apply_and_acknowledge(|poll| {
                assert_eq!(poll.event_count(), 1);
                assert_eq!(poll.events[0].value.conversation_id, "chat-watcher-2");
                Err("app apply failed")
            })
            .expect_err("failed app apply should not acknowledge the batch");
        assert!(matches!(
            apply_error,
            TypedAppSyncDomainWatcherApplyError::Apply("app apply failed")
        ));
        assert_eq!(watcher.current_revision().unwrap(), first.latest_revision);

        let applied = watcher
            .poll_apply_and_acknowledge(|poll| {
                assert_eq!(poll.previous_revision, first.latest_revision);
                assert_eq!(poll.event_count(), 1);
                assert_eq!(poll.events[0].value.conversation_id, "chat-watcher-2");
                Ok::<(), &'static str>(())
            })
            .unwrap();
        assert_eq!(applied.poll.event_count(), 1);
        assert_eq!(applied.cursor.latest_revision, applied.poll.latest_revision);
        assert_eq!(
            watcher.current_revision().unwrap(),
            applied.poll.latest_revision
        );
    }

    #[test]
    fn typed_app_sync_domain_watcher_decode_error_keeps_cursor() {
        let database_path =
            test_dir("typed-app-sync-domain-watcher-decode-error").join(DEFAULT_DATABASE_FILE_NAME);
        let database = SlateProfileDatabase::open_resolved(database_path).unwrap();
        let watcher = TypedAppSyncDomainWatcher::<ChatConversationSyncPayload>::new(
            database.clone(),
            DEFAULT_PROFILE_ID,
            SYNC_DOMAIN_CHAT,
            8,
        )
        .unwrap();
        let initial_revision = watcher.current_revision().unwrap();
        database
            .set_sync_setting_text(
                DEFAULT_PROFILE_ID,
                SYNC_DOMAIN_CHAT,
                "conversation.bad",
                "{not-json",
            )
            .unwrap();

        let error = watcher
            .poll_once()
            .expect_err("malformed watcher payload should fail before acknowledgement");
        assert!(matches!(error, StorageError::DecodeSyncPayload(_)));
        assert_eq!(watcher.current_revision().unwrap(), initial_revision);
    }

    #[test]
    fn app_sync_domain_cursor_rejects_invalid_inputs() {
        let database_path =
            test_dir("app-sync-domain-cursor-invalid").join(DEFAULT_DATABASE_FILE_NAME);
        let database = SlateProfileDatabase::open_resolved(database_path).unwrap();

        let empty_domain_error = database
            .app_sync_domain_cursor(DEFAULT_PROFILE_ID, "")
            .expect_err("empty domain should be invalid");
        assert!(matches!(
            empty_domain_error,
            StorageError::InvalidSyncDomain(domain) if domain.is_empty()
        ));

        assert!(matches!(
            database
                .record_app_sync_domain_cursor(DEFAULT_PROFILE_ID, SYNC_DOMAIN_CALENDAR, -1)
                .expect_err("negative revision should be invalid"),
            StorageError::InvalidSyncRevision(-1)
        ));
    }

    #[test]
    fn sync_snapshot_metadata_tracks_compacted_revisions() {
        let database_path = test_dir("sync-snapshots").join(DEFAULT_DATABASE_FILE_NAME);
        let database = SlateProfileDatabase::open_resolved(database_path).unwrap();

        assert_eq!(
            database.latest_sync_snapshot(DEFAULT_PROFILE_ID).unwrap(),
            None
        );

        database.set_setting_text("ui.theme", "teal").unwrap();
        let first_revision = database
            .sync_revisions_after(DEFAULT_PROFILE_ID, 0)
            .unwrap()
            .last()
            .expect("first revision")
            .revision;
        let first_snapshot = database
            .record_sync_snapshot(&SyncSnapshotRegistration {
                profile: DEFAULT_PROFILE_ID.to_string(),
                snapshot_id: "snapshot-1".to_string(),
                backend_object_id: Some("local-object-1".to_string()),
                covers_revision: first_revision,
                included_domains: vec![
                    SYNC_DOMAIN_SETTINGS.to_string(),
                    SYNC_DOMAIN_BOOKMARKS.to_string(),
                ],
            })
            .unwrap();

        assert_eq!(first_snapshot.snapshot_id, "snapshot-1");
        assert_eq!(
            first_snapshot.backend_object_id.as_deref(),
            Some("local-object-1")
        );
        assert_eq!(first_snapshot.covers_revision, first_revision);
        assert_eq!(
            first_snapshot.included_domains,
            vec![
                SYNC_DOMAIN_BOOKMARKS.to_string(),
                SYNC_DOMAIN_SETTINGS.to_string()
            ]
        );

        database.set_setting_text("chrome.zoom", "1.10").unwrap();
        let second_revision = database
            .sync_revisions_after(DEFAULT_PROFILE_ID, first_revision)
            .unwrap()
            .last()
            .expect("second revision")
            .revision;
        database
            .record_sync_snapshot(&SyncSnapshotRegistration {
                profile: DEFAULT_PROFILE_ID.to_string(),
                snapshot_id: "snapshot-2".to_string(),
                backend_object_id: Some("local-object-2".to_string()),
                covers_revision: second_revision,
                included_domains: vec![SYNC_DOMAIN_SETTINGS.to_string()],
            })
            .unwrap();

        let latest = database
            .latest_sync_snapshot(DEFAULT_PROFILE_ID)
            .unwrap()
            .expect("latest snapshot");
        assert_eq!(latest.snapshot_id, "snapshot-2");
        assert_eq!(latest.covers_revision, second_revision);

        let after_first = database
            .sync_snapshots_after(DEFAULT_PROFILE_ID, first_revision)
            .unwrap();
        assert_eq!(after_first.len(), 1);
        assert_eq!(after_first[0].snapshot_id, "snapshot-2");

        let updated_first = database
            .record_sync_snapshot(&SyncSnapshotRegistration {
                profile: DEFAULT_PROFILE_ID.to_string(),
                snapshot_id: "snapshot-1".to_string(),
                backend_object_id: Some("local-object-1b".to_string()),
                covers_revision: second_revision,
                included_domains: vec![SYNC_DOMAIN_SETTINGS.to_string()],
            })
            .unwrap();
        assert_eq!(
            updated_first.backend_object_id.as_deref(),
            Some("local-object-1b")
        );
        assert_eq!(updated_first.covers_revision, second_revision);

        let snapshot = database
            .sync_snapshot(DEFAULT_PROFILE_ID, "snapshot-1")
            .unwrap()
            .expect("snapshot-1");
        assert_eq!(snapshot, updated_first);
    }

    #[test]
    fn settings_sync_compaction_target_uses_retention_policy_and_latest_snapshot() {
        let database_path = test_dir("sync-compaction-target").join(DEFAULT_DATABASE_FILE_NAME);
        let database = SlateProfileDatabase::open_resolved(database_path).unwrap();
        let baseline_revision = database.latest_sync_revision(DEFAULT_PROFILE_ID).unwrap();
        if baseline_revision > 0 {
            database
                .record_sync_snapshot(&SyncSnapshotRegistration {
                    profile: DEFAULT_PROFILE_ID.to_string(),
                    snapshot_id: "snapshot-baseline".to_string(),
                    backend_object_id: Some("snapshot-object-baseline".to_string()),
                    covers_revision: baseline_revision,
                    included_domains: vec![SYNC_DOMAIN_SETTINGS.to_string()],
                })
                .unwrap();
        }

        for index in 0..5 {
            database
                .set_sync_setting_text(
                    DEFAULT_PROFILE_ID,
                    SYNC_DOMAIN_SETTINGS,
                    &format!("setting.{index}"),
                    &format!("value-{index}"),
                )
                .unwrap();
        }

        let policy = ProfileSyncRetentionPolicy {
            min_tail_change_count: 2,
            change_retention_seconds: 0,
            inactive_device_grace_seconds: DEFAULT_PROFILE_SYNC_INACTIVE_DEVICE_GRACE_SECONDS,
        };
        let events = database
            .sync_setting_text_events_after(DEFAULT_PROFILE_ID, baseline_revision, 10)
            .unwrap();
        assert_eq!(events.len(), 5);

        let target = database
            .settings_sync_compaction_target(DEFAULT_PROFILE_ID, &policy, i64::MAX)
            .unwrap()
            .expect("compaction target");
        assert_eq!(target.previous_snapshot_covers_revision, baseline_revision);
        assert_eq!(target.covers_revision, events[2].revision.revision);
        assert_eq!(target.covers_change_id, events[2].change.id);
        assert_eq!(target.covered_change_count, 3);
        assert_eq!(target.retained_tail_change_count, 2);

        database
            .record_sync_snapshot(&SyncSnapshotRegistration {
                profile: DEFAULT_PROFILE_ID.to_string(),
                snapshot_id: "snapshot-compacted".to_string(),
                backend_object_id: Some("snapshot-object-compacted".to_string()),
                covers_revision: target.covers_revision,
                included_domains: vec![SYNC_DOMAIN_SETTINGS.to_string()],
            })
            .unwrap();
        assert_eq!(
            database
                .settings_sync_compaction_target(DEFAULT_PROFILE_ID, &policy, i64::MAX)
                .unwrap(),
            None
        );
    }

    #[test]
    fn settings_sync_compaction_target_can_filter_domains() {
        let database_path =
            test_dir("sync-compaction-target-domains").join(DEFAULT_DATABASE_FILE_NAME);
        let database = SlateProfileDatabase::open_resolved(database_path).unwrap();
        let baseline_revision = database.latest_sync_revision(DEFAULT_PROFILE_ID).unwrap();
        if baseline_revision > 0 {
            database
                .record_sync_snapshot(&SyncSnapshotRegistration {
                    profile: DEFAULT_PROFILE_ID.to_string(),
                    snapshot_id: "snapshot-baseline".to_string(),
                    backend_object_id: Some("snapshot-object-baseline".to_string()),
                    covers_revision: baseline_revision,
                    included_domains: vec![SYNC_DOMAIN_SETTINGS.to_string()],
                })
                .unwrap();
        }

        database
            .set_sync_setting_text(
                DEFAULT_PROFILE_ID,
                SYNC_DOMAIN_SETTINGS,
                "setting.0",
                "value-0",
            )
            .unwrap();
        database
            .set_sync_setting_text(
                DEFAULT_PROFILE_ID,
                SYNC_DOMAIN_CALENDAR,
                "default_view",
                "month",
            )
            .unwrap();
        database
            .set_sync_setting_text(
                DEFAULT_PROFILE_ID,
                SYNC_DOMAIN_SETTINGS,
                "setting.1",
                "value-1",
            )
            .unwrap();

        let policy = ProfileSyncRetentionPolicy {
            min_tail_change_count: 1,
            change_retention_seconds: 0,
            inactive_device_grace_seconds: DEFAULT_PROFILE_SYNC_INACTIVE_DEVICE_GRACE_SECONDS,
        };
        let all_events = database
            .sync_setting_text_events_after(DEFAULT_PROFILE_ID, baseline_revision, 10)
            .unwrap();
        assert_eq!(all_events.len(), 3);
        let settings_events = all_events
            .iter()
            .filter(|event| event.change.domain == SYNC_DOMAIN_SETTINGS)
            .collect::<Vec<_>>();
        assert_eq!(settings_events.len(), 2);

        let target = database
            .settings_sync_compaction_target_for_domains(
                DEFAULT_PROFILE_ID,
                &policy,
                i64::MAX,
                &[SYNC_DOMAIN_SETTINGS.to_string()],
            )
            .unwrap()
            .expect("settings-domain compaction target");
        assert_eq!(target.previous_snapshot_covers_revision, baseline_revision);
        assert_eq!(target.covers_revision, settings_events[0].revision.revision);
        assert_eq!(target.covers_change_id, settings_events[0].change.id);
        assert_eq!(target.covered_change_count, 1);
        assert_eq!(target.retained_tail_change_count, 1);

        assert_eq!(
            database
                .settings_sync_compaction_target_for_domains(
                    DEFAULT_PROFILE_ID,
                    &policy,
                    i64::MAX,
                    &[SYNC_DOMAIN_CHAT.to_string()],
                )
                .unwrap(),
            None
        );
    }

    #[test]
    fn settings_sync_snapshot_payload_materializes_values_at_target_revision() {
        let database_path = test_dir("sync-snapshot-payload").join(DEFAULT_DATABASE_FILE_NAME);
        let database = SlateProfileDatabase::open_resolved(database_path).unwrap();
        let profile = "snapshot-profile";

        database
            .set_sync_setting_text(profile, SYNC_DOMAIN_SETTINGS, "ui.theme", "teal")
            .unwrap();
        database
            .set_sync_setting_text(profile, SYNC_DOMAIN_CALENDAR, "default_view", "month")
            .unwrap();
        let covers_revision = database.latest_sync_revision(profile).unwrap();
        database
            .set_sync_setting_text(profile, SYNC_DOMAIN_SETTINGS, "ui.theme", "slate")
            .unwrap();

        let included_domains = vec![
            SYNC_DOMAIN_SETTINGS.to_string(),
            SYNC_DOMAIN_CALENDAR.to_string(),
            SYNC_DOMAIN_SETTINGS.to_string(),
            String::new(),
        ];
        let snapshot = database
            .settings_sync_snapshot_payload(profile, covers_revision, &included_domains)
            .unwrap();

        assert_eq!(snapshot.profile, profile);
        assert_eq!(
            snapshot.schema_version,
            PROFILE_SYNC_SETTINGS_SNAPSHOT_SCHEMA_VERSION
        );
        assert_eq!(snapshot.covers_revision, covers_revision);
        assert_eq!(
            snapshot.included_domains,
            vec![
                SYNC_DOMAIN_CALENDAR.to_string(),
                SYNC_DOMAIN_SETTINGS.to_string()
            ]
        );
        assert_eq!(
            snapshot.values,
            vec![
                ProfileSyncSettingsSnapshotValue {
                    domain: SYNC_DOMAIN_CALENDAR.to_string(),
                    key: "default_view".to_string(),
                    value: "month".to_string(),
                    value_kind: "text".to_string(),
                    revision: covers_revision,
                },
                ProfileSyncSettingsSnapshotValue {
                    domain: SYNC_DOMAIN_SETTINGS.to_string(),
                    key: "ui.theme".to_string(),
                    value: "teal".to_string(),
                    value_kind: "text".to_string(),
                    revision: covers_revision - 1,
                }
            ]
        );

        let encoded = serde_json::to_vec(&snapshot).unwrap();
        let decoded: ProfileSyncSettingsSnapshot =
            serde_json::from_slice(encoded.as_slice()).unwrap();
        assert_eq!(decoded, snapshot);
    }

    #[test]
    fn applying_settings_snapshot_materializes_values_idempotently() {
        let source_path = test_dir("sync-snapshot-source").join(DEFAULT_DATABASE_FILE_NAME);
        let source =
            SlateProfileDatabase::open_resolved_with_device_id(source_path, "device-a").unwrap();
        source
            .set_sync_setting_text(DEFAULT_PROFILE_ID, SYNC_DOMAIN_SETTINGS, "ui.theme", "teal")
            .unwrap();
        source
            .set_sync_setting_text(
                DEFAULT_PROFILE_ID,
                SYNC_DOMAIN_CALENDAR,
                "default_view",
                "month",
            )
            .unwrap();
        let covers_revision = source.latest_sync_revision(DEFAULT_PROFILE_ID).unwrap();
        let snapshot = source
            .settings_sync_snapshot_payload(
                DEFAULT_PROFILE_ID,
                covers_revision,
                &[
                    SYNC_DOMAIN_SETTINGS.to_string(),
                    SYNC_DOMAIN_CALENDAR.to_string(),
                ],
            )
            .unwrap();

        let destination_path =
            test_dir("sync-snapshot-destination").join(DEFAULT_DATABASE_FILE_NAME);
        let destination =
            SlateProfileDatabase::open_resolved_with_device_id(destination_path, "device-b")
                .unwrap();
        let baseline_revision = destination
            .latest_sync_revision(DEFAULT_PROFILE_ID)
            .unwrap();
        let applied = destination.apply_settings_snapshot(&snapshot).unwrap();

        assert_eq!(applied.len(), snapshot.values.len());
        assert!(
            applied
                .iter()
                .all(|change| change.device_id == PROFILE_SYNC_SNAPSHOT_DEVICE_ID)
        );
        assert!(applied.iter().all(|change| change.applied_at.is_some()));
        assert_eq!(
            destination.get_setting_text("ui.theme").unwrap().as_deref(),
            Some("teal")
        );
        assert_eq!(
            destination
                .get_setting_text("default_view")
                .unwrap()
                .as_deref(),
            None
        );

        let theme_value = destination
            .get_sync_setting_text(DEFAULT_PROFILE_ID, SYNC_DOMAIN_SETTINGS, "ui.theme")
            .unwrap()
            .unwrap();
        assert_eq!(theme_value.value, "teal");
        let calendar_value = destination
            .get_sync_setting_text(DEFAULT_PROFILE_ID, SYNC_DOMAIN_CALENDAR, "default_view")
            .unwrap()
            .unwrap();
        assert_eq!(calendar_value.value, "month");
        assert_eq!(
            destination
                .sync_revisions_after(DEFAULT_PROFILE_ID, baseline_revision)
                .unwrap()
                .len(),
            applied
                .iter()
                .filter(|change| change.applied_at.is_some())
                .count()
        );

        let latest_revision = destination
            .latest_sync_revision(DEFAULT_PROFILE_ID)
            .unwrap();
        let duplicate = destination.apply_settings_snapshot(&snapshot).unwrap();
        assert_eq!(duplicate, applied);
        assert_eq!(
            destination
                .sync_revisions_after(DEFAULT_PROFILE_ID, latest_revision)
                .unwrap(),
            Vec::<SyncRevisionRecord>::new()
        );
        assert!(
            destination
                .sync_devices(DEFAULT_PROFILE_ID)
                .unwrap()
                .iter()
                .any(|device| device.device_id == PROFILE_SYNC_SNAPSHOT_DEVICE_ID)
        );
    }

    #[test]
    fn applying_settings_snapshot_keeps_newer_local_value() {
        let source_path = test_dir("sync-stale-snapshot-source").join(DEFAULT_DATABASE_FILE_NAME);
        let source =
            SlateProfileDatabase::open_resolved_with_device_id(source_path, "device-a").unwrap();
        source
            .set_sync_setting_text(
                DEFAULT_PROFILE_ID,
                SYNC_DOMAIN_SETTINGS,
                "ui.theme",
                "stale",
            )
            .unwrap();
        let snapshot = source
            .settings_sync_snapshot_payload(
                DEFAULT_PROFILE_ID,
                source.latest_sync_revision(DEFAULT_PROFILE_ID).unwrap(),
                &[SYNC_DOMAIN_SETTINGS.to_string()],
            )
            .unwrap();
        let snapshot_theme_value = snapshot
            .values
            .iter()
            .find(|value| value.domain == SYNC_DOMAIN_SETTINGS && value.key == "ui.theme")
            .expect("stale snapshot contains theme value");
        let snapshot_value_revision = snapshot_theme_value.revision;

        let destination_path =
            test_dir("sync-stale-snapshot-destination").join(DEFAULT_DATABASE_FILE_NAME);
        let destination =
            SlateProfileDatabase::open_resolved_with_device_id(destination_path, "device-b")
                .unwrap();
        for index in 0..=snapshot_value_revision {
            destination
                .set_sync_setting_text(
                    DEFAULT_PROFILE_ID,
                    SYNC_DOMAIN_SETTINGS,
                    "ui.theme",
                    &format!("local-{index}"),
                )
                .unwrap();
        }
        let winning_value = destination
            .get_sync_setting_text(DEFAULT_PROFILE_ID, SYNC_DOMAIN_SETTINGS, "ui.theme")
            .unwrap()
            .unwrap();

        let replayed = destination.apply_settings_snapshot(&snapshot).unwrap();
        let replayed_theme = replayed
            .iter()
            .find(|change| change.domain == SYNC_DOMAIN_SETTINGS && change.entity_key == "ui.theme")
            .expect("replayed snapshot includes theme value");
        assert_eq!(replayed_theme.payload, "stale");
        assert_eq!(replayed_theme.applied_at, None);
        let expected_local_value = format!("local-{snapshot_value_revision}");
        assert_eq!(
            destination.get_setting_text("ui.theme").unwrap().as_deref(),
            Some(expected_local_value.as_str())
        );
        assert_eq!(
            destination
                .get_sync_setting_text(DEFAULT_PROFILE_ID, SYNC_DOMAIN_SETTINGS, "ui.theme")
                .unwrap()
                .unwrap(),
            winning_value
        );
        assert!(
            !destination
                .sync_setting_text_events_after(DEFAULT_PROFILE_ID, winning_value.revision, 10)
                .unwrap()
                .iter()
                .any(|event| {
                    event.change.domain == SYNC_DOMAIN_SETTINGS
                        && event.change.entity_key == "ui.theme"
                })
        );
    }

    #[test]
    fn verified_settings_manifest_applies_snapshot_then_tail_and_tracks_root() {
        let source_path = test_dir("sync-manifest-source").join(DEFAULT_DATABASE_FILE_NAME);
        let source =
            SlateProfileDatabase::open_resolved_with_device_id(source_path, "device-a").unwrap();
        source
            .set_sync_setting_text(DEFAULT_PROFILE_ID, SYNC_DOMAIN_SETTINGS, "ui.theme", "teal")
            .unwrap();
        let snapshot_revision = source.latest_sync_revision(DEFAULT_PROFILE_ID).unwrap();
        let snapshot = source
            .settings_sync_snapshot_payload(
                DEFAULT_PROFILE_ID,
                snapshot_revision,
                &[SYNC_DOMAIN_SETTINGS.to_string()],
            )
            .unwrap();
        let tail_change = source
            .set_sync_setting_text(
                DEFAULT_PROFILE_ID,
                SYNC_DOMAIN_SETTINGS,
                "ui.theme",
                "slate",
            )
            .unwrap();
        let manifest = ProfileSyncManifest {
            profile: DEFAULT_PROFILE_ID.to_string(),
            root_id: "settings/latest".to_string(),
            schema_version: PROFILE_SYNC_MANIFEST_SCHEMA_VERSION,
            membership_epoch: DEFAULT_PROFILE_SYNC_MEMBERSHIP_EPOCH,
            current_snapshot_object_id: Some("snapshot-object-1".to_string()),
            tail_change_object_ids: vec!["tail-object-1".to_string()],
            included_domains: vec![SYNC_DOMAIN_SETTINGS.to_string()],
            device_frontiers: vec![ProfileSyncDeviceFrontier {
                device_id: tail_change.device_id.clone(),
                latest_sequence: tail_change.device_sequence,
                latest_change_object_id: Some("tail-object-1".to_string()),
            }],
            retention_policy: ProfileSyncRetentionPolicy::default(),
            created_at: tail_change.created_at,
        };

        let destination_path =
            test_dir("sync-manifest-destination").join(DEFAULT_DATABASE_FILE_NAME);
        let destination =
            SlateProfileDatabase::open_resolved_with_device_id(destination_path, "device-b")
                .unwrap();
        let verified_objects = VerifiedProfileSyncSettingsManifestObjects {
            manifest_object_id: "manifest-object-1".to_string(),
            manifest: manifest.clone(),
            snapshot: Some(VerifiedProfileSyncSettingsSnapshot {
                object_id: "snapshot-object-1".to_string(),
                snapshot,
            }),
            tail_changes: vec![VerifiedProfileSyncSettingsTailChange {
                object_id: "tail-object-1".to_string(),
                change: IncomingSyncSettingText::new(
                    tail_change.profile,
                    tail_change.domain,
                    tail_change.entity_key,
                    tail_change.payload,
                    tail_change.device_id,
                    tail_change.device_sequence,
                    tail_change.logical_clock,
                ),
            }],
        };
        let applied = destination
            .apply_verified_settings_manifest_objects(&verified_objects)
            .unwrap();

        assert_eq!(applied.profile, DEFAULT_PROFILE_ID);
        assert_eq!(applied.root_id, "settings/latest");
        assert_eq!(applied.manifest_object_id, "manifest-object-1");
        assert_eq!(
            applied.sync_object_ids,
            vec![
                "manifest-object-1".to_string(),
                "snapshot-object-1".to_string(),
                "tail-object-1".to_string(),
            ]
        );
        assert_eq!(
            applied
                .snapshot
                .as_ref()
                .and_then(|snapshot| snapshot.backend_object_id.as_deref()),
            Some("snapshot-object-1")
        );
        assert!(
            applied
                .snapshot_changes
                .iter()
                .any(|change| change.entity_key == "ui.theme" && change.payload == "teal")
        );
        assert_eq!(applied.tail_changes.len(), 1);
        assert_eq!(applied.tail_changes[0].payload, "slate");
        assert!(applied.tail_changes[0].applied_at.is_some());
        assert_eq!(
            destination.get_setting_text("ui.theme").unwrap().as_deref(),
            Some("slate")
        );
        assert_eq!(
            destination
                .profile_sync_root(DEFAULT_PROFILE_ID, "settings/latest")
                .unwrap()
                .expect("settings root")
                .object_id,
            "manifest-object-1"
        );
    }

    #[test]
    fn verified_settings_manifest_rejects_mismatched_tail_objects_before_applying() {
        let database_path = test_dir("sync-manifest-invalid").join(DEFAULT_DATABASE_FILE_NAME);
        let database =
            SlateProfileDatabase::open_resolved_with_device_id(database_path, "device-b").unwrap();
        let manifest = ProfileSyncManifest {
            profile: DEFAULT_PROFILE_ID.to_string(),
            root_id: "settings/latest".to_string(),
            schema_version: PROFILE_SYNC_MANIFEST_SCHEMA_VERSION,
            membership_epoch: DEFAULT_PROFILE_SYNC_MEMBERSHIP_EPOCH,
            current_snapshot_object_id: None,
            tail_change_object_ids: vec!["expected-tail-object".to_string()],
            included_domains: vec![SYNC_DOMAIN_SETTINGS.to_string()],
            device_frontiers: Vec::new(),
            retention_policy: ProfileSyncRetentionPolicy::default(),
            created_at: 1,
        };

        let error = database
            .apply_verified_settings_manifest(
                "manifest-object-1",
                &manifest,
                None,
                &[VerifiedProfileSyncSettingsTailChange {
                    object_id: "wrong-tail-object".to_string(),
                    change: IncomingSyncSettingText::new(
                        DEFAULT_PROFILE_ID,
                        SYNC_DOMAIN_SETTINGS,
                        "ui.theme",
                        "slate",
                        "device-a",
                        1,
                        1,
                    ),
                }],
            )
            .unwrap_err();

        assert!(matches!(error, StorageError::InvalidProfileSyncManifest(_)));
        assert_eq!(
            database.get_setting_text("ui.theme").unwrap().as_deref(),
            None
        );
        assert_eq!(
            database
                .profile_sync_root(DEFAULT_PROFILE_ID, "settings/latest")
                .unwrap(),
            None
        );
    }

    #[test]
    fn verified_settings_manifest_apply_is_atomic_after_snapshot_writes() {
        let database_path = test_dir("sync-manifest-atomic-tail").join(DEFAULT_DATABASE_FILE_NAME);
        let database =
            SlateProfileDatabase::open_resolved_with_device_id(database_path, "device-b").unwrap();
        let snapshot = ProfileSyncSettingsSnapshot {
            profile: DEFAULT_PROFILE_ID.to_string(),
            schema_version: PROFILE_SYNC_SETTINGS_SNAPSHOT_SCHEMA_VERSION,
            covers_revision: 1,
            included_domains: vec![SYNC_DOMAIN_SETTINGS.to_string()],
            values: vec![ProfileSyncSettingsSnapshotValue {
                domain: SYNC_DOMAIN_SETTINGS.to_string(),
                key: "ui.theme".to_string(),
                value: "teal".to_string(),
                value_kind: "text".to_string(),
                revision: 1,
            }],
            created_at: 10,
        };
        let manifest = ProfileSyncManifest {
            profile: DEFAULT_PROFILE_ID.to_string(),
            root_id: "settings/latest".to_string(),
            schema_version: PROFILE_SYNC_MANIFEST_SCHEMA_VERSION,
            membership_epoch: DEFAULT_PROFILE_SYNC_MEMBERSHIP_EPOCH,
            current_snapshot_object_id: Some("snapshot-object-atomic".to_string()),
            tail_change_object_ids: vec!["tail-object-invalid-bookmark".to_string()],
            included_domains: vec![
                SYNC_DOMAIN_SETTINGS.to_string(),
                SYNC_DOMAIN_BOOKMARKS.to_string(),
            ],
            device_frontiers: Vec::new(),
            retention_policy: ProfileSyncRetentionPolicy::default(),
            created_at: 20,
        };

        let error = database
            .apply_verified_settings_manifest(
                "manifest-object-atomic",
                &manifest,
                Some(&VerifiedProfileSyncSettingsSnapshot {
                    object_id: "snapshot-object-atomic".to_string(),
                    snapshot,
                }),
                &[VerifiedProfileSyncSettingsTailChange {
                    object_id: "tail-object-invalid-bookmark".to_string(),
                    change: IncomingSyncSettingText::new(
                        DEFAULT_PROFILE_ID,
                        SYNC_DOMAIN_BOOKMARKS,
                        "home.slot.99",
                        "{not valid json",
                        "manifest-tail-device",
                        1,
                        2,
                    ),
                }],
            )
            .unwrap_err();

        assert!(matches!(error, StorageError::Database { .. }));
        assert_eq!(database.get_setting_text("ui.theme").unwrap(), None);
        assert!(
            database
                .get_sync_setting_text(DEFAULT_PROFILE_ID, SYNC_DOMAIN_SETTINGS, "ui.theme")
                .unwrap()
                .is_none()
        );
        assert!(
            database
                .get_sync_setting_text(DEFAULT_PROFILE_ID, SYNC_DOMAIN_BOOKMARKS, "home.slot.99")
                .unwrap()
                .is_none()
        );
        assert!(
            database
                .sync_snapshot(DEFAULT_PROFILE_ID, "settings-snapshot-r1")
                .unwrap()
                .is_none()
        );
        assert!(
            database
                .profile_sync_root(DEFAULT_PROFILE_ID, "settings/latest")
                .unwrap()
                .is_none()
        );
        assert!(
            !database
                .sync_devices(DEFAULT_PROFILE_ID)
                .unwrap()
                .iter()
                .any(|device| device.device_id == "manifest-tail-device")
        );
    }

    #[test]
    fn profile_sync_roots_track_latest_manifest_objects() {
        let database_path = test_dir("sync-roots").join(DEFAULT_DATABASE_FILE_NAME);
        let database = SlateProfileDatabase::open_resolved(database_path).unwrap();

        assert_eq!(
            database
                .profile_sync_root(DEFAULT_PROFILE_ID, "settings/latest")
                .unwrap(),
            None
        );

        let root = database
            .set_profile_sync_root(DEFAULT_PROFILE_ID, "settings/latest", "manifest-object-1")
            .unwrap();
        assert_eq!(root.profile, DEFAULT_PROFILE_ID);
        assert_eq!(root.root_id, "settings/latest");
        assert_eq!(root.object_id, "manifest-object-1");

        let loaded = database
            .profile_sync_root(DEFAULT_PROFILE_ID, "settings/latest")
            .unwrap()
            .expect("settings root");
        assert_eq!(loaded, root);

        let updated = database
            .set_profile_sync_root(DEFAULT_PROFILE_ID, "settings/latest", "manifest-object-2")
            .unwrap();
        assert_eq!(updated.object_id, "manifest-object-2");

        database
            .set_profile_sync_root(DEFAULT_PROFILE_ID, "bookmarks/latest", "bookmark-manifest")
            .unwrap();
        database
            .set_profile_sync_root("testing", "settings/latest", "testing-manifest")
            .unwrap();

        let roots = database.profile_sync_roots(DEFAULT_PROFILE_ID).unwrap();
        assert_eq!(roots.len(), 2);
        assert_eq!(roots[0].root_id, "bookmarks/latest");
        assert_eq!(roots[1].root_id, "settings/latest");
        assert_eq!(roots[1].object_id, "manifest-object-2");

        let testing_roots = database.profile_sync_roots("testing").unwrap();
        assert_eq!(testing_roots.len(), 1);
        assert_eq!(testing_roots[0].object_id, "testing-manifest");

        let error = database
            .set_profile_sync_root(DEFAULT_PROFILE_ID, "", "manifest")
            .unwrap_err();
        assert!(matches!(error, StorageError::InvalidSyncRootId(root_id) if root_id.is_empty()));
        let error = database
            .profile_sync_root(DEFAULT_PROFILE_ID, "")
            .unwrap_err();
        assert!(matches!(error, StorageError::InvalidSyncRootId(root_id) if root_id.is_empty()));
    }

    #[test]
    fn profile_sync_roots_batch_rolls_back_on_invalid_root() {
        let database_path = test_dir("sync-root-batch").join(DEFAULT_DATABASE_FILE_NAME);
        let database = SlateProfileDatabase::open_resolved(database_path).unwrap();
        let roots = [
            ProfileSyncRootRegistration {
                profile: DEFAULT_PROFILE_ID.to_string(),
                root_id: "settings/latest".to_string(),
                object_id: "manifest-object-1".to_string(),
            },
            ProfileSyncRootRegistration {
                profile: DEFAULT_PROFILE_ID.to_string(),
                root_id: String::new(),
                object_id: "device-head-object-1".to_string(),
            },
        ];

        let error = database.set_profile_sync_roots(&roots).unwrap_err();

        assert!(matches!(error, StorageError::InvalidSyncRootId(root_id) if root_id.is_empty()));
        assert!(
            database
                .profile_sync_root(DEFAULT_PROFILE_ID, "settings/latest")
                .unwrap()
                .is_none()
        );

        let valid_roots = [
            ProfileSyncRootRegistration {
                profile: DEFAULT_PROFILE_ID.to_string(),
                root_id: "settings/latest".to_string(),
                object_id: "manifest-object-1".to_string(),
            },
            ProfileSyncRootRegistration {
                profile: DEFAULT_PROFILE_ID.to_string(),
                root_id: "settings/devices/device-a/head".to_string(),
                object_id: "device-head-object-1".to_string(),
            },
        ];
        let records = database.set_profile_sync_roots(&valid_roots).unwrap();

        assert_eq!(records.len(), 2);
        assert_eq!(records[0].root_id, "settings/latest");
        assert_eq!(records[1].root_id, "settings/devices/device-a/head");
        assert_eq!(records[0].object_id, "manifest-object-1");
        assert_eq!(records[1].object_id, "device-head-object-1");
    }

    #[test]
    fn sync_snapshot_and_roots_record_atomically() {
        let database_path = test_dir("sync-snapshot-root-batch").join(DEFAULT_DATABASE_FILE_NAME);
        let database = SlateProfileDatabase::open_resolved(database_path).unwrap();
        let snapshot = SyncSnapshotRegistration {
            profile: DEFAULT_PROFILE_ID.to_string(),
            snapshot_id: "settings-snapshot-r1".to_string(),
            backend_object_id: Some("snapshot-object-1".to_string()),
            covers_revision: 1,
            included_domains: vec![SYNC_DOMAIN_SETTINGS.to_string()],
        };
        let roots = [
            ProfileSyncRootRegistration {
                profile: DEFAULT_PROFILE_ID.to_string(),
                root_id: "settings/latest".to_string(),
                object_id: "manifest-object-1".to_string(),
            },
            ProfileSyncRootRegistration {
                profile: DEFAULT_PROFILE_ID.to_string(),
                root_id: String::new(),
                object_id: "device-head-object-1".to_string(),
            },
        ];

        let error = database
            .record_sync_snapshot_and_set_profile_sync_roots(&snapshot, &roots)
            .unwrap_err();

        assert!(matches!(error, StorageError::InvalidSyncRootId(root_id) if root_id.is_empty()));
        assert!(
            database
                .sync_snapshot(DEFAULT_PROFILE_ID, "settings-snapshot-r1")
                .unwrap()
                .is_none()
        );
        assert!(
            database
                .profile_sync_root(DEFAULT_PROFILE_ID, "settings/latest")
                .unwrap()
                .is_none()
        );

        let valid_roots = [
            ProfileSyncRootRegistration {
                profile: DEFAULT_PROFILE_ID.to_string(),
                root_id: "settings/latest".to_string(),
                object_id: "manifest-object-1".to_string(),
            },
            ProfileSyncRootRegistration {
                profile: DEFAULT_PROFILE_ID.to_string(),
                root_id: "settings/devices/device-a/head".to_string(),
                object_id: "device-head-object-1".to_string(),
            },
        ];
        let (record, root_records) = database
            .record_sync_snapshot_and_set_profile_sync_roots(&snapshot, &valid_roots)
            .unwrap();

        assert_eq!(record.snapshot_id, "settings-snapshot-r1");
        assert_eq!(
            record.backend_object_id.as_deref(),
            Some("snapshot-object-1")
        );
        assert_eq!(root_records.len(), 2);
        assert_eq!(
            database
                .profile_sync_root(DEFAULT_PROFILE_ID, "settings/latest")
                .unwrap()
                .expect("settings root")
                .object_id,
            "manifest-object-1"
        );
        assert_eq!(
            database
                .profile_sync_root(DEFAULT_PROFILE_ID, "settings/devices/device-a/head")
                .unwrap()
                .expect("device head root")
                .object_id,
            "device-head-object-1"
        );
    }

    #[test]
    fn malformed_setting_value_falls_back_without_resetting_other_settings() {
        let database_path = test_dir("malformed-setting").join(DEFAULT_DATABASE_FILE_NAME);
        let database = SlateProfileDatabase::open_resolved(database_path).unwrap();

        database
            .set_setting_text("chrome.zoom", "not-a-number")
            .unwrap();
        database.set_setting_text("ui.theme", "kept").unwrap();

        assert_eq!(database.get_setting_f32_or_default("chrome.zoom", 0.9), 0.9);
        assert_eq!(
            database.ensure_setting_f32_or_default("chrome.zoom", 0.9),
            0.9
        );
        assert_eq!(
            database.get_setting_text("ui.theme").unwrap().as_deref(),
            Some("kept")
        );
        assert_eq!(
            database.get_setting_text("chrome.zoom").unwrap().as_deref(),
            Some("not-a-number")
        );
    }

    #[test]
    fn bookmark_slot_replacement_keeps_home_slots_bounded() {
        let database_path = test_dir("bookmark-slot").join(DEFAULT_DATABASE_FILE_NAME);
        let database = SlateProfileDatabase::open_resolved(database_path).unwrap();

        database
            .set_bookmark_slot(
                &BookmarkUpdate {
                    profile: DEFAULT_PROFILE_ID.into(),
                    url: "https://example.com/".into(),
                    title: Some("Example".into()),
                    folder: None,
                    position: 0,
                    favicon_key: None,
                },
                Some(DEFAULT_HOME_BOOKMARKS[0].url),
            )
            .unwrap();

        let bookmarks = database.bookmarks(DEFAULT_PROFILE_ID).unwrap();
        assert_eq!(bookmarks.len(), DEFAULT_HOME_BOOKMARKS.len());
        assert_eq!(bookmarks[0].url, "https://example.com/");
        assert_eq!(bookmarks[0].position, 0);
        assert_eq!(bookmarks[1].url, DEFAULT_HOME_BOOKMARKS[1].url);
        let first_events = database
            .sync_setting_text_events_after_for_domain(
                DEFAULT_PROFILE_ID,
                SYNC_DOMAIN_BOOKMARKS,
                0,
                10,
            )
            .unwrap();
        assert_eq!(first_events.len(), 1);
        assert_eq!(
            first_events[0].change.entity_key,
            bookmark_home_slot_sync_key(0)
        );
        let first_payload: BookmarkSlotSyncPayload =
            serde_json::from_str(first_events[0].change.payload.as_str()).unwrap();
        assert_eq!(first_payload.url, "https://example.com/");
        assert_eq!(first_payload.title.as_deref(), Some("Example"));
        assert_eq!(first_payload.position, 0);
        assert!(!first_payload.deleted);
        assert_eq!(
            first_payload.replaced_url,
            Some(DEFAULT_HOME_BOOKMARKS[0].url.to_string())
        );

        database
            .set_bookmark_slot(
                &BookmarkUpdate {
                    profile: DEFAULT_PROFILE_ID.into(),
                    url: "https://example.com/".into(),
                    title: Some("Example moved".into()),
                    folder: None,
                    position: 1,
                    favicon_key: None,
                },
                Some(DEFAULT_HOME_BOOKMARKS[1].url),
            )
            .unwrap();

        let bookmarks = database.bookmarks(DEFAULT_PROFILE_ID).unwrap();
        assert_eq!(bookmarks.len(), 1);
        assert_eq!(bookmarks[0].url, "https://example.com/");
        assert_eq!(bookmarks[0].title.as_deref(), Some("Example moved"));
        assert_eq!(bookmarks[0].position, 1);
        let all_events = database
            .sync_setting_text_events_after_for_domain(
                DEFAULT_PROFILE_ID,
                SYNC_DOMAIN_BOOKMARKS,
                0,
                10,
            )
            .unwrap();
        assert_eq!(all_events.len(), 2);
        assert_eq!(
            all_events[1].change.entity_key,
            bookmark_home_slot_sync_key(1)
        );
        let second_payload: BookmarkSlotSyncPayload =
            serde_json::from_str(all_events[1].change.payload.as_str()).unwrap();
        assert_eq!(second_payload.url, "https://example.com/");
        assert_eq!(second_payload.title.as_deref(), Some("Example moved"));
        assert_eq!(second_payload.position, 1);
        assert!(!second_payload.deleted);
        assert_eq!(
            second_payload.replaced_url,
            Some(DEFAULT_HOME_BOOKMARKS[1].url.to_string())
        );
        assert_eq!(
            database
                .get_sync_setting_text(
                    DEFAULT_PROFILE_ID,
                    SYNC_DOMAIN_BOOKMARKS,
                    bookmark_home_slot_sync_key(1).as_str()
                )
                .unwrap()
                .map(|record| record.value),
            Some(all_events[1].change.payload.clone())
        );
    }

    #[test]
    fn bookmark_removal_records_home_slot_tombstone() {
        let database_path = test_dir("bookmark-slot-remove").join(DEFAULT_DATABASE_FILE_NAME);
        let database = SlateProfileDatabase::open_resolved(database_path).unwrap();
        database
            .set_bookmark_slot(
                &BookmarkUpdate {
                    profile: DEFAULT_PROFILE_ID.into(),
                    url: "https://example.com/".into(),
                    title: Some("Example".into()),
                    folder: None,
                    position: 0,
                    favicon_key: Some("favicon:https://example.com/".into()),
                },
                Some(DEFAULT_HOME_BOOKMARKS[0].url),
            )
            .unwrap();

        database
            .remove_bookmark(DEFAULT_PROFILE_ID, "https://example.com/")
            .unwrap();
        database
            .remove_bookmark(DEFAULT_PROFILE_ID, "https://missing.example/")
            .unwrap();

        let bookmarks = database.bookmarks(DEFAULT_PROFILE_ID).unwrap();
        assert!(
            !bookmarks
                .iter()
                .any(|bookmark| bookmark.url == "https://example.com/")
        );
        let events = database
            .sync_setting_text_events_after_for_domain(
                DEFAULT_PROFILE_ID,
                SYNC_DOMAIN_BOOKMARKS,
                0,
                10,
            )
            .unwrap();
        assert_eq!(events.len(), 2);
        assert_eq!(events[1].change.entity_key, bookmark_home_slot_sync_key(0));
        let tombstone: BookmarkSlotSyncPayload =
            serde_json::from_str(events[1].change.payload.as_str()).unwrap();
        assert!(tombstone.deleted);
        assert_eq!(tombstone.url, "https://example.com/");
        assert_eq!(tombstone.title.as_deref(), Some("Example"));
        assert_eq!(tombstone.position, 0);
        assert_eq!(
            tombstone.favicon_key.as_deref(),
            Some("favicon:https://example.com/")
        );
    }

    #[test]
    fn unreadable_seed_marker_does_not_block_profile_open() {
        let database_path = test_dir("unreadable-seed-marker").join(DEFAULT_DATABASE_FILE_NAME);
        let connection = Connection::open(&database_path).unwrap();
        connection
            .execute_batch("CREATE TABLE settings (bad INTEGER);")
            .unwrap();
        drop(connection);

        let database = SlateProfileDatabase::open_resolved(database_path).unwrap();

        assert_eq!(
            database.ensure_setting_f32_or_default("chrome.zoom", 0.9),
            0.9
        );
        assert_eq!(
            database.bookmarks(DEFAULT_PROFILE_ID).unwrap().len(),
            DEFAULT_HOME_BOOKMARKS.len()
        );
    }

    #[test]
    fn existing_empty_database_receives_default_bookmarks_once() {
        let database_path = test_dir("existing-empty").join(DEFAULT_DATABASE_FILE_NAME);
        std::fs::write(&database_path, b"").unwrap();

        let database = SlateProfileDatabase::open_resolved(database_path.clone()).unwrap();
        assert_eq!(
            database.bookmarks(DEFAULT_PROFILE_ID).unwrap().len(),
            DEFAULT_HOME_BOOKMARKS.len()
        );

        for bookmark in DEFAULT_HOME_BOOKMARKS {
            database
                .remove_bookmark(DEFAULT_PROFILE_ID, bookmark.url)
                .unwrap();
        }
        assert!(database.bookmarks(DEFAULT_PROFILE_ID).unwrap().is_empty());

        let reopened = SlateProfileDatabase::open_resolved(database_path).unwrap();
        assert!(reopened.bookmarks(DEFAULT_PROFILE_ID).unwrap().is_empty());
    }
}
