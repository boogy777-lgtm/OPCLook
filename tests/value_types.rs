//! Unit coverage for the protocol-neutral `Value` type as extended for full OPC
//! UA built-in type support: the new variants (NodeId, QualifiedName,
//! LocalizedText), display formatting (ByteString hex + array truncation), and
//! numeric coercion. These are hermetic and never need a server.

use opcda_egui::bff::model::Value;

#[test]
fn node_id_value_formats_as_string() {
    assert_eq!(
        format!("{}", Value::NodeId("i=85".to_string())),
        "NodeId(i=85)"
    );
    assert_eq!(
        format!("{}", Value::NodeId("ns=2;s=MyVar".to_string())),
        "NodeId(ns=2;s=MyVar)"
    );
    assert_eq!(Value::NodeId("i=85".to_string()).as_f64(), None);
}

#[test]
fn qualified_name_value_formats_with_namespace() {
    assert_eq!(
        format!(
            "{}",
            Value::QualifiedName {
                namespace_index: 0,
                name: "MyVar".to_string()
            }
        ),
        "MyVar"
    );
    assert_eq!(
        format!(
            "{}",
            Value::QualifiedName {
                namespace_index: 2,
                name: "MyVar".to_string()
            }
        ),
        "2:MyVar"
    );
    assert_eq!(
        Value::QualifiedName {
            namespace_index: 0,
            name: "3.5".to_string()
        }
        .as_f64(),
        None
    );
}

#[test]
fn localized_text_formats_with_locale_prefix() {
    assert_eq!(
        format!(
            "{}",
            Value::LocalizedText {
                locale: String::new(),
                text: "hi".to_string()
            }
        ),
        "hi"
    );
    assert_eq!(
        format!(
            "{}",
            Value::LocalizedText {
                locale: "ru".to_string(),
                text: "привет".to_string()
            }
        ),
        "[ru] привет"
    );
    assert_eq!(
        Value::LocalizedText {
            locale: String::new(),
            text: "12.5".to_string()
        }
        .as_f64(),
        Some(12.5)
    );
    assert_eq!(
        Value::LocalizedText {
            locale: "ru".to_string(),
            text: "abc".to_string()
        }
        .as_f64(),
        None
    );
}

#[test]
fn bytes_value_is_hex_with_truncation_marker() {
    assert_eq!(
        format!("{}", Value::Bytes(vec![0xDE, 0xAD, 0xBE, 0xEF])),
        "DE AD BE EF"
    );
    let long: Vec<u8> = (0u8..=30).collect();
    let s = format!("{}", Value::Bytes(long));
    assert!(
        s.starts_with("00 01 02 03 04 05 06 07 08 09 0A 0B 0C 0D 0E 0F"),
        "got: {s}"
    );
    assert!(s.ends_with('…'), "expected truncation ellipsis: {s}");
    assert!(
        !s.contains("1E"),
        "long bytes beyond 16 must be truncated: {s}"
    );
}

#[test]
fn array_value_truncates_after_sixteen_elements() {
    let short = Value::Array(vec![Value::I64(1), Value::I64(-2), Value::I64(3)]);
    assert_eq!(format!("{short}"), "[1, -2, 3]");

    let long: Vec<Value> = (0..20).map(Value::I64).collect();
    let s = format!("{}", Value::Array(long));
    assert!(
        s.starts_with("[0, 1, 2, 3, 4, 5, 6, 7, 8, 9, 10, 11, 12, 13, 14, 15"),
        "got: {s}"
    );
    assert!(s.ends_with("…20 эл.]"), "expected truncation suffix: {s}");
}

#[test]
fn scalar_value_roundtrip_stable() {
    // Existing scalar variants keep their exact display output untouched so
    // downstream consumers (write dialogs, CSV export) never change.
    assert_eq!(format!("{}", Value::Bool(true)), "true");
    assert_eq!(format!("{}", Value::I64(-42)), "-42");
    assert_eq!(format!("{}", Value::U64(7)), "7");
    assert_eq!(format!("{}", Value::F64(1.5)), "1.5");
    assert_eq!(format!("{}", Value::Str("text".to_string())), "text");
    assert_eq!(format!("{}", Value::Empty), "");
    assert_eq!(format!("{}", Value::Other("x".to_string())), "x");
}
