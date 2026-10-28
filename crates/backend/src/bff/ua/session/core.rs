use super::{
    Arc, BackendMsg, DataTypeTreeBuilder, DynamicTypeLoader, GRACEFUL_CLOSE_TIMEOUT, IO_TIMEOUT,
    NodeId, READ_MAX_ITEMS_PER_CALL, ReadValueId, STRUCT_TYPES_TIMEOUT, TimestampsToReturn,
    UaDataValue, UaSession, timeout_drain,
};

impl UaSession {
    /// V3: load the server's own structure types (namespaces other than 0;
    /// the standard ones are built into the client) and register a decoder
    /// for them, so a structured value arrives as named fields instead of an
    /// opaque `structured:…`. Best effort: a server without DataTypeDefinition
    /// (pre-1.04) just keeps the old text.
    pub(in crate::bff::ua) async fn load_struct_types(&mut self) -> Result<(), BackendMsg> {
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
        session.add_type_loader(Arc::new(DynamicTypeLoader::new(tree.clone())));
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
        let mut values = Vec::with_capacity(requests.len());
        for chunk in requests.chunks(READ_MAX_ITEMS_PER_CALL.max(1)) {
            let chunk_result =
                timeout_drain(self.session.read(chunk, timestamps, max_age), IO_TIMEOUT).await;
            let Ok(chunk_result) = chunk_result else {
                self.timed_out = true;
                return Err(BackendMsg::TimeoutRead);
            };
            match chunk_result {
                Ok(mut chunk_values) => values.append(&mut chunk_values),
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
