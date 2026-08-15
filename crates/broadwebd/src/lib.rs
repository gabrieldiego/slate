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
pub const IPFS_KUBO_RPC_PLUGIN: &str = "ipfs-kubo-rpc";
pub const DEFAULT_IPFS_GATEWAY: &str = "http://127.0.0.1:8080";
pub const DEFAULT_IPFS_KUBO_RPC_API: &str = "http://127.0.0.1:5001";
pub const SLATE_IPFS_GATEWAY_ENV: &str = "SLATE_IPFS_GATEWAY";
pub const SLATE_IPFS_GATEWAY_SCOPE_ENV: &str = "SLATE_IPFS_GATEWAY_SCOPE";
pub const SLATE_IPFS_TRANSPORT_ENV: &str = "SLATE_IPFS_TRANSPORT";
pub const SLATE_IPFS_KUBO_RPC_ENV: &str = "SLATE_IPFS_KUBO_RPC";

pub use budget::ResourceBudget;
pub use daemon::{BroadwebDaemon, default_session_state_root};
pub use error::BroadwebdError;
pub use health::{
    DaemonHealth, DaemonLifecycle, PluginHealth, PluginKind, PluginMetadata, PluginStatus,
    ResourceProfile,
};
pub use http::{
    FetchDisposition, FetchRouteInfo, HttpFetchRequest, HttpFetchResponse, HttpHeader,
    ServiceRequest, ServiceResponse, TransportHttpRequest,
};
pub use protocols::ipfs::{
    IpfsConfig, IpfsGatewayEndpoint, IpfsGatewayScope, IpfsGatewayTransport, IpfsKuboRpcEndpoint,
    IpfsKuboRpcTransport, IpfsService, IpfsTransportKind, ipfs_gateway_http_url, ipfs_kubo_cat_url,
};
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
        BroadwebDaemon, BroadwebdError, DEFAULT_IPFS_KUBO_RPC_API, DIRECT_HTTP_PLUGIN,
        FetchDisposition, HttpFetchRequest, HttpFetchResponse, IPFS_GATEWAY_PLUGIN,
        IPFS_KUBO_RPC_PLUGIN, IpfsConfig, IpfsGatewayScope, IpfsGatewayTransport,
        IpfsKuboRpcEndpoint, IpfsService, IpfsTransportKind, PluginHealth, PluginKind,
        PluginMetadata, PluginRegistry, ProtocolService, ResourceBudget, ResourceProfile,
        SLATE_IPFS_GATEWAY_SCOPE_ENV, SLATE_IPFS_TRANSPORT_ENV, StateRoot, TransportHttpRequest,
        TransportPlugin, ipfs_gateway_http_url, ipfs_kubo_cat_url,
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
    fn default_registry_can_use_explicit_ipfs_config() {
        let registry = PluginRegistry::with_default_http_and_ipfs_config(
            IpfsConfig::with_public_gateway("https://ipfs.io").expect("public gateway config"),
        );
        let statuses = registry.plugin_statuses();

        assert!(statuses.iter().any(|status| {
            status.metadata.id == "ipfs"
                && status
                    .metadata
                    .capabilities
                    .iter()
                    .any(|capability| capability == "public-gateway")
                && matches!(status.health, PluginHealth::Ready)
        }));
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
        registry.register_transport(IpfsGatewayTransport::local(&gateway).expect("local gateway"));
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
        registry.register_protocol_service(IpfsService::new(
            IpfsConfig::new(&gateway).expect("local gateway config"),
        ));
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
    fn http_fetch_annotates_ipfs_gateway_profile_and_privacy_context() {
        let (gateway, server) = local_http_fixture(
            "text/html; charset=utf-8",
            "<!doctype html><title>IPFS Profile Fixture</title>",
        );
        let mut registry = PluginRegistry::new();
        registry.register_protocol_service(IpfsService::new(
            IpfsConfig::new(&gateway).expect("local gateway config"),
        ));
        registry.register_service(super::HttpFetchService);
        let daemon = BroadwebDaemon::start_with_registry(
            test_state_root("ipfs-route-context"),
            Default::default(),
            registry,
        )
        .expect("daemon");
        let response = daemon
            .fetch_http(HttpFetchRequest::new(
                "research",
                "ipfs://bafybeigdyrzt/index.html",
            ))
            .expect("fetch IPFS fixture");
        server.join().expect("server");

        let route = response.route.expect("route info");
        assert_eq!(route.profile, "research");
        assert_eq!(route.transport_id, IPFS_GATEWAY_PLUGIN);
        assert!(route.privacy_boundary.contains("local IPFS gateway"));
        assert!(
            daemon
                .state_root()
                .profile_root("research")
                .expect("research profile root")
                .join("protocol-state")
                .is_dir()
        );

        let _ = fs::remove_dir_all(daemon.state_root().path());
    }

    #[test]
    fn http_fetch_can_use_ipfs_gateway_transport_for_downloads() {
        let (gateway, server) = local_http_fixture("image/png", "png-ish");
        let mut registry = PluginRegistry::new();
        registry.register_transport(IpfsGatewayTransport::local(&gateway).expect("local gateway"));
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
    fn http_fetch_infers_html_from_generic_content_type_and_body() {
        let (address, server) = local_http_fixture(
            "application/octet-stream",
            "<!doctype html><title>Sniffed HTML Fixture</title><h1>Fetched</h1>",
        );
        let daemon = BroadwebDaemon::start(test_state_root("sniff-html-body")).expect("daemon");
        let response = daemon
            .fetch_http(HttpFetchRequest::default_profile(&address))
            .expect("fetch fixture");
        server.join().expect("server");

        assert_eq!(
            response.content_type,
            Some("text/html; charset=utf-8".to_string())
        );
        assert_eq!(response.disposition, FetchDisposition::RenderHtml);

        let _ = fs::remove_dir_all(daemon.state_root().path());
    }

    #[test]
    fn http_fetch_infers_html_fragment_from_generic_content_type() {
        let (address, server) =
            local_http_fixture("application/octet-stream", "<h2>Simple IPFS Fixture</h2>");
        let daemon = BroadwebDaemon::start(test_state_root("sniff-html-fragment")).expect("daemon");
        let response = daemon
            .fetch_http(HttpFetchRequest::default_profile(&address))
            .expect("fetch fixture");
        server.join().expect("server");

        assert_eq!(
            response.content_type,
            Some("text/html; charset=utf-8".to_string())
        );
        assert_eq!(response.disposition, FetchDisposition::RenderHtml);

        let _ = fs::remove_dir_all(daemon.state_root().path());
    }

    #[test]
    fn http_fetch_infers_html_from_generic_content_type_and_path() {
        let (address, server) =
            local_http_fixture("application/octet-stream", "<h1>IPFS HTML Path</h1>");
        let address = format!("{address}/index.html");
        let daemon = BroadwebDaemon::start(test_state_root("sniff-html-path")).expect("daemon");
        let response = daemon
            .fetch_http(HttpFetchRequest::default_profile(&address))
            .expect("fetch fixture");
        server.join().expect("server");

        assert_eq!(
            response.content_type,
            Some("text/html; charset=utf-8".to_string())
        );
        assert_eq!(response.disposition, FetchDisposition::RenderHtml);

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
    fn ipfs_gateway_urls_preserve_cidv0_case() {
        assert_eq!(
            ipfs_gateway_http_url(
                "ipfs://QmUKwop8CmB4ictvQyCJQru97NRVakJFVWpV74guJ89tcb/index.html?filename=Index.html#top",
                "http://127.0.0.1:8080",
            )
            .expect("CIDv0 gateway URL"),
            "http://127.0.0.1:8080/ipfs/QmUKwop8CmB4ictvQyCJQru97NRVakJFVWpV74guJ89tcb/index.html?filename=Index.html"
        );
    }

    #[test]
    fn ipfs_service_registers_gateway_transport_from_config() {
        let service = IpfsService::new(
            IpfsConfig::new("http://127.0.0.1:9090").expect("local gateway config"),
        );
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
    fn ipfs_config_rejects_public_gateway_without_policy() {
        assert!(matches!(
            IpfsConfig::new("https://ipfs.io"),
            Err(BroadwebdError::UnsupportedRequest(_))
        ));
    }

    #[test]
    fn ipfs_config_accepts_explicit_public_gateway() {
        let config = IpfsConfig::with_public_gateway("https://ipfs.io")
            .expect("explicit public gateway config");

        assert_eq!(config.gateway_base(), "https://ipfs.io");
        assert_eq!(config.gateway_scope(), IpfsGatewayScope::Public);
        assert!(config.uses_public_gateway());
        assert!(!config.allow_public_gateway_fallback());
    }

    #[test]
    fn ipfs_config_options_default_to_local_gateway() {
        let config = IpfsConfig::from_options(None, None).expect("default IPFS config");

        assert_eq!(config.gateway_base(), super::DEFAULT_IPFS_GATEWAY);
        assert_eq!(config.gateway_scope(), IpfsGatewayScope::Local);
    }

    #[test]
    fn ipfs_config_options_reject_public_gateway_without_public_scope() {
        assert!(matches!(
            IpfsConfig::from_options(Some("https://ipfs.io"), None),
            Err(BroadwebdError::UnsupportedRequest(_))
        ));
    }

    #[test]
    fn ipfs_config_options_accept_public_gateway_with_public_scope() {
        let config = IpfsConfig::from_options(Some("https://ipfs.io"), Some("public"))
            .expect("public IPFS gateway config");

        assert_eq!(config.gateway_base(), "https://ipfs.io");
        assert_eq!(config.gateway_scope(), IpfsGatewayScope::Public);
    }

    #[test]
    fn ipfs_config_options_reject_public_scope_without_gateway() {
        assert!(matches!(
            IpfsConfig::from_options(None, Some("public")),
            Err(BroadwebdError::UnsupportedRequest(error))
                if error.contains(SLATE_IPFS_GATEWAY_SCOPE_ENV)
        ));
    }

    #[test]
    fn ipfs_runtime_options_default_to_gateway_transport() {
        let config =
            IpfsConfig::from_runtime_options(None, None, None, None).expect("default IPFS config");

        assert_eq!(config.transport(), IpfsTransportKind::Gateway);
        assert_eq!(config.gateway_base(), super::DEFAULT_IPFS_GATEWAY);
        assert_eq!(config.http_transport_id(), IPFS_GATEWAY_PLUGIN);
    }

    #[test]
    fn ipfs_runtime_options_accept_explicit_kubo_transport() {
        let config = IpfsConfig::from_runtime_options(None, None, Some("kubo-rpc"), None)
            .expect("Kubo RPC config");

        assert_eq!(config.transport(), IpfsTransportKind::KuboRpc);
        assert_eq!(config.http_transport_id(), IPFS_KUBO_RPC_PLUGIN);
        assert_eq!(
            config
                .kubo_rpc_endpoint()
                .expect("Kubo RPC endpoint")
                .api_base_url(),
            DEFAULT_IPFS_KUBO_RPC_API
        );
    }

    #[test]
    fn ipfs_runtime_options_accept_kubo_rpc_endpoint_override() {
        let config = IpfsConfig::from_runtime_options(
            None,
            None,
            Some("local-kubo-rpc"),
            Some("http://127.0.0.1:5050"),
        )
        .expect("Kubo RPC config");

        assert_eq!(config.transport(), IpfsTransportKind::KuboRpc);
        assert_eq!(
            config
                .kubo_rpc_endpoint()
                .expect("Kubo RPC endpoint")
                .api_base_url(),
            "http://127.0.0.1:5050"
        );
    }

    #[test]
    fn ipfs_runtime_options_select_kubo_when_rpc_endpoint_is_set() {
        let config =
            IpfsConfig::from_runtime_options(None, None, None, Some("http://127.0.0.1:5050"))
                .expect("Kubo RPC config");

        assert_eq!(config.transport(), IpfsTransportKind::KuboRpc);
        assert_eq!(config.http_transport_id(), IPFS_KUBO_RPC_PLUGIN);
    }

    #[test]
    fn ipfs_runtime_options_reject_kubo_mixed_with_gateway_policy() {
        assert!(matches!(
            IpfsConfig::from_runtime_options(
                Some("http://127.0.0.1:8080"),
                None,
                Some("kubo-rpc"),
                None
            ),
            Err(BroadwebdError::UnsupportedRequest(error))
                if error.contains(SLATE_IPFS_TRANSPORT_ENV)
        ));
    }

    #[test]
    fn ipfs_public_gateway_transport_exposes_public_privacy_boundary() {
        let transport =
            IpfsGatewayTransport::public("https://ipfs.io").expect("public gateway transport");
        let metadata = transport.metadata();

        assert_eq!(transport.gateway_scope(), IpfsGatewayScope::Public);
        assert!(
            metadata
                .capabilities
                .iter()
                .any(|capability| capability == "public-gateway")
        );
        assert!(metadata.privacy_boundary.contains("public IPFS gateway"));
        assert_eq!(
            ipfs_gateway_http_url("ipfs://bafybeigdyrzt/index.html", transport.gateway_base())
                .expect("gateway url"),
            "https://ipfs.io/ipfs/bafybeigdyrzt/index.html"
        );
    }

    #[test]
    fn ipfs_service_registers_public_gateway_transport_from_explicit_config() {
        let service = IpfsService::new(
            IpfsConfig::with_public_gateway("https://ipfs.io")
                .expect("explicit public gateway config"),
        );
        let mut registry = PluginRegistry::new();
        let installs = service.install_adapter_plugins(&mut registry);
        let metadata = service.metadata();

        assert!(service.config().uses_public_gateway());
        assert_eq!(installs.len(), 1);
        assert_eq!(installs[0].metadata.id, IPFS_GATEWAY_PLUGIN);
        assert!(
            installs[0]
                .metadata
                .capabilities
                .iter()
                .any(|capability| capability == "public-gateway")
        );
        assert!(
            metadata
                .capabilities
                .iter()
                .any(|capability| capability == "public-gateway")
        );
        assert!(
            registry
                .list_transports()
                .iter()
                .any(|metadata| metadata.id == IPFS_GATEWAY_PLUGIN)
        );
    }

    #[test]
    fn ipfs_kubo_urls_map_to_local_rpc_cat_endpoint() {
        assert_eq!(
            ipfs_kubo_cat_url(
                "ipfs://bafybeigdyrzt/index.html?a=1",
                "http://127.0.0.1:5001"
            )
            .expect("Kubo cat url"),
            "http://127.0.0.1:5001/api/v0/cat?arg=%2Fipfs%2Fbafybeigdyrzt%2Findex.html"
        );
        assert_eq!(
            ipfs_kubo_cat_url("ipns://example.net/docs/app.js", "http://127.0.0.1:5001/")
                .expect("Kubo cat url"),
            "http://127.0.0.1:5001/api/v0/cat?arg=%2Fipns%2Fexample.net%2Fdocs%2Fapp.js"
        );
    }

    #[test]
    fn ipfs_kubo_urls_preserve_cidv0_case() {
        assert_eq!(
            ipfs_kubo_cat_url(
                "ipfs://QmUKwop8CmB4ictvQyCJQru97NRVakJFVWpV74guJ89tcb/index.html?a=1",
                "http://127.0.0.1:5001",
            )
            .expect("CIDv0 Kubo cat URL"),
            "http://127.0.0.1:5001/api/v0/cat?arg=%2Fipfs%2FQmUKwop8CmB4ictvQyCJQru97NRVakJFVWpV74guJ89tcb%2Findex.html"
        );
    }

    #[test]
    fn ipfs_kubo_config_requires_loopback_rpc_endpoint() {
        assert!(IpfsKuboRpcEndpoint::local("http://127.0.0.1:5001").is_ok());
        assert!(matches!(
            IpfsKuboRpcEndpoint::local("https://ipfs.example.test:5001"),
            Err(BroadwebdError::UnsupportedRequest(_))
        ));
    }

    #[test]
    fn ipfs_service_registers_kubo_rpc_transport_from_config() {
        let service = IpfsService::new(
            IpfsConfig::with_kubo_rpc("http://127.0.0.1:5001").expect("Kubo RPC config"),
        );
        let mut registry = PluginRegistry::new();
        let installs = service.install_adapter_plugins(&mut registry);
        let metadata = service.metadata();

        assert_eq!(service.config().transport(), IpfsTransportKind::KuboRpc);
        assert!(service.config().uses_kubo_rpc());
        assert_eq!(service.config().http_transport_id(), IPFS_KUBO_RPC_PLUGIN);
        assert_eq!(installs.len(), 1);
        assert_eq!(installs[0].metadata.id, IPFS_KUBO_RPC_PLUGIN);
        assert!(
            metadata
                .capabilities
                .iter()
                .any(|capability| capability == "local-kubo-rpc")
        );
        assert_eq!(
            metadata.dependencies,
            vec![IPFS_KUBO_RPC_PLUGIN.to_string()]
        );
    }

    #[test]
    fn http_fetch_routes_ipfs_through_kubo_rpc_transport_for_html() {
        let (rpc, server) = local_kubo_rpc_fixture(
            "application/octet-stream",
            "<!doctype html><title>Kubo Fixture</title><h1>Fetched From Kubo</h1>",
        );
        let mut registry = PluginRegistry::new();
        registry.register_protocol_service(IpfsService::new(
            IpfsConfig::with_kubo_rpc(&rpc).expect("Kubo RPC config"),
        ));
        registry.register_service(super::HttpFetchService);
        let daemon = BroadwebDaemon::start_with_registry(
            test_state_root("ipfs-kubo-html"),
            Default::default(),
            registry,
        )
        .expect("daemon");
        let response = daemon
            .fetch_http(HttpFetchRequest::default_profile(
                "ipfs://bafybeigdyrzt/index.html",
            ))
            .expect("fetch Kubo fixture");
        let request = server.join().expect("server");

        assert!(
            request.contains("POST /api/v0/cat?arg=%2Fipfs%2Fbafybeigdyrzt%2Findex.html HTTP/1.1")
        );
        assert_eq!(response.disposition, FetchDisposition::RenderHtml);
        assert_eq!(
            response.content_type,
            Some("text/html; charset=utf-8".to_string())
        );
        assert!(response.body_text_lossy().contains("Kubo Fixture"));

        let _ = fs::remove_dir_all(daemon.state_root().path());
    }

    #[test]
    fn http_fetch_annotates_kubo_rpc_profile_and_privacy_context() {
        let (rpc, server) = local_kubo_rpc_fixture(
            "text/html; charset=utf-8",
            "<!doctype html><title>Kubo Profile Fixture</title>",
        );
        let mut registry = PluginRegistry::new();
        registry.register_protocol_service(IpfsService::new(
            IpfsConfig::with_kubo_rpc(&rpc).expect("Kubo RPC config"),
        ));
        registry.register_service(super::HttpFetchService);
        let daemon = BroadwebDaemon::start_with_registry(
            test_state_root("kubo-route-context"),
            Default::default(),
            registry,
        )
        .expect("daemon");
        let response = daemon
            .fetch_http(HttpFetchRequest::new(
                "research",
                "ipfs://bafybeigdyrzt/index.html",
            ))
            .expect("fetch Kubo fixture");
        server.join().expect("server");

        let route = response.route.expect("route info");
        assert_eq!(route.profile, "research");
        assert_eq!(route.transport_id, IPFS_KUBO_RPC_PLUGIN);
        assert!(route.privacy_boundary.contains("local Kubo RPC"));

        let _ = fs::remove_dir_all(daemon.state_root().path());
    }

    #[test]
    fn http_fetch_routes_kubo_directory_to_index_after_failed_cat() {
        let (rpc, server) = local_kubo_rpc_sequence_fixture(vec![
            ("500 Internal Server Error", "text/plain", "directory"),
            (
                "200 OK",
                "application/octet-stream",
                "<!doctype html><title>Kubo Directory Fixture</title>",
            ),
        ]);
        let mut registry = PluginRegistry::new();
        registry.register_protocol_service(IpfsService::new(
            IpfsConfig::with_kubo_rpc(&rpc).expect("Kubo RPC config"),
        ));
        registry.register_service(super::HttpFetchService);
        let daemon = BroadwebDaemon::start_with_registry(
            test_state_root("ipfs-kubo-directory"),
            Default::default(),
            registry,
        )
        .expect("daemon");
        let response = daemon
            .fetch_http(HttpFetchRequest::default_profile(
                "ipfs://bafybeigdyrzt/docs/",
            ))
            .expect("fetch Kubo directory fixture");
        let requests = server.join().expect("server");

        assert_eq!(requests.len(), 2);
        assert!(
            requests[0].contains("POST /api/v0/cat?arg=%2Fipfs%2Fbafybeigdyrzt%2Fdocs%2F HTTP/1.1")
        );
        assert!(
            requests[1].contains(
                "POST /api/v0/cat?arg=%2Fipfs%2Fbafybeigdyrzt%2Fdocs%2Findex.html HTTP/1.1"
            )
        );
        assert_eq!(response.status_code, 200);
        assert_eq!(response.disposition, FetchDisposition::RenderHtml);
        assert!(
            response
                .body_text_lossy()
                .contains("Kubo Directory Fixture")
        );

        let _ = fs::remove_dir_all(daemon.state_root().path());
    }

    #[test]
    fn http_fetch_routes_kubo_ipns_root_to_index_after_failed_cat() {
        let (rpc, server) = local_kubo_rpc_sequence_fixture(vec![
            ("500 Internal Server Error", "text/plain", "directory"),
            (
                "200 OK",
                "application/octet-stream",
                "<!doctype html><title>Kubo IPNS Fixture</title>",
            ),
        ]);
        let mut registry = PluginRegistry::new();
        registry.register_protocol_service(IpfsService::new(
            IpfsConfig::with_kubo_rpc(&rpc).expect("Kubo RPC config"),
        ));
        registry.register_service(super::HttpFetchService);
        let daemon = BroadwebDaemon::start_with_registry(
            test_state_root("ipns-kubo-root"),
            Default::default(),
            registry,
        )
        .expect("daemon");
        let response = daemon
            .fetch_http(HttpFetchRequest::default_profile("ipns://example.net"))
            .expect("fetch Kubo IPNS root fixture");
        let requests = server.join().expect("server");

        assert_eq!(requests.len(), 2);
        assert!(requests[0].contains("POST /api/v0/cat?arg=%2Fipns%2Fexample.net HTTP/1.1"));
        assert!(
            requests[1]
                .contains("POST /api/v0/cat?arg=%2Fipns%2Fexample.net%2Findex.html HTTP/1.1")
        );
        assert_eq!(response.status_code, 200);
        assert_eq!(response.disposition, FetchDisposition::RenderHtml);
        assert!(response.body_text_lossy().contains("Kubo IPNS Fixture"));

        let _ = fs::remove_dir_all(daemon.state_root().path());
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

    #[test]
    #[ignore = "external internet smoke test; run through `make test-external-network`"]
    fn external_public_ipfs_gateway_fetches_cid() {
        if std::env::var_os("SLATE_EXTERNAL_NETWORK_TESTS").is_none() {
            eprintln!("set SLATE_EXTERNAL_NETWORK_TESTS=1 to run external network tests");
            return;
        }

        let mut registry = PluginRegistry::new();
        registry.register_protocol_service(IpfsService::new(
            IpfsConfig::with_public_gateway("https://ipfs.io")
                .expect("explicit public gateway config"),
        ));
        registry.register_service(super::HttpFetchService);
        let daemon = BroadwebDaemon::start_with_registry(
            test_state_root("external-public-ipfs"),
            ResourceBudget::default(),
            registry,
        )
        .expect("daemon");
        let response = daemon
            .fetch_http(HttpFetchRequest::default_profile(
                "ipfs://bafybeigdyrzt5sfp7udm7hu76uh7y26nf3efuylqabf3oclgtqy55fbzdi",
            ))
            .expect("fetch public gateway CID");

        assert_eq!(response.status_code, 200);
        assert!(!response.body.is_empty());
        assert!(response.final_url.starts_with("https://ipfs.io/ipfs/"));

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

    fn local_kubo_rpc_fixture(
        content_type: &'static str,
        body: &'static str,
    ) -> (String, thread::JoinHandle<String>) {
        let (address, server) =
            local_kubo_rpc_sequence_fixture(vec![("200 OK", content_type, body)]);
        let server = thread::spawn(move || {
            server
                .join()
                .expect("Kubo sequence fixture")
                .into_iter()
                .next()
                .expect("expected one Kubo fixture request")
        });
        (address, server)
    }

    fn local_kubo_rpc_sequence_fixture(
        responses: Vec<(&'static str, &'static str, &'static str)>,
    ) -> (String, thread::JoinHandle<Vec<String>>) {
        let listener = TcpListener::bind("127.0.0.1:0").expect("bind local Kubo fixture");
        let address = format!("http://{}", listener.local_addr().expect("local address"));
        let server = thread::spawn(move || {
            let mut requests = Vec::new();
            for (status, content_type, body) in responses {
                let (mut stream, _) = listener.accept().expect("accept request");
                let mut request = [0_u8; 1024];
                let read = stream.read(&mut request).expect("read request");
                let response = format!(
                    "HTTP/1.1 {status}\r\nContent-Type: {content_type}\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
                    body.len()
                );
                stream
                    .write_all(response.as_bytes())
                    .expect("write response");
                requests.push(String::from_utf8_lossy(&request[..read]).into_owned());
            }
            requests
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
