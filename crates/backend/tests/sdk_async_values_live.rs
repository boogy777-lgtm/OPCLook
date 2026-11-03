//! Test-only one-slot SDK Async values through the existing Rust decoder.
#![cfg(all(windows, feature = "native-da-test-hooks"))]
use opclook_backend::bff::{da::TestManagerAsyncLease, finish_native_sdk};
use opclook_contract::model::{BackendMsg, QualityKind, Value};
use std::time::Duration;

#[test]
fn sdk_async_generic_values_and_bounded_failure() {
    let cases = [
        ("Fixture.Value", Value::I64(42)),
        ("Fixture.Bool", Value::Bool(true)),
        ("Fixture.Unicode", Value::Str("РУГ 中文".into())),
        (
            "Fixture.Float64Array",
            Value::Array(vec![Value::F64(-1.25), Value::F64(0.5), Value::F64(42.0)]),
        ),
        (
            "Fixture.BoolArray",
            Value::Array(vec![
                Value::Bool(false),
                Value::Bool(true),
                Value::Bool(false),
            ]),
        ),
        (
            "Fixture.Int64Array",
            Value::Array(vec![
                Value::I64(i64::MIN),
                Value::I64(-9_007_199_254_740_993),
                Value::I64(i64::MAX),
            ]),
        ),
        (
            "Fixture.Strings",
            Value::Array(vec![Value::Str("РУГ 中文".into()), Value::Str("".into())]),
        ),
    ];
    for (id, expected) in cases {
        let mut lease =
            TestManagerAsyncLease::open_fixture_item(id).expect("SDK subscribed fixture session");
        let value = lease.read_value(10_000).expect("SDK async request");
        assert_eq!(value.value, expected, "{id}: {value:?}");
        assert_eq!(value.quality.code, 0xc0, "{id}");
        assert!(value.timestamp.is_some(), "{id}");
        assert_eq!(value.times.da_timestamp, value.timestamp);
        assert!(value.reason.is_none(), "{id}: {:?}", value.reason);
        lease.close().expect("retired SDK session closes");
    }
    let mut lease =
        TestManagerAsyncLease::open_fixture_item("Fixture.LargeInts").expect("oversized fixture");
    let value = lease.read_value(10_000).expect("SDK async request");
    assert_eq!(value.value, Value::Empty);
    assert_eq!(value.quality.kind, QualityKind::Bad);
    assert!(value.timestamp.is_some());
    assert!(matches!(&value.reason, Some(BackendMsg::DaReadFailure {
        adapter: Some(code), service: Some(0), item: Some(0), quality: Some(0xc0),
        filetime: Some(ticks), vartype: Some(0x2003), ..
    }) if *code < 0 && *ticks != 0));
    lease
        .close()
        .expect("failed conversion still retires SDK session");
    finish_native_sdk(Duration::from_secs(10)).expect("finish SDK manager");
}
