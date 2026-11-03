//! Real SDK single-item Async outcomes, through the shared decoder and lease.
#![cfg(all(windows, feature = "native-da-test-hooks"))]
use opclook_backend::bff::{da::TestManagerAsyncLease, finish_native_sdk};
use opclook_contract::model::{BackendMsg, QualityKind, Value, ValueOrigin};
use std::time::Duration;

#[test]
fn sdk_async_failure_and_quality_facts() {
    for (id, expected, kind, quality) in [
        ("Fixture.Value", 42, QualityKind::Good, 0xc0),
        ("Fixture.ReadUncertain", 51, QualityKind::Uncertain, 0x40),
        ("Fixture.ReadBad", 52, QualityKind::Bad, 0x00),
    ] {
        let mut lease = TestManagerAsyncLease::open_fixture_item(id).expect("fixture registration");
        let value = lease.read_value(10_000).expect("async read");
        assert_eq!(value.value, Value::I64(expected), "{id}: {value:?}");
        assert_eq!(value.quality.kind, kind, "{id}");
        assert_eq!(value.quality.code, quality, "{id}");
        assert!(value.timestamp.is_some(), "{id}");
        assert_eq!(value.times.da_timestamp, value.timestamp, "{id}");
        assert_eq!(value.times.origin, ValueOrigin::Read, "{id}");
        assert!(value.reason.is_none(), "{id}: {:?}", value.reason);
        let (flags, submit, item) = lease.facts().expect("slot facts");
        assert_eq!(
            flags & 0xb,
            0xb,
            "{id}: SDK call, return and matching callback: {flags:#x}"
        );
        assert!(submit >= 0, "{id}: SDK submit {submit:#x}");
        if flags & 4 != 0 {
            assert!(item >= 0, "{id}: SDK admission item {item:#x}");
        }
        eprintln!(
            "{id}: flags={flags:#x} submit={submit:#x} admission_item={:?}",
            (flags & 4 != 0).then_some(item)
        );
        lease.close().expect("SDK task and callback retired");
    }
    let mut lease = TestManagerAsyncLease::open_fixture_item("Fixture.ReadDenied")
        .expect("denied fixture registration");
    let value = lease.read_value(10_000).expect("async read");
    assert_eq!(value.value, Value::Empty);
    assert_eq!(value.quality.kind, QualityKind::Bad);
    assert!(value.timestamp.is_none());
    assert!(value.times.da_timestamp.is_none());
    assert_eq!(value.times.origin, ValueOrigin::Read);
    assert!(
        matches!(&value.reason, Some(BackendMsg::DaReadFailure {
        adapter: Some(0), service: Some(service), item: Some(item), quality: None,
        filetime: None, vartype: None, cause: None,
    }) if *service >= 0 && *item == 0x80070005_u32 as i32),
        "{value:?}"
    );
    let (flags, submit, item) = lease.facts().expect("denied slot facts");
    assert_eq!(flags & 3, 3, "SDK call attempted and returned: {flags:#x}");
    assert!(submit >= 0, "SDK admission returned {submit:#x}");
    if flags & 4 != 0 && item < 0 {
        assert_eq!(item, 0x80070005_u32 as i32, "SDK admission item");
        // An admission rejection does not require a callback.
    } else {
        assert_ne!(
            flags & 8,
            0,
            "accepted or unknown admission requires matching callback"
        );
    }
    eprintln!(
        "denied: flags={flags:#x} submit={submit:#x} admission_item={:?} callback={}",
        (flags & 4 != 0).then_some(item),
        flags & 8 != 0
    );
    lease.close().expect("denied request retired");
    finish_native_sdk(Duration::from_secs(10)).expect("SDK manager cleanup");
}
