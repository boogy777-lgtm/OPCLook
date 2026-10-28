#![windows_subsystem = "windows"]

use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::ffi::c_void;
use std::fs::OpenOptions;
use std::io::{BufRead, BufReader, Write};
use std::net::{IpAddr, Ipv4Addr};
use std::os::windows::io::{AsRawHandle, FromRawHandle};
use std::time::{Duration, SystemTime, UNIX_EPOCH};
use windows::Win32::Foundation::{
    CloseHandle, ERROR_OBJECT_ALREADY_EXISTS, ERROR_PIPE_CONNECTED, ERROR_SUCCESS, GetLastError,
    HANDLE, HLOCAL, LocalFree,
};
use windows::Win32::NetworkManagement::IpHelper::{
    ConvertInterfaceLuidToIndex, CreateIpForwardEntry2, DeleteIpForwardEntry2, IP_ADDRESS_PREFIX,
    InitializeIpForwardEntry, MIB_IPFORWARD_ROW2, SetIpForwardEntry2,
};
use windows::Win32::NetworkManagement::Ndis::NET_LUID_LH;
use windows::Win32::Networking::WinSock::{
    AF_INET, AF_INET6, IN_ADDR, IN_ADDR_0, IN6_ADDR, IN6_ADDR_0, MIB_IPPROTO_NETMGMT, SOCKADDR_IN,
    SOCKADDR_IN6, SOCKADDR_IN6_0, SOCKADDR_INET,
};
use windows::Win32::Security::Authorization::{
    ConvertSidToStringSidW, ConvertStringSecurityDescriptorToSecurityDescriptorW,
};
use windows::Win32::Security::{
    GetTokenInformation, PSECURITY_DESCRIPTOR, PSID, RevertToSelf, SECURITY_ATTRIBUTES,
    TOKEN_QUERY, TOKEN_USER, TokenUser,
};
use windows::Win32::Storage::FileSystem::{FILE_FLAG_FIRST_PIPE_INSTANCE, PIPE_ACCESS_DUPLEX};
use windows::Win32::System::Pipes::{
    ConnectNamedPipe, CreateNamedPipeW, DisconnectNamedPipe, GetNamedPipeClientProcessId,
    ImpersonateNamedPipeClient, PIPE_READMODE_BYTE, PIPE_TYPE_BYTE, PIPE_WAIT,
};
use windows::Win32::System::Threading::{
    GetCurrentProcess, GetCurrentThread, OpenProcessToken, OpenThreadToken,
};
use windows::core::{PCWSTR, PWSTR};

const PROTOCOL_VERSION: u32 = 2;
const DEFAULT_LIFETIME_SECS: u32 = 3600;
const MAX_LIFETIME_SECS: u32 = 86_400;
const PIPE_CONNECT_TIMEOUT: Duration = Duration::from_secs(60);

#[derive(Debug, Serialize, Deserialize)]
struct PipeMessage {
    version: u32,
    op: String,
    #[serde(default)]
    target: Option<String>,
    #[serde(default)]
    prefix: Option<u8>,
    #[serde(default)]
    interface_luid: Option<u64>,
    #[serde(default)]
    next_hop: Option<String>,
    #[serde(default)]
    lifetime_secs: Option<u32>,
    #[serde(default)]
    client_pid: Option<u32>,
}

#[derive(Debug, Serialize, Deserialize)]
struct PipeReply {
    ok: bool,
    #[serde(default)]
    error: Option<String>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
struct CreatedRoute {
    target: IpAddr,
    interface_luid: u64,
    prefix: u8,
    next_hop: IpAddr,
}

fn route_key(route: &RouteSpec) -> CreatedRoute {
    CreatedRoute {
        target: route.target,
        interface_luid: route.interface_luid,
        prefix: route.prefix,
        next_hop: route.next_hop,
    }
}

struct Audit {
    path: std::path::PathBuf,
}

impl Audit {
    fn new() -> Self {
        let path = std::env::var_os("APPDATA")
            .map(std::path::PathBuf::from)
            .unwrap_or_else(std::env::temp_dir)
            .join("opcda-egui")
            .join("nethelper-audit.log");
        let _ = std::fs::create_dir_all(path.parent().unwrap_or_else(|| std::path::Path::new(".")));
        Self { path }
    }

    fn log(&self, level: &str, text: &str) {
        let stamp = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs();
        if let Ok(mut file) = OpenOptions::new()
            .create(true)
            .append(true)
            .open(&self.path)
        {
            let _ = writeln!(file, "{stamp} [{level}] {text}");
        }
    }
}

fn main() {
    let args: Vec<String> = std::env::args().collect();
    let mut pipe = None;
    let mut parent = None;
    let mut index = 1;
    while index + 1 < args.len() {
        match args[index].as_str() {
            "--pipe" => pipe = Some(args[index + 1].clone()),
            "--parent" => parent = args[index + 1].parse::<u32>().ok(),
            _ => {}
        }
        index += 2;
    }
    let (Some(pipe), Some(parent)) = (pipe, parent) else {
        return;
    };
    let audit = Audit::new();
    audit.log("start", &format!("pipe={pipe} parent={parent}"));

    if let Err(error) = run(&pipe, parent, &audit) {
        audit.log("error", &error);
    }
    audit.log("exit", "pipe closed, helper exiting");
}

fn run(pipe: &str, parent: u32, audit: &Audit) -> Result<(), String> {
    let own_sid = current_process_sid()?;
    let user_sid = own_sid.to_string();
    let (handle, security) = create_pipe(pipe, &user_sid)?;
    let _security = security;
    wait_for_client(handle)?;

    let file = unsafe { std::fs::File::from_raw_handle(handle.0 as _) };
    let mut reader = BufReader::new(file);
    let mut writer = reader
        .get_ref()
        .try_clone()
        .map_err(|e| format!("clone pipe: {e}"))?;
    let pipe_handle = HANDLE(reader.get_ref().as_raw_handle() as _);

    let client_pid = client_process_id(pipe_handle)?;
    if client_pid != parent {
        let message = format!("unexpected client pid {client_pid}, expected {parent}");
        let _ = reply(&mut writer, false, Some(&message));
        audit.log("reject", &message);
        return Err(message);
    }
    let client_sid = client_sid(pipe_handle)?;
    if !client_sid.eq_ignore_ascii_case(&own_sid) {
        let message = format!("client SID {client_sid} is not the helper user {own_sid}");
        let _ = reply(&mut writer, false, Some(&message));
        audit.log("reject", &message);
        return Err(message);
    }

    let mut authenticated = false;
    let mut created: HashMap<CreatedRoute, RouteSpec> = HashMap::new();
    loop {
        let mut line = String::new();
        match reader.read_line(&mut line) {
            Ok(0) => break,
            Ok(_) => {}
            Err(error) => {
                audit.log("error", &format!("pipe read: {error}"));
                break;
            }
        }
        let trimmed = line.trim();
        if trimmed.is_empty() {
            continue;
        }
        let message: PipeMessage = match serde_json::from_str(trimmed) {
            Ok(message) => message,
            Err(error) => {
                let _ = reply(&mut writer, false, Some(&format!("invalid JSON: {error}")));
                continue;
            }
        };
        if message.version != PROTOCOL_VERSION {
            let _ = reply(&mut writer, false, Some("protocol version mismatch"));
            continue;
        }
        if !authenticated {
            if message.op != "hello" || message.client_pid != Some(parent) {
                let _ = reply(&mut writer, false, Some("hello required"));
                audit.log("reject", "bad hello");
                break;
            }
            authenticated = true;
            let _ = reply(&mut writer, true, None);
            audit.log("hello", &format!("client pid {client_pid}"));
            continue;
        }
        match message.op.as_str() {
            "add" => match add_route(&message) {
                Ok(route) => {
                    created.insert(route_key(&route), route);
                    audit.log(
                        "add",
                        &format!(
                            "target={} luid={} prefix={} next_hop={} lifetime={}",
                            route.target,
                            route.interface_luid,
                            route.prefix,
                            route.next_hop,
                            route.lifetime_secs
                        ),
                    );
                    let _ = reply(&mut writer, true, None);
                }
                Err(error) => {
                    audit.log("add-rejected", &error);
                    let _ = reply(&mut writer, false, Some(&error));
                }
            },
            "refresh" => match refresh_route(&message, &created) {
                Ok(()) => {
                    audit.log(
                        "refresh",
                        &format!(
                            "target={} luid={}",
                            message.target.as_deref().unwrap_or(""),
                            message.interface_luid.unwrap_or(0)
                        ),
                    );
                    let _ = reply(&mut writer, true, None);
                }
                Err(error) => {
                    audit.log("refresh-rejected", &error);
                    let _ = reply(&mut writer, false, Some(&error));
                }
            },
            "remove" => match remove_route(&message, &mut created) {
                Ok(()) => {
                    audit.log(
                        "remove",
                        &format!("target={}", message.target.as_deref().unwrap_or("")),
                    );
                    let _ = reply(&mut writer, true, None);
                }
                Err(error) => {
                    audit.log("remove-rejected", &error);
                    let _ = reply(&mut writer, false, Some(&error));
                }
            },
            "shutdown" => {
                let _ = reply(&mut writer, true, None);
                break;
            }
            _ => {
                let _ = reply(&mut writer, false, Some("unsupported operation"));
            }
        }
    }

    for (_, route) in created {
        let _ = delete_created(route);
        audit.log(
            "cleanup",
            &format!("target={} luid={}", route.target, route.interface_luid),
        );
    }
    unsafe {
        let _ = DisconnectNamedPipe(pipe_handle);
    }
    Ok(())
}

fn reply(writer: &mut std::fs::File, ok: bool, error: Option<&str>) -> Result<(), String> {
    let reply = PipeReply {
        ok,
        error: error.map(|e| e.to_string()),
    };
    let mut line = serde_json::to_vec(&reply).map_err(|e| e.to_string())?;
    line.push(b'\n');
    writer
        .write_all(&line)
        .map_err(|e| format!("write reply: {e}"))?;
    writer.flush().map_err(|e| format!("flush reply: {e}"))
}

struct LocalDescriptor(PSECURITY_DESCRIPTOR);

impl Drop for LocalDescriptor {
    fn drop(&mut self) {
        if !self.0.0.is_null() {
            // SAFETY: the descriptor was allocated by ConvertStringSecurityDescriptorToSecurityDescriptorW.
            unsafe {
                let _ = LocalFree(Some(HLOCAL(self.0.0)));
            }
        }
    }
}

fn create_pipe(pipe: &str, user_sid: &str) -> Result<(HANDLE, LocalDescriptor), String> {
    let sddl = format!("D:(A;;GA;;;{user_sid})");
    let sddl_w = wide(&sddl);
    let mut descriptor = PSECURITY_DESCRIPTOR::default();
    unsafe {
        ConvertStringSecurityDescriptorToSecurityDescriptorW(
            PCWSTR(sddl_w.as_ptr()),
            1,
            &mut descriptor,
            None,
        )
        .map_err(|e| format!("security descriptor: {e}"))?;
    }
    let descriptor = LocalDescriptor(descriptor);
    let attributes = SECURITY_ATTRIBUTES {
        nLength: std::mem::size_of::<SECURITY_ATTRIBUTES>() as u32,
        lpSecurityDescriptor: descriptor.0.0,
        bInheritHandle: false.into(),
    };
    let name = format!(r"\\.\pipe\{pipe}");
    let name_w = wide(&name);
    let handle = unsafe {
        CreateNamedPipeW(
            PCWSTR(name_w.as_ptr()),
            PIPE_ACCESS_DUPLEX | FILE_FLAG_FIRST_PIPE_INSTANCE,
            PIPE_TYPE_BYTE | PIPE_READMODE_BYTE | PIPE_WAIT,
            1,
            4096,
            4096,
            0,
            Some(&attributes),
        )
    };
    if handle.is_invalid() {
        let error = unsafe { GetLastError() };
        return Err(format!("CreateNamedPipeW failed with {}", error.0));
    }
    Ok((handle, descriptor))
}

fn wait_for_client(handle: HANDLE) -> Result<(), String> {
    let (tx, rx) = std::sync::mpsc::channel();
    let raw = handle.0 as usize;
    std::thread::spawn(move || {
        let handle = HANDLE(raw as _);
        let result = unsafe { ConnectNamedPipe(handle, None) };
        let _ = tx.send(result);
    });
    match rx.recv_timeout(PIPE_CONNECT_TIMEOUT) {
        Ok(Ok(())) => Ok(()),
        Ok(Err(error)) => pipe_connect_result(error),
        Err(_) => Err("client did not connect before the helper timeout".to_string()),
    }
}

fn pipe_connect_result(error: windows::core::Error) -> Result<(), String> {
    if error.code() == windows::core::HRESULT::from_win32(ERROR_PIPE_CONNECTED.0) {
        Ok(())
    } else {
        Err(format!("ConnectNamedPipe failed: {error}"))
    }
}

fn client_process_id(handle: HANDLE) -> Result<u32, String> {
    let mut pid = 0u32;
    unsafe { GetNamedPipeClientProcessId(handle, &mut pid) }
        .map_err(|e| format!("GetNamedPipeClientProcessId: {e}"))?;
    Ok(pid)
}

fn client_sid(handle: HANDLE) -> Result<String, String> {
    unsafe { ImpersonateNamedPipeClient(handle) }
        .map_err(|e| format!("ImpersonateNamedPipeClient: {e}"))?;
    let result = (|| {
        let mut token = HANDLE::default();
        unsafe { OpenThreadToken(GetCurrentThread(), TOKEN_QUERY, true, &mut token) }
            .map_err(|e| format!("OpenThreadToken: {e}"))?;
        let sid = token_sid(token);
        unsafe {
            let _ = CloseHandle(token);
        }
        sid
    })();
    unsafe {
        let _ = RevertToSelf();
    }
    result
}

fn current_process_sid() -> Result<String, String> {
    let mut token = HANDLE::default();
    unsafe { OpenProcessToken(GetCurrentProcess(), TOKEN_QUERY, &mut token) }
        .map_err(|e| format!("OpenProcessToken: {e}"))?;
    let sid = token_sid(token);
    unsafe {
        let _ = CloseHandle(token);
    }
    sid
}

fn aligned_token_buffer(needed: u32) -> Vec<usize> {
    let words = (needed as usize).div_ceil(std::mem::size_of::<usize>());
    vec![0usize; words]
}

fn token_sid(token: HANDLE) -> Result<String, String> {
    let mut needed = 0u32;
    unsafe {
        let _ = GetTokenInformation(token, TokenUser, None, 0, &mut needed);
    }
    if needed == 0 {
        return Err("GetTokenInformation returned no size".to_string());
    }
    // TOKEN_USER contains pointers; the storage must satisfy its alignment.
    let mut buffer = aligned_token_buffer(needed);
    unsafe {
        GetTokenInformation(
            token,
            TokenUser,
            Some(buffer.as_mut_ptr() as *mut c_void),
            needed,
            &mut needed,
        )
    }
    .map_err(|e| format!("GetTokenInformation(TokenUser): {e}"))?;
    let user = unsafe { &*(buffer.as_ptr() as *const TOKEN_USER) };
    sid_to_string(user.User.Sid)
}

fn sid_to_string(sid: PSID) -> Result<String, String> {
    let mut text = PWSTR::null();
    unsafe { ConvertSidToStringSidW(sid, &mut text) }
        .map_err(|e| format!("ConvertSidToStringSidW: {e}"))?;
    let result = unsafe { text.to_string() }.map_err(|e| format!("SID text: {e}"));
    unsafe {
        LocalFree(Some(HLOCAL(text.0 as _)));
    }
    result
}

fn add_route(message: &PipeMessage) -> Result<RouteSpec, String> {
    let route = validate_route(message)?;
    let row = route_row(&route)?;
    let result = unsafe { CreateIpForwardEntry2(&row) };
    if result == ERROR_SUCCESS {
        Ok(route)
    } else if result == ERROR_OBJECT_ALREADY_EXISTS {
        Err(format!(
            "route to {} already exists; refusing to overwrite",
            route.target
        ))
    } else {
        Err(format!("CreateIpForwardEntry2 failed with {}", result.0))
    }
}

fn remove_route(
    message: &PipeMessage,
    created: &mut HashMap<CreatedRoute, RouteSpec>,
) -> Result<(), String> {
    let route = validate_route(message)?;
    let key = route_key(&route);
    let Some(removed) = created.get(&key).copied() else {
        return Err("route is not owned by this helper".to_string());
    };
    delete_created(removed)?;
    created.remove(&key);
    Ok(())
}

fn refresh_route(
    message: &PipeMessage,
    created: &HashMap<CreatedRoute, RouteSpec>,
) -> Result<(), String> {
    let route = validate_route(message)?;
    let key = route_key(&route);
    if !created.contains_key(&key) {
        return Err("route is not owned by this helper".to_string());
    }
    let row = route_row(&route)?;
    let result = unsafe { SetIpForwardEntry2(&row) };
    if result == ERROR_SUCCESS {
        Ok(())
    } else {
        Err(format!("SetIpForwardEntry2 failed with {}", result.0))
    }
}

fn delete_created(route: RouteSpec) -> Result<(), String> {
    let row = route_row(&route)?;
    let result = unsafe { DeleteIpForwardEntry2(&row) };
    if result == ERROR_SUCCESS {
        Ok(())
    } else {
        Err(format!("DeleteIpForwardEntry2 failed with {}", result.0))
    }
}

#[derive(Clone, Copy, Debug)]
struct RouteSpec {
    target: IpAddr,
    prefix: u8,
    interface_luid: u64,
    next_hop: IpAddr,
    lifetime_secs: u32,
}

fn validate_route(message: &PipeMessage) -> Result<RouteSpec, String> {
    let target: IpAddr = message
        .target
        .as_deref()
        .ok_or_else(|| "missing target".to_string())?
        .parse()
        .map_err(|_| "invalid target address".to_string())?;
    if target.is_loopback() || target.is_unspecified() || is_multicast_or_broadcast(target) {
        return Err("target must be a unicast host address".to_string());
    }
    let expected_prefix = if target.is_ipv4() { 32 } else { 128 };
    if message.prefix != Some(expected_prefix) {
        return Err(format!("prefix must be exactly /{expected_prefix}"));
    }
    let interface_luid = message
        .interface_luid
        .filter(|luid| *luid != 0)
        .ok_or_else(|| "missing interface LUID".to_string())?;
    let next_hop: IpAddr = message
        .next_hop
        .as_deref()
        .ok_or_else(|| "missing next hop".to_string())?
        .parse()
        .map_err(|_| "invalid next hop".to_string())?;
    if next_hop.is_ipv4() != target.is_ipv4() {
        return Err("next hop family does not match the target".to_string());
    }
    let unspecified = next_hop.is_unspecified();
    if !unspecified && (next_hop.is_multicast() || next_hop.is_loopback()) {
        return Err("next hop must be on-link, a gateway or the unspecified address".to_string());
    }
    let lifetime_secs = message
        .lifetime_secs
        .unwrap_or(DEFAULT_LIFETIME_SECS)
        .clamp(1, MAX_LIFETIME_SECS);
    Ok(RouteSpec {
        target,
        prefix: expected_prefix,
        interface_luid,
        next_hop,
        lifetime_secs,
    })
}

fn route_row(route: &RouteSpec) -> Result<MIB_IPFORWARD_ROW2, String> {
    let luid = NET_LUID_LH {
        Value: route.interface_luid,
    };
    let mut index = 0u32;
    let result = unsafe { ConvertInterfaceLuidToIndex(&luid, &mut index) };
    if result != ERROR_SUCCESS {
        return Err(format!(
            "ConvertInterfaceLuidToIndex failed with {}",
            result.0
        ));
    }
    let mut row = MIB_IPFORWARD_ROW2::default();
    unsafe { InitializeIpForwardEntry(&mut row) };
    row.InterfaceLuid = luid;
    row.InterfaceIndex = index;
    row.DestinationPrefix = IP_ADDRESS_PREFIX {
        Prefix: sockaddr_inet(route.target),
        PrefixLength: route.prefix,
    };
    row.NextHop = sockaddr_inet(route.next_hop);
    row.SitePrefixLength = 0;
    row.ValidLifetime = route.lifetime_secs;
    row.PreferredLifetime = route.lifetime_secs;
    row.Metric = 1;
    row.Protocol = MIB_IPPROTO_NETMGMT;
    row.Loopback = false;
    row.AutoconfigureAddress = false;
    row.Publish = false;
    row.Immortal = false;
    Ok(row)
}

fn sockaddr_inet(ip: IpAddr) -> SOCKADDR_INET {
    match ip {
        IpAddr::V4(address) => {
            let mut sockaddr = SOCKADDR_INET::default();
            sockaddr.Ipv4 = SOCKADDR_IN {
                sin_family: AF_INET,
                sin_port: 0,
                sin_addr: IN_ADDR {
                    S_un: IN_ADDR_0 {
                        S_addr: u32::from(address).to_be(),
                    },
                },
                sin_zero: [0; 8],
            };
            sockaddr
        }
        IpAddr::V6(address) => {
            let mut sockaddr = SOCKADDR_INET::default();
            sockaddr.Ipv6 = SOCKADDR_IN6 {
                sin6_family: AF_INET6,
                sin6_port: 0,
                sin6_flowinfo: 0,
                sin6_addr: IN6_ADDR {
                    u: IN6_ADDR_0 {
                        Byte: address.octets(),
                    },
                },
                Anonymous: SOCKADDR_IN6_0 { sin6_scope_id: 0 },
            };
            sockaddr
        }
    }
}

fn is_multicast_or_broadcast(ip: IpAddr) -> bool {
    match ip {
        IpAddr::V4(ip) => ip.is_multicast() || ip == Ipv4Addr::BROADCAST,
        IpAddr::V6(ip) => ip.is_multicast(),
    }
}

fn wide(text: &str) -> Vec<u16> {
    text.encode_utf16().chain(std::iter::once(0)).collect()
}

#[cfg(test)]
mod regression_tests {
    use super::*;

    #[test]
    fn token_storage_is_aligned_and_large_enough() {
        for needed in [1, 7, 17, 100] {
            let buffer = aligned_token_buffer(needed);
            assert_eq!(
                buffer.as_ptr() as usize % std::mem::align_of::<TOKEN_USER>(),
                0
            );
            assert!(std::mem::size_of_val(buffer.as_slice()) >= needed as usize);
        }
    }

    #[test]
    fn route_ownership_distinguishes_next_hop() {
        let route = RouteSpec {
            target: "192.0.2.1".parse().unwrap(),
            prefix: 32,
            interface_luid: 7,
            next_hop: "192.0.2.2".parse().unwrap(),
            lifetime_secs: 10,
        };
        let mut other = route;
        other.next_hop = "192.0.2.3".parse().unwrap();
        assert_ne!(route_key(&route), route_key(&other));
    }

    #[test]
    fn already_connected_uses_error_from_pipe_thread() {
        let error =
            windows::core::Error::from(windows::core::HRESULT::from_win32(ERROR_PIPE_CONNECTED.0));
        assert!(pipe_connect_result(error).is_ok());
        let error = windows::core::Error::from(windows::core::HRESULT::from_win32(5));
        assert!(pipe_connect_result(error).is_err());
    }
}
