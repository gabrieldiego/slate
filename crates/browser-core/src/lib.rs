#![forbid(unsafe_code)]

use core::fmt;
use slate_apps::AppId;
use slate_net::fetch_web_page;
use slate_rendering::{
    HtmlDocumentSource, MetricAccent, RenderBackend, RenderDocument, RenderMetric, RenderSurface,
    ServoBackend,
};
use std::path::{Path, PathBuf};

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

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum NavigationError {
    Empty,
}

impl fmt::Display for NavigationError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Empty => formatter.write_str("navigation input is empty"),
        }
    }
}

impl std::error::Error for NavigationError {}

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
        let Some(tab) = self.tabs.get(index).cloned() else {
            return false;
        };

        self.active_tab = index;
        self.open_tab_surface(&tab);
        true
    }

    pub fn select_app(&mut self, app: AppId) {
        self.active_app = app;
        self.surface = match app {
            AppId::Web => self
                .active_tab()
                .map(surface_for_tab)
                .unwrap_or_else(surface_for_web_home),
            AppId::Downloads => downloads_surface(),
            AppId::Calendar => calendar_surface(),
            AppId::Messaging => messaging_surface(),
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

    pub fn navigate(&mut self, input: &str) -> Result<(), NavigationError> {
        let address = normalize_navigation_input(input)?;
        let surface = surface_for_address(&address, None);
        let tab = Tab {
            title: surface.title.clone(),
            address: surface.address.clone(),
        };

        if let Some(active_tab) = self.tabs.get_mut(self.active_tab) {
            *active_tab = tab;
        } else {
            self.tabs.push(tab);
            self.active_tab = self.tabs.len().saturating_sub(1);
        }

        self.active_app = app_for_address(&surface.address);
        self.surface = surface;
        Ok(())
    }

    fn open_tab_surface(&mut self, tab: &Tab) {
        self.active_app = app_for_address(&tab.address);
        self.surface = surface_for_tab(tab);
    }
}

pub fn normalize_navigation_input(input: &str) -> Result<String, NavigationError> {
    let trimmed = input.trim();
    if trimmed.is_empty() {
        return Err(NavigationError::Empty);
    }

    let lower = trimmed.to_ascii_lowercase();
    if has_supported_scheme(&lower) {
        return Ok(trimmed.to_string());
    }

    if looks_like_local_html_path(&lower) {
        return Ok(local_file_address(trimmed));
    }

    if lower.contains("://") {
        return Ok(format!("slate://search?q={}", encode_query(trimmed)));
    }

    if looks_like_host(trimmed) {
        if lower.starts_with("localhost") || lower.contains(".onion") || lower.contains(".i2p") {
            Ok(format!("http://{trimmed}"))
        } else {
            Ok(format!("https://{trimmed}"))
        }
    } else {
        Ok(format!("slate://search?q={}", encode_query(trimmed)))
    }
}

fn has_supported_scheme(lower: &str) -> bool {
    lower.starts_with("slate://")
        || lower.starts_with("https://")
        || lower.starts_with("http://")
        || lower.starts_with("ipfs://")
        || lower.starts_with("ipns://")
        || lower.starts_with("i2p://")
        || lower.starts_with("file://")
}

fn looks_like_local_html_path(lower: &str) -> bool {
    lower.ends_with(".html") || lower.ends_with(".htm")
}

fn local_file_address(input: &str) -> String {
    let path = Path::new(input);
    let absolute = if path.is_absolute() {
        path.to_path_buf()
    } else {
        std::env::current_dir()
            .unwrap_or_else(|_| PathBuf::from("."))
            .join(path)
    };

    format!(
        "file://{}",
        percent_encode_file_path(&absolute.to_string_lossy())
    )
}

fn looks_like_host(input: &str) -> bool {
    !input.chars().any(char::is_whitespace)
        && (input.contains('.')
            || input.starts_with("localhost")
            || input.split(':').next().is_some_and(is_ipv4_like))
}

fn is_ipv4_like(input: &str) -> bool {
    let mut segment_count = 0;
    for segment in input.split('.') {
        if segment.is_empty() || !segment.chars().all(|ch| ch.is_ascii_digit()) {
            return false;
        }
        segment_count += 1;
    }
    segment_count == 4
}

fn encode_query(input: &str) -> String {
    const HEX: &[u8; 16] = b"0123456789ABCDEF";
    let mut output = String::new();

    for ch in input.chars() {
        if ch.is_ascii_alphanumeric() || matches!(ch, '-' | '_' | '.' | '~') {
            output.push(ch);
        } else if ch.is_whitespace() {
            output.push('+');
        } else {
            let mut buffer = [0_u8; 4];
            for byte in ch.encode_utf8(&mut buffer).bytes() {
                output.push('%');
                output.push(char::from(HEX[usize::from(byte / 16)]));
                output.push(char::from(HEX[usize::from(byte % 16)]));
            }
        }
    }

    output
}

fn percent_encode_file_path(input: &str) -> String {
    const HEX: &[u8; 16] = b"0123456789ABCDEF";
    let mut output = String::new();

    for byte in input.bytes() {
        if byte.is_ascii_alphanumeric() || matches!(byte, b'/' | b'.' | b'-' | b'_' | b'~') {
            output.push(char::from(byte));
        } else {
            output.push('%');
            output.push(char::from(HEX[usize::from(byte / 16)]));
            output.push(char::from(HEX[usize::from(byte % 16)]));
        }
    }

    output
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
    surface_for_address(&tab.address, Some(&tab.title))
}

fn surface_for_address(address: &str, fallback_title: Option<&str>) -> RenderSurface {
    match app_for_address(address) {
        AppId::Downloads => downloads_surface(),
        AppId::Calendar => calendar_surface(),
        AppId::Messaging => messaging_surface(),
        AppId::Web => {
            let backend = ServoBackend;
            let mut surface = if is_web_address(address) {
                web_surface(address)
            } else {
                backend.load_address(address)
            };
            if let Some(fallback_title) = fallback_title
                && surface.title.is_empty()
            {
                surface.title = fallback_title.to_string();
            }
            surface
        }
    }
}

fn is_web_address(address: &str) -> bool {
    address.starts_with("http://") || address.starts_with("https://")
}

fn web_surface(address: &str) -> RenderSurface {
    let backend = ServoBackend;
    match fetch_web_page(address) {
        Ok(page) => backend.render_html(&page.final_url, page.body, HtmlDocumentSource::WebFetch),
        Err(error) => backend.render_error(
            address,
            "Web Load Error",
            "Could not load web page",
            &[address.to_string(), error.to_string()],
            HtmlDocumentSource::WebFetch,
        ),
    }
}

fn surface_for_web_home() -> RenderSurface {
    ServoBackend.load_home()
}

fn downloads_surface() -> RenderSurface {
    app_surface(
        "Downloads",
        "slate://downloads",
        "Download queue and saved broadweb files.",
        [
            ("Active", "0", MetricAccent::Teal),
            ("Pinned", "3", MetricAccent::Blue),
            ("Verified", "On", MetricAccent::Amber),
        ],
    )
}

fn calendar_surface() -> RenderSurface {
    app_surface(
        "Calendar",
        "slate://calendar",
        "Local-first calendar surface.",
        [
            ("Today", "11", MetricAccent::Teal),
            ("Events", "4", MetricAccent::Blue),
            ("Private", "On", MetricAccent::Amber),
        ],
    )
}

fn messaging_surface() -> RenderSurface {
    app_surface(
        "Messaging",
        "slate://messages",
        "Private messaging surface.",
        [
            ("Inbox", "2", MetricAccent::Teal),
            ("Muted", "1", MetricAccent::Amber),
            ("Routes", "Tor", MetricAccent::Blue),
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
        document: RenderDocument::App,
    }
}

#[cfg(test)]
mod tests {
    use super::{BrowserState, normalize_navigation_input, percent_encode_file_path};
    use slate_rendering::{HtmlDocumentSource, RenderDocument, ServoBackend};
    use std::io::{Read, Write};
    use std::net::TcpListener;
    use std::thread;

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

    #[test]
    fn navigation_normalizes_bare_hosts() {
        assert_eq!(
            normalize_navigation_input("servo.org").expect("address"),
            "https://servo.org"
        );
        assert_eq!(
            normalize_navigation_input("localhost:8080").expect("address"),
            "http://localhost:8080"
        );
    }

    #[test]
    fn navigation_turns_search_terms_into_local_search() {
        assert_eq!(
            normalize_navigation_input("servo broadweb").expect("search"),
            "slate://search?q=servo+broadweb"
        );
    }

    #[test]
    fn navigation_normalizes_local_html_paths() {
        let path = std::env::temp_dir().join("slate local page.html");
        let path = path.to_string_lossy();
        assert_eq!(
            normalize_navigation_input(&path).expect("file address"),
            format!("file://{}", percent_encode_file_path(&path))
        );
        assert_eq!(
            normalize_navigation_input("file:///tmp/page.html").expect("file address"),
            "file:///tmp/page.html"
        );
    }

    #[test]
    fn navigating_updates_active_tab_and_loads_html_shim() {
        let mut state = BrowserState::new(&ServoBackend);
        state
            .navigate("slate://tests/hello")
            .expect("navigation should load");

        assert_eq!(state.surface.address, "slate://tests/hello");
        assert_eq!(
            state.active_tab().map(|tab| tab.title.as_str()),
            Some("Slate HTML Shim")
        );
        assert!(matches!(state.surface.document, RenderDocument::Html(_)));
    }

    #[test]
    fn navigating_http_fetches_and_renders_html() {
        let listener = TcpListener::bind("127.0.0.1:0").expect("bind local server");
        let address = format!("http://{}", listener.local_addr().expect("local socket"));
        let server = thread::spawn(move || {
            let (mut stream, _) = listener.accept().expect("accept request");
            let mut request = [0_u8; 1024];
            let _ = stream.read(&mut request).expect("read request");
            let body = "<!doctype html><html><head><title>Browser Core Web</title></head>\
                        <body><h1>Fetched From Web</h1><p>HTTP fixture body.</p></body></html>";
            let response = format!(
                "HTTP/1.1 200 OK\r\nContent-Type: text/html\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}",
                body.len(),
                body
            );
            stream
                .write_all(response.as_bytes())
                .expect("write response");
        });

        let mut state = BrowserState::new(&ServoBackend);
        state.navigate(&address).expect("HTTP navigation");
        server.join().expect("server thread");

        assert_eq!(state.surface.title, "Browser Core Web");
        assert_eq!(state.surface.summary, "HTTP fixture body.");
        let RenderDocument::Html(document) = &state.surface.document else {
            panic!("expected HTML document");
        };
        assert_eq!(document.heading, "Fetched From Web");
        assert_eq!(document.source, HtmlDocumentSource::WebFetch);
    }
}
