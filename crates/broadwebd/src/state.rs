use crate::BroadwebdError;
use std::fs;
use std::path::{Path, PathBuf};

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
