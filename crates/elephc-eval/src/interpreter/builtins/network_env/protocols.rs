//! Purpose:
//! Implements protocol and service database lookup eval builtins.
//!
//! Called from:
//! - `crate::interpreter::builtins::network_env` re-exports.
//!
//! Key details:
//! - PHP values are converted to lowercase NUL-free C strings before libc lookup
//!   and returned database names are copied into runtime strings.

use super::super::super::*;
use super::super::*;

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
