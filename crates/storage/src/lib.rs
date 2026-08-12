#![forbid(unsafe_code)]

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
