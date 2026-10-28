use std::net::{IpAddr, Ipv4Addr, Ipv6Addr, SocketAddr, SocketAddrV6};
use std::num::NonZeroU32;

/// Maximum scan size before confirmation.
pub const SCAN_CONFIRM_THRESHOLD: u64 = 4096;

/// Immutable adapter binding snapshot.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct NetworkBinding {
    /// Local source address. Its family selects which resolved remote
    /// addresses this binding may use.
    pub local_ip: IpAddr,
    /// IPv4 interface index (`IfIndex`).
    pub ipv4_if_index: Option<NonZeroU32>,
    /// IPv6 interface index (`Ipv6IfIndex`). Never the IPv4 index.
    pub ipv6_if_index: Option<NonZeroU32>,
    /// Scope id used for IPv6 link-local addresses.
    pub scope_id: Option<NonZeroU32>,
}

impl NetworkBinding {
    pub fn new(
        local_ip: IpAddr,
        ipv4_if_index: Option<NonZeroU32>,
        ipv6_if_index: Option<NonZeroU32>,
        scope_id: Option<NonZeroU32>,
    ) -> Self {
        Self {
            local_ip,
            ipv4_if_index,
            ipv6_if_index,
            scope_id,
        }
    }

    /// Loopback binding for loopback IPv4 traffic; no interface option.
    pub fn loopback() -> Self {
        Self::loopback_for(IpAddr::V4(Ipv4Addr::LOCALHOST))
    }

    /// Loopback binding matching the destination family; no interface option.
    pub fn loopback_for(remote: IpAddr) -> Self {
        let local_ip = if remote.is_ipv6() {
            IpAddr::V6(Ipv6Addr::LOCALHOST)
        } else {
            IpAddr::V4(Ipv4Addr::LOCALHOST)
        };
        Self::new(local_ip, None, None, None)
    }

    pub fn is_loopback(&self) -> bool {
        self.local_ip.is_loopback()
    }

    /// Interface index to apply for a remote address family.
    pub fn interface_index_for(&self, remote: IpAddr) -> Option<NonZeroU32> {
        match remote {
            IpAddr::V4(_) => self.ipv4_if_index,
            IpAddr::V6(_) => self.ipv6_if_index,
        }
    }

    /// Local ephemeral-port socket address for a same-family remote.
    pub fn local_socket_addr(&self, remote: IpAddr) -> Option<SocketAddr> {
        match (self.local_ip, remote) {
            (IpAddr::V4(local), IpAddr::V4(_)) => Some(SocketAddr::new(IpAddr::V4(local), 0)),
            (IpAddr::V6(local), IpAddr::V6(_)) => {
                let scope = if local.is_unicast_link_local() {
                    self.scope_id
                        .or(self.ipv6_if_index)
                        .map_or(0, NonZeroU32::get)
                } else {
                    0
                };
                Some(SocketAddr::V6(SocketAddrV6::new(local, 0, 0, scope)))
            }
            _ => None,
        }
    }

    /// Fill in the scope id for a link-local destination when the resolver did
    /// not provide one.
    #[doc(hidden)]
    pub fn prepare_remote(&self, remote: SocketAddr) -> SocketAddr {
        match remote {
            SocketAddr::V6(mut v6) => {
                if v6.ip().is_unicast_link_local() && v6.scope_id() == 0 {
                    if let Some(scope) = self.scope_id {
                        v6.set_scope_id(scope.get());
                    } else if let Some(index) = self.ipv6_if_index {
                        v6.set_scope_id(index.get());
                    }
                }
                SocketAddr::V6(v6)
            }
            SocketAddr::V4(_) => remote,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
enum SubnetFamily {
    V4,
    V6,
}

/// An IPv4/IPv6 subnet. The network address is always masked.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct SubnetSpec {
    network: IpAddr,
    prefix: u8,
}

impl SubnetSpec {
    pub fn new(ip: IpAddr, prefix: u8) -> Result<Self, NetError> {
        let max = if ip.is_ipv4() { 32 } else { 128 };
        if prefix > max {
            return Err(NetError::InvalidPrefix {
                address: ip.to_string(),
                prefix,
            });
        }
        let network = match ip {
            IpAddr::V4(ip) => {
                let mask = if prefix == 0 {
                    0
                } else {
                    u32::MAX << (32 - u32::from(prefix))
                };
                IpAddr::V4(Ipv4Addr::from(u32::from(ip) & mask))
            }
            IpAddr::V6(ip) => {
                let mask = if prefix == 0 {
                    0
                } else {
                    u128::MAX << (128 - u32::from(prefix))
                };
                IpAddr::V6(Ipv6Addr::from(u128::from(ip) & mask))
            }
        };
        Ok(Self { network, prefix })
    }

    /// A single-host subnet (`/32` or `/128`).
    pub fn host(ip: IpAddr) -> Self {
        let prefix = if ip.is_ipv4() { 32 } else { 128 };
        Self::new(ip, prefix).unwrap_or(Self {
            network: ip,
            prefix,
        })
    }

    pub fn network(&self) -> IpAddr {
        self.network
    }

    pub fn prefix(&self) -> u8 {
        self.prefix
    }

    /// Total number of addresses in the subnet (saturated at `u64::MAX`).
    pub fn capacity(&self) -> u64 {
        match self.network {
            IpAddr::V4(_) => 1u64 << (32 - u32::from(self.prefix)),
            IpAddr::V6(_) => {
                let host_bits = 128 - u32::from(self.prefix);
                if host_bits >= 64 {
                    u64::MAX
                } else {
                    1u64 << host_bits
                }
            }
        }
    }

    /// Usable host count. IPv4 drops network/broadcast for masks up to `/30`;
    /// `/31` keeps both addresses and `/32` keeps the single address.
    pub fn usable_count(&self) -> u64 {
        match self.network {
            IpAddr::V4(_) => match self.prefix {
                32 => 1,
                31 => 2,
                _ => self.capacity().saturating_sub(2),
            },
            IpAddr::V6(_) => self.capacity(),
        }
    }

    /// True when the subnet is larger than the scanner confirmation threshold.
    pub fn needs_confirmation(&self) -> bool {
        self.usable_count() > SCAN_CONFIRM_THRESHOLD
    }

    pub fn contains(&self, ip: IpAddr) -> bool {
        match (self.network, ip) {
            (IpAddr::V4(net), IpAddr::V4(ip)) => {
                let mask = if self.prefix == 0 {
                    0
                } else {
                    u32::MAX << (32 - u32::from(self.prefix))
                };
                (u32::from(net) & mask) == (u32::from(ip) & mask)
            }
            (IpAddr::V6(net), IpAddr::V6(ip)) => {
                let mask = if self.prefix == 0 {
                    0
                } else {
                    u128::MAX << (128 - u32::from(self.prefix))
                };
                (u128::from(net) & mask) == (u128::from(ip) & mask)
            }
            _ => false,
        }
    }

    /// Iterate usable host addresses without allocating the full range.
    pub fn hosts(&self) -> SubnetHosts {
        let family = if self.network.is_ipv4() {
            SubnetFamily::V4
        } else {
            SubnetFamily::V6
        };
        let network = match self.network {
            IpAddr::V4(ip) => u128::from(u32::from(ip)),
            IpAddr::V6(ip) => u128::from(ip),
        };
        let (first, last) = match family {
            SubnetFamily::V4 => {
                let capacity = self.capacity();
                match self.prefix {
                    32 => (network, network),
                    31 => (network, network + 1),
                    _ => (network + 1, network + u128::from(capacity) - 2),
                }
            }
            SubnetFamily::V6 if self.prefix == 128 => (network, network),
            SubnetFamily::V6 => {
                let host_mask = if self.prefix == 0 {
                    u128::MAX
                } else {
                    u128::MAX >> self.prefix
                };
                (network, network | host_mask)
            }
        };
        SubnetHosts {
            family,
            next: first,
            last,
            done: false,
        }
    }

    pub fn hosts_vec(&self, limit: usize) -> Vec<IpAddr> {
        self.hosts().take(limit).collect()
    }
}

pub struct SubnetHosts {
    family: SubnetFamily,
    next: u128,
    last: u128,
    done: bool,
}

impl Iterator for SubnetHosts {
    type Item = IpAddr;

    fn next(&mut self) -> Option<Self::Item> {
        if self.done || self.next > self.last {
            return None;
        }
        let value = self.next;
        if value == self.last {
            self.done = true;
        } else {
            self.next += 1;
        }
        Some(match self.family {
            SubnetFamily::V4 => IpAddr::V4(Ipv4Addr::from(value as u32)),
            SubnetFamily::V6 => IpAddr::V6(Ipv6Addr::from(value)),
        })
    }
}

#[derive(Clone, Debug, PartialEq, Eq, thiserror::Error)]
pub enum NetError {
    #[error("invalid adapter GUID {0:?}")]
    InvalidAdapterId(String),
    #[error("no network adapter selected")]
    NoAdapterSelected,
    #[error("network adapter {adapter} was not found")]
    AdapterNotFound { adapter: String },
    #[error("network adapter {adapter} is unavailable")]
    AdapterUnavailable { adapter: String },
    #[error("adapter {adapter} has no usable address")]
    NoAddress { adapter: String },
    #[error("invalid prefix /{prefix} for {address}")]
    InvalidPrefix { address: String, prefix: u8 },
    #[error("network adapter inventory failed: {0}")]
    Inventory(String),
    #[error("network adapter inventory is not supported on this platform")]
    UnsupportedPlatform,
}

impl NetError {
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::InvalidAdapterId(_) => "invalid_adapter_id",
            Self::NoAdapterSelected => "no_adapter_selected",
            Self::AdapterNotFound { .. } => "adapter_not_found",
            Self::AdapterUnavailable { .. } => "adapter_unavailable",
            Self::NoAddress { .. } => "no_address",
            Self::InvalidPrefix { .. } => "invalid_prefix",
            Self::Inventory(_) => "inventory",
            Self::UnsupportedPlatform => "unsupported_platform",
        }
    }
}
