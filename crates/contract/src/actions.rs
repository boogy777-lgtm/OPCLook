//! Stable, transport-independent action identities. Codes are never request IDs.

#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub enum ActionId {
    OpenConnectDialog,
    Browse,
    Write,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct ActionSpec {
    pub id: ActionId,
    pub semantic_id: &'static str,
    pub code: u32,
    pub shortcut: Option<DefaultShortcut>,
}

/// Portable default binding. Mod means Command on macOS, Control elsewhere.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub enum DefaultShortcut {
    ModN,
}

pub const OPEN_CONNECT_DIALOG: ActionSpec = ActionSpec {
    id: ActionId::OpenConnectDialog,
    semantic_id: "ui.connect.open",
    code: 1001,
    shortcut: Some(DefaultShortcut::ModN),
};
pub const BROWSE: ActionSpec = ActionSpec {
    id: ActionId::Browse,
    semantic_id: "session.browse",
    code: 2001,
    shortcut: None,
};
pub const WRITE: ActionSpec = ActionSpec {
    id: ActionId::Write,
    semantic_id: "session.write",
    code: 2002,
    shortcut: None,
};
pub const CATALOG: &[ActionSpec] = &[OPEN_CONNECT_DIALOG, BROWSE, WRITE];

/// Domain payload remains typed and is independent of frontend and transport.
// Do not derive Debug: write values can contain sensitive data.
#[derive(Clone)]
pub enum DomainAction {
    Browse {
        session: crate::model::SessionId,
        protocol: crate::model::Protocol,
        node_id: String,
        filter: crate::model::NodeClassFilter,
        view_id: Option<String>,
    },
    Write {
        session: crate::model::SessionId,
        protocol: crate::model::Protocol,
        request: crate::model::WriteRequest,
    },
}

impl DomainAction {
    pub fn id(&self) -> ActionId {
        match self {
            Self::Browse { .. } => ActionId::Browse,
            Self::Write { .. } => ActionId::Write,
        }
    }
}
/// Once published, removed action codes must be retained here permanently.
pub const RETIRED_CODES: &[u32] = &[];

/// Catalog validation also accepts historical retired codes for compatibility checks.
pub fn catalog_valid(catalog: &[ActionSpec], retired: &[u32]) -> bool {
    use std::collections::HashSet;
    let mut ids = HashSet::new();
    let mut names = HashSet::new();
    let mut codes = HashSet::new();
    let mut shortcuts = HashSet::new();
    catalog.iter().all(|spec| {
        ids.insert(spec.id)
            && names.insert(spec.semantic_id)
            && codes.insert(spec.code)
            && !retired.contains(&spec.code)
            && spec
                .shortcut
                .is_none_or(|shortcut| shortcuts.insert(shortcut))
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashSet;

    #[test]
    fn identities_and_codes_are_unique_and_retired_codes_stay_reserved() {
        let mut ids = HashSet::new();
        let mut names = HashSet::new();
        let mut codes = HashSet::new();
        for spec in CATALOG {
            assert!(ids.insert(spec.id));
            assert!(names.insert(spec.semantic_id));
            assert!(codes.insert(spec.code));
            assert!(!RETIRED_CODES.contains(&spec.code));
        }
        assert_eq!(OPEN_CONNECT_DIALOG.semantic_id, "ui.connect.open");
        assert_eq!(OPEN_CONNECT_DIALOG.code, 1001);
        assert_eq!(BROWSE.semantic_id, "session.browse");
        assert_eq!(BROWSE.code, 2001);
        assert_eq!(WRITE.semantic_id, "session.write");
        assert_eq!(WRITE.code, 2002);
        assert_eq!(CATALOG, &[OPEN_CONNECT_DIALOG, BROWSE, WRITE]);
        assert!(catalog_valid(CATALOG, RETIRED_CODES));
        assert!(!catalog_valid(CATALOG, &[OPEN_CONNECT_DIALOG.code]));
        let conflicting = ActionSpec {
            shortcut: OPEN_CONNECT_DIALOG.shortcut,
            ..BROWSE
        };
        assert!(!catalog_valid(&[OPEN_CONNECT_DIALOG, conflicting], &[]));
        assert!(!catalog_valid(
            &[OPEN_CONNECT_DIALOG, OPEN_CONNECT_DIALOG],
            &[]
        ));
    }
}
