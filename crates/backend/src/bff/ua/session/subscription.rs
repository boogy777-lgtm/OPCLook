use super::{
    AttributeId, BackendMsg, DataChangeCallback, Duration, Event, EventKind, HashMap, IO_TIMEOUT,
    ItemFailure, ItemKey, MAX_SUBSCRIPTIONS_PER_SESSION, MonitorOutcome, MonitoredItem,
    MonitoredItemCreateRequest, MonitoredItemModifyRequest, MonitoringMode, MonitoringParameters,
    NodeId, NumericRange, QualifiedName, READ_MAX_ITEMS_PER_CALL, ReadValueId, Sender, StatusCode,
    SubscriptionState, TimestampsToReturn, UA_SUBSCRIPTION_KEEPALIVE_COUNT,
    UA_SUBSCRIPTION_LIFETIME_COUNT, UaDataValue, UaSession, create_outcomes, data_change_filter,
    is_filter_refusal, is_too_many_monitored, is_too_many_operations, monitoring_parameters, send,
    timeout_drain, ua_data_value_typed,
};
use crate::constants::UA_SAMPLING_INTERVAL_MS;

impl UaSession {
    pub(in crate::bff::ua) async fn ensure_subscription(
        &mut self,
        tx: &Sender<Event>,
    ) -> Result<(), BackendMsg> {
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
        let created = timeout_drain(
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
        let Ok(created) = created else {
            self.timed_out = true;
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
    pub(in crate::bff::ua) async fn apply_subscription_settings(
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
                let modified = timeout_drain(
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
                if modified.is_err() {
                    self.timed_out = true;
                    problems.push(BackendMsg::SubscriptionProblem {
                        cause: Box::new(BackendMsg::TimeoutPublishingInterval),
                    });
                    return problems;
                }
                match modified {
                    Ok(Ok(())) => {}
                    Ok(Err(e)) => {
                        all_applied = false;
                        problems.push(BackendMsg::SubscriptionProblem {
                            cause: Box::new(BackendMsg::ChangeIntervalFailed {
                                detail: e.to_string(),
                            }),
                        });
                    }
                    Err(_) => all_applied = false,
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
                let modified = timeout_drain(
                    self.session.modify_monitored_items(
                        sub_id,
                        TimestampsToReturn::Both,
                        &requests,
                    ),
                    IO_TIMEOUT,
                )
                .await;
                if modified.is_err() {
                    self.timed_out = true;
                    problems.push(BackendMsg::SubscriptionProblem {
                        cause: Box::new(BackendMsg::TimeoutDeadband),
                    });
                    return problems;
                }
                match modified {
                    Ok(Ok(results)) => {
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
                    Ok(Err(e)) => {
                        problems.push(BackendMsg::SubscriptionProblem {
                            cause: Box::new(BackendMsg::ChangeDeadbandFailed {
                                detail: e.to_string(),
                            }),
                        });
                        return problems;
                    }
                    Err(_) => return problems,
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
            let created = timeout_drain(
                self.session.create_monitored_items(
                    subscription_id,
                    TimestampsToReturn::Both,
                    requests,
                ),
                IO_TIMEOUT,
            )
            .await;
            let failure = if created.is_err() {
                self.timed_out = true;
                Some(BackendMsg::TimeoutMonitoredItems)
            } else {
                match created {
                    Ok(Ok(results)) => {
                        let keys: Vec<ItemKey> = chunk.iter().map(|(key, _)| *key).collect();
                        let statuses: Vec<(StatusCode, u32)> = results
                            .iter()
                            .map(|item| (item.result.status_code, item.result.monitored_item_id))
                            .collect();
                        out.extend(create_outcomes(&keys, &statuses));
                        start += size;
                        None
                    }
                    Ok(Err(e)) if is_too_many_operations(e.status()) && size > 1 => {
                        self.monitor_chunk = (size / 2).max(1);
                        continue;
                    }
                    Ok(Err(e)) if is_too_many_monitored(e.status()) => {
                        for (key, _) in round.iter().skip(start) {
                            out.push((*key, Err(ItemFailure::Status(e.status()))));
                        }
                        break;
                    }
                    Ok(Err(e)) => Some(BackendMsg::MonitorFailed {
                        detail: e.to_string(),
                    }),
                    Err(_) => Some(BackendMsg::TimeoutMonitoredItems),
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
    pub(in crate::bff::ua) async fn monitor_items(
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
        // Preserve the first occurrence for duplicate keys, as the former
        // linear lookup did, while resolving each result in constant time.
        let mut node_by_key = HashMap::with_capacity(nodes.len());
        for (key, node) in nodes {
            node_by_key.entry(*key).or_insert(node);
        }
        for (key, result) in self.create_round(subscription_id, &round, tx).await {
            let node = node_by_key.get(&key).copied();
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
            let mut retry_by_key = HashMap::with_capacity(retry.len());
            for (key, node) in &retry {
                retry_by_key.entry(*key).or_insert(node);
            }
            for (key, result) in self.create_round(subscription_id, &round, tx).await {
                match result {
                    Ok(id) => {
                        self.monitored.insert(key, (subscription_id, id));
                        self.unfiltered.insert(key);
                    }
                    Err(ItemFailure::Status(status)) if is_too_many_monitored(status) => {
                        if let Some(node) = retry_by_key.get(&key) {
                            overflow.push((key, (*node).clone()));
                        }
                    }
                    Err(failure) => outcome.rejected.push((key, failure.text())),
                }
            }
        }
        overflow
    }

    /// Report the current subscription state so the app knows whether to poll.
    pub(in crate::bff::ua) fn report_subscription_state(
        &mut self,
        tx: &Sender<Event>,
        state: SubscriptionState,
    ) {
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
    pub(in crate::bff::ua) fn log(&self, tx: &Sender<Event>, message: BackendMsg) {
        send(
            tx,
            Event {
                origin: crate::bff::model::Origin::Unsolicited,
                owner: self.session_id.into(),
                kind: EventKind::Log { message },
            },
        );
    }

    /// Best-effort cleanup: delete the subscription and report it inactive.
    pub(in crate::bff::ua) async fn teardown_subscription(&mut self, tx: &Sender<Event>) {
        for sub_id in std::mem::take(&mut self.subscriptions) {
            if self.timed_out {
                break;
            }
            let result = timeout_drain(self.session.delete_subscription(sub_id), IO_TIMEOUT).await;
            if result.is_err() {
                self.timed_out = true;
            }
        }
        self.monitored.clear();
        self.unfiltered.clear();
        self.poll_only.clear();
        self.report_subscription_state(tx, SubscriptionState::Inactive);
    }
}
