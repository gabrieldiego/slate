/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at https://mozilla.org/MPL/2.0/. */

//! Runtime watcher for syncable profile settings.

use std::sync::atomic::{AtomicI64, Ordering};

use log::warn;
use slate_storage::{DEFAULT_PROFILE_ID, SlateProfileDatabase, StorageError};

use crate::desktop::protocols::slate::{
    apply_synced_chrome_settings_from_database, initialize_chrome_settings_from_database,
};

const DEFAULT_SETTINGS_EVENT_BATCH_LIMIT: u32 = 64;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) struct SyncedChromeSettingsPoll {
    pub previous_revision: i64,
    pub latest_revision: i64,
}

impl SyncedChromeSettingsPoll {
    pub(crate) fn advanced(&self) -> bool {
        self.latest_revision > self.previous_revision
    }
}

pub(crate) struct SyncedChromeSettingsWatcher {
    database: SlateProfileDatabase,
    settings_sync_revision: AtomicI64,
    batch_limit: u32,
}

impl SyncedChromeSettingsWatcher {
    pub(crate) fn new(database: SlateProfileDatabase) -> Self {
        Self::with_batch_limit(database, DEFAULT_SETTINGS_EVENT_BATCH_LIMIT)
    }

    pub(crate) fn with_batch_limit(database: SlateProfileDatabase, batch_limit: u32) -> Self {
        initialize_chrome_settings_from_database(&database);
        let settings_sync_revision = latest_settings_sync_revision(&database);
        Self {
            database,
            settings_sync_revision: AtomicI64::new(settings_sync_revision),
            batch_limit: batch_limit.max(1),
        }
    }

    pub(crate) fn current_revision(&self) -> i64 {
        self.settings_sync_revision.load(Ordering::Relaxed)
    }

    pub(crate) fn poll_once(&self) -> Result<SyncedChromeSettingsPoll, StorageError> {
        let previous_revision = self.current_revision();
        let latest_revision = apply_synced_chrome_settings_from_database(
            &self.database,
            previous_revision,
            self.batch_limit,
        )?;
        self.settings_sync_revision
            .store(latest_revision, Ordering::Relaxed);
        Ok(SyncedChromeSettingsPoll {
            previous_revision,
            latest_revision,
        })
    }

    pub(crate) fn poll_once_logged(&self) {
        if let Err(error) = self.poll_once() {
            warn!("failed to refresh synced chrome settings: {error}");
        }
    }
}

fn latest_settings_sync_revision(database: &SlateProfileDatabase) -> i64 {
    match database.latest_sync_revision(DEFAULT_PROFILE_ID) {
        Ok(revision) => revision,
        Err(error) => {
            warn!("failed to read latest settings sync revision: {error}");
            0
        }
    }
}

#[cfg(test)]
mod tests {
    use super::SyncedChromeSettingsWatcher;
    use crate::desktop::key_bindings::{
        SlateKeyBindings, current_key_bindings_json_value, set_current_key_bindings,
    };
    use crate::desktop::protocols::slate::{
        CHROME_ELEMENT_ZOOM_SETTING_DEFAULT, current_chrome_element_zoom_setting,
        set_current_chrome_element_zoom_setting,
    };
    use slate_storage::{DEFAULT_PROFILE_ID, SYNC_DOMAIN_SETTINGS, SlateProfileDatabase};
    use std::path::PathBuf;
    use std::sync::atomic::{AtomicU64, Ordering};

    fn unique_database_path(name: &str) -> PathBuf {
        static COUNTER: AtomicU64 = AtomicU64::new(0);
        std::env::temp_dir().join(format!(
            "slate-settings-watcher-{name}-{}-{}.db",
            std::process::id(),
            COUNTER.fetch_add(1, Ordering::Relaxed)
        ))
    }

    #[test]
    fn watcher_initializes_from_database_without_replaying_history() {
        let path = unique_database_path("initial");
        let database = SlateProfileDatabase::open_resolved(path.clone()).unwrap();
        database
            .set_sync_setting_text(
                DEFAULT_PROFILE_ID,
                SYNC_DOMAIN_SETTINGS,
                "chrome.zoom",
                "1.03",
            )
            .unwrap();
        let latest_before_watcher = database.latest_sync_revision(DEFAULT_PROFILE_ID).unwrap();

        set_current_chrome_element_zoom_setting(CHROME_ELEMENT_ZOOM_SETTING_DEFAULT);
        let watcher = SyncedChromeSettingsWatcher::new(database.clone());

        assert_eq!(watcher.current_revision(), latest_before_watcher);
        assert_eq!(current_chrome_element_zoom_setting(), 1.03);
        let poll = watcher.poll_once().unwrap();
        assert_eq!(poll.previous_revision, latest_before_watcher);
        assert_eq!(poll.latest_revision, latest_before_watcher);
        assert!(!poll.advanced());

        drop(database);
        let _ = std::fs::remove_file(path);
    }

    #[test]
    fn watcher_applies_new_sync_revisions_incrementally() {
        let path = unique_database_path("incremental");
        let database = SlateProfileDatabase::open_resolved(path.clone()).unwrap();
        let watcher = SyncedChromeSettingsWatcher::with_batch_limit(database.clone(), 1);

        set_current_chrome_element_zoom_setting(CHROME_ELEMENT_ZOOM_SETTING_DEFAULT);
        set_current_key_bindings(SlateKeyBindings::default());
        database
            .set_sync_setting_text(
                DEFAULT_PROFILE_ID,
                SYNC_DOMAIN_SETTINGS,
                "chrome.zoom",
                "1.05",
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

        let first_poll = watcher.poll_once().unwrap();
        assert!(first_poll.advanced());
        assert_eq!(current_chrome_element_zoom_setting(), 1.05);

        let second_poll = watcher.poll_once().unwrap();
        assert!(second_poll.advanced());
        let key_bindings = current_key_bindings_json_value();
        let next_tab = key_bindings
            .as_array()
            .unwrap()
            .iter()
            .find(|entry| entry["id"] == "next_tab")
            .expect("next_tab binding");
        assert_eq!(next_tab["value"], "Alt+ArrowRight");

        let idle_poll = watcher.poll_once().unwrap();
        assert!(!idle_poll.advanced());
        assert_eq!(idle_poll.previous_revision, idle_poll.latest_revision);

        drop(database);
        let _ = std::fs::remove_file(path);
    }
}
