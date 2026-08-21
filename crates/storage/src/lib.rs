#![forbid(unsafe_code)]

use ring::{aead, rand, signature};
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use std::fmt;
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
pub const PROFILE_SYNC_CONTENT_KEY_BYTES: usize = 32;
pub const PROFILE_SYNC_NONCE_BYTES: usize = 12;
pub const SYNC_OBJECT_VERSION: u8 = 1;
pub const PROFILE_SYNC_MANIFEST_SCHEMA_VERSION: u8 = 1;
pub const PROFILE_SYNC_DEVICE_HEAD_SCHEMA_VERSION: u8 = 1;
pub const PROFILE_SYNC_SETTINGS_SNAPSHOT_SCHEMA_VERSION: u8 = 1;
pub const PROFILE_SYNC_SETTING_CHANGE_OBJECT_KIND: &str = "setting-change";
pub const PROFILE_SYNC_SETTINGS_SNAPSHOT_OBJECT_KIND: &str = "settings-snapshot";
pub const PROFILE_SYNC_MANIFEST_OBJECT_KIND: &str = "manifest";
pub const PROFILE_SYNC_DEVICE_HEAD_OBJECT_KIND: &str = "device-head";
pub const DEFAULT_PROFILE_SYNC_MEMBERSHIP_EPOCH: i64 = 1;
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
const PROFILE_SYNC_ROOT_KEY_PREFIX: &str = "profile_sync.root.";
const PROFILE_SYNC_SNAPSHOT_DEVICE_ID: &str = "snapshot";

const DEFAULT_APP_SYNC_DOMAINS: [DefaultAppSyncDomain; 8] = [
    DefaultAppSyncDomain {
        domain: SYNC_DOMAIN_SETTINGS,
        schema_version: 1,
        privacy_classification: "low-risk",
        sync_content: false,
    },
    DefaultAppSyncDomain {
        domain: SYNC_DOMAIN_BOOKMARKS,
        schema_version: 1,
        privacy_classification: "low-risk",
        sync_content: false,
    },
    DefaultAppSyncDomain {
        domain: SYNC_DOMAIN_CALENDAR,
        schema_version: 1,
        privacy_classification: "sensitive",
        sync_content: false,
    },
    DefaultAppSyncDomain {
        domain: SYNC_DOMAIN_CONTACTS,
        schema_version: 1,
        privacy_classification: "sensitive",
        sync_content: false,
    },
    DefaultAppSyncDomain {
        domain: SYNC_DOMAIN_CHAT,
        schema_version: 1,
        privacy_classification: "sensitive",
        sync_content: false,
    },
    DefaultAppSyncDomain {
        domain: SYNC_DOMAIN_FILES,
        schema_version: 1,
        privacy_classification: "content",
        sync_content: true,
    },
    DefaultAppSyncDomain {
        domain: SYNC_DOMAIN_DOWNLOADS,
        schema_version: 1,
        privacy_classification: "metadata",
        sync_content: false,
    },
    DefaultAppSyncDomain {
        domain: SYNC_DOMAIN_STORAGE,
        schema_version: 1,
        privacy_classification: "sensitive",
        sync_content: false,
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
    pkcs8: Vec<u8>,
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
            pkcs8: pkcs8.as_ref().to_vec(),
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
            pkcs8: pkcs8.into(),
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
        signature::Ed25519KeyPair::from_pkcs8(self.pkcs8.as_slice())
            .map_err(|_| SyncObjectError::Key)
    }
}

impl fmt::Debug for ProfileSyncDeviceSigner {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("ProfileSyncDeviceSigner")
            .field("device_id", &self.device_id)
            .field("pkcs8", &"<redacted>")
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

fn default_profile_sync_membership_epoch() -> i64 {
    DEFAULT_PROFILE_SYNC_MEMBERSHIP_EPOCH
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
            Self::UntrustedDevice { .. } | Self::UnauthorizedDeviceEpoch { .. } => None,
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
    pub created_at: i64,
    pub updated_at: i64,
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
pub struct VerifiedProfileSyncDeviceHead {
    pub object_id: String,
    pub device_head: ProfileSyncDeviceHead,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ProfileSyncSettingsManifestApplication {
    pub profile: String,
    pub root_id: String,
    pub manifest_object_id: String,
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
    Database {
        path: PathBuf,
        source: rusqlite::Error,
    },
    EncodeSnapshotDomains(serde_json::Error),
    Clock(std::time::SystemTimeError),
    InvalidSyncDeviceId(String),
    InvalidSyncContentKeyId(String),
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
            Self::Database { path, source } => {
                write!(
                    formatter,
                    "database operation failed for {}: {source}",
                    path.display()
                )
            }
            Self::EncodeSnapshotDomains(error) => {
                write!(formatter, "failed to encode sync snapshot domains: {error}")
            }
            Self::Clock(error) => write!(formatter, "failed to read system clock: {error}"),
            Self::InvalidSyncDeviceId(device_id) => {
                write!(formatter, "invalid sync device id: {device_id}")
            }
            Self::InvalidSyncContentKeyId(key_id) => {
                write!(formatter, "invalid sync content key id: {key_id}")
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
            Self::Database { source, .. } => Some(source),
            Self::EncodeSnapshotDomains(error) => Some(error),
            Self::Clock(error) => Some(error),
            Self::InvalidSyncDeviceId(_) => None,
            Self::InvalidSyncContentKeyId(_) => None,
            Self::MissingActiveSyncContentKey(_) => None,
            Self::UnsupportedSyncContentKeyAlgorithm { .. }
            | Self::UnauthorizedSyncContentKeyEpoch { .. } => None,
            Self::InvalidSyncRootId(_) => None,
            Self::InvalidProfileSyncManifest(_) => None,
            Self::UnsupportedProfileSyncManifestSchema(_) => None,
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
        Self::open_resolved(resolved.path)
    }

    pub fn open_resolved(path: PathBuf) -> Result<Self, StorageError> {
        Self::open_resolved_with_device_id(path, DEFAULT_SYNC_DEVICE_ID)
    }

    pub fn open_resolved_with_device_id(
        path: PathBuf,
        local_sync_device_id: impl Into<String>,
    ) -> Result<Self, StorageError> {
        let local_sync_device_id = local_sync_device_id.into();
        if !is_valid_sync_identifier(local_sync_device_id.as_str()) {
            return Err(StorageError::InvalidSyncDeviceId(local_sync_device_id));
        }

        if let Some(parent) = path.parent()
            && !parent.as_os_str().is_empty()
        {
            std::fs::create_dir_all(parent).map_err(|source| StorageError::CreateDirectory {
                path: parent.to_path_buf(),
                source,
            })?;
        }

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
        let connection = self.connection()?;
        connection
            .execute(
                "DELETE FROM bookmarks WHERE profile = ?1 AND url = ?2",
                params![profile, url],
            )
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
        if !is_valid_sync_identifier(registration.public_key.device_id.as_str()) {
            return Err(StorageError::InvalidSyncDeviceId(
                registration.public_key.device_id.clone(),
            ));
        }

        let connection = self.connection()?;
        let now = unix_time_seconds()?;
        connection
            .execute(
                "INSERT INTO sync_device_public_keys
                   (profile, device_id, public_key, membership_epoch, created_at, updated_at)
                 VALUES (?1, ?2, ?3, ?4, ?5, ?5)
                 ON CONFLICT(profile, device_id) DO UPDATE SET
                   public_key = excluded.public_key,
                   membership_epoch = excluded.membership_epoch,
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

        self.sync_device_public_key(
            registration.profile.as_str(),
            registration.public_key.device_id.as_str(),
        )?
        .ok_or_else(|| self.database_error(rusqlite::Error::QueryReturnedNoRows))
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
                "SELECT profile, device_id, public_key, membership_epoch, created_at, updated_at
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
                "SELECT profile, device_id, public_key, membership_epoch, created_at, updated_at
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

    pub fn record_sync_snapshot(
        &self,
        snapshot: &SyncSnapshotRegistration,
    ) -> Result<SyncSnapshotRecord, StorageError> {
        let connection = self.connection()?;
        let now = unix_time_seconds()?;
        let normalized_domains = normalized_snapshot_domains(snapshot.included_domains.as_slice());
        let included_domains = encode_snapshot_domains(normalized_domains.as_slice())?;
        connection
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

        self.sync_snapshot(snapshot.profile.as_str(), snapshot.snapshot_id.as_str())?
            .ok_or_else(|| self.database_error(rusqlite::Error::QueryReturnedNoRows))
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

        let mut connection = self.connection()?;
        let transaction = connection
            .transaction()
            .map_err(|source| self.database_error(source))?;
        let now = unix_time_seconds()?;
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
            let applied = apply_sync_setting_text_in_transaction(&transaction, &change, now)
                .map_err(|source| self.database_error(source))?;
            changes.push(applied);
        }
        transaction
            .commit()
            .map_err(|source| self.database_error(source))?;
        Ok(changes)
    }

    pub fn apply_verified_settings_manifest(
        &self,
        manifest_object_id: &str,
        manifest: &ProfileSyncManifest,
        snapshot: Option<&VerifiedProfileSyncSettingsSnapshot>,
        tail_changes: &[VerifiedProfileSyncSettingsTailChange],
    ) -> Result<ProfileSyncSettingsManifestApplication, StorageError> {
        validate_settings_manifest_application(manifest, snapshot, tail_changes)?;

        let mut snapshot_record = None;
        let mut snapshot_changes = Vec::new();
        if let Some(snapshot) = snapshot {
            snapshot_changes = self.apply_settings_snapshot(&snapshot.snapshot)?;
            snapshot_record = Some(self.record_sync_snapshot(&SyncSnapshotRegistration {
                profile: manifest.profile.clone(),
                snapshot_id: settings_snapshot_id(snapshot.snapshot.covers_revision),
                backend_object_id: Some(snapshot.object_id.clone()),
                covers_revision: snapshot.snapshot.covers_revision,
                included_domains: snapshot.snapshot.included_domains.clone(),
            })?);
        }

        let mut applied_tail_changes = Vec::new();
        for tail_change in tail_changes {
            applied_tail_changes.push(self.apply_sync_setting_text(&tail_change.change)?);
        }

        self.set_profile_sync_root(
            manifest.profile.as_str(),
            manifest.root_id.as_str(),
            manifest_object_id,
        )?;

        Ok(ProfileSyncSettingsManifestApplication {
            profile: manifest.profile.clone(),
            root_id: manifest.root_id.clone(),
            manifest_object_id: manifest_object_id.to_string(),
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
        let public_key =
            self.trusted_public_key_for_signed_object(expected_profile, &signed_object)?;
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
        let public_key = self.trusted_public_key_for_signed_object(profile, &signed_object)?;
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
        let public_key = self.trusted_public_key_for_signed_object(profile, &signed_object)?;
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

    fn trusted_public_key_for_signed_object(
        &self,
        profile: &str,
        signed_object: &SignedSyncObject,
    ) -> Result<ProfileSyncDevicePublicKey, ProfileSyncTrustedOpenError> {
        Ok(self
            .trusted_public_key_record_for_signed_object(profile, signed_object)?
            .public_key)
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
        if root_id.is_empty() {
            return Err(StorageError::InvalidSyncRootId(root_id.to_string()));
        }

        let connection = self.connection()?;
        let now = unix_time_seconds()?;
        let key = profile_sync_root_key(root_id);
        connection
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
                    created_at INTEGER NOT NULL,
                    updated_at INTEGER NOT NULL,
                    PRIMARY KEY(profile, device_id)
                );

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
            .map_err(|source| self.database_error(source))
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

        for domain in DEFAULT_APP_SYNC_DOMAINS {
            self.register_app_sync_domain(&AppSyncDomainRegistration {
                profile: DEFAULT_PROFILE_ID.to_string(),
                domain: domain.domain.to_string(),
                schema_version: domain.schema_version,
                enabled: true,
                privacy_classification: domain.privacy_classification.to_string(),
                sync_content: domain.sync_content,
            })?;
        }
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

fn settings_snapshot_id(covers_revision: i64) -> String {
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
        created_at: row.get(4)?,
        updated_at: row.get(5)?,
    })
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
                && domain.enabled
                && domain.privacy_classification == "content"
                && domain.sync_content
        }));
        assert!(domains.iter().any(|domain| {
            domain.domain == SYNC_DOMAIN_CONTACTS
                && domain.enabled
                && domain.privacy_classification == "sensitive"
                && !domain.sync_content
        }));

        let devices = database.sync_devices(DEFAULT_PROFILE_ID).unwrap();
        assert_eq!(devices.len(), 1);
        assert_eq!(devices[0].device_id, DEFAULT_SYNC_DEVICE_ID);
        assert_eq!(devices[0].membership_epoch, 1);
        assert!(!devices[0].provider_authority);
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
        assert_eq!(second_record.created_at, first_record.created_at);
        assert!(second_record.updated_at >= first_record.updated_at);
        assert_eq!(
            database
                .sync_device_public_keys(DEFAULT_PROFILE_ID)
                .unwrap()
                .len(),
            1
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
