use super::{BackendMsg, Quality, QualityKind};
use std::fmt;

#[derive(Clone, Debug, PartialEq)]
pub enum Value {
    Empty,
    Bool(bool),
    I64(i64),
    U64(u64),
    F64(f64),
    Str(String),
    Bytes(Vec<u8>),
    DateTime(chrono::DateTime<chrono::Utc>),
    Array(Vec<Value>),
    NodeId(String),
    QualifiedName {
        namespace_index: u16,
        name: String,
    },
    LocalizedText {
        locale: String,
        text: String,
    },
    /// V3: a decoded structure (OPC UA ExtensionObject whose type the server
    /// described): its type name and the fields in declaration order.
    Struct {
        type_name: String,
        fields: Vec<(String, Value)>,
    },
    Other(String),
}

impl Value {
    pub fn as_f64(&self) -> Option<f64> {
        match self {
            Self::I64(v) => Some(*v as f64),
            Self::U64(v) => Some(*v as f64),
            Self::F64(v) => Some(*v),
            Self::Bool(v) => Some(if *v { 1.0 } else { 0.0 }),
            Self::Str(v) => v.trim().parse().ok(),
            Self::LocalizedText { text, .. } => text.trim().parse().ok(),
            _ => None,
        }
    }
}

/// How many leading bytes are shown for a `Bytes` value before truncation.
const BYTES_SHOWN: usize = 16;
/// How many leading elements are shown for an `Array` value before truncation.
const ARRAY_SHOWN: usize = 16;

impl fmt::Display for Value {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Empty => Ok(()),
            Self::Bool(v) => write!(f, "{v}"),
            Self::I64(v) => write!(f, "{v}"),
            Self::U64(v) => write!(f, "{v}"),
            Self::F64(v) => {
                if v.is_finite() {
                    write!(f, "{v}")
                } else {
                    write!(f, "{v:?}")
                }
            }
            Self::Str(v) => f.write_str(v),
            Self::Bytes(v) => {
                let shown = v
                    .iter()
                    .take(BYTES_SHOWN)
                    .map(|b| format!("{b:02X}"))
                    .collect::<Vec<_>>();
                write!(f, "{}", shown.join(" "))?;
                if v.len() > BYTES_SHOWN {
                    write!(f, "…")?;
                }
                Ok(())
            }
            Self::DateTime(v) => write!(
                f,
                "{}",
                v.with_timezone(&chrono::Local)
                    .format("%Y-%m-%d %H:%M:%S%.3f")
            ),
            Self::Array(items) => {
                f.write_str("[")?;
                for (i, item) in items.iter().take(ARRAY_SHOWN).enumerate() {
                    if i > 0 {
                        f.write_str(", ")?;
                    }
                    write!(f, "{item}")?;
                }
                if items.len() > ARRAY_SHOWN {
                    write!(f, "…({})", items.len())?;
                }
                f.write_str("]")
            }
            Self::NodeId(v) => write!(f, "NodeId({v})"),
            Self::QualifiedName {
                namespace_index,
                name,
            } => {
                if *namespace_index == 0 {
                    write!(f, "{name}")
                } else {
                    write!(f, "{namespace_index}:{name}")
                }
            }
            Self::LocalizedText { locale, text } => {
                if locale.trim().is_empty() {
                    f.write_str(text)
                } else {
                    write!(f, "[{locale}] {text}")
                }
            }
            Self::Struct { fields, .. } => {
                f.write_str("{")?;
                for (i, (name, value)) in fields.iter().take(ARRAY_SHOWN).enumerate() {
                    if i > 0 {
                        f.write_str(", ")?;
                    }
                    write!(f, "{name}: {value}")?;
                }
                if fields.len() > ARRAY_SHOWN {
                    write!(f, ", …({})", fields.len())?;
                }
                f.write_str("}")
            }
            Self::Other(v) => f.write_str(v),
        }
    }
}

impl Value {
    /// Number of direct children without allocating names or a collection.
    pub fn child_count(&self) -> usize {
        match self {
            Self::Struct { fields, .. } => fields.len(),
            Self::Array(items) => items.len(),
            _ => 0,
        }
    }

    /// A child and its name, allocating only for an array index.
    pub fn child(&self, index: usize) -> Option<(std::borrow::Cow<'_, str>, &Value)> {
        match self {
            Self::Struct { fields, .. } => fields
                .get(index)
                .map(|(name, value)| (std::borrow::Cow::Borrowed(name.as_str()), value)),
            Self::Array(items) => items
                .get(index)
                .map(|value| (std::borrow::Cow::Owned(format!("[{index}]")), value)),
            _ => None,
        }
    }

    /// V3: children of a structure (field name, value) or an array (`[i]`,
    /// value), for the expandable table row. Empty for scalars.
    pub fn children(&self) -> Vec<(String, &Value)> {
        match self {
            Self::Struct { fields, .. } => fields
                .iter()
                .map(|(name, value)| (name.clone(), value))
                .collect(),
            Self::Array(items) => items
                .iter()
                .enumerate()
                .map(|(i, value)| (format!("[{i}]"), value))
                .collect(),
            _ => Vec::new(),
        }
    }

    /// Whether the value has children to expand (a non-empty structure or array).
    pub fn is_expandable(&self) -> bool {
        match self {
            Self::Struct { fields, .. } => !fields.is_empty(),
            Self::Array(items) => !items.is_empty(),
            _ => false,
        }
    }
}

#[derive(Clone, Debug)]
pub struct DataValue {
    pub value: Value,
    pub quality: Quality,
    pub timestamp: Option<chrono::DateTime<chrono::Utc>>,
    /// Why the backend produced no value (set by [`DataValue::bad`]); `None`
    /// for a value the server delivered.
    pub reason: Option<BackendMsg>,
}

impl DataValue {
    /// A Bad value carrying a typed reason. Frontends render it in their chosen
    /// language; raw messages are also retained as quality text.
    pub fn bad(reason: BackendMsg) -> Self {
        Self {
            value: Value::Empty,
            quality: Quality {
                code: 0,
                kind: QualityKind::Bad,
                text: match &reason {
                    BackendMsg::Raw(text) => text.clone().into(),
                    _ => "".into(),
                },
            },
            timestamp: None,
            reason: Some(reason),
        }
    }
}

#[derive(Clone, Debug)]
pub struct ItemInfo {
    pub data_type: Option<String>,
    /// OPC DA canonical VT_* code (None for OPC UA).
    pub type_id: Option<u16>,
    pub writable: bool,
}
