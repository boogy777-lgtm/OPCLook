use serde::{Deserialize, Deserializer, Serialize, Serializer};
use std::fmt;

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub enum Protocol {
    #[default]
    OpcDa,
    OpcUa,
}

impl fmt::Display for Protocol {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::OpcDa => f.write_str("OPC DA"),
            Self::OpcUa => f.write_str("OPC UA"),
        }
    }
}

/// OPC UA security-policy selection carried at every first-party boundary
/// (dialog, settings, `EndpointSpec`, recent endpoints). `Auto` is the only
/// non-URI member: it means "resolve against the advertised endpoints".
///
/// Legacy settings used free strings ("Авто", "Auto", "None", short names or
/// full `http://opcfoundation.org/UA/SecurityPolicy#...` URIs). Deserialization
/// accepts those spellings; an unknown value is a serde error, never a silent
/// fallback to `Auto`.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub enum UaSecurityPolicy {
    #[default]
    Auto,
    None,
    Basic128Rsa15,
    Basic256,
    Basic256Sha256,
    Aes128Sha256RsaOaep,
    Aes256Sha256RsaPss,
}

impl UaSecurityPolicy {
    /// Every variant, in UI order (guard tests walk this list).
    pub const ALL: [UaSecurityPolicy; 7] = [
        UaSecurityPolicy::Auto,
        UaSecurityPolicy::None,
        UaSecurityPolicy::Basic128Rsa15,
        UaSecurityPolicy::Basic256,
        UaSecurityPolicy::Basic256Sha256,
        UaSecurityPolicy::Aes128Sha256RsaOaep,
        UaSecurityPolicy::Aes256Sha256RsaPss,
    ];

    /// Stable, ASCII token for logs/persistence.
    pub fn as_str(self) -> &'static str {
        match self {
            UaSecurityPolicy::Auto => "Auto",
            UaSecurityPolicy::None => "None",
            UaSecurityPolicy::Basic128Rsa15 => "Basic128Rsa15",
            UaSecurityPolicy::Basic256 => "Basic256",
            UaSecurityPolicy::Basic256Sha256 => "Basic256Sha256",
            UaSecurityPolicy::Aes128Sha256RsaOaep => "Aes128_Sha256_RsaOaep",
            UaSecurityPolicy::Aes256Sha256RsaPss => "Aes256_Sha256_RsaPss",
        }
    }

    /// Full policy URI for a concrete policy. `Auto` has no URI (it must be
    /// resolved against the server's advertised endpoints first).
    pub fn as_uri(self) -> Option<&'static str> {
        match self {
            UaSecurityPolicy::Auto => None,
            UaSecurityPolicy::None => Some("http://opcfoundation.org/UA/SecurityPolicy#None"),
            UaSecurityPolicy::Basic128Rsa15 => {
                Some("http://opcfoundation.org/UA/SecurityPolicy#Basic128Rsa15")
            }
            UaSecurityPolicy::Basic256 => {
                Some("http://opcfoundation.org/UA/SecurityPolicy#Basic256")
            }
            UaSecurityPolicy::Basic256Sha256 => {
                Some("http://opcfoundation.org/UA/SecurityPolicy#Basic256Sha256")
            }
            UaSecurityPolicy::Aes128Sha256RsaOaep => {
                Some("http://opcfoundation.org/UA/SecurityPolicy#Aes128_Sha256_RsaOaep")
            }
            UaSecurityPolicy::Aes256Sha256RsaPss => {
                Some("http://opcfoundation.org/UA/SecurityPolicy#Aes256_Sha256_RsaPss")
            }
        }
    }

    /// Parse a server-advertised policy (short name or full URI). An empty
    /// string is the legacy "no policy advertised" placeholder and maps to
    /// `None`. Unknown policies return `None` so the caller can report them
    /// explicitly instead of forcing a fallback.
    pub fn from_advertised(text: &str) -> Option<Self> {
        let text = text.trim();
        if text.is_empty() {
            return Some(UaSecurityPolicy::None);
        }
        Self::from_token(text.rsplit('#').next().unwrap_or(text))
    }

    /// Parse a stored/legacy selection token. An empty string means "unset"
    /// and maps to `Auto` (the old dialogs always wrote an explicit token).
    pub fn from_stored(text: &str) -> Option<Self> {
        let text = text.trim();
        if text.is_empty() {
            return Some(UaSecurityPolicy::Auto);
        }
        Self::from_token(text.rsplit('#').next().unwrap_or(text))
    }

    fn from_token(token: &str) -> Option<Self> {
        // `to_ascii_lowercase` leaves Cyrillic untouched; accept both legacy
        // capitalizations explicitly.
        if token == "Авто" || token == "авто" {
            return Some(UaSecurityPolicy::Auto);
        }
        // The OPC UA URI fragment spells the Aes policies with underscores
        // (`Aes256_Sha256_RsaPss`) while the library calls them with hyphens;
        // separators carry no meaning, so ignore them.
        let compact: String = token
            .chars()
            .filter(|c| *c != '_' && *c != '-')
            .collect::<String>()
            .to_ascii_lowercase();
        match compact.as_str() {
            "auto" => Some(UaSecurityPolicy::Auto),
            "none" => Some(UaSecurityPolicy::None),
            "basic128rsa15" => Some(UaSecurityPolicy::Basic128Rsa15),
            "basic256" => Some(UaSecurityPolicy::Basic256),
            "basic256sha256" => Some(UaSecurityPolicy::Basic256Sha256),
            "aes128sha256rsaoaep" => Some(UaSecurityPolicy::Aes128Sha256RsaOaep),
            "aes256sha256rsapss" => Some(UaSecurityPolicy::Aes256Sha256RsaPss),
            _ => None,
        }
    }
}

impl fmt::Display for UaSecurityPolicy {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

impl Serialize for UaSecurityPolicy {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        serializer.serialize_str(self.as_str())
    }
}

impl<'de> Deserialize<'de> for UaSecurityPolicy {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        let raw = String::deserialize(deserializer)?;
        Self::from_stored(&raw).ok_or_else(|| {
            serde::de::Error::custom(format!(
                "unknown OPC UA security policy {raw:?} (expected Auto, None, Basic128Rsa15, Basic256, Basic256Sha256, Aes128_Sha256_RsaOaep, Aes256_Sha256_RsaPss or a full SecurityPolicy URI)"
            ))
        })
    }
}

/// OPC UA message-security-mode selection. `Auto` is the only non-wire member;
/// the rest map 1:1 to `MessageSecurityMode`.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub enum UaSecurityMode {
    #[default]
    Auto,
    None,
    Sign,
    SignAndEncrypt,
}

impl UaSecurityMode {
    pub const ALL: [UaSecurityMode; 4] = [
        UaSecurityMode::Auto,
        UaSecurityMode::None,
        UaSecurityMode::Sign,
        UaSecurityMode::SignAndEncrypt,
    ];

    pub fn as_str(self) -> &'static str {
        match self {
            UaSecurityMode::Auto => "Auto",
            UaSecurityMode::None => "None",
            UaSecurityMode::Sign => "Sign",
            UaSecurityMode::SignAndEncrypt => "SignAndEncrypt",
        }
    }

    /// Parse a server-advertised mode (`{:?}` form). An empty string is the
    /// legacy "not advertised" placeholder and maps to `None`.
    pub fn from_advertised(text: &str) -> Option<Self> {
        let text = text.trim();
        if text.is_empty() {
            return Some(UaSecurityMode::None);
        }
        Self::from_token(text)
    }

    /// Parse a stored/legacy selection token. An empty string means "unset"
    /// and maps to `Auto`.
    pub fn from_stored(text: &str) -> Option<Self> {
        let text = text.trim();
        if text.is_empty() {
            return Some(UaSecurityMode::Auto);
        }
        Self::from_token(text)
    }

    fn from_token(token: &str) -> Option<Self> {
        if token == "Авто" || token == "авто" {
            return Some(UaSecurityMode::Auto);
        }
        match token.to_ascii_lowercase().as_str() {
            "auto" => Some(UaSecurityMode::Auto),
            "none" => Some(UaSecurityMode::None),
            "sign" => Some(UaSecurityMode::Sign),
            "signandencrypt" => Some(UaSecurityMode::SignAndEncrypt),
            _ => None,
        }
    }
}

impl fmt::Display for UaSecurityMode {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

impl Serialize for UaSecurityMode {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        serializer.serialize_str(self.as_str())
    }
}

impl<'de> Deserialize<'de> for UaSecurityMode {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        let raw = String::deserialize(deserializer)?;
        Self::from_stored(&raw).ok_or_else(|| {
            serde::de::Error::custom(format!(
                "unknown OPC UA security mode {raw:?} (expected Auto, None, Sign, SignAndEncrypt)"
            ))
        })
    }
}

/// Why a policy/mode pair cannot be requested.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SecurityPairError {
    /// `None` policy means "no security"; a signing/encrypting mode contradicts it.
    NonePolicyRequiresNoneMode { mode: UaSecurityMode },
}

impl fmt::Display for SecurityPairError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            SecurityPairError::NonePolicyRequiresNoneMode { mode } => write!(
                f,
                "security policy None cannot be combined with mode {mode} (None policy requires mode None)"
            ),
        }
    }
}

/// Validate the policy/mode combination independently of any server.
///
/// Sign/SignAndEncrypt require a non-`None` policy; a `None` policy requires
/// the `None` mode. `Auto` on either axis is always allowed.
pub fn validate_security_pair(
    policy: UaSecurityPolicy,
    mode: UaSecurityMode,
) -> Result<(), SecurityPairError> {
    match (policy, mode) {
        (UaSecurityPolicy::None, UaSecurityMode::Sign | UaSecurityMode::SignAndEncrypt) => {
            Err(SecurityPairError::NonePolicyRequiresNoneMode { mode })
        }
        (UaSecurityPolicy::None, UaSecurityMode::None | UaSecurityMode::Auto) => Ok(()),
        (
            UaSecurityPolicy::Auto
            | UaSecurityPolicy::Basic128Rsa15
            | UaSecurityPolicy::Basic256
            | UaSecurityPolicy::Basic256Sha256
            | UaSecurityPolicy::Aes128Sha256RsaOaep
            | UaSecurityPolicy::Aes256Sha256RsaPss,
            _,
        ) => Ok(()),
    }
}
