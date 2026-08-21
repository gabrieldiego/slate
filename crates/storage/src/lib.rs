#![forbid(unsafe_code)]

use ring::{aead, rand};
use serde::{Deserialize, Serialize};
use std::fmt;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::{SystemTime, UNIX_EPOCH};

use ring::rand::SecureRandom;
use rusqlite::{Connection, OptionalExtension, params};

pub const DEFAULT_DATABASE_FILE_NAME: &str = "slate-settings.db";
pub const DEFAULT_HOME_DIRECTORY_NAME: &str = ".slate";
pub const DEFAULT_PROFILE_ID: &str = "default";
pub const DEFAULT_SYNC_DEVICE_ID: &str = "local-device";
pub const PROFILE_SYNC_CONTENT_KEY_BYTES: usize = 32;
pub const PROFILE_SYNC_NONCE_BYTES: usize = 12;
pub const SYNC_OBJECT_VERSION: u8 = 1;
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
    Encrypt,
    Decrypt,
    UnsupportedVersion(u8),
    InvalidNonceLength { actual: usize },
    Encode(serde_json::Error),
    Decode(serde_json::Error),
}

impl fmt::Display for SyncObjectError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Random => write!(formatter, "failed to generate sync object nonce"),
            Self::Encrypt => write!(formatter, "failed to encrypt sync object"),
            Self::Decrypt => write!(formatter, "failed to decrypt sync object"),
            Self::UnsupportedVersion(version) => {
                write!(formatter, "unsupported sync object version: {version}")
            }
            Self::InvalidNonceLength { actual } => {
                write!(formatter, "invalid sync object nonce length: {actual}")
            }
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
            | Self::Encrypt
            | Self::Decrypt
            | Self::UnsupportedVersion(_)
            | Self::InvalidNonceLength { .. } => None,
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
    Clock(std::time::SystemTimeError),
    InvalidSyncDeviceId(String),
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
            Self::Clock(error) => write!(formatter, "failed to read system clock: {error}"),
            Self::InvalidSyncDeviceId(device_id) => {
                write!(formatter, "invalid sync device id: {device_id}")
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
            Self::Clock(error) => Some(error),
            Self::InvalidSyncDeviceId(_) => None,
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

        record_sync_device_seen_in_transaction(
            &transaction,
            change.profile.as_str(),
            change.device_id.as_str(),
            now,
        )
        .map_err(|source| self.database_error(source))?;

        if let Some(existing) = sync_change_by_device_sequence_in_transaction(
            &transaction,
            change.profile.as_str(),
            change.device_id.as_str(),
            change.device_sequence,
        )
        .map_err(|source| self.database_error(source))?
        {
            transaction
                .commit()
                .map_err(|source| self.database_error(source))?;
            return Ok(existing);
        }

        if change.profile == DEFAULT_PROFILE_ID && change.domain == SYNC_DOMAIN_SETTINGS {
            transaction
                .execute(
                    "INSERT INTO settings (key, value, updated_at)
                     VALUES (?1, ?2, ?3)
                     ON CONFLICT(key) DO UPDATE SET
                       value = excluded.value,
                       updated_at = excluded.updated_at",
                    params![change.key.as_str(), change.value.as_str(), now],
                )
                .map_err(|source| self.database_error(source))?;
        }

        let applied = insert_sync_setting_text_change_in_transaction(
            &transaction,
            change.profile.as_str(),
            change.domain.as_str(),
            change.key.as_str(),
            change.value.as_str(),
            change.device_id.as_str(),
            change.device_sequence,
            change.logical_clock,
            now,
        )
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
    transaction.execute(
        "INSERT INTO settings_changes
           (profile, domain, entity_key, operation, payload, device_id, device_sequence,
            logical_clock, created_at, applied_at)
         VALUES (?1, ?2, ?3, 'set_text', ?4, ?5, ?6, ?7, ?8, ?8)",
        params![
            profile,
            domain,
            key,
            value,
            device_id,
            device_sequence,
            logical_clock,
            now
        ],
    )?;
    let change_id = transaction.last_insert_rowid();
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
        applied_at: Some(now),
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
