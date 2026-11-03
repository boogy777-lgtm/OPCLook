//! Real SDK task on the manager-owned host; one fresh test process.
#![cfg(all(windows, feature = "native-da-test-hooks"))]
use opclook_backend::bff::{
    da::{
        TestManagerAsyncLease, test_async_gate_arm, test_async_gate_close, test_async_gate_entered,
        test_async_gate_release, test_host_acquisitions,
    },
    finish_native_sdk,
};
use std::time::{Duration, Instant};

struct ReleaseGate;
impl Drop for ReleaseGate {
    fn drop(&mut self) {
        let _ = test_async_gate_release();
    }
}

#[test]
fn manager_finish_is_busy_until_sdk_async_lease_retires() {
    assert_eq!(
        test_host_acquisitions(),
        (0, true),
        "requires a fresh process"
    );
    let mut lease = TestManagerAsyncLease::open_fixture().expect("manager-owned session");
    assert_eq!(
        test_host_acquisitions(),
        (1, true),
        "actual manager acquired the host"
    );
    test_async_gate_arm().expect("arm SDK worker barrier");
    let _release = ReleaseGate;
    let started = Instant::now();
    assert_eq!(lease.read(100).expect("async read"), 0x80070102_u32 as i32);
    assert!(
        started.elapsed() < Duration::from_secs(10),
        "read did not return boundedly"
    );
    test_async_gate_entered(10000).expect("SDK worker reached barrier");
    let started = Instant::now();
    assert_eq!(
        finish_native_sdk(Duration::from_secs(3)).unwrap_err(),
        "SDK sessions still running"
    );
    assert!(
        started.elapsed() < Duration::from_secs(10),
        "manager busy reply was not bounded"
    );
    assert_eq!(
        test_host_acquisitions(),
        (1, true),
        "busy finish must not discard host"
    );
    test_async_gate_release().expect("release SDK worker");
    assert_eq!(
        lease.retire(30000).expect("retire"),
        (0, 0),
        "cleanup acknowledgement, not second read"
    );
    test_async_gate_close().expect("close barrier");
    lease.close().expect("close actual session");
    finish_native_sdk(Duration::from_secs(10)).expect("finish manager and clean SDK host");
}
