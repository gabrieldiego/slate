#![forbid(unsafe_code)]

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum AppId {
    Web,
    Downloads,
    Calendar,
    Chat,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum AppIcon {
    Globe,
    Download,
    Calendar,
    Chat,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct AppDescriptor {
    pub id: AppId,
    pub label: &'static str,
    pub icon: AppIcon,
}

pub const DEFAULT_APPS: [AppDescriptor; 4] = [
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
}
