//! Forced SDK client DA2/DA3 routes; not a physical single-version server.
#![cfg(all(windows, feature = "native-da-test-hooks"))]
use opclook_backend::bff::{da::TestManagerAsyncLease, finish_native_sdk};
use opclook_contract::model::Value;
use std::time::Duration;

#[test]
fn forced_da2_da3_async_read_routes() {
    for (version, delta) in [(0x0200, (1, 0)), (0x0300, (0, 1))] {
        let mut lease = TestManagerAsyncLease::open_fixture_version(version)
            .expect("forced started SDK session");
        let before = lease.route_counts().expect("initial request counts");
        let value = lease
            .read_value(10_000)
            .expect("correlated async completion");
        assert_eq!(value.value, Value::I64(42));
        assert!(value.reason.is_none(), "{value:?}");
        let (flags, submit, item) = lease.facts().expect("slot facts");
        assert_eq!(flags & 0xf, 0xf, "call, return, admission and callback");
        assert!(
            submit >= 0 && item >= 0,
            "submit={submit:#x} item={item:#x}"
        );
        let after = lease.route_counts().expect("final request counts");
        assert_eq!(
            (after.0 - before.0, after.1 - before.1),
            delta,
            "version={version:#x}"
        );
        eprintln!(
            "version={version:#x} before={before:?} after={after:?} flags={flags:#x} submit={submit:#x} item={item:#x}"
        );
        lease.close().expect("retired SDK session closes");
    }
    finish_native_sdk(Duration::from_secs(10)).expect("SDK manager cleanup");
}
