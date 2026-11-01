//! Purpose:
//! Exports post-barrier object introspection for values that may have been
//! created from eval-declared classes. Generated code uses this ABI when native
//! type metadata must consult the persistent eval context.
//!
//! Called from:
//! - Generated EIR backend assembly through `__elephc_eval_object_class_name`.
//! - Generated EIR backend assembly through `__elephc_eval_object_is_a`.
//! - Generated EIR backend assembly through `__elephc_eval_object_is_a_dynamic`.
//! - Generated EIR backend assembly through `__elephc_eval_member_exists`.
//! - Generated EIR backend assembly through `__elephc_eval_class_relation`.
//!
//! Key details:
//! - Class-name and relation lookups return boxed Mixed cells through `ElephcEvalResult`.
//! - Named predicate probes fail closed as false; dynamic `instanceof` invalid
//!   targets return -1 so generated code can raise PHP's fatal diagnostic.

use super::util::{abi_name_to_string, clear_result, write_outcome};
use crate::abi::{ElephcEvalContext, ElephcEvalResult, ABI_VERSION};
use crate::errors::EvalStatus;
use crate::interpreter::{self, EvalOutcome, RuntimeValueOps};
use crate::runtime_hooks::ElephcRuntimeOps;
use crate::value::{RuntimeCell, RuntimeCellHandle};

/// A class name handed back to compiled code: `len == 0` means "not an eval-class instance".
#[repr(C)]
#[derive(Clone, Copy)]
pub struct ElephcEvalClassName {
    pub ptr: *const u8,
    pub len: u64,
}

/// Names the eval-declared class of a raw object, for compiled `get_class()`.
///
/// An instance of a class the interpreter declared lives in the storage of its nearest COMPILED
/// ancestor, so the class id compiled code reads names that ancestor: `get_class()` of a
/// `LazyCommand` said `Command`. The eval side knows the real class. The bytes are interned for
/// the life of the process -- compiled code treats a class name as static data and may keep the
/// pointer without copying it. Returns a zero length for any object the interpreter does not own.
///
/// # Safety
/// `object` must be null or a live object payload pointer.
#[cfg(not(test))]
#[no_mangle]
pub unsafe extern "C" fn __elephc_eval_dynamic_object_class_name(object: *const u8) -> ElephcEvalClassName {
    let none = ElephcEvalClassName { ptr: std::ptr::null(), len: 0 };
    if object.is_null() {
        return none;
    }
    std::panic::catch_unwind(|| unsafe {
        let identity = object as u64;
        let Some(owner) = crate::ffi::dynamic_destructors::dynamic_object_owner_context(identity) else {
            return none;
        };
        let Some(context) = owner.as_ref() else {
            return none;
        };
        if let Some((_, class)) = context.dynamic_object_declaring_class(identity) {
            let name = intern_class_name(class.name().trim_start_matches('\\'));
            return ElephcEvalClassName { ptr: name.as_ptr(), len: name.len() as u64 };
        }
        // An interpreter-made `Closure` is carried by a native object whose class id names
        // `stdClass`; its owner knows it as a closure, so `get_debug_type($closure)` in a body
        // without an eval context of its own answers `Closure`, as php does.
        if context.closure_object_target(identity).is_some() {
            let name = intern_class_name("Closure");
            return ElephcEvalClassName { ptr: name.as_ptr(), len: name.len() as u64 };
        }
        none
    })
    .unwrap_or(none)
}

/// Turns an interpreted `Traversable` into a compiled `ArrayIterator` a compiled `foreach` drives.
///
/// `object` is a raw object payload pointer (its identity). Returns the iterator's object pointer
/// -- the compiled loop keeps it as its source, the way it keeps a `getIterator()` result -- or 0
/// when the object is not owned by an eval context or could not be drained, in which case the loop
/// goes on with the original object exactly as before.
///
/// # Safety
/// `object` must be 0 or a live object payload pointer.
#[cfg(not(test))]
#[no_mangle]
pub unsafe extern "C" fn __elephc_eval_native_iterator_for_object(object: u64) -> u64 {
    crate::ffi::util::trace_eval_ffi_entry("__elephc_eval_native_iterator_for_object");
    std::panic::catch_unwind(|| unsafe { native_iterator_for_object_inner(object) }).unwrap_or(0)
}

/// Drains one eval-owned object through its owner context; see the extern above.
#[cfg(not(test))]
unsafe fn native_iterator_for_object_inner(object: u64) -> u64 {
    if object == 0 {
        return 0;
    }
    let Some(owner) = crate::ffi::dynamic_destructors::dynamic_object_owner_context(object) else {
        return 0;
    };
    let Some(context) = owner.as_mut() else {
        return 0;
    };
    if context.abi_version() != ABI_VERSION {
        return 0;
    }
    // Only an instance of an INTERPRETED class: a generator, a closure or any other object of a
    // compiled class is driven by the compiled dispatch already, and a generator drained here
    // could never be resumed again ("Cannot rewind a generator that was already run").
    if context.dynamic_object_declaring_class(object).is_none() {
        return 0;
    }
    let mut values = ElephcRuntimeOps::with_context(context as *const ElephcEvalContext);
    let Ok(cell) = values.raw_heap_word_value(object) else {
        return 0;
    };
    let iterator = interpreter::execute_context_native_iterator(context, cell, &mut values);
    let _ = values.release(cell);
    match iterator {
        // The cell is deliberately kept: it holds the iterator's only reference, which the
        // compiled loop now owns through the raw pointer it stores as its source.
        Ok(iterator) => values.raw_value_word(iterator).unwrap_or(0),
        Err(_) => {
            let _ = context.take_pending_throw();
            0
        }
    }
}

/// Unwraps an interpreted `IteratorAggregate` for a compiled `new IteratorIterator($object)`.
///
/// Returns the raw object pointer of `getIterator()`'s final result, with one reference owned by
/// the caller, or 0 when `object` is not an interpreted aggregate (the caller then wraps the
/// object itself, exactly as before).
///
/// # Safety
/// `object` must be 0 or a live object payload pointer.
#[cfg(not(test))]
#[no_mangle]
pub unsafe extern "C" fn __elephc_eval_unwrap_aggregate(object: u64) -> u64 {
    crate::ffi::util::trace_eval_ffi_entry("__elephc_eval_unwrap_aggregate");
    std::panic::catch_unwind(|| unsafe { unwrap_aggregate_inner(object) }).unwrap_or(0)
}

/// Resolves one interpreted aggregate through its owner context; see the extern above.
#[cfg(not(test))]
unsafe fn unwrap_aggregate_inner(object: u64) -> u64 {
    if object == 0 {
        return 0;
    }
    let Some(owner) = crate::ffi::dynamic_destructors::dynamic_object_owner_context(object) else {
        return 0;
    };
    let Some(context) = owner.as_mut() else {
        return 0;
    };
    if context.abi_version() != ABI_VERSION || context.dynamic_object_declaring_class(object).is_none() {
        return 0;
    }
    let mut values = ElephcRuntimeOps::with_context(context as *const ElephcEvalContext);
    let Ok(cell) = values.raw_heap_word_value(object) else {
        return 0;
    };
    let unwrapped = interpreter::execute_context_unwrap_aggregate(context, cell, &mut values);
    let _ = values.release(cell);
    match unwrapped {
        Ok(Some(inner)) => {
            // Keep one reference for the compiled object that will store the pointer, then drop
            // the box that carried it.
            let word = values.raw_value_word(inner).unwrap_or(0);
            let word = if word == 0 { 0 } else { values.retain_raw_heap_word(word).unwrap_or(0) };
            let _ = values.release(inner);
            word
        }
        Ok(None) => 0,
        Err(_) => {
            let _ = context.take_pending_throw();
            0
        }
    }
}

/// Returns a process-lifetime copy of a class name (the set of class names is small and bounded).
#[cfg(not(test))]
fn intern_class_name(name: &str) -> &'static str {
    use std::collections::HashSet;
    use std::sync::{Mutex, OnceLock};
    static NAMES: OnceLock<Mutex<HashSet<&'static str>>> = OnceLock::new();
    let mut names = NAMES.get_or_init(|| Mutex::new(HashSet::new())).lock().unwrap_or_else(|poisoned| poisoned.into_inner());
    if let Some(existing) = names.get(name) {
        return existing;
    }
    let leaked: &'static str = Box::leak(name.to_string().into_boxed_str());
    names.insert(leaked);
    leaked
}

const CLASS_LOOKUP_GET_CLASS: u64 = 0;
const CLASS_LOOKUP_GET_PARENT_CLASS: u64 = 1;
const MEMBER_LOOKUP_METHOD_EXISTS: u64 = 0;
const MEMBER_LOOKUP_PROPERTY_EXISTS: u64 = 1;
const CLASS_RELATION_IMPLEMENTS: u64 = 0;
const CLASS_RELATION_PARENTS: u64 = 1;
const CLASS_RELATION_USES: u64 = 2;

/// Resolves an object's owning eval context before falling back to the active caller context.
/// Dynamic classes can use native stdClass payloads, so ownership lookup precedes class tables.
///
/// # Safety
/// `object` must be null or a live boxed object and `ctx` null or a valid context handle.
/// The returned context is borrowed from its existing request host.
#[cfg(not(test))]
#[no_mangle]
pub unsafe extern "C" fn __elephc_eval_object_context(
    ctx: *mut ElephcEvalContext, object: *mut RuntimeCell,
) -> *mut ElephcEvalContext {
    std::panic::catch_unwind(|| {
        if object.is_null() { return std::ptr::null_mut(); }
        let object = RuntimeCellHandle::from_raw(object).borrowed();
        let mut values = ElephcRuntimeOps::new();
        let owner = values.object_identity(object).ok().and_then(|identity| {
            crate::ffi::dynamic_destructors::dynamic_object_owner_context(identity)
        });
        let resolved = owner.unwrap_or(ctx);
        match unsafe { resolved.as_ref() } {
            Some(context) if context.abi_version() == ABI_VERSION => resolved,
            _ => std::ptr::null_mut(),
        }
    }).unwrap_or(std::ptr::null_mut())
}

/// Resolves `get_class()` or `get_parent_class()` against eval dynamic objects.
///
/// # Safety
/// `ctx` must be a valid eval context handle. `object_or_class` must point at a
/// boxed runtime cell, `lookup_kind` must be a supported lookup discriminator,
/// and `out` may be null.
#[cfg(not(test))]
#[no_mangle]
pub unsafe extern "C" fn __elephc_eval_object_class_name(
    ctx: *mut ElephcEvalContext,
    object_or_class: *mut RuntimeCell,
    lookup_kind: u64,
    out: *mut ElephcEvalResult,
) -> i32 {
    crate::ffi::util::trace_eval_ffi_entry("__elephc_eval_object_class_name");
    std::panic::catch_unwind(|| unsafe {
        eval_object_class_name_inner(ctx, object_or_class, lookup_kind, out)
    })
    .unwrap_or_else(|_| EvalStatus::RuntimeFatal.code())
}

/// Status returned when an object has no owning eval context.
///
/// The generated runtime treats this as "fall back to the compiled class-name tables"; every
/// other non-zero code is a real eval failure.
pub const EVAL_DYNAMIC_CLASS_NOT_FOUND: i32 = 1;

/// Resolves `get_class()` for an object that may belong to an eval-declared class.
///
/// Unlike [`__elephc_eval_object_class_name`], this entry takes the object's raw payload
/// pointer and finds its owning eval context from the process-local dynamic-object registry.
/// Compiled functions that do not own an eval context (the GMP prelude helpers, ordinary AOT
/// functions called from eval) use it so their diagnostics name the eval class instead of the
/// native `stdClass` carrier.
///
/// # Safety
/// `object` must be null or a live object payload, `lookup_kind` must be a supported lookup
/// discriminator, and `out` may be null.
#[cfg(not(test))]
#[no_mangle]
pub unsafe extern "C" fn __elephc_eval_dynamic_object_class_name_result(
    object: *mut RuntimeCell,
    lookup_kind: u64,
    out: *mut ElephcEvalResult,
) -> i32 {
    std::panic::catch_unwind(|| unsafe {
        if object.is_null() {
            return EVAL_DYNAMIC_CLASS_NOT_FOUND;
        }
        let Some(context) =
            crate::ffi::dynamic_destructors::dynamic_object_owner_context(object as u64)
        else {
            return EVAL_DYNAMIC_CLASS_NOT_FOUND;
        };
        let Some(context) = context.as_mut() else {
            return EVAL_DYNAMIC_CLASS_NOT_FOUND;
        };
        if context.abi_version() != ABI_VERSION {
            return EvalStatus::AbiMismatch.code();
        }
        // `eval_object_class_name_inner` consumes a boxed runtime cell, while compiled
        // callers hold the raw object payload. Box it with the runtime heap kind and release
        // that wrapper once the lookup has published its own result cell.
        let mut values = ElephcRuntimeOps::new();
        let Ok(boxed) = values.raw_heap_word_value(object as u64) else {
            return EVAL_DYNAMIC_CLASS_NOT_FOUND;
        };
        let status = eval_object_class_name_inner(context, boxed.as_ptr(), lookup_kind, out);
        let _ = values.release(boxed);
        status
    })
    .unwrap_or_else(|_| EvalStatus::RuntimeFatal.code())
}

/// Tests whether an object satisfies a class/interface relation in the eval context.
///
/// # Safety
/// `ctx` may be null for an eval-owned object, in which case its registered
/// owner context is used. `object` must point at a boxed runtime cell.
/// `target_ptr` must be readable for `target_len` bytes when `target_len > 0`.
#[cfg(not(test))]
#[no_mangle]
pub unsafe extern "C" fn __elephc_eval_object_is_a(
    ctx: *mut ElephcEvalContext,
    object: *mut RuntimeCell,
    target_ptr: *const u8,
    target_len: u64,
    exclude_self: u64,
) -> i32 {
    crate::ffi::util::trace_eval_ffi_entry("__elephc_eval_object_is_a");
    std::panic::catch_unwind(|| unsafe {
        eval_object_is_a_inner(ctx, object, target_ptr, target_len, exclude_self)
    })
    .unwrap_or(0)
}

/// Tests whether an object satisfies a runtime string/object class target.
///
/// # Safety
/// `ctx` must be a valid eval context handle. `object` and `target` must point
/// at boxed runtime cells. Returns -1 when the target is invalid for
/// `instanceof`.
#[cfg(not(test))]
#[no_mangle]
pub unsafe extern "C" fn __elephc_eval_object_is_a_dynamic(
    ctx: *mut ElephcEvalContext,
    object: *mut RuntimeCell,
    target: *mut RuntimeCell,
    exclude_self: u64,
) -> i32 {
    crate::ffi::util::trace_eval_ffi_entry("__elephc_eval_object_is_a_dynamic");
    std::panic::catch_unwind(|| unsafe {
        eval_object_is_a_dynamic_inner(ctx, object, target, exclude_self)
    })
    .unwrap_or(-1)
}

/// Tests whether a method or property exists in the eval context.
///
/// # Safety
/// `ctx` must be a valid eval context handle. `target` and `member` must point
/// at boxed runtime cells. `lookup_kind` must be a supported discriminator.
#[cfg(not(test))]
#[no_mangle]
pub unsafe extern "C" fn __elephc_eval_member_exists(
    ctx: *mut ElephcEvalContext,
    target: *mut RuntimeCell,
    member: *mut RuntimeCell,
    lookup_kind: u64,
) -> i32 {
    crate::ffi::util::trace_eval_ffi_entry("__elephc_eval_member_exists");
    std::panic::catch_unwind(|| unsafe {
        eval_member_exists_inner(ctx, target, member, lookup_kind)
    })
    .unwrap_or(0)
}

/// Answers `defined($name)` (`want_value == 0`) or `constant($name)` for a runtime name that the
/// compiled constant registry does not hold.
///
/// The registry only lists global constants; `Class::CONST` and enum-case names, and constants an
/// `eval()` defined, live on this side. A compiled function asks with a null handle, which resolves
/// to the shared null-handle context that holds the request's interpreted classes. `constant()` of
/// an unknown name publishes the `Error` php throws.
///
/// # Safety
/// `ctx` must be null or a valid eval context handle. `name` must point at a boxed runtime cell,
/// and `out` may be null.
#[cfg(not(test))]
#[no_mangle]
pub unsafe extern "C" fn __elephc_eval_dynamic_constant(
    ctx: *mut ElephcEvalContext,
    name: *mut RuntimeCell,
    want_value: u64,
    out: *mut ElephcEvalResult,
) -> i32 {
    crate::ffi::util::trace_eval_ffi_entry("__elephc_eval_dynamic_constant");
    std::panic::catch_unwind(|| unsafe {
        let context = match ctx.as_mut() {
            Some(context) => context,
            None => {
                let context = crate::ffi::context::shared_null_handle_context();
                context.sync_global_eval_classes();
                context
            }
        };
        if context.abi_version() != ABI_VERSION {
            return EvalStatus::AbiMismatch.code();
        }
        if name.is_null() {
            return EvalStatus::RuntimeFatal.code();
        }
        clear_result(out);
        let mut values = ElephcRuntimeOps::with_context(context as *const ElephcEvalContext);
        match interpreter::execute_context_dynamic_constant(
            context,
            want_value != 0,
            RuntimeCellHandle::from_raw(name),
            &mut values,
        ) {
            Ok(outcome) => write_outcome(outcome, out).code(),
            Err(status) => status.code(),
        }
    })
    .unwrap_or_else(|_| EvalStatus::RuntimeFatal.code())
}

/// Resolves class/interface/trait relation metadata in the eval context.
///
/// # Safety
/// `ctx` must be a valid eval context handle. `target` must point at a boxed
/// runtime cell. `relation_kind` must be a supported discriminator, and `out`
/// may be null.
#[cfg(not(test))]
#[no_mangle]
pub unsafe extern "C" fn __elephc_eval_class_relation(
    ctx: *mut ElephcEvalContext,
    target: *mut RuntimeCell,
    relation_kind: u64,
    out: *mut ElephcEvalResult,
) -> i32 {
    crate::ffi::util::trace_eval_ffi_entry("__elephc_eval_class_relation");
    std::panic::catch_unwind(|| unsafe {
        eval_class_relation_inner(ctx, target, relation_kind, out)
    })
    .unwrap_or_else(|_| EvalStatus::RuntimeFatal.code())
}

/// Runs the eval object class-name ABI body after installing a panic boundary.
///
/// # Safety
/// Mirrors `__elephc_eval_object_class_name`; callers must provide a valid
/// context, a boxed runtime cell, and optional writable result storage.
#[cfg(not(test))]
unsafe fn eval_object_class_name_inner(
    ctx: *mut ElephcEvalContext,
    object_or_class: *mut RuntimeCell,
    lookup_kind: u64,
    out: *mut ElephcEvalResult,
) -> i32 {
    let Some(context) = ctx.as_mut() else {
        return EvalStatus::RuntimeFatal.code();
    };
    if context.abi_version() != ABI_VERSION {
        return EvalStatus::AbiMismatch.code();
    }
    if object_or_class.is_null() {
        return EvalStatus::RuntimeFatal.code();
    }
    let lookup = match lookup_kind {
        CLASS_LOOKUP_GET_CLASS => "get_class",
        CLASS_LOOKUP_GET_PARENT_CLASS => "get_parent_class",
        _ => return EvalStatus::RuntimeFatal.code(),
    };
    clear_result(out);
    let mut values = ElephcRuntimeOps::with_context(context as *const ElephcEvalContext);
    match interpreter::execute_context_object_class_name(
        context,
        lookup,
        RuntimeCellHandle::from_raw(object_or_class).borrowed(),
        &mut values,
    ) {
        Ok(result) => write_outcome(EvalOutcome::Value(result), out).code(),
        Err(status) => status.code(),
    }
}

/// Runs the eval class-relation ABI body after installing a panic boundary.
///
/// # Safety
/// Mirrors `__elephc_eval_class_relation`; callers must provide a valid context,
/// a boxed runtime cell target, and optional writable result storage.
#[cfg(not(test))]
unsafe fn eval_class_relation_inner(
    ctx: *mut ElephcEvalContext,
    target: *mut RuntimeCell,
    relation_kind: u64,
    out: *mut ElephcEvalResult,
) -> i32 {
    if target.is_null() {
        return EvalStatus::RuntimeFatal.code();
    }
    let lookup = match relation_kind {
        CLASS_RELATION_IMPLEMENTS => "class_implements",
        CLASS_RELATION_PARENTS => "class_parents",
        CLASS_RELATION_USES => "class_uses",
        _ => return EvalStatus::RuntimeFatal.code(),
    };
    // A compiled function with no context of its own asks with a null one: the class it names was
    // declared by an eval somewhere else, so a fresh context that imports the process-global class
    // metadata answers for it. `class_uses_recursive($model)` is such a helper.
    let (context_ptr, created_context) = if ctx.is_null() {
        let context = crate::ffi::context::__elephc_eval_context_new();
        if context.is_null() {
            return EvalStatus::RuntimeFatal.code();
        }
        (context, true)
    } else {
        (ctx, false)
    };
    let Some(context) = context_ptr.as_mut() else {
        return EvalStatus::RuntimeFatal.code();
    };
    if context.abi_version() != ABI_VERSION {
        if created_context {
            crate::ffi::context::finalize_eval_context_free(context_ptr);
        }
        return EvalStatus::AbiMismatch.code();
    }
    if created_context {
        crate::context::sync_global_eval_aot_metadata(context);
        context.sync_global_eval_classes();
    }
    clear_result(out);
    let mut values = ElephcRuntimeOps::with_context(context as *const ElephcEvalContext);
    let status = match interpreter::execute_context_class_relation(
        context,
        lookup,
        RuntimeCellHandle::from_raw(target).borrowed(),
        &mut values,
    ) {
        Ok(result) => write_outcome(EvalOutcome::Value(result), out).code(),
        Err(status) => status.code(),
    };
    if created_context {
        crate::ffi::context::finalize_eval_context_free(context_ptr);
    }
    status
}

/// Runs the eval object relation ABI body after installing a panic boundary.
///
/// # Safety
/// Mirrors `__elephc_eval_object_is_a`; invalid handles or unreadable target
/// storage fail closed as false.
#[cfg(not(test))]
unsafe fn eval_object_is_a_inner(
    ctx: *mut ElephcEvalContext,
    object: *mut RuntimeCell,
    target_ptr: *const u8,
    target_len: u64,
    exclude_self: u64,
) -> i32 {
    if object.is_null() {
        return 0;
    }
    let object = RuntimeCellHandle::from_raw(object);
    let trace = crate::eval_trace::enabled();
    let Ok(target) = abi_name_to_string(target_ptr, target_len) else {
        return 0;
    };
    // Without a caller context the answer feeds compiled code that falls back to its own class-id
    // matcher on a 0. For an object an eval context declared that matcher only sees the generic
    // carrier header, `stdClass`, so it answered `true` for `$evalObject instanceof \stdClass` --
    // league/config then cast every configured service object to an array. Such an object's
    // definitive "no" is reported as `EVAL_OBJECT_IS_A_DEFINITELY_NOT` so the caller stops there.
    let caller_takes_definite_no = ctx.is_null();
    let context = if let Some(context) = ctx.as_mut() {
        context
    } else {
        let mut values = ElephcRuntimeOps::with_context(std::ptr::null());
        let Ok(identity) = values.object_identity(object) else {
            if trace {
                eprintln!("[elephc-eval-trace] phase=object_is_a owner_lookup=invalid_object");
            }
            return 0;
        };
        let Some(owner) = crate::ffi::dynamic_destructors::dynamic_object_owner_context(identity)
        else {
            if trace {
                let class_name = values
                    .object_class_name(object)
                    .ok()
                    .and_then(|class_name| {
                        let bytes = values.string_bytes(class_name).ok();
                        let _ = values.release(class_name);
                        bytes.and_then(|bytes| String::from_utf8(bytes).ok())
                    });
                eprintln!(
                    "[elephc-eval-trace] phase=object_is_a owner_lookup=missing \
                     identity={identity} class={class_name:?} target={target:?} \
                     exclude_self={}",
                    exclude_self != 0,
                );
            }
            return i32::from(
                values
                    .object_is_a(object, &target, exclude_self != 0)
                    .unwrap_or(false),
            );
        };
        let Some(context) = owner.as_mut() else {
            if trace {
                eprintln!("[elephc-eval-trace] phase=object_is_a owner_lookup=dangling identity={identity}");
            }
            return 0;
        };
        if trace {
            eprintln!("[elephc-eval-trace] phase=object_is_a owner_lookup=found identity={identity} context={context:p}");
        }
        context
    };
    if context.abi_version() != ABI_VERSION {
        return 0;
    }
    let mut values = ElephcRuntimeOps::with_context(context as *const ElephcEvalContext);
    let traced_subject = trace.then(|| eval_object_is_a_trace_subject(object, context, &mut values));
    let result = interpreter::execute_context_object_is_a(
        context,
        object.borrowed(),
        &target,
        exclude_self != 0,
        &mut values,
    );
    if let Some((identity, class, decided_by)) = traced_subject {
        eprintln!(
            "[elephc-eval-trace] phase=object_is_a identity={identity} class={class:?} \
             decided_by={decided_by} target={target:?} context={context:p} result={result:?}"
        );
    }
    match result {
        Ok(true) => 1,
        Ok(false) if caller_takes_definite_no => {
            let eval_declared = values
                .object_identity(object)
                .ok()
                .is_some_and(|identity| context.dynamic_object_declaring_class(identity).is_some());
            if eval_declared {
                EVAL_OBJECT_IS_A_DEFINITELY_NOT
            } else {
                0
            }
        }
        Ok(false) => 0,
        Err(_) => 0,
    }
}

/// `__elephc_eval_object_is_a` answer, for a caller that passed no context, meaning "an object an
/// eval context declared, and it is NOT an instance": do not ask the compiled matcher, whose view
/// of such an object is only its carrier header.
const EVAL_OBJECT_IS_A_DEFINITELY_NOT: i32 = 2;

/// Names the object one `object_is_a` probe was asked about, for `ELEPHC_EVAL_TRACE`.
///
/// The relation answer alone cannot be checked against PHP: the same target is asked about many
/// different receivers in one request, so a trace without the receiver's identity and class says
/// nothing about which answer is wrong. `decided_by` reports which table produced the answer —
/// the eval class table of the context that DECLARED the object, or the generated AOT metadata
/// the bridge falls back to when no eval class owns the identity.
#[cfg(not(test))]
fn eval_object_is_a_trace_subject(
    object: RuntimeCellHandle,
    context: &ElephcEvalContext,
    values: &mut ElephcRuntimeOps,
) -> (u64, String, &'static str) {
    let Ok(identity) = values.object_identity(object) else {
        return (0, String::from("<invalid>"), "invalid-object");
    };
    let decided_by = if context.dynamic_object_declaring_class(identity).is_some() {
        "eval-class"
    } else {
        "aot-metadata"
    };
    let runtime_class = values
        .object_class_name(object)
        .ok()
        .and_then(|class_name| {
            let bytes = values.string_bytes(class_name).ok();
            let _ = values.release(class_name);
            bytes.and_then(|bytes| String::from_utf8(bytes).ok())
        })
        .unwrap_or_default();
    let eval_class = context.dynamic_object_class_name(identity).unwrap_or_default();
    (identity, format!("{eval_class}|runtime={runtime_class}"), decided_by)
}

/// Runs the eval dynamic object relation ABI body after installing a panic boundary.
///
/// # Safety
/// Mirrors `__elephc_eval_object_is_a_dynamic`; invalid target cells report -1
/// so generated code can use the normal dynamic-`instanceof` fatal path.
#[cfg(not(test))]
unsafe fn eval_object_is_a_dynamic_inner(
    ctx: *mut ElephcEvalContext,
    object: *mut RuntimeCell,
    target: *mut RuntimeCell,
    exclude_self: u64,
) -> i32 {
    let Some(context) = ctx.as_mut() else {
        return -1;
    };
    if context.abi_version() != ABI_VERSION || object.is_null() || target.is_null() {
        return -1;
    }
    let mut values = ElephcRuntimeOps::with_context(context as *const ElephcEvalContext);
    match interpreter::execute_context_object_is_a_dynamic(
        context,
        RuntimeCellHandle::from_raw(object).borrowed(),
        RuntimeCellHandle::from_raw(target).borrowed(),
        exclude_self != 0,
        &mut values,
    ) {
        Ok(result) => i32::from(result),
        Err(_) => -1,
    }
}

/// Runs the eval member-exists ABI body after installing a panic boundary.
///
/// # Safety
/// Mirrors `__elephc_eval_member_exists`; invalid handles fail closed as false.
#[cfg(not(test))]
unsafe fn eval_member_exists_inner(
    ctx: *mut ElephcEvalContext,
    target: *mut RuntimeCell,
    member: *mut RuntimeCell,
    lookup_kind: u64,
) -> i32 {
    if target.is_null() || member.is_null() {
        return 0;
    }
    // An OBJECT an eval context made is answered by that context, which declared its class: a
    // compiled frame's own context is fresh and has never seen it. Twig's compiled
    // `Template::getTemplateForMacro()` runs `method_exists($this, 'macro_x')` on a template class
    // the interpreter declared, from a frame whose context knows nothing of it (and from frames
    // with no context at all, the null-context probe). With no owner, a null context answers 0,
    // which sends the caller to its native class tables.
    let owner = {
        let mut values = ElephcRuntimeOps::with_context(std::ptr::null());
        values
            .object_identity(RuntimeCellHandle::from_raw(target))
            .ok()
            .and_then(crate::ffi::dynamic_destructors::dynamic_object_owner_context)
            .and_then(|owner| owner.as_mut())
    };
    let Some(context) = owner.or_else(|| ctx.as_mut()) else {
        return 0;
    };
    if context.abi_version() != ABI_VERSION {
        return 0;
    }
    let lookup = match lookup_kind {
        MEMBER_LOOKUP_METHOD_EXISTS => "method_exists",
        MEMBER_LOOKUP_PROPERTY_EXISTS => "property_exists",
        _ => return 0,
    };
    let mut values = ElephcRuntimeOps::with_context(context as *const ElephcEvalContext);
    match interpreter::execute_context_member_exists(
        context,
        lookup,
        RuntimeCellHandle::from_raw(target).borrowed(),
        RuntimeCellHandle::from_raw(member).borrowed(),
        &mut values,
    ) {
        Ok(result) => i32::from(result),
        Err(_) => 0,
    }
}
