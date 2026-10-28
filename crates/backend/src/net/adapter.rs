use super::AdapterId;
use crate::bff::net::NetworkBinding;
use serde::{Deserialize, Deserializer, Serialize};
use std::net::{IpAddr, Ipv4Addr};
use std::num::NonZeroU32;

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AdapterState {
    Up,
    Down,
    Testing,
    Dormant,
    NotPresent,
    LowerLayerDown,
    #[default]
    Unknown,
}

impl AdapterState {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Up => "up",
            Self::Down => "down",
            Self::Testing => "testing",
            Self::Dormant => "dormant",
            Self::NotPresent => "not_present",
            Self::LowerLayerDown => "lower_layer_down",
            Self::Unknown => "unknown",
        }
    }

    pub fn is_up(self) -> bool {
        self == Self::Up
    }
}

/// One unicast address of an adapter.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct AdapterAddress {
    pub ip: IpAddr,
    pub prefix: u8,
    /// IPv6 scope id (link-local addresses need it for binding).
    #[serde(default, deserialize_with = "deserialize_scope_id")]
    pub scope_id: Option<NonZeroU32>,
}

impl AdapterAddress {
    pub fn display(&self) -> String {
        match (self.ip, self.scope_id) {
            (IpAddr::V6(ip), Some(scope)) if ip.is_unicast_link_local() => {
                format!("{ip}%{scope}")
            }
            _ => self.ip.to_string(),
        }
    }

    /// Address with its on-link prefix (`192.168.1.20/24`, `fe80::1%12/64`).
    pub fn display_with_prefix(&self) -> String {
        format!("{}/{}", self.display(), self.prefix)
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct AdapterInfo {
    pub id: AdapterId,
    /// Friendly name shown by Windows (may be empty for the synthetic loopback).
    pub name: String,
    pub description: String,
    pub state: AdapterState,
    pub if_type: u32,
    pub addresses: Vec<AdapterAddress>,
    pub gateways: Vec<IpAddr>,
    /// IPv4 interface index (`IfIndex`).
    pub if_index: Option<NonZeroU32>,
    /// IPv6 interface index (`Ipv6IfIndex`); not interchangeable with IPv4.
    pub ipv6_if_index: Option<NonZeroU32>,
    pub luid: u64,
    pub metric: u32,
    pub is_physical: bool,
    pub is_loopback: bool,
}

impl AdapterInfo {
    pub fn is_up(&self) -> bool {
        self.state.is_up()
    }

    pub fn has_address(&self) -> bool {
        !self.addresses.is_empty()
    }

    /// Usable for a lease: link is up and at least one unicast address exists.
    pub fn is_usable(&self) -> bool {
        self.is_up() && self.has_address()
    }

    pub fn ipv4_addresses(&self) -> impl Iterator<Item = AdapterAddress> + '_ {
        self.addresses.iter().copied().filter(|a| a.ip.is_ipv4())
    }

    pub fn ipv6_addresses(&self) -> impl Iterator<Item = AdapterAddress> + '_ {
        self.addresses.iter().copied().filter(|a| a.ip.is_ipv6())
    }

    /// Pick the best local address, honoring an explicit user preference.
    ///
    /// Order: the preferred IP (when present on this adapter), then a global
    /// IPv4 address, any IPv4, a global IPv6 and finally any IPv6. APIPA and
    /// link-local addresses stay at the end of their family.
    pub fn select_address(&self, preferred: Option<IpAddr>) -> Option<AdapterAddress> {
        if let Some(ip) = preferred {
            if let Some(found) = self.addresses.iter().find(|a| a.ip == ip) {
                return Some(*found);
            }
        }
        self.addresses
            .iter()
            .copied()
            .filter(|a| matches!(a.ip, IpAddr::V4(ip) if !is_apipa(ip)))
            .min_by_key(|a| address_rank(*a))
            .or_else(|| {
                self.addresses
                    .iter()
                    .copied()
                    .min_by_key(|a| address_rank(*a))
            })
    }

    /// Binding for the adapter's best local address. Loopback adapters never
    /// carry an interface index, so loopback traffic cannot be pinned to a
    /// physical card.
    pub fn network_binding(&self) -> Option<NetworkBinding> {
        self.select_address(None)
            .map(|address| self.network_binding_for(address))
    }

    /// Binding for one specific address of this adapter.
    pub fn network_binding_for(&self, address: AdapterAddress) -> NetworkBinding {
        if self.is_loopback || address.ip.is_loopback() {
            return NetworkBinding::loopback_for(address.ip);
        }
        let scope_id = match address.ip {
            IpAddr::V6(ip) if ip.is_unicast_link_local() => address.scope_id,
            _ => None,
        };
        NetworkBinding::new(address.ip, self.if_index, self.ipv6_if_index, scope_id)
    }
}

fn is_apipa(ip: Ipv4Addr) -> bool {
    ip.octets()[0] == 169 && ip.octets()[1] == 254
}

fn address_rank(address: AdapterAddress) -> u8 {
    match address.ip {
        IpAddr::V4(ip) if is_apipa(ip) => 4,
        IpAddr::V4(_) => 0,
        IpAddr::V6(ip) if ip.is_unicast_link_local() => 3,
        IpAddr::V6(_) => 2,
    }
}

// ---------------------------------------------------------------------------
// Persisted cache
// ---------------------------------------------------------------------------

pub(super) fn deserialize_scope_id<'de, D: Deserializer<'de>>(
    deserializer: D,
) -> Result<Option<NonZeroU32>, D::Error> {
    Ok(Option::<u32>::deserialize(deserializer)?.and_then(NonZeroU32::new))
}
