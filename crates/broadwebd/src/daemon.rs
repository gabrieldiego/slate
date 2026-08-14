use crate::{
    ApplicationServicePlugin, BroadwebdError, DEFAULT_PROFILE, DaemonHealth, DaemonLifecycle,
    HttpFetchRequest, HttpFetchResponse, PluginInstallReport, PluginMetadata, PluginRegistry,
    ResourceBudget, StateRoot, TransportPlugin,
};
use std::path::PathBuf;

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
        self.state_root.prepare_profile(&request.profile)?;
        self.registry.fetch_http(request, &self.budget)
    }
}

pub fn default_session_state_root() -> PathBuf {
    std::env::temp_dir()
        .join("slate-broadwebd")
        .join(format!("process-{}", std::process::id()))
}
