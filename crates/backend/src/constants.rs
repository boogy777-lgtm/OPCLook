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
/// V3: budget for loading a server's structure types after connect (a browse
/// of the type hierarchy plus one read per 1000 attributes).
pub const STRUCT_TYPES_TIMEOUT: std::time::Duration = std::time::Duration::from_secs(30);
/// Budget for a graceful `Session::disconnect()` and its event loop to finish.
pub const GRACEFUL_CLOSE_TIMEOUT: std::time::Duration = std::time::Duration::from_secs(5);
/// Hard upper bound for one one-shot discovery call (`FindServers`/`GetEndpoints`/
/// `FindServersOnNetwork`). The discovery client's own request deadline is
/// `UA_SEARCH_TIMEOUT`; this larger guard only fires if even the library cannot
/// finish, so the library normally closes the temporary channel itself before
/// the future is dropped.
pub const DISCOVERY_GUARD_TIMEOUT: std::time::Duration = std::time::Duration::from_secs(20);
/// Standard LDS discovery endpoints queried by `FindServersOnNetwork`. Both are
/// spellings of this machine's own LDS: the numeric one goes first (no name
/// resolution, no IPv6 detour) and `localhost` is only tried when it is unreachable.
pub const DISCOVERY_URLS: [&str; 2] = ["opc.tcp://127.0.0.1:4840", "opc.tcp://localhost:4840"];

// ---------------------------------------------------------------------------
// UA sessions / network discovery
// ---------------------------------------------------------------------------

/// `samplingInterval` of every monitored item. `-1` means "use the subscription's
/// publishing interval" (OPC UA Part 4, 5.12.2): the sampling rate then follows
/// the interval the user picks (and `ConfigureUaSubscription` changes) and no
/// server is asked to sample at "its fastest rate" (`0`), which a PLC-class
/// server may take as a request to sample every cycle.
pub const UA_SAMPLING_INTERVAL_MS: f64 = -1.0;
/// Subscription lifetime / keep-alive counts (lifetime must be >= 3 * keep-alive).
pub const UA_SUBSCRIPTION_LIFETIME_COUNT: u32 = 60;
pub const UA_SUBSCRIPTION_KEEPALIVE_COUNT: u32 = 20;
/// Worker threads of the OPC UA runtime. Sessions, searches and connects are
/// separate tasks; a couple of threads keep the CPU-bound part of a secure
/// handshake (RSA) of one session from stalling the I/O of the others.
pub const UA_HOST_RUNTIME_THREADS: usize = 2;
/// Commands the host keeps for a session whose `Connect` is still running; more
/// are answered "not connected" at once so a stuck connect cannot grow memory.
pub const UA_SESSION_BACKLOG_LIMIT: usize = 256;
/// Maximum number of address checks the subnet scanner runs at once.
pub const SCAN_PARALLELISM: usize = 32;
/// A subnet larger than this many addresses requires explicit confirmation.
pub use opclook_contract::net::SCAN_CONFIRM_THRESHOLD;
/// Collection window for mDNS discovery results.
pub const MDNS_WINDOW: std::time::Duration = std::time::Duration::from_secs(5);
/// TCP connect budget for one scanner address.
pub const SCAN_TCP_CONNECT_TIMEOUT: std::time::Duration = std::time::Duration::from_millis(1500);
/// Budget for the UA handshake/`GetEndpoints` step of one scanner address.
pub const SCAN_UA_HANDSHAKE_TIMEOUT: std::time::Duration = std::time::Duration::from_secs(4);
/// Overall budget for one scanner address (connect + handshake).
pub const SCAN_ADDRESS_TIMEOUT: std::time::Duration = std::time::Duration::from_secs(6);

// Backend browse limit.
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
