use crate::bff::model::{UaEndpointInfo, UaSecurityMode, UaSecurityPolicy, validate_security_pair};
use opcua::types::MessageSecurityMode;

/// Canonical typed selections. These aliases exist so tests and callers can name
/// the values without importing the model enum twice.
pub const SECURITY_POLICY_AUTO: UaSecurityPolicy = UaSecurityPolicy::Auto;
pub const SECURITY_POLICY_NONE: UaSecurityPolicy = UaSecurityPolicy::None;
pub const SECURITY_POLICY_BASIC256_SHA256: UaSecurityPolicy = UaSecurityPolicy::Basic256Sha256;

pub const SECURITY_MODE_AUTO: UaSecurityMode = UaSecurityMode::Auto;
pub const SECURITY_MODE_NONE: UaSecurityMode = UaSecurityMode::None;
pub const SECURITY_MODE_SIGN: UaSecurityMode = UaSecurityMode::Sign;
pub const SECURITY_MODE_SIGN_AND_ENCRYPT: UaSecurityMode = UaSecurityMode::SignAndEncrypt;

/// Marker prefix distinguishing a security-resolution failure from an arbitrary
/// connect error. The app strips it to render a localized wrapper around the
/// technical detail; the worker keeps the raw detail in its string result.
pub const SECURITY_ERROR_PREFIX: &str = "security: ";

/// Prefix a security-resolution detail for transport through `EventKind::Error`.
pub fn security_error(detail: impl Into<String>) -> String {
    format!("{SECURITY_ERROR_PREFIX}{}", detail.into())
}

/// Return the technical detail of a security error, or `None` for other errors.
pub fn security_error_detail(message: &str) -> Option<&str> {
    message.strip_prefix(SECURITY_ERROR_PREFIX)
}

/// One advertised endpoint normalized to the typed selections.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct AdvertisedEndpoint {
    policy: UaSecurityPolicy,
    mode: UaSecurityMode,
}

impl AdvertisedEndpoint {
    /// Normalize an advertised endpoint. Unknown policy/mode values return
    /// `None` so they are reported, not silently coerced.
    fn parse(endpoint: &UaEndpointInfo) -> Option<Self> {
        Some(Self {
            policy: UaSecurityPolicy::from_advertised(&endpoint.security_policy)?,
            mode: UaSecurityMode::from_advertised(&endpoint.security_mode)?,
        })
    }

    fn resolve(self) -> Result<(String, MessageSecurityMode), String> {
        let uri = self
            .policy
            .as_uri()
            .ok_or_else(|| format!("advertised security policy {} has no wire URI", self.policy))?;
        let mode = message_security_mode(self.mode)
            .ok_or_else(|| format!("advertised security mode {} is not a wire mode", self.mode))?;
        Ok((uri.to_string(), mode))
    }
}

/// Map a concrete selection to the library wire enum. `Auto` has no wire mode.
fn message_security_mode(mode: UaSecurityMode) -> Option<MessageSecurityMode> {
    match mode {
        UaSecurityMode::Auto => None,
        UaSecurityMode::None => Some(MessageSecurityMode::None),
        UaSecurityMode::Sign => Some(MessageSecurityMode::Sign),
        UaSecurityMode::SignAndEncrypt => Some(MessageSecurityMode::SignAndEncrypt),
    }
}

/// Human/technical summary of what the server offers, used in errors.
fn advertised_summary(endpoints: &[UaEndpointInfo]) -> String {
    if endpoints.is_empty() {
        return "<no endpoints>".to_string();
    }
    endpoints
        .iter()
        .map(|e| {
            let policy = UaSecurityPolicy::from_advertised(&e.security_policy)
                .map(|p| p.as_str().to_string())
                .unwrap_or_else(|| format!("{:?}", e.security_policy));
            let mode = UaSecurityMode::from_advertised(&e.security_mode)
                .map(|m| m.as_str().to_string())
                .unwrap_or_else(|| format!("{:?}", e.security_mode));
            format!("{policy}/{mode}")
        })
        .collect::<Vec<_>>()
        .join(", ")
}

/// Resolve the wire security policy URI and mode to request.
///
/// `Auto` keeps the historical preference order: None policy first, then
/// Basic256Sha256/Sign, then Basic256Sha256/SignAndEncrypt, then the first
/// recognized endpoint. An explicit policy/mode is honored only when the server
/// advertises that exact endpoint; otherwise this returns a clear error and the
/// caller must NOT connect with a guessed/fallback policy.
///
/// The detail string is technical (policy/mode tokens + the advertised list);
/// the app wraps it in a localized message.
pub fn resolve_security(
    policy: UaSecurityPolicy,
    mode: UaSecurityMode,
    endpoints: &[UaEndpointInfo],
) -> Result<(String, MessageSecurityMode), String> {
    if let Err(err) = validate_security_pair(policy, mode) {
        return Err(err.to_string());
    }
    if endpoints.is_empty() {
        return Err("the server advertised no security endpoints".to_string());
    }

    let advertised: Vec<AdvertisedEndpoint> = endpoints
        .iter()
        .filter_map(AdvertisedEndpoint::parse)
        .collect();

    let find = |policy: UaSecurityPolicy, mode: UaSecurityMode| {
        advertised.iter().copied().find(|e| {
            e.policy == policy && e.mode == mode && validate_security_pair(e.policy, e.mode).is_ok()
        })
    };

    let resolved = match (policy, mode) {
        (UaSecurityPolicy::Auto, UaSecurityMode::Auto) => {
            let preferred = find(UaSecurityPolicy::None, UaSecurityMode::None)
                .or_else(|| find(UaSecurityPolicy::Basic256Sha256, UaSecurityMode::Sign))
                .or_else(|| {
                    find(
                        UaSecurityPolicy::Basic256Sha256,
                        UaSecurityMode::SignAndEncrypt,
                    )
                })
                .or_else(|| {
                    advertised
                        .iter()
                        .copied()
                        .find(|e| validate_security_pair(e.policy, e.mode).is_ok())
                });
            preferred.ok_or_else(|| {
                format!(
                    "none of the advertised endpoints uses a recognized security policy/mode (advertised: {})",
                    advertised_summary(endpoints)
                )
            })?
        }
        (UaSecurityPolicy::Auto, explicit_mode) => advertised
            .iter()
            .copied()
            .find(|e| {
                e.mode == explicit_mode && validate_security_pair(e.policy, e.mode).is_ok()
            })
            .ok_or_else(|| {
                format!(
                    "security mode {explicit_mode} is not offered by the server (advertised: {})",
                    advertised_summary(endpoints)
                )
            })?,
        (explicit_policy, UaSecurityMode::Auto) => advertised
            .iter()
            .copied()
            .find(|e| {
                e.policy == explicit_policy
                    && validate_security_pair(e.policy, e.mode).is_ok()
            })
            .ok_or_else(|| {
                format!(
                    "security policy {explicit_policy} is not offered by the server (advertised: {})",
                    advertised_summary(endpoints)
                )
            })?,
        (explicit_policy, explicit_mode) => advertised
            .iter()
            .copied()
            .find(|e| {
                e.policy == explicit_policy
                    && e.mode == explicit_mode
                    && validate_security_pair(e.policy, e.mode).is_ok()
            })
            .ok_or_else(|| {
                format!(
                    "security policy {explicit_policy} / mode {explicit_mode} is not offered by the server (advertised: {})",
                    advertised_summary(endpoints)
                )
            })?,
    };

    resolved.resolve()
}
