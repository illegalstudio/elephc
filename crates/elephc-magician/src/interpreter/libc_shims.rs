//! Purpose:
//! Provides portable OS shims used by eval builtins that mirror PHP network and system helpers.
//!
//! Called from:
//! - `crate::interpreter::builtins::network_env`
//! - `crate::interpreter::builtins::filesystem`
//!
//! Key details:
//! - Unix delegates to libc while Windows initializes Winsock and uses Win32 system APIs.
//! - Process-global resolver records are copied before returning to callers.

use std::ffi::{CStr, c_char, c_int};
#[cfg(unix)]
use std::ffi::c_void;

/// Owns the five fields exposed through PHP's `php_uname()` modes.
pub(super) struct EvalUnameFields {
    pub(super) sysname: Vec<u8>,
    pub(super) nodename: Vec<u8>,
    pub(super) release: Vec<u8>,
    pub(super) version: Vec<u8>,
    pub(super) machine: Vec<u8>,
}

/// Copies one NUL-terminated C character array into an owned byte vector.
#[cfg(unix)]
fn nul_terminated_bytes(field: &[c_char]) -> Vec<u8> {
    let length = field
        .iter()
        .position(|byte| *byte == 0)
        .unwrap_or(field.len());
    field[..length].iter().map(|byte| *byte as u8).collect()
}

/// Returns the current hostname as raw PHP string bytes.
#[cfg(unix)]
pub(super) fn eval_os_hostname() -> Option<Vec<u8>> {
    let mut buffer = [0 as c_char; 256];
    let status = unsafe {
        // libc writes at most buffer.len() bytes into this stack buffer.
        libc::gethostname(buffer.as_mut_ptr(), buffer.len())
    };
    (status == 0).then(|| nul_terminated_bytes(&buffer))
}

/// Returns the current hostname as raw PHP string bytes.
#[cfg(windows)]
pub(super) fn eval_os_hostname() -> Option<Vec<u8>> {
    windows::hostname()
}

/// Reverse-resolves an IPv4 address and copies the canonical host name.
#[cfg(unix)]
pub(super) fn eval_reverse_ipv4_name(octets: [u8; 4]) -> Option<Vec<u8>> {
    let host = unsafe {
        // libc reads the stack-owned IPv4 octets and returns process-global storage.
        libc_gethostbyaddr(
            octets.as_ptr().cast::<c_void>(),
            octets.len() as libc::socklen_t,
            libc::AF_INET,
        )
    };
    unsafe { copy_host_name(host) }
}

/// Reverse-resolves an IPv4 address and copies the canonical host name.
#[cfg(windows)]
pub(super) fn eval_reverse_ipv4_name(octets: [u8; 4]) -> Option<Vec<u8>> {
    windows::reverse_ipv4_name(octets)
}

/// Looks up an IP protocol number by canonical name or alias.
#[cfg(unix)]
pub(super) fn eval_protocol_number(name: &CStr) -> Option<i32> {
    let entry = unsafe { libc_getprotobyname(name.as_ptr()) };
    (!entry.is_null()).then(|| unsafe { (*entry).p_proto })
}

/// Looks up an IP protocol number by canonical name or alias.
#[cfg(windows)]
pub(super) fn eval_protocol_number(name: &CStr) -> Option<i32> {
    windows::protocol_number(name)
}

/// Looks up and copies an IP protocol's canonical name.
#[cfg(unix)]
pub(super) fn eval_protocol_name(number: i32) -> Option<Vec<u8>> {
    let entry = unsafe { libc_getprotobynumber(number) };
    unsafe { copy_c_name((!entry.is_null()).then(|| (*entry).p_name)) }
}

/// Looks up and copies an IP protocol's canonical name.
#[cfg(windows)]
pub(super) fn eval_protocol_name(number: i32) -> Option<Vec<u8>> {
    windows::protocol_name(number)
}

/// Looks up an internet service port by service name and protocol.
#[cfg(unix)]
pub(super) fn eval_service_port(service: &CStr, protocol: &CStr) -> Option<u16> {
    let entry = unsafe { libc_getservbyname(service.as_ptr(), protocol.as_ptr()) };
    (!entry.is_null()).then(|| unsafe { u16::from_be((*entry).s_port as u16) })
}

/// Looks up an internet service port by service name and protocol.
#[cfg(windows)]
pub(super) fn eval_service_port(service: &CStr, protocol: &CStr) -> Option<u16> {
    windows::service_port(service, protocol)
}

/// Looks up and copies an internet service's canonical name.
#[cfg(unix)]
pub(super) fn eval_service_name(port: u16, protocol: &CStr) -> Option<Vec<u8>> {
    let entry = unsafe { libc_getservbyport(port.to_be() as c_int, protocol.as_ptr()) };
    unsafe { copy_c_name((!entry.is_null()).then(|| (*entry).s_name)) }
}

/// Looks up and copies an internet service's canonical name.
#[cfg(windows)]
pub(super) fn eval_service_name(port: u16, protocol: &CStr) -> Option<Vec<u8>> {
    windows::service_name(port, protocol)
}

/// Reads the operating-system identity fields used by `php_uname()`.
#[cfg(unix)]
pub(super) fn eval_os_uname() -> Option<EvalUnameFields> {
    let mut utsname = std::mem::MaybeUninit::<libc::utsname>::zeroed();
    let status = unsafe {
        // libc initializes the entire stack-owned structure on success.
        libc::uname(utsname.as_mut_ptr())
    };
    if status != 0 {
        return None;
    }
    let utsname = unsafe { utsname.assume_init() };
    Some(EvalUnameFields {
        sysname: nul_terminated_bytes(&utsname.sysname),
        nodename: nul_terminated_bytes(&utsname.nodename),
        release: nul_terminated_bytes(&utsname.release),
        version: nul_terminated_bytes(&utsname.version),
        machine: nul_terminated_bytes(&utsname.machine),
    })
}

/// Reads the operating-system identity fields used by `php_uname()`.
#[cfg(windows)]
pub(super) fn eval_os_uname() -> Option<EvalUnameFields> {
    windows::uname()
}

/// Sets the process file-creation mask and returns the previous mask.
#[cfg(unix)]
pub(super) fn eval_os_umask(mask: u32) -> u32 {
    unsafe { umask(mask) }
}

/// Sets the process file-creation mask and returns the previous mask.
#[cfg(windows)]
pub(super) fn eval_os_umask(mask: u32) -> u32 {
    windows::umask(mask)
}

/// Copies a nullable C string pointer into owned storage.
#[cfg(unix)]
unsafe fn copy_c_name(name: Option<*mut c_char>) -> Option<Vec<u8>> {
    let name = name?;
    if name.is_null() {
        return None;
    }
    Some(unsafe { CStr::from_ptr(name) }.to_bytes().to_vec())
}

/// Copies a libc host entry's canonical name into owned storage.
#[cfg(unix)]
unsafe fn copy_host_name(host: *mut libc::hostent) -> Option<Vec<u8>> {
    if host.is_null() {
        return None;
    }
    unsafe { copy_c_name(Some((*host).h_name)) }
}

#[cfg(unix)]
unsafe extern "C" {
    /// Reverse-resolves one socket address through libc's `gethostbyaddr`.
    #[link_name = "gethostbyaddr"]
    fn libc_gethostbyaddr(
        addr: *const c_void,
        len: libc::socklen_t,
        type_: c_int,
    ) -> *mut libc::hostent;

    /// Looks up one IP protocol entry by protocol name or alias.
    #[link_name = "getprotobyname"]
    fn libc_getprotobyname(name: *const c_char) -> *mut libc::protoent;

    /// Looks up one IP protocol entry by protocol number.
    #[link_name = "getprotobynumber"]
    fn libc_getprotobynumber(proto: c_int) -> *mut libc::protoent;

    /// Looks up one internet service entry by service name and protocol.
    #[link_name = "getservbyname"]
    fn libc_getservbyname(name: *const c_char, proto: *const c_char) -> *mut libc::servent;

    /// Looks up one internet service entry by port and protocol.
    #[link_name = "getservbyport"]
    fn libc_getservbyport(port: c_int, proto: *const c_char) -> *mut libc::servent;

    /// Sets the process file-creation mask and returns the previous mask.
    pub(super) fn umask(mask: u32) -> u32;
}

#[cfg(windows)]
mod windows {
    use super::*;
    use std::mem::MaybeUninit;
    use std::sync::OnceLock;

    const AF_INET: c_int = 2;
    const WINSOCK_VERSION_2_2: u16 = 0x0202;

    #[repr(C)]
    struct WsaData {
        version: u16,
        high_version: u16,
        description: [u8; 257],
        system_status: [u8; 129],
        max_sockets: u16,
        max_udp_datagram: u16,
        vendor_info: *mut c_char,
    }

    #[repr(C)]
    struct HostEnt {
        name: *mut c_char,
        aliases: *mut *mut c_char,
        address_type: i16,
        address_length: i16,
        address_list: *mut *mut c_char,
    }

    #[repr(C)]
    struct ProtoEnt {
        name: *mut c_char,
        aliases: *mut *mut c_char,
        protocol: i16,
    }

    #[repr(C)]
    struct OsVersionInfo {
        size: u32,
        major: u32,
        minor: u32,
        build: u32,
        platform_id: u32,
        service_pack: [u16; 128],
        service_pack_major: u16,
        service_pack_minor: u16,
        suite_mask: u16,
        product_type: u8,
        reserved: u8,
    }

    static WINSOCK_READY: OnceLock<bool> = OnceLock::new();

    /// Initializes Winsock 2.2 once for the process before database lookups.
    fn ensure_winsock() -> bool {
        *WINSOCK_READY.get_or_init(|| {
            let mut data = MaybeUninit::<WsaData>::zeroed();
            unsafe { WSAStartup(WINSOCK_VERSION_2_2, data.as_mut_ptr()) == 0 }
        })
    }

    /// Copies a nullable Winsock-owned C string into owned storage.
    unsafe fn copy_name(name: *mut c_char) -> Option<Vec<u8>> {
        if name.is_null() {
            return None;
        }
        Some(unsafe { CStr::from_ptr(name) }.to_bytes().to_vec())
    }

    /// Returns the current host name through Winsock.
    pub(super) fn hostname() -> Option<Vec<u8>> {
        if !ensure_winsock() {
            return None;
        }
        let mut buffer = [0 as c_char; 256];
        if unsafe { gethostname(buffer.as_mut_ptr(), buffer.len() as c_int) } != 0 {
            return None;
        }
        unsafe { copy_name(buffer.as_mut_ptr()) }
    }

    /// Reverse-resolves one IPv4 address through Winsock.
    pub(super) fn reverse_ipv4_name(octets: [u8; 4]) -> Option<Vec<u8>> {
        if !ensure_winsock() {
            return None;
        }
        let host = unsafe {
            gethostbyaddr(
                octets.as_ptr().cast::<c_char>(),
                octets.len() as c_int,
                AF_INET,
            )
        };
        if host.is_null() {
            return None;
        }
        unsafe { copy_name((*host).name) }
    }

    /// Looks up a protocol number through the Winsock protocol database.
    pub(super) fn protocol_number(name: &CStr) -> Option<i32> {
        if !ensure_winsock() {
            return None;
        }
        let entry = unsafe { getprotobyname(name.as_ptr()) };
        (!entry.is_null()).then(|| unsafe { i32::from((*entry).protocol) })
    }

    /// Looks up a protocol name through the Winsock protocol database.
    pub(super) fn protocol_name(number: i32) -> Option<Vec<u8>> {
        if !ensure_winsock() {
            return None;
        }
        let entry = unsafe { getprotobynumber(number) };
        if entry.is_null() {
            return None;
        }
        unsafe { copy_name((*entry).name) }
    }

    /// Looks up a service port through the Winsock services database.
    pub(super) fn service_port(service: &CStr, protocol: &CStr) -> Option<u16> {
        let service = service.to_string_lossy();
        let protocol = protocol.to_string_lossy();
        services_records().find_map(|record| {
            (record.protocol.eq_ignore_ascii_case(&protocol)
                && record.names.iter().any(|name| name.eq_ignore_ascii_case(&service)))
            .then_some(record.port)
        })
    }

    /// Looks up a service name through the Winsock services database.
    pub(super) fn service_name(port: u16, protocol: &CStr) -> Option<Vec<u8>> {
        let protocol = protocol.to_string_lossy();
        services_records()
            .find(|record| record.port == port && record.protocol.eq_ignore_ascii_case(&protocol))
            .and_then(|record| record.names.into_iter().next())
            .map(String::into_bytes)
    }

    /// Owns one parsed line from Windows' system services database.
    struct ServiceRecord {
        names: Vec<String>,
        port: u16,
        protocol: String,
    }

    /// Reads and parses Windows' canonical services database.
    fn services_records() -> impl Iterator<Item = ServiceRecord> {
        let windows_root = std::env::var_os("SystemRoot")
            .or_else(|| std::env::var_os("WINDIR"))
            .unwrap_or_else(|| r"C:\Windows".into());
        let path = std::path::PathBuf::from(windows_root)
            .join("System32")
            .join("drivers")
            .join("etc")
            .join("services");
        std::fs::read_to_string(path)
            .unwrap_or_default()
            .lines()
            .filter_map(parse_service_record)
            .collect::<Vec<_>>()
            .into_iter()
    }

    /// Parses one services file line, preserving its canonical name and aliases.
    fn parse_service_record(line: &str) -> Option<ServiceRecord> {
        let line = line.split('#').next()?.trim();
        if line.is_empty() {
            return None;
        }
        let mut fields = line.split_whitespace();
        let canonical_name = fields.next()?;
        let (port, protocol) = fields.next()?.split_once('/')?;
        let port = port.parse::<u16>().ok()?;
        let mut names = vec![canonical_name.to_string()];
        names.extend(fields.map(str::to_string));
        Some(ServiceRecord {
            names,
            port,
            protocol: protocol.to_string(),
        })
    }

    /// Reads Windows version data with the manifest-independent `RtlGetVersion` API.
    pub(super) fn uname() -> Option<EvalUnameFields> {
        let mut version = OsVersionInfo {
            size: std::mem::size_of::<OsVersionInfo>() as u32,
            major: 0,
            minor: 0,
            build: 0,
            platform_id: 0,
            service_pack: [0; 128],
            service_pack_major: 0,
            service_pack_minor: 0,
            suite_mask: 0,
            product_type: 0,
            reserved: 0,
        };
        if unsafe { RtlGetVersion(&mut version) } < 0 {
            return None;
        }
        let nodename = hostname().unwrap_or_default();
        let release = format!("{}.{}", version.major, version.minor).into_bytes();
        let service_pack_length = version
            .service_pack
            .iter()
            .position(|unit| *unit == 0)
            .unwrap_or(version.service_pack.len());
        let service_pack = String::from_utf16_lossy(&version.service_pack[..service_pack_length]);
        let version_text = if service_pack.is_empty() {
            format!("build {}", version.build)
        } else {
            format!("build {} ({service_pack})", version.build)
        };
        Some(EvalUnameFields {
            sysname: b"Windows NT".to_vec(),
            nodename,
            release,
            version: version_text.into_bytes(),
            machine: std::env::consts::ARCH.as_bytes().to_vec(),
        })
    }

    /// Sets the process file-creation mask through the Microsoft C runtime.
    pub(super) fn umask(mask: u32) -> u32 {
        unsafe { c_umask(mask as c_int) as u32 }
    }

    #[link(name = "ws2_32")]
    unsafe extern "system" {
        /// Initializes the requested Winsock API version for this process.
        fn WSAStartup(version: u16, data: *mut WsaData) -> c_int;
        /// Writes the local hostname into the caller-owned buffer.
        fn gethostname(name: *mut c_char, length: c_int) -> c_int;
        /// Reverse-resolves one network address through Winsock's resolver database.
        fn gethostbyaddr(address: *const c_char, length: c_int, kind: c_int) -> *mut HostEnt;
        /// Looks up one protocol database entry by name or alias.
        fn getprotobyname(name: *const c_char) -> *mut ProtoEnt;
        /// Looks up one protocol database entry by numeric identifier.
        fn getprotobynumber(number: c_int) -> *mut ProtoEnt;
    }

    #[link(name = "ntdll")]
    unsafe extern "system" {
        /// Reads the real Windows version independently of application manifests.
        fn RtlGetVersion(version: *mut OsVersionInfo) -> i32;
    }

    #[link(name = "msvcrt")]
    unsafe extern "C" {
        /// Sets the Microsoft C runtime file-creation mask.
        #[link_name = "_umask"]
        fn c_umask(mask: c_int) -> c_int;
    }
}
