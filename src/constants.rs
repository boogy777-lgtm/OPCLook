//! Centralized named constants for first-party code.
//!
//! Every timeout, size, range, default and truncation length used by the app
//! lives here so decisions are made in exactly one place and can be referenced
//! by the UI, the settings layer, the BFF workers and the tests. External
//! library defaults (e.g. the OPC UA stack's own retry budgets) are out of
//! scope; only first-party literals are consolidated.

// ---------------------------------------------------------------------------
// OPC UA endpoint defaults
// ---------------------------------------------------------------------------

/// Default host for local OPC UA discovery.
pub const DEFAULT_UA_HOST: &str = "127.0.0.1";
/// Default OPC UA TCP port.
pub const DEFAULT_UA_PORT: u16 = 4840;
/// How long a single server-search step gets to respond.
pub const UA_SEARCH_TIMEOUT: std::time::Duration = std::time::Duration::from_secs(10);
/// Upper bound for a `connect()` call (channel + session setup).
pub const CONNECT_TIMEOUT: std::time::Duration = std::time::Duration::from_secs(20);
/// Upper bound for one blocking IO op (browse/read/write).
pub const IO_TIMEOUT: std::time::Duration = std::time::Duration::from_secs(15);
/// Budget for a graceful `Session::disconnect()` and its event loop to finish.
pub const GRACEFUL_CLOSE_TIMEOUT: std::time::Duration = std::time::Duration::from_secs(5);
/// Hard upper bound for one one-shot discovery call (`FindServers`/`GetEndpoints`/
/// `FindServersOnNetwork`). The discovery client's own request deadline is
/// `UA_SEARCH_TIMEOUT`; this larger guard only fires if even the library cannot
/// finish, so the library normally closes the temporary channel itself before
/// the future is dropped.
pub const DISCOVERY_GUARD_TIMEOUT: std::time::Duration = std::time::Duration::from_secs(20);
/// Standard mDNS/LDS discovery endpoints queried by `FindServersOnNetwork`.
pub const DISCOVERY_URLS: [&str; 2] = ["opc.tcp://localhost:4840", "opc.tcp://127.0.0.1:4840"];

// ---------------------------------------------------------------------------
// UA sessions / network discovery
// ---------------------------------------------------------------------------

/// Hard cap on concurrently open OPC UA sessions (DA sessions are not counted).
pub const MAX_UA_SESSIONS: usize = 100;
/// Maximum number of address checks the subnet scanner runs at once.
pub const SCAN_PARALLELISM: usize = 32;
/// A subnet larger than this many addresses requires explicit confirmation.
pub const SCAN_CONFIRM_THRESHOLD: u64 = 4096;
/// Collection window for mDNS discovery results.
pub const MDNS_WINDOW: std::time::Duration = std::time::Duration::from_secs(5);
/// TCP connect budget for one scanner address.
pub const SCAN_TCP_CONNECT_TIMEOUT: std::time::Duration = std::time::Duration::from_millis(1500);
/// Budget for the UA handshake/`GetEndpoints` step of one scanner address.
pub const SCAN_UA_HANDSHAKE_TIMEOUT: std::time::Duration = std::time::Duration::from_secs(4);
/// Overall budget for one scanner address (connect + handshake).
pub const SCAN_ADDRESS_TIMEOUT: std::time::Duration = std::time::Duration::from_secs(6);

// ---------------------------------------------------------------------------
// DA route steering
// ---------------------------------------------------------------------------

/// Lifetime of a temporary host route created by `opcda-nethelper`. The helper
/// also deletes its routes when the pipe closes; the lifetime is only the
/// crash-safety net.
pub const DA_ROUTE_LIFETIME_SECS: u32 = 3600;
/// How often owned DA host routes are refreshed before their finite lifetime.
pub const DA_ROUTE_REFRESH_SECS: u64 = 1800;
/// How long the app waits for a freshly launched helper/UAC prompt before it
/// asks the user to retry. Never blocks on the UAC prompt itself.
pub const DA_HELPER_START_TIMEOUT: std::time::Duration = std::time::Duration::from_secs(2);
/// Protocol version of the app <-> helper named-pipe messages.
pub const DA_HELPER_PROTOCOL_VERSION: u32 = 1;
/// Environment variable used by tests/manual runs to point at a helper exe.
pub const DA_HELPER_EXE_ENV: &str = "OPCDA_NETHELPER_EXE";

// ---------------------------------------------------------------------------
// Reconnect / auto-reconnect
// ---------------------------------------------------------------------------

/// Maximum automatic reconnect attempts before the loop gives up.
pub const RECONNECT_MAX_ATTEMPTS: u32 = 20;
/// Base backoff (seconds) for the first reconnect attempt; doubles each time.
pub const RECONNECT_BASE_DELAY_SECS: u64 = 1;

// ---------------------------------------------------------------------------
// Option ranges and defaults (seconds unless noted)
// ---------------------------------------------------------------------------

/// Update-interval range (the original TrackBar 1..60 s).
pub const UPDATE_INTERVAL_MIN_SECS: u64 = 1;
pub const UPDATE_INTERVAL_MAX_SECS: u64 = 60;
/// Operation-timeout range (Spin 1..1800 s), default 60.
pub const OPERATION_TIMEOUT_MIN_SECS: u64 = 1;
pub const OPERATION_TIMEOUT_MAX_SECS: u64 = 1800;
pub const OPERATION_TIMEOUT_DEFAULT_SECS: u64 = 60;
/// Test-interval range (Spin 1..1800 s), default 60.
pub const TEST_INTERVAL_MIN_SECS: u64 = 1;
pub const TEST_INTERVAL_MAX_SECS: u64 = 1800;
pub const TEST_INTERVAL_DEFAULT_SECS: u64 = 60;
/// Default update interval is 1 second.
pub const UPDATE_INTERVAL_DEFAULT_SECS: u64 = 1;
/// UA session timeout: default 120 s, range 5000 ms..=3_600_000 ms.
pub const SESSION_TIMEOUT_DEFAULT_MS: u32 = 120_000;
pub const SESSION_TIMEOUT_MIN_MS: u32 = 5_000;
pub const SESSION_TIMEOUT_MAX_MS: u32 = 3_600_000;

// ---------------------------------------------------------------------------
// UI layout / sizing
// ---------------------------------------------------------------------------

/// Default width of the left «Серверы» panel.
pub const LEFT_PANEL_DEFAULT_WIDTH: f32 = 340.0;
/// Default height of the bottom panels panel.
pub const BOTTOM_PANEL_DEFAULT_HEIGHT: f32 = 230.0;
/// Lower bound for the bottom panels panel height.
pub const BOTTOM_PANEL_MIN_HEIGHT: f32 = 120.0;
/// Upper bound for the bottom panels panel height.
pub const BOTTOM_PANEL_MAX_HEIGHT: f32 = 420.0;
/// Height of the status bar at the bottom of the window.
pub const STATUS_BAR_HEIGHT: f32 = 24.0;
/// Height of one signal-table row.
pub const TABLE_ROW_HEIGHT: f32 = 22.0;
/// Height of the signal-table header row.
pub const TABLE_HEADER_HEIGHT: f32 = 20.0;
/// A table column can never shrink below this width.
pub const MIN_COLUMN_WIDTH: f32 = 20.0;

// ---------------------------------------------------------------------------
// Icon / button sizes (painted glyphs)
// ---------------------------------------------------------------------------

/// Toolbar reconnect icon.
pub const ICON_TOOLBAR: f32 = 22.0;
/// Reconnect/stop/resume icons inside a server row.
pub const ICON_SERVER_ROW: f32 = 20.0;
/// Destructive «×» button of a server row.
pub const ICON_SERVER_REMOVE: f32 = 24.0;
/// Server row state dot.
pub const DOT_SERVER_STATE: f32 = 18.0;
/// State-dot / expand-triangle in the left-panel and tree rows.
pub const DOT_LEAF: f32 = 18.0;

// ---------------------------------------------------------------------------
// Text truncation lengths (Charmed by display sites, kept in one place)
// ---------------------------------------------------------------------------

/// Truncated label length for a tab-strip tab (DA:/UA: <endpoint>).
pub const TRUNCATE_TAB_LABEL: usize = 30;
/// Truncated endpoint name length in the status bar.
pub const TRUNCATE_STATUS_NAME: usize = 36;
/// Truncated server name in a left-panel server row.
pub const TRUNCATE_SERVER_ROW: usize = 40;

// ---------------------------------------------------------------------------
// Tree / add-children caps
// ---------------------------------------------------------------------------

/// Upper bound on nodes visited by a single recursive add-children job.
pub const ADD_CHILDREN_CAP: usize = 5000;
/// Upper bound on nodes returned by a single UA browse response.
pub const MAX_BROWSE_NODES: usize = 5000;

// ---------------------------------------------------------------------------
// OPC UA request sizing (server chunk limits)
// ---------------------------------------------------------------------------

/// `requestedMaxReferencesPerNode` for every Browse call (references per page).
///
/// The CODESYS UA stack has `MaxChunkCount = 5`; asking for an unlimited page
/// makes a single Browse response exceed that budget and the server aborts with
/// `0x80800000` ("Too many chunks!"). Live scan of the CODESYS tree shows the
/// widest reachable branch (`…/Application/Programs/MAIN_PRG`) has 149 direct
/// references, so 100 forces the continuation-point loop to run on real data
/// while every page stays far below the chunk budget.
pub const BROWSE_MAX_REFERENCES_PER_NODE: u32 = 100;
/// Maximum number of `ReadValueId`s in one `session.read` call. Larger reads
/// are split into sequential chunks and concatenated in caller order. 100
/// values (with timestamps) fit in a single 64 KiB chunk on CODESYS, so the
/// response cannot hit the server's 5-chunk limit.
pub const READ_MAX_ITEMS_PER_CALL: usize = 100;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ua_defaults_are_present_and_ascii() {
        assert_eq!(DEFAULT_UA_HOST, "127.0.0.1");
        assert_eq!(DEFAULT_UA_PORT, 4840);
        for url in DISCOVERY_URLS {
            assert!(
                url.starts_with("opc.tcp://"),
                "unexpected discovery URL: {url}"
            );
        }
    }

    #[test]
    fn reconnect_cap_is_the_single_source_of_truth() {
        assert_eq!(RECONNECT_MAX_ATTEMPTS, 20);
        assert_eq!(RECONNECT_BASE_DELAY_SECS, 1);
    }

    #[test]
    fn option_ranges_match_the_elesy_spec() {
        assert_eq!(
            (UPDATE_INTERVAL_MIN_SECS, UPDATE_INTERVAL_MAX_SECS),
            (1, 60)
        );
        assert_eq!(
            (
                OPERATION_TIMEOUT_MIN_SECS,
                OPERATION_TIMEOUT_MAX_SECS,
                OPERATION_TIMEOUT_DEFAULT_SECS
            ),
            (1, 1800, 60),
        );
        assert_eq!(
            (
                TEST_INTERVAL_MIN_SECS,
                TEST_INTERVAL_MAX_SECS,
                TEST_INTERVAL_DEFAULT_SECS
            ),
            (1, 1800, 60),
        );
        assert_eq!(
            (
                SESSION_TIMEOUT_MIN_MS,
                SESSION_TIMEOUT_MAX_MS,
                SESSION_TIMEOUT_DEFAULT_MS
            ),
            (5_000, 3_600_000, 120_000),
        );
    }

    #[test]
    fn ui_layout_sizes_are_positive_and_ordered() {
        assert!(LEFT_PANEL_DEFAULT_WIDTH > 0.0);
        assert!(
            BOTTOM_PANEL_MIN_HEIGHT <= BOTTOM_PANEL_DEFAULT_HEIGHT
                && BOTTOM_PANEL_DEFAULT_HEIGHT <= BOTTOM_PANEL_MAX_HEIGHT
        );
        assert!(STATUS_BAR_HEIGHT > 0.0);
        assert!(TABLE_ROW_HEIGHT > 0.0 && TABLE_HEADER_HEIGHT > 0.0);
        assert!(MIN_COLUMN_WIDTH > 0.0);
    }

    #[test]
    fn browse_caps_are_aligned() {
        assert_eq!(ADD_CHILDREN_CAP, 5000);
        assert_eq!(MAX_BROWSE_NODES, 5000);
        assert_eq!(BROWSE_MAX_REFERENCES_PER_NODE, 100);
        assert_eq!(READ_MAX_ITEMS_PER_CALL, 100);
        assert!(DISCOVERY_GUARD_TIMEOUT > UA_SEARCH_TIMEOUT);
    }

    #[test]
    fn network_limits_are_the_single_source_of_truth() {
        assert_eq!(MAX_UA_SESSIONS, 100);
        assert_eq!(SCAN_PARALLELISM, 32);
        assert_eq!(SCAN_CONFIRM_THRESHOLD, 4096);
        assert_eq!(MDNS_WINDOW, std::time::Duration::from_secs(5));
        assert!(SCAN_PARALLELISM > 1);
        assert!(SCAN_CONFIRM_THRESHOLD > 0);
    }

    #[test]
    fn scanner_timeouts_are_ordered_and_positive() {
        assert!(!SCAN_TCP_CONNECT_TIMEOUT.is_zero());
        assert!(SCAN_TCP_CONNECT_TIMEOUT < SCAN_UA_HANDSHAKE_TIMEOUT);
        assert!(SCAN_UA_HANDSHAKE_TIMEOUT < SCAN_ADDRESS_TIMEOUT);
    }

    #[test]
    fn da_route_lifetime_is_finite_and_helper_wait_is_bounded() {
        assert!(DA_ROUTE_LIFETIME_SECS > 0);
        assert!(DA_ROUTE_REFRESH_SECS > 0);
        assert!(DA_ROUTE_REFRESH_SECS < u64::from(DA_ROUTE_LIFETIME_SECS));
        assert!(!DA_HELPER_START_TIMEOUT.is_zero());
        assert_eq!(DA_HELPER_PROTOCOL_VERSION, 1);
    }

    #[test]
    fn truncation_lengths_reflect_the_actual_display_sites() {
        // These are the live truncation lengths in src/ui.rs (30/36/40). The
        // family (26/60) from an earlier spec does not map to any literal that
        // exists in the current code, so it is deliberately not introduced.
        assert_eq!(TRUNCATE_TAB_LABEL, 30);
        assert_eq!(TRUNCATE_STATUS_NAME, 36);
        assert_eq!(TRUNCATE_SERVER_ROW, 40);
    }
}
