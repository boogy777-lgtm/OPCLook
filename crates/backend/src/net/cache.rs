use super::{AdapterAddress, AdapterId, AdapterInfo, AdapterSelection, AdapterState};
use serde::{Deserialize, Deserializer, Serialize, Serializer};
use std::collections::HashSet;
use std::net::IpAddr;
use std::num::NonZeroU32;

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
    #[serde(
        default,
        serialize_with = "serialize_if_index",
        deserialize_with = "deserialize_if_index"
    )]
    pub if_index: Option<NonZeroU32>,
    #[serde(
        default,
        serialize_with = "serialize_if_index",
        deserialize_with = "deserialize_if_index"
    )]
    pub ipv6_if_index: Option<NonZeroU32>,
    #[serde(default)]
    pub luid: u64,
    #[serde(default)]
    pub metric: u32,
    #[serde(default)]
    pub is_physical: bool,
}

fn serialize_if_index<S: Serializer>(
    index: &Option<NonZeroU32>,
    serializer: S,
) -> Result<S::Ok, S::Error> {
    serializer.serialize_u32(index.map_or(0, NonZeroU32::get))
}
fn deserialize_if_index<'de, D: Deserializer<'de>>(
    deserializer: D,
) -> Result<Option<NonZeroU32>, D::Error> {
    Ok(Option::<u32>::deserialize(deserializer)?.and_then(NonZeroU32::new))
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
