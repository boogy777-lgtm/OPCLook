use super::common::{send, timeout_drain};
use super::discovery::build_discovery_client;
use super::security::{
    auto_chose_unprotected_over_secure, resolve_security, security_error, security_summary,
};
use super::trust::{TrustLease, purge_stale, trust_for_session};
use super::values::{
    data_change_filter, parse_node, ua_access_current_write, ua_data_value_typed, ua_type_name,
    ua_variant_scalar,
};
use crate::bff::model::{
    BackendMsg, EndpointSpec, Event, EventKind, ItemInfo, ItemKey, ItemSpec, SessionId,
    SubscriptionState, UaEndpointInfo, UaSecurityPolicy,
};
use crate::bff::net::{BoundConnectionSource, BoundConnectorBuilder, NetworkBinding};
use crate::constants::{
    CONNECT_TIMEOUT, DISCOVERY_GUARD_TIMEOUT, GRACEFUL_CLOSE_TIMEOUT, IO_TIMEOUT,
    READ_MAX_ITEMS_PER_CALL, STRUCT_TYPES_TIMEOUT, UA_SAMPLING_INTERVAL_MS,
    UA_SUBSCRIPTION_KEEPALIVE_COUNT, UA_SUBSCRIPTION_LIFETIME_COUNT,
};
use opcua::client::custom_types::DataTypeTreeBuilder;
use opcua::client::{
    ClientBuilder, DataChangeCallback, IdentityToken, MonitoredItem, Password, Session,
};
use opcua::types::custom::{DataTypeTree, DynamicTypeLoader};
use opcua::types::{
    AttributeId, DataValue as UaDataValue, EndpointDescription, ExtensionObject,
    MessageSecurityMode, MonitoredItemCreateRequest, MonitoredItemModifyRequest, MonitoringMode,
    MonitoringParameters, NodeId, NumericRange, QualifiedName, ReadValueId, StatusCode,
    TimestampsToReturn, UserTokenPolicy, UserTokenType, VariantScalarTypeId,
};
use std::collections::{HashMap, HashSet};
use std::path::PathBuf;
use std::sync::Arc;
use std::sync::mpsc::Sender;
use std::time::Duration;
use tokio::task::JoinHandle;
use tokio_util::sync::CancellationToken;

pub(super) struct UaSession {
    pub(super) session: Arc<Session>,
    pub(super) event_loop: JoinHandle<StatusCode>,
    pub(super) items: HashMap<ItemKey, NodeId>,
    /// UA built-in variant scalar type of each subscribed node, resolved from
    /// the node's DataType attribute when the attribute could be read.
    pub(super) node_types: HashMap<ItemKey, VariantScalarTypeId>,
    pub(super) info: BackendMsg,
    /// The owning session id, used by the DataChange callback to tag pushes.
    pub(super) session_id: SessionId,
    pub(super) connection_id: Option<crate::bff::model::RequestId>,
    /// P1: server-assigned subscription ids, the first created lazily on the
    /// first AddItems. A server limits the monitored items of ONE subscription
    /// (`BadTooManyMonitoredItems`), so a large tag list spreads over several.
    pub(super) subscriptions: Vec<u32>,
    /// Requested publishing interval (ms) for the push subscription; mirrors the
    /// app's update interval. 0 until ConfigureUaSubscription arrives.
    pub(super) publishing_interval_ms: u64,
    /// Absolute deadband for DataChangeFilter (0 = off). Used for new items and
    /// pushed to the live ones by `apply_subscription_settings`.
    pub(super) deadband: f64,
    /// Publishing interval (ms) the live subscription actually runs with.
    pub(super) applied_interval_ms: u64,
    /// Deadband the live monitored items actually carry.
    pub(super) applied_deadband: f64,
    /// Server-assigned monitored item id of every item that is really monitored
    /// (created with a Good status). `delete`/`modify` address items by this id,
    /// NOT by the client handle (`ItemKey`).
    /// Value: (subscription id, monitored item id).
    pub(super) monitored: HashMap<ItemKey, (u32, u32)>,
    /// P1: items the server would not monitor on any subscription (its limits
    /// are reached). They stay registered and the app polls them with Read.
    pub(super) poll_only: HashSet<ItemKey>,
    /// P1: items per `CreateMonitoredItems` call; halved when the server
    /// answers `BadTooManyOperations`.
    pub(super) monitor_chunk: usize,
    /// Items monitored WITHOUT the deadband filter because the server refused it
    /// for their attribute/type (for example a Boolean or String node).
    pub(super) unfiltered: HashSet<ItemKey>,
    /// Set when a request hit [`IO_TIMEOUT`]. A timed-out request may still have
    /// a response in flight, so the session is poisoned: the command loop must
    /// close it gracefully instead of issuing another request on it.
    pub(super) timed_out: bool,
    /// Keeps the server certificate trusted for this session only (the connect
    /// dialog's "trust the certificate"); released when the session closes.
    pub(super) trust: Option<TrustLease>,
    /// V3: the server's structure descriptions, loaded once after connect, so
    /// structured values decode into named fields.
    pub(super) types: Option<Arc<DataTypeTree>>,
}

/// Monitoring parameters of one item: single-slot queue that discards the
/// oldest value, sampled at the subscription's publishing interval.
pub(super) fn monitoring_parameters(
    key: ItemKey,
    deadband: f64,
    with_filter: bool,
) -> MonitoringParameters {
    MonitoringParameters {
        client_handle: key.get(),
        sampling_interval: UA_SAMPLING_INTERVAL_MS,
        filter: if with_filter {
            data_change_filter(deadband)
        } else {
            ExtensionObject::null()
        },
        queue_size: 1,
        discard_oldest: true,
    }
}

/// Status codes with which a server refuses a monitoring filter (deadband) for
/// an attribute/type that cannot carry one, for example a Boolean or String
/// node. The item itself is fine and is monitored again without the filter.
pub(super) fn is_filter_refusal(status: StatusCode) -> bool {
    matches!(
        status.bits() & 0xFFFF_0000,
        0x8043_0000 | 0x8044_0000 | 0x8045_0000
    )
}

/// P1: the subscription holds as many monitored items as the server allows.
pub(super) fn is_too_many_monitored(status: StatusCode) -> bool {
    status.bits() & 0xFFFF_0000 == StatusCode::BadTooManyMonitoredItems.bits() & 0xFFFF_0000
}

/// P1: the call carried more operations than the server takes in one call.
pub(super) fn is_too_many_operations(status: StatusCode) -> bool {
    status.bits() & 0xFFFF_0000 == StatusCode::BadTooManyOperations.bits() & 0xFFFF_0000
}

/// P1: subscriptions one session may open for its tags.
pub(super) const MAX_SUBSCRIPTIONS_PER_SESSION: usize = 32;

/// Result of monitoring a batch of items.
#[derive(Default)]
pub(super) struct MonitorOutcome {
    /// Refused for a reason of their own (shown on the row).
    pub(super) rejected: Vec<(ItemKey, BackendMsg)>,
    /// Over every server limit: registered, but read by polling.
    pub(super) poll_only: Vec<ItemKey>,
}

/// The node does not exist or is not a variable: the server answered the very
/// first attribute read with one of these. Such an item can never deliver a
/// value, so it is refused up front with the server's own reason.
pub(super) fn is_missing_node(status: StatusCode) -> bool {
    [
        StatusCode::BadNodeIdUnknown,
        StatusCode::BadNodeIdInvalid,
        StatusCode::BadAttributeIdInvalid,
    ]
    .iter()
    .any(|bad| status.bits() & 0xFFFF_0000 == bad.bits() & 0xFFFF_0000)
}

/// Why one item could not be monitored.
pub(super) enum ItemFailure {
    /// The server answered the item with a Bad status.
    Status(StatusCode),
    /// The call itself failed or timed out; the message says why.
    Call(BackendMsg),
}

impl ItemFailure {
    pub(super) fn text(&self) -> BackendMsg {
        match self {
            ItemFailure::Status(status) => BackendMsg::Raw(status_text(*status)),
            ItemFailure::Call(reason) => reason.clone(),
        }
    }

    /// True when only the deadband filter was refused.
    pub(super) fn is_filter_refusal(&self) -> bool {
        matches!(self, ItemFailure::Status(status) if is_filter_refusal(*status))
    }
}

/// Language-neutral text for a Bad status of one item: the status name plus its
/// hexadecimal code, so the row and the journal show exactly what the server said.
pub(super) fn status_text(status: StatusCode) -> String {
    format!("{status} (0x{:08X})", status.bits())
}

/// Pairs the keys of one `CreateMonitoredItems` chunk with the server's
/// per-item results (same order): the monitored item id of a Good item, or the
/// reason of a Bad one. A result the server did not send is a failure too, never
/// silently "fine".
pub(super) fn create_outcomes(
    keys: &[ItemKey],
    results: &[(StatusCode, u32)],
) -> Vec<(ItemKey, Result<u32, ItemFailure>)> {
    keys.iter()
        .enumerate()
        .map(|(index, key)| {
            let outcome = match results.get(index) {
                Some((status, _)) if status.is_bad() => Err(ItemFailure::Status(*status)),
                Some((_, id)) => Ok(*id),
                None => Err(ItemFailure::Call(BackendMsg::ItemNoResult)),
            };
            (*key, outcome)
        })
        .collect()
}

/// Folds per-item outcomes back into the caller's item order. A rejected item is
/// an `Err` with the server's reason; everything else carries its attributes.
pub(super) fn merge_add_results(
    items: &[ItemSpec],
    mut infos: HashMap<ItemKey, ItemInfo>,
    rejected: &HashMap<ItemKey, BackendMsg>,
) -> Vec<(ItemKey, Result<ItemInfo, BackendMsg>)> {
    items
        .iter()
        .map(|item| {
            if let Some(reason) = rejected.get(&item.key) {
                return (item.key, Err(reason.clone()));
            }
            let info = infos.remove(&item.key).unwrap_or(ItemInfo {
                data_type: None,
                type_id: None,
                writable: true,
            });
            (item.key, Ok(info))
        })
        .collect()
}

mod connection;
mod core;
mod items;
mod subscription;

#[cfg(test)]
pub(super) use connection::TEST_PKI_DIR;
pub(super) use connection::{build_client, connect, pki_dir};
pub use connection::{client_cert_subject, pki_dir_public, use_private_pki_dir};
#[cfg(test)]
pub(super) use connection::{endpoint_description_for, resolve_identity_token};
