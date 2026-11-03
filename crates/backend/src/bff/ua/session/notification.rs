//! Read-only observations of the SDK-owned Publish stream; no replay owner.
use crate::bff::model::{Origin, RequestId, UaNotificationEvidence};

pub(super) fn scoped_origin(connection: Option<RequestId>, revision: u64) -> Origin {
    connection.map_or(Origin::Unsolicited, |connection| Origin::UaSession {
        connection,
        revision,
    })
}

pub(super) fn data_overflow(status: Option<opcua::types::StatusCode>) -> Option<u32> {
    status
        .filter(|s| s.info_type() == opcua::types::StatusCodeInfoType::DataValue && s.overflow())
        .map(|s| s.bits())
}

#[derive(Default)]
pub(super) struct SequenceTracker {
    revision: Option<u64>,
    last: Option<u32>,
}
impl SequenceTracker {
    pub(super) fn observe(
        &mut self,
        revision: u64,
        sequence: u32,
        has_notifications: bool,
    ) -> Option<UaNotificationEvidence> {
        if self.revision != Some(revision) {
            self.revision = Some(revision);
            self.last = None;
        }
        // A keepalive advertises the next sequence; it is not a message in
        // the notification sequence and must not advance the cursor.
        if !has_notifications {
            return None;
        }
        if sequence == 0 {
            return Some(UaNotificationEvidence::SequenceUncertain {
                previous: self.last.unwrap_or(0),
                current: 0,
            });
        }
        let Some(previous) = self.last else {
            self.last = Some(sequence);
            return None;
        };
        if sequence == previous {
            return None;
        }
        let expected = if previous == u32::MAX {
            1
        } else {
            previous + 1
        };
        if sequence == expected {
            self.last = Some(sequence);
            return None;
        }
        if sequence > previous {
            self.last = Some(sequence);
            return Some(UaNotificationEvidence::SequenceGap {
                previous,
                current: sequence,
            });
        }
        // Backwards or non-adjacent wrap could be reordered/replayed; don't
        // invent a loss count or move the cursor behind the last observation.
        Some(UaNotificationEvidence::SequenceUncertain {
            previous,
            current: sequence,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn only_data_value_overflow_is_reported_with_raw_bits() {
        use opcua::types::{StatusCode, StatusCodeInfoType};
        assert_eq!(data_overflow(None), None);
        assert_eq!(data_overflow(Some(StatusCode::Good)), None);
        assert_eq!(
            data_overflow(Some(StatusCode::Good.set_overflow(true))),
            None
        );
        let status = StatusCode::Good
            .set_info_type(StatusCodeInfoType::DataValue)
            .set_overflow(true);
        assert_eq!(data_overflow(Some(status)), Some(status.bits()));
    }
    #[test]
    fn old_notification_cannot_be_relabelled_after_recovery_before_delegate() {
        use std::sync::{
            Arc, Mutex,
            atomic::{AtomicBool, Ordering},
        };
        let keys = Arc::new(Mutex::new(std::collections::HashSet::from([
            crate::bff::model::ItemKey::FIRST,
        ])));
        let gate = Arc::new(AtomicBool::new(true));
        let (tx, recovery) =
            tokio::sync::watch::channel((4, super::super::RecoveryNotice::Connected));
        let (events, _) = std::sync::mpsc::channel();
        let scope = Arc::new(Mutex::new(None));
        let mut observer = ObservedNotification {
            inner: opcua::client::DataChangeCallback::new(|_, _| {}),
            tracker: SequenceTracker::default(),
            generation: 1,
            connection: Some(RequestId::FIRST),
            session: crate::bff::model::SessionId::FIRST,
            events,
            gate: gate.clone(),
            keys: keys.clone(),
            recovery: recovery.clone(),
            scope: scope.clone(),
        };
        let captured = observer.begin(&opcua::types::NotificationMessage::default());
        *scope.lock().unwrap() = captured;
        assert_eq!(captured, Some(4));
        // Deterministically inject Lost+Reconnected+Ready between the raw
        // callback's observation and the SDK's delegated data callback.
        tx.send_modify(|(revision, _)| *revision = 6);
        gate.store(true, Ordering::SeqCst);
        let _keys = keys.lock().unwrap();
        assert_eq!(current_notification_scope(&scope, &gate, &recovery), None);
        *scope.lock().unwrap() = Some(6);
        assert_eq!(
            current_notification_scope(&scope, &gate, &recovery),
            Some(6)
        );
        gate.store(false, Ordering::SeqCst);
        assert_eq!(current_notification_scope(&scope, &gate, &recovery), None);
    }

    #[test]
    fn sequence_keepalive_wrap_gap_duplicate_and_revision() {
        let mut t = SequenceTracker::default();
        assert_eq!(t.observe(0, 1, false), None);
        assert_eq!(t.observe(0, 1, true), None);
        assert_eq!(t.observe(0, 2, false), None);
        assert_eq!(t.observe(0, 2, true), None);
        assert_eq!(t.observe(0, 2, true), None);
        assert_eq!(
            t.observe(0, 4, true),
            Some(UaNotificationEvidence::SequenceGap {
                previous: 2,
                current: 4
            })
        );
        assert_eq!(
            t.observe(0, 3, true),
            Some(UaNotificationEvidence::SequenceUncertain {
                previous: 4,
                current: 3
            })
        );
        assert_eq!(
            t.observe(0, 0, true),
            Some(UaNotificationEvidence::SequenceUncertain {
                previous: 4,
                current: 0
            })
        );
        assert_eq!(t.observe(1, u32::MAX, true), None);
        assert_eq!(t.observe(1, 1, true), None);
        assert_eq!(t.observe(2, 77, true), None);
    }
}

/// Decorates the public SDK callback and delegates payload decoding to SDK.
pub(super) struct ObservedNotification {
    pub inner: opcua::client::DataChangeCallback,
    pub tracker: SequenceTracker,
    pub generation: u64,
    pub connection: Option<RequestId>,
    pub session: crate::bff::model::SessionId,
    pub events: std::sync::mpsc::Sender<crate::bff::model::Event>,
    pub gate: std::sync::Arc<std::sync::atomic::AtomicBool>,
    pub keys:
        std::sync::Arc<std::sync::Mutex<std::collections::HashSet<crate::bff::model::ItemKey>>>,
    pub recovery: tokio::sync::watch::Receiver<(u64, super::RecoveryNotice)>,
    pub scope: std::sync::Arc<std::sync::Mutex<Option<u64>>>,
}

impl ObservedNotification {
    fn begin(&mut self, notification: &opcua::types::NotificationMessage) -> Option<u64> {
        let keys = self
            .keys
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        if !self.gate.load(std::sync::atomic::Ordering::SeqCst) || keys.is_empty() {
            self.tracker = SequenceTracker::default();
            return None;
        }
        let revision = self.recovery.borrow().0;
        let has_notifications = notification
            .notification_data
            .as_ref()
            .is_some_and(|items| !items.is_empty());
        if let Some(evidence) =
            self.tracker
                .observe(revision, notification.sequence_number, has_notifications)
        {
            let _ = self.events.send(crate::bff::model::Event {
                owner: self.session.into(),
                origin: scoped_origin(self.connection, revision),
                kind: crate::bff::model::EventKind::UaNotificationFacts {
                    subscription_generation: self.generation,
                    evidence,
                },
            });
        }
        Some(revision)
    }
}

/// Caller holds the push-keys mutex, matching the Lost event's lock ordering.
pub(super) fn current_notification_scope(
    scope: &std::sync::Mutex<Option<u64>>,
    gate: &std::sync::atomic::AtomicBool,
    recovery: &tokio::sync::watch::Receiver<(u64, super::RecoveryNotice)>,
) -> Option<u64> {
    let captured = *scope
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner);
    captured.filter(|revision| {
        gate.load(std::sync::atomic::Ordering::SeqCst) && recovery.borrow().0 == *revision
    })
}

impl opcua::client::OnSubscriptionNotificationCore for ObservedNotification {
    fn on_subscription_notification(
        &mut self,
        notification: opcua::types::NotificationMessage,
        items: opcua::client::MonitoredItemMap<'_>,
    ) {
        let captured = self.begin(&notification);
        *self
            .scope
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner) = captured;
        // Reuse the SDK's implementation for item mapping and value callbacks.
        opcua::client::OnSubscriptionNotificationCore::on_subscription_notification(
            &mut self.inner,
            notification,
            items,
        );
        *self
            .scope
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner) = None;
    }
}
