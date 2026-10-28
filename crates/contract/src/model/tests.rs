use super::*;

#[test]
fn session_ids_are_nonzero_and_do_not_wrap() {
    assert_eq!(SessionId::new(0), None);
    assert_eq!(SessionId::FIRST.get(), 1);
    assert_eq!(SessionId::FIRST.successor().unwrap().get(), 2);
    assert_eq!(SessionId::new(u64::MAX).unwrap().successor(), None);
}

#[test]
fn synthetic_views_root_is_identified_by_reserved_id() {
    let mut node = BrowseNode {
        node_id: BrowseNode::VIEWS_ROOT_ID.to_owned(),
        name: String::new(),
        kind: NodeKind::Branch,
        has_children: true,
    };
    assert!(node.is_views_root());
    node.node_id = "ns=1;s=views".to_owned();
    assert!(!node.is_views_root());
}

#[test]
fn operation_codes_are_ascii_and_cover_the_emitters() {
    // Typed operations are compared by enum (not localized text), so their
    // log tokens must never contain non-ASCII and must be unique.
    let mut seen = std::collections::HashSet::new();
    for op in Operation::ALL {
        let code = op.as_str();
        assert!(code.is_ascii(), "operation code must be ASCII: {op:?}");
        assert!(!code.is_empty());
        assert!(seen.insert(code), "duplicate operation code: {code}");
    }
    // The tokens are the exact legacy values the journal/logs used.
    assert_eq!(Operation::SearchServers.as_str(), "search_servers");
    assert_eq!(Operation::SearchNetwork.as_str(), "search_network");
    assert_eq!(Operation::Connect.as_str(), "connect");
    assert_eq!(Operation::BrowseViews.as_str(), "views");
    assert_eq!(Operation::Subscription.as_str(), "subscription");
    assert_eq!(Operation::Add.as_str(), "add");
    assert_eq!(Operation::Remove.as_str(), "remove");
}

#[test]
fn command_kind_maps_to_its_operation() {
    // Pins the command -> operation mapping used to label dispatch failures.
    assert_eq!(
        CommandKind::Connect {
            endpoint: EndpointSpec::ua("opc.tcp://x:4840")
        }
        .operation(),
        Operation::Connect
    );
    assert_eq!(
        CommandKind::ConfigureUaSubscription {
            publishing_interval_ms: 1000,
            deadband: 0.0
        }
        .operation(),
        Operation::Subscription
    );
    assert_eq!(
        CommandKind::AddItems { items: Vec::new() }.operation(),
        Operation::AddItems
    );
    assert_eq!(
        CommandKind::RemoveItems { keys: Vec::new() }.operation(),
        Operation::RemoveItems
    );
    assert_eq!(CommandKind::Test.operation(), Operation::Test);
    assert_eq!(
        CommandKind::ScanUaNetwork {
            scan_id: ScanId::FIRST,
            bind: crate::net::NetworkBinding::loopback(),
            subnet: None,
            local_ip: std::net::IpAddr::V4(std::net::Ipv4Addr::LOCALHOST),
        }
        .operation(),
        Operation::SearchNetwork
    );
    assert_eq!(
        CommandKind::CancelUaScan {
            scan_id: ScanId::FIRST
        }
        .operation(),
        Operation::SearchNetwork
    );
}

#[test]
fn endpoint_display_name_serde_backward_compatible() {
    // Old JSON (no display_name) must deserialize to `None` and load fine.
    let old = r#"{"protocol":"OpcDa","host":"box1","address":"Alpha.Server","user":"","password":"","ua_trust_server_cert":false}"#;
    let ep: EndpointSpec = serde_json::from_str(old).unwrap();
    assert!(ep.display_name.is_none());
    // And it round-trips without the field (skipped when None).
    let json = serde_json::to_string(&ep).unwrap();
    assert!(!json.contains("display_name"), "got: {json}");
    // New JSON carries the advertised name.
    let ep2 =
        EndpointSpec::ua("opc.tcp://x:4840").with_advertised_name(Some("My Server".to_string()));
    let json = serde_json::to_string(&ep2).unwrap();
    assert!(json.contains("display_name") && json.contains("My Server"));
    let back: EndpointSpec = serde_json::from_str(&json).unwrap();
    assert_eq!(back.display_name.as_deref(), Some("My Server"));
}

#[test]
fn endpoint_ua_binding_is_runtime_only_and_not_persisted() {
    let mut ep = EndpointSpec::ua("opc.tcp://192.168.1.50:4840");
    ep.ua_bind = Some(crate::net::NetworkBinding::new(
        "192.168.1.20".parse().unwrap(),
        std::num::NonZeroU32::new(7),
        std::num::NonZeroU32::new(9),
        None,
    ));
    let json = serde_json::to_string(&ep).unwrap();
    assert!(!json.contains("ua_bind"), "got: {json}");
    let back: EndpointSpec = serde_json::from_str(&json).unwrap();
    assert!(back.ua_bind.is_none());

    let da = EndpointSpec::da("", "A.Server");
    assert!(da.ua_bind.is_none());
}

#[test]
fn endpoint_display_name_composition() {
    let ep = EndpointSpec::ua("opc.tcp://127.0.0.1:4840")
        .with_advertised_name(Some("Prosys OPC UA".to_string()));
    assert_eq!(ep.display_name.as_deref(), Some("Prosys OPC UA"));
    // Blank advertised names are dropped (None -> fallback to address).
    let ep2 = EndpointSpec::da("", "Alpha.Server").with_advertised_name(Some("   ".to_string()));
    assert!(ep2.display_name.is_none());
}

#[test]
fn endpoint_constructors_keep_no_advertised_name() {
    assert!(EndpointSpec::da("", "A.Server").display_name.is_none());
    assert!(EndpointSpec::ua("opc.tcp://x:4840").display_name.is_none());
}

#[test]
fn quality_from_ua_severity_mapping() {
    assert_eq!(Quality::from_ua(0x0000, "Good").kind, QualityKind::Good);
    assert_eq!(
        Quality::from_ua(0x4000_0000, "Uncertain").kind,
        QualityKind::Uncertain
    );
    assert_eq!(Quality::from_ua(0x8000_0000, "Bad").kind, QualityKind::Bad);
    assert_eq!(Quality::from_ua(0xC000_0001, "Bad").kind, QualityKind::Bad);
    assert_eq!(Quality::from_ua(0x8000_0000, "x").code >> 30, 0b10);
}

#[test]
fn value_as_f64() {
    assert_eq!(Value::I64(42).as_f64(), Some(42.0));
    assert_eq!(Value::U64(7).as_f64(), Some(7.0));
    assert_eq!(Value::F64(3.5).as_f64(), Some(3.5));
    assert_eq!(Value::Bool(true).as_f64(), Some(1.0));
    assert_eq!(Value::Bool(false).as_f64(), Some(0.0));
    assert_eq!(Value::Str("12.5".to_string()).as_f64(), Some(12.5));
    assert_eq!(Value::Str(" 4 ".to_string()).as_f64(), Some(4.0));
    assert_eq!(Value::Str("abc".to_string()).as_f64(), None);
    assert_eq!(Value::Str("".to_string()).as_f64(), None);
    assert_eq!(Value::Empty.as_f64(), None);
}

#[test]
fn value_format() {
    assert_eq!(format!("{}", Value::Bool(false)), "false");
    assert_eq!(format!("{}", Value::I64(-5)), "-5");
    assert_eq!(format!("{}", Value::U64(9)), "9");
    assert_eq!(format!("{}", Value::F64(1.5)), "1.5");
    assert_eq!(format!("{}", Value::F64(f64::NAN)), "NaN");
    assert_eq!(format!("{}", Value::F64(f64::INFINITY)), "inf");
    assert_eq!(format!("{}", Value::Str("hi".to_string())), "hi");
    assert_eq!(format!("{}", Value::Empty), "");
}

#[test]
fn datetime_format() {
    let dt = chrono::TimeZone::with_ymd_and_hms(&chrono::Utc, 2024, 7, 2, 3, 4, 5).unwrap();
    let s = format!("{}", Value::DateTime(dt));
    assert!(s.contains("2024"), "unexpected datetime format: {s}");
}

#[test]
fn node_id_format() {
    assert_eq!(
        format!("{}", Value::NodeId("i=85".to_string())),
        "NodeId(i=85)"
    );
    assert_eq!(
        format!("{}", Value::NodeId("ns=2;s=MyVar".to_string())),
        "NodeId(ns=2;s=MyVar)"
    );
}

#[test]
fn qualified_name_format() {
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
}

#[test]
fn localized_text_format() {
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
                locale: "en".to_string(),
                text: "hi".to_string()
            }
        ),
        "[en] hi"
    );
}

#[test]
fn bytes_format_is_hex_with_truncation() {
    assert_eq!(
        format!("{}", Value::Bytes(vec![0xAA, 0x01, 0xFF])),
        "AA 01 FF"
    );
    let long: Vec<u8> = (0..40).collect();
    let s = format!("{}", Value::Bytes(long));
    assert!(
        s.starts_with("00 01 02 03 04 05 06 07 08 09 0A 0B 0C 0D 0E 0F"),
        "got: {s}"
    );
    assert!(s.ends_with('…'), "truncation marker expected: {s}");
    assert!(!s.contains("40"), "bytes beyond 16 must be hidden: {s}");
}

#[test]
fn array_format_truncates_after_16() {
    let short = Value::Array(vec![Value::I64(1), Value::I64(2)]);
    assert_eq!(format!("{short}"), "[1, 2]");
    let long = Value::Array((0..20).map(Value::I64).collect());
    let s = format!("{long}");
    assert!(
        s.starts_with("[0, 1, 2, 3, 4, 5, 6, 7, 8, 9, 10, 11, 12, 13, 14, 15"),
        "got: {s}"
    );
    assert!(s.ends_with("…(20)]"), "got: {s}");
}

#[test]
fn new_value_variants_as_f64() {
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
    assert_eq!(Value::NodeId("i=5".to_string()).as_f64(), None);
    assert_eq!(
        Value::QualifiedName {
            namespace_index: 0,
            name: "x".to_string()
        }
        .as_f64(),
        None
    );
}

#[test]
fn security_enums_serde_roundtrip_and_stable_tokens() {
    for policy in UaSecurityPolicy::ALL {
        let json = serde_json::to_string(&policy).unwrap();
        assert_eq!(json, format!("\"{}\"", policy.as_str()));
        let back: UaSecurityPolicy = serde_json::from_str(&json).unwrap();
        assert_eq!(back, policy);
        assert!(policy.as_str().is_ascii());
    }
    for mode in UaSecurityMode::ALL {
        let json = serde_json::to_string(&mode).unwrap();
        assert_eq!(json, format!("\"{}\"", mode.as_str()));
        let back: UaSecurityMode = serde_json::from_str(&json).unwrap();
        assert_eq!(back, mode);
        assert!(mode.as_str().is_ascii());
    }
    // Tokens are unique (guard against two variants serializing alike).
    let mut seen = std::collections::HashSet::new();
    for policy in UaSecurityPolicy::ALL {
        assert!(seen.insert(policy.as_str()), "duplicate policy token");
    }
    let mut seen = std::collections::HashSet::new();
    for mode in UaSecurityMode::ALL {
        assert!(seen.insert(mode.as_str()), "duplicate mode token");
    }
}

#[test]
fn security_legacy_strings_deserialize() {
    // The spellings the old UI could persist.
    for (raw, expected) in [
        ("\"Авто\"", UaSecurityPolicy::Auto),
        ("\"Auto\"", UaSecurityPolicy::Auto),
        ("\"auto\"", UaSecurityPolicy::Auto),
        ("\"None\"", UaSecurityPolicy::None),
        ("\"Basic128Rsa15\"", UaSecurityPolicy::Basic128Rsa15),
        ("\"Basic256\"", UaSecurityPolicy::Basic256),
        ("\"Basic256Sha256\"", UaSecurityPolicy::Basic256Sha256),
        (
            "\"http://opcfoundation.org/UA/SecurityPolicy#Basic256Sha256\"",
            UaSecurityPolicy::Basic256Sha256,
        ),
        (
            "\"http://opcfoundation.org/UA/SecurityPolicy#None\"",
            UaSecurityPolicy::None,
        ),
        ("\"\"", UaSecurityPolicy::Auto),
    ] {
        let got: UaSecurityPolicy = serde_json::from_str(raw).unwrap();
        assert_eq!(got, expected, "legacy policy {raw}");
    }
    for (raw, expected) in [
        ("\"Авто\"", UaSecurityMode::Auto),
        ("\"Auto\"", UaSecurityMode::Auto),
        ("\"None\"", UaSecurityMode::None),
        ("\"Sign\"", UaSecurityMode::Sign),
        ("\"SignAndEncrypt\"", UaSecurityMode::SignAndEncrypt),
        ("\"\"", UaSecurityMode::Auto),
    ] {
        let got: UaSecurityMode = serde_json::from_str(raw).unwrap();
        assert_eq!(got, expected, "legacy mode {raw}");
    }
}

#[test]
fn security_unknown_strings_are_errors_not_auto() {
    for raw in [
        "\"Aes512Sha999Hypothetical\"",
        "\"http://example.org/SecurityPolicy#Custom\"",
        "\"garbage\"",
    ] {
        let err = serde_json::from_str::<UaSecurityPolicy>(raw).unwrap_err();
        assert!(
            err.to_string().contains("unknown OPC UA security policy"),
            "unexpected error for {raw}: {err}"
        );
    }
    for raw in ["\"Invalid\"", "\"SignAndEncryption\"", "\"garbage\""] {
        let err = serde_json::from_str::<UaSecurityMode>(raw).unwrap_err();
        assert!(
            err.to_string().contains("unknown OPC UA security mode"),
            "unexpected error for {raw}: {err}"
        );
    }
}

#[test]
fn aes_policies_round_trip_through_every_spelling() {
    let aes128 = "http://opcfoundation.org/UA/SecurityPolicy#Aes128_Sha256_RsaOaep";
    let aes256 = "http://opcfoundation.org/UA/SecurityPolicy#Aes256_Sha256_RsaPss";
    assert_eq!(UaSecurityPolicy::Aes128Sha256RsaOaep.as_uri(), Some(aes128));
    assert_eq!(UaSecurityPolicy::Aes256Sha256RsaPss.as_uri(), Some(aes256));
    for (policy, uri) in [
        (UaSecurityPolicy::Aes128Sha256RsaOaep, aes128),
        (UaSecurityPolicy::Aes256Sha256RsaPss, aes256),
    ] {
        // What the server advertises (URI) and what the settings stored
        // (token) both come back as the same typed value.
        assert_eq!(UaSecurityPolicy::from_advertised(uri), Some(policy));
        assert_eq!(UaSecurityPolicy::from_stored(policy.as_str()), Some(policy));
        let json = serde_json::to_string(&policy).unwrap();
        assert_eq!(
            serde_json::from_str::<UaSecurityPolicy>(&json).unwrap(),
            policy
        );
        assert!(UaSecurityPolicy::ALL.contains(&policy));
        assert!(validate_security_pair(policy, UaSecurityMode::SignAndEncrypt).is_ok());
        assert!(validate_security_pair(policy, UaSecurityMode::Sign).is_ok());
    }
    // The spelling without separators is accepted too.
    assert_eq!(
        UaSecurityPolicy::from_stored("Aes256Sha256RsaPss"),
        Some(UaSecurityPolicy::Aes256Sha256RsaPss)
    );
}

#[test]
fn security_policy_uri_mapping() {
    assert_eq!(UaSecurityPolicy::Auto.as_uri(), None);
    assert_eq!(
        UaSecurityPolicy::None.as_uri(),
        Some("http://opcfoundation.org/UA/SecurityPolicy#None")
    );
    assert_eq!(
        UaSecurityPolicy::Basic128Rsa15.as_uri(),
        Some("http://opcfoundation.org/UA/SecurityPolicy#Basic128Rsa15")
    );
    assert_eq!(
        UaSecurityPolicy::Basic256.as_uri(),
        Some("http://opcfoundation.org/UA/SecurityPolicy#Basic256")
    );
    assert_eq!(
        UaSecurityPolicy::Basic256Sha256.as_uri(),
        Some("http://opcfoundation.org/UA/SecurityPolicy#Basic256Sha256")
    );
    // Advertised full URIs and short names both normalize to the enum.
    for policy in UaSecurityPolicy::ALL
        .into_iter()
        .filter(|p| *p != UaSecurityPolicy::Auto)
    {
        assert_eq!(
            UaSecurityPolicy::from_advertised(policy.as_uri().unwrap()),
            Some(policy)
        );
        assert_eq!(
            UaSecurityPolicy::from_advertised(policy.as_str()),
            Some(policy)
        );
    }
}

#[test]
fn security_pair_compatibility() {
    use UaSecurityMode::{None as ModeNone, Sign, SignAndEncrypt};
    use UaSecurityPolicy::{Auto, Basic256Sha256, None as PolicyNone};
    // None policy requires None mode; Auto is on the policy axis.
    assert!(validate_security_pair(PolicyNone, ModeNone).is_ok());
    assert!(validate_security_pair(PolicyNone, Sign).is_err());
    assert!(validate_security_pair(PolicyNone, SignAndEncrypt).is_err());
    // Auto policy is decided against the server.
    assert!(validate_security_pair(Auto, ModeNone).is_ok());
    assert!(validate_security_pair(Auto, Sign).is_ok());
    assert!(validate_security_pair(Auto, SignAndEncrypt).is_ok());
    // Concrete policy with a protecting mode.
    assert!(validate_security_pair(Basic256Sha256, Sign).is_ok());
    assert!(validate_security_pair(Basic256Sha256, SignAndEncrypt).is_ok());
    assert!(validate_security_pair(Basic256Sha256, ModeNone).is_ok());
}

#[test]
fn endpoint_security_legacy_json_migrates_to_enums() {
    let legacy = r#"{
            "protocol":"OpcUa",
            "host":"",
            "address":"opc.tcp://127.0.0.1:4840",
            "user":"",
            "ua_security_policy":"Авто",
            "ua_security_mode":"Sign"
        }"#;
    let ep: EndpointSpec = serde_json::from_str(legacy).unwrap();
    assert_eq!(ep.ua_security_policy, UaSecurityPolicy::Auto);
    assert_eq!(ep.ua_security_mode, UaSecurityMode::Sign);
    // New JSON carries stable ASCII tokens.
    let json = serde_json::to_string(&ep).unwrap();
    assert!(!json.contains("\"ua_security_policy\""), "got: {json}");
    assert!(
        json.contains("\"ua_security_mode\":\"Sign\""),
        "got: {json}"
    );
    // Unknown legacy value is a hard error, not a silent Auto.
    let bad = legacy.replace("Авто", "NoSuchPolicy");
    assert!(serde_json::from_str::<EndpointSpec>(&bad).is_err());
}
#[test]
fn endpoint_auto_legacy_forms_share_identity_and_canonical_json() {
    let base = serde_json::to_value(EndpointSpec::ua("opc.tcp://example:4840")).unwrap();
    let mut nulls = base.clone();
    nulls["ua_security_policy"] = serde_json::Value::Null;
    nulls["ua_security_mode"] = serde_json::Value::Null;
    let mut explicit = base.clone();
    explicit["ua_security_policy"] = serde_json::json!("Auto");
    explicit["ua_security_mode"] = serde_json::json!("Auto");
    for value in [base.clone(), nulls, explicit] {
        let endpoint: EndpointSpec = serde_json::from_value(value).unwrap();
        assert_eq!(endpoint.ua_security_policy, UaSecurityPolicy::Auto);
        assert_eq!(endpoint.ua_security_mode, UaSecurityMode::Auto);
        assert_eq!(serde_json::to_value(endpoint).unwrap(), base);
    }
    let mut concrete = base.clone();
    concrete["ua_security_policy"] = serde_json::json!("Basic256Sha256");
    concrete["ua_security_mode"] = serde_json::json!("SignAndEncrypt");
    let endpoint: EndpointSpec = serde_json::from_value(concrete.clone()).unwrap();
    assert_eq!(serde_json::to_value(endpoint).unwrap(), concrete);
    let da = serde_json::to_value(EndpointSpec::da("host", "Server")).unwrap();
    assert!(da.get("ua_security_policy").is_none());
    assert!(da.get("ua_security_mode").is_none());
}
