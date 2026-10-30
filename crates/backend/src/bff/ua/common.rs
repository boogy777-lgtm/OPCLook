use crate::bff::model::{
    BackendMsg, Command, CommandKind, DataValue, Event, EventKind, Operation, Owner,
};
use crate::constants::GRACEFUL_CLOSE_TIMEOUT;
use std::future::Future;
use std::sync::mpsc::Sender;
use std::time::Duration;

// ---------------------------------------------------------------------------
// Shared helpers
// ---------------------------------------------------------------------------

pub(super) fn send(tx: &Sender<Event>, event: Event) {
    let _ = tx.send(event);
}

/// Diagnostics of a search or scan as journal lines. They are unsolicited
/// (without a request ID), so they never take the place of the request's own reply.
pub(super) fn send_notes(tx: &Sender<Event>, owner: Owner, notes: Vec<BackendMsg>) {
    for message in notes {
        send(
            tx,
            Event {
                origin: crate::bff::model::Origin::Unsolicited,
                owner,
                kind: EventKind::Log { message },
            },
        );
    }
}

pub(super) fn send_error(
    tx: &Sender<Event>,
    request_id: Option<crate::bff::model::RequestId>,
    owner: impl Into<Owner>,
    operation: Operation,
    message: BackendMsg,
) {
    send(
        tx,
        Event {
            origin: request_id.into(),
            owner: owner.into(),
            kind: EventKind::Error { operation, message },
        },
    );
}

/// Answers a command addressed to a session that is not (or no longer)
/// connected. Exactly one reply per command; `Disconnect` has nothing to close
/// and stays silent.
pub(super) fn reply_no_connection(tx: &Sender<Event>, command: Command) {
    let Command {
        request_id,
        owner,
        kind,
        ..
    } = command;
    let error = |operation: Operation| {
        send_error(tx, request_id, owner, operation, BackendMsg::NoConnection);
    };
    match kind {
        // These historical error operations differ from the dispatch labels.
        CommandKind::AddItems { .. } => error(Operation::Add),
        CommandKind::RemoveItems { .. } => error(Operation::Remove),
        CommandKind::Read { keys } | CommandKind::ReadCached { keys, .. } => send(
            tx,
            Event {
                origin: request_id.into(),
                owner,
                kind: EventKind::Values {
                    values: keys
                        .into_iter()
                        .map(|key| (key, DataValue::bad(BackendMsg::NoConnection)))
                        .collect(),
                },
            },
        ),
        CommandKind::Disconnect => {}
        other => error(other.operation()),
    }
}

/// Poll the same request during the grace period after its budget expires.
/// Even a late completion is a timeout: callers must close the session.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) struct RequestTimeout;

pub(super) async fn timeout_drain<T>(
    future: impl Future<Output = T>,
    budget: Duration,
) -> Result<T, RequestTimeout> {
    tokio::pin!(future);
    match tokio::time::timeout(budget, &mut future).await {
        Ok(value) => Ok(value),
        Err(_) => {
            let _ = tokio::time::timeout(GRACEFUL_CLOSE_TIMEOUT, &mut future).await;
            Err(RequestTimeout)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::bff::model::{ItemKey, Protocol, SessionId};
    use std::sync::mpsc::channel;

    #[test]
    fn no_connection_preserves_add_remove_and_read_reply_shapes() {
        let (tx, rx) = channel();
        let command = |kind| Command {
            request_id: None,
            owner: SessionId::FIRST.into(),
            protocol: Protocol::OpcUa,
            kind,
        };
        reply_no_connection(&tx, command(CommandKind::AddItems { items: vec![] }));
        assert!(matches!(
            rx.recv().unwrap().kind,
            EventKind::Error {
                operation: Operation::Add,
                message: BackendMsg::NoConnection
            }
        ));
        reply_no_connection(&tx, command(CommandKind::RemoveItems { keys: vec![] }));
        assert!(matches!(
            rx.recv().unwrap().kind,
            EventKind::Error {
                operation: Operation::Remove,
                message: BackendMsg::NoConnection
            }
        ));
        reply_no_connection(
            &tx,
            command(CommandKind::Read {
                keys: vec![ItemKey::FIRST],
            }),
        );
        assert!(
            matches!(rx.recv().unwrap().kind, EventKind::Values { values } if matches!(values[0].1.reason, Some(BackendMsg::NoConnection)))
        );
    }
}
