use super::discovery::build_discovery_client;
use super::io::read_values_chunked;
use super::security::{
    auto_chose_unprotected_over_secure, resolve_security, security_error, security_summary,
};
use super::trust::{TrustLease, purge_stale, trust_for_session};
use super::values::{
    data_change_filter, parse_node, ua_access_current_write, ua_data_value_typed, ua_type_name,
    ua_variant_scalar,
};
use super::worker::{send, timeout_drain};
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
    pub(super) items: HashMap<ItemKey, String>,
    /// UA built-in variant scalar type of each subscribed node, resolved from
    /// the node's DataType attribute when the attribute could be read.
    pub(super) node_types: HashMap<ItemKey, VariantScalarTypeId>,
    pub(super) info: BackendMsg,
    /// The owning session id, used by the DataChange callback to tag pushes.
    pub(super) session_id: SessionId,
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

impl UaSession {
    /// V3: load the server's own structure types (namespaces other than 0;
    /// the standard ones are built into the client) and register a decoder
    /// for them, so a structured value arrives as named fields instead of an
    /// opaque `structured:…`. Best effort: a server without DataTypeDefinition
    /// (pre-1.04) just keeps the old text.
    pub(super) async fn load_struct_types(&mut self) -> Result<(), BackendMsg> {
        let session = self.session.clone();
        let work = async move {
            // The decoder resolves encoding ids through the namespace table.
            session.read_namespace_array().await?;
            let tree = DataTypeTreeBuilder::new(|id: &NodeId| id.namespace != 0)
                .build(&session)
                .await?;
            Ok::<_, opcua::types::Error>((session, tree))
        };
        let (result, timed_out) = timeout_drain(work, STRUCT_TYPES_TIMEOUT).await;
        let Some(result) = result else {
            // The build never finished, even after the drain: a request may
            // still be in flight, so the session is closed like any timeout.
            self.timed_out = true;
            return Err(BackendMsg::TimeoutStructTypes);
        };
        let _ = timed_out;
        let (session, tree) = result.map_err(|e| BackendMsg::raw(e.to_string()))?;
        let tree = Arc::new(tree);
        session.add_type_loader(Arc::new(DynamicTypeLoader::new(tree.clone())));
        self.types = Some(tree);
        Ok(())
    }

    /// V3: name of the loaded structure type a DataType attribute value
    /// points at, if any.
    fn struct_type_name(&self, dv: &UaDataValue) -> Option<String> {
        let Some(opcua::types::Variant::NodeId(type_id)) = dv.value.as_ref() else {
            return None;
        };
        let info = self.types.as_ref()?.get_struct_type(type_id)?;
        Some(if info.name.trim().is_empty() {
            type_id.to_string()
        } else {
            info.name.clone()
        })
    }

    /// Closes the session on the wire: CloseSession + CloseSecureChannel, then
    /// waits for the event loop to finish on its own (it only returns once the
    /// channel is fully closed). Aborts it only as a last resort when the
    /// graceful close cannot complete in [`GRACEFUL_CLOSE_TIMEOUT`].
    pub(super) async fn close(&mut self) {
        // Untrust the server certificate once the channel is gone (below), but
        // keep it while the close handshake still runs.
        let trust = self.trust.take();
        let closed = tokio::time::timeout(GRACEFUL_CLOSE_TIMEOUT, self.session.disconnect()).await;
        let joined = tokio::time::timeout(GRACEFUL_CLOSE_TIMEOUT, &mut self.event_loop).await;
        if closed.is_err() || joined.is_err() {
            self.event_loop.abort();
        }
        drop(trust);
    }

    pub(super) async fn add_items(
        &mut self,
        items: &[ItemSpec],
        tx: &Sender<Event>,
    ) -> (
        Vec<(ItemKey, Result<ItemInfo, BackendMsg>)>,
        Option<BackendMsg>,
    ) {
        // Query the DataType and AccessLevel attributes once per node so the
        // write path can honor the server's actual type and writability.
        let mut rejected: HashMap<ItemKey, BackendMsg> = HashMap::new();
        let mut requests = Vec::new();
        let mut plan: Vec<(ItemKey, NodeId)> = Vec::new();
        for item in items {
            let Some(node) = parse_node(&item.item_id) else {
                rejected.insert(
                    item.key,
                    BackendMsg::InvalidNodeId {
                        value: item.item_id.trim().to_string(),
                    },
                );
                continue;
            };
            requests.push(ReadValueId {
                node_id: node.clone(),
                attribute_id: AttributeId::DataType as u32,
                index_range: NumericRange::None,
                data_encoding: QualifiedName::null(),
            });
            requests.push(ReadValueId {
                node_id: node.clone(),
                attribute_id: AttributeId::AccessLevel as u32,
                index_range: NumericRange::None,
                data_encoding: QualifiedName::null(),
            });
            self.items.insert(item.key, item.item_id.clone());
            plan.push((item.key, node));
        }

        let raw = if requests.is_empty() {
            None
        } else {
            match read_values_chunked(self, &requests, TimestampsToReturn::Neither, 0.0).await {
                Ok(values) => Some(values),
                Err(e) => {
                    let infos = plan
                        .iter()
                        .map(|(key, _)| {
                            (
                                *key,
                                ItemInfo {
                                    data_type: None,
                                    type_id: None,
                                    writable: true,
                                },
                            )
                        })
                        .collect();
                    return (
                        merge_add_results(items, infos, &rejected),
                        Some(BackendMsg::AttributesReadFailed { cause: Box::new(e) }),
                    );
                }
            }
        };

        let mut infos: HashMap<ItemKey, ItemInfo> = HashMap::with_capacity(plan.len());
        // V3: nodes whose DataType is a known structure: their type is clear
        // (shown by name), it just is not a scalar.
        let mut structured: HashSet<ItemKey> = HashSet::new();
        for (idx, (key, _)) in plan.iter().enumerate() {
            let mut scalar = None;
            let mut struct_name = None;
            let mut writable = false;
            if let Some(values) = &raw {
                if let Some(dv) = values.get(idx * 2) {
                    scalar = ua_variant_scalar(dv);
                    if scalar.is_none() {
                        struct_name = self.struct_type_name(dv);
                    }
                }
                if let Some(dv) = values.get(idx * 2 + 1) {
                    writable = ua_access_current_write(dv).unwrap_or(false);
                }
            }
            if struct_name.is_some() {
                structured.insert(*key);
            }
            let data_type = scalar.map(|t| ua_type_name(t).to_string()).or(struct_name);
            let type_id = scalar.map(|t| t as u16);
            if let Some(t) = scalar {
                self.node_types.insert(*key, t);
            } else {
                self.node_types.remove(key);
            }
            infos.insert(
                *key,
                ItemInfo {
                    data_type,
                    type_id,
                    writable,
                },
            );
        }

        // A node the server does not know (or that is no variable) answered the
        // attribute read with a Bad status already: refuse it right here, with the
        // server's own reason, instead of subscribing to something that can never
        // deliver a value.
        if let Some(values) = &raw {
            for (idx, (key, _)) in plan.iter().enumerate() {
                let status = values.get(idx * 2).and_then(|dv| dv.status);
                if let Some(status) = status.filter(|s| is_missing_node(*s)) {
                    rejected.insert(*key, BackendMsg::Raw(status_text(status)));
                    self.items.remove(key);
                    self.node_types.remove(key);
                }
            }
        }
        let plan: Vec<(ItemKey, NodeId)> = plan
            .into_iter()
            .filter(|(key, _)| !rejected.contains_key(key))
            .collect();
        let attribute_failures = plan
            .iter()
            .filter(|(key, _)| !self.node_types.contains_key(key) && !structured.contains(key))
            .count();
        let warning = if attribute_failures > 0 {
            Some(BackendMsg::TypesUnresolved {
                count: attribute_failures,
            })
        } else {
            None
        };

        if plan.is_empty() {
            return (merge_add_results(items, infos, &rejected), warning);
        }

        // After items are successfully registered, (re-)establish the push
        // subscription for this session. The same DataChange callback pushes
        // values into the shared event channel so rows update without polling.
        match self.ensure_subscription(tx).await {
            Ok(()) => {
                let outcome = self.monitor_items(tx, &plan).await;
                for (key, reason) in outcome.rejected {
                    rejected.insert(key, reason);
                }
                if !outcome.poll_only.is_empty() {
                    // P1: over the server's limits: still read, by polling.
                    self.poll_only.extend(outcome.poll_only.iter().copied());
                    self.log(
                        tx,
                        BackendMsg::PollOnlyItems {
                            count: outcome.poll_only.len(),
                        },
                    );
                    send(
                        tx,
                        Event {
                            origin: crate::bff::model::Origin::Unsolicited,
                            owner: self.session_id.into(),
                            kind: EventKind::PollOnly {
                                keys: outcome.poll_only,
                            },
                        },
                    );
                }
                // Push is live as soon as at least one item is really
                // monitored. When none is, the app keeps polling.
                let state = if self.monitored.is_empty() {
                    SubscriptionState::Failed
                } else {
                    SubscriptionState::Active
                };
                self.report_subscription_state(tx, state);
            }
            Err(message) => {
                let not_created = BackendMsg::SubscriptionNotCreated {
                    cause: Box::new(message),
                };
                let warning = match warning {
                    Some(warning) => BackendMsg::Lines(vec![warning, not_created]),
                    None => not_created,
                };
                return (merge_add_results(items, infos, &rejected), Some(warning));
            }
        }

        (merge_add_results(items, infos, &rejected), warning)
    }

    /// Create the first subscription of the session on its first use.
    async fn ensure_subscription(&mut self, tx: &Sender<Event>) -> Result<(), BackendMsg> {
        if !self.subscriptions.is_empty() {
            return Ok(());
        }
        self.create_subscription(tx).await.map(|_| ())
    }

    /// Create one more subscription. Publishing interval mirrors the app update
    /// interval (default 1000 ms) and the DataChange callback forwards
    /// quality/timestamp-mapped values to the UI.
    async fn create_subscription(&mut self, tx: &Sender<Event>) -> Result<u32, BackendMsg> {
        let interval_ms = self.publishing_interval_ms.clamp(100, 3600_000);
        let interval = Duration::from_millis(interval_ms);
        let session_id = self.session_id;
        let event_tx = tx.clone();
        let types = self.types.clone();
        let callback =
            DataChangeCallback::new(move |data_value: UaDataValue, item: &MonitoredItem| {
                let Some(key) = ItemKey::new(item.client_handle()) else {
                    let _ = event_tx.send(Event {
                        origin: crate::bff::model::Origin::Unsolicited,
                        owner: session_id.into(),
                        kind: EventKind::Log {
                            message: BackendMsg::InvalidItemHandle,
                        },
                    });
                    return;
                };
                let _ = event_tx.send(Event {
                    origin: crate::bff::model::Origin::Unsolicited,
                    owner: session_id.into(),
                    kind: EventKind::Values {
                        values: vec![(key, ua_data_value_typed(&data_value, types.as_deref()))],
                    },
                });
            });
        let (created, timed_out) = timeout_drain(
            self.session.create_subscription(
                interval,
                UA_SUBSCRIPTION_LIFETIME_COUNT,
                UA_SUBSCRIPTION_KEEPALIVE_COUNT,
                0,    // max_notifications_per_publish (0 = no limit)
                0,    // priority
                true, // publishing_enabled
                callback,
            ),
            IO_TIMEOUT,
        )
        .await;
        if timed_out {
            self.timed_out = true;
            return Err(BackendMsg::TimeoutCreateSubscription);
        }
        let Some(created) = created else {
            return Err(BackendMsg::TimeoutCreateSubscription);
        };
        let sub_id = created.map_err(|e| BackendMsg::CreateSubscriptionFailed {
            detail: e.to_string(),
        })?;
        if self.subscriptions.is_empty() {
            self.applied_interval_ms = interval_ms;
            self.applied_deadband = self.deadband;
        }
        self.subscriptions.push(sub_id);
        Ok(sub_id)
    }

    /// Applies new update-interval / deadband settings to the LIVE subscription
    /// (`ModifySubscription` + `ModifyMonitoredItems`). Without a subscription the
    /// values are only stored and used when it is created. Returns the problems
    /// found, each one a journal line; an empty list means everything applied.
    pub(super) async fn apply_subscription_settings(
        &mut self,
        publishing_interval_ms: u64,
        deadband: f64,
    ) -> Vec<BackendMsg> {
        let interval_ms = publishing_interval_ms.clamp(100, 3600_000);
        let deadband = if deadband.is_finite() {
            deadband.max(0.0)
        } else {
            0.0
        };
        self.publishing_interval_ms = interval_ms;
        self.deadband = deadband;
        if self.subscriptions.is_empty() {
            return Vec::new();
        }
        let mut problems = Vec::new();

        if interval_ms != self.applied_interval_ms {
            let mut all_applied = true;
            for sub_id in self.subscriptions.clone() {
                let (modified, timed_out) = timeout_drain(
                    self.session.modify_subscription(
                        sub_id,
                        Duration::from_millis(interval_ms),
                        UA_SUBSCRIPTION_LIFETIME_COUNT,
                        UA_SUBSCRIPTION_KEEPALIVE_COUNT,
                        0,
                        0,
                    ),
                    IO_TIMEOUT,
                )
                .await;
                if timed_out {
                    self.timed_out = true;
                    problems.push(BackendMsg::SubscriptionProblem {
                        cause: Box::new(BackendMsg::TimeoutPublishingInterval),
                    });
                    return problems;
                }
                match modified {
                    Some(Ok(())) => {}
                    Some(Err(e)) => {
                        all_applied = false;
                        problems.push(BackendMsg::SubscriptionProblem {
                            cause: Box::new(BackendMsg::ChangeIntervalFailed {
                                detail: e.to_string(),
                            }),
                        });
                    }
                    None => all_applied = false,
                }
            }
            if all_applied {
                self.applied_interval_ms = interval_ms;
            }
        }

        if deadband != self.applied_deadband {
            let mut targets: Vec<(u32, ItemKey, u32)> = self
                .monitored
                .iter()
                .filter(|(key, _)| !self.unfiltered.contains(key))
                .map(|(key, (sub, id))| (*sub, *key, *id))
                .collect();
            targets.sort_unstable();
            let mut failed = 0usize;
            // One ModifyMonitoredItems per subscription (and per chunk).
            let mut groups: Vec<(u32, Vec<(ItemKey, u32)>)> = Vec::new();
            for (sub, key, id) in targets {
                match groups.last_mut() {
                    Some((last, items)) if *last == sub => items.push((key, id)),
                    _ => groups.push((sub, vec![(key, id)])),
                }
            }
            let chunks: Vec<(u32, Vec<(ItemKey, u32)>)> = groups
                .into_iter()
                .flat_map(|(sub, items)| {
                    items
                        .chunks(READ_MAX_ITEMS_PER_CALL.max(1))
                        .map(|c| (sub, c.to_vec()))
                        .collect::<Vec<_>>()
                })
                .collect();
            for (sub_id, chunk) in &chunks {
                let sub_id = *sub_id;
                let requests: Vec<MonitoredItemModifyRequest> = chunk
                    .iter()
                    .map(|(key, id)| MonitoredItemModifyRequest {
                        monitored_item_id: *id,
                        requested_parameters: MonitoringParameters {
                            client_handle: key.get(),
                            sampling_interval: UA_SAMPLING_INTERVAL_MS,
                            filter: data_change_filter(deadband),
                            queue_size: 1,
                            discard_oldest: true,
                        },
                    })
                    .collect();
                let (modified, timed_out) = timeout_drain(
                    self.session.modify_monitored_items(
                        sub_id,
                        TimestampsToReturn::Both,
                        &requests,
                    ),
                    IO_TIMEOUT,
                )
                .await;
                if timed_out {
                    self.timed_out = true;
                    problems.push(BackendMsg::SubscriptionProblem {
                        cause: Box::new(BackendMsg::TimeoutDeadband),
                    });
                    return problems;
                }
                match modified {
                    Some(Ok(results)) => {
                        for (index, (key, id)) in chunk.iter().enumerate() {
                            let status = results.get(index).map(|r| r.status_code);
                            match status {
                                Some(status) if status.is_bad() => {
                                    if is_filter_refusal(status) {
                                        // Not a numeric node: it simply has no
                                        // deadband. Keep it monitored as it is.
                                        self.unfiltered.insert(*key);
                                    } else {
                                        failed += 1;
                                    }
                                }
                                Some(_) => {}
                                None => failed += 1,
                            }
                            let _ = id;
                        }
                    }
                    Some(Err(e)) => {
                        problems.push(BackendMsg::SubscriptionProblem {
                            cause: Box::new(BackendMsg::ChangeDeadbandFailed {
                                detail: e.to_string(),
                            }),
                        });
                        return problems;
                    }
                    None => return problems,
                }
            }
            if failed > 0 {
                problems.push(BackendMsg::SubscriptionProblem {
                    cause: Box::new(BackendMsg::DeadbandRejected { count: failed }),
                });
            } else {
                self.applied_deadband = deadband;
            }
        }
        problems
    }

    /// Builds the `CreateMonitoredItems` entry of one node.
    fn monitor_request(
        &self,
        key: ItemKey,
        node: &NodeId,
        with_filter: bool,
    ) -> MonitoredItemCreateRequest {
        MonitoredItemCreateRequest {
            item_to_monitor: ReadValueId {
                node_id: node.clone(),
                attribute_id: AttributeId::Value as u32,
                index_range: NumericRange::None,
                data_encoding: QualifiedName::null(),
            },
            monitoring_mode: MonitoringMode::Reporting,
            requested_parameters: monitoring_parameters(key, self.deadband, with_filter),
        }
    }

    /// Sends one `CreateMonitoredItems` round (chunked) and returns, in request
    /// order, either the server-assigned monitored item id or the Bad status the
    /// server gave that item. A failure of the whole call is an `Err`.
    ///
    /// P1: a call refused with `BadTooManyOperations` is retried with half the
    /// chunk (the size is kept for later rounds); a call refused with
    /// `BadTooManyMonitoredItems` reports that status for every remaining item,
    /// so the caller can move them to another subscription.
    async fn create_round(
        &mut self,
        subscription_id: u32,
        round: &[(ItemKey, MonitoredItemCreateRequest)],
        tx: &Sender<Event>,
    ) -> Vec<(ItemKey, Result<u32, ItemFailure>)> {
        let mut out: Vec<(ItemKey, Result<u32, ItemFailure>)> = Vec::with_capacity(round.len());
        let mut start = 0usize;
        while start < round.len() {
            let size = self.monitor_chunk.max(1).min(round.len() - start);
            let Some(chunk) = round.get(start..start + size) else {
                break;
            };
            let requests: Vec<MonitoredItemCreateRequest> =
                chunk.iter().map(|(_, request)| request.clone()).collect();
            let (created, timed_out) = timeout_drain(
                self.session.create_monitored_items(
                    subscription_id,
                    TimestampsToReturn::Both,
                    requests,
                ),
                IO_TIMEOUT,
            )
            .await;
            let failure = if timed_out {
                self.timed_out = true;
                Some(BackendMsg::TimeoutMonitoredItems)
            } else {
                match created {
                    Some(Ok(results)) => {
                        let keys: Vec<ItemKey> = chunk.iter().map(|(key, _)| *key).collect();
                        let statuses: Vec<(StatusCode, u32)> = results
                            .iter()
                            .map(|item| (item.result.status_code, item.result.monitored_item_id))
                            .collect();
                        out.extend(create_outcomes(&keys, &statuses));
                        start += size;
                        None
                    }
                    Some(Err(e)) if is_too_many_operations(e.status()) && size > 1 => {
                        self.monitor_chunk = (size / 2).max(1);
                        continue;
                    }
                    Some(Err(e)) if is_too_many_monitored(e.status()) => {
                        for (key, _) in round.iter().skip(start) {
                            out.push((*key, Err(ItemFailure::Status(e.status()))));
                        }
                        break;
                    }
                    Some(Err(e)) => Some(BackendMsg::MonitorFailed {
                        detail: e.to_string(),
                    }),
                    None => Some(BackendMsg::TimeoutMonitoredItems),
                }
            };
            if let Some(reason) = failure {
                // The whole call failed: every item of this and the remaining
                // chunks is reported with the reason, never silently dropped.
                self.log(
                    tx,
                    BackendMsg::SubscriptionProblem {
                        cause: Box::new(reason.clone()),
                    },
                );
                for (key, _) in round.iter().skip(start) {
                    out.push((*key, Err(ItemFailure::Call(reason.clone()))));
                }
                break;
            }
        }
        out
    }

    /// Create monitored items for `nodes`, mapping each ItemKey to the monitored
    /// item's client handle and applying the configured absolute deadband
    /// filter (0 = none). Every item's own `status_code` is checked: a Bad item
    /// is NOT monitored and is returned with the server's reason, so the caller
    /// can show it instead of leaving the row "waiting for a value" forever. An
    /// item refused only because of the deadband filter (Boolean/String nodes)
    /// is monitored again without it.
    ///
    /// P1: items refused because the subscription is full go to a new
    /// subscription; when the server refuses one more subscription too (or a
    /// fresh one takes nothing), they become poll-only instead of silent.
    async fn monitor_items(
        &mut self,
        tx: &Sender<Event>,
        nodes: &[(ItemKey, NodeId)],
    ) -> MonitorOutcome {
        let mut outcome = MonitorOutcome::default();
        let mut pending: Vec<(ItemKey, NodeId)> = nodes.to_vec();
        let mut fresh = false;
        // Each pass either monitors something or ends; the bound is a guard.
        for _ in 0..=MAX_SUBSCRIPTIONS_PER_SESSION {
            let Some(&sub_id) = self.subscriptions.last() else {
                break;
            };
            let overflow = self.monitor_on(tx, sub_id, &pending, &mut outcome).await;
            if overflow.is_empty() || self.timed_out {
                pending.clear();
                break;
            }
            let progressed = overflow.len() < pending.len();
            if fresh && !progressed {
                // A brand-new subscription took nothing: the limit is the
                // session's, not the subscription's.
                pending = overflow;
                break;
            }
            pending = overflow;
            if self.subscriptions.len() >= MAX_SUBSCRIPTIONS_PER_SESSION {
                break;
            }
            match self.create_subscription(tx).await {
                Ok(_) => fresh = true,
                Err(_) => break,
            }
        }
        outcome
            .poll_only
            .extend(pending.into_iter().map(|(key, _)| key));
        outcome
    }

    /// One pass of [`Self::monitor_items`] on one subscription. Returns the
    /// items refused because that subscription is full.
    async fn monitor_on(
        &mut self,
        tx: &Sender<Event>,
        subscription_id: u32,
        nodes: &[(ItemKey, NodeId)],
        outcome: &mut MonitorOutcome,
    ) -> Vec<(ItemKey, NodeId)> {
        let round: Vec<(ItemKey, MonitoredItemCreateRequest)> = nodes
            .iter()
            .map(|(key, node)| (*key, self.monitor_request(*key, node, true)))
            .collect();
        let mut overflow: Vec<(ItemKey, NodeId)> = Vec::new();
        let mut retry: Vec<(ItemKey, NodeId)> = Vec::new();
        for (key, result) in self.create_round(subscription_id, &round, tx).await {
            let node = nodes.iter().find(|(k, _)| *k == key).map(|(_, n)| n);
            match result {
                Ok(id) => {
                    self.monitored.insert(key, (subscription_id, id));
                }
                Err(ItemFailure::Status(status)) if is_too_many_monitored(status) => {
                    if let Some(node) = node {
                        overflow.push((key, node.clone()));
                    }
                }
                Err(failure) => match node {
                    Some(node)
                        if failure.is_filter_refusal()
                            && self.deadband > 0.0
                            && !self.timed_out =>
                    {
                        retry.push((key, node.clone()));
                    }
                    _ => outcome.rejected.push((key, failure.text())),
                },
            }
        }

        if !retry.is_empty() {
            let round: Vec<(ItemKey, MonitoredItemCreateRequest)> = retry
                .iter()
                .map(|(key, node)| (*key, self.monitor_request(*key, node, false)))
                .collect();
            for (key, result) in self.create_round(subscription_id, &round, tx).await {
                match result {
                    Ok(id) => {
                        self.monitored.insert(key, (subscription_id, id));
                        self.unfiltered.insert(key);
                    }
                    Err(ItemFailure::Status(status)) if is_too_many_monitored(status) => {
                        if let Some((_, node)) = retry.iter().find(|(k, _)| *k == key) {
                            overflow.push((key, node.clone()));
                        }
                    }
                    Err(failure) => outcome.rejected.push((key, failure.text())),
                }
            }
        }
        overflow
    }

    /// Report the current subscription state so the app knows whether to poll.
    fn report_subscription_state(&mut self, tx: &Sender<Event>, state: SubscriptionState) {
        send(
            tx,
            Event {
                origin: crate::bff::model::Origin::Unsolicited,
                owner: self.session_id.into(),
                kind: EventKind::SubscriptionState { state },
            },
        );
    }

    /// Unsolicited diagnostic line for the application journal.
    fn log(&self, tx: &Sender<Event>, message: BackendMsg) {
        send(
            tx,
            Event {
                origin: crate::bff::model::Origin::Unsolicited,
                owner: self.session_id.into(),
                kind: EventKind::Log { message },
            },
        );
    }

    /// Delete the monitored items for `keys` and drop the whole subscription
    /// when no item remains. Items are addressed by the server-assigned
    /// monitored item id (the `ItemKey` is only the client handle). Returns the
    /// problems found, each one a journal line.
    pub(super) async fn remove_items(
        &mut self,
        tx: &Sender<Event>,
        keys: &[ItemKey],
    ) -> Vec<BackendMsg> {
        let mut problems = Vec::new();
        let known: Vec<ItemKey> = keys
            .iter()
            .filter(|k| self.items.contains_key(k))
            .copied()
            .collect();
        if known.is_empty() {
            return problems;
        }
        let mut poisoned = false;
        // Monitored item ids grouped by their subscription.
        let mut by_sub: Vec<(u32, Vec<u32>)> = Vec::new();
        for key in &known {
            if let Some((sub, id)) = self.monitored.get(key).copied() {
                match by_sub.iter_mut().find(|(s, _)| *s == sub) {
                    Some((_, ids)) => ids.push(id),
                    None => by_sub.push((sub, vec![id])),
                }
            }
        }
        'subs: for (sub_id, ids) in by_sub {
            for chunk in ids.chunks(READ_MAX_ITEMS_PER_CALL.max(1)) {
                let (delete_result, timed_out) = timeout_drain(
                    self.session.delete_monitored_items(sub_id, chunk),
                    IO_TIMEOUT,
                )
                .await;
                if timed_out {
                    self.timed_out = true;
                    poisoned = true;
                    break 'subs;
                }
                match delete_result {
                    Some(Ok(statuses)) => {
                        let bad = statuses
                            .iter()
                            .filter(|s| {
                                s.is_bad()
                                    && s.bits() & 0xFFFF_0000
                                        != StatusCode::BadMonitoredItemIdInvalid.bits()
                            })
                            .count();
                        if bad > 0 {
                            problems.push(BackendMsg::SubscriptionProblem {
                                cause: Box::new(BackendMsg::DeleteRejected { count: bad }),
                            });
                        }
                    }
                    Some(Err(e)) => {
                        problems.push(BackendMsg::SubscriptionProblem {
                            cause: Box::new(BackendMsg::DeleteFailed {
                                detail: e.to_string(),
                            }),
                        });
                        break 'subs;
                    }
                    None => break 'subs,
                }
            }
        }

        // Prune the maps now so a subsequent add re-monitors correctly.
        for key in &known {
            self.items.remove(key);
            self.node_types.remove(key);
            self.monitored.remove(key);
            self.unfiltered.remove(key);
            self.poll_only.remove(key);
        }

        // A timed-out delete may still have a response in flight; the caller
        // closes the session gracefully, so do not enqueue another request.
        if poisoned {
            return problems;
        }

        if self.items.is_empty() {
            self.teardown_subscription(tx).await;
        }
        problems
    }

    /// Best-effort cleanup: delete the subscription and report it inactive.
    pub(super) async fn teardown_subscription(&mut self, tx: &Sender<Event>) {
        for sub_id in std::mem::take(&mut self.subscriptions) {
            if self.timed_out {
                break;
            }
            let (_, timed_out) =
                timeout_drain(self.session.delete_subscription(sub_id), IO_TIMEOUT).await;
            if timed_out {
                self.timed_out = true;
            }
        }
        self.monitored.clear();
        self.unfiltered.clear();
        self.poll_only.clear();
        self.report_subscription_state(tx, SubscriptionState::Inactive);
    }
}

pub(super) fn describe_connect_error(
    url: &str,
    pki: &std::path::Path,
    error: opcua::types::Error,
) -> BackendMsg {
    let msg = error.to_string();
    if certification_error(&msg) {
        BackendMsg::UaCertUntrusted {
            url: url.to_string(),
            detail: msg,
            pki: pki.display().to_string(),
        }
    } else {
        BackendMsg::UaConnectFailed {
            url: url.to_string(),
            detail: msg,
        }
    }
}

/// Spawn the session event loop and wait for the session to activate. `build()`
/// itself does not open a connection, so this is the first network step after
/// the endpoint description has been selected. A cancelled attempt closes the
/// channel gracefully like a failed one.
async fn activate_ua_session<C>(
    session: &Session,
    event_loop: opcua::client::SessionEventLoop<C>,
    url: &str,
    cancel: &CancellationToken,
) -> Result<JoinHandle<StatusCode>, BackendMsg>
where
    C: opcua::client::transport::Connector + Send + Sync + 'static,
{
    let mut event_loop = event_loop.spawn();
    let connected = tokio::select! {
        biased;
        _ = cancel.cancelled() => None,
        connected = tokio::time::timeout(CONNECT_TIMEOUT, session.wait_for_connection()) => {
            Some(connected)
        }
    };
    match connected {
        Some(Ok(true)) => Ok(event_loop),
        Some(Ok(false)) => {
            let _ = tokio::time::timeout(GRACEFUL_CLOSE_TIMEOUT, session.disconnect()).await;
            let _ = tokio::time::timeout(GRACEFUL_CLOSE_TIMEOUT, &mut event_loop).await;
            Err(BackendMsg::UaSessionClosedOnConnect {
                url: url.to_string(),
            })
        }
        Some(Err(_)) => {
            // Timeout activating the session: the transport is already connected
            // so the server expects a CloseSession/CloseSecureChannel. Shut the
            // channel down gracefully instead of aborting the loop mid-request.
            let _ = tokio::time::timeout(GRACEFUL_CLOSE_TIMEOUT, session.disconnect()).await;
            let _ = tokio::time::timeout(GRACEFUL_CLOSE_TIMEOUT, &mut event_loop).await;
            Err(BackendMsg::TimeoutActivateSession)
        }
        None => {
            let _ = tokio::time::timeout(GRACEFUL_CLOSE_TIMEOUT, session.disconnect()).await;
            let _ = tokio::time::timeout(GRACEFUL_CLOSE_TIMEOUT, &mut event_loop).await;
            Err(BackendMsg::ConnectCancelled)
        }
    }
}

pub(super) async fn connect(
    session_id: SessionId,
    endpoint: EndpointSpec,
    cancel: &CancellationToken,
    event_tx: &Sender<Event>,
) -> Result<UaSession, BackendMsg> {
    let url = endpoint.address.trim();
    if url.parse::<opclook_contract::ua_url::UaUrl>().is_err() {
        return Err(BackendMsg::UaUrlScheme);
    }
    let binding = endpoint.ua_bind.clone();

    // 1. Fetch the server's actual endpoints so we can pick a security policy
    //    and mode the server really offers instead of guessing blindly. The full
    //    descriptions are kept so the connect description can reuse the server's
    //    own user-token policies (CODESYS advertises "Anonymous", not the
    //    crate's hardcoded "anonymous", and rejects ActivateSession otherwise
    //    with BadIdentityTokenInvalid).
    let mut server_endpoints = tokio::select! {
        biased;
        _ = cancel.cancelled() => return Err(BackendMsg::ConnectCancelled),
        found = discover_endpoints(url, binding.as_ref()) => found?,
    };
    let mut discovered = Vec::with_capacity(server_endpoints.len());
    server_endpoints.retain(|e| match e.endpoint_url.to_string().parse() {
        Ok(url) => {
            discovered.push(UaEndpointInfo {
                url,
                security_policy: crate::bff::model::AdvertisedPolicy::from_wire(
                    &e.security_policy_uri.to_string(),
                ),
                security_mode: super::security::advertised_mode(e.security_mode),
            });
            true
        }
        Err(_) => {
            let _ = event_tx.send(Event {
                origin: crate::bff::model::Origin::Unsolicited,
                owner: session_id.into(),
                kind: EventKind::Log {
                    message: BackendMsg::InvalidDiscoveredUaUrl,
                },
            });
            false
        }
    });

    // 2. Resolve the requested policy/mode (Auto default) against what the
    //    server advertises. An unsupported explicit choice is a hard error: no
    //    silent fallback to None/other.
    let (policy_uri, mode) = resolve_security(
        endpoint.ua_security_policy,
        endpoint.ua_security_mode,
        &discovered,
    )
    .map_err(security_error)?;

    let policy_short = UaSecurityPolicy::from_advertised(&policy_uri)
        .map(|p| p.as_str().to_string())
        .unwrap_or_else(|| policy_uri.clone());

    // 3. Build the client with the PKI store and the user's trust preference.
    let pki = pki_dir();
    purge_stale(&pki);
    let mut builder = ClientBuilder::new()
        .application_name("OPC DA/UA Test Client")
        .application_uri("urn:opcda-egui")
        .product_uri("urn:opcda-egui")
        .create_sample_keypair(true)
        // The library's own "trust unknown certificates" switch would store the
        // server certificate in `trusted` FOREVER. "Trust" in the connect dialog
        // means this session only, which `TrustLease` provides (see trust.rs).
        .trust_server_certs(false)
        // A trusted server is also exempt from the validity period, host name and
        // application URI checks (industrial devices often fail them); the key
        // length and the identity of the certificate are still checked.
        .verify_server_certs(!endpoint.ua_trust_server_cert)
        .pki_dir(pki.clone())
        .session_retry_limit(3);

    // 4. Session timeout (optional; the crate accepts a requested value and the
    //    server revises it, but the revised value is not exposed — see docs).
    if let Some(timeout_ms) = endpoint.ua_session_timeout {
        builder = builder.session_timeout(timeout_ms);
    }

    let client = build_client(builder)?;

    // 5. Reuse the selected server's full endpoint description, including its
    //    actual user-token policies.
    let description = endpoint_description_for(&server_endpoints, url, &policy_uri, mode);

    // 5b. "Trust the certificate" = trust for the lifetime of this session. The
    //     lease is dropped (and the certificate untrusted again) when the connect
    //     fails below or the session closes.
    let trust = if endpoint.ua_trust_server_cert {
        trust_for_session(&pki, &description.server_certificate)?
    } else {
        None
    };

    // 6. Validate identity against the selected endpoint, not a guessed
    //    correlation between message security and certificate support.
    let identity = resolve_identity_token(&endpoint, &description)?;

    // 7. Build the session (no network I/O), then start its event loop and wait
    //    for activation. The binding captured on the EndpointSpec is reused on
    //    every reconnect so a session never silently moves to another card.
    let (session, event_loop) = if let Some(binding) = binding {
        let (session, event_loop) = client
            .session_builder()
            .with_connector(BoundConnectionSource::new(binding))
            .connect_to_endpoint_directly(description)
            .map_err(|error| describe_connect_error(url, &pki, error))?
            .user_identity_token(identity)
            .build(client.certificate_store().clone())
            .map_err(|error| describe_connect_error(url, &pki, error))?;
        let event_loop = activate_ua_session(&session, event_loop, url, cancel).await?;
        (session, event_loop)
    } else {
        let (session, event_loop) = client
            .session_builder()
            .connect_to_endpoint_directly(description)
            .map_err(|error| describe_connect_error(url, &pki, error))?
            .user_identity_token(identity)
            .build(client.certificate_store().clone())
            .map_err(|error| describe_connect_error(url, &pki, error))?;
        let event_loop = activate_ua_session(&session, event_loop, url, cancel).await?;
        (session, event_loop)
    };

    let secure_was_available = auto_chose_unprotected_over_secure(
        endpoint.ua_security_policy,
        endpoint.ua_security_mode,
        mode,
        &discovered,
    );
    Ok(UaSession {
        session,
        event_loop,
        items: HashMap::new(),
        node_types: HashMap::new(),
        info: security_summary(url, &policy_short, mode, secure_was_available),
        session_id,
        subscriptions: Vec::new(),
        publishing_interval_ms: 0,
        deadband: 0.0,
        applied_interval_ms: 0,
        applied_deadband: 0.0,
        monitored: HashMap::new(),
        poll_only: HashSet::new(),
        monitor_chunk: READ_MAX_ITEMS_PER_CALL,
        unfiltered: HashSet::new(),
        timed_out: false,
        trust,
        types: None,
    })
}

/// Choose the endpoint description the session connects with: the server's own
/// advertised entry for the resolved policy/mode, so its user-token policies
/// (and certificate) are reused, with the endpoint URL replaced by the
/// user-supplied one. Falls back to a minimal anonymous description when no
/// advertised entry matches.
///
/// The hand-built fallback pins `UserTokenPolicy::anonymous()`, whose
/// `policy_id` is the crate default `"anonymous"`. CODESYS advertises its
/// anonymous policy as `"Anonymous"` and answers ActivateSession with
/// `BadIdentityTokenInvalid` on a mismatch, so a matching advertised endpoint
/// must be preferred.
pub(super) fn endpoint_description_for(
    server_endpoints: &[EndpointDescription],
    url: &str,
    policy_uri: &str,
    mode: MessageSecurityMode,
) -> EndpointDescription {
    let mut description = server_endpoints
        .iter()
        .find(|e| e.security_policy_uri.as_ref() == policy_uri && e.security_mode == mode)
        .cloned()
        .unwrap_or_else(|| (url, policy_uri, mode, UserTokenPolicy::anonymous()).into());
    description.endpoint_url = url.into();
    description
}

/// Determine the OPC UA user identity token for a connect. Supports Anonymous,
/// UserName and (when the endpoint advertises Certificate tokens) X509 reusing
/// the client certificate from the PKI store.
pub(super) fn resolve_identity_token(
    endpoint: &EndpointSpec,
    description: &EndpointDescription,
) -> Result<IdentityToken, BackendMsg> {
    if endpoint.ua_use_certificate {
        if description
            .find_policy(UserTokenType::Certificate)
            .is_none()
        {
            return Err(BackendMsg::UaCertLoginUnsupported);
        }
        let pki = pki_dir();
        let cert_path = pki.join("own").join("cert.der");
        let key_path = pki.join("private").join("private.pem");
        if !cert_path.exists() || !key_path.exists() {
            return Err(BackendMsg::UaClientCertMissing {
                cert: cert_path.display().to_string(),
                key: key_path.display().to_string(),
            });
        }
        return IdentityToken::new_x509_path(&cert_path, &key_path).map_err(|e| {
            BackendMsg::UaClientCertLoadFailed {
                detail: e.to_string(),
            }
        });
    }

    if endpoint.user.trim().is_empty() {
        Ok(IdentityToken::Anonymous)
    } else {
        Ok(IdentityToken::UserName(
            endpoint.user.clone(),
            Password(endpoint.password.clone()),
        ))
    }
}

/// Best-effort detection of certificate-related connect failures so we can tell
/// the user to enable trust / import the certificate.
fn certification_error(message: &str) -> bool {
    let m = message.to_ascii_lowercase();
    m.contains("badcertificate") || m.contains("untrusted") || m.contains("certificate")
}

/// Fetches the server's advertised endpoints via `GetEndpoints`. The client is
/// built with a permissive trust setting so discovery works even when the
/// server cert is not yet trusted (GetEndpoints runs over an anonymous channel).
/// The full descriptions are returned so the connect description can reuse the
/// server's own user-token policies.
async fn discover_endpoints(
    url: &str,
    binding: Option<&NetworkBinding>,
) -> Result<Vec<EndpointDescription>, BackendMsg> {
    let client = build_discovery_client()?;
    let request = if let Some(binding) = binding {
        let connector = BoundConnectorBuilder::new(url, binding.clone());
        tokio::time::timeout(
            DISCOVERY_GUARD_TIMEOUT,
            client.get_server_endpoints_from_url(connector),
        )
        .await
    } else {
        tokio::time::timeout(
            DISCOVERY_GUARD_TIMEOUT,
            client.get_server_endpoints_from_url(url),
        )
        .await
    };
    let endpoints = request
        .map_err(|_| BackendMsg::TimeoutEndpoints {
            url: url.to_string(),
        })?
        .map_err(|e| BackendMsg::EndpointsFailed {
            url: url.to_string(),
            detail: e.to_string(),
        })?;

    Ok(endpoints)
}

/// Test-only override of the PKI store location (see `live_tests`).
#[cfg(test)]
pub(super) static TEST_PKI_DIR: std::sync::OnceLock<PathBuf> = std::sync::OnceLock::new();

pub(super) fn pki_dir() -> PathBuf {
    pki_dir_public()
}

/// Builds the client. The library reads the application key pair from the PKI
/// store here and, on the first run, creates it (certificate, then private key,
/// as two files). Two clients built at the same moment (the combined search
/// starts several, tests run many in parallel) could both see "missing", or one
/// could read a half-written pair; the loser then reported an error or failed
/// its first secure connect, and the next attempt (files complete by then)
/// worked. One lock makes the creation happen once, before any other client
/// looks at the files.
pub(super) fn build_client(
    builder: opcua::client::ClientBuilder,
) -> Result<opcua::client::Client, BackendMsg> {
    static BUILD: std::sync::Mutex<()> = std::sync::Mutex::new(());
    let _guard = BUILD
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner);
    builder
        .client()
        .map_err(|errors| BackendMsg::Raw(errors.join("; ")))
}

/// Process-wide redirect of the PKI store used by headless tests of the
/// application, so that they share neither the real store nor the working
/// directory's `opcda-egui/pki`. The application never sets it.
static PKI_DIR_OVERRIDE: std::sync::OnceLock<PathBuf> = std::sync::OnceLock::new();

/// Redirect this process's PKI store to a private temporary directory (once;
/// later calls return the same directory).
pub fn use_private_pki_dir() -> PathBuf {
    PKI_DIR_OVERRIDE
        .get_or_init(|| {
            let dir =
                std::env::temp_dir().join(format!("opcda-egui-test-pki-{}", std::process::id()));
            let _ = std::fs::remove_dir_all(&dir);
            dir
        })
        .clone()
}

/// Public accessor for the PKI store path (the connect dialog / About dialog
/// display it). Path: `%APPDATA%\opcda-egui\pki`.
pub fn pki_dir_public() -> PathBuf {
    // Live tests run many clients in one process: they share one scratch PKI
    // store instead of the user's real one.
    #[cfg(test)]
    if let Some(dir) = TEST_PKI_DIR.get() {
        return dir.clone();
    }
    if let Some(dir) = PKI_DIR_OVERRIDE.get() {
        return dir.clone();
    }
    // Unit tests that build a discovery client before live test setup must not
    // create certificates in the repository or the user's profile.
    #[cfg(test)]
    return use_private_pki_dir();
    #[cfg(not(test))]
    std::env::var_os("APPDATA")
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from("."))
        .join("opcda-egui")
        .join("pki")
}

/// Best-effort read of the client certificate subject from the PKI store.
/// Returns `None` when the cert has not been generated yet (first connect) or
/// cannot be read. Used by the «О программе» dialog.
pub fn client_cert_subject() -> Option<String> {
    let cert_path = pki_dir().join("own").join("cert.der");
    if !cert_path.exists() {
        return None;
    }
    opcua::crypto::CertificateStore::read_cert(&cert_path)
        .ok()
        .map(|cert| cert.subject_name())
        .map(|s| {
            if s.trim().is_empty() {
                "—".to_string()
            } else {
                s
            }
        })
}

#[cfg(test)]
mod build_client_tests {
    #[test]
    fn tests_use_a_temporary_pki_store() {
        assert!(super::pki_dir_public().starts_with(std::env::temp_dir()));
    }

    use super::*;
    use opcua::client::ClientBuilder;
    use opcua::crypto::CertificateStore;
    use std::sync::{Arc, Barrier, Mutex};

    /// Collects the library's error lines (the only place where it reports that
    /// creating or reading the key pair went wrong).
    struct Capture(Mutex<Vec<String>>);

    impl log::Log for Capture {
        fn enabled(&self, metadata: &log::Metadata) -> bool {
            metadata.level() <= log::Level::Error
        }
        fn log(&self, record: &log::Record) {
            if self.enabled(record.metadata()) {
                let mut lines = self.0.lock().unwrap_or_else(|e| e.into_inner());
                lines.push(record.args().to_string());
            }
        }
        fn flush(&self) {}
    }

    /// The first run creates the application key pair; clients built at the same
    /// moment must create it once and all find it complete. Without the lock the
    /// losers tried to create it again ("Certificate creation failed ... already
    /// exists") or read one half written.
    #[test]
    fn clients_built_concurrently_on_a_fresh_store_create_the_key_pair_once() {
        static CAPTURE: Capture = Capture(Mutex::new(Vec::new()));
        // Another test in this process may have installed a logger already; then
        // the lines cannot be observed and only the files are checked.
        let observing = log::set_logger(&CAPTURE).is_ok();
        if observing {
            log::set_max_level(log::LevelFilter::Error);
        }
        let dir =
            std::env::temp_dir().join(format!("opcda-egui-build-client-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        let threads = 8;
        let barrier = Arc::new(Barrier::new(threads));
        let handles: Vec<_> = (0..threads)
            .map(|_| {
                let dir = dir.clone();
                let barrier = Arc::clone(&barrier);
                std::thread::spawn(move || {
                    barrier.wait();
                    let builder = ClientBuilder::new()
                        .application_name("OPC DA/UA Test Client")
                        .application_uri("urn:opcda-egui")
                        .product_uri("urn:opcda-egui")
                        .create_sample_keypair(true)
                        .pki_dir(dir.clone());
                    build_client(builder)
                        .map_err(|e| crate::i18n::backend_text(crate::i18n::Lang::En, &e))?;
                    let store = CertificateStore::new(&dir);
                    store.read_own_cert()?;
                    CertificateStore::read_pkey(&store.own_private_key_path())?;
                    Ok::<(), String>(())
                })
            })
            .collect();
        for handle in handles {
            let result = handle.join().expect("client thread");
            assert!(result.is_ok(), "{result:?}");
        }
        if observing {
            // Other tests of this process log too (connection errors); only the
            // key-pair lines belong here.
            let lines: Vec<String> = CAPTURE
                .0
                .lock()
                .unwrap_or_else(|e| e.into_inner())
                .iter()
                .filter(|line| {
                    line.contains("Certificate creation failed")
                        || line.contains("missing its application instance certificate")
                })
                .cloned()
                .collect();
            assert!(lines.is_empty(), "the library reported: {lines:?}");
        }
        let _ = std::fs::remove_dir_all(&dir);
    }
}
