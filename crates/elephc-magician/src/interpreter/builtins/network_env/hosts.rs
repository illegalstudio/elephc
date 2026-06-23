//! Purpose:
//! Implements host-name lookup eval builtins.
//!
//! Called from:
//! - `crate::interpreter::builtins::network_env` re-exports.
//!
//! Key details:
//! - Host resolver results from libc are copied immediately because they point at
//!   process-global static storage.

use super::super::super::*;

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
