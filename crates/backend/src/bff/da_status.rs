//! Server status wording of the OPC DA backend (D6 test verdict, D7 info
//! line): platform-neutral.
#![cfg_attr(not(windows), allow(dead_code))]

use super::model::{BackendMsg, DaServerStatus};

/// `OPC_STATUS_RUNNING` (OPCSERVERSTATE in the OPC DA 2.0 spec).
pub const STATE_RUNNING: i32 = 1;
pub const STATE_FAILED: i32 = 2;
pub const STATE_NOCONFIG: i32 = 3;
pub const STATE_SUSPENDED: i32 = 4;
pub const STATE_TEST: i32 = 5;
pub const STATE_COMM_FAULT: i32 = 6;

/// The OPC name of a server state, `None` for a value outside the spec.
pub fn server_state_name(state: i32) -> Option<&'static str> {
    match state {
        STATE_RUNNING => Some("RUNNING"),
        STATE_FAILED => Some("FAILED"),
        STATE_NOCONFIG => Some("NOCONFIG"),
        STATE_SUSPENDED => Some("SUSPENDED"),
        STATE_TEST => Some("TEST"),
        STATE_COMM_FAULT => Some("COMM_FAULT"),
        _ => None,
    }
}

/// Verdict of "test connection" for the state returned by `GetStatus`: only
/// `RUNNING` is a success. The error names the state.
pub fn check_server_state(state: i32) -> Result<BackendMsg, BackendMsg> {
    match server_state_name(state) {
        Some("RUNNING") => Ok(BackendMsg::DaStateOk),
        Some(name) => Err(BackendMsg::DaNotReady {
            name: name.to_string(),
            state,
        }),
        None => Err(BackendMsg::DaUnknownState { state }),
    }
}

// ---------------------------------------------------------------------------
// D7: connection info line
// ---------------------------------------------------------------------------

/// Data from `IOPCServer::GetStatus` shown in the connection info.
pub struct ServerStatusInfo<'a> {
    pub major: u16,
    pub minor: u16,
    pub build: u16,
    pub group_count: u32,
    pub vendor: &'a str,
}

/// The connection info line: group, update period and, when the server
/// reported it, its version, group count and vendor. `wMajorVersion.
/// wMinorVersion.wBuildNumber` is the version; `dwGroupCount` is a separate,
/// labelled number.
pub fn format_connect_info(
    group_handle: u32,
    update_rate_ms: u32,
    status: Option<&ServerStatusInfo<'_>>,
) -> BackendMsg {
    BackendMsg::DaConnected {
        group_handle,
        update_rate_ms,
        server: status.map(|s| DaServerStatus {
            major: s.major,
            minor: s.minor,
            build: s.build,
            group_count: s.group_count,
            vendor: s.vendor.to_string(),
        }),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::i18n::{Lang, backend_text};

    // --- D6 ---------------------------------------------------------------

    #[test]
    fn only_running_is_a_successful_connection_test() {
        // Before D6 every state was reported as "ok".
        assert_eq!(check_server_state(STATE_RUNNING), Ok(BackendMsg::DaStateOk));
        for (state, name) in [
            (STATE_FAILED, "FAILED"),
            (STATE_NOCONFIG, "NOCONFIG"),
            (STATE_SUSPENDED, "SUSPENDED"),
            (STATE_TEST, "TEST"),
            (STATE_COMM_FAULT, "COMM_FAULT"),
        ] {
            let err = backend_text(Lang::En, &check_server_state(state).unwrap_err());
            assert!(err.contains(name), "{err}");
            assert_eq!(server_state_name(state), Some(name));
        }
        let err = backend_text(Lang::En, &check_server_state(42).unwrap_err());
        assert!(err.contains("42"), "{err}");
        assert_eq!(server_state_name(0), None);
        assert_eq!(server_state_name(-1), None);
    }

    // --- D7 ---------------------------------------------------------------

    #[test]
    fn connect_info_separates_version_from_group_count() {
        // Before D7 "5.2.1.3" mixed the build number with dwGroupCount.
        let status = ServerStatusInfo {
            major: 5,
            minor: 2,
            build: 1,
            group_count: 3,
            vendor: " Acme ",
        };
        let info = backend_text(Lang::Ru, &format_connect_info(4, 1000, Some(&status)));
        assert!(info.contains("версия сервера 5.2.1,"), "{info}");
        assert!(info.contains("групп на сервере: 3"), "{info}");
        assert!(!info.contains("5.2.1.3"), "{info}");
        assert!(info.ends_with(", Acme"), "{info}");
        assert_eq!(
            backend_text(Lang::Ru, &format_connect_info(4, 1000, None)),
            "подключено, группа #4, период 1000 мс"
        );
        assert_eq!(
            backend_text(Lang::En, &format_connect_info(4, 1000, None)),
            "connected, group #4, period 1000 ms"
        );
        let no_vendor = ServerStatusInfo {
            vendor: "",
            ..status
        };
        let info = backend_text(Lang::Ru, &format_connect_info(1, 500, Some(&no_vendor)));
        assert!(info.ends_with("групп на сервере: 3"), "{info}");
    }
}
