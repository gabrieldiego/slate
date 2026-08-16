#![forbid(unsafe_code)]

use std::fmt;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::{SystemTime, UNIX_EPOCH};

use rusqlite::{Connection, OptionalExtension, params};

pub const DEFAULT_DATABASE_FILE_NAME: &str = "slate-settings.db";
pub const DEFAULT_HOME_DIRECTORY_NAME: &str = ".slate";
pub const DEFAULT_PROFILE_ID: &str = "default";

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
        };
        database.initialize()?;
        database.try_seed_default_bookmarks();
        Ok(database)
    }

    pub fn path(&self) -> &Path {
        self.path.as_ref()
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
        let connection = self.connection()?;
        let now = unix_time_seconds()?;
        connection
            .execute(
                "INSERT INTO settings (key, value, updated_at)
                 VALUES (?1, ?2, ?3)
                 ON CONFLICT(key) DO UPDATE SET
                   value = excluded.value,
                   updated_at = excluded.updated_at",
                params![key, value, now],
            )
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

                INSERT OR IGNORE INTO schema_migrations(version, applied_at)
                    VALUES (1, CAST(strftime('%s', 'now') AS INTEGER));
                ",
            )
            .map_err(|source| self.database_error(source))
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
