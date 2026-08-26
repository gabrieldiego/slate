use crate::object_ids::push_unique_object_id;
use serde::{Deserialize, Serialize};
use slate_storage::{ProfileSyncRootRecord, SyncAccountMembershipRecordApplication};
use std::collections::BTreeSet;

pub const PROFILE_SYNC_MEMBERSHIP_LOG_SCHEMA_VERSION: u8 = 1;
pub const PROFILE_SYNC_MEMBERSHIP_LOG_ROOT_ID: &str = "account/membership/log";
pub const PROFILE_SYNC_MEMBERSHIP_LOG_MAX_RECORDS: usize = 512;

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct PublishedProfileSyncMembershipRecord {
    pub profile: String,
    pub root_id: String,
    pub object_id: String,
    pub signed_record: Vec<u8>,
}

#[derive(Clone, Debug, Eq, PartialEq, Deserialize, Serialize)]
pub struct ProfileSyncMembershipLog {
    pub profile: String,
    pub schema_version: u8,
    pub records: Vec<ProfileSyncMembershipLogEntry>,
}

#[derive(Clone, Debug, Eq, PartialEq, Deserialize, Serialize)]
pub struct ProfileSyncMembershipLogEntry {
    pub record_id: String,
    pub root_id: String,
    pub object_id: String,
    pub membership_epoch: i64,
    pub record_kind: String,
    pub device_id: String,
    pub signer_device_id: String,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct PublishedProfileSyncMembershipLog {
    pub profile: String,
    pub root_id: String,
    pub object_id: String,
    pub log: ProfileSyncMembershipLog,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ProfileSyncMembershipLogPublicationPlanStatus {
    Empty,
    Publishable,
    TooLarge,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ProfileSyncMembershipLogPublicationPlan {
    pub profile: String,
    pub root_id: String,
    pub record_count: usize,
    pub max_records: usize,
    pub status: ProfileSyncMembershipLogPublicationPlanStatus,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ProfileSyncMembershipLogPreviewStatus {
    NoPublishedRoot,
    Unchanged,
    Available,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ProfileSyncMembershipLogPreview {
    pub profile: String,
    pub root_id: String,
    pub object_id: Option<String>,
    pub record_count: usize,
    pub max_records: usize,
    pub status: ProfileSyncMembershipLogPreviewStatus,
}

impl ProfileSyncMembershipLogPublicationPlan {
    pub fn for_record_count(profile: &str, root_id: &str, record_count: usize) -> Self {
        let status = if record_count == 0 {
            ProfileSyncMembershipLogPublicationPlanStatus::Empty
        } else if record_count > PROFILE_SYNC_MEMBERSHIP_LOG_MAX_RECORDS {
            ProfileSyncMembershipLogPublicationPlanStatus::TooLarge
        } else {
            ProfileSyncMembershipLogPublicationPlanStatus::Publishable
        };
        Self {
            profile: profile.to_string(),
            root_id: root_id.to_string(),
            record_count,
            max_records: PROFILE_SYNC_MEMBERSHIP_LOG_MAX_RECORDS,
            status,
        }
    }

    pub fn is_empty(&self) -> bool {
        self.status == ProfileSyncMembershipLogPublicationPlanStatus::Empty
    }

    pub fn is_publishable(&self) -> bool {
        self.status == ProfileSyncMembershipLogPublicationPlanStatus::Publishable
    }

    pub fn requires_compaction(&self) -> bool {
        self.status == ProfileSyncMembershipLogPublicationPlanStatus::TooLarge
    }
}

impl ProfileSyncMembershipLogPreview {
    pub fn no_published_root(profile: &str, root_id: &str) -> Self {
        Self {
            profile: profile.to_string(),
            root_id: root_id.to_string(),
            object_id: None,
            record_count: 0,
            max_records: PROFILE_SYNC_MEMBERSHIP_LOG_MAX_RECORDS,
            status: ProfileSyncMembershipLogPreviewStatus::NoPublishedRoot,
        }
    }

    pub fn unchanged(profile: &str, root_id: &str, object_id: String) -> Self {
        Self {
            profile: profile.to_string(),
            root_id: root_id.to_string(),
            object_id: Some(object_id),
            record_count: 0,
            max_records: PROFILE_SYNC_MEMBERSHIP_LOG_MAX_RECORDS,
            status: ProfileSyncMembershipLogPreviewStatus::Unchanged,
        }
    }

    pub fn available(profile: &str, root_id: &str, object_id: String, record_count: usize) -> Self {
        Self {
            profile: profile.to_string(),
            root_id: root_id.to_string(),
            object_id: Some(object_id),
            record_count,
            max_records: PROFILE_SYNC_MEMBERSHIP_LOG_MAX_RECORDS,
            status: ProfileSyncMembershipLogPreviewStatus::Available,
        }
    }

    pub fn requires_pull(&self) -> bool {
        self.status == ProfileSyncMembershipLogPreviewStatus::Available
    }

    pub fn is_unchanged(&self) -> bool {
        self.status == ProfileSyncMembershipLogPreviewStatus::Unchanged
    }
}

impl PublishedProfileSyncMembershipLog {
    pub fn published_object_ids(&self) -> Vec<String> {
        let mut object_ids = Vec::new();
        let mut seen = BTreeSet::new();
        for entry in &self.log.records {
            push_unique_object_id(&mut object_ids, &mut seen, entry.object_id.as_str());
        }
        push_unique_object_id(&mut object_ids, &mut seen, self.object_id.as_str());
        object_ids
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum ProfileSyncMembershipRecordPullStatus {
    NoPublishedRoot {
        profile: String,
        root_id: String,
    },
    Unchanged {
        profile: String,
        root_id: String,
        object_id: String,
    },
    Applied {
        root: ProfileSyncRootRecord,
        application: SyncAccountMembershipRecordApplication,
    },
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum ProfileSyncMembershipLogPullStatus {
    NoPublishedRoot {
        profile: String,
        root_id: String,
    },
    Unchanged {
        profile: String,
        root_id: String,
        object_id: String,
    },
    Applied {
        root: ProfileSyncRootRecord,
        log: ProfileSyncMembershipLog,
        applications: Vec<SyncAccountMembershipRecordApplication>,
    },
}

impl ProfileSyncMembershipLogPullStatus {
    pub fn applied_count(&self) -> usize {
        match self {
            Self::Applied { applications, .. } => applications.len(),
            Self::NoPublishedRoot { .. } | Self::Unchanged { .. } => 0,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::{
        PROFILE_SYNC_MEMBERSHIP_LOG_MAX_RECORDS, PROFILE_SYNC_MEMBERSHIP_LOG_SCHEMA_VERSION,
        ProfileSyncMembershipLog, ProfileSyncMembershipLogEntry, ProfileSyncMembershipLogPreview,
        ProfileSyncMembershipLogPreviewStatus, ProfileSyncMembershipLogPublicationPlan,
        ProfileSyncMembershipLogPublicationPlanStatus, ProfileSyncMembershipLogPullStatus,
        PublishedProfileSyncMembershipLog,
    };
    use slate_storage::{
        ProfileSyncRootRecord, SyncAccountMembershipRecord, SyncAccountMembershipRecordApplication,
    };

    fn log_entry(record_id: &str, object_id: &str) -> ProfileSyncMembershipLogEntry {
        ProfileSyncMembershipLogEntry {
            record_id: record_id.to_string(),
            root_id: format!("account/membership/{record_id}"),
            object_id: object_id.to_string(),
            membership_epoch: 1,
            record_kind: "enroll-device".to_string(),
            device_id: "device-a".to_string(),
            signer_device_id: "authority-a".to_string(),
        }
    }

    fn root_record() -> ProfileSyncRootRecord {
        ProfileSyncRootRecord {
            profile: "profile-a".to_string(),
            root_id: "account/membership/log".to_string(),
            object_id: "log-object".to_string(),
            updated_at: 1,
        }
    }

    fn applied_record(record_id: &str) -> SyncAccountMembershipRecordApplication {
        SyncAccountMembershipRecordApplication {
            membership_record: SyncAccountMembershipRecord {
                profile: "profile-a".to_string(),
                record_id: record_id.to_string(),
                membership_epoch: 1,
                record_kind: "enroll-device".to_string(),
                device_id: "device-a".to_string(),
                signer_device_id: "authority-a".to_string(),
                signed_record: vec![1, 2, 3],
                created_at: 1,
                applied_at: Some(2),
            },
            device_key: None,
            bootstrapped: false,
            applied: true,
        }
    }

    #[test]
    fn membership_log_publication_plan_classifies_record_count_boundaries() {
        let empty =
            ProfileSyncMembershipLogPublicationPlan::for_record_count("profile-a", "root-a", 0);
        assert!(empty.is_empty());
        assert!(!empty.is_publishable());
        assert!(!empty.requires_compaction());
        assert_eq!(
            empty.status,
            ProfileSyncMembershipLogPublicationPlanStatus::Empty
        );
        assert_eq!(empty.max_records, PROFILE_SYNC_MEMBERSHIP_LOG_MAX_RECORDS);

        let capped = ProfileSyncMembershipLogPublicationPlan::for_record_count(
            "profile-a",
            "root-a",
            PROFILE_SYNC_MEMBERSHIP_LOG_MAX_RECORDS,
        );
        assert!(!capped.is_empty());
        assert!(capped.is_publishable());
        assert!(!capped.requires_compaction());
        assert_eq!(
            capped.status,
            ProfileSyncMembershipLogPublicationPlanStatus::Publishable
        );

        let oversized = ProfileSyncMembershipLogPublicationPlan::for_record_count(
            "profile-a",
            "root-a",
            PROFILE_SYNC_MEMBERSHIP_LOG_MAX_RECORDS + 1,
        );
        assert!(!oversized.is_empty());
        assert!(!oversized.is_publishable());
        assert!(oversized.requires_compaction());
        assert_eq!(
            oversized.status,
            ProfileSyncMembershipLogPublicationPlanStatus::TooLarge
        );
    }

    #[test]
    fn membership_log_preview_constructors_classify_pull_state() {
        let no_root = ProfileSyncMembershipLogPreview::no_published_root("profile-a", "root-a");
        assert!(!no_root.requires_pull());
        assert!(!no_root.is_unchanged());
        assert_eq!(no_root.object_id, None);
        assert_eq!(
            no_root.status,
            ProfileSyncMembershipLogPreviewStatus::NoPublishedRoot
        );

        let unchanged =
            ProfileSyncMembershipLogPreview::unchanged("profile-a", "root-a", "object-a".into());
        assert!(!unchanged.requires_pull());
        assert!(unchanged.is_unchanged());
        assert_eq!(
            unchanged.status,
            ProfileSyncMembershipLogPreviewStatus::Unchanged
        );

        let available =
            ProfileSyncMembershipLogPreview::available("profile-a", "root-a", "object-b".into(), 2);
        assert!(available.requires_pull());
        assert!(!available.is_unchanged());
        assert_eq!(available.record_count, 2);
        assert_eq!(
            available.status,
            ProfileSyncMembershipLogPreviewStatus::Available
        );
    }

    #[test]
    fn published_membership_log_deduplicates_record_objects_before_log_object() {
        let published = PublishedProfileSyncMembershipLog {
            profile: "profile-a".to_string(),
            root_id: "account/membership/log".to_string(),
            object_id: "log-object".to_string(),
            log: ProfileSyncMembershipLog {
                profile: "profile-a".to_string(),
                schema_version: PROFILE_SYNC_MEMBERSHIP_LOG_SCHEMA_VERSION,
                records: vec![
                    log_entry("record-a", "object-a"),
                    log_entry("record-b", "object-b"),
                    log_entry("record-c", "object-a"),
                    log_entry("record-d", "log-object"),
                ],
            },
        };

        assert_eq!(
            published.published_object_ids(),
            vec!["object-a", "object-b", "log-object"]
        );
    }

    #[test]
    fn membership_log_pull_status_counts_only_applied_records() {
        let no_root = ProfileSyncMembershipLogPullStatus::NoPublishedRoot {
            profile: "profile-a".to_string(),
            root_id: "account/membership/log".to_string(),
        };
        assert_eq!(no_root.applied_count(), 0);

        let unchanged = ProfileSyncMembershipLogPullStatus::Unchanged {
            profile: "profile-a".to_string(),
            root_id: "account/membership/log".to_string(),
            object_id: "log-object".to_string(),
        };
        assert_eq!(unchanged.applied_count(), 0);

        let applied = ProfileSyncMembershipLogPullStatus::Applied {
            root: root_record(),
            log: ProfileSyncMembershipLog {
                profile: "profile-a".to_string(),
                schema_version: PROFILE_SYNC_MEMBERSHIP_LOG_SCHEMA_VERSION,
                records: Vec::new(),
            },
            applications: vec![applied_record("record-a"), applied_record("record-b")],
        };
        assert_eq!(applied.applied_count(), 2);
    }
}
