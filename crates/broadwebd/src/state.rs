use crate::BroadwebdError;
use std::fs;
use std::fs::OpenOptions;
use std::io::Write;
use std::path::{Path, PathBuf};

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct TemporaryDownloadRecord {
    pub profile: String,
    pub filename: String,
    pub path: PathBuf,
    pub size_bytes: u64,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct StateRoot {
    path: PathBuf,
}

impl StateRoot {
    pub fn prepare(path: impl Into<PathBuf>) -> Result<Self, BroadwebdError> {
        let path = path.into();
        fs::create_dir_all(path.join("profiles"))?;
        fs::create_dir_all(path.join("volatile"))?;
        Ok(Self { path })
    }

    pub fn path(&self) -> &Path {
        &self.path
    }

    pub fn profile_root(&self, profile: &str) -> Result<PathBuf, BroadwebdError> {
        validate_profile_id(profile)?;
        Ok(self.path.join("profiles").join(profile))
    }

    pub fn prepare_profile(&self, profile: &str) -> Result<PathBuf, BroadwebdError> {
        let root = self.profile_root(profile)?;
        fs::create_dir_all(root.join("protocol-state"))?;
        fs::create_dir_all(root.join("temporary"))?;
        Ok(root)
    }

    pub fn store_temporary_download(
        &self,
        profile: &str,
        filename: &str,
        body: &[u8],
    ) -> Result<PathBuf, BroadwebdError> {
        let root = self.prepare_profile(profile)?;
        let downloads_root = root.join("temporary").join("downloads");
        fs::create_dir_all(&downloads_root)?;
        let filename = sanitized_filename(filename);
        for candidate in download_filename_candidates(&filename).take(1000) {
            let path = downloads_root.join(candidate);
            match OpenOptions::new().write(true).create_new(true).open(&path) {
                Ok(mut file) => {
                    file.write_all(body)?;
                    return Ok(path);
                }
                Err(error) if error.kind() == std::io::ErrorKind::AlreadyExists => {}
                Err(error) => return Err(error.into()),
            }
        }
        Err(BroadwebdError::UnsupportedRequest(format!(
            "could not allocate a temporary download filename for {filename}"
        )))
    }

    pub fn temporary_downloads(
        &self,
        profile: &str,
    ) -> Result<Vec<TemporaryDownloadRecord>, BroadwebdError> {
        let root = self.profile_root(profile)?;
        let downloads_root = root.join("temporary").join("downloads");
        let entries = match fs::read_dir(downloads_root) {
            Ok(entries) => entries,
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(Vec::new()),
            Err(error) => return Err(error.into()),
        };

        let mut downloads = Vec::new();
        for entry in entries {
            let entry = entry?;
            let metadata = entry.metadata()?;
            if !metadata.is_file() {
                continue;
            }

            let path = entry.path();
            let Some(filename) = path
                .file_name()
                .and_then(|filename| filename.to_str())
                .map(ToOwned::to_owned)
            else {
                continue;
            };

            downloads.push(TemporaryDownloadRecord {
                profile: profile.to_string(),
                filename,
                path,
                size_bytes: metadata.len(),
            });
        }

        downloads.sort_by(|first, second| first.filename.cmp(&second.filename));
        Ok(downloads)
    }
}

fn validate_profile_id(profile: &str) -> Result<(), BroadwebdError> {
    if !profile.is_empty()
        && profile
            .chars()
            .all(|ch| ch.is_ascii_alphanumeric() || matches!(ch, '-' | '_'))
    {
        return Ok(());
    }

    Err(BroadwebdError::InvalidProfile(profile.to_string()))
}

fn sanitized_filename(filename: &str) -> String {
    let mut output = String::new();
    for ch in filename.chars().take(160) {
        if ch.is_ascii_alphanumeric() || matches!(ch, '.' | '-' | '_') {
            output.push(ch);
        } else {
            output.push('_');
        }
    }
    let output = output.trim_matches('.');
    if output.is_empty() {
        "download".to_string()
    } else {
        output.to_string()
    }
}

fn download_filename_candidates(filename: &str) -> impl Iterator<Item = String> + '_ {
    let (stem, extension) = filename
        .rsplit_once('.')
        .filter(|(stem, extension)| !stem.is_empty() && !extension.is_empty())
        .map(|(stem, extension)| (stem.to_string(), format!(".{extension}")))
        .unwrap_or_else(|| (filename.to_string(), String::new()));

    std::iter::once(filename.to_string())
        .chain((1..).map(move |suffix| format!("{stem}-{suffix}{extension}")))
}
