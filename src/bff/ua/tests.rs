use super::SCAN_SUBNET_TOO_LARGE;
use super::discovery::merge_network_info;
use super::io::parse_relative_path;
use super::scanner::{merge_scan_batch, run_scan_job, scan_server_key, scan_targets};
use super::security::{
    SECURITY_MODE_AUTO, SECURITY_MODE_SIGN, SECURITY_MODE_SIGN_AND_ENCRYPT, SECURITY_POLICY_AUTO,
    SECURITY_POLICY_BASIC256_SHA256, SECURITY_POLICY_NONE, resolve_security, security_error,
    security_error_detail,
};
use super::values::{data_change_filter, ua_type_name, value_to_ua_variant, variant_to_value};
use super::worker::{is_control_command, partition_commands};
use crate::bff::model::{
    Command, CommandKind, EndpointSpec, EventKind, Protocol, UaEndpointInfo, UaNetworkInfo,
    UaSecurityMode, UaSecurityPolicy, UaServerInfo, Value, WriteRequest,
};
use crate::bff::net::NetworkBinding;
use crate::constants::{DEFAULT_UA_PORT, SCAN_CONFIRM_THRESHOLD};
use crate::net::SubnetSpec;
use chrono::TimeZone;
use opcua::types::VariantScalarTypeId as Vt;
use opcua::types::{
    DataChangeFilter, DataChangeTrigger, DeadbandType, LocalizedText, MessageSecurityMode,
    MonitoringParameters, NodeId, QualifiedName, StatusCode, UAString, Variant,
    VariantScalarTypeId,
};
use std::collections::VecDeque;
use std::net::IpAddr;
use std::sync::Arc;
use std::sync::atomic::AtomicBool;
use std::time::Duration;

fn roundtrip_with_type(value: &Value, ty: Vt) -> Variant {
    value_to_ua_variant(value, Some(ty)).expect("typed conversion")
}

#[test]
fn typed_boolean() {
    assert_eq!(
        roundtrip_with_type(&Value::Bool(true), Vt::Boolean),
        Variant::Boolean(true)
    );
    assert_eq!(
        roundtrip_with_type(&Value::I64(1), Vt::Boolean),
        Variant::Boolean(true)
    );
    assert_eq!(
        roundtrip_with_type(&Value::I64(0), Vt::Boolean),
        Variant::Boolean(false)
    );
    assert_eq!(
        roundtrip_with_type(&Value::Str("да".to_string()), Vt::Boolean),
        Variant::Boolean(true)
    );
    assert!(value_to_ua_variant(&Value::Str("nope".to_string()), Some(Vt::Boolean)).is_err());
}

#[test]
fn typed_integer_widths() {
    assert_eq!(
        roundtrip_with_type(&Value::I64(5), Vt::SByte),
        Variant::SByte(5)
    );
    assert_eq!(
        roundtrip_with_type(&Value::I64(-5), Vt::Int16),
        Variant::Int16(-5)
    );
    assert_eq!(
        roundtrip_with_type(&Value::I64(7), Vt::Byte),
        Variant::Byte(7)
    );
    assert_eq!(
        roundtrip_with_type(&Value::I64(300), Vt::UInt16),
        Variant::UInt16(300)
    );
    assert_eq!(
        roundtrip_with_type(&Value::I64(123456), Vt::Int32),
        Variant::Int32(123456)
    );
    assert_eq!(
        roundtrip_with_type(&Value::I64(42), Vt::UInt32),
        Variant::UInt32(42)
    );
    assert_eq!(
        roundtrip_with_type(&Value::I64(i64::MIN), Vt::Int64),
        Variant::Int64(i64::MIN)
    );
    assert_eq!(
        roundtrip_with_type(&Value::U64(u64::MAX), Vt::UInt64),
        Variant::UInt64(u64::MAX)
    );
}

#[test]
fn typed_integer_out_of_range() {
    assert!(value_to_ua_variant(&Value::I64(300), Some(Vt::SByte)).is_err());
    assert!(value_to_ua_variant(&Value::I64(65536), Some(Vt::UInt16)).is_err());
    assert!(value_to_ua_variant(&Value::I64(-1), Some(Vt::UInt64)).is_err());
    assert!(value_to_ua_variant(&Value::I64(-1), Some(Vt::Byte)).is_err());
}

#[test]
fn typed_float() {
    assert_eq!(
        roundtrip_with_type(&Value::F64(1.5), Vt::Float),
        Variant::Float(1.5)
    );
    assert_eq!(
        roundtrip_with_type(&Value::F64(-2.25), Vt::Double),
        Variant::Double(-2.25)
    );
}

#[test]
fn typed_string_and_text() {
    assert_eq!(
        roundtrip_with_type(&Value::Str("hi".to_string()), Vt::String),
        Variant::String(UAString::from("hi"))
    );
    assert_eq!(
        roundtrip_with_type(&Value::I64(7), Vt::String),
        Variant::String(UAString::from("7"))
    );
    assert_eq!(
        roundtrip_with_type(&Value::Str("текс".to_string()), Vt::LocalizedText),
        Variant::LocalizedText(Box::new(LocalizedText::new("", "текс")))
    );
}

#[test]
fn typed_bytes() {
    assert_eq!(
        roundtrip_with_type(&Value::Bytes(vec![1, 2, 3]), Vt::ByteString),
        Variant::ByteString(opcua::types::ByteString::from(vec![1, 2, 3]))
    );
    assert_eq!(
        roundtrip_with_type(&Value::Str("ab".to_string()), Vt::ByteString),
        Variant::ByteString(opcua::types::ByteString::from(vec![b'a', b'b']))
    );
}

#[test]
fn typed_date_time() {
    let dt = chrono::Utc.with_ymd_and_hms(2026, 5, 4, 3, 2, 1).unwrap();
    match roundtrip_with_type(&Value::DateTime(dt), Vt::DateTime) {
        Variant::DateTime(b) => assert_eq!(b.as_chrono(), dt),
        other => panic!("expected DateTime variant, got {other:?}"),
    }
    let parsed = roundtrip_with_type(
        &Value::Str("2026-05-04T03:02:01Z".to_string()),
        Vt::DateTime,
    );
    assert!(matches!(parsed, Variant::DateTime(_)));
}

#[test]
fn typed_array_of_int32() {
    let value = Value::Array(vec![Value::I64(1), Value::I64(2), Value::I64(-3)]);
    match roundtrip_with_type(&value, Vt::Int32) {
        Variant::Array(a) => {
            assert_eq!(a.value_type, Vt::Int32);
            assert_eq!(
                a.values,
                vec![Variant::Int32(1), Variant::Int32(2), Variant::Int32(-3)]
            );
        }
        other => panic!("expected Array, got {other:?}"),
    }
}

#[test]
fn typed_array_of_boolean() {
    let value = Value::Array(vec![Value::Bool(true), Value::Bool(false)]);
    match roundtrip_with_type(&value, Vt::Boolean) {
        Variant::Array(a) => assert_eq!(
            a.values,
            vec![Variant::Boolean(true), Variant::Boolean(false)]
        ),
        other => panic!("expected Array, got {other:?}"),
    }
}

#[test]
fn typed_array_range_error_propagates() {
    let value = Value::Array(vec![Value::I64(1), Value::I64(99999)]);
    assert!(value_to_ua_variant(&value, Some(Vt::Byte)).is_err());
}

#[test]
fn unknown_type_infers_from_value() {
    // No data_type -> infer types from the Value variants (legacy behavior).
    assert_eq!(
        value_to_ua_variant(&Value::I64(5), None).unwrap(),
        Variant::Int64(5)
    );
    assert_eq!(
        value_to_ua_variant(&Value::Bool(true), None).unwrap(),
        Variant::Boolean(true)
    );
    assert_eq!(
        value_to_ua_variant(&Value::F64(1.5), None).unwrap(),
        Variant::Double(1.5)
    );
    assert_eq!(
        value_to_ua_variant(&Value::Str("x".to_string()), None).unwrap(),
        Variant::String(UAString::from("x"))
    );
}

#[test]
fn ua_type_name_mapping() {
    assert_eq!(ua_type_name(Vt::Boolean), "Boolean");
    assert_eq!(ua_type_name(Vt::Int32), "Int32");
    assert_eq!(ua_type_name(Vt::UInt64), "UInt64");
    assert_eq!(ua_type_name(Vt::Float), "Float");
    assert_eq!(ua_type_name(Vt::LocalizedText), "LocalizedText");
    assert_eq!(ua_type_name(VariantScalarTypeId::Guid), "Other");
}

fn cmd(kind: CommandKind) -> Command {
    Command {
        request_id: 0,
        session: 1,
        protocol: Protocol::OpcUa,
        kind,
    }
}

#[test]
fn control_command_classification() {
    assert!(!is_control_command(&CommandKind::Read { keys: vec![1] }));
    assert!(!is_control_command(&CommandKind::Test));
    assert!(is_control_command(&CommandKind::Browse {
        node_id: "".into(),
        filter: None,
        view_id: None
    }));
    assert!(is_control_command(&CommandKind::Connect {
        endpoint: EndpointSpec::ua("opc.tcp://x")
    }));
    assert!(is_control_command(&CommandKind::AddItems { items: vec![] }));
    assert!(is_control_command(&CommandKind::Write {
        request: WriteRequest {
            key: 1,
            value: Value::I64(1),
            type_hint: None
        }
    }));
    assert!(is_control_command(&CommandKind::RemoveItems {
        keys: vec![]
    }));
    assert!(is_control_command(&CommandKind::Disconnect));
    assert!(is_control_command(&CommandKind::ConfigureUaSubscription {
        publishing_interval_ms: 1000,
        deadband: 0.0
    }));
}

#[test]
fn deadband_filter_off_by_default() {
    // 0 / negative deadband -> no filter (deliver on every change).
    assert!(data_change_filter(0.0).is_null());
    assert!(data_change_filter(-1.0).is_null());
}

#[test]
fn deadband_filter_builds_absolute() {
    // Off (0 / negative) means no filter -> delivers on every change.
    assert!(data_change_filter(0.0).is_null());
    assert!(data_change_filter(-1.0).is_null());

    // A positive deadband becomes a non-empty absolute DataChangeFilter.
    assert!(
        !data_change_filter(2.5).is_null(),
        "absolute deadband must attach a filter"
    );
}

#[test]
fn parsed_absolute_deadband_matches() {
    // The semantic parser (no EURange needed for absolute) confirms the
    // reader applies an absolute deadband of the requested magnitude.
    let parsed = opcua::types::ParsedDataChangeFilter::parse(
        DataChangeFilter {
            trigger: DataChangeTrigger::StatusValue,
            deadband_type: DeadbandType::Absolute as u32,
            deadband_value: 2.5,
        },
        None,
    )
    .expect("absolute deadband parses without EURange");
    assert!(matches!(
        parsed.deadband,
        opcua::types::Deadband::Absolute(v) if v == 2.5
    ));
}

#[test]
fn subscription_monitoring_parameters_shape() {
    // ItemKey is used directly as the client handle; the request uses a
    // single-slot queue that always discards the oldest value, sampling at
    // the server's fastest rate (0), reporting mode.
    let params = MonitoringParameters {
        client_handle: 42,
        sampling_interval: 0.0,
        filter: data_change_filter(0.0),
        queue_size: 1,
        discard_oldest: true,
    };
    assert_eq!(params.client_handle, 42);
    assert_eq!(params.sampling_interval, 0.0);
    assert_eq!(params.queue_size, 1);
    assert!(params.discard_oldest);
    assert!(params.filter.is_null());
}

#[test]
fn controls_jump_over_reads_without_losing_them() {
    let mut queue = VecDeque::from(vec![
        cmd(CommandKind::Read { keys: vec![1] }),
        cmd(CommandKind::Read { keys: vec![2] }),
        cmd(CommandKind::Browse {
            node_id: "i=85".into(),
            filter: None,
            view_id: None,
        }),
        cmd(CommandKind::Read { keys: vec![3] }),
        cmd(CommandKind::AddItems { items: vec![] }),
    ]);
    let (controls, reads) = partition_commands(&mut queue);
    // Control commands ordered first (in relative order).
    let control_kinds: Vec<&str> = controls
        .iter()
        .map(|c| match &c.kind {
            CommandKind::Browse { .. } => "browse",
            CommandKind::AddItems { .. } => "add",
            _ => "?",
        })
        .collect();
    assert_eq!(control_kinds, vec!["browse", "add"]);
    // Reads are kept too (deferred), so no response is ever lost.
    assert_eq!(reads.len(), 3);
    assert!(
        reads
            .iter()
            .all(|c| matches!(&c.kind, CommandKind::Read { .. }))
    );
    assert!(queue.is_empty());
}

#[test]
fn variant_to_value_maps_new_types() {
    assert_eq!(
        variant_to_value(&Variant::NodeId(Box::new(NodeId::new(2, "MyVar")))),
        Value::NodeId("ns=2;s=MyVar".to_string())
    );
    assert_eq!(
        variant_to_value(&Variant::QualifiedName(Box::new(QualifiedName::new(
            2, "MyVar"
        )))),
        Value::QualifiedName {
            namespace_index: 2,
            name: "MyVar".to_string()
        }
    );
    assert_eq!(
        variant_to_value(&Variant::LocalizedText(Box::new(LocalizedText::new(
            "ru",
            "текст"
        )))),
        Value::LocalizedText {
            locale: "ru".to_string(),
            text: "текст".to_string()
        }
    );
    assert_eq!(
        variant_to_value(&Variant::StatusCode(StatusCode::Bad)),
        Value::Other(format!("0x{:08X}", StatusCode::Bad.bits()))
    );
    // ByteString -> Bytes is preserved.
    assert_eq!(
        variant_to_value(&Variant::ByteString(opcua::types::ByteString::from(vec![
            1, 2, 3
        ]))),
        Value::Bytes(vec![1, 2, 3])
    );
}

#[test]
fn variant_to_value_flattens_multi_dim_array() {
    use opcua::types::Array as UaArray;
    let flat = vec![
        Variant::Int32(1),
        Variant::Int32(2),
        Variant::Int32(3),
        Variant::Int32(4),
    ];
    let array = UaArray {
        value_type: VariantScalarTypeId::Int32,
        values: flat,
        dimensions: Some(vec![2, 2]),
    };
    let value = variant_to_value(&Variant::Array(Box::new(array)));
    match value {
        Value::Array(outer) => {
            assert_eq!(outer.len(), 2);
            match &outer[0] {
                Value::Array(inner) => {
                    assert_eq!(inner, &vec![Value::I64(1), Value::I64(2)])
                }
                other => panic!("expected inner array, got {other:?}"),
            }
        }
        other => panic!("expected array, got {other:?}"),
    }
}

#[test]
fn typed_write_supports_new_scalar_types() {
    assert_eq!(
        value_to_ua_variant(
            &Value::NodeId("ns=2;s=MyVar".to_string()),
            Some(VariantScalarTypeId::NodeId)
        )
        .unwrap(),
        Variant::NodeId(Box::new(NodeId::new(2, "MyVar")))
    );
    assert!(matches!(
        value_to_ua_variant(
            &Value::NodeId("not-a-node".to_string()),
            Some(VariantScalarTypeId::NodeId)
        ),
        Err(_)
    ));
    assert_eq!(
        value_to_ua_variant(
            &Value::QualifiedName {
                namespace_index: 2,
                name: "MyVar".to_string()
            },
            Some(VariantScalarTypeId::QualifiedName),
        )
        .unwrap(),
        Variant::QualifiedName(Box::new(QualifiedName::new(2, "MyVar")))
    );
    assert_eq!(
        value_to_ua_variant(
            &Value::LocalizedText {
                locale: "en".to_string(),
                text: "hi".to_string()
            },
            Some(VariantScalarTypeId::LocalizedText),
        )
        .unwrap(),
        Variant::LocalizedText(Box::new(LocalizedText::new("en", "hi")))
    );
    // Structured/encoded types are rejected with a clear error.
    assert!(
        value_to_ua_variant(
            &Value::Str("x".to_string()),
            Some(VariantScalarTypeId::ExtensionObject)
        )
        .is_err()
    );
}

#[test]
fn infer_write_supports_new_value_types() {
    assert_eq!(
        value_to_ua_variant(&Value::NodeId("i=85".to_string()), None).unwrap(),
        Variant::NodeId(Box::new(NodeId::new(0, 85u32)))
    );
    assert_eq!(
        value_to_ua_variant(
            &Value::QualifiedName {
                namespace_index: 3,
                name: "N".to_string()
            },
            None
        )
        .unwrap(),
        Variant::QualifiedName(Box::new(QualifiedName::new(3, "N")))
    );
    assert!(match value_to_ua_variant(
        &Value::LocalizedText {
            locale: "ru".to_string(),
            text: "привет".to_string()
        },
        None,
    )
    .unwrap()
    {
        Variant::LocalizedText(l) => {
            l.locale.to_string() == "ru" && l.text.to_string() == "привет"
        }
        _ => false,
    });
}

#[test]
fn nested_array_write_is_rejected() {
    let value = Value::Array(vec![Value::Array(vec![Value::I64(1)])]);
    assert!(value_to_ua_variant(&value, Some(VariantScalarTypeId::Int32)).is_err());
    assert!(value_to_ua_variant(&value, None).is_err());
}

#[test]
fn relative_path_parsing() {
    let elements = parse_relative_path("0:Objects/2:MyVar").unwrap();
    assert_eq!(elements.len(), 2);
    assert_eq!(elements[0].target_name.namespace_index, 0);
    assert_eq!(elements[0].target_name.name.to_string(), "Objects");
    assert_eq!(elements[1].target_name.namespace_index, 2);
    assert_eq!(elements[1].target_name.name.to_string(), "MyVar");
    assert!(parse_relative_path("").is_err());
    assert!(parse_relative_path("Objects/").is_ok());
}

fn ep(url: &str, policy: &str, mode: &str) -> UaEndpointInfo {
    UaEndpointInfo {
        url: url.to_string(),
        security_policy: policy.to_string(),
        security_mode: mode.to_string(),
    }
}

fn eps() -> Vec<UaEndpointInfo> {
    vec![
        ep("opc.tcp://h:4840", "Basic256Sha256", "SignAndEncrypt"),
        ep("opc.tcp://h:4840", "Basic256Sha256", "Sign"),
        ep(
            "opc.tcp://h:4840",
            "http://opcfoundation.org/UA/SecurityPolicy#None",
            "None",
        ),
    ]
}

#[test]
fn auto_prefers_none_then_basic256_sha256() {
    // Auto/Auto with a None endpoint present -> picks None first.
    let (uri, mode) = resolve_security(SECURITY_POLICY_AUTO, SECURITY_MODE_AUTO, &eps()).unwrap();
    assert!(
        uri.ends_with("#None"),
        "Auto must prefer None policy, got {uri}"
    );
    assert_eq!(mode, MessageSecurityMode::None);

    // Without a None endpoint -> falls to Basic256Sha256 Sign (preferred over
    // SignAndEncrypt).
    let no_none: Vec<UaEndpointInfo> = eps()
        .into_iter()
        .filter(|e| !e.security_policy.ends_with("#None"))
        .collect();
    let (uri, mode) = resolve_security(SECURITY_POLICY_AUTO, SECURITY_MODE_AUTO, &no_none).unwrap();
    assert!(uri.ends_with("#Basic256Sha256"), "got {uri}");
    assert_eq!(mode, MessageSecurityMode::Sign);
}

#[test]
fn auto_sign_mode_matches_over_encrypt() {
    // Auto policy + Sign mode: only the Sign endpoint matches.
    let items: Vec<UaEndpointInfo> = eps()
        .into_iter()
        .filter(|e| e.security_mode == "Sign")
        .collect();
    let (uri, mode) = resolve_security(SECURITY_POLICY_AUTO, SECURITY_MODE_SIGN, &items).unwrap();
    assert_eq!(mode, MessageSecurityMode::Sign);
    assert!(uri.ends_with("#Basic256Sha256"));
}

#[test]
fn explicit_policy_and_mode_are_honored() {
    let (uri, mode) = resolve_security(
        SECURITY_POLICY_BASIC256_SHA256,
        SECURITY_MODE_SIGN_AND_ENCRYPT,
        &eps(),
    )
    .unwrap();
    assert!(uri.ends_with("#Basic256Sha256"));
    assert_eq!(mode, MessageSecurityMode::SignAndEncrypt);
}

#[test]
fn explicit_unsupported_choice_is_an_error_not_a_fallback() {
    // The server offers None/None, Sign, SignAndEncrypt only for Basic256.
    // Basic256 + Sign must not silently resolve to anything else.
    let only_none = vec![ep("opc.tcp://h:4840", "None", "None")];
    let err =
        resolve_security(UaSecurityPolicy::Basic256, UaSecurityMode::Sign, &only_none).unwrap_err();
    assert!(err.contains("not offered"), "got: {err}");
    assert!(
        err.contains("None/None"),
        "detail must list advertised endpoints: {err}"
    );
}

#[test]
fn explicit_mode_not_offered_is_an_error() {
    let only_none = vec![ep("opc.tcp://h:4840", "None", "None")];
    let err = resolve_security(
        SECURITY_POLICY_AUTO,
        UaSecurityMode::SignAndEncrypt,
        &only_none,
    )
    .unwrap_err();
    assert!(
        err.contains("mode SignAndEncrypt is not offered"),
        "got: {err}"
    );
}

#[test]
fn explicit_policy_not_offered_is_an_error() {
    let only_none = vec![ep("opc.tcp://h:4840", "None", "None")];
    let err = resolve_security(
        UaSecurityPolicy::Basic256Sha256,
        SECURITY_MODE_AUTO,
        &only_none,
    )
    .unwrap_err();
    assert!(
        err.contains("policy Basic256Sha256 is not offered"),
        "got: {err}"
    );
}

#[test]
fn invalid_policy_mode_combination_is_an_error() {
    let err =
        resolve_security(SECURITY_POLICY_NONE, SECURITY_MODE_SIGN_AND_ENCRYPT, &eps()).unwrap_err();
    assert!(err.contains("policy None"), "got: {err}");
    assert!(err.contains("requires mode None"), "got: {err}");
}

#[test]
fn empty_endpoint_list_is_an_error() {
    let err = resolve_security(SECURITY_POLICY_AUTO, SECURITY_MODE_AUTO, &[]).unwrap_err();
    assert!(err.contains("no security endpoints"), "got: {err}");
}

#[test]
fn unknown_advertised_policy_is_an_error_not_none() {
    // A server policy outside the typed class must never collapse to None.
    let unknown = vec![ep(
        "opc.tcp://h:4840",
        "http://example.org/SecurityPolicy#Custom",
        "Sign",
    )];
    let err = resolve_security(SECURITY_POLICY_AUTO, SECURITY_MODE_AUTO, &unknown).unwrap_err();
    assert!(
        err.contains("recognized security policy/mode"),
        "got: {err}"
    );
}

#[test]
fn concrete_mode_with_auto_policy_picks_first_matching() {
    let only_encrypt: Vec<UaEndpointInfo> = eps()
        .into_iter()
        .filter(|e| e.security_mode == "SignAndEncrypt")
        .collect();
    let (uri, mode) = resolve_security(
        SECURITY_POLICY_AUTO,
        SECURITY_MODE_SIGN_AND_ENCRYPT,
        &only_encrypt,
    )
    .unwrap();
    assert_eq!(mode, MessageSecurityMode::SignAndEncrypt);
    assert!(uri.ends_with("#Basic256Sha256"), "got {uri}");
}

#[test]
fn security_error_prefix_roundtrips_for_the_ui_wrapper() {
    let message = security_error("policy Basic256 / mode Sign is not offered");
    assert_eq!(
        security_error_detail(&message),
        Some("policy Basic256 / mode Sign is not offered")
    );
    assert_eq!(security_error_detail("ordinary failure"), None);
}

fn network_info(name: &str, uri: &str, urls: &[&str]) -> UaNetworkInfo {
    UaNetworkInfo {
        application_name: name.to_string(),
        application_uri: uri.to_string(),
        discovery_urls: urls.iter().map(|url| url.to_string()).collect(),
    }
}

#[test]
fn network_results_merge_by_application_uri_and_union_urls() {
    let mut found = vec![network_info(
        "LDS",
        "urn:example:server",
        &["opc.tcp://10.0.0.5:4840"],
    )];
    merge_network_info(
        &mut found,
        network_info(
            "mDNS",
            "urn:example:server",
            &["opc.tcp://10.0.0.5:4840", "opc.tcp://[fe80::5]:4840"],
        ),
    );
    assert_eq!(found.len(), 1);
    assert_eq!(
        found[0].discovery_urls,
        vec![
            "opc.tcp://10.0.0.5:4840".to_string(),
            "opc.tcp://[fe80::5]:4840".to_string()
        ]
    );
}

#[test]
fn network_results_merge_by_discovery_url_when_uri_is_missing() {
    let mut found = vec![network_info("LDS", "", &["opc.tcp://host:4840/"])];
    merge_network_info(
        &mut found,
        network_info("mDNS", "", &["opc.tcp://HOST:4840"]),
    );
    assert_eq!(found.len(), 1);
    assert_eq!(found[0].discovery_urls, vec!["opc.tcp://host:4840/"]);
}

#[test]
fn network_results_keep_distinct_servers() {
    let mut found = vec![network_info(
        "A",
        "urn:example:a",
        &["opc.tcp://10.0.0.5:4840"],
    )];
    merge_network_info(
        &mut found,
        network_info("B", "urn:example:b", &["opc.tcp://10.0.0.6:4840"]),
    );
    assert_eq!(found.len(), 2);
}

fn scan_binding(local: &str) -> NetworkBinding {
    NetworkBinding::new(local.parse().unwrap(), Some(7), Some(8), None)
}

#[test]
fn scan_targets_are_loopback_first_and_deduplicated() {
    let binding = scan_binding("192.168.1.10");
    let subnet = SubnetSpec::new("192.168.1.10".parse::<IpAddr>().unwrap(), 30).unwrap();
    let targets = scan_targets(
        &binding,
        Some(subnet),
        "192.168.1.10".parse::<IpAddr>().unwrap(),
    );
    let ips: Vec<IpAddr> = targets.iter().map(|target| target.ip).collect();
    assert_eq!(
        ips,
        vec![
            "127.0.0.1".parse::<IpAddr>().unwrap(),
            "192.168.1.10".parse::<IpAddr>().unwrap(),
            "192.168.1.9".parse::<IpAddr>().unwrap(),
        ]
    );
    assert!(targets[0].binding.is_loopback());
    assert!(!targets[1].binding.is_loopback());
    assert_eq!(
        targets
            .iter()
            .filter(|t| t.ip == "192.168.1.10".parse::<IpAddr>().unwrap())
            .count(),
        1
    );
}

#[test]
fn scan_targets_include_ipv6_loopback_only_for_ipv6() {
    let v4 = scan_binding("10.0.0.5");
    let v4_targets = scan_targets(&v4, None, "10.0.0.5".parse::<IpAddr>().unwrap());
    assert_eq!(v4_targets[0].ip, "127.0.0.1".parse::<IpAddr>().unwrap());
    assert!(!v4_targets.iter().any(|target| target.ip.is_ipv6()));

    let v6 = scan_binding("2001:db8::5");
    let v6_targets = scan_targets(&v6, None, "2001:db8::5".parse::<IpAddr>().unwrap());
    assert_eq!(v6_targets[0].ip, "127.0.0.1".parse::<IpAddr>().unwrap());
    assert_eq!(v6_targets[1].ip, "::1".parse::<IpAddr>().unwrap());
    assert_eq!(v6_targets[2].ip, "2001:db8::5".parse::<IpAddr>().unwrap());
    assert!(v6_targets[1].binding.is_loopback());
}

#[test]
fn scan_targets_follow_ipv6_subnet_family() {
    let binding = scan_binding("10.0.0.5");
    let subnet = SubnetSpec::new("2001:db8::5".parse::<IpAddr>().unwrap(), 126).unwrap();
    let targets = scan_targets(
        &binding,
        Some(subnet),
        "10.0.0.5".parse::<IpAddr>().unwrap(),
    );
    assert_eq!(targets[0].ip, "127.0.0.1".parse::<IpAddr>().unwrap());
    assert_eq!(targets[1].ip, "::1".parse::<IpAddr>().unwrap());
    assert!(
        targets
            .iter()
            .any(|target| target.ip == "2001:db8::5".parse::<IpAddr>().unwrap())
    );
}

#[test]
fn over_large_subnet_is_rejected_with_the_error_token() {
    let subnet = SubnetSpec::new("10.0.0.0".parse().unwrap(), 19).unwrap();
    assert!(subnet.usable_count() > SCAN_CONFIRM_THRESHOLD);
    let (tx, rx) = std::sync::mpsc::channel();
    let runtime = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .unwrap();
    runtime.block_on(run_scan_job(
        9,
        NetworkBinding::loopback(),
        Some(subnet),
        "127.0.0.1".parse::<IpAddr>().unwrap(),
        DEFAULT_UA_PORT,
        Arc::new(AtomicBool::new(false)),
        &tx,
    ));
    drop(tx);
    let events: Vec<EventKind> = rx.try_iter().map(|event| event.kind).collect();
    assert_eq!(events.len(), 1);
    match &events[0] {
        EventKind::UaScanFinished {
            scan_id,
            cancelled,
            found,
            error,
        } => {
            assert_eq!(*scan_id, 9);
            assert!(!cancelled);
            assert_eq!(*found, 0);
            let error = error.as_deref().expect("error token");
            assert!(
                error.starts_with(SCAN_SUBNET_TOO_LARGE),
                "unexpected error: {error}"
            );
        }
        other => panic!("unexpected event: {other:?}"),
    }
}

#[test]
fn cancelled_scan_finishes_without_checking_addresses() {
    let (tx, rx) = std::sync::mpsc::channel();
    let cancel = Arc::new(AtomicBool::new(true));
    let runtime = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .unwrap();
    runtime.block_on(run_scan_job(
        10,
        NetworkBinding::loopback(),
        None,
        "127.0.0.1".parse::<IpAddr>().unwrap(),
        DEFAULT_UA_PORT,
        cancel,
        &tx,
    ));
    drop(tx);
    let events: Vec<EventKind> = rx.try_iter().map(|event| event.kind).collect();
    assert!(matches!(
        events.first(),
        Some(EventKind::UaScanProgress {
            scan_id: 10,
            total: 1,
            checked: 0,
            found: 0
        })
    ));
    assert!(matches!(
        events.last(),
        Some(EventKind::UaScanFinished {
            scan_id: 10,
            cancelled: true,
            found: 0,
            error: None
        })
    ));
}

#[test]
fn loopback_tcp_without_opc_ua_is_not_a_scan_result() {
    let runtime = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .unwrap();
    runtime.block_on(async {
        let listener = tokio::net::TcpListener::bind((std::net::Ipv4Addr::LOCALHOST, 0))
            .await
            .unwrap();
        let port = listener.local_addr().unwrap().port();
        let server = tokio::spawn(async move {
            // Accept the TCP precheck and close immediately: open port, no UA.
            if let Ok((stream, _)) = listener.accept().await {
                drop(stream);
            }
        });
        let (tx, rx) = std::sync::mpsc::channel();
        tokio::time::timeout(
            Duration::from_secs(15),
            run_scan_job(
                11,
                NetworkBinding::loopback(),
                None,
                "127.0.0.1".parse::<IpAddr>().unwrap(),
                port,
                Arc::new(AtomicBool::new(false)),
                &tx,
            ),
        )
        .await
        .expect("scan must finish");
        drop(tx);
        let _ = server.await;
        let events: Vec<EventKind> = rx.try_iter().map(|event| event.kind).collect();
        assert!(matches!(
            events.first(),
            Some(EventKind::UaScanProgress {
                scan_id: 11,
                total: 1,
                checked: 0,
                found: 0
            })
        ));
        assert!(matches!(
            events.last(),
            Some(EventKind::UaScanFinished {
                scan_id: 11,
                cancelled: false,
                found: 0,
                error: None
            })
        ));
        assert!(
            !events
                .iter()
                .any(|event| matches!(event, EventKind::UaScanFound { .. })),
            "an open TCP port without OPC UA must not be reported"
        );
    });
}

#[test]
fn scan_server_keys_prefer_application_uri_then_url() {
    let by_uri = UaServerInfo {
        application_name: "A".to_string(),
        application_uri: "urn:Example:A".to_string(),
        endpoints: vec![UaEndpointInfo {
            url: "opc.tcp://10.0.0.5:4840".to_string(),
            security_policy: String::new(),
            security_mode: String::new(),
        }],
    };
    assert_eq!(scan_server_key(&by_uri), "uri:urn:example:a");

    let by_url = UaServerInfo {
        application_uri: String::new(),
        ..by_uri
    };
    assert_eq!(scan_server_key(&by_url), "url:opc.tcp://10.0.0.5:4840");
}

#[test]
fn scanner_unions_endpoints_for_the_same_application_uri() {
    let endpoint = |url: &str| UaEndpointInfo {
        url: url.to_string(),
        security_policy: "None".to_string(),
        security_mode: "None".to_string(),
    };
    let first = UaServerInfo {
        application_name: "CODESYS".to_string(),
        application_uri: "urn:CODESYS".to_string(),
        endpoints: vec![endpoint("opc.tcp://127.0.0.1:4840")],
    };
    let second = UaServerInfo {
        application_name: "CODESYS".to_string(),
        application_uri: "urn:CODESYS".to_string(),
        endpoints: vec![
            endpoint("opc.tcp://127.0.0.1:4840"),
            endpoint("opc.tcp://NTBK700582:4840"),
        ],
    };
    let mut seen = std::collections::HashMap::new();
    let (fresh, new_servers) = merge_scan_batch(&mut seen, vec![first]);
    assert_eq!(new_servers, 1);
    assert_eq!(fresh.len(), 1);
    let (fresh, new_servers) = merge_scan_batch(&mut seen, vec![second]);
    assert_eq!(new_servers, 0, "the same ApplicationUri is one server");
    assert_eq!(fresh.len(), 1);
    assert_eq!(fresh[0].endpoints.len(), 1);
    assert_eq!(fresh[0].endpoints[0].url, "opc.tcp://NTBK700582:4840");
    let merged = seen.values().next().unwrap();
    assert_eq!(merged.endpoints.len(), 2);
}
