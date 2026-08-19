#![forbid(unsafe_code)]

use core::fmt;
use slate_apps::AppId;
use slate_rendering::{
    RenderBackend, RenderDocument, RenderSurface, RenderViewport, ServoBackend, ServoDocumentStatus,
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
        self.select_app_with_surface_loader(app, surface_for_address);
    }

    fn select_app_with_surface_loader(
        &mut self,
        app: AppId,
        load_surface: impl FnOnce(&str, Option<&str>) -> RenderSurface,
    ) {
        self.active_app = app;
        self.surface = match app {
            AppId::Web => self
                .cached_active_tab_surface()
                .unwrap_or_else(surface_for_web_home),
            AppId::Downloads | AppId::Calendar | AppId::Chat => {
                load_surface(app_internal_address(app), None)
            }
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
    if let Some(address) = normalize_http_onion_url(trimmed) {
        return Ok(address);
    }

    if is_http_url_with_onion_hint(&lower) {
        return Ok(format!("slate://search?q={}", encode_query(trimmed)));
    }

    if has_supported_scheme(&lower) {
        return Ok(trimmed.to_string());
    }

    if let Some(address) = normalize_ipfs_path_address(trimmed) {
        return Ok(address);
    }

    if let Some(address) = normalize_bare_ipfs_cid(trimmed) {
        return Ok(address);
    }

    if let Some(address) = normalize_onion_address(trimmed) {
        return Ok(address);
    }

    if looks_like_local_html_path(&lower) {
        return Ok(local_file_address(trimmed));
    }

    if lower.contains("://") {
        return Ok(format!("slate://search?q={}", encode_query(trimmed)));
    }

    if looks_like_host(trimmed) {
        if lower.starts_with("localhost") || lower.contains(".i2p") {
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
        || lower.starts_with("tor+http://")
        || lower.starts_with("tor+https://")
        || lower.starts_with("ipfs://")
        || lower.starts_with("ipns://")
        || lower.starts_with("i2p://")
        || lower.starts_with("gemini://")
        || lower.starts_with("magnet:")
        || lower.starts_with("file://")
}

fn is_http_url_with_onion_hint(lower: &str) -> bool {
    (lower.starts_with("http://") || lower.starts_with("https://")) && lower.contains(".onion")
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

fn normalize_bare_ipfs_cid(input: &str) -> Option<String> {
    if is_cidv0_like(input) {
        return Some(format!("ipfs://{input}"));
    }

    let lower = input.to_ascii_lowercase();
    if is_cidv1_base32_like(&lower) && input.chars().all(|ch| ch.is_ascii_alphanumeric()) {
        return Some(format!("ipfs://{lower}"));
    }

    None
}

fn normalize_onion_address(input: &str) -> Option<String> {
    if input.chars().any(char::is_whitespace) {
        return None;
    }
    let name_end = input
        .find(|ch| matches!(ch, '/' | '?' | '#'))
        .unwrap_or(input.len());
    let name = &input[..name_end];
    if !is_onion_host(name) {
        return None;
    }

    let mut address = format!(
        "tor+http://{}",
        name.trim_end_matches('.').to_ascii_lowercase()
    );
    let rest = (name_end < input.len()).then_some(&input[name_end..]);
    match rest {
        Some(rest) if rest.starts_with('?') => {
            address.push('/');
            address.push_str(rest);
        }
        Some(rest) => address.push_str(rest),
        None => address.push('/'),
    }
    Some(address)
}

fn normalize_http_onion_url(input: &str) -> Option<String> {
    let lower = input.to_ascii_lowercase();
    let (source_scheme, rest) = if lower.starts_with("http://") {
        ("http", &input["http://".len()..])
    } else if lower.starts_with("https://") {
        ("https", &input["https://".len()..])
    } else {
        return None;
    };
    let authority_end = rest
        .find(|ch| matches!(ch, '/' | '?' | '#'))
        .unwrap_or(rest.len());
    let authority = &rest[..authority_end];
    if authority.is_empty() || authority.contains('@') {
        return None;
    }

    let (host, port) = authority
        .split_once(':')
        .map_or((authority, ""), |(host, port)| (host, port));
    if !is_onion_host(host) {
        return None;
    }
    if !port.is_empty() && !port.bytes().all(|byte| byte.is_ascii_digit()) {
        return None;
    }

    let mut address = format!(
        "tor+{source_scheme}://{}{}",
        host.trim_end_matches('.').to_ascii_lowercase(),
        if port.is_empty() {
            String::new()
        } else {
            format!(":{port}")
        }
    );
    let suffix = rest[authority_end..]
        .split_once('#')
        .map_or(&rest[authority_end..], |(without_fragment, _)| {
            without_fragment
        });
    match suffix {
        "" => address.push('/'),
        suffix if suffix.starts_with('?') => {
            address.push('/');
            address.push_str(suffix);
        }
        suffix => address.push_str(suffix),
    }
    Some(address)
}

fn is_onion_host(host: &str) -> bool {
    let host = host.trim_end_matches('.').to_ascii_lowercase();
    let Some(name) = host.strip_suffix(".onion") else {
        return false;
    };
    !name.is_empty()
        && name.split('.').all(|label| {
            !label.is_empty()
                && label
                    .bytes()
                    .all(|byte| byte.is_ascii_lowercase() || byte.is_ascii_digit() || byte == b'-')
        })
}

fn is_cidv0_like(input: &str) -> bool {
    input.len() == 46
        && input.starts_with("Qm")
        && input.bytes().all(|byte| {
            b"123456789ABCDEFGHJKLMNPQRSTUVWXYZabcdefghijkmnopqrstuvwxyz".contains(&byte)
        })
}

fn is_cidv1_base32_like(input: &str) -> bool {
    input.len() >= 32
        && matches!(input.get(..4), Some("bafy" | "bafk"))
        && input
            .bytes()
            .all(|byte| byte.is_ascii_lowercase() || matches!(byte, b'2'..=b'7'))
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
    } else if address.starts_with("slate://chat") || address.starts_with("slate://messages") {
        AppId::Chat
    } else {
        AppId::Web
    }
}

fn app_internal_address(app: AppId) -> &'static str {
    match app {
        AppId::Web => "slate://web",
        AppId::Downloads => "slate://downloads",
        AppId::Calendar => "slate://calendar",
        AppId::Chat => "slate://chat",
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

fn surface_for_web_home() -> RenderSurface {
    ServoBackend.load_home()
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
        state.select_app_with_surface_loader(slate_apps::AppId::Downloads, |address, _title| {
            cached_web_surface(address, 640, 360)
        });

        assert_eq!(state.active_app, slate_apps::AppId::Downloads);
        assert_eq!(state.surface.address, "slate://downloads");
        assert!(matches!(state.surface.document, RenderDocument::Web(_)));
    }

    #[test]
    fn selecting_web_reuses_cached_tab_surface() {
        let mut state = BrowserState::new(&ServoBackend);
        let cached_surface = cached_web_surface("https://cached.example", 640, 360);
        state.surface = cached_surface.clone();
        state.tabs[0].title = cached_surface.title.clone();
        state.tabs[0].address = cached_surface.address.clone();
        state.tabs[0].cached_surface = Some(cached_surface.clone());

        state.select_app_with_surface_loader(slate_apps::AppId::Downloads, |address, _title| {
            cached_web_surface(address, 640, 360)
        });
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
        assert_eq!(state.active_tab, 2);
        assert_eq!(
            state.active_tab().map(|tab| tab.address.as_str()),
            Some("slate://new")
        );
    }

    #[test]
    fn navigating_to_internal_app_pages_selects_their_rail_app() {
        let mut state = BrowserState::new(&ServoBackend);
        state
            .navigate_with_surface_loader(
                "slate://calendar",
                RenderViewport::default(),
                |address, _title, viewport| {
                    cached_web_surface(address, viewport.width as usize, viewport.height as usize)
                },
            )
            .expect("calendar navigation should load");

        assert_eq!(state.active_app, slate_apps::AppId::Calendar);
        assert_eq!(state.surface.address, "slate://calendar");
        assert!(matches!(state.surface.document, RenderDocument::Web(_)));

        let mut state = BrowserState::new(&ServoBackend);
        state
            .navigate_with_surface_loader(
                "slate://chat",
                RenderViewport::default(),
                |address, _title, viewport| {
                    cached_web_surface(address, viewport.width as usize, viewport.height as usize)
                },
            )
            .expect("chat navigation should load");

        assert_eq!(state.active_app, slate_apps::AppId::Chat);
        assert_eq!(state.surface.address, "slate://chat");
        assert!(matches!(state.surface.document, RenderDocument::Web(_)));
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
    fn navigation_normalizes_onion_hosts_to_tor_schemes() {
        assert_eq!(
            normalize_navigation_input("example.onion").expect("onion host"),
            "tor+http://example.onion/"
        );
        assert_eq!(
            normalize_navigation_input("example.onion/docs?a=1").expect("onion path"),
            "tor+http://example.onion/docs?a=1"
        );
        assert_eq!(
            normalize_navigation_input("http://Example.Onion:8080/docs#client")
                .expect("onion HTTP URL"),
            "tor+http://example.onion:8080/docs"
        );
        assert_eq!(
            normalize_navigation_input("https://example.onion/secure").expect("onion HTTPS URL"),
            "tor+https://example.onion/secure"
        );
        assert_eq!(
            normalize_navigation_input("http://user@example.onion/").expect("malformed onion URL"),
            "slate://search?q=http%3A%2F%2Fuser%40example.onion%2F"
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
    fn navigation_normalizes_bare_ipfs_cids() {
        assert_eq!(
            normalize_navigation_input("QmUKwop8CmB4ictvQyCJQru97NRVakJFVWpV74guJ89tcb")
                .expect("CIDv0 address"),
            "ipfs://QmUKwop8CmB4ictvQyCJQru97NRVakJFVWpV74guJ89tcb"
        );
        assert_eq!(
            normalize_navigation_input(
                "bafybeigdyrzt5sfp7udm7hu76uh7y26nf3efuylqabf3oclgtqy55fbzdi"
            )
            .expect("CIDv1 address"),
            "ipfs://bafybeigdyrzt5sfp7udm7hu76uh7y26nf3efuylqabf3oclgtqy55fbzdi"
        );
        assert_eq!(
            normalize_navigation_input(
                "BAFYBEIGDYRZT5SFP7UDM7HU76UH7Y26NF3EFUYLQABF3OCLGTQY55FBZDI"
            )
            .expect("uppercase CIDv1 address"),
            "ipfs://bafybeigdyrzt5sfp7udm7hu76uh7y26nf3efuylqabf3oclgtqy55fbzdi"
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
        assert_eq!(
            normalize_navigation_input("bafy short").expect("search"),
            "slate://search?q=bafy+short"
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
