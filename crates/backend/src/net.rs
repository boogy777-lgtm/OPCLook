//! Network adapter inventory, selection and subnet models.
//!
//! The app-wide adapter choice is persisted as a GUID in `settings.json`.
//! `NetworkState` turns that preference into an immutable `NetworkLease`: a
//! snapshot of the adapter, the local address, its subnet and a generation
//! that invalidates stale leases after a refresh. Selection rules are pure and
//! testable without touching the Windows API.
//!
//! Selection policy (see `docs/plan-ua-search-network.md`):
//! `None` means no adapter is selected; `Some(GUID)` requires that exact
//! adapter. There is no automatic selection and no fallback. Loopback-only
//! operations do not need a lease and use `NetworkBinding::loopback()`.

use crate::bff::net::NetworkBinding;
use serde::{Deserialize, Deserializer, Serialize, Serializer};
use std::collections::HashSet;
use std::fmt;
use std::net::{IpAddr, Ipv4Addr, Ipv6Addr};

// ---------------------------------------------------------------------------
// Adapter identity
// ---------------------------------------------------------------------------

/// Adapter identity: a 16-byte GUID in canonical textual order.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct AdapterId([u8; 16]);

impl AdapterId {
    /// The all-zero GUID (used by the synthetic loopback adapter).
    pub const UNSPECIFIED: Self = Self([0u8; 16]);

    pub fn from_bytes(bytes: [u8; 16]) -> Self {
        Self(bytes)
    }

    pub fn as_bytes(&self) -> &[u8; 16] {
        &self.0
    }

    pub fn is_unspecified(&self) -> bool {
        self.0 == [0u8; 16]
    }

    /// Parse a GUID with or without braces and with either case.
    pub fn parse(text: &str) -> Result<Self, NetError> {
        let trimmed = text.trim();
        let body = trimmed
            .strip_prefix('{')
            .and_then(|s| s.strip_suffix('}'))
            .unwrap_or(trimmed);
        let mut parts = body.split('-');
        let mut bytes = [0u8; 16];
        let Ok(first) = next_hex(parts.next(), 4) else {
            return Err(NetError::InvalidAdapterId(text.to_string()));
        };
        bytes[0..4].copy_from_slice(&first);
        for range in [4usize..6usize, 6..8, 8..10] {
            let Ok(part) = next_hex(parts.next(), 2) else {
                return Err(NetError::InvalidAdapterId(text.to_string()));
            };
            bytes[range].copy_from_slice(&part);
        }
        let Ok(last) = next_hex(parts.next(), 6) else {
            return Err(NetError::InvalidAdapterId(text.to_string()));
        };
        bytes[10..16].copy_from_slice(&last);
        if parts.next().is_some() {
            return Err(NetError::InvalidAdapterId(text.to_string()));
        }
        Ok(Self(bytes))
    }
}

fn next_hex(part: Option<&str>, bytes: usize) -> Result<Vec<u8>, ()> {
    let Some(part) = part else {
        return Err(());
    };
    if part.len() != bytes * 2 || !part.chars().all(|c| c.is_ascii_hexdigit()) {
        return Err(());
    }
    let mut out = Vec::with_capacity(bytes);
    for i in 0..bytes {
        let hi = part.as_bytes()[i * 2];
        let lo = part.as_bytes()[i * 2 + 1];
        let hi = (hi as char).to_digit(16).ok_or(())?;
        let lo = (lo as char).to_digit(16).ok_or(())?;
        out.push(((hi << 4) | lo) as u8);
    }
    Ok(out)
}

impl fmt::Display for AdapterId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let b = &self.0;
        write!(
            f,
            "{:02x}{:02x}{:02x}{:02x}-{:02x}{:02x}-{:02x}{:02x}-{:02x}{:02x}-{:02x}{:02x}{:02x}{:02x}{:02x}{:02x}",
            b[0],
            b[1],
            b[2],
            b[3],
            b[4],
            b[5],
            b[6],
            b[7],
            b[8],
            b[9],
            b[10],
            b[11],
            b[12],
            b[13],
            b[14],
            b[15]
        )
    }
}

impl Serialize for AdapterId {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        serializer.serialize_str(&self.to_string())
    }
}

impl<'de> Deserialize<'de> for AdapterId {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        let raw = String::deserialize(deserializer)?;
        Self::parse(&raw).map_err(serde::de::Error::custom)
    }
}

/// Which adapter the user requested: exactly one, or none.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub enum AdapterSelection {
    #[default]
    None,
    Adapter(AdapterId),
}

impl AdapterSelection {
    pub fn adapter_id(&self) -> Option<AdapterId> {
        match self {
            Self::None => None,
            Self::Adapter(id) => Some(*id),
        }
    }

    pub fn is_none(&self) -> bool {
        matches!(self, Self::None)
    }
}

// ---------------------------------------------------------------------------
// Adapter snapshot
// ---------------------------------------------------------------------------

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
    #[serde(default)]
    pub scope_id: Option<u32>,
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
    pub if_index: u32,
    /// IPv6 interface index (`Ipv6IfIndex`); not interchangeable with IPv4.
    pub ipv6_if_index: u32,
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
        NetworkBinding::new(
            address.ip,
            (self.if_index != 0).then_some(self.if_index),
            (self.ipv6_if_index != 0).then_some(self.ipv6_if_index),
            scope_id,
        )
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

/// A compact, serializable snapshot of one usable adapter. This is what lives
/// in `settings.json` and is the only source of the adapter dropdown at
/// startup; Windows is not queried until the user presses «Обновить».
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct CachedAdapter {
    pub id: AdapterId,
    #[serde(default)]
    pub name: String,
    #[serde(default)]
    pub description: String,
    #[serde(default)]
    pub state: AdapterState,
    #[serde(default)]
    pub addresses: Vec<AdapterAddress>,
    #[serde(default)]
    pub gateways: Vec<IpAddr>,
    #[serde(default)]
    pub if_index: u32,
    #[serde(default)]
    pub ipv6_if_index: u32,
    #[serde(default)]
    pub luid: u64,
    #[serde(default)]
    pub metric: u32,
    #[serde(default)]
    pub is_physical: bool,
}

impl CachedAdapter {
    pub fn from_info(info: &AdapterInfo) -> Self {
        Self {
            id: info.id,
            name: info.name.clone(),
            description: info.description.clone(),
            state: info.state,
            addresses: info.addresses.clone(),
            gateways: info.gateways.clone(),
            if_index: info.if_index,
            ipv6_if_index: info.ipv6_if_index,
            luid: info.luid,
            metric: info.metric,
            is_physical: info.is_physical,
        }
    }

    /// Rebuild the runtime snapshot used for binding/DA route planning.
    pub fn to_info(&self) -> AdapterInfo {
        AdapterInfo {
            id: self.id,
            name: self.name.clone(),
            description: self.description.clone(),
            state: self.state,
            if_type: 0,
            addresses: self.addresses.clone(),
            gateways: self.gateways.clone(),
            if_index: self.if_index,
            ipv6_if_index: self.ipv6_if_index,
            luid: self.luid,
            metric: self.metric,
            is_physical: self.is_physical,
            is_loopback: false,
        }
    }

    pub fn is_usable(&self) -> bool {
        !self.id.is_unspecified() && self.state.is_up() && !self.addresses.is_empty()
    }

    pub fn select_address(&self, preferred: Option<IpAddr>) -> Option<AdapterAddress> {
        self.to_info().select_address(preferred)
    }
}

/// Why an adapter was not put into the cached/dropdown list.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CacheSkipReason {
    Loopback,
    UnspecifiedId,
    DuplicateId,
    NotUp,
    NoAddress,
}

impl CacheSkipReason {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Loopback => "loopback",
            Self::UnspecifiedId => "unspecified_guid",
            Self::DuplicateId => "duplicate_guid",
            Self::NotUp => "not_up",
            Self::NoAddress => "no_address",
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CacheSkip {
    pub adapter: AdapterInfo,
    pub reason: CacheSkipReason,
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct CacheBuild {
    pub cached: Vec<CachedAdapter>,
    pub skipped: Vec<CacheSkip>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CacheRejection {
    pub id: String,
    pub name: String,
    pub reason: CacheSkipReason,
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct CacheLoadReport {
    pub accepted: usize,
    pub rejected: Vec<CacheRejection>,
}

fn duplicate_ids(adapters: &[AdapterInfo]) -> HashSet<AdapterId> {
    let mut seen = HashSet::new();
    let mut duplicates = HashSet::new();
    for adapter in adapters.iter().filter(|a| !a.id.is_unspecified()) {
        if !seen.insert(adapter.id) {
            duplicates.insert(adapter.id);
        }
    }
    duplicates
}

fn is_loopback_candidate(adapter: &AdapterInfo) -> bool {
    adapter.is_loopback
        || (!adapter.addresses.is_empty() && adapter.addresses.iter().all(|a| a.ip.is_loopback()))
}

/// Replace the persisted cache with the current Windows inventory. Only Up
/// adapters with at least one non-loopback address are kept; every other
/// candidate is returned with a reason so the caller can journal it.
pub fn build_adapter_cache(adapters: &[AdapterInfo]) -> CacheBuild {
    let duplicates = duplicate_ids(adapters);
    let mut build = CacheBuild::default();
    for adapter in adapters {
        let reason = if adapter.id.is_unspecified() {
            Some(CacheSkipReason::UnspecifiedId)
        } else if duplicates.contains(&adapter.id) {
            Some(CacheSkipReason::DuplicateId)
        } else if is_loopback_candidate(adapter) {
            Some(CacheSkipReason::Loopback)
        } else if !adapter.is_up() {
            Some(CacheSkipReason::NotUp)
        } else if !adapter.has_address() {
            Some(CacheSkipReason::NoAddress)
        } else {
            None
        };
        match reason {
            Some(reason) => build.skipped.push(CacheSkip {
                adapter: adapter.clone(),
                reason,
            }),
            None => build.cached.push(CachedAdapter::from_info(adapter)),
        }
    }
    build
}

/// Validate and load the persisted cache. Missing/invalid records are rejected
/// with a reason; duplicate GUIDs are rejected entirely so the dropdown can
/// never contain two rows with the same identity.
pub fn load_cached_adapters(cached: &[CachedAdapter]) -> (Vec<AdapterInfo>, CacheLoadReport) {
    let infos: Vec<AdapterInfo> = cached.iter().map(CachedAdapter::to_info).collect();
    let duplicates = duplicate_ids(&infos);
    let mut adapters = Vec::new();
    let mut report = CacheLoadReport::default();
    for (cached, info) in cached.iter().zip(infos.iter()) {
        let reason = if info.id.is_unspecified() {
            Some(CacheSkipReason::UnspecifiedId)
        } else if duplicates.contains(&info.id) {
            Some(CacheSkipReason::DuplicateId)
        } else if is_loopback_candidate(info) {
            Some(CacheSkipReason::Loopback)
        } else if !cached.is_usable() {
            Some(if !info.is_up() {
                CacheSkipReason::NotUp
            } else {
                CacheSkipReason::NoAddress
            })
        } else {
            None
        };
        match reason {
            Some(reason) => report.rejected.push(CacheRejection {
                id: info.id.to_string(),
                name: info.name.clone(),
                reason,
            }),
            None => adapters.push(info.clone()),
        }
    }
    report.accepted = adapters.len();
    (adapters, report)
}

/// One adapter row is selected only when its GUID is exactly the saved one.
pub fn adapter_is_selected(selection: &AdapterSelection, adapter: &AdapterInfo) -> bool {
    !adapter.id.is_unspecified() && selection.adapter_id() == Some(adapter.id)
}

/// Adapters that may be shown in the dropdown: unique, non-loopback, Up and
/// with at least one address. Duplicate GUIDs collapse to the first row.
pub fn selectable_adapters(adapters: &[AdapterInfo]) -> Vec<&AdapterInfo> {
    let mut seen = HashSet::new();
    adapters
        .iter()
        .filter(|adapter| {
            !adapter.id.is_unspecified()
                && !is_loopback_candidate(adapter)
                && adapter.is_usable()
                && seen.insert(adapter.id)
        })
        .collect()
}

/// The single selected adapter, or `None` for stale/unselected GUIDs. There is
/// no fallback to the first row.
pub fn selected_adapter<'a>(
    adapters: &'a [AdapterInfo],
    selection: &AdapterSelection,
) -> Option<&'a AdapterInfo> {
    let id = selection.adapter_id()?;
    selectable_adapters(adapters)
        .into_iter()
        .find(|adapter| adapter.id == id)
}

/// Number of rows a dropdown would render as selected (0 or 1, never more).
pub fn selected_adapter_count(adapters: &[AdapterInfo], selection: &AdapterSelection) -> usize {
    usize::from(selected_adapter(adapters, selection).is_some())
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SelectionState {
    NotSelected,
    Selected,
    Stale,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SelectionResolution {
    /// Effective selection for a lease (`None` when stale/unselected).
    pub selection: AdapterSelection,
    pub local_ip: Option<IpAddr>,
    pub state: SelectionState,
}

/// Turn a saved selection into an effective one. A saved GUID that is absent
/// (or no longer usable) is stale: no fallback to another adapter and no IP.
/// A saved IP survives only while it belongs to the selected adapter;
/// otherwise the adapter's first suitable address is used.
pub fn reconcile_selection(
    adapters: &[AdapterInfo],
    requested: &AdapterSelection,
    preferred_ip: Option<IpAddr>,
) -> SelectionResolution {
    let Some(id) = requested.adapter_id() else {
        return SelectionResolution {
            selection: AdapterSelection::None,
            local_ip: None,
            state: SelectionState::NotSelected,
        };
    };
    let Some(adapter) = selector_by_id(adapters, id) else {
        return SelectionResolution {
            selection: AdapterSelection::None,
            local_ip: None,
            state: SelectionState::Stale,
        };
    };
    let address = adapter.select_address(preferred_ip);
    SelectionResolution {
        selection: AdapterSelection::Adapter(id),
        local_ip: address.map(|address| address.ip),
        state: SelectionState::Selected,
    }
}

fn selector_by_id(adapters: &[AdapterInfo], id: AdapterId) -> Option<&AdapterInfo> {
    selectable_adapters(adapters)
        .into_iter()
        .find(|adapter| adapter.id == id)
}

// ---------------------------------------------------------------------------
// Subnet model
// ---------------------------------------------------------------------------

pub use opclook_contract::net::{SubnetHosts, SubnetSpec};

// ---------------------------------------------------------------------------
// Lease
// ---------------------------------------------------------------------------

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
        if_index: 0,
        ipv6_if_index: 0,
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

// ---------------------------------------------------------------------------
// Errors
// ---------------------------------------------------------------------------

pub use opclook_contract::net::NetError;

// ---------------------------------------------------------------------------
// Windows inventory
// ---------------------------------------------------------------------------

#[cfg(windows)]
pub fn inventory_adapters() -> Result<Vec<AdapterInfo>, NetError> {
    windows_inventory::read_adapters()
}

#[cfg(not(windows))]
pub fn inventory_adapters() -> Result<Vec<AdapterInfo>, NetError> {
    Err(NetError::UnsupportedPlatform)
}

#[cfg(windows)]
mod windows_inventory {
    #![deny(clippy::undocumented_unsafe_blocks)]
    use super::*;
    use std::mem::size_of;
    use windows::Win32::Foundation::{ERROR_BUFFER_OVERFLOW, ERROR_NO_DATA, ERROR_SUCCESS};
    use windows::Win32::NetworkManagement::IpHelper::{
        GAA_FLAG_INCLUDE_GATEWAYS, GAA_FLAG_INCLUDE_PREFIX, GAA_FLAG_SKIP_ANYCAST,
        GAA_FLAG_SKIP_DNS_SERVER, GAA_FLAG_SKIP_MULTICAST, GetAdaptersAddresses, GetIfEntry2,
        IF_TYPE_ETHERNET_CSMACD, IF_TYPE_GIGABITETHERNET, IF_TYPE_IEEE1394, IF_TYPE_IEEE80211,
        IF_TYPE_SOFTWARE_LOOPBACK, IF_TYPE_WWANPP, IF_TYPE_WWANPP2, IP_ADAPTER_ADDRESSES_LH,
        MIB_IF_ROW2,
    };
    use windows::Win32::NetworkManagement::Ndis::{
        IfOperStatusDormant, IfOperStatusDown, IfOperStatusLowerLayerDown, IfOperStatusNotPresent,
        IfOperStatusTesting, IfOperStatusUnknown, IfOperStatusUp, NET_LUID_LH,
    };
    use windows::Win32::Networking::WinSock::{
        AF_INET, AF_INET6, SOCKADDR_IN, SOCKADDR_IN6, SOCKET_ADDRESS,
    };

    pub fn read_adapters() -> Result<Vec<AdapterInfo>, NetError> {
        let flags = GAA_FLAG_INCLUDE_PREFIX
            | GAA_FLAG_INCLUDE_GATEWAYS
            | GAA_FLAG_SKIP_ANYCAST
            | GAA_FLAG_SKIP_MULTICAST
            | GAA_FLAG_SKIP_DNS_SERVER;
        let mut size = 0u32;
        // SAFETY: the null buffer requests only the size in writable storage.
        let first = unsafe { GetAdaptersAddresses(0, flags, None, None, &mut size) };
        if first != ERROR_SUCCESS.0 && first != ERROR_BUFFER_OVERFLOW.0 {
            return Err(NetError::Inventory(format!(
                "GetAdaptersAddresses size query failed with {first}"
            )));
        }
        if size == 0 {
            return Ok(Vec::new());
        }
        let item = size_of::<IP_ADAPTER_ADDRESSES_LH>();
        let count = (size as usize + item - 1) / item;
        let mut buffer = vec![IP_ADAPTER_ADDRESSES_LH::default(); count];
        let mut size = (buffer.len() * item) as u32;
        // SAFETY: buffer is suitably aligned and contains size writable bytes.
        let result =
            unsafe { GetAdaptersAddresses(0, flags, None, Some(buffer.as_mut_ptr()), &mut size) };
        if result == ERROR_NO_DATA.0 {
            return Ok(Vec::new());
        }
        if result != ERROR_SUCCESS.0 {
            return Err(NetError::Inventory(format!(
                "GetAdaptersAddresses failed with {result}"
            )));
        }

        let mut adapters = Vec::new();
        let mut current = buffer.as_ptr();
        while !current.is_null() {
            // SAFETY: the successful API call returned this live linked entry.
            let raw = unsafe { &*current };
            // SAFETY: all nested pointers refer to the live API output buffer.
            adapters.push(unsafe { from_raw_adapter(raw) });
            current = raw.Next;
        }
        adapters.sort_by(|a, b| {
            a.name
                .to_ascii_lowercase()
                .cmp(&b.name.to_ascii_lowercase())
                .then_with(|| a.id.cmp(&b.id))
        });
        Ok(adapters)
    }

    unsafe fn from_raw_adapter(raw: &IP_ADAPTER_ADDRESSES_LH) -> AdapterInfo {
        // SAFETY: adapter names are nullable terminated strings in the API buffer.
        let name = unsafe { crate::winutil::wide_to_string(raw.FriendlyName) };
        // SAFETY: the description is a nullable terminated API string.
        let description = unsafe { crate::winutil::wide_to_string(raw.Description) };
        let mut addresses = Vec::new();
        let mut unicast = raw.FirstUnicastAddress;
        while !unicast.is_null() {
            // SAFETY: the API supplies this linked unicast entry in its live buffer.
            let entry = unsafe { &*unicast };
            // SAFETY: this socket address belongs to the live API output buffer.
            if let Some((ip, scope_id)) = unsafe { socket_ip(entry.Address) } {
                addresses.push(AdapterAddress {
                    ip,
                    prefix: entry.OnLinkPrefixLength,
                    scope_id,
                });
            }
            unicast = entry.Next;
        }
        let mut gateways = Vec::new();
        let mut gateway = raw.FirstGatewayAddress;
        while !gateway.is_null() {
            // SAFETY: the API supplies this linked gateway entry in its live buffer.
            let entry = unsafe { &*gateway };
            // SAFETY: the address belongs to the live API output buffer.
            if let Some((ip, _)) = unsafe { socket_ip(entry.Address) } {
                gateways.push(ip);
            }
            gateway = entry.Next;
        }
        // SAFETY: GetAdaptersAddresses initialized the adapter index union.
        let if_index = unsafe { raw.Anonymous1.Anonymous.IfIndex };
        let ipv6_if_index = raw.Ipv6IfIndex;
        // SAFETY: the initialized LUID union is read as its integer representation.
        let luid = unsafe { raw.Luid.Value };
        let state = map_oper_status(raw.OperStatus);
        let is_loopback = raw.IfType == IF_TYPE_SOFTWARE_LOOPBACK
            || (addresses.iter().all(|a| a.ip.is_loopback()) && !addresses.is_empty())
            || looks_like_loopback(&name, &description);
        let is_physical =
            !is_loopback && physical_if_type(raw.IfType) && !looks_virtual(&name, &description);
        // `NetworkGuid` is the network-profile identity, not the adapter
        // identity. It can be shared by several adapters (and produced the
        // duplicate-GUID cache that hid every adapter). `AdapterName` is the
        // adapter connection GUID, which is stable and unique.
        let id = if raw.AdapterName.is_null() {
            AdapterId::UNSPECIFIED
        } else {
            // SAFETY: a non-null adapter name is terminated in the API buffer.
            unsafe { raw.AdapterName.to_string() }
                .ok()
                .and_then(|name| AdapterId::parse(&name).ok())
                .unwrap_or(AdapterId::UNSPECIFIED)
        };
        let mut info = AdapterInfo {
            id,
            name,
            description,
            state,
            if_type: raw.IfType,
            addresses,
            gateways,
            if_index,
            ipv6_if_index,
            luid,
            metric: effective_metric(raw.Ipv4Metric, raw.Ipv6Metric),
            is_physical,
            is_loopback,
        };
        enrich_from_if_entry(&mut info);
        info
    }

    unsafe fn socket_ip(address: SOCKET_ADDRESS) -> Option<(IpAddr, Option<u32>)> {
        if address.lpSockaddr.is_null() {
            return None;
        }
        if address.iSockaddrLength
            < size_of::<windows::Win32::Networking::WinSock::ADDRESS_FAMILY>() as i32
        {
            return None;
        }
        // SAFETY: the API supplied an aligned address with room for its family.
        let family = unsafe { (*address.lpSockaddr).sa_family };
        if family == AF_INET {
            if address.iSockaddrLength < size_of::<SOCKADDR_IN>() as i32 {
                return None;
            }
            // SAFETY: family and length identify a complete aligned IPv4 sockaddr.
            let sin = unsafe { &*(address.lpSockaddr as *const SOCKADDR_IN) };
            // SAFETY: an IPv4 sockaddr initializes this address union member.
            let raw = unsafe { sin.sin_addr.S_un.S_addr };
            let ip = Ipv4Addr::from(u32::from_be(raw));
            Some((IpAddr::V4(ip), None))
        } else if family == AF_INET6 {
            if address.iSockaddrLength < size_of::<SOCKADDR_IN6>() as i32 {
                return None;
            }
            // SAFETY: family and length identify a complete aligned IPv6 sockaddr.
            let sin6 = unsafe { &*(address.lpSockaddr as *const SOCKADDR_IN6) };
            // SAFETY: an IPv6 sockaddr initializes its address union.
            let bytes = unsafe { sin6.sin6_addr.u.Byte };
            // SAFETY: the scope union is initialized by the socket address API.
            let scope = unsafe { sin6.Anonymous.sin6_scope_id };
            Some((IpAddr::V6(Ipv6Addr::from(bytes)), Some(scope)))
        } else {
            None
        }
    }

    fn enrich_from_if_entry(info: &mut AdapterInfo) {
        let mut row = MIB_IF_ROW2::default();
        row.InterfaceLuid = NET_LUID_LH { Value: info.luid };
        // SAFETY: row contains the selected LUID and is writable for the API.
        let result = unsafe { GetIfEntry2(&mut row) };
        if result == ERROR_SUCCESS {
            info.state = map_oper_status(row.OperStatus);
        }
    }

    fn map_oper_status(
        status: windows::Win32::NetworkManagement::Ndis::IF_OPER_STATUS,
    ) -> AdapterState {
        if status == IfOperStatusUp {
            AdapterState::Up
        } else if status == IfOperStatusDown {
            AdapterState::Down
        } else if status == IfOperStatusTesting {
            AdapterState::Testing
        } else if status == IfOperStatusDormant {
            AdapterState::Dormant
        } else if status == IfOperStatusNotPresent {
            AdapterState::NotPresent
        } else if status == IfOperStatusLowerLayerDown {
            AdapterState::LowerLayerDown
        } else if status == IfOperStatusUnknown {
            AdapterState::Unknown
        } else {
            AdapterState::Unknown
        }
    }

    fn effective_metric(v4: u32, v6: u32) -> u32 {
        match (v4, v6) {
            (0, 0) => 0,
            (0, v6) => v6,
            (v4, 0) => v4,
            (v4, v6) => v4.min(v6),
        }
    }

    fn physical_if_type(if_type: u32) -> bool {
        matches!(
            if_type,
            IF_TYPE_ETHERNET_CSMACD
                | IF_TYPE_GIGABITETHERNET
                | IF_TYPE_IEEE80211
                | IF_TYPE_IEEE1394
                | IF_TYPE_WWANPP
                | IF_TYPE_WWANPP2
        )
    }

    fn looks_virtual(name: &str, description: &str) -> bool {
        const VIRTUAL_MARKERS: &[&str] = &[
            "virtual",
            "vpn",
            "tap",
            "tun",
            "wireguard",
            "openvpn",
            "hyper-v",
            "vmware",
            "virtualbox",
            "loopback",
            "pseudo",
            "docker",
            "wsl",
            "tailscale",
            "zerotier",
            "npcap",
        ];
        let haystack = format!("{} {}", name, description).to_ascii_lowercase();
        VIRTUAL_MARKERS
            .iter()
            .any(|marker| haystack.contains(marker))
    }

    fn looks_like_loopback(name: &str, description: &str) -> bool {
        let haystack = format!("{} {}", name, description).to_ascii_lowercase();
        haystack.contains("loopback")
    }

    #[cfg(test)]
    mod tests {
        use super::*;

        #[test]
        fn null_adapter_name_and_short_socket_address_are_safe() {
            let raw = IP_ADAPTER_ADDRESSES_LH::default();
            // SAFETY: all nullable fields of the initialized empty structure are null.
            let info = unsafe { from_raw_adapter(&raw) };
            assert_eq!(info.id, AdapterId::UNSPECIFIED);
            let mut address = SOCKADDR_IN::default();
            let socket = SOCKET_ADDRESS {
                lpSockaddr: (&raw mut address).cast(),
                iSockaddrLength: 0,
            };
            // SAFETY: the pointer refers to live aligned storage; length is intentionally short.
            assert!(unsafe { socket_ip(socket) }.is_none());
        }

        #[test]
        fn physical_and_virtual_interface_classification() {
            assert!(physical_if_type(IF_TYPE_ETHERNET_CSMACD));
            assert!(physical_if_type(IF_TYPE_IEEE80211));
            assert!(!physical_if_type(IF_TYPE_SOFTWARE_LOOPBACK));
            assert!(!physical_if_type(
                windows::Win32::NetworkManagement::IpHelper::IF_TYPE_TUNNEL
            ));

            assert!(looks_virtual(
                "vEthernet (Default Switch)",
                "Hyper-V Virtual Ethernet Adapter"
            ));
            assert!(looks_virtual("Ethernet 3", "TAP-Windows Adapter V9"));
            assert!(looks_virtual("VPN", "WireGuard Tunnel"));
            assert!(!looks_virtual(
                "Ethernet",
                "Intel(R) Ethernet Connection I219-LM"
            ));
            assert!(!looks_virtual("Wi-Fi", "Intel(R) Wi-Fi 6 AX201 160MHz"));

            assert!(looks_like_loopback("Loopback Pseudo-Interface 1", ""));
            assert!(!looks_like_loopback("Ethernet", "Intel Ethernet"));
        }

        #[test]
        fn effective_metric_ignores_zero_entries() {
            assert_eq!(effective_metric(0, 0), 0);
            assert_eq!(effective_metric(0, 25), 25);
            assert_eq!(effective_metric(10, 0), 10);
            assert_eq!(effective_metric(25, 5), 5);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn addr(ip: &str, prefix: u8) -> AdapterAddress {
        AdapterAddress {
            ip: ip.parse().unwrap(),
            prefix,
            scope_id: None,
        }
    }

    fn adapter(
        id: AdapterId,
        name: &str,
        is_physical: bool,
        is_loopback: bool,
        up: bool,
        addresses: Vec<AdapterAddress>,
    ) -> AdapterInfo {
        AdapterInfo {
            id,
            name: name.to_string(),
            description: String::new(),
            state: if up {
                AdapterState::Up
            } else {
                AdapterState::Down
            },
            if_type: if is_loopback { 24 } else { 6 },
            addresses,
            gateways: Vec::new(),
            if_index: 1,
            ipv6_if_index: 2,
            luid: 1,
            metric: 10,
            is_physical,
            is_loopback,
        }
    }

    fn id(text: &str) -> AdapterId {
        AdapterId::parse(text).unwrap()
    }

    #[test]
    fn adapter_id_parses_and_formats_canonically() {
        let text = "AABBCCDD-1122-3344-5566-77889900AABB";
        let parsed = id(text);
        assert_eq!(parsed.to_string(), "aabbccdd-1122-3344-5566-77889900aabb");
        let braced = AdapterId::parse("{aabbccdd-1122-3344-5566-77889900aabb}").unwrap();
        assert_eq!(parsed, braced);
        assert!(!parsed.is_unspecified());
        let json = serde_json::to_string(&parsed).unwrap();
        let back: AdapterId = serde_json::from_str(&json).unwrap();
        assert_eq!(back, parsed);
    }

    #[test]
    fn adapter_id_rejects_malformed_values_without_panicking() {
        for raw in [
            "",
            "not-a-guid",
            "aabbccdd-1122-3344-5566",
            "zzbcccdd-1122-3344-5566-77889900aabb",
        ] {
            assert!(AdapterId::parse(raw).is_err(), "accepted {raw:?}");
        }
        assert!(AdapterId::default().is_unspecified());
    }

    #[test]
    fn subnet_sizes_and_hosts_for_common_masks() {
        let s23 = SubnetSpec::new("192.168.2.10".parse().unwrap(), 23).unwrap();
        assert_eq!(s23.network().to_string(), "192.168.2.0");
        assert_eq!(s23.capacity(), 512);
        assert_eq!(s23.usable_count(), 510);
        assert_eq!(s23.hosts().count(), 510);
        assert!(s23.contains("192.168.3.200".parse().unwrap()));
        assert!(!s23.contains("192.168.4.1".parse().unwrap()));
        assert!(!s23.needs_confirmation());

        let s24 = SubnetSpec::new("10.0.0.7".parse().unwrap(), 24).unwrap();
        assert_eq!(s24.capacity(), 256);
        assert_eq!(s24.usable_count(), 254);
        assert_eq!(s24.hosts().next().unwrap().to_string(), "10.0.0.1");
        assert_eq!(s24.hosts().last().unwrap().to_string(), "10.0.0.254");
    }

    #[test]
    fn subnet_31_and_32_keep_all_addresses() {
        let s31 = SubnetSpec::new("10.0.0.4".parse().unwrap(), 31).unwrap();
        assert_eq!(s31.capacity(), 2);
        assert_eq!(s31.usable_count(), 2);
        assert_eq!(s31.hosts().collect::<Vec<_>>().len(), 2);
        assert!(s31.contains("10.0.0.5".parse().unwrap()));

        let s32 = SubnetSpec::new("10.0.0.4".parse().unwrap(), 32).unwrap();
        assert_eq!(s32.capacity(), 1);
        assert_eq!(s32.usable_count(), 1);
        assert_eq!(s32.hosts().collect::<Vec<_>>().len(), 1);
    }

    #[test]
    fn subnet_ipv6_and_huge_ranges_do_not_enumerate_eagerly() {
        let s64 = SubnetSpec::new("fe80::1".parse().unwrap(), 64).unwrap();
        assert_eq!(s64.capacity(), u64::MAX);
        assert!(s64.needs_confirmation());
        assert!(s64.contains("fe80::abcd".parse().unwrap()));
        assert_eq!(s64.hosts().take(3).count(), 3);

        let s128 = SubnetSpec::host("::1".parse().unwrap());
        assert_eq!(s128.capacity(), 1);
        assert_eq!(s128.usable_count(), 1);
        assert_eq!(
            s128.hosts().collect::<Vec<_>>(),
            vec!["::1".parse::<IpAddr>().unwrap()]
        );

        let tail = SubnetSpec::new(
            "ffff:ffff:ffff:ffff:ffff:ffff:ffff:fffe".parse().unwrap(),
            127,
        )
        .unwrap();
        assert_eq!(tail.hosts().collect::<Vec<_>>().len(), 2);
    }

    #[test]
    fn invalid_prefix_is_an_error_not_a_panic() {
        assert!(SubnetSpec::new("10.0.0.1".parse().unwrap(), 33).is_err());
        assert!(SubnetSpec::new("::1".parse().unwrap(), 129).is_err());
    }

    #[test]
    fn none_selection_is_an_explicit_error_without_fallback() {
        let physical = id("11111111-1111-1111-1111-111111111111");
        let inv = NetworkInventory::new(
            vec![adapter(
                physical,
                "Ethernet",
                true,
                false,
                true,
                vec![addr("192.168.1.20", 24)],
            )],
            7,
        );
        assert_eq!(
            inv.resolve(&AdapterSelection::None, None),
            Err(NetError::NoAdapterSelected)
        );
    }

    #[test]
    fn manual_selection_is_used_when_available() {
        let selected = id("11111111-1111-1111-1111-111111111111");
        let inv = NetworkInventory::new(
            vec![adapter(
                selected,
                "Ethernet",
                true,
                false,
                true,
                vec![addr("192.168.1.20", 24)],
            )],
            7,
        );
        let lease = inv
            .resolve(&AdapterSelection::Adapter(selected), None)
            .unwrap();
        assert_eq!(lease.generation, 7);
        assert_eq!(lease.adapter.id, selected);
        assert_eq!(lease.local_ip().to_string(), "192.168.1.20");
        assert_eq!(lease.requested, AdapterSelection::Adapter(selected));
    }

    #[test]
    fn missing_or_down_adapter_is_an_error_not_a_fallback() {
        let missing = id("22222222-2222-2222-2222-222222222222");
        let down = id("55555555-5555-5555-5555-555555555555");
        let usable = id("66666666-6666-6666-6666-666666666666");
        let inv = NetworkInventory::new(
            vec![
                adapter(
                    down,
                    "Ethernet 2",
                    true,
                    false,
                    false,
                    vec![addr("192.168.2.10", 24)],
                ),
                adapter(
                    usable,
                    "Ethernet",
                    true,
                    false,
                    true,
                    vec![addr("192.168.1.20", 24)],
                ),
            ],
            4,
        );
        assert_eq!(
            inv.resolve(&AdapterSelection::Adapter(missing), None),
            Err(NetError::AdapterNotFound {
                adapter: missing.to_string()
            })
        );
        assert_eq!(
            inv.resolve(&AdapterSelection::Adapter(down), None),
            Err(NetError::AdapterUnavailable {
                adapter: down.to_string()
            })
        );
    }

    #[test]
    fn preferred_ip_is_used_when_present() {
        let selected = id("11111111-1111-1111-1111-111111111111");
        let inv = NetworkInventory::new(
            vec![adapter(
                selected,
                "Ethernet",
                true,
                false,
                true,
                vec![addr("192.168.1.20", 24), addr("192.168.1.21", 24)],
            )],
            1,
        );
        let preferred: IpAddr = "192.168.1.21".parse().unwrap();
        let lease = inv
            .resolve(&AdapterSelection::Adapter(selected), Some(preferred))
            .unwrap();
        assert_eq!(lease.local_ip(), preferred);
        assert_eq!(lease.subnet.prefix(), 24);
    }

    #[test]
    fn address_preference_avoids_apipa_and_uses_ipv4_first() {
        let selected = id("aaaaaaaa-aaaa-aaaa-aaaa-aaaaaaaaaaaa");
        let info = adapter(
            selected,
            "Ethernet",
            true,
            false,
            true,
            vec![
                addr("fe80::1", 64),
                addr("169.254.10.10", 16),
                addr("192.168.1.20", 24),
            ],
        );
        let selected_address = info.select_address(None).unwrap();
        assert_eq!(selected_address.ip.to_string(), "192.168.1.20");
    }

    #[test]
    fn network_state_advances_generation_and_clears_lease_without_selection() {
        let selected = id("bbbbbbbb-bbbb-bbbb-bbbb-bbbbbbbbbbbb");
        let mut state = NetworkState::default();
        state.adapters = vec![adapter(
            selected,
            "Ethernet",
            true,
            false,
            true,
            vec![addr("192.168.1.20", 24)],
        )];
        state.resolve(AdapterSelection::None, None);
        let first = state.generation;
        assert!(state.lease.is_none());
        assert!(state.error.is_none());
        state.resolve(AdapterSelection::Adapter(selected), None);
        assert!(state.generation > first);
        assert!(state.lease.is_some());
        assert!(state.error.is_none());
    }

    #[test]
    fn successful_resolve_clears_a_previous_network_error() {
        let selected = id("01010101-0101-0101-0101-010101010101");
        let missing = id("02020202-0202-0202-0202-020202020202");
        let mut state = NetworkState::default();
        state.set_adapters(vec![adapter(
            selected,
            "Ethernet",
            true,
            false,
            true,
            vec![addr("192.168.1.20", 24)],
        )]);
        state.resolve(AdapterSelection::Adapter(missing), None);
        assert!(
            state.error.is_some(),
            "missing adapter must report an error"
        );
        state.resolve(AdapterSelection::Adapter(selected), None);
        assert!(state.lease.is_some());
        assert!(state.error.is_none(), "successful resolve clears the error");
    }

    #[test]
    fn build_adapter_cache_replaces_and_filters_inventory() {
        let up = id("10000000-0000-0000-0000-000000000001");
        let down = id("10000000-0000-0000-0000-000000000002");
        let no_address = id("10000000-0000-0000-0000-000000000003");
        let loopback = id("10000000-0000-0000-0000-000000000004");
        let duplicate = id("10000000-0000-0000-0000-000000000005");
        let inventory = vec![
            adapter(
                up,
                "Ethernet",
                true,
                false,
                true,
                vec![addr("192.168.1.20", 24)],
            ),
            adapter(
                down,
                "Ethernet 2",
                true,
                false,
                false,
                vec![addr("10.0.0.2", 24)],
            ),
            adapter(no_address, "VPN", false, false, true, Vec::new()),
            adapter(
                loopback,
                "Loopback",
                false,
                true,
                true,
                vec![addr("127.0.0.1", 8)],
            ),
            adapter(
                duplicate,
                "Duplicate A",
                true,
                false,
                true,
                vec![addr("172.16.0.2", 24)],
            ),
            adapter(
                duplicate,
                "Duplicate B",
                true,
                false,
                true,
                vec![addr("172.16.0.3", 24)],
            ),
        ];
        let build = build_adapter_cache(&inventory);
        assert_eq!(
            build.cached.len(),
            1,
            "only the usable unique adapter remains"
        );
        assert_eq!(build.cached[0].id, up);
        let reasons: Vec<CacheSkipReason> = build.skipped.iter().map(|skip| skip.reason).collect();
        assert!(reasons.contains(&CacheSkipReason::NotUp));
        assert!(reasons.contains(&CacheSkipReason::NoAddress));
        assert!(reasons.contains(&CacheSkipReason::Loopback));
        assert_eq!(
            reasons
                .iter()
                .filter(|reason| **reason == CacheSkipReason::DuplicateId)
                .count(),
            2
        );

        // A second refresh fully replaces the cache; it is never a union.
        let replacement = build_adapter_cache(&[adapter(
            no_address,
            "Ethernet 3",
            true,
            false,
            true,
            vec![addr("192.168.5.10", 24)],
        )]);
        assert_eq!(replacement.cached.len(), 1);
        assert_eq!(replacement.cached[0].id, no_address);
        assert!(replacement.cached.iter().all(|item| item.id != up));
    }

    #[test]
    fn load_cached_adapters_rejects_duplicates_unspecified_down_and_no_address() {
        let up = id("20000000-0000-0000-0000-000000000001");
        let down = id("20000000-0000-0000-0000-000000000002");
        let no_address = id("20000000-0000-0000-0000-000000000003");
        let duplicate = id("20000000-0000-0000-0000-000000000004");
        let cached = vec![
            CachedAdapter::from_info(&adapter(
                up,
                "Ethernet",
                true,
                false,
                true,
                vec![addr("192.168.1.20", 24)],
            )),
            CachedAdapter::from_info(&adapter(
                duplicate,
                "Duplicate A",
                true,
                false,
                true,
                vec![addr("10.0.0.2", 24)],
            )),
            CachedAdapter::from_info(&adapter(
                duplicate,
                "Duplicate B",
                true,
                false,
                true,
                vec![addr("10.0.0.3", 24)],
            )),
            CachedAdapter::from_info(&adapter(
                down,
                "Ethernet 2",
                true,
                false,
                false,
                vec![addr("10.0.0.4", 24)],
            )),
            CachedAdapter::from_info(&adapter(no_address, "VPN", false, false, true, Vec::new())),
            CachedAdapter::from_info(&adapter(
                AdapterId::UNSPECIFIED,
                "Loopback",
                false,
                true,
                true,
                vec![addr("127.0.0.1", 8)],
            )),
        ];
        let (adapters, report) = load_cached_adapters(&cached);
        assert_eq!(adapters.len(), 1);
        assert_eq!(adapters[0].id, up);
        assert_eq!(report.accepted, 1);
        assert_eq!(report.rejected.len(), 5);
    }

    #[test]
    fn adapter_cache_loads_without_windows_and_keeps_ipv6_scope() {
        let selected = id("30000000-0000-0000-0000-000000000001");
        let mut link_local = addr("fe80::10", 64);
        link_local.scope_id = Some(12);
        let cached = CachedAdapter::from_info(&adapter(
            selected,
            "Ethernet",
            true,
            false,
            true,
            vec![link_local],
        ));
        let mut state = NetworkState::default();
        let report = state.load_cache(&[cached]);
        assert_eq!(report.accepted, 1);
        assert!(state.loaded);
        assert_eq!(state.adapters.len(), 1);
        let resolution = reconcile_selection(
            &state.adapters,
            &AdapterSelection::Adapter(selected),
            Some("fe80::10".parse().unwrap()),
        );
        assert_eq!(resolution.state, SelectionState::Selected);
        state.resolve(resolution.selection, resolution.local_ip);
        let lease = state.lease.expect("lease");
        assert_eq!(lease.local_ip().to_string(), "fe80::10");
        assert_eq!(lease.network_binding().scope_id, Some(12));
    }

    #[test]
    fn stale_guid_has_zero_selected_rows_and_no_fallback() {
        let available = id("40000000-0000-0000-0000-000000000001");
        let stale = id("40000000-0000-0000-0000-000000000002");
        let adapters = vec![adapter(
            available,
            "Ethernet",
            true,
            false,
            true,
            vec![addr("192.168.1.20", 24)],
        )];
        let selection = AdapterSelection::Adapter(stale);
        assert_eq!(selected_adapter_count(&adapters, &selection), 0);
        assert!(selected_adapter(&adapters, &selection).is_none());
        let resolution =
            reconcile_selection(&adapters, &selection, Some("10.0.0.9".parse().unwrap()));
        assert_eq!(resolution.state, SelectionState::Stale);
        assert_eq!(resolution.selection, AdapterSelection::None);
        assert!(resolution.local_ip.is_none());
    }

    #[test]
    fn exactly_one_row_is_selected_even_with_duplicate_guids() {
        let id_a = id("50000000-0000-0000-0000-000000000001");
        let id_b = id("50000000-0000-0000-0000-000000000002");
        let adapters = vec![
            adapter(
                id_a,
                "Ethernet",
                true,
                false,
                true,
                vec![addr("192.168.1.20", 24)],
            ),
            adapter(
                id_a,
                "Ethernet duplicate",
                true,
                false,
                true,
                vec![addr("192.168.1.21", 24)],
            ),
            adapter(id_b, "Wi-Fi", true, false, true, vec![addr("10.0.0.5", 24)]),
        ];
        assert_eq!(selectable_adapters(&adapters).len(), 2);
        assert_eq!(
            selected_adapter_count(&adapters, &AdapterSelection::Adapter(id_a)),
            1
        );
        assert_eq!(
            selected_adapter_count(&adapters, &AdapterSelection::Adapter(id_b)),
            1
        );
        assert_eq!(
            selected_adapter_count(&adapters, &AdapterSelection::None),
            0
        );
        assert_eq!(
            selected_adapter_count(
                &adapters,
                &AdapterSelection::Adapter(AdapterId::UNSPECIFIED)
            ),
            0
        );
    }

    #[test]
    fn ip_must_belong_to_the_selected_adapter() {
        let ethernet = id("60000000-0000-0000-0000-000000000001");
        let wifi = id("60000000-0000-0000-0000-000000000002");
        let adapters = vec![
            adapter(
                ethernet,
                "Ethernet",
                true,
                false,
                true,
                vec![addr("192.168.1.20", 24)],
            ),
            adapter(wifi, "Wi-Fi", true, false, true, vec![addr("10.0.0.5", 24)]),
        ];
        // A foreign IP is ignored and the selected adapter's first address wins.
        let a = reconcile_selection(
            &adapters,
            &AdapterSelection::Adapter(ethernet),
            Some("10.0.0.5".parse().unwrap()),
        );
        assert_eq!(a.local_ip, Some("192.168.1.20".parse().unwrap()));
        let b = reconcile_selection(
            &adapters,
            &AdapterSelection::Adapter(wifi),
            Some("192.168.1.20".parse().unwrap()),
        );
        assert_eq!(b.local_ip, Some("10.0.0.5".parse().unwrap()));
        // A saved IP that belongs to the selected adapter is preserved.
        let keep = reconcile_selection(
            &adapters,
            &AdapterSelection::Adapter(ethernet),
            Some("192.168.1.20".parse().unwrap()),
        );
        assert_eq!(keep.local_ip, Some("192.168.1.20".parse().unwrap()));
    }

    #[test]
    fn multiple_addresses_select_ip_and_matching_subnet() {
        let selected = id("70000000-0000-0000-0000-000000000001");
        let adapters = vec![adapter(
            selected,
            "Ethernet",
            true,
            false,
            true,
            vec![addr("192.168.1.20", 24), addr("192.168.2.30", 16)],
        )];
        let resolution = reconcile_selection(
            &adapters,
            &AdapterSelection::Adapter(selected),
            Some("192.168.2.30".parse().unwrap()),
        );
        assert_eq!(resolution.local_ip, Some("192.168.2.30".parse().unwrap()));
        let lease = NetworkInventory::new(adapters, 1)
            .resolve(&resolution.selection, resolution.local_ip)
            .unwrap();
        assert_eq!(lease.local_ip().to_string(), "192.168.2.30");
        assert_eq!(lease.subnet.prefix(), 16);
    }

    #[test]
    fn loopback_is_not_a_selectable_adapter() {
        let loopback = id("80000000-0000-0000-0000-000000000001");
        let adapters = vec![adapter(
            loopback,
            "Loopback",
            false,
            true,
            true,
            vec![addr("127.0.0.1", 8)],
        )];
        assert!(selectable_adapters(&adapters).is_empty());
        let resolution = reconcile_selection(&adapters, &AdapterSelection::Adapter(loopback), None);
        assert_eq!(resolution.state, SelectionState::Stale);
    }

    #[test]
    fn empty_cache_yields_no_selection() {
        let adapters: Vec<AdapterInfo> = Vec::new();
        assert!(selectable_adapters(&adapters).is_empty());
        let resolution = reconcile_selection(&adapters, &AdapterSelection::None, None);
        assert_eq!(resolution.state, SelectionState::NotSelected);
        assert!(resolution.local_ip.is_none());
    }

    #[test]
    fn windows_inventory_is_non_panicking_without_a_selection() {
        let inventory = match inventory_adapters() {
            Ok(adapters) => {
                let mut ids = HashSet::new();
                for adapter in &adapters {
                    assert!(
                        !adapter.id.is_unspecified(),
                        "adapter {:?} has no connection GUID",
                        adapter.name
                    );
                    assert!(
                        ids.insert(adapter.id),
                        "duplicate adapter GUID {} for {:?}",
                        adapter.id,
                        adapter.name
                    );
                }
                NetworkInventory::new(adapters, 1)
            }
            Err(_) => NetworkInventory::default(),
        };
        assert_eq!(
            inventory.resolve(&AdapterSelection::None, None),
            Err(NetError::NoAdapterSelected)
        );
    }

    #[test]
    fn lease_binding_carries_both_interface_indexes() {
        let selected = id("cccccccc-cccc-cccc-cccc-cccccccccccc");
        let mut info = adapter(
            selected,
            "Ethernet",
            true,
            false,
            true,
            vec![addr("192.168.1.20", 24)],
        );
        info.if_index = 11;
        info.ipv6_if_index = 17;
        let inv = NetworkInventory::new(vec![info], 1);
        let lease = inv
            .resolve(&AdapterSelection::Adapter(selected), None)
            .unwrap();
        let binding = lease.network_binding();
        assert_eq!(binding.local_ip.to_string(), "192.168.1.20");
        assert_eq!(binding.ipv4_if_index, Some(11));
        assert_eq!(binding.ipv6_if_index, Some(17));
        assert_eq!(binding.scope_id, None);
    }

    #[test]
    fn ipv6_link_local_lease_binding_keeps_scope_id() {
        let selected = id("dddddddd-dddd-dddd-dddd-dddddddddddd");
        let mut link_local = addr("fe80::10", 64);
        link_local.scope_id = Some(6);
        let info = adapter(selected, "Ethernet", true, false, true, vec![link_local]);
        let inv = NetworkInventory::new(vec![info], 3);
        let lease = inv
            .resolve(
                &AdapterSelection::Adapter(selected),
                Some("fe80::10".parse().unwrap()),
            )
            .unwrap();
        let binding = lease.network_binding();
        assert_eq!(binding.local_ip.to_string(), "fe80::10");
        assert_eq!(binding.scope_id, Some(6));
        assert_eq!(binding.ipv6_if_index, Some(2));
    }

    #[test]
    fn synthetic_loopback_has_no_interface_index() {
        let loopback = synthetic_loopback();
        assert!(loopback.is_loopback);
        let binding = loopback.network_binding_for(loopback.addresses[0]);
        assert!(binding.is_loopback());
        assert_eq!(binding.ipv4_if_index, None);
        assert_eq!(binding.ipv6_if_index, None);
    }
}
