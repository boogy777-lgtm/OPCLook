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

pub const ACQUISITION_INTERVAL_MIN_MS: u64 = 1;
pub const ACQUISITION_INTERVAL_MAX_MS: u64 = 3_600_000;
fn deserialize_acquisition_interval<'de, D: serde::Deserializer<'de>>(
    deserializer: D,
) -> Result<u64, D::Error> {
    let value = u64::deserialize(deserializer)?;
    if !(ACQUISITION_INTERVAL_MIN_MS..=ACQUISITION_INTERVAL_MAX_MS).contains(&value) {
        return Err(serde::de::Error::custom(
            "acquisition interval out of range",
        ));
    }
    Ok(value)
}

fn deserialize_present_acquisition<'de, D: serde::Deserializer<'de>>(
    deserializer: D,
) -> Result<Option<AcquisitionConfig>, D::Error> {
    AcquisitionConfig::deserialize(deserializer).map(Some)
}

/// Requested product policy, captured at Connect; revised SDK parameters are separate.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum UpdateMode {
    Subscription,
    Poll,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PollFreshness {
    CacheAllowed,
    FreshSource,
}
/// DA read transport selection. The absent legacy field means synchronous.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum DaReadMethod {
    #[default]
    Sync,
    Async,
}
fn da_read_method_is_sync(method: &DaReadMethod) -> bool {
    *method == DaReadMethod::Sync
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct AcquisitionConfig {
    pub update_mode: UpdateMode,
    #[serde(deserialize_with = "deserialize_acquisition_interval")]
    pub interval_ms: u64,
    pub read_after_write: bool,
    pub poll_freshness: PollFreshness,
    #[serde(default, skip_serializing_if = "da_read_method_is_sync")]
    pub da_read_method: DaReadMethod,
}
impl AcquisitionConfig {
    pub fn ua_default() -> Self {
        Self {
            update_mode: UpdateMode::Subscription,
            interval_ms: 1000,
            read_after_write: true,
            poll_freshness: PollFreshness::FreshSource,
            da_read_method: DaReadMethod::Sync,
        }
    }
    pub fn da_available() -> Self {
        Self {
            update_mode: UpdateMode::Poll,
            interval_ms: 1000,
            read_after_write: false,
            poll_freshness: PollFreshness::CacheAllowed,
            da_read_method: DaReadMethod::Sync,
        }
    }
}

#[derive(Clone, Debug, Serialize, Deserialize, Default)]
pub struct EndpointSpec {
    pub protocol: Protocol,
    /// None preserves legacy behavior until explicitly migrated by the user.
    #[serde(
        default,
        deserialize_with = "deserialize_present_acquisition",
        skip_serializing_if = "Option::is_none"
    )]
    pub acquisition: Option<AcquisitionConfig>,
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
            acquisition: None,
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
            acquisition: None,
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

#[cfg(test)]
mod acquisition_tests {
    use super::*;
    #[test]
    fn da_read_method_defaults_sync_and_roundtrips_async() {
        let sync = AcquisitionConfig::da_available();
        let old = serde_json::to_value(&sync).unwrap();
        assert!(old.get("da_read_method").is_none());
        assert_eq!(
            serde_json::from_value::<AcquisitionConfig>(old.clone())
                .unwrap()
                .da_read_method,
            DaReadMethod::Sync
        );
        let mut asynchronous = old;
        asynchronous["da_read_method"] = serde_json::json!("async");
        assert_eq!(
            serde_json::from_value::<AcquisitionConfig>(asynchronous.clone())
                .unwrap()
                .da_read_method,
            DaReadMethod::Async
        );
        asynchronous["da_read_method"] = serde_json::json!("unknown");
        assert!(serde_json::from_value::<AcquisitionConfig>(asynchronous).is_err());
    }

    #[test]
    fn legacy_none_roundtrip_and_unknown_or_partial_profiles_fail() {
        // Use the actual serialized protocol spelling so this stays tied to the contract.
        let old = serde_json::to_value(EndpointSpec::ua("opc.tcp://localhost:4840")).unwrap();
        let parsed: EndpointSpec = serde_json::from_value(old.clone()).unwrap();
        assert!(parsed.acquisition.is_none());
        let mut full = old.clone();
        full["acquisition"] = serde_json::to_value(AcquisitionConfig::ua_default()).unwrap();
        let parsed: EndpointSpec = serde_json::from_value(full.clone()).unwrap();
        assert_eq!(parsed.acquisition, Some(AcquisitionConfig::ua_default()));
        full["acquisition"]["update_mode"] = serde_json::json!("unknown");
        assert!(serde_json::from_value::<EndpointSpec>(full).is_err());
        let mut partial = old;
        partial["acquisition"] = serde_json::json!({"update_mode":"poll", "interval_ms":1000, "poll_freshness":"fresh_source"});
        assert!(serde_json::from_value::<EndpointSpec>(partial).is_err());
        let mut null = serde_json::to_value(EndpointSpec::ua("opc.tcp://localhost:4840")).unwrap();
        null["acquisition"] = serde_json::Value::Null;
        assert!(serde_json::from_value::<EndpointSpec>(null).is_err());
        for interval in [0, 3_600_001] {
            let mut invalid =
                serde_json::to_value(EndpointSpec::ua("opc.tcp://localhost:4840")).unwrap();
            let mut profile = serde_json::to_value(AcquisitionConfig::ua_default()).unwrap();
            profile["interval_ms"] = serde_json::json!(interval);
            invalid["acquisition"] = profile;
            assert!(serde_json::from_value::<EndpointSpec>(invalid).is_err());
        }
    }
}
