//! Purpose:
//! Network lookup, IP conversion, environment, and realpath-cache builtins.
//!
//! Called from:
//! - `crate::interpreter::builtins` re-exports used by core call dispatch.
//!
//! Key details:
//! - Helpers stay scoped to the eval interpreter and preserve PHP-visible runtime
//!   behavior through `RuntimeValueOps`.

use super::super::*;
use super::*;

/// Evaluates PHP `gethostbyaddr($ip)` over one eval expression.
pub(in crate::interpreter) fn eval_builtin_gethostbyaddr(
    args: &[EvalExpr],
    context: &mut ElephcEvalContext,
    scope: &mut ElephcEvalScope,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    let [ip] = args else {
        return Err(EvalStatus::RuntimeFatal);
    };
    let ip = eval_expr(ip, context, scope, values)?;
    eval_gethostbyaddr_result(ip, values)
}

/// Reverse-resolves one IPv4 address, returns the input on miss, or PHP false when malformed.
pub(in crate::interpreter) fn eval_gethostbyaddr_result(
    ip: RuntimeCellHandle,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    let ip_bytes = values.string_bytes(ip)?;
    let ip_text = String::from_utf8_lossy(&ip_bytes);
    let Ok(ipv4) = ip_text.parse::<std::net::Ipv4Addr>() else {
        return values.bool_value(false);
    };
    let octets = ipv4.octets();
    let resolved = unsafe {
        // libc reads the stack-owned IPv4 octets during this call and returns
        // static resolver storage, which is copied before the next resolver call.
        let host = libc_gethostbyaddr(
            octets.as_ptr().cast::<libc::c_void>(),
            octets.len() as libc::socklen_t,
            libc::AF_INET,
        );
        if host.is_null() || (*host).h_name.is_null() {
            None
        } else {
            Some(CStr::from_ptr((*host).h_name).to_bytes().to_vec())
        }
    };
    match resolved {
        Some(name) if !name.is_empty() => values.string_bytes_value(&name),
        _ => values.string(ip_text.as_ref()),
    }
}

/// Evaluates PHP `gethostbyname($hostname)` over one eval expression.
pub(in crate::interpreter) fn eval_builtin_gethostbyname(
    args: &[EvalExpr],
    context: &mut ElephcEvalContext,
    scope: &mut ElephcEvalScope,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    let [hostname] = args else {
        return Err(EvalStatus::RuntimeFatal);
    };
    let hostname = eval_expr(hostname, context, scope, values)?;
    eval_gethostbyname_result(hostname, values)
}

/// Resolves one host name to an IPv4 string, or returns the original input on failure.
pub(in crate::interpreter) fn eval_gethostbyname_result(
    hostname: RuntimeCellHandle,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    let hostname = values.string_bytes(hostname)?;
    let hostname = String::from_utf8_lossy(&hostname);
    if hostname.parse::<std::net::Ipv4Addr>().is_ok() {
        return values.string(hostname.as_ref());
    }
    let resolved = (hostname.as_ref(), 0_u16)
        .to_socket_addrs()
        .ok()
        .and_then(|addrs| {
            addrs
                .filter_map(|addr| match addr.ip() {
                    std::net::IpAddr::V4(ip) => Some(ip.to_string()),
                    std::net::IpAddr::V6(_) => None,
                })
                .next()
        });
    values.string(resolved.as_deref().unwrap_or_else(|| hostname.as_ref()))
}

/// Evaluates PHP `gethostname()` over one eval expression.
pub(in crate::interpreter) fn eval_builtin_gethostname(
    args: &[EvalExpr],
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    let [] = args else {
        return Err(EvalStatus::RuntimeFatal);
    };
    eval_gethostname_result(values)
}

/// Reads the current host name through libc and returns an empty string on failure.
pub(in crate::interpreter) fn eval_gethostname_result(
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    let mut buffer = [0 as libc::c_char; 256];
    let status = unsafe {
        // libc writes at most buffer.len() bytes into this stack buffer.
        libc::gethostname(buffer.as_mut_ptr(), buffer.len())
    };
    if status != 0 {
        return values.string("");
    }
    let length = buffer
        .iter()
        .position(|byte| *byte == 0)
        .unwrap_or(buffer.len());
    let hostname = buffer[..length]
        .iter()
        .map(|byte| *byte as u8)
        .collect::<Vec<_>>();
    values.string_bytes_value(&hostname)
}

/// Evaluates PHP `getprotobyname($protocol)` over one eval expression.
pub(in crate::interpreter) fn eval_builtin_getprotobyname(
    args: &[EvalExpr],
    context: &mut ElephcEvalContext,
    scope: &mut ElephcEvalScope,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    let [protocol] = args else {
        return Err(EvalStatus::RuntimeFatal);
    };
    let protocol = eval_expr(protocol, context, scope, values)?;
    eval_getprotobyname_result(protocol, values)
}

/// Looks up an IP protocol number by name or alias.
pub(in crate::interpreter) fn eval_getprotobyname_result(
    protocol: RuntimeCellHandle,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    let Some(protocol) = eval_lowercase_c_string(protocol, values)? else {
        return values.bool_value(false);
    };
    let entry = unsafe {
        // libc returns a process-global protoent; copy scalar fields before another lookup.
        libc_getprotobyname(protocol.as_ptr())
    };
    if entry.is_null() {
        return values.bool_value(false);
    }
    let number = unsafe { (*entry).p_proto };
    values.int(i64::from(number))
}

/// Evaluates PHP `getprotobynumber($protocol)` over one eval expression.
pub(in crate::interpreter) fn eval_builtin_getprotobynumber(
    args: &[EvalExpr],
    context: &mut ElephcEvalContext,
    scope: &mut ElephcEvalScope,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    let [protocol] = args else {
        return Err(EvalStatus::RuntimeFatal);
    };
    let protocol = eval_expr(protocol, context, scope, values)?;
    eval_getprotobynumber_result(protocol, values)
}

/// Looks up an IP protocol name by numeric protocol id.
pub(in crate::interpreter) fn eval_getprotobynumber_result(
    protocol: RuntimeCellHandle,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    let protocol = eval_int_value(protocol, values)?;
    let Ok(protocol) = libc::c_int::try_from(protocol) else {
        return values.bool_value(false);
    };
    let entry = unsafe {
        // libc returns a process-global protoent; copy the name before another lookup.
        libc_getprotobynumber(protocol)
    };
    eval_protoent_name_or_false(entry, values)
}

/// Evaluates PHP `getservbyname($service, $protocol)` over two eval expressions.
pub(in crate::interpreter) fn eval_builtin_getservbyname(
    args: &[EvalExpr],
    context: &mut ElephcEvalContext,
    scope: &mut ElephcEvalScope,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    let [service, protocol] = args else {
        return Err(EvalStatus::RuntimeFatal);
    };
    let service = eval_expr(service, context, scope, values)?;
    let protocol = eval_expr(protocol, context, scope, values)?;
    eval_getservbyname_result(service, protocol, values)
}

/// Looks up an internet service port by service name and protocol.
pub(in crate::interpreter) fn eval_getservbyname_result(
    service: RuntimeCellHandle,
    protocol: RuntimeCellHandle,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    let Some(service) = eval_lowercase_c_string(service, values)? else {
        return values.bool_value(false);
    };
    let Some(protocol) = eval_lowercase_c_string(protocol, values)? else {
        return values.bool_value(false);
    };
    let entry = unsafe {
        // libc returns a process-global servent; copy scalar fields before another lookup.
        libc_getservbyname(service.as_ptr(), protocol.as_ptr())
    };
    if entry.is_null() {
        return values.bool_value(false);
    }
    let port = unsafe { u16::from_be((*entry).s_port as u16) };
    values.int(i64::from(port))
}

/// Evaluates PHP `getservbyport($port, $protocol)` over two eval expressions.
pub(in crate::interpreter) fn eval_builtin_getservbyport(
    args: &[EvalExpr],
    context: &mut ElephcEvalContext,
    scope: &mut ElephcEvalScope,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    let [port, protocol] = args else {
        return Err(EvalStatus::RuntimeFatal);
    };
    let port = eval_expr(port, context, scope, values)?;
    let protocol = eval_expr(protocol, context, scope, values)?;
    eval_getservbyport_result(port, protocol, values)
}

/// Looks up an internet service name by port and protocol.
pub(in crate::interpreter) fn eval_getservbyport_result(
    port: RuntimeCellHandle,
    protocol: RuntimeCellHandle,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    let port = eval_int_value(port, values)?;
    let Ok(port) = u16::try_from(port) else {
        return values.bool_value(false);
    };
    let Some(protocol) = eval_lowercase_c_string(protocol, values)? else {
        return values.bool_value(false);
    };
    let network_port = port.to_be() as libc::c_int;
    let entry = unsafe {
        // libc returns a process-global servent; copy the name before another lookup.
        libc_getservbyport(network_port, protocol.as_ptr())
    };
    eval_servent_name_or_false(entry, values)
}

/// Converts a PHP value to a NUL-free lowercase C string for libc database lookups.
pub(in crate::interpreter) fn eval_lowercase_c_string(
    value: RuntimeCellHandle,
    values: &mut impl RuntimeValueOps,
) -> Result<Option<CString>, EvalStatus> {
    let bytes = values.string_bytes(value)?;
    let bytes = bytes
        .into_iter()
        .map(|byte| byte.to_ascii_lowercase())
        .collect::<Vec<_>>();
    Ok(CString::new(bytes).ok())
}

/// Copies a protoent canonical name into a PHP string or returns PHP false.
pub(in crate::interpreter) fn eval_protoent_name_or_false(
    entry: *mut libc::protoent,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    if entry.is_null() {
        return values.bool_value(false);
    }
    let name = unsafe {
        let name = (*entry).p_name;
        if name.is_null() {
            return values.bool_value(false);
        }
        CStr::from_ptr(name).to_bytes().to_vec()
    };
    values.string_bytes_value(&name)
}

/// Copies a servent canonical name into a PHP string or returns PHP false.
pub(in crate::interpreter) fn eval_servent_name_or_false(
    entry: *mut libc::servent,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    if entry.is_null() {
        return values.bool_value(false);
    }
    let name = unsafe {
        let name = (*entry).s_name;
        if name.is_null() {
            return values.bool_value(false);
        }
        CStr::from_ptr(name).to_bytes().to_vec()
    };
    values.string_bytes_value(&name)
}

/// Evaluates PHP `long2ip($ip)` over one eval expression.
pub(in crate::interpreter) fn eval_builtin_long2ip(
    args: &[EvalExpr],
    context: &mut ElephcEvalContext,
    scope: &mut ElephcEvalScope,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    let [ip] = args else {
        return Err(EvalStatus::RuntimeFatal);
    };
    let ip = eval_expr(ip, context, scope, values)?;
    eval_long2ip_result(ip, values)
}

/// Formats one 32-bit IPv4 integer as a dotted-quad string.
pub(in crate::interpreter) fn eval_long2ip_result(
    ip: RuntimeCellHandle,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    let ip = eval_int_value(ip, values)? as u32;
    values.string(&eval_format_ipv4(ip))
}

/// Evaluates PHP `ip2long($ip)` over one eval expression.
pub(in crate::interpreter) fn eval_builtin_ip2long(
    args: &[EvalExpr],
    context: &mut ElephcEvalContext,
    scope: &mut ElephcEvalScope,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    let [ip] = args else {
        return Err(EvalStatus::RuntimeFatal);
    };
    let ip = eval_expr(ip, context, scope, values)?;
    eval_ip2long_result(ip, values)
}

/// Parses a dotted-quad IPv4 string into an integer or PHP false.
pub(in crate::interpreter) fn eval_ip2long_result(
    ip: RuntimeCellHandle,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    let bytes = values.string_bytes(ip)?;
    match eval_parse_ipv4(&bytes) {
        Some(ip) => values.int(i64::from(ip)),
        None => values.bool_value(false),
    }
}

/// Evaluates PHP `inet_pton($ip)` over one eval expression.
pub(in crate::interpreter) fn eval_builtin_inet_pton(
    args: &[EvalExpr],
    context: &mut ElephcEvalContext,
    scope: &mut ElephcEvalScope,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    let [ip] = args else {
        return Err(EvalStatus::RuntimeFatal);
    };
    let ip = eval_expr(ip, context, scope, values)?;
    eval_inet_pton_result(ip, values)
}

/// Packs a dotted-quad IPv4 string into four network-order bytes or PHP false.
pub(in crate::interpreter) fn eval_inet_pton_result(
    ip: RuntimeCellHandle,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    let bytes = values.string_bytes(ip)?;
    let Some(ip) = eval_parse_ipv4(&bytes) else {
        return values.bool_value(false);
    };
    values.string_bytes_value(&ip.to_be_bytes())
}

/// Evaluates PHP `inet_ntop($binary)` over one eval expression.
pub(in crate::interpreter) fn eval_builtin_inet_ntop(
    args: &[EvalExpr],
    context: &mut ElephcEvalContext,
    scope: &mut ElephcEvalScope,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    let [binary] = args else {
        return Err(EvalStatus::RuntimeFatal);
    };
    let binary = eval_expr(binary, context, scope, values)?;
    eval_inet_ntop_result(binary, values)
}

/// Renders a four-byte IPv4 string as dotted-quad text or PHP false.
pub(in crate::interpreter) fn eval_inet_ntop_result(
    binary: RuntimeCellHandle,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    let bytes = values.string_bytes(binary)?;
    let [a, b, c, d] = bytes.as_slice() else {
        return values.bool_value(false);
    };
    let ip = u32::from_be_bytes([*a, *b, *c, *d]);
    values.string(&eval_format_ipv4(ip))
}

/// Parses exactly four decimal IPv4 octets separated by dots.
pub(in crate::interpreter) fn eval_parse_ipv4(bytes: &[u8]) -> Option<u32> {
    let mut octets = [0_u8; 4];
    let mut position = 0_usize;
    let mut index = 0_usize;

    while index < 4 {
        if position >= bytes.len() {
            return None;
        }
        let start = position;
        let mut value = 0_u16;
        while position < bytes.len() && bytes[position].is_ascii_digit() {
            value = value
                .checked_mul(10)?
                .checked_add(u16::from(bytes[position] - b'0'))?;
            position += 1;
            if position - start > 3 || value > 255 {
                return None;
            }
        }
        if position == start {
            return None;
        }
        octets[index] = value as u8;
        index += 1;
        if index == 4 {
            return (position == bytes.len()).then(|| u32::from_be_bytes(octets));
        }
        if bytes.get(position).copied() != Some(b'.') {
            return None;
        }
        position += 1;
    }
    None
}

/// Formats one packed IPv4 integer into dotted-quad text.
pub(in crate::interpreter) fn eval_format_ipv4(ip: u32) -> String {
    let [a, b, c, d] = ip.to_be_bytes();
    format!("{}.{}.{}.{}", a, b, c, d)
}

/// Evaluates PHP `getenv($name)` over one eval expression.
pub(in crate::interpreter) fn eval_builtin_getenv(
    args: &[EvalExpr],
    context: &mut ElephcEvalContext,
    scope: &mut ElephcEvalScope,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    let [name] = args else {
        return Err(EvalStatus::RuntimeFatal);
    };
    let name = eval_expr(name, context, scope, values)?;
    eval_getenv_result(name, values)
}

/// Reads one environment variable and returns an empty string when it is unset.
pub(in crate::interpreter) fn eval_getenv_result(
    name: RuntimeCellHandle,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    let name = values.string_bytes(name)?;
    let name = String::from_utf8_lossy(&name);
    let value = std::env::var_os(name.as_ref())
        .map(|value| value.to_string_lossy().into_owned())
        .unwrap_or_default();
    values.string(&value)
}

/// Evaluates PHP `putenv($assignment)` over one eval expression.
pub(in crate::interpreter) fn eval_builtin_putenv(
    args: &[EvalExpr],
    context: &mut ElephcEvalContext,
    scope: &mut ElephcEvalScope,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    let [assignment] = args else {
        return Err(EvalStatus::RuntimeFatal);
    };
    let assignment = eval_expr(assignment, context, scope, values)?;
    eval_putenv_result(assignment, values)
}

/// Applies one `putenv()` assignment to the host environment.
pub(in crate::interpreter) fn eval_putenv_result(
    assignment: RuntimeCellHandle,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    let assignment = values.string_bytes(assignment)?;
    if let Some(separator) = assignment.iter().position(|byte| *byte == b'=') {
        let name = String::from_utf8_lossy(&assignment[..separator]);
        let value = String::from_utf8_lossy(&assignment[separator + 1..]);
        std::env::set_var(name.as_ref(), value.as_ref());
    } else {
        let name = String::from_utf8_lossy(&assignment);
        std::env::remove_var(name.as_ref());
    }
    values.bool_value(true)
}

/// Evaluates PHP `sys_get_temp_dir()` with no arguments.
pub(in crate::interpreter) fn eval_builtin_sys_get_temp_dir(
    args: &[EvalExpr],
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    if !args.is_empty() {
        return Err(EvalStatus::RuntimeFatal);
    }
    eval_sys_get_temp_dir_result(values)
}

/// Returns the same temporary directory literal as the native static builtin.
pub(in crate::interpreter) fn eval_sys_get_temp_dir_result(
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    values.string("/tmp")
}

/// Evaluates PHP `realpath_cache_get()` with no arguments.
pub(in crate::interpreter) fn eval_builtin_realpath_cache_get(
    args: &[EvalExpr],
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    if !args.is_empty() {
        return Err(EvalStatus::RuntimeFatal);
    }
    eval_realpath_cache_get_result(values)
}

/// Returns elephc's intentionally empty realpath-cache view.
pub(in crate::interpreter) fn eval_realpath_cache_get_result(
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    values.array_new(0)
}

/// Evaluates PHP `realpath_cache_size()` with no arguments.
pub(in crate::interpreter) fn eval_builtin_realpath_cache_size(
    args: &[EvalExpr],
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    if !args.is_empty() {
        return Err(EvalStatus::RuntimeFatal);
    }
    eval_realpath_cache_size_result(values)
}

/// Returns zero because elephc does not maintain a runtime realpath cache.
pub(in crate::interpreter) fn eval_realpath_cache_size_result(
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    values.int(0)
}
