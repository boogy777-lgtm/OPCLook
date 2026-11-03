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

/// Product policy for locally submitted actions, not OPC protocol limits.
pub const ACTION_MAX_BYTES: usize = 1024 * 1024;
pub const ACTION_MAX_DEPTH: usize = 32;
pub const ACTION_MAX_NODES: usize = 65_536;
pub const ACTION_MAX_CHILDREN: usize = 65_536;

/// Does not contain input contents; safe to project into frontend diagnostics.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ActionPayloadError {
    Bytes,
    Depth,
    Nodes,
    Collection,
}

fn add_bytes(total: &mut usize, n: usize) -> Result<(), ActionPayloadError> {
    *total = total.checked_add(n).ok_or(ActionPayloadError::Bytes)?;
    if *total > ACTION_MAX_BYTES {
        Err(ActionPayloadError::Bytes)
    } else {
        Ok(())
    }
}

/// Check borrowed data without cloning, formatting, or walking past the depth bound.
pub fn validate_write_payload(value: &crate::model::Value) -> Result<(), ActionPayloadError> {
    use crate::model::Value;
    fn walk(
        value: &Value,
        depth: usize,
        nodes: &mut usize,
        bytes: &mut usize,
    ) -> Result<(), ActionPayloadError> {
        if depth > ACTION_MAX_DEPTH {
            return Err(ActionPayloadError::Depth);
        }
        *nodes = nodes.checked_add(1).ok_or(ActionPayloadError::Nodes)?;
        if *nodes > ACTION_MAX_NODES {
            return Err(ActionPayloadError::Nodes);
        }
        match value {
            Value::Str(v) | Value::NodeId(v) | Value::Other(v) => add_bytes(bytes, v.len())?,
            Value::Bytes(v) => add_bytes(bytes, v.len())?,
            Value::QualifiedName { name, .. } => add_bytes(bytes, name.len())?,
            Value::LocalizedText { locale, text } => {
                add_bytes(bytes, locale.len())?;
                add_bytes(bytes, text.len())?;
            }
            Value::Array(items) => {
                if items.len() > ACTION_MAX_CHILDREN {
                    return Err(ActionPayloadError::Collection);
                }
                for item in items {
                    walk(item, depth + 1, nodes, bytes)?;
                }
            }
            Value::Struct { type_name, fields } => {
                if fields.len() > ACTION_MAX_CHILDREN {
                    return Err(ActionPayloadError::Collection);
                }
                add_bytes(bytes, type_name.len())?;
                for (name, value) in fields {
                    add_bytes(bytes, name.len())?;
                    walk(value, depth + 1, nodes, bytes)?;
                }
            }
            Value::Empty
            | Value::Bool(_)
            | Value::I64(_)
            | Value::U64(_)
            | Value::F64(_)
            | Value::DateTime(_) => {}
        }
        Ok(())
    }
    walk(value, 0, &mut 0, &mut 0)
}

pub fn validate_browse_payload(
    node_id: &str,
    view_id: Option<&str>,
) -> Result<(), ActionPayloadError> {
    let mut bytes = 0;
    add_bytes(&mut bytes, node_id.len())?;
    if let Some(view_id) = view_id {
        add_bytes(&mut bytes, view_id.len())?;
    }
    Ok(())
}

/// Check a single UTF-8 field before a frontend creates an owned value.
pub fn validate_action_text(text: &str) -> Result<(), ActionPayloadError> {
    add_bytes(&mut 0, text.len())
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
    fn bounded_utf8_bytes_and_browse_aggregate() {
        let text = "я".repeat(ACTION_MAX_BYTES / 2);
        assert_eq!(validate_action_text(&text), Ok(()));
        assert_eq!(
            validate_browse_payload(&text, Some("x")),
            Err(ActionPayloadError::Bytes)
        );
        assert_eq!(
            validate_write_payload(&crate::model::Value::Str(text)),
            Ok(())
        );
        assert_eq!(
            validate_write_payload(&crate::model::Value::Bytes(vec![0; ACTION_MAX_BYTES + 1])),
            Err(ActionPayloadError::Bytes)
        );
        assert_eq!(validate_browse_payload("", None), Ok(()));
    }

    #[test]
    fn depth_nodes_collection_and_nested_names_are_bounded() {
        use crate::model::Value;
        let mut deep = Value::I64(1);
        for _ in 0..ACTION_MAX_DEPTH {
            deep = Value::Array(vec![deep]);
        }
        assert_eq!(validate_write_payload(&deep), Ok(()));
        deep = Value::Array(vec![deep]);
        assert_eq!(
            validate_write_payload(&deep),
            Err(ActionPayloadError::Depth)
        );
        let wide = Value::Array(vec![Value::Empty; ACTION_MAX_CHILDREN + 1]);
        assert_eq!(
            validate_write_payload(&wide),
            Err(ActionPayloadError::Collection)
        );
        let many = Value::Array(vec![Value::Empty; ACTION_MAX_NODES]);
        assert_eq!(
            validate_write_payload(&many),
            Err(ActionPayloadError::Nodes)
        );
        let valid = Value::Struct {
            type_name: "T".into(),
            fields: vec![(
                "x".into(),
                Value::LocalizedText {
                    locale: "ru".into(),
                    text: "тег".into(),
                },
            )],
        };
        assert_eq!(validate_write_payload(&valid), Ok(()));
        let invalid = Value::Struct {
            type_name: String::new(),
            fields: vec![("x".repeat(ACTION_MAX_BYTES), Value::Str("!".into()))],
        };
        assert_eq!(
            validate_write_payload(&invalid),
            Err(ActionPayloadError::Bytes)
        );
    }

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
