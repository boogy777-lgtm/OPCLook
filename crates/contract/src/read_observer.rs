//! A bounded, non-I/O sink for consumers of read terminal notifications.
use crate::model::{ItemKey, ReadTerminal, ReadTerminalStatus, RequestId, SessionId};
use std::collections::HashMap;

/// The maximum number of distinct item observations held between drains.
pub const MAX_READ_OBSERVATIONS: usize = crate::model::MAX_ITEM_KEYS_PER_CONNECTION;

#[derive(Clone, Debug)]
pub struct ReadObservation {
    pub session: SessionId,
    pub epoch: Option<RequestId>,
    /// Received values have their value payload removed, while quality,
    /// timestamp and reason remain intact. Full values arrive in normal events.
    pub terminal: ReadTerminal,
}

#[derive(Default)]
pub struct ReadObservations {
    latest: HashMap<(SessionId, ItemKey), ReadObservation>,
    replaced: usize,
    overflow: usize,
}

#[derive(Default)]
pub struct ReadObservationBatch {
    pub latest: Vec<ReadObservation>,
    /// Earlier observations for a key were superseded in this batch.
    pub replaced: usize,
    /// New keys could not be stored because the bound was reached.
    pub overflow: usize,
}

impl ReadObservations {
    /// Called by the synchronous Engine sink; performs no I/O.
    pub fn record(
        &mut self,
        session: SessionId,
        epoch: Option<RequestId>,
        terminal: &ReadTerminal,
    ) {
        let key = (session, terminal.key);
        if !self.latest.contains_key(&key) && self.latest.len() >= MAX_READ_OBSERVATIONS {
            self.overflow = self.overflow.saturating_add(1);
            return;
        }
        let status = match &terminal.status {
            ReadTerminalStatus::Received(value) => ReadTerminalStatus::ReceivedMetadata {
                quality: value.quality.clone(),
                timestamp: value.timestamp,
                times: value.times.clone(),
                reason: value.reason.clone(),
            },
            status => status.clone(),
        };
        let terminal = ReadTerminal {
            request_id: terminal.request_id,
            key: terminal.key,
            needs: terminal.needs.clone(),
            status,
        };
        if self
            .latest
            .insert(
                key,
                ReadObservation {
                    session,
                    epoch,
                    terminal,
                },
            )
            .is_some()
        {
            self.replaced = self.replaced.saturating_add(1);
        }
    }

    pub fn drain(&mut self) -> ReadObservationBatch {
        ReadObservationBatch {
            latest: self.latest.drain().map(|(_, value)| value).collect(),
            replaced: std::mem::take(&mut self.replaced),
            overflow: std::mem::take(&mut self.overflow),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::{ReadNeed, ReadTerminalStatus};
    #[test]
    fn latest_per_key_reports_coalescing_and_clears_on_drain() {
        let mut buffer = ReadObservations::default();
        let terminal = ReadTerminal {
            request_id: None,
            key: ItemKey::FIRST,
            needs: vec![ReadNeed::Initial],
            status: ReadTerminalStatus::MissingResult,
        };
        buffer.record(SessionId::FIRST, None, &terminal);
        buffer.record(SessionId::FIRST, None, &terminal);
        let batch = buffer.drain();
        assert_eq!(batch.latest.len(), 1);
        assert_eq!(batch.replaced, 1);
        assert_eq!(batch.overflow, 0);
        assert!(buffer.drain().latest.is_empty());
    }
    #[test]
    fn large_values_are_not_retained_and_overflow_is_reported() {
        use crate::model::{BackendMsg, DataValue, Quality, QualityKind, Value};
        let mut buffer = ReadObservations::default();
        let terminal = ReadTerminal {
            request_id: None,
            key: ItemKey::FIRST,
            needs: vec![ReadNeed::Direct],
            status: ReadTerminalStatus::Received(DataValue {
                value: Value::Bytes(vec![7; 1024 * 1024]),
                quality: Quality::from_ua(0x4000_0000, "uncertain"),
                timestamp: None,
                reason: Some(BackendMsg::Raw("reason".into())),
                times: Default::default(),
            }),
        };
        buffer.record(SessionId::FIRST, None, &terminal);
        let batch = buffer.drain();
        assert!(
            matches!(&batch.latest[0].terminal.status, ReadTerminalStatus::ReceivedMetadata { quality, reason: Some(BackendMsg::Raw(reason)), .. } if quality.kind == QualityKind::Uncertain && reason == "reason")
        );
        // Fill the actual bound: a new key is counted as overflow, not silently dropped.
        let mut item = ReadTerminal {
            request_id: None,
            key: ItemKey::FIRST,
            needs: vec![ReadNeed::Direct],
            status: ReadTerminalStatus::MissingResult,
        };
        for raw in 1..=MAX_READ_OBSERVATIONS as u32 {
            item.key = ItemKey::new(raw).unwrap();
            buffer.record(SessionId::FIRST, None, &item);
        }
        let mut extra = item;
        extra.key = ItemKey::new(MAX_READ_OBSERVATIONS as u32 + 1).unwrap();
        buffer.record(SessionId::FIRST, None, &extra);
        let batch = buffer.drain();
        assert_eq!(batch.latest.len(), MAX_READ_OBSERVATIONS);
        assert_eq!(batch.overflow, 1);
    }
}
