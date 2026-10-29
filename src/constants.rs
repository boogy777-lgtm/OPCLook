//! Presentation and route constants.
pub use opclook_engine::constants::*;
pub use opclook_engine::metadata::{
    BROWSE_MAX_REFERENCES_PER_NODE, DA_HELPER_PROTOCOL_VERSION, DA_HELPER_START_TIMEOUT,
    DA_ROUTE_LIFETIME, DA_ROUTE_LIFETIME_SECS, DA_ROUTE_REFRESH, DEFAULT_UA_HOST, DEFAULT_UA_PORT,
    DISCOVERY_GUARD_TIMEOUT, DISCOVERY_URLS, MAX_BROWSE_NODES, MDNS_WINDOW,
    READ_MAX_ITEMS_PER_CALL, SCAN_ADDRESS_TIMEOUT, SCAN_CONFIRM_THRESHOLD, SCAN_PARALLELISM,
    SCAN_TCP_CONNECT_TIMEOUT, SCAN_UA_HANDSHAKE_TIMEOUT, UA_SEARCH_TIMEOUT,
};

// ---------------------------------------------------------------------------
// UI layout / sizing
// ---------------------------------------------------------------------------

/// Default width of the left «Серверы» panel.
pub const LEFT_PANEL_DEFAULT_WIDTH: f32 = 340.0;
/// Default height of the bottom panels panel.
pub const BOTTOM_PANEL_DEFAULT_HEIGHT: f32 = 160.0;
/// Lower bound for the bottom panels panel height.
pub const BOTTOM_PANEL_MIN_HEIGHT: f32 = 120.0;
/// Upper bound for the bottom panels panel height.
pub const BOTTOM_PANEL_MAX_HEIGHT: f32 = 420.0;
/// Height of the status bar at the bottom of the window.
pub const STATUS_BAR_HEIGHT: f32 = 24.0;
/// UI8 type scale: `Small`.
pub const TEXT_SIZE_SMALL: f32 = 12.0;
/// UI8 type scale: `Body`, `Button` and `Monospace`.
pub const TEXT_SIZE_BODY: f32 = 13.0;
/// UI8 type scale: `Heading`.
pub const TEXT_SIZE_HEADING: f32 = 15.0;
/// UI8 spacing scale: gap between neighbouring widgets (x, y).
pub const ITEM_SPACING: (f32, f32) = (8.0, 6.0);
/// UI8 spacing scale: padding inside buttons (x, y).
pub const BUTTON_PADDING: (f32, f32) = (8.0, 4.0);
/// UI8 spacing scale: inner margin of windows and dialogs.
pub const WINDOW_MARGIN: i8 = 12;
/// Height of one signal-table row.
/// Vertical gap between signal table rows (UI8).
pub const TABLE_ROW_GAP: f32 = 2.0;
pub const TABLE_ROW_HEIGHT: f32 = 24.0;
/// Height of the signal-table header row.
pub const TABLE_HEADER_HEIGHT: f32 = 22.0;
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
        assert_eq!(RECONNECT_BASE_DELAY, std::time::Duration::from_secs(1));
        assert!(RECONNECT_MAX_DELAY >= RECONNECT_BASE_DELAY);
    }

    #[test]
    fn option_ranges_match_the_documented_spec() {
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
        assert!(!DA_ROUTE_REFRESH.is_zero());
        assert!(DA_ROUTE_REFRESH < DA_ROUTE_LIFETIME);
        assert!(!DA_HELPER_START_TIMEOUT.is_zero());
        assert_eq!(DA_HELPER_PROTOCOL_VERSION, 2);
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
