use super::{
    AdapterAddress, AdapterId, AdapterInfo, AdapterSelection, AdapterState, CacheLoadReport,
    CachedAdapter, NetError, SubnetSpec, load_cached_adapters,
};
use crate::bff::net::NetworkBinding;
use std::net::{IpAddr, Ipv4Addr, Ipv6Addr};

/// Immutable snapshot used by one network operation.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct NetworkLease {
    pub generation: u64,
    /// The requested selection (never rewritten by a lease).
    pub requested: AdapterSelection,
    pub adapter: AdapterInfo,
    pub local_address: AdapterAddress,
    pub subnet: SubnetSpec,
}

impl NetworkLease {
    pub fn local_ip(&self) -> IpAddr {
        self.local_address.ip
    }

    pub fn is_current(&self, generation: u64) -> bool {
        self.generation == generation
    }

    /// Immutable binding snapshot for this lease. Loopback leases carry no
    /// interface option; physical ones keep both IPv4 and IPv6 indexes.
    pub fn network_binding(&self) -> NetworkBinding {
        self.adapter.network_binding_for(self.local_address)
    }
}

/// One consistent adapter snapshot plus its generation.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct NetworkInventory {
    pub adapters: Vec<AdapterInfo>,
    pub generation: u64,
}

impl NetworkInventory {
    pub fn new(adapters: Vec<AdapterInfo>, generation: u64) -> Self {
        Self {
            adapters,
            generation,
        }
    }

    pub fn find(&self, id: AdapterId) -> Option<&AdapterInfo> {
        self.adapters.iter().find(|a| a.id == id)
    }

    /// Resolve an explicit selection into an immutable lease.
    ///
    /// There is no automatic selection or fallback: `None` is an error and a
    /// saved GUID that is missing/down/no longer has an address is an error.
    /// The caller must ask the user to choose an adapter.
    pub fn resolve(
        &self,
        selection: &AdapterSelection,
        preferred_ip: Option<IpAddr>,
    ) -> Result<NetworkLease, NetError> {
        let AdapterSelection::Adapter(id) = selection else {
            return Err(NetError::NoAdapterSelected);
        };
        let adapter = self.find(*id).ok_or_else(|| NetError::AdapterNotFound {
            adapter: id.to_string(),
        })?;
        if !adapter.is_up() {
            return Err(NetError::AdapterUnavailable {
                adapter: id.to_string(),
            });
        }
        if !adapter.has_address() {
            return Err(NetError::NoAddress {
                adapter: id.to_string(),
            });
        }
        let Some(local_address) = adapter.select_address(preferred_ip) else {
            return Err(NetError::NoAddress {
                adapter: id.to_string(),
            });
        };
        let subnet = SubnetSpec::new(local_address.ip, local_address.prefix)
            .unwrap_or_else(|_| SubnetSpec::host(local_address.ip));
        Ok(NetworkLease {
            generation: self.generation,
            requested: selection.clone(),
            adapter: adapter.clone(),
            local_address,
            subnet,
        })
    }
}

/// Always-available loopback snapshot used when Windows reports no adapters.
pub fn synthetic_loopback() -> AdapterInfo {
    AdapterInfo {
        id: AdapterId::UNSPECIFIED,
        name: String::new(),
        description: String::new(),
        state: AdapterState::Up,
        if_type: 24,
        addresses: vec![
            AdapterAddress {
                ip: IpAddr::V4(Ipv4Addr::LOCALHOST),
                prefix: 8,
                scope_id: None,
            },
            AdapterAddress {
                ip: IpAddr::V6(Ipv6Addr::LOCALHOST),
                prefix: 128,
                scope_id: None,
            },
        ],
        gateways: Vec::new(),
        if_index: None,
        ipv6_if_index: None,
        luid: 0,
        metric: 0,
        is_physical: false,
        is_loopback: true,
    }
}

// ---------------------------------------------------------------------------
// Application-side state
// ---------------------------------------------------------------------------

/// Mutable network state owned by `App`. The pure inventory types above stay
/// free of UI/history concerns.
#[derive(Clone, Debug, Default)]
pub struct NetworkState {
    pub adapters: Vec<AdapterInfo>,
    pub generation: u64,
    pub lease: Option<NetworkLease>,
    pub error: Option<String>,
    pub loaded: bool,
    /// The last explicit inventory refresh failed; the previous cache is kept.
    pub refresh_failed: bool,
}

impl NetworkState {
    /// Load the persisted cache. This is the only startup path; Windows is
    /// never queried here.
    pub fn load_cache(&mut self, cached: &[CachedAdapter]) -> CacheLoadReport {
        let (adapters, report) = load_cached_adapters(cached);
        self.adapters = adapters;
        self.loaded = true;
        self.error = None;
        self.refresh_failed = false;
        report
    }

    /// Replace the runtime snapshots after an explicit «Обновить».
    pub fn set_adapters(&mut self, adapters: Vec<AdapterInfo>) {
        self.adapters = adapters;
        self.loaded = true;
        self.error = None;
        self.refresh_failed = false;
    }

    /// Resolve against the current snapshot, advancing the lease generation.
    /// A successful resolution always clears the previous network error.
    pub fn resolve(&mut self, selection: AdapterSelection, preferred_ip: Option<IpAddr>) {
        self.generation = self.generation.wrapping_add(1).max(1);
        let inventory = NetworkInventory::new(self.adapters.clone(), self.generation);
        match inventory.resolve(&selection, preferred_ip) {
            Ok(lease) => {
                self.lease = Some(lease);
                self.error = None;
            }
            Err(err) => {
                self.lease = None;
                if matches!(err, NetError::NoAdapterSelected) {
                    self.error = None;
                } else {
                    self.error = Some(err.to_string());
                }
            }
        }
    }

    pub fn inventory(&self) -> NetworkInventory {
        NetworkInventory::new(self.adapters.clone(), self.generation)
    }
}
