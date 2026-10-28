//! Route descriptions shared without platform or localization dependencies.
use std::net::IpAddr;

/// A planned temporary host route.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct RoutePlan {
    pub target: IpAddr,
    pub prefix: u8,
    pub interface_luid: u64,
    pub next_hop: IpAddr,
    pub via_gateway: bool,
}

/// What to do with the current routing table state for one DA target.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum RouteDecision {
    /// The best route already goes through the selected adapter.
    Reuse,
    /// No host route exists; add one through the selected adapter.
    Add,
    /// A host route already goes through a different adapter. Never overwrite.
    Conflict,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum DaRouteError {
    RequestIdsExhausted,
    NoAdapter,
    AdapterUnavailable,
    Resolve(String),
    InvalidTarget(IpAddr),
    NoGateway(IpAddr),
    Conflict(IpAddr),
    HelperStarting,
    HelperUnavailable(String),
    Add(IpAddr, String),
    Verify(IpAddr),
    UacDeclined,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum DaRouteStatus {
    Idle,
    HelperStarting,
    HelperRunning,
    HelperUnavailable,
}
