const DEFAULT_MAX_HTTP_RESPONSE_BYTES: usize = 8 * 1024 * 1024;
const DEFAULT_MAX_PROFILE_SYNC_OBJECT_BYTES: usize = 4 * 1024 * 1024;

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
    pub max_profile_sync_object_bytes: usize,
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
            max_profile_sync_object_bytes: DEFAULT_MAX_PROFILE_SYNC_OBJECT_BYTES,
        }
    }
}
