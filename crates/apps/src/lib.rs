#![forbid(unsafe_code)]

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum AppId {
    Web,
    Downloads,
    Calendar,
    Messaging,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum AppIcon {
    Globe,
    Download,
    Calendar,
    Message,
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
        id: AppId::Messaging,
        label: "Messaging",
        icon: AppIcon::Message,
    },
];

pub fn default_apps() -> &'static [AppDescriptor] {
    &DEFAULT_APPS
}

#[cfg(test)]
mod tests {
    use super::{AppId, default_apps};

    #[test]
    fn default_apps_start_with_web() {
        assert_eq!(default_apps()[0].id, AppId::Web);
    }
}
