use std::fmt;

/// Nonzero identity of a session, distinct from a request identity.
///
/// ```compile_fail
/// use opclook_contract::model::{RequestId, SessionId};
/// let _: RequestId = SessionId::FIRST;
/// ```
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct SessionId(std::num::NonZeroU64);
impl SessionId {
    pub const FIRST: Self = Self(std::num::NonZeroU64::MIN);
    pub const fn new(value: u64) -> Option<Self> {
        match std::num::NonZeroU64::new(value) {
            Some(value) => Some(Self(value)),
            None => None,
        }
    }
    pub const fn get(self) -> u64 {
        self.0.get()
    }
    pub const fn successor(self) -> Option<Self> {
        match self.get().checked_add(1) {
            Some(value) => Self::new(value),
            None => None,
        }
    }
}
impl fmt::Display for SessionId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        self.get().fmt(f)
    }
}
/// The owner of a command or event: global discovery or a real session.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum Owner {
    Discovery,
    Session(SessionId),
}
impl From<SessionId> for Owner {
    fn from(id: SessionId) -> Self {
        Self::Session(id)
    }
}
impl fmt::Display for Owner {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Discovery => f.write_str("discovery"),
            Self::Session(id) => id.fmt(f),
        }
    }
}

/// Nonzero client handle identifying one item.
///
/// Item and scan identities cannot be interchanged:
/// ```compile_fail
/// use opclook_contract::model::{ItemKey, ScanId};
/// let _: ScanId = ItemKey::FIRST;
/// ```
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct ItemKey(std::num::NonZeroU32);
impl ItemKey {
    pub const FIRST: Self = Self(std::num::NonZeroU32::MIN);
    pub const fn new(value: u32) -> Option<Self> {
        match std::num::NonZeroU32::new(value) {
            Some(value) => Some(Self(value)),
            None => None,
        }
    }
    pub const fn get(self) -> u32 {
        self.0.get()
    }
    pub const fn successor(self) -> Option<Self> {
        match self.get().checked_add(1) {
            Some(value) => Self::new(value),
            None => None,
        }
    }
}
impl fmt::Display for ItemKey {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        self.get().fmt(f)
    }
}
/// Nonzero identity of one tracked request.
///
/// ```compile_fail
/// use opclook_contract::model::{RequestId, ScanId};
/// let _: ScanId = RequestId::FIRST;
/// ```
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct RequestId(std::num::NonZeroU64);
impl RequestId {
    pub const FIRST: Self = Self(std::num::NonZeroU64::MIN);
    pub const fn new(value: u64) -> Option<Self> {
        match std::num::NonZeroU64::new(value) {
            Some(value) => Some(Self(value)),
            None => None,
        }
    }
    pub const fn get(self) -> u64 {
        self.0.get()
    }
    pub const fn successor(self) -> Option<Self> {
        match self.get().checked_add(1) {
            Some(value) => Self::new(value),
            None => None,
        }
    }
}
impl fmt::Display for RequestId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        self.get().fmt(f)
    }
}

/// Whether an event answers a tracked command or is a spontaneous update.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Origin {
    Reply(RequestId),
    Unsolicited,
    /// Unsolicited event from the connection created by this Connect request.
    Session(RequestId),
}
impl From<Option<RequestId>> for Origin {
    fn from(id: Option<RequestId>) -> Self {
        match id {
            Some(id) => Self::Reply(id),
            None => Self::Unsolicited,
        }
    }
}

/// Identity of one streaming UA subnet scan. Zero is never a scan identity.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct ScanId(std::num::NonZeroU64);

impl ScanId {
    pub const FIRST: Self = Self(std::num::NonZeroU64::MIN);

    pub const fn new(value: u64) -> Option<Self> {
        match std::num::NonZeroU64::new(value) {
            Some(value) => Some(Self(value)),
            None => None,
        }
    }

    pub const fn get(self) -> u64 {
        self.0.get()
    }

    /// Returns None after the last possible ID, without wrapping or reuse.
    pub const fn successor(self) -> Option<Self> {
        match self.get().checked_add(1) {
            Some(value) => Self::new(value),
            None => None,
        }
    }
}

impl fmt::Display for ScanId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        self.get().fmt(f)
    }
}
