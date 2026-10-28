use super::{Protocol, UaSecurityMode, UaSecurityPolicy, UaUrl};
use serde::{Deserialize, Serialize};
use std::fmt;

fn deserialize_auto<'de, D, T>(deserializer: D) -> Result<T, D::Error>
where
    D: serde::Deserializer<'de>,
    T: Deserialize<'de> + Default,
{
    Ok(Option::<T>::deserialize(deserializer)?.unwrap_or_default())
}

fn policy_is_auto(value: &UaSecurityPolicy) -> bool {
    *value == UaSecurityPolicy::Auto
}
fn mode_is_auto(value: &UaSecurityMode) -> bool {
    *value == UaSecurityMode::Auto
}

#[derive(Clone, Debug, Serialize, Deserialize, Default)]
pub struct EndpointSpec {
    pub protocol: Protocol,
    /// DA: computer name (empty = local). UA: unused (kept for display).
    pub host: String,
    /// DA: ProgID. UA: opc.tcp:// URL.
    pub address: String,
    /// UA user name (empty = anonymous).
    pub user: String,
    #[serde(skip)]
    pub password: String,
    /// UA security selections; absent or null legacy values mean Auto.
    #[serde(
        default,
        deserialize_with = "deserialize_auto",
        skip_serializing_if = "policy_is_auto"
    )]
    pub ua_security_policy: UaSecurityPolicy,
    #[serde(
        default,
        deserialize_with = "deserialize_auto",
        skip_serializing_if = "mode_is_auto"
    )]
    pub ua_security_mode: UaSecurityMode,
    /// UA: trust the server certificate without importing it into the PKI trust
    /// store (reuses `settings` trust flag on the connect dialog).
    #[serde(default)]
    pub ua_trust_server_cert: bool,
    /// UA: whether the identity was requested as X509 (client certificate).
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub ua_use_certificate: bool,
    /// UA: requested session timeout in milliseconds.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub ua_session_timeout: Option<u32>,
    /// UA: transport binding captured when the session was created. Reconnects
    /// reuse this snapshot instead of the current adapter map. Never persisted
    /// and always `None` for DA.
    #[serde(skip)]
    pub ua_bind: Option<crate::net::NetworkBinding>,
    /// The name advertised by the server itself, captured when the user selects
    /// a discovered server. UA: the application name (e.g. "Prosys OPC UA
    /// Simulation Server"); DA: the description + ProgID. `None` when a server
    /// was selected or typed manually and the advertised name was never seen,
    /// in which case the endpoint falls back to the address (`display_name()`).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub display_name: Option<String>,
}

impl EndpointSpec {
    pub fn da(host: impl Into<String>, prog_id: impl Into<String>) -> Self {
        Self {
            protocol: Protocol::OpcDa,
            host: host.into(),
            address: prog_id.into(),
            user: String::new(),
            password: String::new(),
            ua_security_policy: UaSecurityPolicy::Auto,
            ua_security_mode: UaSecurityMode::Auto,
            ua_trust_server_cert: false,
            ua_use_certificate: false,
            ua_session_timeout: None,
            ua_bind: None,
            display_name: None,
        }
    }

    pub fn ua(url: impl Into<String>) -> Self {
        Self {
            protocol: Protocol::OpcUa,
            host: String::new(),
            address: url.into(),
            user: String::new(),
            password: String::new(),
            ua_security_policy: UaSecurityPolicy::Auto,
            ua_security_mode: UaSecurityMode::Auto,
            ua_trust_server_cert: false,
            ua_use_certificate: false,
            ua_session_timeout: None,
            ua_bind: None,
            display_name: None,
        }
    }

    pub fn display_name(&self) -> String {
        match self.protocol {
            Protocol::OpcDa => {
                if self.host.is_empty() {
                    self.address.clone()
                } else {
                    format!("{} @ {}", self.address, self.host)
                }
            }
            Protocol::OpcUa => self.address.clone(),
        }
    }

    /// Record the name the server itself advertises (UA application name or the
    /// DA description + ProgID). Stored in `self.display_name` and written back
    /// into recent-endpoints persistence so the servers list/tab show it.
    pub fn with_advertised_name(mut self, name: Option<String>) -> Self {
        self.display_name = name.filter(|s| !s.trim().is_empty());
        self
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ServerEntry {
    pub id: String,
    pub description: String,
}

/// An advertised value can be absent, supported, or unknown to this client.
/// Unknown wire values remain visible and can never become an Auto/None choice.
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub enum Advertised<T> {
    Unspecified,
    Known(T),
    Unknown(String),
}

pub type AdvertisedPolicy = Advertised<UaSecurityPolicy>;
pub type AdvertisedMode = Advertised<UaSecurityMode>;

impl<T: Copy> Advertised<T> {
    pub fn known(&self) -> Option<T> {
        match self {
            Self::Known(value) => Some(*value),
            _ => None,
        }
    }
    pub fn is_unspecified(&self) -> bool {
        matches!(self, Self::Unspecified)
    }
}
impl AdvertisedPolicy {
    pub fn from_wire(text: &str) -> Self {
        let raw = text;
        let text = text.trim();
        if text.is_empty() {
            return Self::Unspecified;
        }
        // A policy URI is an identity, not just its fragment. Never turn an
        // unrecognized namespace ending in `#None` into an unprotected choice.
        if text.contains('#') && !text.starts_with("http://opcfoundation.org/UA/SecurityPolicy#") {
            return Self::Unknown(raw.to_owned());
        }
        match UaSecurityPolicy::from_advertised(text) {
            Some(policy)
                if policy != UaSecurityPolicy::Auto
                    && (!text.contains('#') || policy.as_uri() == Some(text)) =>
            {
                Self::Known(policy)
            }
            _ => Self::Unknown(raw.to_owned()),
        }
    }
}
impl AdvertisedMode {
    pub fn from_wire(text: &str) -> Self {
        let raw = text;
        let text = text.trim();
        if text.is_empty() {
            return Self::Unspecified;
        }
        match UaSecurityMode::from_advertised(text) {
            Some(mode) if mode != UaSecurityMode::Auto => Self::Known(mode),
            _ => Self::Unknown(raw.to_owned()),
        }
    }
}
impl<T: fmt::Display> fmt::Display for Advertised<T> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Unspecified => Ok(()),
            Self::Known(value) => value.fmt(f),
            Self::Unknown(value) => f.write_str(value),
        }
    }
}

/// A single OPC UA endpoint as reported by a discovery search.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct UaEndpointInfo {
    pub url: UaUrl,
    pub security_policy: AdvertisedPolicy,
    pub security_mode: AdvertisedMode,
}

/// A single OPC UA server found by the discovery search.
#[derive(Clone, Debug)]
pub struct UaServerInfo {
    pub application_name: String,
    pub application_uri: String,
    pub endpoints: Vec<UaEndpointInfo>,
}

/// A server reported by `FindServersOnNetwork` (mDNS/LDS network discovery),
/// carrying its discovery URLs instead of full endpoint descriptions.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct UaNetworkInfo {
    pub application_name: String,
    pub application_uri: String,
    pub discovery_urls: Vec<UaUrl>,
}
