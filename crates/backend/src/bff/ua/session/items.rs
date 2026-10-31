use super::{
    AttributeId, BackendMsg, Event, EventKind, HashMap, HashSet, IO_TIMEOUT, ItemInfo, ItemKey,
    ItemSpec, NodeId, NumericRange, QualifiedName, READ_MAX_ITEMS_PER_CALL, ReadValueId, Sender,
    StatusCode, SubscriptionState, TimestampsToReturn, UaSession, is_missing_node,
    merge_add_results, parse_node, send, status_text, timeout_drain, ua_access_current_write,
    ua_type_name, ua_variant_scalar,
};

pub(super) fn user_access_level(value: Option<&opcua::types::DataValue>) -> Option<u8> {
    let value = value?;
    if value.status.is_some_and(|status| !status.is_good()) {
        return None;
    }
    match value.value.as_ref() {
        Some(opcua::types::Variant::Byte(level)) => Some(*level),
        _ => None,
    }
}

type PriorRegistration = (
    Option<NodeId>,
    Option<Option<std::sync::Arc<str>>>,
    bool,
    Option<opcua::types::VariantScalarTypeId>,
);

fn rollback_add(session: &mut UaSession, previous: HashMap<ItemKey, PriorRegistration>) {
    for (key, (old_node, old_namespace, was_blocked, old_type)) in previous {
        if old_node.is_none() {
            session.monitored.remove(&key);
            session.unfiltered.remove(&key);
            session.poll_only.remove(&key);
        }
        match old_node {
            Some(node) => {
                session.items.insert(key, node);
            }
            None => {
                session.items.remove(&key);
            }
        }
        match old_namespace {
            Some(uri) => {
                session.namespace_uris.insert(key, uri);
            }
            None => {
                session.namespace_uris.remove(&key);
            }
        }
        match old_type {
            Some(ty) => {
                session.node_types.insert(key, ty);
            }
            None => {
                session.node_types.remove(&key);
            }
        }
        if was_blocked {
            session.identity_blocked.insert(key);
        } else {
            session.identity_blocked.remove(&key);
        }
    }
}

impl UaSession {
    #[cfg(test)]
    pub(in crate::bff::ua) async fn add_items(
        &mut self,
        items: &[ItemSpec],
        tx: &Sender<Event>,
    ) -> (
        Vec<(ItemKey, Result<ItemInfo, BackendMsg>)>,
        Option<BackendMsg>,
    ) {
        self.add_items_with_metadata(items, tx, &mut Vec::new())
            .await
    }

    pub(in crate::bff::ua) async fn add_items_with_metadata(
        &mut self,
        items: &[ItemSpec],
        tx: &Sender<Event>,
        metadata_out: &mut Vec<crate::bff::model::UaItemMetadata>,
    ) -> (
        Vec<(ItemKey, Result<ItemInfo, BackendMsg>)>,
        Option<BackendMsg>,
    ) {
        let identity_error = || BackendMsg::UaIdentityChanged {
            service: crate::bff::model::UaService::Read,
        };
        let Some(revision) = self.identity_revision() else {
            return (
                items
                    .iter()
                    .map(|item| (item.key, Err(identity_error())))
                    .collect(),
                None,
            );
        };
        // Preserve any pre-existing registration if this attempt becomes stale.
        let namespaces = self.namespace_snapshot();
        let mut previous = HashMap::new();
        // Query the DataType and UserAccessLevel attributes once per node so the
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
                attribute_id: AttributeId::UserAccessLevel as u32,
                index_range: NumericRange::None,
                data_encoding: QualifiedName::null(),
            });
            previous.entry(item.key).or_insert_with(|| {
                (
                    self.items.get(&item.key).cloned(),
                    self.namespace_uris.get(&item.key).cloned(),
                    self.identity_blocked.contains(&item.key),
                    self.node_types.get(&item.key).copied(),
                )
            });
            self.items.insert(item.key, node.clone());
            let namespace_uri = namespaces.get(&node.namespace).cloned();
            self.namespace_uris.insert(item.key, namespace_uri);
            self.identity_blocked.remove(&item.key);
            plan.push((item.key, node));
        }

        let raw_result = if requests.is_empty() {
            Ok(None)
        } else {
            self.read_values_chunked_scoped(
                &requests,
                TimestampsToReturn::Neither,
                0.0,
                Some(revision),
            )
            .await
            .map(Some)
        };
        if !self.identity_is_current(revision) {
            rollback_add(self, previous);
            for (key, _) in &plan {
                rejected.insert(*key, identity_error());
            }
            return (merge_add_results(items, HashMap::new(), &rejected), None);
        }
        let raw = match raw_result {
            Ok(values) => values,
            Err(e) => {
                let infos = plan
                    .iter()
                    .map(|(key, _)| {
                        (
                            *key,
                            ItemInfo {
                                data_type: None,
                                type_id: None,
                                writable: false,
                            },
                        )
                    })
                    .collect();
                return (
                    merge_add_results(items, infos, &rejected),
                    Some(BackendMsg::AttributesReadFailed { cause: Box::new(e) }),
                );
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
            let mut user_access_level = None;
            if let Some(values) = &raw {
                if let Some(dv) = values.get(idx * 2) {
                    scalar = dv
                        .status
                        .filter(|s| !s.is_good())
                        .map_or_else(|| ua_variant_scalar(dv), |_| None);
                    if scalar.is_none() && dv.status.is_none_or(|status| status.is_good()) {
                        struct_name = self.struct_type_name(dv);
                    }
                }
                if let Some(dv) = values.get(idx * 2 + 1) {
                    user_access_level = self::user_access_level(Some(dv));
                    writable = dv
                        .status
                        .filter(|s| !s.is_good())
                        .map_or_else(|| ua_access_current_write(dv).unwrap_or(false), |_| false);
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
            metadata_out.push(crate::bff::model::UaItemMetadata {
                key: *key,
                data_type: data_type.clone(),
                type_id,
                user_access_level,
                diagnostic: None,
            });
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
                    self.namespace_uris.remove(key);
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

        if self.poll_mode {
            return (merge_add_results(items, infos, &rejected), warning);
        }

        // After items are successfully registered, (re-)establish the push
        // subscription for this session. The same DataChange callback pushes
        // values into the shared event channel so rows update without polling.
        let ensured = self.ensure_subscription(tx, revision).await;
        if !self.check_add_scope(revision, crate::bff::model::UaService::CreateSubscription) {
            let service = self
                .identity_quarantined
                .unwrap_or(crate::bff::model::UaService::CreateSubscription);
            rollback_add(self, previous);
            for (key, _) in &plan {
                rejected.insert(*key, BackendMsg::UaIdentityChanged { service });
            }
            return (merge_add_results(items, HashMap::new(), &rejected), None);
        }
        if self.timed_out {
            self.block_pushes();
            rollback_add(self, previous);
            for (key, _) in &plan {
                rejected.insert(*key, BackendMsg::TimeoutCreateSubscription);
            }
            return (merge_add_results(items, HashMap::new(), &rejected), None);
        }
        match ensured {
            Ok(()) => {
                self.push_keys
                    .lock()
                    .unwrap_or_else(std::sync::PoisonError::into_inner)
                    .extend(plan.iter().map(|(key, _)| *key));
                let outcome = self.monitor_items(tx, &plan, revision).await;
                if !self
                    .check_add_scope(revision, crate::bff::model::UaService::CreateMonitoredItems)
                {
                    let service = self
                        .identity_quarantined
                        .unwrap_or(crate::bff::model::UaService::CreateMonitoredItems);
                    rollback_add(self, previous);
                    for (key, _) in &plan {
                        rejected.insert(*key, BackendMsg::UaIdentityChanged { service });
                    }
                    return (merge_add_results(items, HashMap::new(), &rejected), None);
                }
                if self.timed_out {
                    self.block_pushes();
                    rollback_add(self, previous);
                    for (key, _) in &plan {
                        rejected.insert(*key, BackendMsg::TimeoutMonitoredItems);
                    }
                    return (merge_add_results(items, HashMap::new(), &rejected), None);
                }
                for (key, reason) in outcome.rejected {
                    self.push_keys
                        .lock()
                        .unwrap_or_else(std::sync::PoisonError::into_inner)
                        .remove(&key);
                    rejected.insert(key, reason);
                }
                if !outcome.poll_only.is_empty() {
                    {
                        let mut allowed = self
                            .push_keys
                            .lock()
                            .unwrap_or_else(std::sync::PoisonError::into_inner);
                        for key in &outcome.poll_only {
                            allowed.remove(key);
                        }
                    }
                    // P1: over the server's limits: still read, by polling.
                    self.poll_only.extend(outcome.poll_only.iter().copied());
                    self.log(
                        tx,
                        if self.explicit_acquisition {
                            BackendMsg::SubscriptionItemsUnavailable {
                                count: outcome.poll_only.len(),
                            }
                        } else {
                            BackendMsg::PollOnlyItems {
                                count: outcome.poll_only.len(),
                            }
                        },
                    );
                    send(
                        tx,
                        Event {
                            origin: self.connection_id.map_or(
                                crate::bff::model::Origin::Unsolicited,
                                |connection| crate::bff::model::Origin::UaSession {
                                    connection,
                                    revision,
                                },
                            ),
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
                self.report_subscription_state_at(tx, state, revision);
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

    /// Delete monitored items for these keys, reporting any problems.
    pub(in crate::bff::ua) async fn remove_items(
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
        {
            let mut allowed = self
                .push_keys
                .lock()
                .unwrap_or_else(std::sync::PoisonError::into_inner);
            for key in &known {
                allowed.remove(key);
            }
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
                let delete_result = timeout_drain(
                    self.session.delete_monitored_items(sub_id, chunk),
                    IO_TIMEOUT,
                )
                .await;
                if delete_result.is_err() {
                    self.timed_out = true;
                    poisoned = true;
                    break 'subs;
                }
                match delete_result {
                    Ok(Ok(statuses)) => {
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
                    Ok(Err(e)) => {
                        problems.push(BackendMsg::SubscriptionProblem {
                            cause: Box::new(BackendMsg::DeleteFailed {
                                detail: e.to_string(),
                            }),
                        });
                        break 'subs;
                    }
                    Err(_) => break 'subs,
                }
            }
        }

        // Prune the maps now so a subsequent add re-monitors correctly.
        for key in &known {
            self.items.remove(key);
            self.namespace_uris.remove(key);
            self.identity_blocked.remove(key);
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
}

#[cfg(test)]
mod metadata_access_tests {
    #[test]
    fn bad_or_missing_access_never_becomes_a_write_grant() {
        use opcua::types::{DataValue, StatusCode};
        let mut value = DataValue::value_only(3u8);
        assert_eq!(super::user_access_level(Some(&value)), Some(3));
        value.status = Some(StatusCode::BadUserAccessDenied);
        assert_eq!(super::user_access_level(Some(&value)), None);
        value.status = Some(StatusCode::Good);
        value.value = None;
        assert_eq!(super::user_access_level(Some(&value)), None);
        assert_eq!(super::user_access_level(None), None);
    }
}
