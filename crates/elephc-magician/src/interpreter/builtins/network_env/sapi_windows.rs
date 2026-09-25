//! Purpose:
//! Implements the Windows-only `sapi_windows_*` eval surface.
//!
//! Called from:
//! - `crate::interpreter::builtins::network_env` direct and evaluated-value dispatch.
//!
//! Key details:
//! - The registry hides this module's names on non-Windows hosts, matching php-src's
//!   `PHP_WIN32` declarations; Windows code paths use kernel32 code-page APIs directly.
//! - Control-handler registration is deliberately conservative until eval can retain a callable
//!   across the asynchronous Win32 callback boundary; it returns the native failure result.

use super::*;
#[cfg(target_os = "windows")]
use crate::context::{pcntl_runtime, EvalPcntlSignalHandler};
#[cfg(target_os = "windows")]
use std::sync::atomic::{AtomicI32, Ordering};

#[cfg(target_os = "windows")]
static CTRL_EVENT: AtomicI32 = AtomicI32::new(-1);

#[cfg(target_os = "windows")]
unsafe extern "system" fn sapi_windows_ctrl_handler(event: u32) -> i32 {
    if event <= 1 {
        CTRL_EVENT.store(event as i32, Ordering::Release);
        1
    } else {
        0
    }
}

eval_builtin! {
    contract: "sapi_windows_vt100_support",
    area: NetworkEnv,
    direct: NetworkEnv,
    values: NetworkEnv,
}
eval_builtin! {
    contract: "sapi_windows_cp_set",
    area: NetworkEnv,
    direct: NetworkEnv,
    values: NetworkEnv,
}
eval_builtin! {
    contract: "sapi_windows_cp_get",
    area: NetworkEnv,
    direct: NetworkEnv,
    values: NetworkEnv,
}
eval_builtin! {
    contract: "sapi_windows_cp_conv",
    area: NetworkEnv,
    direct: NetworkEnv,
    values: NetworkEnv,
}
eval_builtin! {
    contract: "sapi_windows_cp_is_utf8",
    area: NetworkEnv,
    direct: NetworkEnv,
    values: NetworkEnv,
}
eval_builtin! {
    contract: "sapi_windows_set_ctrl_handler",
    area: NetworkEnv,
    direct: NetworkEnv,
    values: NetworkEnv,
}
eval_builtin! {
    contract: "sapi_windows_generate_ctrl_event",
    area: NetworkEnv,
    direct: NetworkEnv,
    values: NetworkEnv,
}

/// Evaluates one Windows SAPI builtin over source expressions.
pub(in crate::interpreter) fn eval_builtin_sapi_windows_call(
    name: &str,
    args: &[EvalExpr],
    context: &mut ElephcEvalContext,
    scope: &mut ElephcEvalScope,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    let mut evaluated = Vec::with_capacity(args.len());
    for arg in args {
        evaluated.push(eval_expr(arg, context, scope, values)?);
    }
    eval_sapi_windows_values_result(name, &evaluated, context, values)
}

/// Evaluates one Windows SAPI builtin over already materialized arguments.
pub(in crate::interpreter) fn eval_sapi_windows_values_result(
    name: &str,
    args: &[RuntimeCellHandle],
    context: &mut ElephcEvalContext,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    #[cfg(not(target_os = "windows"))]
    {
        let _ = (name, args, context, values);
        return Err(EvalStatus::UnsupportedConstruct);
    }

    #[cfg(target_os = "windows")]
    {
        match name {
            "sapi_windows_cp_set" => {
                let [codepage] = args else { return Err(EvalStatus::RuntimeFatal); };
                let codepage = eval_int_value(*codepage, values)?;
                if !(0..=u32::MAX as i64).contains(&codepage) {
                    return eval_throw_builtin_value_error(
                        "sapi_windows_cp_set(): Argument #1 ($codepage) must be between 0 and 4294967295",
                        context,
                        values,
                    );
                }
                let codepage = codepage as u32;
                if elephc_builtin_contract::windows_codepages::windows_codepage_by_id(codepage)
                    .is_none()
                {
                    values.warning(&format!(
                        "Warning: sapi_windows_cp_set(): Failed to switch to codepage {codepage}\n"
                    ))?;
                    return values.bool_value(false);
                }
                let ok = unsafe {
                    SetConsoleCP(codepage) != 0 && SetConsoleOutputCP(codepage) != 0
                };
                if !ok {
                    values.warning(&format!(
                        "Warning: sapi_windows_cp_set(): Failed to switch to codepage {codepage}\n"
                    ))?;
                }
                return values.bool_value(ok);
            }
            "sapi_windows_cp_get" => {
                let kind_bytes = match args {
                    [] => Vec::new(),
                    [kind] => values.string_bytes(*kind)?,
                    _ => return Err(EvalStatus::RuntimeFatal),
                };
                let kind = std::str::from_utf8(&kind_bytes).unwrap_or("");
                let value = if kind.eq_ignore_ascii_case("ansi") {
                    unsafe { GetACP() }
                } else if kind.eq_ignore_ascii_case("oem") {
                    unsafe { GetOEMCP() }
                } else {
                    unsafe { GetConsoleOutputCP() }
                };
                return values.int(i64::from(value));
            }
            "sapi_windows_cp_is_utf8" => {
                return values.bool_value(unsafe { GetConsoleOutputCP() } == CP_UTF8);
            }
            "sapi_windows_cp_conv" => {
                let [input, output, subject] = args else {
                    return Err(EvalStatus::RuntimeFatal);
                };
                let input = resolve_codepage_argument(
                    *input,
                    1,
                    "in_codepage",
                    context,
                    values,
                )?;
                let output = resolve_codepage_argument(
                    *output,
                    2,
                    "out_codepage",
                    context,
                    values,
                )?;
                let subject = values.string_bytes(*subject)?;
                return match convert_codepage(input, output, &subject) {
                    Some(converted) => values.string_bytes_value(&converted),
                    None => {
                        values.warning("Warning: sapi_windows_cp_conv(): Wide char conversion failed\n")?;
                        values.null()
                    }
                };
            }
            "sapi_windows_vt100_support" => {
                let (stream, enable) = match args {
                    [stream] => (*stream, None),
                    [stream, enable] if values.is_null(*enable)? => (*stream, None),
                    [stream, enable] => (*stream, Some(values.truthy(*enable)?)),
                    _ => return Err(EvalStatus::RuntimeFatal),
                };
                let id = eval_stream_resource_id(stream, values)?;
                let Some(handle) = context.stream_resources().windows_raw_handle(id) else {
                    return values.bool_value(false);
                };
                let mut mode = 0_u32;
                if unsafe { GetConsoleMode(handle as *mut std::ffi::c_void, &mut mode) } == 0 {
                    return values.bool_value(false);
                }
                let Some(enable) = enable else {
                    return values.bool_value(mode & 4 != 0);
                };
                let new_mode = if enable { mode | 4 } else { mode & !4 };
                return values.bool_value(
                    unsafe { SetConsoleMode(handle as *mut std::ffi::c_void, new_mode) } != 0,
                );
            }
            "sapi_windows_set_ctrl_handler" | "sapi_windows_generate_ctrl_event" => {
                if name == "sapi_windows_set_ctrl_handler" {
                    let (handler, add) = match args {
                        [handler] => (*handler, true),
                        [handler, add] => (*handler, values.truthy(*add)?),
                        _ => return Err(EvalStatus::RuntimeFatal),
                    };
                    if values.is_null(handler)? {
                        let ok = unsafe {
                            SetConsoleCtrlHandler(std::ptr::null(), add as i32) != 0
                        };
                        if ok {
                            if let Some(previous) = pcntl_runtime::replace_signal_handler(
                                0,
                                std::ptr::null_mut(),
                                EvalPcntlSignalHandler::Disposition(0),
                            ) {
                                if let EvalPcntlSignalHandler::Callable(previous_handler) = previous.handler {
                                    values.release(previous_handler)?;
                                }
                            }
                        }
                        return values.bool_value(ok);
                    }
                    let _ = eval_callable(handler, context, values)?;
                    let retained = values.retain(handler)?;
                    let reset_ok = unsafe { SetConsoleCtrlHandler(std::ptr::null(), 0) != 0 };
                    let installed = reset_ok && unsafe {
                        SetConsoleCtrlHandler(sapi_windows_ctrl_handler as *const (), add as i32)
                            != 0
                    };
                    if !installed {
                        values.release(retained)?;
                        return values.bool_value(false);
                    }
                    if let Some(previous) = pcntl_runtime::replace_signal_handler(
                        0,
                        context as *mut ElephcEvalContext,
                        EvalPcntlSignalHandler::Callable(retained),
                    ) {
                        let previous_context = previous.context;
                        if let EvalPcntlSignalHandler::Callable(previous_handler) = previous.handler {
                            values.release(previous_handler)?;
                        }
                        if pcntl_runtime::take_collectable_context(
                            previous_context,
                        ) {
                            unsafe {
                                crate::ffi::context::drop_eval_context_now(previous_context);
                            }
                        }
                    }
                    return values.bool_value(true);
                }
                let ([event] | [event, _]) = args else {
                    return Err(EvalStatus::RuntimeFatal);
                };
                let event = eval_int_value(*event, values)?;
                let pid = args.get(1).map(|value| eval_int_value(*value, values)).transpose()?.unwrap_or(0);
                let had_handler = matches!(
                    pcntl_runtime::signal_handler(0),
                    Some(entry) if matches!(entry.handler, EvalPcntlSignalHandler::Callable(_))
                );
                let disabled = unsafe { SetConsoleCtrlHandler(std::ptr::null(), 1) != 0 };
                if !disabled {
                    return values.bool_value(false);
                }
                let generated = unsafe { GenerateConsoleCtrlEvent(event as u32, pid as u32) != 0 };
                let restored = !had_handler || unsafe {
                    SetConsoleCtrlHandler(sapi_windows_ctrl_handler as *const (), 1) != 0
                };
                return values.bool_value(generated && restored);
            }
            _ => return Err(EvalStatus::UnsupportedConstruct),
        }
    }
}

/// Drains one queued Windows CTRL event at the interpreter's ordinary statement safe point.
pub(in crate::interpreter) fn eval_sapi_windows_maybe_dispatch(
    context: &mut ElephcEvalContext,
    values: &mut impl RuntimeValueOps,
) -> Result<(), EvalStatus> {
    #[cfg(not(target_os = "windows"))]
    {
        let _ = (context, values);
        return Ok(());
    }
    #[cfg(target_os = "windows")]
    {
        let event = CTRL_EVENT.swap(-1, Ordering::Acquire);
        if event < 0 {
            return Ok(());
        }
        let Some(entry) = pcntl_runtime::begin_handler_dispatch(0) else {
            return Ok(());
        };
        let EvalPcntlSignalHandler::Callable(handler) = entry.handler else {
            return Ok(());
        };
        let callback = values.retain(handler)?;
        let event = values.int(i64::from(event))?;
        let current_context = context as *mut ElephcEvalContext;
        let owner_context = if entry.context.is_null() { current_context } else { entry.context };
        let result = unsafe {
            crate::interpreter::eval_call_user_func_with_values(
                vec![callback, event],
                &mut *owner_context,
                values,
            )
        };
        let result = match result {
            Ok(result) => values.release(result),
            Err(status) => Err(status),
        };
        values.release(callback)?;
        if pcntl_runtime::end_handler_dispatch(entry.context) {
            unsafe { crate::ffi::context::drop_eval_context_now(entry.context) };
        }
        result
    }
}

#[cfg(target_os = "windows")]
const CP_UTF8: u32 = 65_001;

#[cfg(target_os = "windows")]
unsafe extern "system" {
    fn GetACP() -> u32;
    fn GetOEMCP() -> u32;
    fn GetConsoleOutputCP() -> u32;
    fn GetConsoleMode(handle: *mut std::ffi::c_void, mode: *mut u32) -> i32;
    fn SetConsoleMode(handle: *mut std::ffi::c_void, mode: u32) -> i32;
    fn SetConsoleCtrlHandler(handler: *const (), add: i32) -> i32;
    fn GenerateConsoleCtrlEvent(event: u32, process_group_id: u32) -> i32;
    fn SetConsoleCP(code_page: u32) -> i32;
    fn SetConsoleOutputCP(code_page: u32) -> i32;
    fn MultiByteToWideChar(code_page: u32, flags: u32, input: *const u8, input_len: i32, output: *mut u16, output_len: i32) -> i32;
    fn WideCharToMultiByte(code_page: u32, flags: u32, input: *const u16, input_len: i32, output: *mut u8, output_len: i32, default_char: *const u8, used_default: *mut i32) -> i32;
}

#[cfg(target_os = "windows")]
fn resolve_codepage_argument(
    value: RuntimeCellHandle,
    argument: usize,
    parameter_name: &str,
    context: &mut ElephcEvalContext,
    values: &mut impl RuntimeValueOps,
) -> Result<u32, EvalStatus> {
    if values.type_tag(value)? == EVAL_TAG_INT {
        let value = eval_int_value(value, values)?;
        let id = match u32::try_from(value) {
            Ok(id) => id,
            Err(_) => {
                return eval_throw_builtin_value_error(
                    &format!(
                        "sapi_windows_cp_conv(): Argument #{argument} (${parameter_name}) must be between 0 and 4294967295"
                    ),
                    context,
                    values,
                )
            }
        };
        return elephc_builtin_contract::windows_codepages::windows_codepage_by_id(id)
            .map(|entry| entry.id)
            .ok_or_else(|| EvalStatus::RuntimeFatal)
            .or_else(|_| {
                eval_throw_builtin_value_error(
                    &format!(
                        "sapi_windows_cp_conv(): Argument #{argument} (${parameter_name}) must be a valid codepage"
                    ),
                    context,
                    values,
                )
            });
    }
    let encoded_name = values.string_bytes(value)?;
    let encoded_name = std::str::from_utf8(&encoded_name).unwrap_or("");
    elephc_builtin_contract::windows_codepages::windows_codepage_by_name(encoded_name)
        .map(|entry| entry.id)
        .ok_or(EvalStatus::RuntimeFatal)
        .or_else(|_| {
            eval_throw_builtin_value_error(
                &format!(
                    "sapi_windows_cp_conv(): Argument #{argument} (${parameter_name}) must be a valid charset"
                ),
                context,
                values,
            )
        })
}

#[cfg(target_os = "windows")]
fn convert_codepage(input: u32, output: u32, subject: &[u8]) -> Option<Vec<u8>> {
    let input_len = i32::try_from(subject.len()).ok()?;
    let input_flags = if matches!(input, 54_936 | 65_001) { 8 } else { 0 };
    let output_flags = if matches!(output, 54_936 | 65_001) { 128 } else { 0 };
    let wide_len = unsafe { MultiByteToWideChar(input, input_flags, subject.as_ptr(), input_len, std::ptr::null_mut(), 0) };
    if wide_len <= 0 { return None; }
    let mut wide = vec![0u16; wide_len as usize];
    let written = unsafe { MultiByteToWideChar(input, input_flags, subject.as_ptr(), input_len, wide.as_mut_ptr(), wide_len) };
    if written <= 0 { return None; }
    let out_len = unsafe { WideCharToMultiByte(output, output_flags, wide.as_ptr(), written, std::ptr::null_mut(), 0, std::ptr::null(), std::ptr::null_mut()) };
    if out_len <= 0 { return None; }
    let mut result = vec![0u8; out_len as usize];
    let written = unsafe { WideCharToMultiByte(output, output_flags, wide.as_ptr(), written, result.as_mut_ptr(), out_len, std::ptr::null(), std::ptr::null_mut()) };
    (written > 0).then_some(result)
}
