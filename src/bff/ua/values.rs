use crate::bff::model::{DataValue, ItemInfo, ItemKey, ItemSpec, Quality, Value};
use opcua::types::{
    DataChangeFilter, DataChangeTrigger, DataValue as UaDataValue, DeadbandType, ExtensionObject,
    LocalizedText, NodeId, ObjectId, QualifiedName, StatusCode, UAString, Variant,
    VariantScalarTypeId, VariantTypeId,
};

pub(super) fn fallback_items(items: &[ItemSpec]) -> Vec<(ItemKey, ItemInfo)> {
    items
        .iter()
        .map(|item| {
            (
                item.key,
                ItemInfo {
                    data_type: None,
                    type_id: None,
                    writable: true,
                },
            )
        })
        .collect()
}

/// Build the DataChangeFilter for an absolute deadband. `deadband <= 0` yields
/// no filter (deliver on every change). async-opcua 0.19 exposes `DataChangeFilter`
/// with `DeadbandType::Absolute`, so an absolute «мёртвая зона» is supported.
pub(super) fn data_change_filter(deadband: f64) -> ExtensionObject {
    if deadband > 0.0 {
        ExtensionObject::from_message(DataChangeFilter {
            trigger: DataChangeTrigger::StatusValue,
            deadband_type: DeadbandType::Absolute as u32,
            deadband_value: deadband,
        })
    } else {
        ExtensionObject::null()
    }
}

/// Extracts the built-in scalar variant type from a node's DataType attribute value.
pub(super) fn ua_variant_scalar(dv: &UaDataValue) -> Option<VariantScalarTypeId> {
    let value = dv.value.as_ref()?;
    let node_id = match value {
        Variant::NodeId(node_id) => node_id.as_ref(),
        _ => return None,
    };
    VariantScalarTypeId::try_from(node_id).ok().filter(|t| {
        matches!(
            t,
            VariantScalarTypeId::Boolean
                | VariantScalarTypeId::SByte
                | VariantScalarTypeId::Byte
                | VariantScalarTypeId::Int16
                | VariantScalarTypeId::UInt16
                | VariantScalarTypeId::Int32
                | VariantScalarTypeId::UInt32
                | VariantScalarTypeId::Int64
                | VariantScalarTypeId::UInt64
                | VariantScalarTypeId::Float
                | VariantScalarTypeId::Double
                | VariantScalarTypeId::String
                | VariantScalarTypeId::DateTime
                | VariantScalarTypeId::ByteString
                | VariantScalarTypeId::LocalizedText
                | VariantScalarTypeId::NodeId
                | VariantScalarTypeId::QualifiedName
        )
    })
}

/// Returns true if the node's AccessLevel attribute includes CurrentWrite (bit 2).
pub(super) fn ua_access_current_write(dv: &UaDataValue) -> Option<bool> {
    let value = dv.value.as_ref()?;
    match value {
        Variant::Byte(level) => Some(level & 0x02 != 0),
        _ => None,
    }
}

pub(super) fn ua_type_name(t: VariantScalarTypeId) -> &'static str {
    match t {
        VariantScalarTypeId::Boolean => "Boolean",
        VariantScalarTypeId::SByte => "SByte",
        VariantScalarTypeId::Byte => "Byte",
        VariantScalarTypeId::Int16 => "Int16",
        VariantScalarTypeId::UInt16 => "UInt16",
        VariantScalarTypeId::Int32 => "Int32",
        VariantScalarTypeId::UInt32 => "UInt32",
        VariantScalarTypeId::Int64 => "Int64",
        VariantScalarTypeId::UInt64 => "UInt64",
        VariantScalarTypeId::Float => "Float",
        VariantScalarTypeId::Double => "Double",
        VariantScalarTypeId::String => "String",
        VariantScalarTypeId::DateTime => "DateTime",
        VariantScalarTypeId::ByteString => "ByteString",
        VariantScalarTypeId::LocalizedText => "LocalizedText",
        VariantScalarTypeId::NodeId => "NodeId",
        VariantScalarTypeId::QualifiedName => "QualifiedName",
        _ => "Other",
    }
}

pub(super) fn ua_data_value(dv: &UaDataValue) -> DataValue {
    let status = dv.status.unwrap_or(StatusCode::Good);
    let quality = Quality::from_ua(
        status.bits(),
        format!("{} (0x{:08X})", status.to_string(), status.bits()),
    );
    let value = dv
        .value
        .as_ref()
        .map(variant_to_value)
        .unwrap_or(Value::Empty);
    DataValue {
        value,
        quality,
        timestamp: dv.source_timestamp.map(|dt| dt.as_chrono()),
    }
}

pub(super) fn variant_to_value(variant: &Variant) -> Value {
    match variant {
        Variant::Empty => Value::Empty,
        Variant::Boolean(v) => Value::Bool(*v),
        Variant::SByte(v) => Value::I64(i64::from(*v)),
        Variant::Byte(v) => Value::U64(u64::from(*v)),
        Variant::Int16(v) => Value::I64(i64::from(*v)),
        Variant::UInt16(v) => Value::U64(u64::from(*v)),
        Variant::Int32(v) => Value::I64(i64::from(*v)),
        Variant::UInt32(v) => Value::U64(u64::from(*v)),
        Variant::Int64(v) => Value::I64(*v),
        Variant::UInt64(v) => Value::U64(*v),
        Variant::Float(v) => Value::F64(f64::from(*v)),
        Variant::Double(v) => Value::F64(*v),
        Variant::String(v) => Value::Str(v.to_string()),
        Variant::DateTime(v) => Value::DateTime(v.as_chrono()),
        Variant::ByteString(v) => Value::Bytes(v.as_ref().to_vec()),
        Variant::StatusCode(v) => Value::Other(format!("0x{:08X}", v.bits())),
        Variant::NodeId(v) => Value::NodeId(v.to_string()),
        Variant::ExpandedNodeId(v) => Value::NodeId(v.node_id.to_string()),
        Variant::QualifiedName(v) => Value::QualifiedName {
            namespace_index: v.namespace_index,
            name: v.name.to_string(),
        },
        Variant::LocalizedText(v) => Value::LocalizedText {
            locale: v.locale.to_string(),
            text: v.text.to_string(),
        },
        Variant::Guid(v) => Value::Other(format!("guid:{v}")),
        Variant::Variant(v) => variant_to_value(v),
        Variant::DataValue(v) => {
            let dv = ua_data_value(v);
            dv.value
        }
        Variant::ExtensionObject(v) => {
            // ExtensionObjects are structured/encoded values we cannot fully
            // decode in the generic path; surface a stable textual summary.
            let kind = v.body.as_ref().map(|b| b.type_name()).unwrap_or("null");
            Value::Other(format!("structured:{kind}"))
        }
        Variant::Array(array) => {
            // Multi-dimensional arrays flatten the values into nested 1-D
            // Arrays mirroring the dimension layout.
            if let Some(dims) = &array.dimensions {
                let values: Vec<Value> = array.values.iter().map(variant_to_value).collect();
                Value::Array(flatten_dimensions(&values, dims))
            } else {
                Value::Array(
                    array
                        .values
                        .iter()
                        .take(512)
                        .map(variant_to_value)
                        .collect(),
                )
            }
        }
        other => Value::Other(format!("{other:?}")),
    }
}

/// Rebuilds a nested `Value::Array` tree from a flat element list and the
/// dimension vector returned by the server (highest rank first).
fn flatten_dimensions(values: &Vec<Value>, dims: &[u32]) -> Vec<Value> {
    if dims.len() <= 1 {
        return values.clone();
    }
    let head = dims[0] as usize;
    let tail = &dims[1..];
    let tail_len: usize = tail.iter().map(|d| *d as usize).product::<usize>().max(1);
    let mut out = Vec::with_capacity(head);
    for chunk in values.chunks(tail_len).take(head) {
        out.push(Value::Array(flatten_dimensions(&chunk.to_vec(), tail)));
    }
    out
}

/// Builds an OPC UA [Variant] for a write operation.
///
/// When `data_type` is known it drives the concrete scalar type (with range /
/// kind validation); otherwise it falls back to inferring the type from the
/// [Value] variant so the previous behavior is preserved.
pub(crate) fn value_to_ua_variant(
    value: &Value,
    data_type: Option<VariantScalarTypeId>,
) -> Result<Variant, String> {
    match data_type {
        Some(scalar) => scalar_value_to_variant(value, scalar),
        None => infer_value_to_variant(value),
    }
}

fn infer_value_to_variant(value: &Value) -> Result<Variant, String> {
    Ok(match value {
        Value::Empty => Variant::Empty,
        Value::Bool(v) => Variant::Boolean(*v),
        Value::I64(v) => Variant::Int64(*v),
        Value::U64(v) => Variant::UInt64(*v),
        Value::F64(v) => Variant::Double(*v),
        Value::Str(v) => Variant::String(UAString::from(v.as_str())),
        Value::Bytes(v) => Variant::ByteString(opcua::types::ByteString::from(v.clone())),
        Value::DateTime(v) => Variant::DateTime(Box::new(opcua::types::DateTime::from(*v))),
        Value::NodeId(v) => Variant::NodeId(Box::new(
            parse_node(v).ok_or_else(|| format!("некорректный NodeId при записи: \"{v}\""))?,
        )),
        Value::QualifiedName {
            namespace_index,
            name,
        } => {
            let qn = match name.split_once(':') {
                Some((ns, rest)) => {
                    let ns: u16 = ns.trim().parse().unwrap_or(*namespace_index);
                    QualifiedName::new(ns, rest.trim().to_string())
                }
                None => QualifiedName::new(*namespace_index, name.clone()),
            };
            Variant::QualifiedName(Box::new(qn))
        }
        Value::LocalizedText { locale, text } => {
            Variant::LocalizedText(Box::new(LocalizedText::new(locale, text)))
        }
        Value::Array(items) => {
            if items.iter().any(|i| matches!(i, Value::Array(_))) {
                return Err(
                    "запись многомерных массивов не поддерживается; введите одномерный массив"
                        .to_string(),
                );
            }
            let values = items
                .iter()
                .map(infer_value_to_variant)
                .collect::<Result<Vec<_>, _>>()?;
            Variant::Array(Box::new(opcua::types::Array {
                value_type: match values.first().map(|v| v.type_id()) {
                    Some(VariantTypeId::Scalar(t)) => t,
                    Some(VariantTypeId::Array(t, _)) => t,
                    _ => VariantScalarTypeId::Double,
                },
                values,
                dimensions: None,
            }))
        }
        Value::Other(v) => return Err(format!("нельзя записать значение типа Other: {v}")),
    })
}

/// Converts a scalar (or flat array of scalars) into the requested UA scalar type.
///
/// Returns a descriptive error on a type/range mismatch so the caller can surface
/// it in the journal instead of silently writing a value of the wrong type.
fn scalar_value_to_variant(value: &Value, scalar: VariantScalarTypeId) -> Result<Variant, String> {
    let build_scalar = |value: &Value| -> Result<Variant, String> {
        match scalar {
            VariantScalarTypeId::Boolean => {
                let b = match value {
                    Value::Bool(b) => *b,
                    Value::I64(v) => *v != 0,
                    Value::U64(v) => *v != 0,
                    Value::F64(v) => *v != 0.0,
                    Value::Str(s) => match s.trim().to_ascii_lowercase().as_str() {
                        "true" | "1" | "да" | "on" => true,
                        "false" | "0" | "нет" | "off" => false,
                        _ => {
                            return Err(format!(
                                "\"{s}\" не является логическим значением для Boolean"
                            ));
                        }
                    },
                    _ => return Err("значение не является логическим для Boolean".to_string()),
                };
                Ok(Variant::Boolean(b))
            }
            VariantScalarTypeId::SByte => {
                let v = to_i64(value, "SByte")?;
                i8::try_from(v)
                    .map(|n| Variant::SByte(n))
                    .map_err(|_| format!("значение {v} вне диапазона SByte"))
            }
            VariantScalarTypeId::Byte => {
                let v = to_i64(value, "Byte")?;
                u8::try_from(v)
                    .map(Variant::Byte)
                    .map_err(|_| format!("значение {v} вне диапазона Byte"))
            }
            VariantScalarTypeId::Int16 => {
                let v = to_i64(value, "Int16")?;
                i16::try_from(v)
                    .map(Variant::Int16)
                    .map_err(|_| format!("значение {v} вне диапазона Int16"))
            }
            VariantScalarTypeId::UInt16 => {
                let v = to_i64(value, "UInt16")?;
                u16::try_from(v)
                    .map(Variant::UInt16)
                    .map_err(|_| format!("значение {v} вне диапазона UInt16"))
            }
            VariantScalarTypeId::Int32 => {
                let v = to_i64(value, "Int32")?;
                i32::try_from(v)
                    .map(Variant::Int32)
                    .map_err(|_| format!("значение {v} вне диапазона Int32"))
            }
            VariantScalarTypeId::UInt32 => {
                let v = to_i64(value, "UInt32")?;
                u32::try_from(v)
                    .map(Variant::UInt32)
                    .map_err(|_| format!("значение {v} вне диапазона UInt32"))
            }
            VariantScalarTypeId::Int64 => to_i64(value, "Int64").map(Variant::Int64),
            VariantScalarTypeId::UInt64 => {
                let v = match value {
                    Value::U64(v) => *v,
                    Value::I64(v) => u64::try_from(*v)
                        .map_err(|_| format!("значение {v} вне диапазона UInt64"))?,
                    Value::F64(v) if *v >= 0.0 => *v as u64,
                    Value::Str(s) => s
                        .trim()
                        .parse::<u64>()
                        .map_err(|_| format!("\"{s}\" не является целым числом UInt64"))?,
                    Value::Bool(b) => u64::from(*b),
                    other => {
                        return Err(format!(
                            "значение {other} не является беззнаковым целым для UInt64"
                        ));
                    }
                };
                Ok(Variant::UInt64(v))
            }
            VariantScalarTypeId::Float => {
                let f = to_f64(value, "Float")?;
                Ok(Variant::Float(f as f32))
            }
            VariantScalarTypeId::Double => to_f64(value, "Double").map(Variant::Double),
            VariantScalarTypeId::String => {
                let s = match value {
                    Value::Str(s) => s.clone(),
                    other => other.to_string(),
                };
                Ok(Variant::String(UAString::from(s)))
            }
            VariantScalarTypeId::DateTime => match value {
                Value::DateTime(dt) => Ok(Variant::DateTime(Box::new(
                    opcua::types::DateTime::from(*dt),
                ))),
                Value::Str(s) => s
                    .parse::<opcua::types::DateTime>()
                    .map(|dt| Variant::DateTime(Box::new(dt)))
                    .map_err(|_| format!("\"{s}\" не является датой/временем DateTime")),
                other => Err(format!(
                    "значение {other} не является датой/временем для DateTime"
                )),
            },
            VariantScalarTypeId::ByteString => match value {
                Value::Bytes(v) => Ok(Variant::ByteString(opcua::types::ByteString::from(
                    v.clone(),
                ))),
                Value::Str(s) => Ok(Variant::ByteString(opcua::types::ByteString::from(
                    s.as_bytes().to_vec(),
                ))),
                other => Err(format!(
                    "значение {other} не является байтовой строкой для ByteString"
                )),
            },
            VariantScalarTypeId::LocalizedText => match value {
                Value::Str(s) => Ok(Variant::LocalizedText(Box::new(LocalizedText::new("", s)))),
                Value::LocalizedText { locale, text } => Ok(Variant::LocalizedText(Box::new(
                    LocalizedText::new(locale, text),
                ))),
                other => Err(format!(
                    "значение {other} не является текстом для LocalizedText"
                )),
            },
            VariantScalarTypeId::NodeId => match value {
                Value::NodeId(s) => parse_node(s)
                    .map(|n| Variant::NodeId(Box::new(n)))
                    .ok_or_else(|| format!("некорректный NodeId: \"{s}\"")),
                Value::Str(s) => parse_node(s)
                    .map(|n| Variant::NodeId(Box::new(n)))
                    .ok_or_else(|| format!("некорректный NodeId: \"{s}\"")),
                other => Err(format!("значение {other} не является NodeId")),
            },
            VariantScalarTypeId::QualifiedName => match value {
                Value::QualifiedName {
                    namespace_index,
                    name,
                } => {
                    let qn = match name.split_once(':') {
                        Some((ns, rest)) => {
                            let ns: u16 = ns.trim().parse().unwrap_or(*namespace_index);
                            QualifiedName::new(ns, rest.trim().to_string())
                        }
                        None => QualifiedName::new(*namespace_index, name.clone()),
                    };
                    Ok(Variant::QualifiedName(Box::new(qn)))
                }
                Value::Str(s) => {
                    let qn = match s.split_once(':') {
                        Some((ns, name)) => {
                            let ns = ns.trim().parse::<u16>().map_err(|_| {
                                format!("некорректный префикс пространства имён в \"{s}\"")
                            })?;
                            QualifiedName::new(ns, name.trim().to_string())
                        }
                        None => QualifiedName::new(0, s.clone()),
                    };
                    Ok(Variant::QualifiedName(Box::new(qn)))
                }
                other => Err(format!("значение {other} не является QualifiedName")),
            },
            VariantScalarTypeId::ExtensionObject
            | VariantScalarTypeId::Guid
            | VariantScalarTypeId::XmlElement => Err(format!(
                "запись типа {scalar} (структурированных данных) не поддерживается"
            )),
            other => Err(format!("запись типа {other} не поддерживается")),
        }
    };

    match value {
        Value::Array(items) => {
            if items.iter().any(|i| matches!(i, Value::Array(_))) {
                return Err(
                    "запись многомерных массивов не поддерживается; введите одномерный массив"
                        .to_string(),
                );
            }
            let values = items
                .iter()
                .map(&build_scalar)
                .collect::<Result<Vec<_>, _>>()?;
            let array = opcua::types::Array::new(scalar, values)
                .map_err(|e| format!("не удалось собрать массив: {e}"))?;
            Ok(Variant::Array(Box::new(array)))
        }
        single => build_scalar(single),
    }
}

fn to_i64(value: &Value, ty: &str) -> Result<i64, String> {
    match value {
        Value::Bool(b) => Ok(i64::from(*b)),
        Value::I64(v) => Ok(*v),
        Value::U64(v) => i64::try_from(*v).map_err(|_| format!("значение {v} вне диапазона {ty}")),
        Value::F64(v) if v.is_finite() => Ok(*v as i64),
        Value::Str(s) => s
            .trim()
            .parse::<i64>()
            .map_err(|_| format!("\"{s}\" не является целым числом для {ty}")),
        other => Err(format!(
            "значение {other} не является целым числом для {ty}"
        )),
    }
}

fn to_f64(value: &Value, ty: &str) -> Result<f64, String> {
    match value {
        Value::F64(v) => Ok(*v),
        Value::I64(v) => Ok(*v as f64),
        Value::U64(v) => Ok(*v as f64),
        Value::Bool(b) => Ok(if *b { 1.0 } else { 0.0 }),
        Value::Str(s) => s
            .trim()
            .replace(',', ".")
            .parse::<f64>()
            .map_err(|_| format!("\"{s}\" не является числом для {ty}")),
        other => Err(format!("значение {other} не является числом для {ty}")),
    }
}

pub(super) fn parse_node(text: &str) -> Option<NodeId> {
    let text = text.trim();
    if text.is_empty() {
        return Some(NodeId::from(ObjectId::RootFolder));
    }
    text.parse::<NodeId>().ok()
}
