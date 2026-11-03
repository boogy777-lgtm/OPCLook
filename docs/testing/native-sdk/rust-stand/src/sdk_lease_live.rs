#[cfg(windows)]
#[path = "../../../../../crates/backend/src/bff/softing_runtime.rs"]
mod softing_runtime;

#[cfg(windows)]
fn main() -> Result<(), String> {
    use softing_runtime::{
        Guid, Host, TestAsyncSessionLease, test_async_gate_arm, test_async_gate_close,
        test_async_gate_entered, test_async_gate_release,
    };
    let guid = Guid {
        data1: 0x7d4bd1b1,
        data2: 0x7422,
        data3: 0x48c0,
        data4: [0xad, 0x58, 0x60, 0x1b, 0x70, 0x14, 0x49, 0xc6],
    };
    let host = Host::new().map_err(|e| format!("host: {e:?}"))?;
    let shared = host.shared();
    let (session, _) = shared
        .open_config("OPCLook.NativeDaFixture.1", "localhost", &guid, true, 1000)
        .map_err(|e| format!("connect: {e:?}"))?;
    drop(shared);
    let mut lease = TestAsyncSessionLease::new(session);
    lease.prepare().map_err(|e| format!("prepare: {e:?}"))?;
    let (key, _, _) = lease
        .session_mut()
        .ok_or("missing session")?
        .add("Fixture.DeviceValue")
        .map_err(|e| format!("add: {e:?}"))?;
    test_async_gate_arm().map_err(|e| format!("arm: {e:?}"))?;
    let result = (|| -> Result<(), String> {
        let (hr, _) = lease
            .read(key, 0, 100)
            .map_err(|e| format!("read: {e:?}"))?;
        if hr != 0x80070102_u32 as i32 {
            return Err(format!("expected timeout: {hr:#x}"));
        }
        test_async_gate_entered(10000).map_err(|e| format!("entered: {e:?}"))?;
        if host.is_idle() {
            return Err("host became idle while SDK task was blocked".into());
        }
        test_async_gate_release().map_err(|e| format!("release: {e:?}"))?;
        let (hr, value) = lease
            .wait_retire(30000)
            .map_err(|e| format!("retire: {e:?}"))?;
        if hr != 0 || value.value != 0 {
            return Err(format!("completion: {hr:#x}, {value:?}"));
        }
        test_async_gate_close().map_err(|e| format!("gate close: {e:?}"))?;
        Ok(())
    })();
    if result.is_err() {
        let _ = test_async_gate_release();
    }
    result?;
    let session = lease.into_session().map_err(|_| "lease remains busy")?;
    session
        .close()
        .map_err(|e| format!("session close: {e:?}"))?;
    if !host.is_idle() {
        return Err("host remains busy after close".into());
    }
    host.close().map_err(|e| format!("host close: {e:?}"))?;
    println!("Rust SDK async session lease and host cleanup: PASS");
    Ok(())
}
#[cfg(not(windows))]
fn main() {
    eprintln!("native SDK lease fixture requires Windows x86");
    std::process::exit(1);
}
