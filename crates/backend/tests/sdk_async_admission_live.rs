//! A real SDK Async per-item admission failure using ordinary access rights.
#![cfg(all(windows, feature = "native-da-test-hooks"))]
use opclook_backend::bff::{da::TestManagerAsyncLease, finish_native_sdk};
use opclook_contract::model::{BackendMsg, QualityKind, Value, ValueOrigin};
use std::time::Duration;

#[test]
fn sdk_async_write_only_item_rejected_at_admission() {
    let mut lease = TestManagerAsyncLease::open_fixture_item("Fixture.WriteOnly")
        .expect("SDK registers write-only item");
    let value = lease.read_value(10_000).expect("SDK async request");
    let (flags, submit, item) = lease.facts().expect("SDK slot facts");
    eprintln!("write-only: flags={flags:#x}, submit={submit:#x}, item={item:#x}");
    // Actual SDK call attempted and returned, per-item status supplied, no
    // matching completion for a transaction with no accepted requests.
    assert_eq!(flags & 0xf, 0x7);
    assert_eq!(submit, 1, "S_FALSE: per-item admission failure");
    assert_eq!(item, 0xc0040006_u32 as i32, "OPC_E_BADRIGHTS");
    assert_eq!(value.value, Value::Empty);
    assert_eq!(value.quality.kind, QualityKind::Bad);
    assert!(value.timestamp.is_none());
    assert!(value.times.da_timestamp.is_none());
    assert_eq!(value.times.origin, ValueOrigin::Read);
    assert!(matches!(value.reason, Some(BackendMsg::DaReadFailure {
        adapter: Some(0), service: Some(1), item: Some(code),
        quality: None, filetime: None, vartype: None, cause: None,
    }) if code == 0xc0040006_u32 as i32));
    lease
        .close()
        .expect("rejected item retired without callback");
    finish_native_sdk(Duration::from_secs(10)).expect("SDK manager cleanup");
}
