//! Purpose:
//! Exports post-barrier construction and method calls for classes declared by
//! eval fragments. Generated code uses this ABI when native object operations
//! may target a dynamic eval class instead of an AOT class.
//!
//! Called from:
//! - Generated EIR backend assembly through `__elephc_eval_new_object`.
//! - Generated EIR backend assembly through `__elephc_eval_try_new_object`.
//! - Generated EIR backend assembly through `__elephc_eval_method_call`.
//! - Generated EIR backend assembly through `__elephc_eval_property_get`.
//! - Generated runtime assembly through `__elephc_eval_string_context`.
//! - Generated runtime assembly through `__elephc_eval_sprintf_numeric_warning`.
//! - Generated EIR backend assembly through `__elephc_eval_static_method_call`.
//!
//! Key details:
//! - The ABI currently accepts positional arguments already boxed as Mixed cells.
//! - Calls return either a value cell or an uncaught throwable cell through
//!   `ElephcEvalResult`.

use super::util::{abi_name_to_string, clear_result, write_outcome};
use crate::abi::{
    ElephcEvalContext, ElephcEvalResult, ABI_VERSION, EVAL_ARG_PACK_COLLECTOR_COUNTS,
    EVAL_ARG_PACK_SURPLUS_COLLECTOR,
};
use crate::context::native_frame_called_class_override_context;
use crate::errors::EvalStatus;
use crate::interpreter::{self, RuntimeValueOps};
use crate::runtime_hooks::ElephcRuntimeOps;
use crate::value::{RuntimeCell, RuntimeCellHandle};
use std::slice;

/// Constructs a class previously declared through `eval()` with positional cells.
///
/// # Safety
/// `ctx` must be a valid eval context handle. `name_ptr` must be readable for
/// `name_len` bytes when `name_len > 0`. `args` must be readable for
/// `arg_count` runtime-cell pointers when `arg_count > 0`, and `out` may be null.
#[cfg(not(test))]
#[no_mangle]
pub unsafe extern "C" fn __elephc_eval_new_object(
    ctx: *mut ElephcEvalContext,
    name_ptr: *const u8,
    name_len: u64,
    args: *const *mut RuntimeCell,
    arg_count: u64,
    out: *mut ElephcEvalResult,
) -> i32 {
    crate::ffi::util::trace_eval_ffi_entry("__elephc_eval_new_object");
    std::panic::catch_unwind(|| unsafe {
        eval_new_object_inner(ctx, name_ptr, name_len, args, arg_count, out)
    })
    .unwrap_or_else(|_| EvalStatus::RuntimeFatal.code())
}

/// Attempts to construct an eval-declared class with positional cells.
///
/// # Safety
/// `ctx` may be null to query request-global registered autoload contexts. `name_ptr` must be
/// readable for `name_len` bytes when `name_len > 0`. `args` must be readable for `arg_count`
/// runtime-cell pointers when `arg_count > 0`, and `out` may be null. Returns -1 when no
/// context can materialize the class name.
#[cfg(not(test))]
#[no_mangle]
pub unsafe extern "C" fn __elephc_eval_try_new_object(
    ctx: *mut ElephcEvalContext,
    name_ptr: *const u8,
    name_len: u64,
    args: *const *mut RuntimeCell,
    arg_count: u64,
    out: *mut ElephcEvalResult,
) -> i32 {
    crate::ffi::util::trace_eval_ffi_entry("__elephc_eval_try_new_object");
    std::panic::catch_unwind(|| unsafe {
        eval_try_new_object_inner(ctx, name_ptr, name_len, args, arg_count, out, None)
    })
    .unwrap_or_else(|_| EvalStatus::RuntimeFatal.code())
}

/// Allocates an eval-declared class WITHOUT running its constructor.
///
/// This is a compiled `ReflectionClass::newInstanceWithoutConstructor()` whose class no compiled
/// case matched. `ctx` may be null: the request-global autoload contexts are tried in turn, then a
/// fresh context importing the process-global classes. Returns -1 when no context declares it.
///
/// # Safety
/// `name_ptr` must be readable for `name_len` bytes when `name_len > 0`; `out` may be null.
#[cfg(not(test))]
#[no_mangle]
pub unsafe extern "C" fn __elephc_eval_try_new_object_without_constructor(
    ctx: *mut ElephcEvalContext,
    name_ptr: *const u8,
    name_len: u64,
    out: *mut ElephcEvalResult,
) -> i32 {
    crate::ffi::util::trace_eval_ffi_entry("__elephc_eval_try_new_object_without_constructor");
    std::panic::catch_unwind(|| unsafe {
        let Ok(name) = abi_name_to_string(name_ptr, name_len) else {
            return EvalStatus::RuntimeFatal.code();
        };
        clear_result(out);
        let try_context = |context: &mut ElephcEvalContext| -> Option<i32> {
            if context.abi_version() != ABI_VERSION {
                return Some(EvalStatus::AbiMismatch.code());
            }
            let mut values = ElephcRuntimeOps::with_context(context as *const ElephcEvalContext);
            match interpreter::execute_context_try_new_object_without_constructor_outcome(
                context,
                &name,
                &mut values,
            ) {
                Ok(Some(outcome)) => Some(write_outcome(outcome, out).code()),
                Ok(None) => None,
                Err(status) => Some(status.code()),
            }
        };
        if let Some(context) = ctx.as_mut() {
            return try_context(context).unwrap_or(-1);
        }
        for owner in crate::context::global_eval_autoload_contexts_snapshot() {
            let Some(context) = owner.as_mut() else {
                continue;
            };
            if let Some(status) = try_context(context) {
                return status;
            }
        }
        // Heap, for the reason `eval_try_new_object_from_global_autoload_contexts` gives: the new
        // object registers this context as its owner and reads it back when destroyed.
        let fallback_context = crate::ffi::context::__elephc_eval_context_new();
        let Some(context) = fallback_context.as_mut() else {
            return EvalStatus::RuntimeFatal.code();
        };
        crate::context::sync_global_eval_aot_metadata(context);
        let status = try_context(context).unwrap_or(-1);
        crate::ffi::context::__elephc_eval_context_free(fallback_context);
        status
    })
    .unwrap_or_else(|_| EvalStatus::RuntimeFatal.code())
}

/// Borrowed native construction metadata. All fields are native 64-bit words.
#[repr(C)]
pub struct ElephcEvalConstructionSite {
    pub arg_count: u64,
    pub file_ptr: *const u8,
    pub file_len: u64,
    pub line: i64,
    /// Non-zero when the single argument is the array a `...$args` spread produced, to be
    /// unpacked here with its string keys bound as named arguments.
    pub spread_container: u64,
}

/// Constructs with a source location even when the caller owns no eval context.
///
/// # Safety
/// Same pointer requirements as `__elephc_eval_try_new_object`; `site` must
/// reference a live metadata record and its file bytes must remain readable.
#[cfg(not(test))]
#[no_mangle]
pub unsafe extern "C" fn __elephc_eval_try_new_object_at(
    ctx: *mut ElephcEvalContext,
    name_ptr: *const u8,
    name_len: u64,
    args: *const *mut RuntimeCell,
    site: *const ElephcEvalConstructionSite,
    out: *mut ElephcEvalResult,
) -> i32 {
    std::panic::catch_unwind(|| unsafe {
        let Some(site) = site.as_ref() else { return EvalStatus::RuntimeFatal.code(); };
        let Ok(file) = abi_name_to_string(site.file_ptr, site.file_len) else {
            return EvalStatus::RuntimeFatal.code();
        };
        let source = (file, site.line);
        let spread = site.spread_container != 0 && site.arg_count == 1;
        eval_try_new_object_inner_spread(
            ctx,
            name_ptr,
            name_len,
            args,
            site.arg_count,
            out,
            Some(&source),
            spread,
        )
    }).unwrap_or_else(|_| EvalStatus::RuntimeFatal.code())
}

/// Applies a native call site to the selected context only for this construction.
///
/// With `spread`, `args` holds the one array a compiled `new $class(...$args)` produced; it is
/// unpacked here, string keys becoming named arguments, since only this side knows the class.
#[cfg(not(test))]
fn execute_native_new_at(
    context: &mut ElephcEvalContext,
    name: &str,
    args: Vec<RuntimeCellHandle>,
    values: &mut ElephcRuntimeOps,
    source: Option<&(String, i64)>,
    spread: bool,
) -> Result<Option<interpreter::EvalOutcome>, EvalStatus> {
    let args = match (spread, args.as_slice()) {
        (true, [container]) => interpreter::eval_unpack_constructor_arg_container(*container, values)?,
        _ => args.into_iter().map(|value| (None, value)).collect(),
    };
    let previous = source.map(|(file, line)| {
        let previous = context.call_site();
        let dir = std::path::Path::new(file).parent()
            .map(|path| path.to_string_lossy().into_owned()).unwrap_or_default();
        context.set_call_site(file.clone(), dir, *line);
        context.set_file_magic_override(Some(file.clone()));
        previous
    });
    // A COMPILED frame constructing an eval-declared class publishes its lexical class instead of
    // forwarding a scope; that class is the calling scope php judges the constructor from, so a
    // PROTECTED constructor (brick/math's `BigNumber` building a `BigInteger`) is reachable.
    let native_caller_class = context
        .current_class_scope()
        .is_none()
        .then(crate::context::current_native_caller_class)
        .flatten()
        .filter(|class_name| !class_name.is_empty());
    if let Some(class_name) = &native_caller_class {
        context.push_class_scope(class_name.clone());
    }
    let outcome = context.with_native_global_sync(|context| {
        interpreter::execute_context_try_new_object_named_outcome(context, name, args, values)
    });
    if native_caller_class.is_some() {
        context.pop_class_scope();
    }
    if let Some((file, dir, line, file_override)) = previous {
        context.set_call_site(file, dir, line);
        context.set_file_magic_override(file_override);
    }
    outcome
}

/// Calls a method on a value that may be an eval-created object.
///
/// # Safety
/// `ctx` may be null when the boxed object belongs to a dynamic class; in that
/// case its registered owner context is used. `object` must point at a boxed
/// runtime cell. `method_ptr` must be readable for `method_len` bytes when
/// `method_len > 0`. `arg_pack` points at a native-word count followed by that
/// many runtime-cell pointers, and `out` may be null. A null `ctx` with an AOT
/// object returns -1 so generated code can use its native dispatch fallback.
#[cfg(not(test))]
#[no_mangle]
pub unsafe extern "C" fn __elephc_eval_method_call(
    ctx: *mut ElephcEvalContext,
    object: *mut RuntimeCell,
    method_ptr: *const u8,
    method_len: u64,
    arg_pack: *const usize,
    out: *mut ElephcEvalResult,
) -> i32 {
    crate::ffi::util::trace_eval_ffi_entry("__elephc_eval_method_call");
    std::panic::catch_unwind(|| unsafe {
        eval_method_call_inner(ctx, object, method_ptr, method_len, arg_pack, out)
    })
    .unwrap_or_else(|_| EvalStatus::RuntimeFatal.code())
}

/// Reads a property from an object that is owned by the active eval context.
///
/// # Safety
/// `ctx` must be a live eval context. `object` must point at a boxed runtime cell,
/// `property_ptr` must be readable for `property_len` bytes, and `out` may be null.
#[cfg(not(test))]
#[no_mangle]
pub unsafe extern "C" fn __elephc_eval_property_get(
    ctx: *mut ElephcEvalContext,
    object: *mut RuntimeCell,
    property_ptr: *const u8,
    property_len: u64,
    out: *mut ElephcEvalResult,
) -> i32 {
    crate::ffi::util::trace_eval_ffi_entry("__elephc_eval_property_get");
    std::panic::catch_unwind(|| unsafe {
        eval_property_get_inner(ctx, object, property_ptr, property_len, out)
    })
    .unwrap_or_else(|_| EvalStatus::RuntimeFatal.code())
}

/// Applies eval-aware PHP string-context semantics to a boxed runtime value.
///
/// # Safety
/// `ctx` may be null when `value` is an eval-created object still registered with its
/// owning context. Otherwise it must be a valid eval context handle. `value` must point
/// at a boxed runtime cell, and `out` may be null.
#[cfg(not(test))]
#[no_mangle]
pub unsafe extern "C" fn __elephc_eval_string_context(
    ctx: *mut ElephcEvalContext,
    value: *mut RuntimeCell,
    out: *mut ElephcEvalResult,
) -> i32 {
    crate::ffi::util::trace_eval_ffi_entry("__elephc_eval_string_context");
    std::panic::catch_unwind(|| unsafe { eval_string_context_inner(ctx, value, out) })
        .unwrap_or_else(|_| EvalStatus::RuntimeFatal.code())
}

/// Emits the PHP warning for numeric conversion of an eval-created object.
///
/// # Safety
/// `ctx` may be null when `object` remains registered with its owning eval context.
/// `object` must be a live eval-created object payload. `is_float != 0` selects the
/// float wording; zero selects the integer wording.
#[cfg(not(test))]
#[no_mangle]
pub unsafe extern "C" fn __elephc_eval_sprintf_numeric_warning(
    ctx: *mut ElephcEvalContext,
    object: *mut RuntimeCell,
    is_float: u64,
) -> i32 {
    crate::ffi::util::trace_eval_ffi_entry("__elephc_eval_sprintf_numeric_warning");
    std::panic::catch_unwind(|| unsafe {
        eval_sprintf_numeric_warning_inner(ctx, object, is_float)
    })
    .unwrap_or_else(|_| EvalStatus::RuntimeFatal.code())
}

/// Calls a static method on a class previously declared through `eval()`.
///
/// # Safety
/// `ctx` must be a valid eval context handle. `target_ptr` must be readable
/// for `target_len` bytes and contain `ClassName::method`. `arg_pack` points at
/// a native-word count followed by that many runtime-cell pointers, and `out`
/// may be null.
#[cfg(not(test))]
#[no_mangle]
pub unsafe extern "C" fn __elephc_eval_static_method_call(
    ctx: *mut ElephcEvalContext,
    target_ptr: *const u8,
    target_len: u64,
    arg_pack: *const usize,
    out: *mut ElephcEvalResult,
) -> i32 {
    crate::ffi::util::trace_eval_ffi_entry("__elephc_eval_static_method_call");
    std::panic::catch_unwind(|| unsafe {
        eval_static_method_call_inner(ctx, target_ptr, target_len, arg_pack, out)
    })
    .unwrap_or_else(|_| EvalStatus::RuntimeFatal.code())
}

/// Calls a static method through the current eval late-static AOT-frame override.
///
/// # Safety
/// `frame_class_ptr` and `method_ptr` must be readable UTF-8 byte ranges.
/// `arg_pack` points at a native-word count followed by that many runtime-cell
/// pointers, and `out` may be null.
#[cfg(not(test))]
#[no_mangle]
pub unsafe extern "C" fn __elephc_eval_native_frame_static_method_call(
    frame_class_ptr: *const u8,
    frame_class_len: u64,
    method_ptr: *const u8,
    method_len: u64,
    arg_pack: *const usize,
    out: *mut ElephcEvalResult,
) -> i32 {
    crate::ffi::util::trace_eval_ffi_entry("__elephc_eval_native_frame_static_method_call");
    std::panic::catch_unwind(|| unsafe {
        eval_native_frame_static_method_call_inner(
            frame_class_ptr,
            frame_class_len,
            method_ptr,
            method_len,
            arg_pack,
            out,
        )
    })
    .unwrap_or_else(|_| EvalStatus::RuntimeFatal.code())
}

/// Runs the dynamic object-construction ABI body after installing a panic boundary.
///
/// # Safety
/// Mirrors `__elephc_eval_new_object`; callers must provide a valid context,
/// readable class-name bytes, and readable argument pointer storage.
#[cfg(not(test))]
unsafe fn eval_new_object_inner(
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
            .map(|arg| RuntimeCellHandle::from_raw(*arg).borrowed())
            .collect()
    };
    clear_result(out);
    let mut values = ElephcRuntimeOps::with_context(context as *const ElephcEvalContext);
    match context.with_native_global_sync(|context| {
        interpreter::execute_context_new_object_outcome(context, &name, args, &mut values)
    }) {
        Ok(outcome) => {
            if crate::eval_trace::enabled() {
                eprintln!("[elephc-eval-trace] phase=new_object_ok class={name:?}");
            }
            write_outcome(outcome, out).code()
        }
        Err(status) => {
            if crate::eval_trace::enabled() {
                eprintln!(
                    "[elephc-eval-trace] phase=new_object_error class={name:?} status={status:?}",
                );
            }
            status.code()
        }
    }
}

/// Tries retained request-global callback owners, then published AOT metadata, for null-context construction.
///
/// A dynamically constructed object stores its owning context for later method calls and
/// destruction, so dynamic classes must execute in the callback owner's context rather than a
/// temporary context. AOT classes have no dynamic owner and can safely use a metadata-only
/// fallback context after every retained owner declines the name.
#[cfg(not(test))]
unsafe fn eval_try_new_object_from_global_autoload_contexts(
    name: &str,
    args: &[RuntimeCellHandle],
    out: *mut ElephcEvalResult,
    source: Option<&(String, i64)>,
    spread: bool,
) -> i32 {
    for owner in crate::context::global_eval_autoload_contexts_snapshot() {
        let Some(context) = owner.as_mut() else {
            continue;
        };
        if context.abi_version() != ABI_VERSION {
            continue;
        }
        // An autoload owner may predate the published snapshot; a constructor bound without its
        // signature misses the hidden `func_get_args` collector and the bridge refuses it.
        crate::context::sync_global_eval_aot_metadata_when_empty(context);
        let mut values = ElephcRuntimeOps::with_context(context as *const ElephcEvalContext);
        match execute_native_new_at(
            context,
            name,
            args.to_vec(),
            &mut values,
            source,
            spread,
        ) {
            Ok(Some(outcome)) => {
                if crate::eval_trace::enabled() {
                    eprintln!("[elephc-eval-trace] phase=try_new_object_ok class={name:?}");
                }
                return write_outcome(outcome, out).code();
            }
            Ok(None) => {}
            Err(status) => {
                if crate::eval_trace::enabled() {
                    eprintln!(
                        "[elephc-eval-trace] phase=try_new_object_error class={name:?} status={status:?}",
                    );
                }
                return status.code();
            }
        }
    }

    // HEAP, not the stack. A newly constructed eval object REGISTERS its context as its owner
    // and reads that pointer back when it is destroyed -- which is long after this frame has
    // returned. A stack context therefore handed the destructor an address whose storage had
    // been reused: `dynamic_object_class()` then probed a HashMap made of stack garbage and
    // spun forever, which is what hung the first `new` of an eval class inside any compiled
    // method body. `__elephc_eval_context_free` defers the teardown while the object is alive,
    // so releasing it here is right whether or not the object retained it.
    let fallback_context = crate::ffi::context::__elephc_eval_context_new();
    let Some(context) = (unsafe { fallback_context.as_mut() }) else {
        return EvalStatus::RuntimeFatal.code();
    };
    crate::context::sync_global_eval_aot_metadata(context);
    let mut values = ElephcRuntimeOps::with_context(context as *const ElephcEvalContext);
    let status = match execute_native_new_at(context, name, args.to_vec(), &mut values, source, spread) {
        Ok(Some(outcome)) => {
            if crate::eval_trace::enabled() {
                eprintln!("[elephc-eval-trace] phase=try_new_object_ok class={name:?}");
            }
            write_outcome(outcome, out).code()
        }
        Ok(None) => -1,
        Err(status) => {
            if crate::eval_trace::enabled() {
                eprintln!(
                    "[elephc-eval-trace] phase=try_new_object_error class={name:?} status={status:?}",
                );
            }
            status.code()
        }
    };
    drop(values);
    unsafe { crate::ffi::context::__elephc_eval_context_free(fallback_context) };
    status
}

/// Runs the dynamic object-construction probe ABI body after installing a panic boundary.
///
/// # Safety
/// Mirrors `__elephc_eval_try_new_object`; the context may be null for request-global
/// autoload lookup, while class-name bytes and argument pointer storage must remain readable.
#[cfg(not(test))]
unsafe fn eval_try_new_object_inner(
    ctx: *mut ElephcEvalContext,
    name_ptr: *const u8,
    name_len: u64,
    args: *const *mut RuntimeCell,
    arg_count: u64,
    out: *mut ElephcEvalResult,
    source: Option<&(String, i64)>,
) -> i32 {
    eval_try_new_object_inner_spread(ctx, name_ptr, name_len, args, arg_count, out, source, false)
}

/// `eval_try_new_object_inner`, with `spread` marking a single `...$args` container argument.
#[cfg(not(test))]
unsafe fn eval_try_new_object_inner_spread(
    ctx: *mut ElephcEvalContext,
    name_ptr: *const u8,
    name_len: u64,
    args: *const *mut RuntimeCell,
    arg_count: u64,
    out: *mut ElephcEvalResult,
    source: Option<&(String, i64)>,
    spread: bool,
) -> i32 {
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
            .map(|arg| RuntimeCellHandle::from_raw(*arg).borrowed())
            .collect()
    };
    clear_result(out);
    let Some(context) = ctx.as_mut() else {
        return eval_try_new_object_from_global_autoload_contexts(&name, &args, out, source, spread);
    };
    if context.abi_version() != ABI_VERSION {
        return EvalStatus::AbiMismatch.code();
    }
    let mut values = ElephcRuntimeOps::with_context(context as *const ElephcEvalContext);
    let outcome = execute_native_new_at(context, &name, args, &mut values, source, spread);
    match outcome {
        Ok(Some(outcome)) => {
            if crate::eval_trace::enabled() {
                eprintln!("[elephc-eval-trace] phase=try_new_object_ok class={name:?}");
            }
            write_outcome(outcome, out).code()
        }
        Ok(None) => -1,
        Err(status) => {
            if crate::eval_trace::enabled() {
                eprintln!(
                    "[elephc-eval-trace] phase=try_new_object_error class={name:?} status={status:?}",
                );
            }
            status.code()
        }
    }
}

/// Runs the dynamic static-method call ABI body after installing a panic boundary.
///
/// # Safety
/// Mirrors `__elephc_eval_static_method_call`; callers must provide a valid
/// context, readable `ClassName::method` bytes, and a readable argument pack.
#[cfg(not(test))]
unsafe fn eval_static_method_call_inner(
    ctx: *mut ElephcEvalContext,
    target_ptr: *const u8,
    target_len: u64,
    arg_pack: *const usize,
    out: *mut ElephcEvalResult,
) -> i32 {
    let Some(context) = ctx.as_mut() else {
        return EvalStatus::RuntimeFatal.code();
    };
    if context.abi_version() != ABI_VERSION {
        return EvalStatus::AbiMismatch.code();
    }
    if arg_pack.is_null() {
        return EvalStatus::RuntimeFatal.code();
    }
    let Ok(target) = abi_name_to_string(target_ptr, target_len) else {
        return EvalStatus::RuntimeFatal.code();
    };
    let Some((class_name, method)) = target.rsplit_once("::") else {
        return EvalStatus::RuntimeFatal.code();
    };
    clear_result(out);
    let mut values = ElephcRuntimeOps::with_context(context as *const ElephcEvalContext);
    let (args, spliced) = match read_method_call_arg_pack(arg_pack, &mut values) {
        Ok(pack) => pack,
        Err(status) => return status.code(),
    };
    let outcome = interpreter::execute_context_static_method_call_outcome(
        context, class_name, method, args, &mut values,
    );
    for element in spliced {
        let _ = values.release(element);
    }
    match outcome {
        Ok(outcome) => write_outcome(outcome, out).code(),
        Err(status) => status.code(),
    }
}

/// Runs a late-static eval override call from a generated/AOT frame.
///
/// # Safety
/// Mirrors `__elephc_eval_native_frame_static_method_call`; callers must
/// provide readable frame/method names and a readable argument pack.
#[cfg(not(test))]
unsafe fn eval_native_frame_static_method_call_inner(
    frame_class_ptr: *const u8,
    frame_class_len: u64,
    method_ptr: *const u8,
    method_len: u64,
    arg_pack: *const usize,
    out: *mut ElephcEvalResult,
) -> i32 {
    if arg_pack.is_null() {
        return EvalStatus::RuntimeFatal.code();
    }
    let Ok(frame_class) = abi_name_to_string(frame_class_ptr, frame_class_len) else {
        return EvalStatus::RuntimeFatal.code();
    };
    let Ok(method) = abi_name_to_string(method_ptr, method_len) else {
        return EvalStatus::RuntimeFatal.code();
    };
    let Some((context, called_class)) = native_frame_called_class_override_context(&frame_class)
    else {
        return -1;
    };
    let Some(context) = context.as_mut() else {
        return EvalStatus::RuntimeFatal.code();
    };
    if context.abi_version() != ABI_VERSION {
        return EvalStatus::AbiMismatch.code();
    }
    clear_result(out);
    let mut values = ElephcRuntimeOps::with_context(context as *const ElephcEvalContext);
    let (args, spliced) = match read_method_call_arg_pack(arg_pack, &mut values) {
        Ok(pack) => pack,
        Err(status) => return status.code(),
    };
    let outcome = interpreter::execute_context_static_method_call_outcome(
        context,
        &called_class,
        &method,
        args,
        &mut values,
    );
    for element in spliced {
        let _ = values.release(element);
    }
    match outcome {
        Ok(outcome) => write_outcome(outcome, out).code(),
        Err(status) => status.code(),
    }
}

/// Runs the dynamic method-call ABI body after installing a panic boundary.
///
/// # Safety
/// Mirrors `__elephc_eval_method_call`; callers must provide a boxed object
/// cell, readable method-name bytes, and a readable argument pack. A null
/// context resolves through dynamic-object ownership or returns the miss sentinel.
#[cfg(not(test))]
unsafe fn eval_method_call_inner(
    ctx: *mut ElephcEvalContext,
    object: *mut RuntimeCell,
    method_ptr: *const u8,
    method_len: u64,
    arg_pack: *const usize,
    out: *mut ElephcEvalResult,
) -> i32 {
    if object.is_null() || arg_pack.is_null() {
        return EvalStatus::RuntimeFatal.code();
    }
    let object = RuntimeCellHandle::from_raw(object);
    let Ok(method) = abi_name_to_string(method_ptr, method_len) else {
        return EvalStatus::RuntimeFatal.code();
    };
    let caller_context = (!ctx.is_null()).then_some(ctx);
    let dynamic_owner = {
        let mut values = ElephcRuntimeOps::with_context(std::ptr::null());
        values
            .object_identity(object)
            .ok()
            .and_then(crate::ffi::dynamic_destructors::dynamic_object_owner_context)
    };
    let context = dynamic_owner.or(caller_context);
    let Some(context) = context else {
        return -1;
    };
    let Some(context) = context.as_mut() else {
        return -1;
    };
    if context.abi_version() != ABI_VERSION {
        return EvalStatus::AbiMismatch.code();
    }
    // A compiled caller publishes its source position in `_php_site` before it can enter PHP
    // code. Carry it into the interpreter: the synthetic frame pushed for the native callee, and
    // any diagnostic raised while entering it, name the compiled call site (`Router.php:188`)
    // instead of the empty position the context held.
    if let Some((file, directory, line)) = unsafe { crate::ffi::compiled_site::compiled_call_site() } {
        context.set_call_site(file, directory, line);
    }
    let forwarded_scopes = caller_context
        .filter(|caller| !std::ptr::eq(*caller, context))
        .and_then(|caller| unsafe { caller.as_ref() })
        .map(|caller| {
            (
                caller.current_class_scope().map(str::to_string),
                caller.current_called_class_scope().map(str::to_string),
            )
        });
    if let Some((class_scope, called_class_scope)) = &forwarded_scopes {
        if let Some(class_scope) = class_scope {
            context.push_class_scope(class_scope.clone());
        }
        if let Some(called_class_scope) = called_class_scope {
            context.push_called_class_scope(called_class_scope.clone());
        }
    }
    // A COMPILED frame with no eval context of its own publishes its lexical class here instead
    // of forwarding scopes, and that class is the calling scope php would use: the call really
    // is made from inside that class body. Without it a protected override -- Twig's
    // `Template::yield()` calling `$this->doDisplay()` -- was refused "from global scope".
    let native_caller_class = forwarded_scopes
        .is_none()
        .then(crate::context::current_native_caller_class)
        .flatten()
        .filter(|class_name| !class_name.is_empty());
    if let Some(class_name) = &native_caller_class {
        context.push_class_scope(class_name.clone());
    }
    clear_result(out);
    let mut values = ElephcRuntimeOps::with_context(context as *const ElephcEvalContext);
    let (args, spliced) = match read_method_call_arg_pack(arg_pack, &mut values) {
        Ok(pack) => pack,
        Err(status) => return status.code(),
    };
    if crate::eval_trace::enabled() {
        let call_site = context.call_site();
        eprintln!(
            "[elephc-eval-trace] phase=method_call_start method={method:?} class_scope={:?} called_class_scope={:?} file={:?} line={}",
            context.current_class_scope(),
            context.current_called_class_scope(),
            call_site.0,
            call_site.2,
        );
    }
    let outcome = context.with_native_global_sync(|context| {
        interpreter::execute_context_method_call_outcome(
            context,
            object.borrowed(),
            &method,
            args,
            &mut values,
        )
    });
    if let Some((class_scope, called_class_scope)) = forwarded_scopes {
        if called_class_scope.is_some() {
            context.pop_called_class_scope();
        }
        if class_scope.is_some() {
            context.pop_class_scope();
        }
    }
    if native_caller_class.is_some() {
        context.pop_class_scope();
    }
    for element in spliced {
        let _ = values.release(element);
    }
    match outcome {
        Ok(outcome) => write_outcome(outcome, out).code(),
        Err(status) => {
            if crate::eval_trace::enabled() {
                let call_site = context.call_site();
                eprintln!(
                    "[elephc-eval-trace] phase=method_call_error method={method:?} status={status:?} class_scope={:?} called_class_scope={:?} file={:?} line={}",
                    context.current_class_scope(),
                    context.current_called_class_scope(),
                    call_site.0,
                    call_site.2,
                );
            }
            status.code()
        }
    }
}

/// Reads a method-call (instance or static) argument pack as borrowed argument cells.
///
/// A compiled call planned against a signature carrying the hidden `func_get_args()` collector
/// passes that collector as its last operand and says so in the count word's high bits. The
/// interpreted callee declares only its source parameters, so the collector is spliced back into
/// the arguments PHP actually passed: its elements are the surplus, and a collector that opens
/// with the actual count also says how many of the regular operands the caller really supplied
/// rather than filled in from the compiled defaults. Returns the arguments plus the owned element
/// reads the caller releases once the call is done.
///
/// # Safety
/// `arg_pack` must point at a count word followed by that many runtime-cell pointers.
#[cfg(not(test))]
unsafe fn read_method_call_arg_pack(
    arg_pack: *const usize,
    values: &mut ElephcRuntimeOps,
) -> Result<(Vec<RuntimeCellHandle>, Vec<RuntimeCellHandle>), EvalStatus> {
    let word = *arg_pack;
    let flags = EVAL_ARG_PACK_SURPLUS_COLLECTOR | EVAL_ARG_PACK_COLLECTOR_COUNTS;
    let arg_count = word & !flags;
    let arg_ptrs = arg_pack.add(1) as *const *mut RuntimeCell;
    let mut args: Vec<RuntimeCellHandle> = if arg_count == 0 {
        Vec::new()
    } else {
        slice::from_raw_parts(arg_ptrs, arg_count)
            .iter()
            .map(|arg| RuntimeCellHandle::from_raw(*arg).borrowed())
            .collect()
    };
    let mut spliced = Vec::new();
    if word & EVAL_ARG_PACK_SURPLUS_COLLECTOR == 0 {
        return Ok((args, spliced));
    }
    let Some(collector) = args.pop() else {
        return Ok((args, spliced));
    };
    if !values.is_array_like(collector)? {
        return Ok((args, spliced));
    }
    let len = values.array_len(collector)?;
    let mut position = 0;
    if word & EVAL_ARG_PACK_COLLECTOR_COUNTS != 0 && len > 0 {
        let count = values.array_iter_value(collector, 0)?;
        let actual = values.raw_value_word(count)? as usize;
        values.release(count)?;
        position = 1;
        if actual < args.len() {
            args.truncate(actual);
            return Ok((args, spliced));
        }
    }
    while position < len {
        let element = values.array_iter_value(collector, position)?;
        args.push(element.borrowed());
        spliced.push(element);
        position += 1;
    }
    Ok((args, spliced))
}

/// Runs the eval property-read ABI body after installing a panic boundary.
///
/// # Safety
/// Mirrors `__elephc_eval_property_get`; callers must provide a live context,
/// a boxed object cell, readable property-name bytes, and optional result storage.
#[cfg(not(test))]
unsafe fn eval_property_get_inner(
    ctx: *mut ElephcEvalContext,
    object: *mut RuntimeCell,
    property_ptr: *const u8,
    property_len: u64,
    out: *mut ElephcEvalResult,
) -> i32 {
    if object.is_null() {
        return EvalStatus::RuntimeFatal.code();
    }
    let Ok(property) = abi_name_to_string(property_ptr, property_len) else {
        return EvalStatus::RuntimeFatal.code();
    };
    let object = RuntimeCellHandle::from_raw(object);
    // An object of an eval-declared class is known only to the context that declared it. A
    // compiled function reading it may hold a DIFFERENT context (every frame that needs the
    // bridge makes its own), which found no class for the object and read even a declared
    // property as an undeclared one: `$provider->singletons` on an eval subclass of a compiled
    // class came back null. Read through the owner, as a method call does, with the caller's
    // class scope forwarded so visibility is still judged from where the read is written. A
    // compiled frame with no context of its own passes null and publishes its lexical class
    // instead.
    let dynamic_owner = {
        let mut probe = ElephcRuntimeOps::with_context(std::ptr::null());
        probe
            .object_identity(object)
            .ok()
            .and_then(crate::ffi::dynamic_destructors::dynamic_object_owner_context)
    };
    let context_ptr = dynamic_owner.unwrap_or(ctx);
    let Some(context) = context_ptr.as_mut() else {
        return EvalStatus::RuntimeFatal.code();
    };
    if context.abi_version() != ABI_VERSION {
        return EvalStatus::AbiMismatch.code();
    }
    let forwarded_class_scope = if ctx.is_null() {
        crate::context::current_native_caller_class().filter(|class_name| !class_name.is_empty())
    } else if std::ptr::eq(context_ptr, ctx) {
        None
    } else {
        ctx.as_ref()
            .and_then(|caller| caller.current_class_scope().map(str::to_string))
    };
    if let Some(class_scope) = &forwarded_class_scope {
        context.push_class_scope(class_scope.clone());
    }
    clear_result(out);
    let mut values = ElephcRuntimeOps::with_context(context as *const ElephcEvalContext);
    let outcome =
        interpreter::execute_context_property_get_outcome(context, object, &property, &mut values);
    if forwarded_class_scope.is_some() {
        context.pop_class_scope();
    }
    match outcome {
        Ok(outcome) => write_outcome(outcome, out).code(),
        Err(status) => {
            // A refusal that reaches the user as six words is a defect of its own. Name the class
            // the object actually has, the property asked for, and the scope it was asked from --
            // the three facts that separate "no such property" from "not visible from here".
            if status == EvalStatus::RuntimeFatal {
                let class_name = crate::ffi::util::eval_object_class_name_for_diagnostic(
                    object,
                    &mut values,
                );
                let scope = context
                    .current_class_scope()
                    .map_or_else(|| "no class scope".to_string(), str::to_string);
                let (file, _, line, _) = context.call_site();
                crate::errors::note_eval_runtime_failure(
                    format!(
                        "property {}::${property} could not be read from {scope}",
                        class_name.as_deref().unwrap_or("an object of unknown class"),
                    ),
                    file,
                    Some(line),
                );
            }
            status.code()
        }
    }
}

/// Runs the eval-aware string-context ABI body after installing a panic boundary.
///
/// # Safety
/// Mirrors `__elephc_eval_string_context`; callers must provide a boxed cell. Dynamic
/// objects recover their owning eval context from the process registry, so they remain
/// stringable after crossing into an ordinary AOT helper that has no local eval slot.
#[cfg(not(test))]
unsafe fn eval_string_context_inner(
    ctx: *mut ElephcEvalContext,
    value: *mut RuntimeCell,
    out: *mut ElephcEvalResult,
) -> i32 {
    if value.is_null() {
        return EvalStatus::RuntimeFatal.code();
    }
    let value = RuntimeCellHandle::from_raw(value).borrowed();
    let mut probe = ElephcRuntimeOps::new();
    let owner = probe.object_identity(value).ok().and_then(|identity| {
        crate::ffi::dynamic_destructors::dynamic_object_owner_context(identity)
    });
    let resolved_context = owner.unwrap_or(ctx);
    let Some(context) = resolved_context.as_mut() else {
        return EvalStatus::RuntimeFatal.code();
    };
    if context.abi_version() != ABI_VERSION {
        return EvalStatus::AbiMismatch.code();
    }
    clear_result(out);
    let mut values = ElephcRuntimeOps::with_context(context as *const ElephcEvalContext);
    match interpreter::execute_context_string_outcome(
        context,
        value,
        &mut values,
    ) {
        Ok(outcome) => write_outcome(outcome, out).code(),
        Err(status) => status.code(),
    }
}

/// Runs the eval-created object numeric-warning ABI body after installing a panic boundary.
///
/// # Safety
/// Mirrors `__elephc_eval_sprintf_numeric_warning`; `object` must remain live for the
/// duration of the registry lookup and warning emission.
#[cfg(not(test))]
unsafe fn eval_sprintf_numeric_warning_inner(
    ctx: *mut ElephcEvalContext,
    object: *mut RuntimeCell,
    is_float: u64,
) -> i32 {
    if object.is_null() {
        return EvalStatus::RuntimeFatal.code();
    }
    let identity = object as u64;
    let owner = crate::ffi::dynamic_destructors::dynamic_object_owner_context(identity);
    let resolved_context = owner.unwrap_or(ctx);
    let Some(context) = resolved_context.as_mut() else {
        return EvalStatus::RuntimeFatal.code();
    };
    if context.abi_version() != ABI_VERSION {
        return EvalStatus::AbiMismatch.code();
    }
    let Some(class_name) = context.dynamic_object_class_name(identity) else {
        return EvalStatus::RuntimeFatal.code();
    };
    let target = if is_float == 0 { "int" } else { "float" };
    let mut values = ElephcRuntimeOps::with_context(context as *const ElephcEvalContext);
    match values.warning(&format!(
        "Warning: Object of class {} could not be converted to {target}\n",
        class_name.trim_start_matches('\\')
    )) {
        Ok(()) => EvalStatus::Ok.code(),
        Err(status) => status.code(),
    }
}
