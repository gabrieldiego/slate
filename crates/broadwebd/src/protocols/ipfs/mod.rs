mod address;
mod config;
mod gateway;
mod kubo;
mod service;

pub use config::{IpfsConfig, IpfsGatewayEndpoint, IpfsGatewayScope, IpfsTransportKind};
pub use gateway::{IpfsGatewayTransport, ipfs_gateway_http_url};
#[cfg(any(test, feature = "test-fixtures"))]
pub use kubo::InternalKuboRpcResponse;
pub use kubo::{IpfsKuboRpcEndpoint, IpfsKuboRpcTransport, ipfs_kubo_cat_url};
#[cfg(any(test, feature = "test-fixtures"))]
pub(crate) use kubo::{
    internal_kubo_rpc_url_belongs_to_network, register_internal_kubo_rpc_fixture_for_network,
    take_internal_kubo_rpc_fixture_requests,
};
pub use service::IpfsService;
