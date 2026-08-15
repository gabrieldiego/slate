#![forbid(unsafe_code)]

use core::fmt;
use slate_apps::AppId;
use slate_rendering::{
    MetricAccent, RenderBackend, RenderDocument, RenderMetric, RenderSurface, RenderViewport,
    ServoBackend, ServoDocumentStatus,
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
    pub cached_surface: Option<RenderSurface>,
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
                    cached_surface: Some(surface.clone()),
                },
                Tab {
                    title: "Research".to_string(),
                    address: "https://servo.org".to_string(),
                    cached_surface: None,
                },
                Tab {
                    title: "Calendar".to_string(),
                    address: "slate://calendar".to_string(),
                    cached_surface: None,
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
        if index >= self.tabs.len() {
            return false;
        }

        self.active_tab = index;
        self.open_active_tab_surface();
        true
    }

    pub fn select_app(&mut self, app: AppId) {
        self.active_app = app;
        self.surface = match app {
            AppId::Web => self
                .cached_active_tab_surface()
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
            cached_surface: None,
        });
        let index = self.tabs.len().saturating_sub(1);
        let _ = self.activate_tab(index);
    }

    pub fn navigate(&mut self, input: &str) -> Result<(), NavigationError> {
        self.navigate_with_surface_loader(
            input,
            RenderViewport::default(),
            surface_for_address_with_viewport,
        )
    }

    pub fn navigate_with_viewport(
        &mut self,
        input: &str,
        viewport: RenderViewport,
    ) -> Result<(), NavigationError> {
        self.navigate_with_surface_loader(input, viewport, surface_for_address_with_viewport)
    }

    fn navigate_with_surface_loader(
        &mut self,
        input: &str,
        viewport: RenderViewport,
        load_surface: impl FnOnce(&str, Option<&str>, RenderViewport) -> RenderSurface,
    ) -> Result<(), NavigationError> {
        let address = normalize_navigation_input(input)?;
        let surface = load_surface(&address, None, viewport);
        self.set_active_surface(surface);
        Ok(())
    }

    pub fn ensure_active_web_viewport(&mut self, viewport: RenderViewport) -> bool {
        self.refresh_active_web_viewport(viewport, surface_for_address_with_viewport)
    }

    fn refresh_active_web_viewport(
        &mut self,
        viewport: RenderViewport,
        load_surface: impl FnOnce(&str, Option<&str>, RenderViewport) -> RenderSurface,
    ) -> bool {
        if !self.active_web_viewport_needs_refresh(viewport) {
            return false;
        }

        let Some((address, title)) = self
            .active_tab()
            .map(|tab| (tab.address.clone(), tab.title.clone()))
        else {
            return false;
        };
        let surface = load_surface(&address, Some(&title), viewport);
        self.set_active_surface(surface);
        true
    }

    pub fn active_web_viewport_needs_refresh(&self, viewport: RenderViewport) -> bool {
        self.active_app == AppId::Web && self.active_surface_needs_viewport(viewport)
    }

    fn set_active_surface(&mut self, surface: RenderSurface) {
        let tab = Tab {
            title: surface.title.clone(),
            address: surface.address.clone(),
            cached_surface: Some(surface.clone()),
        };

        if let Some(active_tab) = self.tabs.get_mut(self.active_tab) {
            *active_tab = tab;
        } else {
            self.tabs.push(tab);
            self.active_tab = self.tabs.len().saturating_sub(1);
        }

        self.active_app = app_for_address(&surface.address);
        self.surface = surface;
    }

    fn open_active_tab_surface(&mut self) {
        let Some(surface) = self.cached_active_tab_surface() else {
            return;
        };

        self.active_app = app_for_address(&surface.address);
        self.surface = surface;
    }

    fn cached_active_tab_surface(&mut self) -> Option<RenderSurface> {
        let tab = self.tabs.get(self.active_tab)?;
        let surface = tab
            .cached_surface
            .clone()
            .unwrap_or_else(|| surface_for_tab(tab));

        if let Some(active_tab) = self.tabs.get_mut(self.active_tab) {
            active_tab.title = surface.title.clone();
            active_tab.address = surface.address.clone();
            active_tab.cached_surface = Some(surface.clone());
        }

        Some(surface)
    }

    fn active_surface_needs_viewport(&self, viewport: RenderViewport) -> bool {
        let RenderDocument::Web(document) = &self.surface.document else {
            return false;
        };
        if !matches!(document.status, ServoDocumentStatus::Rendered) {
            return false;
        }
        document.frame.width != viewport.width as usize
            || document.frame.height != viewport.height as usize
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

    if let Some(address) = normalize_ipfs_path_address(trimmed) {
        return Ok(address);
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
        || lower.starts_with("gemini://")
        || lower.starts_with("magnet:")
        || lower.starts_with("file://")
}

fn normalize_ipfs_path_address(input: &str) -> Option<String> {
    let path = input.strip_prefix('/').unwrap_or(input);
    let lower = path.to_ascii_lowercase();
    let (scheme, rest) = if lower.starts_with("ipfs/") {
        ("ipfs", &path["ipfs/".len()..])
    } else if lower.starts_with("ipns/") {
        ("ipns", &path["ipns/".len()..])
    } else {
        return None;
    };

    if rest.is_empty() || rest.chars().any(char::is_whitespace) {
        return None;
    }

    let name_end = rest
        .find(|ch| matches!(ch, '/' | '?' | '#'))
        .unwrap_or(rest.len());
    if name_end == 0 {
        return None;
    }

    Some(format!("{scheme}://{rest}"))
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
    surface_for_address_with_viewport(address, fallback_title, RenderViewport::default())
}

fn surface_for_address_with_viewport(
    address: &str,
    fallback_title: Option<&str>,
    viewport: RenderViewport,
) -> RenderSurface {
    match app_for_address(address) {
        AppId::Downloads => downloads_surface(),
        AppId::Calendar => calendar_surface(),
        AppId::Messaging => messaging_surface(),
        AppId::Web => {
            let backend = ServoBackend;
            let mut surface = backend.load_address_with_viewport(address, viewport);
            if address == "slate://new" {
                surface.address = address.to_string();
            }
            if let Some(fallback_title) = fallback_title
                && surface.title.is_empty()
            {
                surface.title = fallback_title.to_string();
            }
            surface
        }
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
    use slate_rendering::{
        RenderDocument, RenderSurface, RenderViewport, ServoBackend, ServoDocument,
        ServoDocumentSource, ServoDocumentStatus, ServoFrame,
    };
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
    fn selecting_web_reuses_cached_tab_surface() {
        let mut state = BrowserState::new(&ServoBackend);
        let cached_surface = cached_web_surface("https://cached.example", 640, 360);
        state.surface = cached_surface.clone();
        state.tabs[0].title = cached_surface.title.clone();
        state.tabs[0].address = cached_surface.address.clone();
        state.tabs[0].cached_surface = Some(cached_surface.clone());

        state.select_app(slate_apps::AppId::Downloads);
        state.select_app(slate_apps::AppId::Web);

        assert_eq!(state.surface, cached_surface);
    }

    #[test]
    fn activating_tab_reuses_cached_surface() {
        let mut state = BrowserState::new(&ServoBackend);
        let cached_surface = cached_web_surface("https://research.example", 800, 500);
        state.tabs[1].title = cached_surface.title.clone();
        state.tabs[1].address = cached_surface.address.clone();
        state.tabs[1].cached_surface = Some(cached_surface.clone());

        assert!(state.activate_tab(1));

        assert_eq!(state.surface, cached_surface);
    }

    #[test]
    fn viewport_refresh_updates_cached_tab_surface() {
        let mut state = BrowserState::new(&ServoBackend);
        let cached_surface = cached_web_surface("slate://tests/cache-refresh", 320, 240);
        state.surface = cached_surface.clone();
        state.tabs[0].title = cached_surface.title.clone();
        state.tabs[0].address = cached_surface.address.clone();
        state.tabs[0].cached_surface = Some(cached_surface);

        assert!(state.refresh_active_web_viewport(
            RenderViewport::new(640, 360),
            |address, title, viewport| {
                assert_eq!(address, "slate://tests/cache-refresh");
                assert_eq!(title, Some("Cached Surface"));
                cached_web_surface(address, viewport.width as usize, viewport.height as usize)
            }
        ));

        assert_eq!(state.tabs[0].cached_surface.as_ref(), Some(&state.surface));
        assert!(!state.active_web_viewport_needs_refresh(RenderViewport::new(640, 360)));
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
    fn navigation_normalizes_ipfs_and_ipns_paths() {
        assert_eq!(
            normalize_navigation_input("/ipfs/bafybeigdyrzt/index.html?view=1#top")
                .expect("IPFS address"),
            "ipfs://bafybeigdyrzt/index.html?view=1#top"
        );
        assert_eq!(
            normalize_navigation_input("ipns/example.net/docs").expect("IPNS address"),
            "ipns://example.net/docs"
        );
    }

    #[test]
    fn navigation_searches_incomplete_ipfs_paths() {
        assert_eq!(
            normalize_navigation_input("/ipfs/").expect("search"),
            "slate://search?q=%2Fipfs%2F"
        );
        assert_eq!(
            normalize_navigation_input("ipns/example docs").expect("search"),
            "slate://search?q=ipns%2Fexample+docs"
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
    fn navigating_updates_active_tab_from_loaded_surface() {
        let mut state = BrowserState::new(&ServoBackend);
        state
            .navigate_with_surface_loader(
                "slate://tests/hello",
                RenderViewport::default(),
                |address, _title, viewport| {
                    let mut surface = cached_web_surface(
                        address,
                        viewport.width as usize,
                        viewport.height as usize,
                    );
                    surface.title = "Slate Servo Test".to_string();
                    surface
                },
            )
            .expect("navigation should load");

        assert_eq!(state.surface.address, "slate://tests/hello");
        assert_eq!(
            state.active_tab().map(|tab| tab.title.as_str()),
            Some("Slate Servo Test")
        );
        assert!(matches!(state.surface.document, RenderDocument::Web(_)));
    }

    #[test]
    fn navigating_http_hands_page_to_servo() {
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
        assert_eq!(state.surface.summary, "Rendered by Servo");
        let RenderDocument::Web(document) = &state.surface.document else {
            panic!("expected Servo document");
        };
        assert_eq!(document.source, ServoDocumentSource::Web);
        assert!(!document.frame.pixels.is_empty());
    }

    fn cached_web_surface(address: &str, width: usize, height: usize) -> RenderSurface {
        RenderSurface {
            title: "Cached Surface".to_string(),
            address: address.to_string(),
            summary: "Rendered by Servo".to_string(),
            metrics: Vec::new(),
            document: RenderDocument::Web(ServoDocument {
                title: "Cached Surface".to_string(),
                address: address.to_string(),
                frame: ServoFrame {
                    width,
                    height,
                    pixels: vec![0x00EAF4F2; width.saturating_mul(height)],
                },
                source: ServoDocumentSource::SlateGenerated,
                status: ServoDocumentStatus::Rendered,
            }),
        }
    }
}
