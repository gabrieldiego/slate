use crate::{
    ApplicationServicePlugin, BroadwebdError, DEFAULT_PROFILE, DaemonHealth, DaemonLifecycle,
    DownloadRecord, FetchDisposition, FetchPurpose, HttpFetchRequest, HttpFetchResponse,
    IpfsConfig, PluginInstallReport, PluginMetadata, PluginRegistry, ProtocolInstallReport,
    ProtocolService, ResourceBudget, StateRoot, TemporaryDownloadRecord, TransportPlugin,
};
use std::path::PathBuf;
use std::sync::OnceLock;

use crate::{BroadwebStatusReporter, BroadwebStatusSnapshot};

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
        Self::start_with_registry_and_download_root(
            state_root,
            std::env::current_dir()?,
            budget,
            registry,
        )
    }

    pub fn start_with_download_root(
        state_root: impl Into<PathBuf>,
        downloads_root: impl Into<PathBuf>,
    ) -> Result<Self, BroadwebdError> {
        Self::start_with_registry_and_download_root(
            state_root,
            downloads_root,
            ResourceBudget::default(),
            PluginRegistry::with_default_http(),
        )
    }

    pub fn start_with_registry_and_download_root(
        state_root: impl Into<PathBuf>,
        downloads_root: impl Into<PathBuf>,
        budget: ResourceBudget,
        registry: PluginRegistry,
    ) -> Result<Self, BroadwebdError> {
        let state_root = StateRoot::prepare_with_download_root(state_root, downloads_root)?;
        state_root.prepare_profile(DEFAULT_PROFILE)?;
        Ok(Self {
            state_root,
            budget,
            registry,
            lifecycle: DaemonLifecycle::Ready,
        })
    }

    pub fn start_default_session() -> Result<Self, BroadwebdError> {
        Self::start_with_registry(
            default_session_state_root(),
            ResourceBudget::default(),
            PluginRegistry::with_default_http_and_ipfs_config_and_status(
                IpfsConfig::from_environment()?,
                default_session_status_reporter(),
            ),
        )
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

    pub fn status_snapshot(&self) -> BroadwebStatusSnapshot {
        self.registry.status_snapshot()
    }

    pub fn install_protocol_service(
        &mut self,
        service: impl ProtocolService + 'static,
    ) -> ProtocolInstallReport {
        self.registry.install_protocol_service(service)
    }

    pub fn install_transport(
        &mut self,
        plugin: impl TransportPlugin + 'static,
    ) -> PluginInstallReport {
        self.registry.install_transport(plugin)
    }

    pub fn install_service(
        &mut self,
        plugin: impl ApplicationServicePlugin + 'static,
    ) -> PluginInstallReport {
        self.registry.install_service(plugin)
    }

    pub fn remove_transport(&mut self, id: &str) -> Result<PluginMetadata, BroadwebdError> {
        self.registry.remove_transport(id)
    }

    pub fn remove_protocol_service(&mut self, id: &str) -> Result<PluginMetadata, BroadwebdError> {
        self.registry.remove_protocol_service(id)
    }

    pub fn remove_service(&mut self, id: &str) -> Result<PluginMetadata, BroadwebdError> {
        self.registry.remove_service(id)
    }

    pub fn budget(&self) -> &ResourceBudget {
        &self.budget
    }

    pub fn fetch_http(
        &self,
        request: HttpFetchRequest,
    ) -> Result<HttpFetchResponse, BroadwebdError> {
        let profile = request.profile.clone();
        let should_record_download = request.purpose == FetchPurpose::Navigation;
        let requested_download_filename = request.suggested_download_filename.clone();
        self.state_root.prepare_profile(&profile)?;
        let mut response = self.registry.fetch_http(request, &self.budget)?;
        if let Some(filename) = requested_download_filename
            && (200..=299).contains(&response.status_code)
        {
            response = response.with_download_disposition(filename);
        }
        if !should_record_download {
            return Ok(response);
        }
        self.record_download(profile, response)
    }

    pub fn temporary_downloads(
        &self,
        profile: &str,
    ) -> Result<Vec<TemporaryDownloadRecord>, BroadwebdError> {
        self.state_root.temporary_downloads(profile)
    }

    pub fn downloads(&self, profile: &str) -> Result<Vec<TemporaryDownloadRecord>, BroadwebdError> {
        self.state_root.downloads(profile)
    }

    fn record_download(
        &self,
        profile: String,
        response: HttpFetchResponse,
    ) -> Result<HttpFetchResponse, BroadwebdError> {
        let FetchDisposition::Download { suggested_filename } = &response.disposition else {
            return Ok(response);
        };

        let path = self
            .state_root
            .store_download(&profile, suggested_filename, &response.body)?;
        let filename = path
            .file_name()
            .and_then(|filename| filename.to_str())
            .unwrap_or(suggested_filename)
            .to_string();
        let download = DownloadRecord::new(
            profile,
            filename,
            path,
            response.body.len(),
            response.content_type.clone(),
        );
        Ok(response.with_download(download))
    }
}

pub fn default_session_state_root() -> PathBuf {
    std::env::temp_dir()
        .join("slate-broadwebd")
        .join(format!("process-{}", std::process::id()))
}

pub fn default_session_status_reporter() -> BroadwebStatusReporter {
    static REPORTER: OnceLock<BroadwebStatusReporter> = OnceLock::new();
    REPORTER.get_or_init(BroadwebStatusReporter::new).clone()
}

pub fn default_session_status_snapshot() -> BroadwebStatusSnapshot {
    default_session_status_reporter().snapshot()
}
