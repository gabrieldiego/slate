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
}

pub const DEFAULT_APPS: [AppDescriptor; 7] = [
    AppDescriptor {
        id: AppId::Web,
        label: "Web",
        icon: AppIcon::Globe,
    },
    AppDescriptor {
        id: AppId::Downloads,
        label: "Downloads",
        icon: AppIcon::Download,
    },
    AppDescriptor {
        id: AppId::Calendar,
        label: "Calendar",
        icon: AppIcon::Calendar,
    },
    AppDescriptor {
        id: AppId::Chat,
        label: "Chat",
        icon: AppIcon::Chat,
    },
    AppDescriptor {
        id: AppId::Contacts,
        label: "Contacts",
        icon: AppIcon::Contacts,
    },
    AppDescriptor {
        id: AppId::Files,
        label: "Files",
        icon: AppIcon::Files,
    },
    AppDescriptor {
        id: AppId::Settings,
        label: "Settings",
        icon: AppIcon::Settings,
    },
];

pub fn default_apps() -> &'static [AppDescriptor] {
    &DEFAULT_APPS
}

#[cfg(test)]
mod tests {
    use super::{AppIcon, AppId, default_apps};

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
}
