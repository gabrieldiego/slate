#![forbid(unsafe_code)]

mod budget;
mod daemon;
mod error;
mod health;
mod http;
pub mod protocols;
mod registry;
pub mod services;
mod state;
pub mod transports;

pub(crate) const DEFAULT_PROFILE: &str = "default";
pub const DIRECT_HTTP_PLUGIN: &str = "direct-http";
pub const HTTP_FETCH_PLUGIN: &str = "http-fetch";
pub const IPFS_PROTOCOL_SERVICE: &str = "ipfs";
pub const IPFS_GATEWAY_PLUGIN: &str = "ipfs-gateway";
pub const DEFAULT_IPFS_GATEWAY: &str = "http://127.0.0.1:8080";

pub use budget::ResourceBudget;
pub use daemon::{BroadwebDaemon, default_session_state_root};
pub use error::BroadwebdError;
pub use health::{
    DaemonHealth, DaemonLifecycle, PluginHealth, PluginKind, PluginMetadata, PluginStatus,
    ResourceProfile,
};
pub use http::{
    FetchDisposition, HttpFetchRequest, HttpFetchResponse, HttpHeader, ServiceRequest,
    ServiceResponse, TransportHttpRequest,
};
pub use protocols::ipfs::{IpfsConfig, IpfsGatewayTransport, IpfsService, ipfs_gateway_http_url};
pub use registry::{
    ApplicationServicePlugin, PluginInstallReport, PluginRegistry, ProtocolInstallReport,
    ProtocolService, TransportPlugin,
};
pub use services::http_fetch::HttpFetchService;
pub use state::StateRoot;
pub use transports::direct_http::DirectHttpTransport;

#[cfg(test)]
mod tests {
    use super::{
        BroadwebDaemon, BroadwebdError, DIRECT_HTTP_PLUGIN, FetchDisposition, HttpFetchRequest,
        HttpFetchResponse, IPFS_GATEWAY_PLUGIN, IpfsConfig, IpfsService, PluginHealth, PluginKind,
        PluginMetadata, PluginRegistry, ResourceBudget, ResourceProfile, StateRoot,
        TransportHttpRequest, TransportPlugin, ipfs_gateway_http_url,
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
        assert!(health.plugins.iter().any(|status| {
            status.metadata.id == "ipfs" && matches!(status.health, PluginHealth::Ready)
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
    fn daemon_can_install_replace_and_remove_plugins_without_restart() {
        let mut daemon = BroadwebDaemon::start_with_registry(
            test_state_root("hot-plugins"),
            ResourceBudget::default(),
            PluginRegistry::new(),
        )
        .expect("daemon");

        assert!(matches!(
            daemon.fetch_http(HttpFetchRequest::default_profile("http://example.test/")),
            Err(BroadwebdError::MissingPlugin(plugin)) if plugin == "http-fetch"
        ));

        let service_install = daemon.install_service(super::HttpFetchService);
        assert_eq!(service_install.metadata.id, "http-fetch");
        assert!(!service_install.replaced_existing);

        let transport_install =
            daemon.install_transport(FixtureTransport::new(DIRECT_HTTP_PLUGIN, "first body"));
        assert_eq!(transport_install.metadata.id, DIRECT_HTTP_PLUGIN);
        assert!(!transport_install.replaced_existing);

        let first_response = daemon
            .fetch_http(HttpFetchRequest::default_profile("http://example.test/"))
            .expect("fetch with installed plugins");
        assert!(first_response.body_text_lossy().contains("first body"));

        let transport_replace =
            daemon.install_transport(FixtureTransport::new(DIRECT_HTTP_PLUGIN, "second body"));
        assert_eq!(transport_replace.metadata.id, DIRECT_HTTP_PLUGIN);
        assert!(transport_replace.replaced_existing);

        let second_response = daemon
            .fetch_http(HttpFetchRequest::default_profile("http://example.test/"))
            .expect("fetch with replaced transport");
        assert!(second_response.body_text_lossy().contains("second body"));

        let removed = daemon.remove_service("http-fetch").expect("remove service");
        assert_eq!(removed.id, "http-fetch");
        assert!(matches!(
            daemon.fetch_http(HttpFetchRequest::default_profile("http://example.test/")),
            Err(BroadwebdError::MissingPlugin(plugin)) if plugin == "http-fetch"
        ));

        let _ = fs::remove_dir_all(daemon.state_root().path());
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
    fn http_fetch_routes_ipfs_through_protocol_service() {
        let (gateway, server) = local_http_fixture(
            "text/html; charset=utf-8",
            "<!doctype html><title>IPFS Service Fixture</title><h1>Fetched From IPFS Service</h1>",
        );
        let mut registry = PluginRegistry::new();
        registry.register_protocol_service(IpfsService::new(IpfsConfig::new(&gateway)));
        registry.register_service(super::HttpFetchService);
        let daemon = BroadwebDaemon::start_with_registry(
            test_state_root("ipfs-service-html"),
            Default::default(),
            registry,
        )
        .expect("daemon");
        let response = daemon
            .fetch_http(HttpFetchRequest::default_profile(
                "ipfs://bafybeigdyrzt/index.html",
            ))
            .expect("fetch IPFS fixture");
        server.join().expect("server");

        assert_eq!(response.disposition, FetchDisposition::RenderHtml);
        assert!(response.body_text_lossy().contains("IPFS Service Fixture"));

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
    fn ipfs_service_registers_gateway_transport_from_config() {
        let service = IpfsService::new(IpfsConfig::new("http://127.0.0.1:9090"));
        let mut registry = PluginRegistry::new();
        let installs = service.install_adapter_plugins(&mut registry);

        assert!(!service.config().allow_public_gateway_fallback());
        assert_eq!(installs.len(), 1);
        assert_eq!(installs[0].metadata.id, IPFS_GATEWAY_PLUGIN);
        assert!(!installs[0].replaced_existing);
        assert!(
            registry
                .list_transports()
                .iter()
                .any(|metadata| metadata.id == IPFS_GATEWAY_PLUGIN)
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

    struct FixtureTransport {
        id: &'static str,
        body: &'static str,
    }

    impl FixtureTransport {
        fn new(id: &'static str, body: &'static str) -> Self {
            Self { id, body }
        }
    }

    impl TransportPlugin for FixtureTransport {
        fn metadata(&self) -> PluginMetadata {
            PluginMetadata::new(self.id, PluginKind::Transport)
                .with_capabilities(&["http", "https", "http-fetch"])
                .with_privacy_boundary("test fixture transport")
                .with_resource_profile(ResourceProfile::Low)
        }

        fn fetch_http(
            &self,
            request: &TransportHttpRequest,
            _budget: &ResourceBudget,
        ) -> Result<HttpFetchResponse, BroadwebdError> {
            Ok(HttpFetchResponse::new(
                &request.url,
                200,
                Some("text/html; charset=utf-8".to_string()),
                Vec::new(),
                format!("<!doctype html><title>Fixture</title>{}", self.body).into_bytes(),
            ))
        }
    }
}
