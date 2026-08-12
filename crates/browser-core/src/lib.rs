#![forbid(unsafe_code)]

use slate_apps::AppId;
use slate_rendering::{MetricAccent, RenderBackend, RenderMetric, RenderSurface};

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct BrowserState {
    pub tabs: Vec<Tab>,
    pub active_tab: usize,
    pub active_app: AppId,
    pub backend_name: &'static str,
    pub surface: RenderSurface,
    pub status: BrowserStatus,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Tab {
    pub title: String,
    pub address: String,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct BrowserStatus {
    pub privacy: String,
    pub sync: String,
}

impl BrowserState {
    pub fn new<B: RenderBackend>(backend: &B) -> Self {
        let surface = backend.load_home();
        Self {
            tabs: vec![
                Tab {
                    title: surface.title.clone(),
                    address: surface.address.clone(),
                },
                Tab {
                    title: "Research".to_string(),
                    address: "https://servo.org".to_string(),
                },
                Tab {
                    title: "Calendar".to_string(),
                    address: "slate://calendar".to_string(),
                },
            ],
            active_tab: 0,
            active_app: AppId::Web,
            backend_name: backend.name(),
            surface,
            status: BrowserStatus {
                privacy: "Protected. Private. Yours.".to_string(),
                sync: "Sync On".to_string(),
            },
        }
    }

    pub fn active_tab(&self) -> Option<&Tab> {
        self.tabs.get(self.active_tab)
    }

    pub fn activate_tab(&mut self, index: usize) -> bool {
        let Some(tab) = self.tabs.get(index) else {
            return false;
        };

        self.active_tab = index;
        self.active_app = app_for_address(&tab.address);
        self.surface = surface_for_tab(tab);
        true
    }

    pub fn select_app(&mut self, app: AppId) {
        self.active_app = app;
        self.surface = match app {
            AppId::Web => self
                .active_tab()
                .map(surface_for_tab)
                .unwrap_or_else(surface_for_web_home),
            AppId::Downloads => app_surface(
                "Downloads",
                "slate://downloads",
                "Download queue and saved broadweb files.",
                [
                    ("Active", "0", MetricAccent::Teal),
                    ("Pinned", "3", MetricAccent::Blue),
                    ("Verified", "On", MetricAccent::Amber),
                ],
            ),
            AppId::Calendar => app_surface(
                "Calendar",
                "slate://calendar",
                "Local-first calendar surface.",
                [
                    ("Today", "11", MetricAccent::Teal),
                    ("Events", "4", MetricAccent::Blue),
                    ("Private", "On", MetricAccent::Amber),
                ],
            ),
            AppId::Messaging => app_surface(
                "Messaging",
                "slate://messages",
                "Private messaging surface.",
                [
                    ("Inbox", "2", MetricAccent::Teal),
                    ("Muted", "1", MetricAccent::Amber),
                    ("Routes", "Tor", MetricAccent::Blue),
                ],
            ),
        };
    }

    pub fn add_mock_tab(&mut self) {
        let number = self.tabs.len().saturating_add(1);
        self.tabs.push(Tab {
            title: format!("Tab {number}"),
            address: "slate://new".to_string(),
        });
        let index = self.tabs.len().saturating_sub(1);
        let _ = self.activate_tab(index);
    }
}

fn app_for_address(address: &str) -> AppId {
    if address.starts_with("slate://calendar") {
        AppId::Calendar
    } else if address.starts_with("slate://downloads") {
        AppId::Downloads
    } else if address.starts_with("slate://messages") {
        AppId::Messaging
    } else {
        AppId::Web
    }
}

fn surface_for_tab(tab: &Tab) -> RenderSurface {
    if tab.address == "slate://home" || tab.address == "slate://new" {
        surface_for_web_home()
    } else {
        app_surface(
            &tab.title,
            &tab.address,
            "Servo boundary active; page rendering pending.",
            [
                ("Privacy", "On", MetricAccent::Teal),
                ("Trackers", "23", MetricAccent::Amber),
                ("Routes", "4", MetricAccent::Blue),
            ],
        )
    }
}

fn surface_for_web_home() -> RenderSurface {
    app_surface(
        "New Tab",
        "slate://home",
        "Servo boundary active; renderer embedding pending.",
        [
            ("Privacy First", "", MetricAccent::Teal),
            ("Tracker Blocked", "23", MetricAccent::Amber),
            ("Ads Blocked", "184", MetricAccent::Blue),
            ("Time Saved", "2h 14m", MetricAccent::Teal),
        ],
    )
}

fn app_surface<const N: usize>(
    title: &str,
    address: &str,
    summary: &str,
    metrics: [(&str, &str, MetricAccent); N],
) -> RenderSurface {
    RenderSurface {
        title: title.to_string(),
        address: address.to_string(),
        summary: summary.to_string(),
        metrics: metrics
            .into_iter()
            .map(|(label, value, accent)| RenderMetric {
                label: label.to_string(),
                value: value.to_string(),
                accent,
            })
            .collect(),
    }
}

#[cfg(test)]
mod tests {
    use super::BrowserState;
    use slate_rendering::ServoBackend;

    #[test]
    fn initial_state_has_home_tab() {
        let state = BrowserState::new(&ServoBackend);
        assert_eq!(
            state.active_tab().map(|tab| tab.address.as_str()),
            Some("slate://home")
        );
    }

    #[test]
    fn selecting_apps_changes_surface() {
        let mut state = BrowserState::new(&ServoBackend);
        state.select_app(slate_apps::AppId::Downloads);
        assert_eq!(state.surface.address, "slate://downloads");
    }

    #[test]
    fn adding_tab_activates_it() {
        let mut state = BrowserState::new(&ServoBackend);
        state.add_mock_tab();
        assert_eq!(state.active_tab, 3);
        assert_eq!(
            state.active_tab().map(|tab| tab.address.as_str()),
            Some("slate://new")
        );
    }
}
