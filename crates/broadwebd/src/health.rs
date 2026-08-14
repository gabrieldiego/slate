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
