use crate::bff::model::{
    AdvertisedMode, BackendMsg, UaEndpointInfo, UaProtection, UaSecurityMode, UaSecurityPolicy,
    validate_security_pair,
};
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

/// A security-resolution failure for transport through `EventKind::Error`: the
/// application wraps the (English, technical) detail in its localized sentence.
pub fn security_error(detail: impl Into<String>) -> BackendMsg {
    BackendMsg::SecurityResolveFailed {
        detail: detail.into(),
    }
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
            policy: endpoint.security_policy.known()?,
            mode: endpoint.security_mode.known()?,
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

/// Translate the SDK enum directly; invalid wire modes remain unknown.
pub(super) fn advertised_mode(mode: MessageSecurityMode) -> AdvertisedMode {
    match mode {
        MessageSecurityMode::None => AdvertisedMode::Known(UaSecurityMode::None),
        MessageSecurityMode::Sign => AdvertisedMode::Known(UaSecurityMode::Sign),
        MessageSecurityMode::SignAndEncrypt => {
            AdvertisedMode::Known(UaSecurityMode::SignAndEncrypt)
        }
        MessageSecurityMode::Invalid => AdvertisedMode::Unknown("Invalid".to_string()),
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
            let policy = &e.security_policy;
            let mode = &e.security_mode;
            format!("{policy}/{mode}")
        })
        .collect::<Vec<_>>()
        .join(", ")
}

/// Resolve the wire security policy URI and mode to request.
///
/// **Decision on `Auto` (U12):** this is a test/diagnostic client. `Auto` keeps
/// the historical preference order: None policy first (it connects to every
/// server without certificate exchange, which is what an engineer checking a
/// controller wants), then Basic256Sha256/Sign, then
/// Basic256Sha256/SignAndEncrypt, then the first recognized endpoint. Because
/// `Auto` can therefore pick an unencrypted channel while the server offers a
/// secure one, the session info always states the real protection
/// ([`security_summary`]) and says so when a secure endpoint was available. Anyone
/// who needs protection picks the policy and mode explicitly, and an explicit
/// choice is never downgraded.
///
/// An explicit choice aside, the order is: None, then Basic256Sha256/Sign, then
/// Basic256Sha256/SignAndEncrypt, then the first recognized endpoint. An explicit policy/mode is honored only when the server
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

/// True when `Auto`/`Auto` was requested, the connection ended up without any
/// protection and the server also offers a secure endpoint the user could have
/// asked for.
pub fn auto_chose_unprotected_over_secure(
    requested_policy: UaSecurityPolicy,
    requested_mode: UaSecurityMode,
    resolved_mode: MessageSecurityMode,
    endpoints: &[UaEndpointInfo],
) -> bool {
    if requested_policy != UaSecurityPolicy::Auto
        || requested_mode != UaSecurityMode::Auto
        || resolved_mode != MessageSecurityMode::None
    {
        return false;
    }
    endpoints
        .iter()
        .filter_map(AdvertisedEndpoint::parse)
        .any(|e| {
            e.policy != UaSecurityPolicy::None
                && matches!(
                    e.mode,
                    UaSecurityMode::Sign | UaSecurityMode::SignAndEncrypt
                )
                && validate_security_pair(e.policy, e.mode).is_ok()
        })
}

/// The session info line shown to the user and written to the journal: where it
/// is connected and, in plain words, how the channel is protected.
pub fn security_summary(
    url: &str,
    policy: &str,
    mode: MessageSecurityMode,
    secure_was_available: bool,
) -> BackendMsg {
    let protection = match mode {
        MessageSecurityMode::None => UaProtection::None,
        MessageSecurityMode::Sign => UaProtection::Sign,
        MessageSecurityMode::SignAndEncrypt => UaProtection::SignAndEncrypt,
        MessageSecurityMode::Invalid => UaProtection::Unknown,
    };
    BackendMsg::UaConnected {
        url: url.to_string(),
        policy: policy.to_string(),
        mode: format!("{mode:?}"),
        protection,
        secure_available: secure_was_available,
    }
}
