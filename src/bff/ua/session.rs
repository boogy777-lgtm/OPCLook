use super::discovery::build_discovery_client;
use super::io::read_values_chunked;
use super::security::{resolve_security, security_error};
use super::values::{
    data_change_filter, parse_node, ua_access_current_write, ua_data_value, ua_type_name,
    ua_variant_scalar,
};
use super::worker::{send, timeout_drain};
use crate::bff::model::{
    EndpointSpec, Event, EventKind, ItemInfo, ItemKey, ItemSpec, SessionId, SubscriptionState,
    UaEndpointInfo, UaSecurityPolicy,
};
use crate::bff::net::{BoundConnectionSource, BoundConnectorBuilder, NetworkBinding};
use crate::constants::{
    CONNECT_TIMEOUT, DISCOVERY_GUARD_TIMEOUT, GRACEFUL_CLOSE_TIMEOUT, IO_TIMEOUT,
    READ_MAX_ITEMS_PER_CALL,
};
use opcua::client::{
    ClientBuilder, DataChangeCallback, IdentityToken, MonitoredItem, Password, Session,
};
use opcua::types::{
    AttributeId, DataValue as UaDataValue, EndpointDescription, ExtensionObject,
    MessageSecurityMode, MonitoredItemCreateRequest, MonitoringMode, MonitoringParameters, NodeId,
    NumericRange, QualifiedName, ReadValueId, StatusCode, TimestampsToReturn, UserTokenPolicy,
    UserTokenType, VariantScalarTypeId,
};
use std::collections::{HashMap, HashSet};
use std::path::PathBuf;
use std::sync::Arc;
use std::sync::mpsc::Sender;
use std::time::Duration;
use tokio::task::JoinHandle;

pub(super) struct UaSession {
    pub(super) session: Arc<Session>,
    pub(super) event_loop: JoinHandle<StatusCode>,
    pub(super) items: HashMap<ItemKey, String>,
    /// UA built-in variant scalar type of each subscribed node, resolved from
    /// the node's DataType attribute when the attribute could be read.
    pub(super) node_types: HashMap<ItemKey, VariantScalarTypeId>,
    pub(super) info: String,
    /// The owning session id, used by the DataChange callback to tag pushes.
    pub(super) session_id: SessionId,
    /// Server-assigned subscription id, created lazily on the first AddItems.
    pub(super) subscription_id: Option<u32>,
    /// Requested publishing interval (ms) for the push subscription; mirrors the
    /// app's update interval. 0 until ConfigureUaSubscription arrives.
    pub(super) publishing_interval_ms: u64,
    /// Absolute deadband for DataChangeFilter (0 = off). Applied at creation.
    pub(super) deadband: f64,
    /// Server-assigned monitored item id of every item that is really monitored
    /// (created with a Good status). `delete`/`modify` address items by this id,
    /// NOT by the client handle (`ItemKey`).
    pub(super) monitored: HashMap<ItemKey, u32>,
    /// Items monitored WITHOUT the deadband filter because the server refused it
    /// for their attribute/type (for example a Boolean or String node).
    pub(super) unfiltered: HashSet<ItemKey>,
    /// Set when a request hit [`IO_TIMEOUT`]. A timed-out request may still have
    /// a response in flight, so the session is poisoned: the command loop must
    /// close it gracefully instead of issuing another request on it.
    pub(super) timed_out: bool,
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
    /// The call itself failed or timed out; the text says why.
    Call(String),
}

impl ItemFailure {
    pub(super) fn text(&self) -> String {
        match self {
            ItemFailure::Status(status) => status_text(*status),
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
                None => Err(ItemFailure::Call(
                    "сервер не вернул результат для элемента".to_string(),
                )),
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
    rejected: &HashMap<ItemKey, String>,
) -> Vec<(ItemKey, Result<ItemInfo, String>)> {
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
    pub(super) async fn add_items(
        &mut self,
        items: &[ItemSpec],
        tx: &Sender<Event>,
    ) -> (Vec<(ItemKey, Result<ItemInfo, String>)>, Option<String>) {
        // Query the DataType and AccessLevel attributes once per node so the
        // write path can honor the server's actual type and writability.
        let mut rejected: HashMap<ItemKey, String> = HashMap::new();
        let mut requests = Vec::new();
        let mut plan: Vec<(ItemKey, NodeId)> = Vec::new();
        for item in items {
            let Some(node) = parse_node(&item.item_id) else {
                rejected.insert(
                    item.key,
                    format!("некорректный NodeId: {}", item.item_id.trim()),
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
                        Some(format!(
                            "не удалось прочитать атрибуты узлов OPC UA, тип/доступ не определён: {e}"
                        )),
                    );
                }
            }
        };

        let mut infos: HashMap<ItemKey, ItemInfo> = HashMap::with_capacity(plan.len());
        for (idx, (key, _)) in plan.iter().enumerate() {
            let mut scalar = None;
            let mut writable = false;
            if let Some(values) = &raw {
                if let Some(dv) = values.get(idx * 2) {
                    scalar = ua_variant_scalar(dv);
                }
                if let Some(dv) = values.get(idx * 2 + 1) {
                    writable = ua_access_current_write(dv).unwrap_or(false);
                }
            }
            let data_type = scalar.map(|t| ua_type_name(t).to_string());
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
                    rejected.insert(*key, status_text(status));
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
            .filter(|(key, _)| !self.node_types.contains_key(key))
            .count();
        let warning = if attribute_failures > 0 {
            Some(format!(
                "не удалось определить тип {attribute_failures} узлов OPC UA, запись выполняется с авто-типом"
            ))
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
                if let Some(sub_id) = self.subscription_id {
                    for (key, reason) in self.monitor_items(tx, sub_id, &plan).await {
                        rejected.insert(key, reason);
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
            }
            Err(message) => {
                let warning = match warning {
                    Some(mut warning) => {
                        warning.push_str(&format!("\nподписка не создана: {message}"));
                        warning
                    }
                    None => format!("подписка не создана: {message}"),
                };
                return (merge_add_results(items, infos, &rejected), Some(warning));
            }
        }

        (merge_add_results(items, infos, &rejected), warning)
    }

    /// Create the single per-session subscription on its first use. Publishing
    /// interval mirrors the app update interval (default 1000 ms) and the
    /// DataChange callback forwards quality/timestamp-mapped values to the UI.
    async fn ensure_subscription(&mut self, tx: &Sender<Event>) -> Result<(), String> {
        if self.subscription_id.is_some() {
            return Ok(());
        }
        let interval_ms = self.publishing_interval_ms.clamp(100, 3600_000);
        let interval = Duration::from_millis(interval_ms);
        let session_id = self.session_id;
        let event_tx = tx.clone();
        let callback =
            DataChangeCallback::new(move |data_value: UaDataValue, item: &MonitoredItem| {
                let key = item.client_handle();
                let _ = event_tx.send(Event {
                    request_id: 0,
                    session: session_id,
                    kind: EventKind::Values {
                        values: vec![(key, ua_data_value(&data_value))],
                    },
                });
            });
        let (created, timed_out) = timeout_drain(
            self.session.create_subscription(
                interval, 60,   // lifetime count (>= 3 * keep_alive)
                20,   // keep_alive count
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
            return Err("таймаут создания подписки OPC UA".to_string());
        }
        let Some(created) = created else {
            return Err("таймаут создания подписки OPC UA".to_string());
        };
        let sub_id = created.map_err(|e| format!("не удалось создать подписку OPC UA: {e}"))?;
        self.subscription_id = Some(sub_id);
        Ok(())
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
            requested_parameters: MonitoringParameters {
                client_handle: key,
                sampling_interval: 0.0,
                filter: if with_filter {
                    data_change_filter(self.deadband)
                } else {
                    ExtensionObject::null()
                },
                queue_size: 1,
                discard_oldest: true,
            },
        }
    }

    /// Sends one `CreateMonitoredItems` round (chunked) and returns, in request
    /// order, either the server-assigned monitored item id or the Bad status the
    /// server gave that item. A failure of the whole call is an `Err`.
    async fn create_round(
        &mut self,
        subscription_id: u32,
        round: &[(ItemKey, MonitoredItemCreateRequest)],
        tx: &Sender<Event>,
    ) -> Vec<(ItemKey, Result<u32, ItemFailure>)> {
        let mut out: Vec<(ItemKey, Result<u32, ItemFailure>)> = Vec::with_capacity(round.len());
        for chunk in round.chunks(READ_MAX_ITEMS_PER_CALL.max(1)) {
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
                Some("таймаут создания контролируемых элементов".to_string())
            } else {
                match created {
                    Some(Ok(results)) => {
                        let keys: Vec<ItemKey> = chunk.iter().map(|(key, _)| *key).collect();
                        let statuses: Vec<(StatusCode, u32)> = results
                            .iter()
                            .map(|item| (item.result.status_code, item.result.monitored_item_id))
                            .collect();
                        out.extend(create_outcomes(&keys, &statuses));
                        None
                    }
                    Some(Err(e)) => Some(format!("не удалось контролировать элементы: {e}")),
                    None => Some("таймаут создания контролируемых элементов".to_string()),
                }
            };
            if let Some(reason) = failure {
                // The whole call failed: every item of this and the remaining
                // chunks is reported with the reason, never silently dropped.
                self.log(tx, format!("подписка: {reason}"));
                let done = out.len();
                for (key, _) in round.iter().skip(done) {
                    out.push((*key, Err(ItemFailure::Call(reason.clone()))));
                }
                break;
            }
        }
        out
    }

    /// Create monitored items for `nodes` on the session subscription, mapping
    /// each ItemKey to the monitored item's client handle and applying the
    /// configured absolute deadband filter (0 = none). Every item's own
    /// `status_code` is checked: a Bad item is NOT monitored and is returned with
    /// the server's reason, so the caller can show it instead of leaving the row
    /// "waiting for a value" forever. An item refused only because of the
    /// deadband filter (Boolean/String nodes) is monitored again without it.
    async fn monitor_items(
        &mut self,
        tx: &Sender<Event>,
        subscription_id: u32,
        nodes: &[(ItemKey, NodeId)],
    ) -> Vec<(ItemKey, String)> {
        let round: Vec<(ItemKey, MonitoredItemCreateRequest)> = nodes
            .iter()
            .map(|(key, node)| (*key, self.monitor_request(*key, node, true)))
            .collect();
        let mut rejected: Vec<(ItemKey, String)> = Vec::new();
        let mut retry: Vec<(ItemKey, NodeId)> = Vec::new();
        for (key, outcome) in self.create_round(subscription_id, &round, tx).await {
            match outcome {
                Ok(id) => {
                    self.monitored.insert(key, id);
                }
                Err(failure) => {
                    let node = nodes.iter().find(|(k, _)| *k == key).map(|(_, n)| n);
                    match node {
                        Some(node)
                            if failure.is_filter_refusal()
                                && self.deadband > 0.0
                                && !self.timed_out =>
                        {
                            retry.push((key, node.clone()));
                        }
                        _ => rejected.push((key, failure.text())),
                    }
                }
            }
        }

        if !retry.is_empty() {
            let round: Vec<(ItemKey, MonitoredItemCreateRequest)> = retry
                .iter()
                .map(|(key, node)| (*key, self.monitor_request(*key, node, false)))
                .collect();
            for (key, outcome) in self.create_round(subscription_id, &round, tx).await {
                match outcome {
                    Ok(id) => {
                        self.monitored.insert(key, id);
                        self.unfiltered.insert(key);
                    }
                    Err(failure) => rejected.push((key, failure.text())),
                }
            }
        }
        rejected
    }

    /// Report the current subscription state so the app knows whether to poll.
    fn report_subscription_state(&mut self, tx: &Sender<Event>, state: SubscriptionState) {
        send(
            tx,
            Event {
                request_id: 0,
                session: self.session_id,
                kind: EventKind::SubscriptionState { state },
            },
        );
    }

    /// Unsolicited diagnostic line for the application journal.
    fn log(&self, tx: &Sender<Event>, message: String) {
        send(
            tx,
            Event {
                request_id: 0,
                session: self.session_id,
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
    ) -> Vec<String> {
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
        if let Some(sub_id) = self.subscription_id {
            let ids: Vec<u32> = known
                .iter()
                .filter_map(|key| self.monitored.get(key).copied())
                .collect();
            for chunk in ids.chunks(READ_MAX_ITEMS_PER_CALL.max(1)) {
                let (delete_result, timed_out) = timeout_drain(
                    self.session.delete_monitored_items(sub_id, chunk),
                    IO_TIMEOUT,
                )
                .await;
                if timed_out {
                    self.timed_out = true;
                    poisoned = true;
                    break;
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
                            problems.push(format!(
                                "подписка: сервер отклонил удаление {bad} элементов"
                            ));
                        }
                    }
                    Some(Err(e)) => {
                        problems.push(format!("подписка: не удалось удалить элементы: {e}"));
                        break;
                    }
                    None => break,
                }
            }
        }

        // Prune the maps now so a subsequent add re-monitors correctly.
        for key in &known {
            self.items.remove(key);
            self.node_types.remove(key);
            self.monitored.remove(key);
            self.unfiltered.remove(key);
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
        if let Some(sub_id) = self.subscription_id.take() {
            let (_, timed_out) =
                timeout_drain(self.session.delete_subscription(sub_id), IO_TIMEOUT).await;
            if timed_out {
                self.timed_out = true;
            }
        }
        self.monitored.clear();
        self.unfiltered.clear();
        self.report_subscription_state(tx, SubscriptionState::Inactive);
    }
}

pub(super) fn describe_connect_error(
    url: &str,
    pki: &std::path::Path,
    error: opcua::types::Error,
) -> String {
    let msg = error.to_string();
    if certification_error(&msg) {
        format!(
            "сертификат сервера {url} не доверенный: {msg}\nВключите «Доверять сертификату сервера» или импортируйте сертификат в PKI-хранилище: {}",
            pki.display()
        )
    } else {
        format!("не удалось подключиться к {url}: {msg}")
    }
}

/// Spawn the session event loop and wait for the session to activate. `build()`
/// itself does not open a connection, so this is the first network step after
/// the endpoint description has been selected.
async fn activate_ua_session<C>(
    session: &Session,
    event_loop: opcua::client::SessionEventLoop<C>,
    url: &str,
) -> Result<JoinHandle<StatusCode>, String>
where
    C: opcua::client::transport::Connector + Send + Sync + 'static,
{
    let mut event_loop = event_loop.spawn();
    let connected = tokio::time::timeout(CONNECT_TIMEOUT, session.wait_for_connection()).await;
    match connected {
        Ok(true) => Ok(event_loop),
        Ok(false) => {
            let _ = tokio::time::timeout(GRACEFUL_CLOSE_TIMEOUT, session.disconnect()).await;
            let _ = tokio::time::timeout(GRACEFUL_CLOSE_TIMEOUT, &mut event_loop).await;
            Err(format!("сервер {url} закрыл сессию при подключении"))
        }
        Err(_) => {
            // Timeout activating the session: the transport is already connected
            // so the server expects a CloseSession/CloseSecureChannel. Shut the
            // channel down gracefully instead of aborting the loop mid-request.
            let _ = tokio::time::timeout(GRACEFUL_CLOSE_TIMEOUT, session.disconnect()).await;
            let _ = tokio::time::timeout(GRACEFUL_CLOSE_TIMEOUT, &mut event_loop).await;
            Err("таймаут активации OPC UA сессии".to_string())
        }
    }
}

pub(super) async fn connect(
    session_id: SessionId,
    endpoint: EndpointSpec,
) -> Result<UaSession, String> {
    let url = endpoint.address.trim();
    if !url.starts_with("opc.tcp://") {
        return Err("URL OPC UA должен начинаться с opc.tcp://".to_string());
    }
    let binding = endpoint.ua_bind.clone();

    // 1. Fetch the server's actual endpoints so we can pick a security policy
    //    and mode the server really offers instead of guessing blindly. The full
    //    descriptions are kept so the connect description can reuse the server's
    //    own user-token policies (CODESYS advertises "Anonymous", not the
    //    crate's hardcoded "anonymous", and rejects ActivateSession otherwise
    //    with BadIdentityTokenInvalid).
    let server_endpoints = match discover_endpoints(url, binding.as_ref()).await {
        Ok(list) => list,
        Err(message) => return Err(message),
    };
    let discovered: Vec<UaEndpointInfo> = server_endpoints
        .iter()
        .map(|e| UaEndpointInfo {
            url: e.endpoint_url.to_string(),
            security_policy: e.security_policy_uri.to_string(),
            security_mode: format!("{:?}", e.security_mode),
        })
        .collect();

    // 2. Resolve the requested policy/mode (Auto default) against what the
    //    server advertises. An unsupported explicit choice is a hard error: no
    //    silent fallback to None/other.
    let (policy_uri, mode) = resolve_security(
        endpoint.ua_security_policy.unwrap_or_default(),
        endpoint.ua_security_mode.unwrap_or_default(),
        &discovered,
    )
    .map_err(security_error)?;

    let policy_short = UaSecurityPolicy::from_advertised(&policy_uri)
        .map(|p| p.as_str().to_string())
        .unwrap_or_else(|| policy_uri.clone());

    // 3. Build the client with the PKI store and the user's trust preference.
    let pki = pki_dir();
    let mut builder = ClientBuilder::new()
        .application_name("OPC DA/UA Test Client")
        .application_uri("urn:opcda-egui")
        .product_uri("urn:opcda-egui")
        .create_sample_keypair(true)
        // Trust flag drives auto-storing server certs into the trusted store.
        .trust_server_certs(endpoint.ua_trust_server_cert)
        // When trusting server certs we also skip strict hostname/expiry checks.
        .verify_server_certs(!endpoint.ua_trust_server_cert)
        .pki_dir(pki.clone())
        .session_retry_limit(3);

    // 4. Session timeout (optional; the crate accepts a requested value and the
    //    server revises it, but the revised value is not exposed — see docs).
    if let Some(timeout_ms) = endpoint.ua_session_timeout {
        builder = builder.session_timeout(timeout_ms);
    }

    let client = builder.client().map_err(|errors| errors.join("; "))?;

    // 5. Reuse the selected server's full endpoint description, including its
    //    actual user-token policies.
    let description = endpoint_description_for(&server_endpoints, url, &policy_uri, mode);

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
        let event_loop = activate_ua_session(&session, event_loop, url).await?;
        (session, event_loop)
    } else {
        let (session, event_loop) = client
            .session_builder()
            .connect_to_endpoint_directly(description)
            .map_err(|error| describe_connect_error(url, &pki, error))?
            .user_identity_token(identity)
            .build(client.certificate_store().clone())
            .map_err(|error| describe_connect_error(url, &pki, error))?;
        let event_loop = activate_ua_session(&session, event_loop, url).await?;
        (session, event_loop)
    };

    let mode_text = format!("{mode:?}");
    Ok(UaSession {
        session,
        event_loop,
        items: HashMap::new(),
        node_types: HashMap::new(),
        info: format!(
            "подключено к {url} [{policy_short} / {mode_text}]{}",
            if mode == MessageSecurityMode::None {
                ""
            } else {
                " (TLS)"
            }
        ),
        session_id,
        subscription_id: None,
        publishing_interval_ms: 0,
        deadband: 0.0,
        monitored: HashMap::new(),
        unfiltered: HashSet::new(),
        timed_out: false,
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
) -> Result<IdentityToken, String> {
    if endpoint.ua_use_certificate {
        if description
            .find_policy(UserTokenType::Certificate)
            .is_none()
        {
            return Err(
                "выбранная точка безопасности не поддерживает вход по сертификату (Certificate)"
                    .to_string(),
            );
        }
        let pki = pki_dir();
        let cert_path = pki.join("own").join("cert.der");
        let key_path = pki.join("private").join("private.pem");
        if !cert_path.exists() || !key_path.exists() {
            return Err(format!(
                "клиентский сертификат не найден в PKI ({}, {}) — подключитесь один раз с обычным входом, чтобы сгенерировать его",
                cert_path.display(),
                key_path.display()
            ));
        }
        return IdentityToken::new_x509_path(&cert_path, &key_path)
            .map_err(|e| format!("не удалось загрузить клиентский сертификат для X509: {e}"));
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
) -> Result<Vec<EndpointDescription>, String> {
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
        .map_err(|_| format!("таймаут запроса точек безопасности у {url}"))?
        .map_err(|e| format!("не удалось получить точки безопасности от {url}: {e}"))?;

    Ok(endpoints)
}

/// Test-only override of the PKI store location (see `live_tests`).
#[cfg(test)]
pub(super) static TEST_PKI_DIR: std::sync::OnceLock<PathBuf> = std::sync::OnceLock::new();

pub(super) fn pki_dir() -> PathBuf {
    pki_dir_public()
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
