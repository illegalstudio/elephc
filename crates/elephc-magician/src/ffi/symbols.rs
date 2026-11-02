//! Purpose:
//! Exports dynamic symbol probes and constant fetching for eval-created state.
//! Generated code calls these after eval barriers to observe dynamic functions,
//! constants, and classes registered by interpreted fragments.
//!
//! Called from:
//! - Generated EIR backend assembly through `__elephc_eval_*exists` symbols.
//!
//! Key details:
//! - Existence probes fail closed as `false` on invalid ABI inputs.
//! - Constant fetch retains the boxed cell before handing it back to generated code.

use super::util::abi_name_to_string;
#[cfg(not(test))]
use super::util::clear_result;
#[cfg(not(test))]
use crate::abi::ElephcEvalResult;
use crate::abi::{ElephcEvalContext, ABI_VERSION};
#[cfg(not(test))]
use crate::context::EvalCallFrame;
#[cfg(not(test))]
use crate::errors::EvalStatus;
#[cfg(not(test))]
use crate::interpreter::RuntimeValueOps;
#[cfg(not(test))]
use crate::interpreter::eval_spl_autoload_class_bridge;
#[cfg(not(test))]
use crate::runtime_hooks::{self, ElephcRuntimeOps};

/// Identifies the PHP class-like symbol table queried through the eval bridge.
#[derive(Clone, Copy)]
enum DynamicClassLikeKind {
    Class,
    Interface,
    Trait,
    Enum,
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
    crate::ffi::util::trace_eval_ffi_entry("__elephc_eval_function_exists");
    std::panic::catch_unwind(|| unsafe { eval_function_exists_inner(ctx, name_ptr, name_len) })
        .unwrap_or(0)
}

/// Reports whether the bridge can answer a call to this function name with no declaring context.
///
/// Generated code resolves a call by asking which eval context DECLARED the function; a name no
/// context declares was then an undefined function, full stop. But the bridge answers for names
/// nobody declares -- the two backtrace functions, the OPcache family, the procedural date aliases
/// and every builtin the interpreter implements -- so that question was the wrong one to end on.
///
/// PHP resolves an unqualified call written inside a namespace against the namespaced name first
/// and the global name second, and generated code hands over the namespaced candidate, so the
/// global fallback is applied here too. The answer comes from `context_function_is_callable()`,
/// the same table the call itself dispatches through.
///
/// # Safety
/// `name_ptr` must be null or readable for `name_len` bytes.
#[cfg(not(test))]
#[no_mangle]
pub unsafe extern "C" fn __elephc_eval_bridge_can_call_function(
    name_ptr: *const u8,
    name_len: u64,
) -> i32 {
    crate::ffi::util::trace_eval_ffi_entry("__elephc_eval_bridge_can_call_function");
    std::panic::catch_unwind(|| {
        let Ok(name) = abi_name_to_string(name_ptr, name_len) else {
            return 0;
        };
        let name = name.to_ascii_lowercase();
        let mut context = ElephcEvalContext::new();
        crate::context::sync_global_eval_aot_metadata(&mut context);
        let callable = crate::interpreter::context_function_is_callable(&context, &name)
            || name.rsplit_once('\\').is_some_and(|(_, bare)| {
                crate::interpreter::context_function_is_callable(&context, bare)
            });
        i32::from(callable)
    })
    .unwrap_or(0)
}

/// Checks whether a constant was previously defined through `eval()`.
///
/// # Safety
/// `ctx` must be null or a valid eval context handle. `name_ptr` must be
/// readable for `name_len` bytes when `name_len > 0`.
#[no_mangle]
pub unsafe extern "C" fn __elephc_eval_constant_exists(
    ctx: *const ElephcEvalContext,
    name_ptr: *const u8,
    name_len: u64,
) -> i32 {
    crate::ffi::util::trace_eval_ffi_entry("__elephc_eval_constant_exists");
    std::panic::catch_unwind(|| unsafe { eval_constant_exists_inner(ctx, name_ptr, name_len) })
        .unwrap_or(0)
}

/// Checks whether an eval-created or generated class exists, optionally autoloading it.
///
/// # Safety
/// `ctx` must be null or a valid mutable eval context handle. `name_ptr` must
/// be readable for `name_len` bytes when `name_len > 0`; nonzero `autoload`
/// invokes registered SPL callbacks after the direct lookup misses.
#[no_mangle]
pub unsafe extern "C" fn __elephc_eval_dynamic_class_exists(
    ctx: *mut ElephcEvalContext,
    name_ptr: *const u8,
    name_len: u64,
    autoload: i32,
) -> i32 {
    crate::ffi::util::trace_eval_ffi_entry("__elephc_eval_dynamic_class_exists");
    std::panic::catch_unwind(|| unsafe {
        eval_dynamic_class_like_exists_inner(
            ctx,
            name_ptr,
            name_len,
            autoload,
            DynamicClassLikeKind::Class,
        )
    })
    .unwrap_or(0)
}

/// Checks whether an eval-created or generated interface exists, optionally autoloading it.
///
/// # Safety
/// `ctx` must be null or a valid mutable eval context handle. `name_ptr` must
/// be readable for `name_len` bytes when `name_len > 0`; nonzero `autoload`
/// invokes registered SPL callbacks after the direct lookup misses.
#[no_mangle]
pub unsafe extern "C" fn __elephc_eval_dynamic_interface_exists(
    ctx: *mut ElephcEvalContext,
    name_ptr: *const u8,
    name_len: u64,
    autoload: i32,
) -> i32 {
    crate::ffi::util::trace_eval_ffi_entry("__elephc_eval_dynamic_interface_exists");
    std::panic::catch_unwind(|| unsafe {
        eval_dynamic_class_like_exists_inner(
            ctx,
            name_ptr,
            name_len,
            autoload,
            DynamicClassLikeKind::Interface,
        )
    })
    .unwrap_or(0)
}

/// Checks whether an eval-created or generated trait exists, optionally autoloading it.
///
/// # Safety
/// `ctx` must be null or a valid mutable eval context handle. `name_ptr` must
/// be readable for `name_len` bytes when `name_len > 0`; nonzero `autoload`
/// invokes registered SPL callbacks after the direct lookup misses.
#[no_mangle]
pub unsafe extern "C" fn __elephc_eval_dynamic_trait_exists(
    ctx: *mut ElephcEvalContext,
    name_ptr: *const u8,
    name_len: u64,
    autoload: i32,
) -> i32 {
    crate::ffi::util::trace_eval_ffi_entry("__elephc_eval_dynamic_trait_exists");
    std::panic::catch_unwind(|| unsafe {
        eval_dynamic_class_like_exists_inner(
            ctx,
            name_ptr,
            name_len,
            autoload,
            DynamicClassLikeKind::Trait,
        )
    })
    .unwrap_or(0)
}

/// Checks whether an eval-created or generated enum exists, optionally autoloading it.
///
/// # Safety
/// `ctx` must be null or a valid mutable eval context handle. `name_ptr` must
/// be readable for `name_len` bytes when `name_len > 0`; nonzero `autoload`
/// invokes registered SPL callbacks after the direct lookup misses.
#[no_mangle]
pub unsafe extern "C" fn __elephc_eval_dynamic_enum_exists(
    ctx: *mut ElephcEvalContext,
    name_ptr: *const u8,
    name_len: u64,
    autoload: i32,
) -> i32 {
    crate::ffi::util::trace_eval_ffi_entry("__elephc_eval_dynamic_enum_exists");
    std::panic::catch_unwind(|| unsafe {
        eval_dynamic_class_like_exists_inner(
            ctx,
            name_ptr,
            name_len,
            autoload,
            DynamicClassLikeKind::Enum,
        )
    })
    .unwrap_or(0)
}

/// Fetches a constant previously defined through `eval()`.
///
/// # Safety
/// `ctx` must be a valid eval context handle. `name_ptr` must be readable for
/// `name_len` bytes when `name_len > 0`, and `out` may be null.
#[cfg(not(test))]
#[no_mangle]
pub unsafe extern "C" fn __elephc_eval_constant_fetch(
    ctx: *mut ElephcEvalContext,
    name_ptr: *const u8,
    name_len: u64,
    out: *mut ElephcEvalResult,
) -> i32 {
    crate::ffi::util::trace_eval_ffi_entry("__elephc_eval_constant_fetch");
    std::panic::catch_unwind(|| unsafe { eval_constant_fetch_inner(ctx, name_ptr, name_len, out) })
        .unwrap_or_else(|_| EvalStatus::RuntimeFatal.code())
}

/// Runs the eval function-exists ABI body after installing a panic boundary.
///
/// # Safety
/// Mirrors `__elephc_eval_function_exists`; invalid handles or unreadable name
/// storage fail closed as `false`.
///
/// A null context asks the process-wide registry instead: compiled code with no eval frame of
/// its own still has to see a function a runtime-included file declared, since PHP's function
/// table is global. The name may carry PHP's one optional leading namespace separator.
unsafe fn eval_function_exists_inner(
    ctx: *const ElephcEvalContext,
    name_ptr: *const u8,
    name_len: u64,
) -> i32 {
    let Ok(name) = abi_name_to_string(name_ptr, name_len) else {
        return 0;
    };
    let name = name.strip_prefix('\\').unwrap_or(&name).to_ascii_lowercase();
    if name.is_empty() || name.starts_with('\\') {
        return 0;
    }
    let Some(context) = ctx.as_ref() else {
        #[cfg(not(test))]
        {
            return i32::from(crate::context::global_eval_function_is_declared(&name));
        }
        #[cfg(test)]
        {
            return 0;
        }
    };
    if context.abi_version() != ABI_VERSION {
        return 0;
    }
    // Exact names only: `has_function()` also retries a namespaced name as a global one, which is
    // right for resolving a CALL and wrong for `function_exists('Ns\\f')`.
    #[cfg(not(test))]
    let declared_elsewhere = crate::context::global_eval_function_is_declared(&name);
    #[cfg(test)]
    let declared_elsewhere = false;
    i32::from(
        context.function(&name).is_some()
            || context.native_function(&name).is_some()
            || declared_elsewhere,
    )
}

/// Runs the eval constant-exists ABI body after installing a panic boundary.
///
/// # Safety
/// Mirrors `__elephc_eval_constant_exists`; invalid handles or unreadable name
/// storage fail closed as `false`.
unsafe fn eval_constant_exists_inner(
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
    i32::from(context.has_constant(&name))
}

/// Returns whether one eval context owns a class-like declaration of the requested kind.
fn eval_context_has_class_like(
    context: &ElephcEvalContext,
    name: &str,
    kind: DynamicClassLikeKind,
) -> bool {
    match kind {
        DynamicClassLikeKind::Class => context.has_class(name),
        DynamicClassLikeKind::Interface => context.has_interface(name),
        DynamicClassLikeKind::Trait => context.has_trait(name),
        DynamicClassLikeKind::Enum => context.has_enum(name),
    }
}

/// Returns whether generated runtime metadata owns a class-like declaration of the requested kind.
#[cfg(not(test))]
fn eval_runtime_has_class_like(
    values: &mut ElephcRuntimeOps,
    name: &str,
    kind: DynamicClassLikeKind,
) -> bool {
    match kind {
        DynamicClassLikeKind::Class => values.class_exists(name),
        DynamicClassLikeKind::Interface => values.interface_exists(name),
        DynamicClassLikeKind::Trait => values.trait_exists(name),
        DynamicClassLikeKind::Enum => values.enum_exists(name),
    }
    .unwrap_or(false)
}

/// Runs the eval dynamic-class-like-exists ABI body after installing a panic boundary.
///
/// # Safety
/// Mirrors the `__elephc_eval_dynamic_*_exists` exports; invalid handles or
/// unreadable name storage fail closed as `false`. When autoload is enabled,
/// this mutates the context by running registered callbacks exactly as PHP
/// class-like probes do.
#[cfg(not(test))]
unsafe fn eval_dynamic_class_like_exists_inner(
    ctx: *mut ElephcEvalContext,
    name_ptr: *const u8,
    name_len: u64,
    autoload: i32,
    kind: DynamicClassLikeKind,
) -> i32 {
    // php resolves `'\Foo\Bar'` as `Foo\Bar`. Every lookup below keys bare names, so a leading
    // separator missed the compiled fast path, built a full fallback context per call, and still
    // answered false: Laravel's `class_exists('\Illuminate\Console\View\Components\' . ...)`
    // threw "Console component [info] not found" from every artisan command.
    let (name_ptr, name_len) = if name_len > 0 && !name_ptr.is_null() && *name_ptr == b'\\' {
        (name_ptr.add(1), name_len - 1)
    } else {
        (name_ptr, name_len)
    };
    // Compiled code with no eval context of its own asks here for every `class_exists()` it
    // makes, and building a fallback context with a copy of the AOT metadata cost more than the
    // probe. A compiled class-like needs neither: when the generated tables hold it and it may be
    // answered (loaded, or autoloading allowed), the answer is the one the full path gives.
    // `Closure` is a language intrinsic with no class-table row on either side; the interpreter's
    // own `class_exists()` answers it the same way (`eval_class_is_declared`).
    if matches!(kind, DynamicClassLikeKind::Class) {
        if let Ok(name) = abi_name_to_string(name_ptr, name_len) {
            if name.trim_start_matches('\\').eq_ignore_ascii_case("Closure") {
                return 1;
            }
        }
    }
    if ctx.is_null() {
        let Ok(name) = abi_name_to_string(name_ptr, name_len) else {
            return 0;
        };
        let mut values = ElephcRuntimeOps::with_context(std::ptr::null());
        if eval_runtime_has_class_like(&mut values, &name, kind)
            && (autoload != 0 || runtime_hooks::compiled_class_is_loaded(&name))
        {
            runtime_hooks::mark_compiled_class_loaded(&name);
            return 1;
        }
    }
    let mut fallback_context;
    let context = if let Some(context) = ctx.as_mut() {
        context
    } else {
        fallback_context = ElephcEvalContext::new();
        crate::context::sync_global_eval_aot_metadata(&mut fallback_context);
        &mut fallback_context
    };
    if context.abi_version() != ABI_VERSION {
        return 0;
    }
    let Ok(name) = abi_name_to_string(name_ptr, name_len) else {
        return 0;
    };
    context.sync_global_eval_classes();
    if eval_context_has_class_like(context, &name, kind) {
        return 1;
    }
    let mut values = ElephcRuntimeOps::with_context(context as *const _);
    if eval_runtime_has_class_like(&mut values, &name, kind) {
        // A class the compiler pulled in ONLY so a probe could be answered is DECLARED but not
        // LOADED, which is exactly the difference `$autoload = false` asks about. A probe that
        // allows autoloading is what loads it, here as in php.
        if autoload == 0 && !runtime_hooks::compiled_class_is_loaded(&name) {
            return 0;
        }
        runtime_hooks::mark_compiled_class_loaded(&name);
        return 1;
    }
    if autoload == 0 {
        return 0;
    }
    // PHP calls an internal function on a frame of its own, and a loader started by one reads it:
    // `ClassExistenceResource::throwOnRequiredClass` returns quietly only when `$trace[1]` names
    // one of these probes and carries no `class` key. The interpreter's own probes have pushed
    // that frame for a while; a probe written in COMPILED code arrives here instead and pushed
    // nothing, so the same loader saw its own frame and nothing above it.
    let probe_argument = values.string(&name).ok();
    if let Some(argument) = probe_argument {
        context.push_call_frame(EvalCallFrame::function(
            eval_class_like_probe_name(kind),
            Some(vec![argument]),
            context,
        ));
    }
    let bridge = eval_spl_autoload_class_bridge(&name, context, &mut values);
    // A Throwable an autoloader raised (`ClassExistenceResource::throwOnRequiredClass` throws a
    // `ReflectionException` out of `class_exists()`) leaves the probe through this channel: the
    // owned box is parked for `__elephc_eval_take_probe_throwable` and the status is 2, which the
    // compiled caller turns into a native throw. Answering `false` here swallowed it.
    let escaped = if matches!(bridge, Err(EvalStatus::UncaughtThrowable)) {
        context.take_pending_throw().and_then(|thrown| {
            if thrown.is_borrowed() {
                values.retain(thrown).ok()
            } else {
                Some(thrown)
            }
        })
    } else {
        None
    };
    if let Some(argument) = probe_argument {
        context.pop_call_frame();
        let _ = values.release(argument);
    }
    if let Some(thrown) = escaped {
        PROBE_THROWABLE.store(thrown.as_ptr() as usize, std::sync::atomic::Ordering::SeqCst);
        return PROBE_THROWN_STATUS;
    }
    context.sync_global_eval_classes();
    i32::from(
        eval_context_has_class_like(context, &name, kind)
            || eval_runtime_has_class_like(&mut values, &name, kind),
    )
}

/// Status a class-like probe returns when an autoloader raised a Throwable.
#[cfg(not(test))]
const PROBE_THROWN_STATUS: i32 = 2;

/// Owned Throwable box parked by a probe that returned [`PROBE_THROWN_STATUS`].
#[cfg(not(test))]
static PROBE_THROWABLE: std::sync::atomic::AtomicUsize = std::sync::atomic::AtomicUsize::new(0);

/// Hands the parked probe Throwable (an owned box, or null) to the compiled caller.
///
/// # Safety
/// Only meaningful straight after a `__elephc_eval_dynamic_*_exists` call returned status 2.
#[cfg(not(test))]
#[no_mangle]
pub unsafe extern "C" fn __elephc_eval_take_probe_throwable() -> *mut crate::value::RuntimeCell {
    PROBE_THROWABLE.swap(0, std::sync::atomic::Ordering::SeqCst) as *mut crate::value::RuntimeCell
}

/// Returns the PHP function name of the class-like probe that can start an autoloader.
#[cfg(not(test))]
fn eval_class_like_probe_name(kind: DynamicClassLikeKind) -> &'static str {
    match kind {
        DynamicClassLikeKind::Class => "class_exists",
        DynamicClassLikeKind::Interface => "interface_exists",
        DynamicClassLikeKind::Trait => "trait_exists",
        DynamicClassLikeKind::Enum => "enum_exists",
    }
}

/// Checks an eval declaration table in unit tests without native runtime callbacks.
///
/// The unit-test runtime has no generated callback ABI, so this branch verifies
/// lookup normalization while integration tests cover autoload through the real bridge.
#[cfg(test)]
unsafe fn eval_dynamic_class_like_exists_inner(
    ctx: *mut ElephcEvalContext,
    name_ptr: *const u8,
    name_len: u64,
    _autoload: i32,
    kind: DynamicClassLikeKind,
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
    i32::from(eval_context_has_class_like(context, &name, kind))
}

/// Runs the eval constant-fetch ABI body after installing a panic boundary.
///
/// # Safety
/// Mirrors `__elephc_eval_constant_fetch`; callers must provide a valid context,
/// readable constant-name bytes, and optional writable result storage.
#[cfg(not(test))]
unsafe fn eval_constant_fetch_inner(
    ctx: *mut ElephcEvalContext,
    name_ptr: *const u8,
    name_len: u64,
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
    clear_result(out);
    let Some(value) = context.constant(&name) else {
        return EvalStatus::RuntimeFatal.code();
    };
    if out.is_null() {
        return EvalStatus::Ok.code();
    }
    let mut values = ElephcRuntimeOps::new();
    match values.retain(value) {
        Ok(result) => {
            (*out).kind = 0;
            (*out).value_cell = result.as_ptr();
            (*out).error = std::ptr::null_mut();
            EvalStatus::Ok.code()
        }
        Err(status) => status.code(),
    }
}
