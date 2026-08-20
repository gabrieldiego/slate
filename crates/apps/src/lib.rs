#![forbid(unsafe_code)]

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum AppId {
    Web,
    Downloads,
    Calendar,
    Chat,
    Contacts,
    Files,
    Settings,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum AppIcon {
    Globe,
    Download,
    Calendar,
    Chat,
    Contacts,
    Files,
    Settings,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct AppDescriptor {
    pub id: AppId,
    pub label: &'static str,
    pub icon: AppIcon,
    pub sync: AppSyncDescriptor,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct AppSyncDescriptor {
    pub domain: &'static str,
    pub privacy_classification: SyncPrivacyClass,
    pub sync_content: bool,
    pub default_enabled: bool,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum SyncPrivacyClass {
    LowRisk,
    Metadata,
    Sensitive,
    Content,
}

impl SyncPrivacyClass {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::LowRisk => "low-risk",
            Self::Metadata => "metadata",
            Self::Sensitive => "sensitive",
            Self::Content => "content",
        }
    }
}

pub const DEFAULT_APPS: [AppDescriptor; 7] = [
    AppDescriptor {
        id: AppId::Web,
        label: "Web",
        icon: AppIcon::Globe,
        sync: AppSyncDescriptor {
            domain: "settings",
            privacy_classification: SyncPrivacyClass::LowRisk,
            sync_content: false,
            default_enabled: true,
        },
    },
    AppDescriptor {
        id: AppId::Downloads,
        label: "Downloads",
        icon: AppIcon::Download,
        sync: AppSyncDescriptor {
            domain: "downloads",
            privacy_classification: SyncPrivacyClass::Metadata,
            sync_content: false,
            default_enabled: true,
        },
    },
    AppDescriptor {
        id: AppId::Calendar,
        label: "Calendar",
        icon: AppIcon::Calendar,
        sync: AppSyncDescriptor {
            domain: "calendar",
            privacy_classification: SyncPrivacyClass::Sensitive,
            sync_content: false,
            default_enabled: false,
        },
    },
    AppDescriptor {
        id: AppId::Chat,
        label: "Chat",
        icon: AppIcon::Chat,
        sync: AppSyncDescriptor {
            domain: "chat",
            privacy_classification: SyncPrivacyClass::Sensitive,
            sync_content: false,
            default_enabled: false,
        },
    },
    AppDescriptor {
        id: AppId::Contacts,
        label: "Contacts",
        icon: AppIcon::Contacts,
        sync: AppSyncDescriptor {
            domain: "contacts",
            privacy_classification: SyncPrivacyClass::Sensitive,
            sync_content: false,
            default_enabled: false,
        },
    },
    AppDescriptor {
        id: AppId::Files,
        label: "Files",
        icon: AppIcon::Files,
        sync: AppSyncDescriptor {
            domain: "files",
            privacy_classification: SyncPrivacyClass::Content,
            sync_content: true,
            default_enabled: false,
        },
    },
    AppDescriptor {
        id: AppId::Settings,
        label: "Settings",
        icon: AppIcon::Settings,
        sync: AppSyncDescriptor {
            domain: "settings",
            privacy_classification: SyncPrivacyClass::LowRisk,
            sync_content: false,
            default_enabled: true,
        },
    },
];

pub const FUTURE_SYNC_DOMAINS: [AppSyncDescriptor; 2] = [
    AppSyncDescriptor {
        domain: "bookmarks",
        privacy_classification: SyncPrivacyClass::LowRisk,
        sync_content: false,
        default_enabled: true,
    },
    AppSyncDescriptor {
        domain: "storage",
        privacy_classification: SyncPrivacyClass::Sensitive,
        sync_content: false,
        default_enabled: false,
    },
];

pub fn default_apps() -> &'static [AppDescriptor] {
    &DEFAULT_APPS
}

pub fn app_for_sync_domain(domain: &str) -> Option<&'static AppDescriptor> {
    default_apps().iter().find(|app| app.sync.domain == domain)
}

#[cfg(test)]
mod tests {
    use super::{AppIcon, AppId, SyncPrivacyClass, app_for_sync_domain, default_apps};

    #[test]
    fn default_apps_start_with_web() {
        assert_eq!(default_apps()[0].id, AppId::Web);
    }

    #[test]
    fn default_apps_include_chat() {
        let chat = default_apps()
            .iter()
            .find(|app| app.id == AppId::Chat)
            .expect("chat app should be registered");

        assert_eq!(chat.label, "Chat");
        assert_eq!(chat.icon, AppIcon::Chat);
        assert_eq!(chat.sync.domain, "chat");
        assert_eq!(
            chat.sync.privacy_classification,
            SyncPrivacyClass::Sensitive
        );
    }

    #[test]
    fn default_apps_include_next_mock_apps() {
        let apps = default_apps();

        assert!(apps.iter().any(|app| app.id == AppId::Contacts
            && app.label == "Contacts"
            && app.icon == AppIcon::Contacts));
        assert!(apps.iter().any(|app| app.id == AppId::Files
            && app.label == "Files"
            && app.icon == AppIcon::Files));
        assert!(apps.iter().any(|app| app.id == AppId::Settings
            && app.label == "Settings"
            && app.icon == AppIcon::Settings));
    }

    #[test]
    fn rail_apps_declare_sync_domains() {
        for app in default_apps() {
            assert!(!app.sync.domain.is_empty());
            assert!(!app.sync.privacy_classification.as_str().is_empty());
        }

        assert_eq!(
            app_for_sync_domain("calendar").map(|app| app.id),
            Some(AppId::Calendar)
        );
        assert_eq!(
            app_for_sync_domain("files").map(|app| app.sync.sync_content),
            Some(true)
        );
        assert_eq!(
            app_for_sync_domain("settings").map(|app| app.sync.default_enabled),
            Some(true)
        );
    }
}
