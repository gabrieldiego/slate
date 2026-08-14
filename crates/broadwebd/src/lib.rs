#![forbid(unsafe_code)]

use core::fmt;
use std::collections::BTreeMap;
use std::fs;
use std::path::{Path, PathBuf};
use std::time::Duration;
use url::Url;

const DEFAULT_PROFILE: &str = "default";
pub const DIRECT_HTTP_PLUGIN: &str = "direct-http";
pub const HTTP_FETCH_PLUGIN: &str = "http-fetch";
pub const IPFS_GATEWAY_PLUGIN: &str = "ipfs-gateway";
pub const DEFAULT_IPFS_GATEWAY: &str = "http://127.0.0.1:8080";
const USER_AGENT: &str = "Slate/0.0.1";
const REQUEST_TIMEOUT: Duration = Duration::from_secs(12);
const DEFAULT_MAX_HTTP_RESPONSE_BYTES: usize = 8 * 1024 * 1024;

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum BroadwebdError {
    Io(String),
    InvalidProfile(String),
    InvalidUrl(String),
    MissingPlugin(String),
    Request(String),
    ResponseTooLarge { limit: usize, actual: usize },
    UnsupportedRequest(String),
}

impl fmt::Display for BroadwebdError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Io(error) => formatter.write_str(error),
            Self::InvalidProfile(profile) => write!(formatter, "invalid profile id: {profile}"),
            Self::InvalidUrl(url) => write!(formatter, "invalid URL: {url}"),
            Self::MissingPlugin(plugin) => write!(formatter, "missing broadwebd plugin: {plugin}"),
            Self::Request(error) => formatter.write_str(error),
            Self::ResponseTooLarge { limit, actual } => {
                write!(
                    formatter,
                    "response too large: {actual} bytes over {limit} byte limit"
                )
            }
            Self::UnsupportedRequest(request) => {
                write!(formatter, "unsupported broadwebd request: {request}")
            }
        }
    }
}

impl std::error::Error for BroadwebdError {}

impl From<std::io::Error> for BroadwebdError {
    fn from(error: std::io::Error) -> Self {
        Self::Io(error.to_string())
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct StateRoot {
    path: PathBuf,
}

impl StateRoot {
    pub fn prepare(path: impl Into<PathBuf>) -> Result<Self, BroadwebdError> {
        let path = path.into();
        fs::create_dir_all(path.join("profiles"))?;
        fs::create_dir_all(path.join("volatile"))?;
        Ok(Self { path })
    }

    pub fn path(&self) -> &Path {
        &self.path
    }

    pub fn profile_root(&self, profile: &str) -> Result<PathBuf, BroadwebdError> {
        validate_profile_id(profile)?;
        Ok(self.path.join("profiles").join(profile))
    }

    pub fn prepare_profile(&self, profile: &str) -> Result<PathBuf, BroadwebdError> {
        let root = self.profile_root(profile)?;
        fs::create_dir_all(root.join("protocol-state"))?;
        fs::create_dir_all(root.join("temporary"))?;
        Ok(root)
    }
}

fn validate_profile_id(profile: &str) -> Result<(), BroadwebdError> {
    if !profile.is_empty()
        && profile
            .chars()
            .all(|ch| ch.is_ascii_alphanumeric() || matches!(ch, '-' | '_'))
    {
        return Ok(());
    }

    Err(BroadwebdError::InvalidProfile(profile.to_string()))
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ResourceBudget {
    pub max_idle_memory_bytes: usize,
    pub max_cache_size_per_profile_bytes: u64,
    pub max_peer_connections: usize,
    pub max_protocol_workers: usize,
    pub max_background_bandwidth_bytes_per_second: Option<u64>,
    pub allow_metered_network: bool,
    pub allow_background_on_battery: bool,
    pub allow_inbound_connections: bool,
    pub allow_reprovide: bool,
    pub allow_public_gateway_fallback: bool,
    pub max_http_response_bytes: usize,
}

impl Default for ResourceBudget {
    fn default() -> Self {
        Self {
            max_idle_memory_bytes: 128 * 1024 * 1024,
            max_cache_size_per_profile_bytes: 512 * 1024 * 1024,
            max_peer_connections: 64,
            max_protocol_workers: 4,
            max_background_bandwidth_bytes_per_second: None,
            allow_metered_network: false,
            allow_background_on_battery: false,
            allow_inbound_connections: false,
            allow_reprovide: false,
            allow_public_gateway_fallback: false,
            max_http_response_bytes: DEFAULT_MAX_HTTP_RESPONSE_BYTES,
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum DaemonLifecycle {
    Stopped,
    Starting,
    Ready,
    Degraded,
    Stopping,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum PluginKind {
    Transport,
    ApplicationService,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ResourceProfile {
    Low,
    Medium,
    High,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct PluginMetadata {
    pub id: String,
    pub kind: PluginKind,
    pub capabilities: Vec<String>,
    pub dependencies: Vec<String>,
    pub privacy_boundary: String,
    pub resource_profile: ResourceProfile,
}

impl PluginMetadata {
    pub fn new(id: impl Into<String>, kind: PluginKind) -> Self {
        Self {
            id: id.into(),
            kind,
            capabilities: Vec::new(),
            dependencies: Vec::new(),
            privacy_boundary: String::new(),
            resource_profile: ResourceProfile::Low,
        }
    }

    pub fn with_capabilities(mut self, capabilities: &[&str]) -> Self {
        self.capabilities = capabilities
            .iter()
            .map(|capability| (*capability).to_string())
            .collect();
        self
    }

    pub fn with_dependencies(mut self, dependencies: &[&str]) -> Self {
        self.dependencies = dependencies
            .iter()
            .map(|dependency| (*dependency).to_string())
            .collect();
        self
    }

    pub fn with_privacy_boundary(mut self, privacy_boundary: impl Into<String>) -> Self {
        self.privacy_boundary = privacy_boundary.into();
        self
    }

    pub fn with_resource_profile(mut self, resource_profile: ResourceProfile) -> Self {
        self.resource_profile = resource_profile;
        self
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum PluginHealth {
    Ready,
    Degraded(String),
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct PluginStatus {
    pub metadata: PluginMetadata,
    pub health: PluginHealth,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct DaemonHealth {
    pub lifecycle: DaemonLifecycle,
    pub plugins: Vec<PluginStatus>,
}

pub trait TransportPlugin: Send + Sync {
    fn metadata(&self) -> PluginMetadata;

    fn fetch_http(
        &self,
        request: &TransportHttpRequest,
        budget: &ResourceBudget,
    ) -> Result<HttpFetchResponse, BroadwebdError>;
}

pub trait ApplicationServicePlugin: Send + Sync {
    fn metadata(&self) -> PluginMetadata;

    fn call(
        &self,
        request: ServiceRequest,
        registry: &PluginRegistry,
        budget: &ResourceBudget,
    ) -> Result<ServiceResponse, BroadwebdError>;
}

pub struct PluginRegistry {
    transports: BTreeMap<String, Box<dyn TransportPlugin>>,
    services: BTreeMap<String, Box<dyn ApplicationServicePlugin>>,
}

impl PluginRegistry {
    pub fn new() -> Self {
        Self {
            transports: BTreeMap::new(),
            services: BTreeMap::new(),
        }
    }

    pub fn with_default_http() -> Self {
        let mut registry = Self::new();
        registry.register_transport(DirectHttpTransport);
        registry.register_transport(IpfsGatewayTransport::default());
        registry.register_service(HttpFetchService);
        registry
    }

    pub fn register_transport(&mut self, plugin: impl TransportPlugin + 'static) {
        let metadata = plugin.metadata();
        self.transports.insert(metadata.id, Box::new(plugin));
    }

    pub fn register_service(&mut self, plugin: impl ApplicationServicePlugin + 'static) {
        let metadata = plugin.metadata();
        self.services.insert(metadata.id, Box::new(plugin));
    }

    pub fn list_plugins(&self) -> Vec<PluginMetadata> {
        self.transports
            .values()
            .map(|plugin| plugin.metadata())
            .chain(self.services.values().map(|plugin| plugin.metadata()))
            .collect()
    }

    pub fn list_transports(&self) -> Vec<PluginMetadata> {
        self.transports
            .values()
            .map(|plugin| plugin.metadata())
            .collect()
    }

    pub fn list_application_services(&self) -> Vec<PluginMetadata> {
        self.services
            .values()
            .map(|plugin| plugin.metadata())
            .collect()
    }

    pub fn plugin_statuses(&self) -> Vec<PluginStatus> {
        self.list_plugins()
            .into_iter()
            .map(|metadata| {
                let missing: Vec<String> = metadata
                    .dependencies
                    .iter()
                    .filter(|dependency| !self.has_plugin(dependency))
                    .cloned()
                    .collect();
                let health = if missing.is_empty() {
                    PluginHealth::Ready
                } else {
                    PluginHealth::Degraded(format!("missing dependencies: {}", missing.join(", ")))
                };
                PluginStatus { metadata, health }
            })
            .collect()
    }

    pub fn fetch_http(
        &self,
        request: HttpFetchRequest,
        budget: &ResourceBudget,
    ) -> Result<HttpFetchResponse, BroadwebdError> {
        let service = self.service(HTTP_FETCH_PLUGIN)?;
        match service.call(ServiceRequest::HttpFetch(request), self, budget)? {
            ServiceResponse::HttpFetch(response) => Ok(response),
        }
    }

    fn transport(&self, id: &str) -> Result<&dyn TransportPlugin, BroadwebdError> {
        self.transports
            .get(id)
            .map(|plugin| plugin.as_ref())
            .ok_or_else(|| BroadwebdError::MissingPlugin(id.to_string()))
    }

    fn service(&self, id: &str) -> Result<&dyn ApplicationServicePlugin, BroadwebdError> {
        self.services
            .get(id)
            .map(|plugin| plugin.as_ref())
            .ok_or_else(|| BroadwebdError::MissingPlugin(id.to_string()))
    }

    fn has_plugin(&self, id: &str) -> bool {
        self.transports.contains_key(id) || self.services.contains_key(id)
    }
}

impl Default for PluginRegistry {
    fn default() -> Self {
        Self::with_default_http()
    }
}

pub struct BroadwebDaemon {
    state_root: StateRoot,
    budget: ResourceBudget,
    registry: PluginRegistry,
    lifecycle: DaemonLifecycle,
}

impl BroadwebDaemon {
    pub fn start(state_root: impl Into<PathBuf>) -> Result<Self, BroadwebdError> {
        Self::start_with_registry(
            state_root,
            ResourceBudget::default(),
            PluginRegistry::with_default_http(),
        )
    }

    pub fn start_with_registry(
        state_root: impl Into<PathBuf>,
        budget: ResourceBudget,
        registry: PluginRegistry,
    ) -> Result<Self, BroadwebdError> {
        let state_root = StateRoot::prepare(state_root)?;
        state_root.prepare_profile(DEFAULT_PROFILE)?;
        Ok(Self {
            state_root,
            budget,
            registry,
            lifecycle: DaemonLifecycle::Ready,
        })
    }

    pub fn start_default_session() -> Result<Self, BroadwebdError> {
        Self::start(default_session_state_root())
    }

    pub fn health(&self) -> DaemonHealth {
        DaemonHealth {
            lifecycle: self.lifecycle,
            plugins: self.registry.plugin_statuses(),
        }
    }

    pub fn state_root(&self) -> &StateRoot {
        &self.state_root
    }

    pub fn registry(&self) -> &PluginRegistry {
        &self.registry
    }

    pub fn budget(&self) -> &ResourceBudget {
        &self.budget
    }

    pub fn fetch_http(
        &self,
        request: HttpFetchRequest,
    ) -> Result<HttpFetchResponse, BroadwebdError> {
        self.state_root.prepare_profile(&request.profile)?;
        self.registry.fetch_http(request, &self.budget)
    }
}

pub fn default_session_state_root() -> PathBuf {
    std::env::temp_dir()
        .join("slate-broadwebd")
        .join(format!("process-{}", std::process::id()))
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct HttpFetchRequest {
    pub profile: String,
    pub url: String,
    pub transport_id: String,
}

impl HttpFetchRequest {
    pub fn new(profile: impl Into<String>, url: impl Into<String>) -> Self {
        Self {
            profile: profile.into(),
            url: url.into(),
            transport_id: DIRECT_HTTP_PLUGIN.to_string(),
        }
    }

    pub fn default_profile(url: impl Into<String>) -> Self {
        Self::new(DEFAULT_PROFILE, url)
    }

    pub fn through_transport(mut self, transport_id: impl Into<String>) -> Self {
        self.transport_id = transport_id.into();
        self
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct TransportHttpRequest {
    pub profile: String,
    pub url: String,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct HttpHeader {
    pub name: String,
    pub value: String,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum FetchDisposition {
    RenderHtml,
    Download { suggested_filename: String },
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct HttpFetchResponse {
    pub final_url: String,
    pub status_code: u16,
    pub content_type: Option<String>,
    pub headers: Vec<HttpHeader>,
    pub body: Vec<u8>,
    pub disposition: FetchDisposition,
}

impl HttpFetchResponse {
    pub fn new(
        final_url: impl Into<String>,
        status_code: u16,
        content_type: Option<String>,
        headers: Vec<HttpHeader>,
        body: Vec<u8>,
    ) -> Self {
        let final_url = final_url.into();
        let disposition = response_disposition(&final_url, content_type.as_deref());
        Self {
            final_url,
            status_code,
            content_type,
            headers,
            body,
            disposition,
        }
    }

    pub fn body_text_lossy(&self) -> String {
        String::from_utf8_lossy(&self.body).into_owned()
    }

    pub fn is_html(&self) -> bool {
        matches!(self.disposition, FetchDisposition::RenderHtml)
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum ServiceRequest {
    HttpFetch(HttpFetchRequest),
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum ServiceResponse {
    HttpFetch(HttpFetchResponse),
}

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct DirectHttpTransport;

impl TransportPlugin for DirectHttpTransport {
    fn metadata(&self) -> PluginMetadata {
        PluginMetadata::new(DIRECT_HTTP_PLUGIN, PluginKind::Transport)
            .with_capabilities(&["http", "https", "http-fetch"])
            .with_privacy_boundary("ordinary direct HTTP(S); uses normal DNS and network routing")
            .with_resource_profile(ResourceProfile::Low)
    }

    fn fetch_http(
        &self,
        request: &TransportHttpRequest,
        budget: &ResourceBudget,
    ) -> Result<HttpFetchResponse, BroadwebdError> {
        let url = parse_http_url(&request.url)?;
        if !matches!(url.scheme(), "http" | "https") {
            return Err(BroadwebdError::UnsupportedRequest(format!(
                "{} cannot fetch {}",
                DIRECT_HTTP_PLUGIN, request.url
            )));
        }

        fetch_http_url(url, budget)
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct IpfsGatewayTransport {
    gateway_base: String,
}

impl IpfsGatewayTransport {
    pub fn new(gateway_base: impl Into<String>) -> Self {
        Self {
            gateway_base: gateway_base.into(),
        }
    }

    pub fn gateway_base(&self) -> &str {
        &self.gateway_base
    }
}

impl Default for IpfsGatewayTransport {
    fn default() -> Self {
        Self::new(DEFAULT_IPFS_GATEWAY)
    }
}

impl TransportPlugin for IpfsGatewayTransport {
    fn metadata(&self) -> PluginMetadata {
        PluginMetadata::new(IPFS_GATEWAY_PLUGIN, PluginKind::Transport)
            .with_capabilities(&["ipfs", "ipns", "http-fetch"])
            .with_privacy_boundary(
                "local IPFS gateway over HTTP; no public gateway fallback by default",
            )
            .with_resource_profile(ResourceProfile::Medium)
    }

    fn fetch_http(
        &self,
        request: &TransportHttpRequest,
        budget: &ResourceBudget,
    ) -> Result<HttpFetchResponse, BroadwebdError> {
        let gateway_url = ipfs_gateway_http_url(&request.url, &self.gateway_base)?;
        let url = parse_http_url(&gateway_url)?;
        fetch_http_url(url, budget)
    }
}

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct HttpFetchService;

impl ApplicationServicePlugin for HttpFetchService {
    fn metadata(&self) -> PluginMetadata {
        PluginMetadata::new(HTTP_FETCH_PLUGIN, PluginKind::ApplicationService)
            .with_capabilities(&[
                "application/http-response",
                "html-render-boundary",
                "download-boundary",
            ])
            .with_dependencies(&[DIRECT_HTTP_PLUGIN])
            .with_privacy_boundary(
                "uses an approved transport plugin to produce HTTP-like responses",
            )
            .with_resource_profile(ResourceProfile::Low)
    }

    fn call(
        &self,
        request: ServiceRequest,
        registry: &PluginRegistry,
        budget: &ResourceBudget,
    ) -> Result<ServiceResponse, BroadwebdError> {
        match request {
            ServiceRequest::HttpFetch(request) => {
                let transport = registry.transport(&request.transport_id)?;
                let transport_request = TransportHttpRequest {
                    profile: request.profile,
                    url: request.url,
                };
                transport
                    .fetch_http(&transport_request, budget)
                    .map(ServiceResponse::HttpFetch)
            }
        }
    }
}

pub fn ipfs_gateway_http_url(source: &str, gateway_base: &str) -> Result<String, BroadwebdError> {
    let parsed =
        Url::parse(source).map_err(|error| BroadwebdError::InvalidUrl(error.to_string()))?;
    let namespace = match parsed.scheme() {
        "ipfs" => "ipfs",
        "ipns" => "ipns",
        scheme => {
            return Err(BroadwebdError::UnsupportedRequest(format!(
                "unsupported IPFS gateway scheme: {scheme}"
            )));
        }
    };
    let name = parsed
        .host_str()
        .ok_or_else(|| BroadwebdError::InvalidUrl(format!("{source} is missing a content name")))?;
    let mut output = format!(
        "{}/{}/{}",
        gateway_base.trim_end_matches('/'),
        namespace,
        name
    );
    if parsed.path() != "/" {
        output.push_str(parsed.path());
    }
    if let Some(query) = parsed.query() {
        output.push('?');
        output.push_str(query);
    }
    Ok(output)
}

fn request_error(error: reqwest::Error) -> BroadwebdError {
    BroadwebdError::Request(error.to_string())
}

fn parse_http_url(input: &str) -> Result<Url, BroadwebdError> {
    Url::parse(input).map_err(|error| BroadwebdError::InvalidUrl(error.to_string()))
}

fn fetch_http_url(url: Url, budget: &ResourceBudget) -> Result<HttpFetchResponse, BroadwebdError> {
    let client = reqwest::blocking::Client::builder()
        .timeout(REQUEST_TIMEOUT)
        .user_agent(USER_AGENT)
        .redirect(reqwest::redirect::Policy::limited(6))
        .build()
        .map_err(request_error)?;
    let response = client.get(url).send().map_err(request_error)?;
    let final_url = response.url().to_string();
    let status_code = response.status().as_u16();
    let content_type = header_value(response.headers(), reqwest::header::CONTENT_TYPE);
    let headers = response_headers(response.headers());
    let body = response.bytes().map_err(request_error)?.to_vec();
    if body.len() > budget.max_http_response_bytes {
        return Err(BroadwebdError::ResponseTooLarge {
            limit: budget.max_http_response_bytes,
            actual: body.len(),
        });
    }

    Ok(HttpFetchResponse::new(
        final_url,
        status_code,
        content_type,
        headers,
        body,
    ))
}

fn header_value(
    headers: &reqwest::header::HeaderMap,
    name: reqwest::header::HeaderName,
) -> Option<String> {
    headers
        .get(name)
        .and_then(|value| value.to_str().ok())
        .map(str::to_string)
}

fn response_headers(headers: &reqwest::header::HeaderMap) -> Vec<HttpHeader> {
    headers
        .iter()
        .filter_map(|(name, value)| {
            Some(HttpHeader {
                name: name.as_str().to_string(),
                value: value.to_str().ok()?.to_string(),
            })
        })
        .collect()
}

fn response_disposition(final_url: &str, content_type: Option<&str>) -> FetchDisposition {
    if is_html_content_type(content_type) {
        return FetchDisposition::RenderHtml;
    }

    FetchDisposition::Download {
        suggested_filename: suggested_filename(final_url),
    }
}

fn is_html_content_type(content_type: Option<&str>) -> bool {
    let Some(content_type) = content_type else {
        return false;
    };
    let media_type = content_type
        .split(';')
        .next()
        .unwrap_or_default()
        .trim()
        .to_ascii_lowercase();
    matches!(media_type.as_str(), "text/html" | "application/xhtml+xml")
}

fn suggested_filename(final_url: &str) -> String {
    Url::parse(final_url)
        .ok()
        .and_then(|url| {
            url.path_segments()
                .and_then(|mut segments| segments.next_back().map(str::to_string))
        })
        .filter(|segment| !segment.is_empty())
        .unwrap_or_else(|| "download".to_string())
}

#[cfg(test)]
mod tests {
    use super::{
        BroadwebDaemon, BroadwebdError, FetchDisposition, HttpFetchRequest, PluginHealth,
        PluginRegistry, ResourceBudget, StateRoot, TransportHttpRequest, TransportPlugin,
        ipfs_gateway_http_url,
    };
    use std::fs;
    use std::io::{Read, Write};
    use std::net::TcpListener;
    use std::path::PathBuf;
    use std::thread;

    #[test]
    fn state_root_prepares_profile_directories() {
        let root = test_state_root("state-root");
        let state = StateRoot::prepare(&root).expect("prepare state root");
        let profile_root = state.prepare_profile("default").expect("prepare profile");

        assert!(profile_root.join("protocol-state").is_dir());
        assert!(profile_root.join("temporary").is_dir());
        assert!(state.prepare_profile("../escape").is_err());

        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn default_registry_reports_ready_http_plugins() {
        let daemon = BroadwebDaemon::start(test_state_root("ready-plugins")).expect("daemon");
        let health = daemon.health();

        assert!(health.plugins.iter().any(|status| {
            status.metadata.id == "direct-http" && matches!(status.health, PluginHealth::Ready)
        }));
        assert!(health.plugins.iter().any(|status| {
            status.metadata.id == "http-fetch" && matches!(status.health, PluginHealth::Ready)
        }));

        let _ = fs::remove_dir_all(daemon.state_root().path());
    }

    #[test]
    fn registry_reports_missing_dependencies_as_degraded() {
        let mut registry = PluginRegistry::new();
        registry.register_service(super::HttpFetchService);

        let statuses = registry.plugin_statuses();
        assert!(statuses.iter().any(|status| {
            status.metadata.id == "http-fetch" && matches!(status.health, PluginHealth::Degraded(_))
        }));
    }

    #[test]
    fn http_fetch_uses_direct_http_transport() {
        let (address, server) = local_http_fixture(
            "text/html; charset=utf-8",
            "<!doctype html><title>Broadwebd Fixture</title><h1>Fetched</h1>",
        );
        let daemon = BroadwebDaemon::start(test_state_root("http-fetch")).expect("daemon");
        let response = daemon
            .fetch_http(HttpFetchRequest::default_profile(&address))
            .expect("fetch fixture");
        server.join().expect("server");

        assert_eq!(response.status_code, 200);
        assert_eq!(response.disposition, FetchDisposition::RenderHtml);
        assert!(response.body_text_lossy().contains("Broadwebd Fixture"));

        let _ = fs::remove_dir_all(daemon.state_root().path());
    }

    #[test]
    fn http_fetch_can_use_ipfs_gateway_transport_for_html() {
        let (gateway, server) = local_http_fixture(
            "text/html; charset=utf-8",
            "<!doctype html><title>IPFS Fixture</title><h1>Fetched From IPFS</h1>",
        );
        let mut registry = PluginRegistry::new();
        registry.register_transport(super::IpfsGatewayTransport::new(&gateway));
        registry.register_service(super::HttpFetchService);
        let daemon = BroadwebDaemon::start_with_registry(
            test_state_root("ipfs-html"),
            Default::default(),
            registry,
        )
        .expect("daemon");
        let response = daemon
            .fetch_http(
                HttpFetchRequest::default_profile("ipfs://bafybeigdyrzt/index.html")
                    .through_transport("ipfs-gateway"),
            )
            .expect("fetch IPFS fixture");
        server.join().expect("server");

        assert_eq!(response.disposition, FetchDisposition::RenderHtml);
        assert!(response.body_text_lossy().contains("IPFS Fixture"));

        let _ = fs::remove_dir_all(daemon.state_root().path());
    }

    #[test]
    fn http_fetch_can_use_ipfs_gateway_transport_for_downloads() {
        let (gateway, server) = local_http_fixture("image/png", "png-ish");
        let mut registry = PluginRegistry::new();
        registry.register_transport(super::IpfsGatewayTransport::new(&gateway));
        registry.register_service(super::HttpFetchService);
        let daemon = BroadwebDaemon::start_with_registry(
            test_state_root("ipfs-download"),
            Default::default(),
            registry,
        )
        .expect("daemon");
        let response = daemon
            .fetch_http(
                HttpFetchRequest::default_profile("ipfs://bafybeigdyrzt/image.png")
                    .through_transport("ipfs-gateway"),
            )
            .expect("fetch IPFS fixture");
        server.join().expect("server");

        assert_eq!(
            response.disposition,
            FetchDisposition::Download {
                suggested_filename: "image.png".to_string()
            }
        );

        let _ = fs::remove_dir_all(daemon.state_root().path());
    }

    #[test]
    fn non_html_http_fetch_is_marked_as_download() {
        let (address, server) = local_http_fixture("application/octet-stream", "binary-ish");
        let daemon = BroadwebDaemon::start(test_state_root("download")).expect("daemon");
        let response = daemon
            .fetch_http(HttpFetchRequest::default_profile(&address))
            .expect("fetch fixture");
        server.join().expect("server");

        assert_eq!(
            response.disposition,
            FetchDisposition::Download {
                suggested_filename: "download".to_string()
            }
        );

        let _ = fs::remove_dir_all(daemon.state_root().path());
    }

    #[test]
    fn response_size_budget_is_enforced() {
        let (address, server) = local_http_fixture("text/html", "0123456789");
        let mut registry = PluginRegistry::new();
        registry.register_transport(super::DirectHttpTransport);
        registry.register_service(super::HttpFetchService);
        let budget = ResourceBudget {
            max_http_response_bytes: 4,
            ..ResourceBudget::default()
        };
        let daemon =
            BroadwebDaemon::start_with_registry(test_state_root("budget"), budget, registry)
                .expect("daemon");
        let error = daemon
            .fetch_http(HttpFetchRequest::default_profile(&address))
            .expect_err("budget exceeded");
        server.join().expect("server");

        assert!(matches!(error, BroadwebdError::ResponseTooLarge { .. }));

        let _ = fs::remove_dir_all(daemon.state_root().path());
    }

    #[test]
    fn direct_http_rejects_non_http_schemes() {
        let transport = super::DirectHttpTransport;
        let request = TransportHttpRequest {
            profile: "default".to_string(),
            url: "ipfs://bafybeigdyrzt".to_string(),
        };

        assert!(matches!(
            transport.fetch_http(&request, &ResourceBudget::default()),
            Err(BroadwebdError::UnsupportedRequest(_))
        ));
    }

    #[test]
    fn ipfs_urls_can_be_mapped_to_local_gateway_http() {
        assert_eq!(
            ipfs_gateway_http_url("ipfs://bafybeigdyrzt/index.html", "http://127.0.0.1:8080")
                .expect("gateway url"),
            "http://127.0.0.1:8080/ipfs/bafybeigdyrzt/index.html"
        );
        assert_eq!(
            ipfs_gateway_http_url("ipns://example.net/docs?a=1", "http://127.0.0.1:8080/")
                .expect("gateway url"),
            "http://127.0.0.1:8080/ipns/example.net/docs?a=1"
        );
    }

    #[test]
    #[ignore = "external internet smoke test; run through `make test-external-network`"]
    fn external_direct_http_fetches_example_domain() {
        if std::env::var_os("SLATE_EXTERNAL_NETWORK_TESTS").is_none() {
            eprintln!("set SLATE_EXTERNAL_NETWORK_TESTS=1 to run external network tests");
            return;
        }

        let daemon = BroadwebDaemon::start(test_state_root("external-http")).expect("daemon");
        let response = daemon
            .fetch_http(HttpFetchRequest::default_profile("http://example.com/"))
            .expect("fetch example.com");

        assert_eq!(response.status_code, 200);
        assert_eq!(response.disposition, FetchDisposition::RenderHtml);
        assert!(response.body_text_lossy().contains("Example Domain"));

        let _ = fs::remove_dir_all(daemon.state_root().path());
    }

    fn local_http_fixture(
        content_type: &'static str,
        body: &'static str,
    ) -> (String, thread::JoinHandle<()>) {
        let listener = TcpListener::bind("127.0.0.1:0").expect("bind local server");
        let address = format!("http://{}", listener.local_addr().expect("local address"));
        let server = thread::spawn(move || {
            let (mut stream, _) = listener.accept().expect("accept request");
            let mut request = [0_u8; 1024];
            let _ = stream.read(&mut request).expect("read request");
            let response = format!(
                "HTTP/1.1 200 OK\r\nContent-Type: {content_type}\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
                body.len()
            );
            stream
                .write_all(response.as_bytes())
                .expect("write response");
        });
        (address, server)
    }

    fn test_state_root(name: &str) -> PathBuf {
        std::env::temp_dir().join(format!(
            "slate-broadwebd-test-{}-{name}",
            std::process::id()
        ))
    }
}
