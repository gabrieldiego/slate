use crate::peer_discovery::{
    DEFAULT_PROFILE_SYNC_PEER_DISCOVERY_MAX_BYTES, PROFILE_SYNC_DISCOVERY_PROTOCOL_IPNS,
    ProfileSyncPeerAdvertisement, ProfileSyncPeerDiscoveryProtocol,
    ProfileSyncPeerDiscoveryProvider, ProfileSyncPeerDiscoveryPublication,
    ProfileSyncPeerDiscoveryQuery, ProfileSyncPeerDiscoveryResult,
};
use crate::protocols::ipfs::kubo::{IpfsKuboProfileSyncRpc, IpfsKuboProfileSyncRpcExecutor};
use crate::{BroadwebdError, ResourceBudget};
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

#[derive(Debug)]
pub struct IpnsProfileSyncPeerDiscoveryProvider<Executor> {
    rpc: IpfsKuboProfileSyncRpc,
    executor: Executor,
    budget: ResourceBudget,
    publish_key_id: Option<String>,
    resolve_names: Vec<String>,
}

impl<Executor> IpnsProfileSyncPeerDiscoveryProvider<Executor> {
    pub fn new(rpc: IpfsKuboProfileSyncRpc, executor: Executor, budget: ResourceBudget) -> Self {
        Self {
            rpc,
            executor,
            budget,
            publish_key_id: None,
            resolve_names: Vec::new(),
        }
    }

    pub fn with_publish_key_id(mut self, key_id: impl Into<String>) -> Self {
        self.publish_key_id = Some(key_id.into());
        self
    }

    pub fn with_resolve_name(mut self, name: impl Into<String>) -> Self {
        self.resolve_names.push(name.into());
        self
    }

    pub fn with_resolve_names(
        mut self,
        names: impl IntoIterator<Item = impl Into<String>>,
    ) -> Self {
        self.resolve_names.extend(names.into_iter().map(Into::into));
        self
    }

    pub fn resolve_names(&self) -> &[String] {
        self.resolve_names.as_slice()
    }

    pub fn publish_key_id(&self) -> Option<&str> {
        self.publish_key_id.as_deref()
    }
}

impl<Executor> ProfileSyncPeerDiscoveryProvider for IpnsProfileSyncPeerDiscoveryProvider<Executor>
where
    Executor: IpfsKuboProfileSyncRpcExecutor + Send + Sync,
{
    fn publish_profile_sync_peer(
        &self,
        protocol: ProfileSyncPeerDiscoveryProtocol,
        namespace: &str,
        advertisement: ProfileSyncPeerAdvertisement,
    ) -> Result<ProfileSyncPeerDiscoveryPublication, BroadwebdError> {
        if protocol != ProfileSyncPeerDiscoveryProtocol::Ipns {
            return Err(BroadwebdError::UnsupportedRequest(format!(
                "IPNS profile-sync peer discovery cannot publish {} records",
                protocol.as_str()
            )));
        }
        let Some(key_id) = self.publish_key_id.as_deref() else {
            return Err(BroadwebdError::UnsupportedRequest(
                "IPNS profile-sync peer discovery publish requires an IPNS key id".to_string(),
            ));
        };
        let record = IpnsProfileSyncPeerDiscoveryRecord::new(namespace, advertisement.clone())?;
        let bytes = record.encode()?;
        let object_id =
            self.rpc
                .put_encrypted_object(&self.executor, bytes.as_slice(), &self.budget)?;
        self.rpc
            .retain_object(&self.executor, object_id.as_str(), &self.budget)?;
        self.rpc
            .publish_root(&self.executor, key_id, object_id.as_str(), &self.budget)?;

        Ok(ProfileSyncPeerDiscoveryPublication {
            protocol,
            namespace: namespace.to_string(),
            advertisement,
        })
    }

    fn discover_profile_sync_peers(
        &self,
        query: &ProfileSyncPeerDiscoveryQuery,
    ) -> Result<Vec<ProfileSyncPeerDiscoveryResult>, BroadwebdError> {
        query.validate()?;
        if !query
            .protocols
            .contains(&ProfileSyncPeerDiscoveryProtocol::Ipns)
        {
            return Ok(Vec::new());
        }

        let mut results = Vec::<Option<ProfileSyncPeerDiscoveryResult>>::new();
        let mut freshest_by_peer = BTreeMap::<(String, String), usize>::new();
        for name in &self.resolve_names {
            let object_id = self
                .rpc
                .resolve_root(&self.executor, name.as_str(), &self.budget)?;
            let bytes =
                self.rpc
                    .get_encrypted_object(&self.executor, object_id.as_str(), &self.budget)?;
            let record = IpnsProfileSyncPeerDiscoveryRecord::decode(bytes.as_slice())?;
            let advertisement = record.advertisement;
            if record.namespace != query.namespace
                || advertisement.network_id != query.network_id
                || advertisement.node_id == query.requester_node_id
                || !advertisement.supports_profile_sync_service_frames()
            {
                continue;
            }
            let seen_key = (
                advertisement.node_id.clone(),
                advertisement.provider_id.clone(),
            );
            let candidate = ProfileSyncPeerDiscoveryResult {
                protocol: ProfileSyncPeerDiscoveryProtocol::Ipns,
                namespace: record.namespace,
                advertisement,
            };
            if let Some(index) = freshest_by_peer.get(&seen_key).copied() {
                let stored = results[index]
                    .as_ref()
                    .expect("IPNS discovery freshness key should point at a candidate");
                if candidate.advertisement.sequence > stored.advertisement.sequence {
                    results[index] = Some(candidate);
                }
            } else if results.len() < query.max_peers {
                freshest_by_peer.insert(seen_key, results.len());
                results.push(Some(candidate));
            }
        }

        Ok(results.into_iter().flatten().collect())
    }
}

#[derive(Clone, Debug, Eq, PartialEq, Deserialize, Serialize)]
struct IpnsProfileSyncPeerDiscoveryRecord {
    protocol: String,
    namespace: String,
    advertisement: ProfileSyncPeerAdvertisement,
}

impl IpnsProfileSyncPeerDiscoveryRecord {
    fn new(
        namespace: &str,
        advertisement: ProfileSyncPeerAdvertisement,
    ) -> Result<Self, BroadwebdError> {
        advertisement.validate()?;
        let record = Self {
            protocol: PROFILE_SYNC_DISCOVERY_PROTOCOL_IPNS.to_string(),
            namespace: namespace.to_string(),
            advertisement,
        };
        record.validate()?;
        Ok(record)
    }

    fn encode(&self) -> Result<Vec<u8>, BroadwebdError> {
        self.validate()?;
        let bytes = serde_json::to_vec(self).map_err(|error| {
            BroadwebdError::Request(format!(
                "encode IPNS profile-sync discovery record: {error}"
            ))
        })?;
        if bytes.len() > DEFAULT_PROFILE_SYNC_PEER_DISCOVERY_MAX_BYTES {
            return Err(BroadwebdError::FrameTooLarge {
                limit: DEFAULT_PROFILE_SYNC_PEER_DISCOVERY_MAX_BYTES,
                actual: bytes.len(),
            });
        }
        Ok(bytes)
    }

    fn decode(bytes: &[u8]) -> Result<Self, BroadwebdError> {
        if bytes.len() > DEFAULT_PROFILE_SYNC_PEER_DISCOVERY_MAX_BYTES {
            return Err(BroadwebdError::FrameTooLarge {
                limit: DEFAULT_PROFILE_SYNC_PEER_DISCOVERY_MAX_BYTES,
                actual: bytes.len(),
            });
        }
        let record = serde_json::from_slice::<Self>(bytes).map_err(|error| {
            BroadwebdError::Request(format!(
                "decode IPNS profile-sync discovery record: {error}"
            ))
        })?;
        record.validate()?;
        Ok(record)
    }

    fn validate(&self) -> Result<(), BroadwebdError> {
        if self.protocol != PROFILE_SYNC_DISCOVERY_PROTOCOL_IPNS {
            return Err(BroadwebdError::Request(format!(
                "IPNS profile-sync discovery record has unsupported protocol: {}",
                self.protocol
            )));
        }
        if self.namespace.is_empty()
            || self.namespace.len() > 256
            || !self.namespace.bytes().all(|byte| {
                byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'_' | b'.' | b'/')
            })
        {
            return Err(BroadwebdError::Request(format!(
                "invalid IPNS profile-sync discovery namespace: {:?}",
                self.namespace
            )));
        }
        self.advertisement.validate()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::peer_discovery::DEFAULT_PROFILE_SYNC_DISCOVERY_NAMESPACE;

    #[test]
    fn ipns_profile_sync_peer_discovery_record_round_trips_bounded_json() {
        let record = IpnsProfileSyncPeerDiscoveryRecord::new(
            DEFAULT_PROFILE_SYNC_DISCOVERY_NAMESPACE,
            ProfileSyncPeerAdvertisement::new(
                "profile-a",
                "device-a",
                "provider-a",
                "/ipns/k51-profile-sync-provider-a",
                1,
            )
            .expect("advertisement"),
        )
        .expect("record");
        let bytes = record.encode().expect("encode record");

        assert_eq!(
            IpnsProfileSyncPeerDiscoveryRecord::decode(bytes.as_slice()).expect("decode record"),
            record
        );
    }
}
