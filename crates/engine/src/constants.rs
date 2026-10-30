//! Session policy and settings limits.
/// Hard cap on concurrently open OPC UA sessions (DA sessions are not counted).
pub const MAX_UA_SESSIONS: usize = 100;
/// Bound the registration identity ledger for one connection epoch.
pub const MAX_ITEM_KEYS_PER_CONNECTION: usize =
    opclook_contract::model::MAX_ITEM_KEYS_PER_CONNECTION;
// ---------------------------------------------------------------------------
// Reconnect / auto-reconnect
// ---------------------------------------------------------------------------

/// Maximum automatic reconnect attempts before the loop gives up.
pub const RECONNECT_MAX_ATTEMPTS: u32 = 20;
/// Base backoff (seconds) for the first reconnect attempt; doubles each time.
pub const RECONNECT_BASE_DELAY: std::time::Duration = std::time::Duration::from_secs(1);
/// Ceiling of the reconnect backoff: an unreachable server is retried at least
/// once a minute instead of at 2^n seconds (U10).
pub const RECONNECT_MAX_DELAY: std::time::Duration = std::time::Duration::from_secs(60);
const _: () = assert!(RECONNECT_BASE_DELAY.as_secs() > 0);
const _: () = assert!(RECONNECT_BASE_DELAY.as_secs() <= RECONNECT_MAX_DELAY.as_secs());

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
/// K1: no request is declared unanswered sooner than this, whatever the
/// operation timeout setting says.
pub const REQUEST_MIN: std::time::Duration = std::time::Duration::from_secs(5);
/// K1: lower bound for slow operations (connect, discovery, server search).
pub const REQUEST_SLOW_MIN: std::time::Duration = std::time::Duration::from_secs(45);
const _: () = assert!(REQUEST_MIN.as_secs() > 0);
const _: () = assert!(REQUEST_MIN.as_secs() <= REQUEST_SLOW_MIN.as_secs());
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
