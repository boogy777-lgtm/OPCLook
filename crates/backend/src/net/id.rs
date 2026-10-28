use super::NetError;
use serde::{Deserialize, Deserializer, Serialize, Serializer};
use std::fmt;

/// Adapter identity: a 16-byte GUID in canonical textual order.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct AdapterId([u8; 16]);

impl AdapterId {
    /// The all-zero GUID (used by the synthetic loopback adapter).
    pub const UNSPECIFIED: Self = Self([0u8; 16]);

    pub fn from_bytes(bytes: [u8; 16]) -> Self {
        Self(bytes)
    }

    pub fn as_bytes(&self) -> &[u8; 16] {
        &self.0
    }

    pub fn is_unspecified(&self) -> bool {
        self.0 == [0u8; 16]
    }

    /// Parse a GUID with or without braces and with either case.
    pub fn parse(text: &str) -> Result<Self, NetError> {
        let trimmed = text.trim();
        let body = trimmed
            .strip_prefix('{')
            .and_then(|s| s.strip_suffix('}'))
            .unwrap_or(trimmed);
        let mut parts = body.split('-');
        let mut bytes = [0u8; 16];
        let Some(first) = next_hex::<4>(parts.next()) else {
            return Err(NetError::InvalidAdapterId(text.to_string()));
        };
        bytes[0..4].copy_from_slice(&first);
        for range in [4usize..6usize, 6..8, 8..10] {
            let Some(part) = next_hex::<2>(parts.next()) else {
                return Err(NetError::InvalidAdapterId(text.to_string()));
            };
            bytes[range].copy_from_slice(&part);
        }
        let Some(last) = next_hex::<6>(parts.next()) else {
            return Err(NetError::InvalidAdapterId(text.to_string()));
        };
        bytes[10..16].copy_from_slice(&last);
        if parts.next().is_some() {
            return Err(NetError::InvalidAdapterId(text.to_string()));
        }
        Ok(Self(bytes))
    }
}

fn next_hex<const N: usize>(part: Option<&str>) -> Option<[u8; N]> {
    let part = part?;
    if part.len() != N * 2 {
        return None;
    }
    let mut out = [0; N];
    for (slot, pair) in out.iter_mut().zip(part.as_bytes().chunks_exact(2)) {
        let hi = char::from(*pair.first()?).to_digit(16)?;
        let lo = char::from(*pair.get(1)?).to_digit(16)?;
        *slot = ((hi << 4) | lo) as u8;
    }
    Some(out)
}

impl fmt::Display for AdapterId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let b = &self.0;
        write!(
            f,
            "{:02x}{:02x}{:02x}{:02x}-{:02x}{:02x}-{:02x}{:02x}-{:02x}{:02x}-{:02x}{:02x}{:02x}{:02x}{:02x}{:02x}",
            b[0],
            b[1],
            b[2],
            b[3],
            b[4],
            b[5],
            b[6],
            b[7],
            b[8],
            b[9],
            b[10],
            b[11],
            b[12],
            b[13],
            b[14],
            b[15]
        )
    }
}

impl Serialize for AdapterId {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        serializer.serialize_str(&self.to_string())
    }
}

impl<'de> Deserialize<'de> for AdapterId {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        let raw = String::deserialize(deserializer)?;
        Self::parse(&raw).map_err(serde::de::Error::custom)
    }
}

/// Which adapter the user requested: exactly one, or none.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub enum AdapterSelection {
    #[default]
    None,
    Adapter(AdapterId),
}

impl AdapterSelection {
    pub fn adapter_id(&self) -> Option<AdapterId> {
        match self {
            Self::None => None,
            Self::Adapter(id) => Some(*id),
        }
    }

    pub fn is_none(&self) -> bool {
        matches!(self, Self::None)
    }
}

// ---------------------------------------------------------------------------
// Adapter snapshot
// ---------------------------------------------------------------------------
