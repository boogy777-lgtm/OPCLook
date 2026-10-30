#[cfg(windows)]
#[path = "../../../../../crates/backend/src/bff/softing_runtime.rs"]
mod softing_runtime;

#[cfg(windows)]
fn main() -> Result<(), String> {
    use softing_runtime::{Error, Guid, Host};
    let guid = Guid {
        data1: 0x7d4bd1b1,
        data2: 0x7422,
        data3: 0x48c0,
        data4: [0xad, 0x58, 0x60, 0x1b, 0x70, 0x14, 0x49, 0xc6],
    };
    let host = Host::new().map_err(|e| format!("host: {e:?}"))?;
    let shared = host.shared();
    let mut session = shared
        .open("OPCLook.NativeDaFixture.1", "localhost", &guid)
        .map_err(|e| format!("connect: {e:?}"))?;
    let (first, vt, rights) = session
        .add("Fixture.Value")
        .map_err(|e| format!("add: {e:?}"))?;
    let value = session
        .read_i4(first, 0)
        .map_err(|e| format!("read: {e:?}"))?;
    if vt != 3
        || rights & 2 == 0
        || value.service_result < 0
        || value.item_result < 0
        || value.value != 42
        || value.quality & 0xc0 != 0xc0
        || (value.filetime_low == 0 && value.filetime_high == 0)
    {
        return Err(format!("bad first read: {value:?}"));
    }
    let written = session
        .write_i4(first, 77)
        .map_err(|e| format!("write: {e:?}"))?;
    if written < 0 {
        return Err(format!("write item status: {written:#x}"));
    }
    session
        .remove(first)
        .map_err(|e| format!("remove: {e:?}"))?;
    let (second, _, _) = session
        .add("Fixture.Value")
        .map_err(|e| format!("re-add: {e:?}"))?;
    if first == second || !matches!(session.read_i4(first, 0), Err(Error::InvalidHandle)) {
        return Err("stale registration was accepted".to_string());
    }
    let value = session
        .read_i4(second, 0)
        .map_err(|e| format!("readback: {e:?}"))?;
    if value.service_result < 0 || value.item_result < 0 || value.value != 77 {
        return Err(format!("bad readback: {value:?}"));
    }
    let mut other = shared
        .open("OPCLook.NativeDaFixture.1", "localhost", &guid)
        .map_err(|e| format!("second session: {e:?}"))?;
    let (other_key, _, _) = other
        .add("Fixture.Value")
        .map_err(|e| format!("second session add: {e:?}"))?;
    if other_key == second || !matches!(other.read_i4(second, 0), Err(Error::InvalidHandle)) {
        return Err("registration from another session was accepted".to_string());
    }
    other
        .close()
        .map_err(|e| format!("second session close: {e:?}"))?;
    session
        .close()
        .map_err(|e| format!("session close: {e:?}"))?;
    drop(shared);
    host.close().map_err(|e| format!("host close: {e:?}"))?;
    println!("Rust -> SDK C ABI live fixture and stale registration: PASS");
    Ok(())
}
#[cfg(not(windows))]
fn main() {
    eprintln!("native-live requires Windows x86");
    std::process::exit(1);
}
