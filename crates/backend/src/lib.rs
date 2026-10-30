//! Protocol workers and platform networking.
pub mod bff;
pub mod constants;
pub mod net;
pub mod opc;
#[cfg(test)]
pub use opclook_locale as i18n;

pub mod da_route;
pub mod route_resolver;

pub mod route_service;
#[cfg(windows)]
mod winutil;

/// Current adapter read capability, independent of negotiated server features.
/// UA read uses maxAge=0; the legacy DA adapter only reads OPC_DS_CACHE.
pub fn supports_fresh_source(protocol: opclook_contract::model::Protocol) -> bool {
    matches!(protocol, opclook_contract::model::Protocol::OpcUa)
}
