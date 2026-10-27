//! Server status wording of the OPC DA backend (D6 test verdict, D7 info
//! line): platform-neutral.
#![cfg_attr(not(windows), allow(dead_code))]

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
pub fn check_server_state(state: i32) -> Result<String, String> {
    match server_state_name(state) {
        Some("RUNNING") => Ok("связь с сервером в порядке (состояние RUNNING)".to_string()),
        Some(name) => Err(format!(
            "сервер не готов к работе: состояние {name} ({state})"
        )),
        None => Err(format!("сервер вернул неизвестное состояние ({state})")),
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

/// "подключено, группа #1, период 1000 мс, версия сервера 5.2.1, групп на
/// сервере: 3, Vendor". `wMajorVersion.wMinorVersion.wBuildNumber` is the
/// version; `dwGroupCount` is a separate, labelled number.
pub fn format_connect_info(
    group_handle: u32,
    update_rate_ms: u32,
    status: Option<&ServerStatusInfo<'_>>,
) -> String {
    let mut info = format!("подключено, группа #{group_handle}, период {update_rate_ms} мс");
    if let Some(s) = status {
        info.push_str(&format!(
            ", версия сервера {}.{}.{}, групп на сервере: {}",
            s.major, s.minor, s.build, s.group_count
        ));
        let vendor = s.vendor.trim();
        if !vendor.is_empty() {
            info.push_str(", ");
            info.push_str(vendor);
        }
    }
    info
}

#[cfg(test)]
mod tests {
    use super::*;

    // --- D6 ---------------------------------------------------------------

    #[test]
    fn only_running_is_a_successful_connection_test() {
        // Before D6 every state was reported as "ok".
        assert!(check_server_state(STATE_RUNNING).is_ok());
        for (state, name) in [
            (STATE_FAILED, "FAILED"),
            (STATE_NOCONFIG, "NOCONFIG"),
            (STATE_SUSPENDED, "SUSPENDED"),
            (STATE_TEST, "TEST"),
            (STATE_COMM_FAULT, "COMM_FAULT"),
        ] {
            let err = check_server_state(state).unwrap_err();
            assert!(err.contains(name), "{err}");
            assert_eq!(server_state_name(state), Some(name));
        }
        let err = check_server_state(42).unwrap_err();
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
        let info = format_connect_info(4, 1000, Some(&status));
        assert!(info.contains("версия сервера 5.2.1,"), "{info}");
        assert!(info.contains("групп на сервере: 3"), "{info}");
        assert!(!info.contains("5.2.1.3"), "{info}");
        assert!(info.ends_with(", Acme"), "{info}");
        assert_eq!(
            format_connect_info(4, 1000, None),
            "подключено, группа #4, период 1000 мс"
        );
        let no_vendor = ServerStatusInfo {
            vendor: "",
            ..status
        };
        assert!(format_connect_info(1, 500, Some(&no_vendor)).ends_with("групп на сервере: 3"));
    }
}
