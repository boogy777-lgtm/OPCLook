use super::{
    Arc, BackendMsg, DataTypeTreeBuilder, DynamicTypeLoader, GRACEFUL_CLOSE_TIMEOUT, IO_TIMEOUT,
    NodeId, READ_MAX_ITEMS_PER_CALL, ReadValueId, STRUCT_TYPES_TIMEOUT, TimestampsToReturn,
    UaDataValue, UaSession, timeout_drain,
};

/// Bound untrusted labels before cloning or formatting a variable-size NodeId.
fn bounded_struct_type_label(name: &str, type_id: &NodeId) -> Option<String> {
    use opcua::types::Identifier;
    const MAX_TYPE_NAME_BYTES: usize = 256;
    if name.len() > MAX_TYPE_NAME_BYTES {
        return None;
    }
    if !name.trim().is_empty() {
        return Some(name.to_owned());
    }
    match &type_id.identifier {
        Identifier::String(value)
            if value
                .value()
                .as_ref()
                .is_none_or(|v| v.len() > MAX_TYPE_NAME_BYTES) =>
        {
            return None;
        }
        Identifier::ByteString(value) if value.len() > MAX_TYPE_NAME_BYTES => return None,
        _ => {}
    }
    let label = type_id.to_string();
    (label.len() <= MAX_TYPE_NAME_BYTES).then_some(label)
}

fn advertised_read_chunk(values: &[UaDataValue]) -> usize {
    let limit = values.first().and_then(|value| {
        if value.status.is_some_and(|status| !status.is_good()) {
            return None;
        }
        match value.value.as_ref()? {
            opcua::types::Variant::UInt32(n) if *n > 0 => Some(*n as usize),
            _ => None,
        }
    });
    limit
        .unwrap_or(READ_MAX_ITEMS_PER_CALL.max(1))
        .min(READ_MAX_ITEMS_PER_CALL.max(1))
}

impl UaSession {
    /// Capture the remote-session revision only while namespace identity is ready.
    /// A later check rejects replies from a session which was lost or replaced.
    pub(in crate::bff::ua) fn identity_revision(&self) -> Option<u64> {
        use std::sync::atomic::Ordering;
        if !self.identity_gate.load(Ordering::SeqCst) {
            return None;
        }
        let revision = self.recovery.borrow().0;
        self.identity_gate
            .load(Ordering::SeqCst)
            .then_some(revision)
    }

    pub(in crate::bff::ua) fn identity_is_current(&self, revision: u64) -> bool {
        self.identity_revision() == Some(revision)
    }

    /// Stop all value delivery and require owner-driven SDK session close when
    /// an Add's SDK side effects span two remote-session revisions.
    pub(in crate::bff::ua) fn check_add_scope(
        &mut self,
        revision: u64,
        service: crate::bff::model::UaService,
    ) -> bool {
        if self.identity_quarantined.is_none() && self.identity_is_current(revision) {
            return true;
        }
        self.quarantine_add(service);
        false
    }

    pub(in crate::bff::ua) fn quarantine_add(&mut self, service: crate::bff::model::UaService) {
        self.block_pushes();
        self.identity_quarantined.get_or_insert(service);
    }

    pub(in crate::bff::ua) fn block_pushes(&self) {
        self.identity_gate
            .store(false, std::sync::atomic::Ordering::SeqCst);
        self.push_keys
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .clear();
    }

    /// Snapshot SDK namespace indices once; interned strings have a session budget.
    pub(in crate::bff::ua) fn namespace_snapshot(
        &mut self,
    ) -> std::collections::HashMap<u16, Arc<str>> {
        let context = self.session.encoding_context();
        let guard = context.read();
        self.namespace_pool.snapshot(
            guard
                .namespaces()
                .known_namespaces()
                .iter()
                .map(|(uri, &index)| (uri.as_str(), index)),
        )
    }

    /// Unknown or changed namespace indices cannot safely identify the old item.
    pub(in crate::bff::ua) fn validate_namespace_identity(
        &mut self,
        snapshot_fresh: bool,
    ) -> Vec<super::ItemKey> {
        let snapshot = snapshot_fresh.then(|| self.namespace_snapshot());
        let mut blocked = Vec::new();
        for (&key, node) in &self.items {
            let expected = self.namespace_uris.get(&key).and_then(Option::as_deref);
            let actual = snapshot
                .as_ref()
                .and_then(|map| map.get(&node.namespace))
                .map(Arc::as_ref);
            if self.identity_blocked.contains(&key)
                || !super::recovery::namespace_matches(expected, actual)
            {
                blocked.push(key);
            }
        }
        blocked.sort_unstable();
        blocked
    }
    pub(in crate::bff::ua) async fn refresh_node_types(
        &mut self,
    ) -> Result<(usize, usize, u64, Vec<crate::bff::model::UaItemMetadata>), BackendMsg> {
        use super::{AttributeId, NumericRange, QualifiedName, ua_type_name, ua_variant_scalar};
        let revision = self
            .identity_revision()
            .ok_or(BackendMsg::UaIdentityChanged {
                service: crate::bff::model::UaService::Read,
            })?;
        let total = self.items.len();
        let mut items: Vec<_> = self
            .items
            .iter()
            .filter(|(key, _)| !self.identity_blocked.contains(key))
            .map(|(key, node)| (*key, node.clone()))
            .collect();
        items.sort_unstable_by_key(|(key, _)| *key);
        self.node_types.clear();
        let requests: Vec<_> = items
            .iter()
            .flat_map(|(_, node)| {
                [AttributeId::DataType, AttributeId::UserAccessLevel]
                    .into_iter()
                    .map(|attribute| ReadValueId {
                        node_id: node.clone(),
                        attribute_id: attribute as u32,
                        index_range: NumericRange::None,
                        data_encoding: QualifiedName::null(),
                    })
            })
            .collect();
        let result = if requests.is_empty() {
            Vec::new()
        } else {
            self.read_values_chunked_scoped(
                &requests,
                TimestampsToReturn::Neither,
                0.0,
                Some(revision),
            )
            .await?
        };
        if !self.identity_is_current(revision) {
            return Err(BackendMsg::UaIdentityChanged {
                service: crate::bff::model::UaService::Read,
            });
        }
        let mut facts = Vec::with_capacity(items.len());
        for (index, (key, _)) in items.iter().enumerate() {
            let good = |dv: &&super::UaDataValue| dv.status.is_none_or(|status| status.is_good());
            let data = result.get(index * 2).filter(good);
            let access = result.get(index * 2 + 1).filter(good);
            let scalar = data.and_then(ua_variant_scalar);
            let data_type = scalar
                .map(|ty| ua_type_name(ty).to_string())
                .or_else(|| data.and_then(|dv| self.struct_type_name(dv)));
            let type_id = scalar.map(|ty| ty as u16);
            if let Some(ty) = scalar {
                self.node_types.insert(*key, ty);
            }
            let user_access_level = super::items::user_access_level(access);
            let diagnostic = [result.get(index * 2), result.get(index * 2 + 1)]
                .into_iter()
                .flatten()
                .filter_map(|dv| dv.status)
                .find(|status| !status.is_good())
                .map(|status| BackendMsg::UaBadStatus {
                    status: super::status_text(status),
                });
            facts.push(crate::bff::model::UaItemMetadata {
                key: *key,
                data_type,
                type_id,
                user_access_level,
                diagnostic,
            });
        }
        let known = facts.iter().filter(|fact| fact.data_type.is_some()).count();
        Ok((known, total, revision, facts))
    }
    /// V3: load the server's own structure types (namespaces other than 0;
    /// the standard ones are built into the client) and register a decoder
    /// for them, so a structured value arrives as named fields instead of an
    /// opaque `structured:…`. Best effort: a server without DataTypeDefinition
    /// (pre-1.04) just keeps the old text.
    pub(in crate::bff::ua) async fn load_struct_types(&mut self) -> Result<(), BackendMsg> {
        let expected_revision = *self.recovery.borrow();
        let session = self.session.clone();
        let work = async move {
            // The decoder resolves encoding ids through the namespace table.
            session.read_namespace_array().await?;
            let tree = DataTypeTreeBuilder::new(|id: &NodeId| id.namespace != 0)
                .build(&session)
                .await?;
            Ok::<_, opcua::types::Error>((session, tree))
        };
        let result = timeout_drain(work, STRUCT_TYPES_TIMEOUT).await;
        let Ok(result) = result else {
            // A late response is still a timeout; never reuse the session.
            self.timed_out = true;
            return Err(BackendMsg::TimeoutStructTypes);
        };
        let (session, tree) = result.map_err(|e| BackendMsg::raw(e.to_string()))?;
        let tree = Arc::new(tree);
        let mut callback_types = self
            .callback_types
            .write()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        if *self.recovery.borrow() != expected_revision {
            return Err(BackendMsg::raw(
                "UA type descriptions superseded during recovery",
            ));
        }
        session.add_type_loader(Arc::new(DynamicTypeLoader::new(tree.clone())));
        *callback_types = Some(tree.clone());
        self.types = Some(tree);
        Ok(())
    }

    /// V3: name of the loaded structure type a DataType attribute value
    /// points at, if any.
    pub(in crate::bff::ua) fn struct_type_name(&self, dv: &UaDataValue) -> Option<String> {
        let Some(opcua::types::Variant::NodeId(type_id)) = dv.value.as_ref() else {
            return None;
        };
        let info = self.types.as_ref()?.get_struct_type(type_id)?;
        bounded_struct_type_label(&info.name, type_id)
    }

    /// Closes the session on the wire: CloseSession + CloseSecureChannel, then
    /// waits for the event loop to finish on its own (it only returns once the
    /// channel is fully closed). Aborts it only as a last resort when the
    /// graceful close cannot complete in [`GRACEFUL_CLOSE_TIMEOUT`].
    pub(in crate::bff::ua) async fn close(&mut self) {
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

    /// Reads values in sequential chunks of at most [`READ_MAX_ITEMS_PER_CALL`]
    /// `ReadValueId`s per service call and concatenates the results in request
    /// order. A single huge Read response can exceed the server's chunk budget
    /// (CODESYS `MaxChunkCount = 5` -> `0x80800000`), so each call must stay small.
    ///
    /// On timeout the session is marked poisoned because a response may still be in
    /// flight; the caller closes it gracefully instead of reusing it.
    pub(in crate::bff::ua) async fn read_values_chunked(
        &mut self,
        requests: &[ReadValueId],
        timestamps: TimestampsToReturn,
        max_age: f64,
    ) -> Result<Vec<UaDataValue>, BackendMsg> {
        self.read_values_chunked_scoped(requests, timestamps, max_age, None)
            .await
    }

    pub(in crate::bff::ua) async fn read_values_chunked_scoped(
        &mut self,
        requests: &[ReadValueId],
        timestamps: TimestampsToReturn,
        max_age: f64,
        revision: Option<u64>,
    ) -> Result<Vec<UaDataValue>, BackendMsg> {
        if requests.is_empty() {
            return Ok(Vec::new());
        }
        let stale = || BackendMsg::UaIdentityChanged {
            service: crate::bff::model::UaService::Read,
        };
        let scope = revision.or_else(|| self.identity_revision());
        if let Some(scope) = scope {
            if self.read_chunk.is_none_or(|(cached, _)| cached != scope) {
                if !self.identity_is_current(scope) {
                    return Err(stale());
                }
                // Standard ServerCapabilities.OperationLimits.MaxNodesPerRead.
                let limit_node = ReadValueId {
                    node_id: opcua::types::VariableId::Server_ServerCapabilities_OperationLimits_MaxNodesPerRead.into(),
                    attribute_id: super::AttributeId::Value as u32,
                    index_range: super::NumericRange::None,
                    data_encoding: super::QualifiedName::null(),
                };
                let result = timeout_drain(
                    self.session
                        .read(&[limit_node], TimestampsToReturn::Neither, 0.0),
                    IO_TIMEOUT,
                )
                .await;
                if result.is_err()
                    || result.as_ref().is_ok_and(|result| {
                        result
                            .as_ref()
                            .is_err_and(|error| error.status() == super::StatusCode::BadTimeout)
                    })
                {
                    self.timed_out = true;
                }
                if !self.identity_is_current(scope) {
                    return Err(stale());
                }
                let Ok(result) = result else {
                    return Err(BackendMsg::TimeoutRead);
                };
                if result
                    .as_ref()
                    .is_err_and(|error| error.status() == super::StatusCode::BadTimeout)
                {
                    return Err(BackendMsg::TimeoutRead);
                }
                let limit = advertised_read_chunk(result.as_deref().unwrap_or(&[]));
                self.read_chunk = Some((scope, limit));
            }
        }
        let mut chunk_size = self
            .read_chunk
            .filter(|(cached, _)| Some(*cached) == scope)
            .map_or(READ_MAX_ITEMS_PER_CALL.max(1), |(_, size)| size.max(1));
        let mut values = Vec::with_capacity(requests.len());
        let mut start = 0usize;
        while start < requests.len() {
            let size = chunk_size.min(requests.len() - start);
            let Some(chunk) = requests.get(start..start + size) else {
                break;
            };
            if scope.is_some_and(|revision| !self.identity_is_current(revision)) {
                return Err(stale());
            }
            let chunk_result =
                timeout_drain(self.session.read(chunk, timestamps, max_age), IO_TIMEOUT).await;
            if chunk_result.is_err()
                || chunk_result.as_ref().is_ok_and(|result| {
                    result
                        .as_ref()
                        .is_err_and(|error| error.status() == super::StatusCode::BadTimeout)
                })
            {
                self.timed_out = true;
            }
            if scope.is_some_and(|revision| !self.identity_is_current(revision)) {
                return Err(stale());
            }
            let Ok(chunk_result) = chunk_result else {
                return Err(BackendMsg::TimeoutRead);
            };
            match chunk_result {
                Ok(mut chunk_values) => {
                    let complete = chunk_values.len() == size;
                    chunk_values.truncate(size);
                    values.append(&mut chunk_values);
                    // Do not shift later results onto missing keys.
                    if !complete {
                        break;
                    }
                    start += size;
                }
                Err(e) if super::is_too_many_operations(e.status()) && size > 1 => {
                    chunk_size = (size / 2).max(1);
                    if let Some(scope) = scope {
                        self.read_chunk = Some((scope, chunk_size));
                    }
                }
                Err(e) if e.status() == super::StatusCode::BadTimeout => {
                    return Err(BackendMsg::TimeoutRead);
                }
                Err(e) => {
                    return Err(BackendMsg::UaServiceError {
                        service: crate::bff::model::UaService::Read,
                        detail: e.to_string(),
                    });
                }
            }
        }
        Ok(values)
    }
}

#[cfg(test)]
mod struct_label_tests {
    use super::*;
    use opcua::types::{ByteString, Identifier};

    #[test]
    fn read_limit_absent_zero_bad_and_positive_are_bounded() {
        use opcua::types::{StatusCode, Variant};
        let value = |n| UaDataValue::new_now(Variant::UInt32(n));
        let fallback = READ_MAX_ITEMS_PER_CALL.max(1);
        assert_eq!(advertised_read_chunk(&[]), fallback);
        assert_eq!(advertised_read_chunk(&[value(0)]), fallback);
        assert_eq!(advertised_read_chunk(&[value(2)]), 2.min(fallback));
        assert_eq!(advertised_read_chunk(&[value(u32::MAX)]), fallback);
        let mut bad = value(2);
        bad.status = Some(StatusCode::BadNodeIdUnknown);
        assert_eq!(advertised_read_chunk(&[bad]), fallback);
    }

    #[test]
    fn unnamed_structure_uses_bounded_canonical_identifier() {
        let numeric = NodeId::new(2, 1234u32);
        assert_eq!(
            bounded_struct_type_label("", &numeric).as_deref(),
            Some("ns=2;i=1234")
        );
        assert_eq!(
            bounded_struct_type_label("PumpType", &numeric).as_deref(),
            Some("PumpType")
        );
        assert!(bounded_struct_type_label(&"x".repeat(257), &numeric).is_none());
        assert!(bounded_struct_type_label(&" ".repeat(257), &numeric).is_none());
        assert_eq!(
            bounded_struct_type_label(&" ".repeat(256), &numeric).as_deref(),
            Some("ns=2;i=1234")
        );
        let long_string = NodeId::new(2, "x".repeat(257));
        assert!(bounded_struct_type_label("", &long_string).is_none());
        let expanded_string = NodeId::new(2, "x".repeat(252));
        assert!(bounded_struct_type_label("", &expanded_string).is_none());
        let opaque = NodeId {
            namespace: 2,
            identifier: Identifier::ByteString(ByteString::from(vec![7; 193])),
        };
        assert!(bounded_struct_type_label("", &opaque).is_none());
        let oversized_opaque = NodeId {
            namespace: 2,
            identifier: Identifier::ByteString(ByteString::from(vec![7; 257])),
        };
        assert!(bounded_struct_type_label("", &oversized_opaque).is_none());
    }
}
