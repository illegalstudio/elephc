//! Purpose:
//! Optional C ABI bridge for elephc's runtime `eval()` support.
//! Exposes the stable entry points linked only into programs that use eval.
//!
//! Called from:
//! - Generated EIR backend assembly through `__elephc_eval_*` symbols.
//! - `cargo test -p elephc-eval` for ABI-shape validation.
//!
//! Key details:
//! - No Rust panic or Rust-specific enum crosses the ABI boundary.
//! - Non-test builds execute the base EvalIR subset through generated runtime
//!   value wrappers; crate unit tests keep a controlled stub because they do not
//!   link the generated runtime assembly object.

pub mod abi;
pub mod context;
pub mod errors;
pub mod eval_ir;
pub mod interpreter;
pub mod lower;
pub mod parser;
pub mod runtime_hooks;
pub mod scope;
pub mod value;

use abi::{
    ElephcEvalContext, ElephcEvalResult, ElephcEvalScope, ABI_VERSION, SCOPE_FLAG_BY_REF,
    SCOPE_FLAG_DIRTY, SCOPE_FLAG_OWNED, SCOPE_FLAG_PRESENT, SCOPE_FLAG_UNSET,
};
use context::{NativeFunction, NativeFunctionInvoker};
use errors::EvalStatus;
#[cfg(not(test))]
use runtime_hooks::ElephcRuntimeOps;
use scope::{ScopeCellOwnership, ScopeEntry};
use std::ffi::c_void;
use std::slice;
use value::{RuntimeCell, RuntimeCellHandle};

#[cfg(not(test))]
unsafe extern "C" {
    fn __elephc_eval_value_release(value: *mut RuntimeCell);
}

/// Returns the ABI version expected by generated elephc eval call sites.
#[no_mangle]
pub extern "C" fn __elephc_eval_abi_version() -> u32 {
    ABI_VERSION
}

/// Allocates a process-level eval context handle for generated code.
#[no_mangle]
pub extern "C" fn __elephc_eval_context_new() -> *mut ElephcEvalContext {
    Box::into_raw(Box::new(ElephcEvalContext::new()))
}

/// Frees a process-level eval context handle allocated by the eval bridge.
///
/// # Safety
/// `ctx` must be null or a pointer returned by `__elephc_eval_context_new`
/// that has not already been freed.
#[no_mangle]
pub unsafe extern "C" fn __elephc_eval_context_free(ctx: *mut ElephcEvalContext) {
    if !ctx.is_null() {
        drop(Box::from_raw(ctx));
    }
}

/// Records source metadata for the next eval fragment executed in this context.
///
/// # Safety
/// `ctx` must be a valid eval context handle. `file_ptr` and `dir_ptr` must be
/// readable for their matching lengths when the length is greater than zero.
#[no_mangle]
pub unsafe extern "C" fn __elephc_eval_context_set_call_site(
    ctx: *mut ElephcEvalContext,
    file_ptr: *const u8,
    file_len: u64,
    dir_ptr: *const u8,
    dir_len: u64,
    line: u64,
) -> i32 {
    std::panic::catch_unwind(|| unsafe {
        eval_context_set_call_site_inner(ctx, file_ptr, file_len, dir_ptr, dir_len, line)
    })
    .unwrap_or_else(|_| EvalStatus::RuntimeFatal.code())
}

/// Records the materialized program-global eval scope for `global` aliases.
///
/// # Safety
/// `ctx` and `scope` must be valid handles allocated by the eval bridge. The
/// context does not own `scope`; generated code must keep the scope alive for
/// as long as the context can execute eval fragments that reference globals.
#[no_mangle]
pub unsafe extern "C" fn __elephc_eval_context_set_global_scope(
    ctx: *mut ElephcEvalContext,
    scope: *mut ElephcEvalScope,
) -> i32 {
    std::panic::catch_unwind(|| unsafe { eval_context_set_global_scope_inner(ctx, scope) })
        .unwrap_or_else(|_| EvalStatus::RuntimeFatal.code())
}

/// Allocates a materialized activation scope handle for generated code.
#[no_mangle]
pub extern "C" fn __elephc_eval_scope_new() -> *mut ElephcEvalScope {
    Box::into_raw(Box::new(ElephcEvalScope::new()))
}

/// Frees a materialized activation scope handle allocated by the eval bridge.
///
/// # Safety
/// `scope` must be null or a pointer returned by `__elephc_eval_scope_new`
/// that has not already been freed.
#[no_mangle]
pub unsafe extern "C" fn __elephc_eval_scope_free(scope: *mut ElephcEvalScope) {
    if !scope.is_null() {
        let mut scope = Box::from_raw(scope);
        release_owned_scope_cells(&mut scope);
        drop(scope);
    }
}

/// Stores a named runtime cell in a materialized eval scope.
///
/// # Safety
/// `scope` must be a valid eval scope handle. `name_ptr` must be readable for
/// `name_len` bytes when `name_len > 0`; names must be UTF-8 variable names.
#[no_mangle]
pub unsafe extern "C" fn __elephc_eval_scope_set(
    scope: *mut ElephcEvalScope,
    name_ptr: *const u8,
    name_len: u64,
    cell: *mut RuntimeCell,
    flags: u32,
) -> i32 {
    let Some(scope) = scope.as_mut() else {
        return EvalStatus::RuntimeFatal.code();
    };
    let Ok(name) = abi_name_to_string(name_ptr, name_len) else {
        return EvalStatus::RuntimeFatal.code();
    };
    let ownership = if flags & SCOPE_FLAG_OWNED != 0 {
        ScopeCellOwnership::Owned
    } else {
        ScopeCellOwnership::Borrowed
    };
    if let Some(replaced) = scope.set(name, RuntimeCellHandle::from_raw(cell), ownership) {
        release_scope_cell(replaced);
    }
    EvalStatus::Ok.code()
}

/// Looks up a named runtime cell in a materialized eval scope.
///
/// # Safety
/// `scope` must be a valid eval scope handle. `name_ptr` must be readable for
/// `name_len` bytes when `name_len > 0`. Output pointers may be null.
#[no_mangle]
pub unsafe extern "C" fn __elephc_eval_scope_get(
    scope: *const ElephcEvalScope,
    name_ptr: *const u8,
    name_len: u64,
    out_cell: *mut *mut RuntimeCell,
    out_flags: *mut u32,
) -> i32 {
    let Some(scope) = scope.as_ref() else {
        return EvalStatus::RuntimeFatal.code();
    };
    let Ok(name) = abi_name_to_string(name_ptr, name_len) else {
        return EvalStatus::RuntimeFatal.code();
    };
    let entry = scope.entry(&name);
    if !out_cell.is_null() {
        *out_cell = entry
            .filter(|entry| entry.flags().is_visible())
            .map(|entry| entry.cell().as_ptr())
            .unwrap_or(std::ptr::null_mut());
    }
    if !out_flags.is_null() {
        *out_flags = entry.map(scope_entry_abi_flags).unwrap_or(0);
    }
    EvalStatus::Ok.code()
}

/// Marks a named runtime cell as unset in a materialized eval scope.
///
/// # Safety
/// `scope` must be a valid eval scope handle. `name_ptr` must be readable for
/// `name_len` bytes when `name_len > 0`; names must be UTF-8 variable names.
#[no_mangle]
pub unsafe extern "C" fn __elephc_eval_scope_unset(
    scope: *mut ElephcEvalScope,
    name_ptr: *const u8,
    name_len: u64,
) -> i32 {
    let Some(scope) = scope.as_mut() else {
        return EvalStatus::RuntimeFatal.code();
    };
    let Ok(name) = abi_name_to_string(name_ptr, name_len) else {
        return EvalStatus::RuntimeFatal.code();
    };
    if let Some(replaced) = scope.unset(name) {
        release_scope_cell(replaced);
    }
    EvalStatus::Ok.code()
}

/// Clears dirty flags for every entry in a materialized eval scope.
///
/// # Safety
/// `scope` must be a valid eval scope handle allocated by the eval bridge.
#[no_mangle]
pub unsafe extern "C" fn __elephc_eval_scope_clear_dirty(scope: *mut ElephcEvalScope) -> i32 {
    let Some(scope) = scope.as_mut() else {
        return EvalStatus::RuntimeFatal.code();
    };
    scope.mark_all_clean();
    EvalStatus::Ok.code()
}

/// Executes an eval fragment against a materialized caller scope.
///
/// The FFI shape is final for the initial bridge: context/scope are opaque
/// runtime handles, `code_ptr`/`code_len` identify the PHP fragment bytes, and
/// `out` receives the eval return cell when provided. Non-test builds execute
/// the current EvalIR subset; test builds return `UnsupportedConstruct` because
/// they do not link elephc's generated runtime value wrappers.
///
/// # Safety
/// Callers must pass valid pointers for any non-null handle and ensure
/// `code_ptr` is readable for `code_len` bytes when `code_len > 0`.
#[no_mangle]
pub unsafe extern "C" fn __elephc_eval_execute(
    ctx: *mut ElephcEvalContext,
    scope: *mut ElephcEvalScope,
    code_ptr: *const u8,
    code_len: u64,
    out: *mut ElephcEvalResult,
) -> i32 {
    std::panic::catch_unwind(|| unsafe { execute_eval_inner(ctx, scope, code_ptr, code_len, out) })
        .unwrap_or_else(|_| EvalStatus::RuntimeFatal.code())
}

/// Checks whether a function was previously declared through `eval()`.
///
/// # Safety
/// `ctx` must be null or a valid eval context handle. `name_ptr` must be
/// readable for `name_len` bytes when `name_len > 0`.
#[no_mangle]
pub unsafe extern "C" fn __elephc_eval_function_exists(
    ctx: *const ElephcEvalContext,
    name_ptr: *const u8,
    name_len: u64,
) -> i32 {
    std::panic::catch_unwind(|| unsafe { eval_function_exists_inner(ctx, name_ptr, name_len) })
        .unwrap_or(0)
}

/// Registers a generated native PHP function callback in an eval context.
///
/// # Safety
/// `ctx` must be a valid eval context handle. `name_ptr` must be readable for
/// `name_len` bytes when `name_len > 0`. `descriptor` and `invoker` must follow
/// the descriptor-invoker ABI emitted by generated code.
#[no_mangle]
pub unsafe extern "C" fn __elephc_eval_register_native_function(
    ctx: *mut ElephcEvalContext,
    name_ptr: *const u8,
    name_len: u64,
    descriptor: *mut c_void,
    invoker: Option<NativeFunctionInvoker>,
    param_count: u64,
) -> i32 {
    std::panic::catch_unwind(|| unsafe {
        register_native_function_inner(ctx, name_ptr, name_len, descriptor, invoker, param_count)
    })
    .unwrap_or(0)
}

/// Registers one generated native PHP function parameter name in an eval context.
///
/// # Safety
/// `ctx` must be a valid eval context handle. Function and parameter name
/// pointers must be readable for their declared byte lengths.
#[no_mangle]
pub unsafe extern "C" fn __elephc_eval_register_native_function_param(
    ctx: *mut ElephcEvalContext,
    function_name_ptr: *const u8,
    function_name_len: u64,
    param_index: u64,
    param_name_ptr: *const u8,
    param_name_len: u64,
) -> i32 {
    std::panic::catch_unwind(|| unsafe {
        register_native_function_param_inner(
            ctx,
            function_name_ptr,
            function_name_len,
            param_index,
            param_name_ptr,
            param_name_len,
        )
    })
    .unwrap_or(0)
}

/// Calls a zero-argument function previously declared through `eval()`.
///
/// # Safety
/// `ctx` must be a valid eval context handle. `name_ptr` must be readable for
/// `name_len` bytes when `name_len > 0`, and `out` may be null.
#[cfg(not(test))]
#[no_mangle]
pub unsafe extern "C" fn __elephc_eval_call_function_zero_args(
    ctx: *mut ElephcEvalContext,
    name_ptr: *const u8,
    name_len: u64,
    out: *mut ElephcEvalResult,
) -> i32 {
    std::panic::catch_unwind(|| unsafe {
        call_eval_function_inner(ctx, name_ptr, name_len, std::ptr::null(), 0, out)
    })
    .unwrap_or_else(|_| EvalStatus::RuntimeFatal.code())
}

/// Calls a function previously declared through `eval()` with positional cells.
///
/// # Safety
/// `ctx` must be a valid eval context handle. `name_ptr` must be readable for
/// `name_len` bytes when `name_len > 0`. `args` must be readable for
/// `arg_count` runtime-cell pointers when `arg_count > 0`, and `out` may be null.
#[cfg(not(test))]
#[no_mangle]
pub unsafe extern "C" fn __elephc_eval_call_function(
    ctx: *mut ElephcEvalContext,
    name_ptr: *const u8,
    name_len: u64,
    args: *const *mut RuntimeCell,
    arg_count: u64,
    out: *mut ElephcEvalResult,
) -> i32 {
    std::panic::catch_unwind(|| unsafe {
        call_eval_function_inner(ctx, name_ptr, name_len, args, arg_count, out)
    })
    .unwrap_or_else(|_| EvalStatus::RuntimeFatal.code())
}

/// Calls a function previously declared through `eval()` with an argument array/hash.
///
/// # Safety
/// `ctx` must be a valid eval context handle. `name_ptr` must be readable for
/// `name_len` bytes when `name_len > 0`. `arg_array` must be a boxed Mixed
/// indexed or associative array cell, and `out` may be null.
#[cfg(not(test))]
#[no_mangle]
pub unsafe extern "C" fn __elephc_eval_call_function_array(
    ctx: *mut ElephcEvalContext,
    name_ptr: *const u8,
    name_len: u64,
    arg_array: *mut RuntimeCell,
    out: *mut ElephcEvalResult,
) -> i32 {
    std::panic::catch_unwind(|| unsafe {
        call_eval_function_array_inner(ctx, name_ptr, name_len, arg_array, out)
    })
    .unwrap_or_else(|_| EvalStatus::RuntimeFatal.code())
}

/// Runs the eval function-exists ABI body after installing a panic boundary.
///
/// # Safety
/// Mirrors `__elephc_eval_function_exists`; invalid handles or unreadable name
/// storage fail closed as `false`.
unsafe fn eval_function_exists_inner(
    ctx: *const ElephcEvalContext,
    name_ptr: *const u8,
    name_len: u64,
) -> i32 {
    let Some(context) = ctx.as_ref() else {
        return 0;
    };
    if context.abi_version() != ABI_VERSION {
        return 0;
    }
    let Ok(name) = abi_name_to_string(name_ptr, name_len) else {
        return 0;
    };
    i32::from(context.has_function(&name.to_ascii_lowercase()))
}

/// Runs the native registration ABI body after installing a panic boundary.
///
/// # Safety
/// Mirrors `__elephc_eval_register_native_function`; invalid handles, names, or
/// callback pointers fail closed as `false`.
unsafe fn register_native_function_inner(
    ctx: *mut ElephcEvalContext,
    name_ptr: *const u8,
    name_len: u64,
    descriptor: *mut c_void,
    invoker: Option<NativeFunctionInvoker>,
    param_count: u64,
) -> i32 {
    let Some(context) = ctx.as_mut() else {
        return 0;
    };
    if context.abi_version() != ABI_VERSION || descriptor.is_null() {
        return 0;
    }
    let Some(invoker) = invoker else {
        return 0;
    };
    let Ok(name) = abi_name_to_string(name_ptr, name_len) else {
        return 0;
    };
    let Ok(param_count) = usize::try_from(param_count) else {
        return 0;
    };
    let function = NativeFunction::new(descriptor, invoker, param_count);
    i32::from(
        context
            .define_native_function(name.to_ascii_lowercase(), function)
            .is_ok(),
    )
}

/// Runs the native parameter-name registration ABI body after installing a panic boundary.
///
/// # Safety
/// Mirrors `__elephc_eval_register_native_function_param`; invalid handles,
/// names, or indexes fail closed as `false`.
unsafe fn register_native_function_param_inner(
    ctx: *mut ElephcEvalContext,
    function_name_ptr: *const u8,
    function_name_len: u64,
    param_index: u64,
    param_name_ptr: *const u8,
    param_name_len: u64,
) -> i32 {
    let Some(context) = ctx.as_mut() else {
        return 0;
    };
    if context.abi_version() != ABI_VERSION {
        return 0;
    }
    let Ok(function_name) = abi_name_to_string(function_name_ptr, function_name_len) else {
        return 0;
    };
    let Ok(param_name) = abi_name_to_string(param_name_ptr, param_name_len) else {
        return 0;
    };
    let Ok(param_index) = usize::try_from(param_index) else {
        return 0;
    };
    i32::from(context.define_native_function_param(
        &function_name.to_ascii_lowercase(),
        param_index,
        param_name,
    ))
}

/// Runs the call-site metadata setter ABI body after installing a panic boundary.
///
/// # Safety
/// Mirrors `__elephc_eval_context_set_call_site`; callers must pass a valid
/// context and readable UTF-8 file/directory byte slices.
unsafe fn eval_context_set_call_site_inner(
    ctx: *mut ElephcEvalContext,
    file_ptr: *const u8,
    file_len: u64,
    dir_ptr: *const u8,
    dir_len: u64,
    line: u64,
) -> i32 {
    let Some(context) = ctx.as_mut() else {
        return EvalStatus::RuntimeFatal.code();
    };
    if context.abi_version() != ABI_VERSION {
        return EvalStatus::AbiMismatch.code();
    }
    let Ok(file) = abi_name_to_string(file_ptr, file_len) else {
        return EvalStatus::RuntimeFatal.code();
    };
    let Ok(dir) = abi_name_to_string(dir_ptr, dir_len) else {
        return EvalStatus::RuntimeFatal.code();
    };
    let Ok(line) = i64::try_from(line) else {
        return EvalStatus::RuntimeFatal.code();
    };
    context.set_call_site(file, dir, line);
    EvalStatus::Ok.code()
}

/// Runs the global-scope setter ABI body after installing a panic boundary.
///
/// # Safety
/// Mirrors `__elephc_eval_context_set_global_scope`; callers must pass valid
/// context and scope handles owned by generated code.
unsafe fn eval_context_set_global_scope_inner(
    ctx: *mut ElephcEvalContext,
    scope: *mut ElephcEvalScope,
) -> i32 {
    let Some(context) = ctx.as_mut() else {
        return EvalStatus::RuntimeFatal.code();
    };
    if context.abi_version() != ABI_VERSION {
        return EvalStatus::AbiMismatch.code();
    }
    if !context.set_global_scope(scope) {
        return EvalStatus::RuntimeFatal.code();
    }
    EvalStatus::Ok.code()
}

/// Runs the dynamic function-call ABI body after installing a panic boundary.
///
/// # Safety
/// Mirrors `__elephc_eval_call_function`; callers must provide a valid context,
/// readable function-name bytes, and readable argument pointer storage.
#[cfg(not(test))]
unsafe fn call_eval_function_inner(
    ctx: *mut ElephcEvalContext,
    name_ptr: *const u8,
    name_len: u64,
    args: *const *mut RuntimeCell,
    arg_count: u64,
    out: *mut ElephcEvalResult,
) -> i32 {
    let Some(context) = ctx.as_mut() else {
        return EvalStatus::RuntimeFatal.code();
    };
    if context.abi_version() != ABI_VERSION {
        return EvalStatus::AbiMismatch.code();
    }
    let Ok(name) = abi_name_to_string(name_ptr, name_len) else {
        return EvalStatus::RuntimeFatal.code();
    };
    let Ok(arg_count) = usize::try_from(arg_count) else {
        return EvalStatus::RuntimeFatal.code();
    };
    if arg_count > 0 && args.is_null() {
        return EvalStatus::RuntimeFatal.code();
    }
    let args = if arg_count == 0 {
        Vec::new()
    } else {
        slice::from_raw_parts(args, arg_count)
            .iter()
            .map(|arg| RuntimeCellHandle::from_raw(*arg))
            .collect()
    };
    if !out.is_null() {
        (*out).clear();
    }
    let mut values = ElephcRuntimeOps::new();
    match interpreter::execute_context_function(
        context,
        &name.to_ascii_lowercase(),
        args,
        &mut values,
    ) {
        Ok(result) => {
            if !out.is_null() {
                (*out).kind = 0;
                (*out).value_cell = result.as_ptr();
                (*out).error = std::ptr::null_mut();
            }
            EvalStatus::Ok.code()
        }
        Err(status) => status.code(),
    }
}

/// Runs the dynamic function-call-array ABI body after installing a panic boundary.
///
/// # Safety
/// Mirrors `__elephc_eval_call_function_array`; callers must provide a valid
/// context, readable function-name bytes, and a boxed array/hash argument cell.
#[cfg(not(test))]
unsafe fn call_eval_function_array_inner(
    ctx: *mut ElephcEvalContext,
    name_ptr: *const u8,
    name_len: u64,
    arg_array: *mut RuntimeCell,
    out: *mut ElephcEvalResult,
) -> i32 {
    let Some(context) = ctx.as_mut() else {
        return EvalStatus::RuntimeFatal.code();
    };
    if context.abi_version() != ABI_VERSION {
        return EvalStatus::AbiMismatch.code();
    }
    let Ok(name) = abi_name_to_string(name_ptr, name_len) else {
        return EvalStatus::RuntimeFatal.code();
    };
    if arg_array.is_null() {
        return EvalStatus::RuntimeFatal.code();
    }
    if !out.is_null() {
        (*out).clear();
    }
    let mut values = ElephcRuntimeOps::new();
    match interpreter::execute_context_function_call_array(
        context,
        &name.to_ascii_lowercase(),
        RuntimeCellHandle::from_raw(arg_array),
        &mut values,
    ) {
        Ok(result) => {
            if !out.is_null() {
                (*out).kind = 0;
                (*out).value_cell = result.as_ptr();
                (*out).error = std::ptr::null_mut();
            }
            EvalStatus::Ok.code()
        }
        Err(status) => status.code(),
    }
}

/// Runs the eval ABI body after the exported wrapper has installed a panic boundary.
///
/// # Safety
/// Mirrors `__elephc_eval_execute`; callers must provide valid handles and code
/// storage for every non-null pointer argument.
unsafe fn execute_eval_inner(
    ctx: *mut ElephcEvalContext,
    scope: *mut ElephcEvalScope,
    code_ptr: *const u8,
    code_len: u64,
    out: *mut ElephcEvalResult,
) -> i32 {
    if !ctx.is_null() && (*ctx).abi_version() != ABI_VERSION {
        return EvalStatus::AbiMismatch.code();
    }
    if code_len > 0 && code_ptr.is_null() {
        return EvalStatus::RuntimeFatal.code();
    }
    let Ok(code_len) = usize::try_from(code_len) else {
        return EvalStatus::RuntimeFatal.code();
    };
    let code = if code_len == 0 {
        &[]
    } else {
        slice::from_raw_parts(code_ptr, code_len)
    };
    let Ok(program) = parser::parse_fragment(code) else {
        return EvalStatus::ParseError.code();
    };
    if !out.is_null() {
        (*out).clear();
    }
    execute_parsed_eval(ctx, scope, &program, out)
}

/// Executes a parsed eval program in production builds using elephc runtime hooks.
///
/// # Safety
/// `scope` and `out` must be null or valid pointers supplied by generated code.
#[cfg(not(test))]
unsafe fn execute_parsed_eval(
    ctx: *mut ElephcEvalContext,
    scope: *mut ElephcEvalScope,
    program: &eval_ir::EvalProgram,
    out: *mut ElephcEvalResult,
) -> i32 {
    let mut fallback_context;
    let context = if let Some(ctx) = ctx.as_mut() {
        ctx
    } else {
        fallback_context = ElephcEvalContext::new();
        &mut fallback_context
    };
    let mut fallback_scope;
    let scope = if let Some(scope) = scope.as_mut() {
        scope
    } else {
        fallback_scope = ElephcEvalScope::new();
        &mut fallback_scope
    };
    let mut values = ElephcRuntimeOps::new();
    match interpreter::execute_program_with_context(context, program, scope, &mut values) {
        Ok(result) => {
            if !out.is_null() {
                (*out).kind = 0;
                (*out).value_cell = result.as_ptr();
                (*out).error = std::ptr::null_mut();
            }
            EvalStatus::Ok.code()
        }
        Err(status) => status.code(),
    }
}

/// Keeps crate unit tests independent from generated runtime assembly wrappers.
///
/// # Safety
/// `out` must be null or valid result storage supplied by the test caller.
#[cfg(test)]
unsafe fn execute_parsed_eval(
    _ctx: *mut ElephcEvalContext,
    _scope: *mut ElephcEvalScope,
    _program: &eval_ir::EvalProgram,
    _out: *mut ElephcEvalResult,
) -> i32 {
    EvalStatus::UnsupportedConstruct.code()
}

/// Converts an ABI name byte slice into an owned Rust string.
fn abi_name_to_string(name_ptr: *const u8, name_len: u64) -> Result<String, EvalStatus> {
    if name_len > 0 && name_ptr.is_null() {
        return Err(EvalStatus::RuntimeFatal);
    }
    let name_len = usize::try_from(name_len).map_err(|_| EvalStatus::RuntimeFatal)?;
    let bytes = if name_len == 0 {
        &[]
    } else {
        unsafe { slice::from_raw_parts(name_ptr, name_len) }
    };
    std::str::from_utf8(bytes)
        .map(|name| name.to_string())
        .map_err(|_| EvalStatus::RuntimeFatal)
}

/// Converts internal scope-entry flags into stable ABI bit flags.
fn scope_entry_abi_flags(entry: ScopeEntry) -> u32 {
    let flags = entry.flags();
    let mut abi_flags = 0;
    if flags.present {
        abi_flags |= SCOPE_FLAG_PRESENT;
    }
    if flags.unset {
        abi_flags |= SCOPE_FLAG_UNSET;
    }
    if flags.dirty {
        abi_flags |= SCOPE_FLAG_DIRTY;
    }
    if flags.by_ref {
        abi_flags |= SCOPE_FLAG_BY_REF;
    }
    if flags.ownership == ScopeCellOwnership::Owned {
        abi_flags |= SCOPE_FLAG_OWNED;
    }
    abi_flags
}

/// Releases every owned cell currently held by a scope.
fn release_owned_scope_cells(scope: &mut ElephcEvalScope) {
    for cell in scope.drain_owned_cells() {
        release_scope_cell(cell);
    }
}

/// Releases one scope-owned runtime cell through the generated runtime wrapper.
fn release_scope_cell(cell: RuntimeCellHandle) {
    #[cfg(not(test))]
    unsafe {
        __elephc_eval_value_release(cell.as_ptr());
    }
    #[cfg(test)]
    {
        let _ = cell;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Test native invoker placeholder used only to validate ABI registration.
    unsafe extern "C" fn fake_native_invoker(
        _descriptor: *mut c_void,
        _args: *mut RuntimeCell,
    ) -> *mut RuntimeCell {
        std::ptr::null_mut()
    }

    /// Verifies the exported version entry point reports the crate ABI constant.
    #[test]
    fn abi_version_matches_constant() {
        assert_eq!(__elephc_eval_abi_version(), ABI_VERSION);
    }

    /// Verifies the initial execute stub clears result storage and returns the
    /// documented unsupported status instead of panicking or succeeding.
    #[test]
    fn execute_stub_returns_unsupported_and_clears_result() {
        let mut result = ElephcEvalResult {
            kind: 99,
            value_cell: 1usize as *mut std::ffi::c_void,
            error: 2usize as *mut std::ffi::c_void,
        };
        let status = unsafe {
            __elephc_eval_execute(
                std::ptr::null_mut(),
                std::ptr::null_mut(),
                b"$x = 1;".as_ptr(),
                7,
                &mut result,
            )
        };
        assert_eq!(status, EvalStatus::UnsupportedConstruct.code());
        assert_eq!(result.kind, 0);
        assert!(result.value_cell.is_null());
        assert!(result.error.is_null());
    }

    /// Verifies context allocation returns a current-version opaque handle.
    #[test]
    fn context_new_returns_current_version_handle() {
        let ctx = __elephc_eval_context_new();
        assert!(!ctx.is_null());
        let version = unsafe { (*ctx).abi_version() };
        unsafe {
            __elephc_eval_context_free(ctx);
        }
        assert_eq!(version, ABI_VERSION);
    }

    /// Verifies call-site metadata can be set through the stable context ABI.
    #[test]
    fn context_set_call_site_records_file_dir_and_line() {
        let mut ctx = ElephcEvalContext::new();
        let file = b"/tmp/source.php";
        let dir = b"/tmp";

        let status = unsafe {
            __elephc_eval_context_set_call_site(
                &mut ctx,
                file.as_ptr(),
                file.len() as u64,
                dir.as_ptr(),
                dir.len() as u64,
                9,
            )
        };

        assert_eq!(status, EvalStatus::Ok.code());
        assert_eq!(ctx.call_dir(), "/tmp");
        assert_eq!(ctx.eval_file_magic(), "/tmp/source.php(9) : eval()'d code");
    }

    /// Verifies the context ABI records a non-owned global scope handle.
    #[test]
    fn context_set_global_scope_records_handle() {
        let mut ctx = ElephcEvalContext::new();
        let mut scope = ElephcEvalScope::new();

        let status = unsafe { __elephc_eval_context_set_global_scope(&mut ctx, &mut scope) };

        assert_eq!(status, EvalStatus::Ok.code());
        assert_eq!(ctx.global_scope_ptr(), Some(&mut scope as *mut ElephcEvalScope));
    }

    /// Verifies the function-exists ABI probes eval-declared functions by folded name.
    #[test]
    fn function_exists_reports_declared_eval_function() {
        let mut ctx = ElephcEvalContext::new();
        ctx.define_function(
            "dyn_probe",
            crate::eval_ir::EvalFunction::new("dyn_probe", Vec::new(), Vec::new()),
        )
        .expect("first dynamic function declaration should succeed");
        let existing = b"DYN_PROBE";
        let missing = b"missing";

        let existing_result = unsafe {
            __elephc_eval_function_exists(&ctx, existing.as_ptr(), existing.len() as u64)
        };
        let missing_result =
            unsafe { __elephc_eval_function_exists(&ctx, missing.as_ptr(), missing.len() as u64) };

        assert_eq!(existing_result, 1);
        assert_eq!(missing_result, 0);
    }

    /// Verifies native AOT registration records function and parameter metadata.
    #[test]
    fn register_native_function_reports_function_exists() {
        let mut ctx = ElephcEvalContext::new();
        let name = b"NATIVE_PROBE";
        let param = b"value";
        let descriptor = 1usize as *mut c_void;

        let registered = unsafe {
            __elephc_eval_register_native_function(
                &mut ctx,
                name.as_ptr(),
                name.len() as u64,
                descriptor,
                Some(fake_native_invoker),
                1,
            )
        };
        let param_registered = unsafe {
            __elephc_eval_register_native_function_param(
                &mut ctx,
                name.as_ptr(),
                name.len() as u64,
                0,
                param.as_ptr(),
                param.len() as u64,
            )
        };
        let exists = unsafe { __elephc_eval_function_exists(&ctx, b"native_probe".as_ptr(), 12) };

        assert_eq!(registered, 1);
        let native = ctx
            .native_function("native_probe")
            .expect("native function should be registered");

        assert_eq!(param_registered, 1);
        assert_eq!(exists, 1);
        assert_eq!(native.param_names(), &["value".to_string()]);
    }

    /// Verifies scope allocation returns an empty opaque activation scope handle.
    #[test]
    fn scope_new_returns_empty_handle() {
        let scope = __elephc_eval_scope_new();
        assert!(!scope.is_null());
        let generation = unsafe { (*scope).generation() };
        unsafe {
            __elephc_eval_scope_free(scope);
        }
        assert_eq!(generation, 0);
    }

    /// Verifies execute rejects contexts whose ABI version no longer matches.
    #[test]
    fn execute_rejects_mismatched_context_version() {
        let mut ctx = ElephcEvalContext::for_abi_version(ABI_VERSION + 1);
        let status = unsafe {
            __elephc_eval_execute(
                &mut ctx,
                std::ptr::null_mut(),
                std::ptr::null(),
                0,
                std::ptr::null_mut(),
            )
        };

        assert_eq!(status, EvalStatus::AbiMismatch.code());
    }

    /// Verifies execute maps invalid eval fragments to the stable parse status.
    #[test]
    fn execute_rejects_php_opening_tags_as_parse_errors() {
        let code = b"<?php echo 1;";
        let status = unsafe {
            __elephc_eval_execute(
                std::ptr::null_mut(),
                std::ptr::null_mut(),
                code.as_ptr(),
                code.len() as u64,
                std::ptr::null_mut(),
            )
        };

        assert_eq!(status, EvalStatus::ParseError.code());
    }

    /// Verifies scope set/get expose runtime-cell handles and dirty flags through the ABI.
    #[test]
    fn scope_set_get_round_trips_cell_and_flags() {
        let scope = __elephc_eval_scope_new();
        let name = b"x";
        let cell = 1usize as *mut RuntimeCell;
        let set_status = unsafe {
            __elephc_eval_scope_set(
                scope,
                name.as_ptr(),
                name.len() as u64,
                cell,
                SCOPE_FLAG_OWNED,
            )
        };
        let mut out_cell = std::ptr::null_mut();
        let mut out_flags = 0;
        let get_status = unsafe {
            __elephc_eval_scope_get(
                scope,
                name.as_ptr(),
                name.len() as u64,
                &mut out_cell,
                &mut out_flags,
            )
        };
        unsafe {
            __elephc_eval_scope_free(scope);
        }

        assert_eq!(set_status, EvalStatus::Ok.code());
        assert_eq!(get_status, EvalStatus::Ok.code());
        assert_eq!(out_cell, cell);
        assert_eq!(out_flags & SCOPE_FLAG_PRESENT, SCOPE_FLAG_PRESENT);
        assert_eq!(out_flags & SCOPE_FLAG_DIRTY, SCOPE_FLAG_DIRTY);
        assert_eq!(out_flags & SCOPE_FLAG_OWNED, SCOPE_FLAG_OWNED);
    }

    /// Verifies scope unset and clear-dirty expose missing/clean state through the ABI.
    #[test]
    fn scope_unset_and_clear_dirty_update_flags() {
        let scope = __elephc_eval_scope_new();
        let name = b"x";
        let cell = 1usize as *mut RuntimeCell;
        unsafe {
            __elephc_eval_scope_set(
                scope,
                name.as_ptr(),
                name.len() as u64,
                cell,
                SCOPE_FLAG_OWNED,
            );
            __elephc_eval_scope_clear_dirty(scope);
            __elephc_eval_scope_unset(scope, name.as_ptr(), name.len() as u64);
        }
        let mut out_cell = cell;
        let mut out_flags = 0;
        let get_status = unsafe {
            __elephc_eval_scope_get(
                scope,
                name.as_ptr(),
                name.len() as u64,
                &mut out_cell,
                &mut out_flags,
            )
        };
        unsafe {
            __elephc_eval_scope_free(scope);
        }

        assert_eq!(get_status, EvalStatus::Ok.code());
        assert!(out_cell.is_null());
        assert_eq!(out_flags & SCOPE_FLAG_UNSET, SCOPE_FLAG_UNSET);
        assert_eq!(out_flags & SCOPE_FLAG_DIRTY, SCOPE_FLAG_DIRTY);
        assert_eq!(out_flags & SCOPE_FLAG_PRESENT, 0);
    }
}
