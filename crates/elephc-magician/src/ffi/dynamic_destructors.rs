//! Purpose:
//! Owns the process-local registry that lets native object release call back
//! into eval-declared `__destruct()` methods for dynamic classes, and the sibling
//! callback that lets a generated `clone` reach the same eval class metadata.
//!
//! Called from:
//! - `crate::context::ElephcEvalContext` when dynamic objects are registered or freed.
//! - The generated runtime through the installed destructor and clone hook function pointers.
//!
//! Key details:
//! - The runtime owns object storage and calls this hook while the object is still
//!   intact, either in a pinned collector pass or in the final-release path.
//! - Registry values are stored as integer addresses so the global mutex remains
//!   `Sync`; every use revalidates null pointers and ABI version.

#[cfg(not(test))]
use crate::abi::ABI_VERSION;
use crate::abi::ElephcEvalContext;
#[cfg(not(test))]
use crate::errors::EvalStatus;
#[cfg(not(test))]
use crate::interpreter::eval_dynamic_destructor_for_object_cell;
#[cfg(not(test))]
use crate::interpreter::eval_json_encode_serialized_object;
#[cfg(not(test))]
use crate::interpreter::eval_object_clone_with_properties_for_ffi;
#[cfg(not(test))]
use crate::interpreter::RuntimeValueOps;
#[cfg(not(test))]
use crate::runtime_hooks::{self, ElephcRuntimeOps};
#[cfg(not(test))]
use crate::value::{RuntimeCell, RuntimeCellHandle};
use std::collections::HashMap;
use std::hash::{BuildHasherDefault, Hasher};
use std::sync::atomic::{AtomicU32, Ordering};
use std::sync::{Mutex, OnceLock};

/// Hashes an object address for the owner registry.
///
/// The runtime asks this registry about EVERY object it frees once the bridge is linked, and
/// SipHash on those lookups was 4% of a Symfony request. Addresses are not adversarial input,
/// only aligned: the low four bits carry nothing, and the bucket is taken from the low bits, so
/// they are dropped before a multiplicative mix whose high half is folded back down.
#[derive(Default)]
struct ObjectAddressHasher(u64);

impl Hasher for ObjectAddressHasher {
    fn write(&mut self, bytes: &[u8]) {
        for byte in bytes {
            self.0 = (self.0.rotate_left(8) ^ u64::from(*byte)).wrapping_mul(0x9E37_79B9_7F4A_7C15);
        }
    }

    fn write_u64(&mut self, address: u64) {
        let mixed = (address >> 4).wrapping_mul(0x9E37_79B9_7F4A_7C15);
        self.0 = mixed ^ (mixed >> 32);
    }

    fn finish(&self) -> u64 {
        self.0
    }
}

type ObjectOwnerMap = HashMap<u64, usize, BuildHasherDefault<ObjectAddressHasher>>;

static DYNAMIC_DESTRUCTOR_CONTEXTS: OnceLock<Mutex<ObjectOwnerMap>> = OnceLock::new();

/// Buckets in the owner filter. Must match `EVAL_OWNER_FILTER_MASK` in the generated runtime,
/// which picks a bucket as `(identity >> 4) & (OWNER_FILTER_BUCKETS - 1)`.
pub(crate) const OWNER_FILTER_BUCKETS: usize = 4096;

/// How many registered identities fall in each bucket, read by generated code without a call.
///
/// WHY. Once the bridge is linked, every non-final method call and declared-property read on a
/// typed receiver asks this registry whether the receiver is eval-owned, and every object free
/// asks it for an eval destructor: some 900 calls and a mutex per Symfony request, for a map
/// that holds a handful of objects. A zero bucket answers "no owner" for all of them. The counts
/// change only under the registry mutex and always equal the map's membership per bucket, so a
/// zero is exact; a non-zero bucket only means "ask".
static OWNER_FILTER: [AtomicU32; OWNER_FILTER_BUCKETS] =
    [const { AtomicU32::new(0) }; OWNER_FILTER_BUCKETS];

/// Returns the filter bucket of one object identity, as the generated probe computes it.
fn owner_filter_bucket(identity: u64) -> &'static AtomicU32 {
    &OWNER_FILTER[(identity >> 4) as usize & (OWNER_FILTER_BUCKETS - 1)]
}

/// Returns the process-local dynamic object to eval context registry.
fn dynamic_destructor_contexts() -> &'static Mutex<ObjectOwnerMap> {
    DYNAMIC_DESTRUCTOR_CONTEXTS.get_or_init(|| Mutex::new(ObjectOwnerMap::default()))
}

/// Reports whether a raw object identity belongs to a live eval-declared class.
/// Native property helpers use this only after ordinary declared-property dispatch misses.
#[cfg(not(test))]
#[no_mangle]
pub extern "C" fn __elephc_eval_dynamic_object_owns_properties(identity: u64) -> u64 {
    let Some(context) = dynamic_object_owner_context(identity) else { return 0; };
    // Closure identities also use this registry for metadata cleanup, but their
    // stdClass payload does not have eval-declared property storage.
    let Some(context) = (unsafe { context.as_ref() }) else { return 0; };
    u64::from(context.abi_version() == ABI_VERSION && context.dynamic_object_class(identity).is_some())
}

/// Installs the eval dynamic object destructor and clone callbacks into the generated runtime.
#[cfg(not(test))]
pub(crate) fn install_dynamic_object_destructor_hook() {
    unsafe {
        runtime_hooks::install_dynamic_object_destructor_hook(
            __elephc_eval_dynamic_object_destruct as *const () as usize,
            OWNER_FILTER.as_ptr() as usize,
        );
        runtime_hooks::install_dynamic_object_clone_hook(
            __elephc_eval_dynamic_object_clone as *const () as usize,
        );
        runtime_hooks::install_closure_bind_hook(
            crate::ffi::callables::__elephc_eval_closure_bind_this as *const () as usize,
        );
    }
}

/// Records which eval context owns one dynamic object's eval class metadata.
pub(crate) fn register_dynamic_object_context(identity: u64, context: *mut ElephcEvalContext) {
    if identity == 0 || context.is_null() {
        return;
    }
    if let Ok(mut contexts) = dynamic_destructor_contexts().lock() {
        if contexts.insert(identity, context as usize).is_none() {
            owner_filter_bucket(identity).fetch_add(1, Ordering::Relaxed);
        }
    }
}

/// Removes one dynamic object from the process-local destructor registry.
pub(crate) fn unregister_dynamic_object(identity: u64) {
    if identity == 0 {
        return;
    }
    if let Ok(mut contexts) = dynamic_destructor_contexts().lock() {
        if contexts.remove(&identity).is_some() {
            owner_filter_bucket(identity).fetch_sub(1, Ordering::Relaxed);
        }
    }
}

/// Removes every dynamic object currently associated with a soon-to-be-freed context.
pub(crate) fn unregister_dynamic_objects_for_context(context: *mut ElephcEvalContext) {
    if context.is_null() {
        return;
    }
    let context = context as usize;
    if let Ok(mut contexts) = dynamic_destructor_contexts().lock() {
        contexts.retain(|identity, owner| {
            let keep = *owner != context;
            if !keep {
                owner_filter_bucket(*identity).fetch_sub(1, Ordering::Relaxed);
            }
            keep
        });
    }
}

/// Forgets every dynamic object at a generated web request boundary.
///
/// The key is the object's IDENTITY, which is its address in the PHP arena, and `__rt_web_reset`
/// hands that whole arena back to pure-bump allocation. Keeping the map means the next request's
/// object lands on a previous request's address and inherits its entry: `__rt_heap_free` then
/// asks this registry for an owner, gets one, and runs a `__destruct()` belonging to a class the
/// new object never had, through a context whose own tables were emptied at the same boundary.
/// Nothing in the map can still be valid after the wipe, so the whole map goes.
#[cfg(not(test))]
pub(crate) fn reset_dynamic_object_contexts() {
    if let Ok(mut contexts) = dynamic_destructor_contexts().lock() {
        contexts.clear();
        for bucket in &OWNER_FILTER {
            bucket.store(0, Ordering::Relaxed);
        }
    }
}

/// Unit tests run a single interpreter context with no registry, so no object has a foreign
/// owner; the cross-context closure path (`eval_foreign_closure_owner`) then stays inert.
#[cfg(test)]
pub(crate) fn dynamic_object_owner_context(_identity: u64) -> Option<*mut ElephcEvalContext> {
    None
}

/// Looks up the eval context that owns one dynamic object identity.
#[cfg(not(test))]
pub(crate) fn dynamic_object_owner_context(identity: u64) -> Option<*mut ElephcEvalContext> {
    let contexts = dynamic_destructor_contexts().lock().ok()?;
    let context = *contexts.get(&identity)?;
    Some(context as *mut ElephcEvalContext)
}

/// Returns one borrowed cell the owning eval context holds for an object, or zero past the end.
///
/// Feeds the native cycle collector's hidden-edge walk (`object_owners::object_gc_child`): see
/// `ElephcEvalContext::dynamic_object_gc_child`. The owner filter answers most objects, which
/// have no eval owner, without taking the registry mutex.
#[cfg(not(test))]
pub(crate) fn dynamic_object_gc_child(identity: u64, index: usize) -> usize {
    if identity == 0 || owner_filter_bucket(identity).load(Ordering::Relaxed) == 0 {
        return 0;
    }
    let Some(context) = dynamic_object_owner_context(identity) else { return 0; };
    let Some(context) = (unsafe { context.as_ref() }) else { return 0; };
    if context.abi_version() != ABI_VERSION {
        return 0;
    }
    context
        .dynamic_object_gc_child(identity, index)
        .map_or(0, |cell| cell.as_ptr() as usize)
}

/// Unit tests have no owner registry, so no object has context-held edges.
#[cfg(test)]
pub(crate) fn dynamic_object_gc_child(_identity: u64, _index: usize) -> usize {
    0
}

/// Checks an object identity without allocating argument cells or dereferencing the object.
#[cfg(not(test))]
#[no_mangle]
pub extern "C" fn __elephc_eval_object_has_dynamic_owner(identity: u64) -> u64 {
    u64::from(dynamic_object_owner_context(identity).is_some())
}

/// Answers what a compiled `json_encode()` should encode for an object the interpreter owns.
///
/// Returns an owned boxed Mixed cell -- the object's `jsonSerialize()` result, or an array of its
/// public properties -- or null when the interpreter does not own the object or cannot answer, in
/// which case the compiled encoder emits `null`.
///
/// # Safety
/// `object` must be null or a live elephc runtime object pointer.
#[cfg(not(test))]
#[no_mangle]
pub unsafe extern "C" fn __elephc_eval_json_serialize(object: *mut RuntimeCell) -> *mut RuntimeCell {
    crate::ffi::util::trace_eval_ffi_entry("__elephc_eval_json_serialize");
    std::panic::catch_unwind(|| unsafe { json_serialize_inner(object) })
        .unwrap_or(std::ptr::null_mut())
}

/// Executes the serializer body after the exported ABI shim has installed a panic boundary.
///
/// # Safety
/// Mirrors `__elephc_eval_json_serialize`.
#[cfg(not(test))]
unsafe fn json_serialize_inner(object: *mut RuntimeCell) -> *mut RuntimeCell {
    if object.is_null() {
        return std::ptr::null_mut();
    }
    let Some(context) = dynamic_object_owner_context(object as u64) else {
        return std::ptr::null_mut();
    };
    let Some(context) = (unsafe { context.as_mut() }) else {
        return std::ptr::null_mut();
    };
    if context.abi_version() != ABI_VERSION {
        return std::ptr::null_mut();
    }
    let mut values = ElephcRuntimeOps::with_context(context as *const ElephcEvalContext);
    let Ok(object_cell) = ElephcRuntimeOps::object_from_raw(object) else {
        return std::ptr::null_mut();
    };
    let result = eval_json_encode_serialized_object(object_cell, context, &mut values);
    let _ = values.release(object_cell);
    match result {
        Ok(handle) => handle.as_ptr(),
        Err(_) => std::ptr::null_mut(),
    }
}

/// Drops final object metadata after receiver release, preserving it during collector destructors.
#[cfg(not(test))]
pub(crate) fn forget_released_object(identity: u64) {
    let Some(context_ptr) = dynamic_object_owner_context(identity) else { return; };
    // Context teardown unregisters its identities before freeing the context.
    let Some(context) = (unsafe { context_ptr.as_mut() }) else { return; };
    if context.abi_version() == ABI_VERSION {
        // The context also owns the object's property overlay values and any array-iterator
        // storage or lazy-ghost initializer; they are released here, with the identity, and the
        // context itself is finalized when this was the last thing keeping it alive.
        let (property_values, should_finalize) = context.forget_dynamic_object(identity);
        let mut values = ElephcRuntimeOps::with_context(context_ptr as *const ElephcEvalContext);
        for value in property_values {
            let _ = values.release(value);
        }
        drop(values);
        if should_finalize {
            unsafe { crate::ffi::context::finalize_eval_context_free(context_ptr) };
        }
    }
}

/// Drops eval metadata when the native runtime frees an object's final owner.
#[cfg(not(test))]
#[no_mangle]
pub unsafe extern "C" fn __elephc_eval_dynamic_object_forget(object: *mut RuntimeCell) {
    forget_released_object(object as u64);
}

/// Lifts an escaping eval Throwable out of a bridge result, retaining a borrowed one.
///
/// Split out so the cleanup below can run even when taking or retaining the Throwable fails.
/// Returning `Err` from inside the `escaped` expression itself used to skip both the pending-throw
/// restore and the receiver release.
#[cfg(not(test))]
fn take_escaping_throwable<T>(
    result: &Result<T, EvalStatus>,
    context: &mut ElephcEvalContext,
    values: &mut ElephcRuntimeOps,
) -> Result<Option<RuntimeCellHandle>, EvalStatus> {
    if !matches!(result, Err(EvalStatus::UncaughtThrowable)) {
        return Ok(None);
    }
    let thrown = context.take_pending_throw().ok_or(EvalStatus::RuntimeFatal)?;
    Ok(Some(if thrown.is_borrowed() { values.retain(thrown)? } else { thrown }))
}

/// Restores the caller's pending throw and retires the bridge's own receiver box, unconditionally.
///
/// Both steps run before either failure is propagated, so no bridge exit path can drop the
/// caller's pending throw or leak the boxed `$this` the bridge made for the operation.
#[cfg(not(test))]
fn finish_bridge_ownership(
    escaped: Result<Option<RuntimeCellHandle>, EvalStatus>,
    previous_throw: Option<RuntimeCellHandle>,
    object_cell: RuntimeCellHandle,
    context: &mut ElephcEvalContext,
    values: &mut ElephcRuntimeOps,
) -> Result<Option<RuntimeCellHandle>, EvalStatus> {
    if let Some(previous) = previous_throw {
        context.set_pending_throw(previous);
    }
    let released = values.release(object_cell);
    let escaped = escaped?;
    released?;
    Ok(escaped)
}

/// Runs an eval destructor, returning zero for a miss, one for success, or two with an owned Throwable.
///
/// # Safety
/// `object` must be null or a live elephc runtime object pointer. The runtime
/// calls this only while its object destruction guard bit is set, so boxing the
/// borrowed object for `$this` cannot recursively free the same storage.
/// `throwable_out` must point to a writable cell-pointer slot. Throws are transferred
/// through that output only after Rust has returned; native unwinding must not cross Rust frames.
#[cfg(not(test))]
#[no_mangle]
pub unsafe extern "C" fn __elephc_eval_dynamic_object_destruct(
    object: *mut RuntimeCell,
    throwable_out: *mut *mut RuntimeCell,
) -> u64 {
    crate::ffi::util::trace_eval_ffi_entry("__elephc_eval_dynamic_object_destruct");
    match std::panic::catch_unwind(|| unsafe { dynamic_object_destruct_inner(object, throwable_out) }) {
        Ok(Ok(status)) => status,
        Ok(Err(_)) | Err(_) => {
            let mut values = ElephcRuntimeOps::with_context(std::ptr::null());
            let _ = values.fatal("Fatal error: eval() destructor failed\n");
            std::process::abort()
        }
    }
}

/// Executes the callback body after the exported ABI shim has installed a panic boundary.
///
/// # Safety
/// Mirrors `__elephc_eval_dynamic_object_destruct`; callers must pass a live raw
/// object pointer whose refcount guard marks destruction active and a writable output slot.
#[cfg(not(test))]
unsafe fn dynamic_object_destruct_inner(
    object: *mut RuntimeCell,
    throwable_out: *mut *mut RuntimeCell,
) -> Result<u64, EvalStatus> {
    if throwable_out.is_null() {
        return Err(EvalStatus::RuntimeFatal);
    }
    unsafe { *throwable_out = std::ptr::null_mut(); }
    if object.is_null() {
        return Ok(0);
    }
    let identity = object as u64;
    if crate::eval_trace::enabled() {
        let owner_class = dynamic_object_owner_context(identity)
            .and_then(|owner| unsafe { owner.as_ref() })
            .and_then(|context| context.dynamic_object_class(identity))
            .map(|class| class.name().to_string());
        eprintln!(
            "[elephc-eval-trace] phase=dynamic_object_destruct_begin object={object:p} identity={identity} class={owner_class:?}"
        );
    }
    let Some(context) = dynamic_object_owner_context(identity) else {
        return Ok(0);
    };
    let Some(context) = (unsafe { context.as_mut() }) else {
        unregister_dynamic_object(identity);
        return Ok(0);
    };
    if context.abi_version() != ABI_VERSION {
        unregister_dynamic_object(identity);
        return Ok(0);
    }
    // A generator the interpreter created is an instance of the compiled `Generator` class, not
    // of an eval class, but its frame lives in this context and has to go with the object:
    // `eval_dynamic_destructor_for_object_cell` runs its pending `finally` blocks and drops it.
    // Returning here left the frame registered under the freed ADDRESS, and the next compiled
    // generator allocated there was answered for by the dead one -- "finished" before it ran,
    // so an interpreted `yield from compiled_gen()` produced nothing the second time (every
    // Twig block after the first rendered empty).
    if context.dynamic_object_class(identity).is_none() && !context.has_eval_generator(identity) {
        // Closure metadata may own a foreign eval context needed by its receiver's destructor.
        // The receiver is owned by the closure object's native child edges, so the final owner
        // callback (`forget_released_object`) drops the metadata after that child release.
        if context.closure_object_target(identity).is_none() {
            unregister_dynamic_object(identity);
        }
        return Ok(0);
    }

    let mut values = ElephcRuntimeOps::with_context(context as *const ElephcEvalContext);
    let object_cell = ElephcRuntimeOps::object_from_raw(object)?;
    let previous_throw = context.take_pending_throw();
    let destruct_result =
        eval_dynamic_destructor_for_object_cell(identity, object_cell, context, &mut values);
    let escaped = take_escaping_throwable(&destruct_result, context, &mut values);
    let escaped =
        finish_bridge_ownership(escaped, previous_throw, object_cell, context, &mut values)?;
    // The collector can still retain or resurrect the receiver. Final runtime
    // release, not destructor execution, retires its class and property metadata.
    if let Some(thrown) = escaped {
        unsafe { *throwable_out = thrown.as_ptr(); }
        return Ok(2);
    }
    destruct_result.map(u64::from)
}

/// Writes a property on an eval-declared object under eval's visibility and `__set` rules.
///
/// The generated stdClass write arm is where a boxed receiver lands when no declared slot
/// matched, and an eval-declared class instance is backed by exactly that layout. Writing
/// straight into its hash bypassed the class's private slots and its `__set` guard, so a
/// `mixed $object` parameter could create `$object->hidden` where php raises
/// `Cannot access private property`. Returns zero when Magician does not own `object` (the
/// generated path continues untouched), one when the write completed, or two with an owned
/// Throwable in `throwable_out`.
///
/// The write runs under the CALLER's lexical class scope, `scope_ptr`/`scope_len` (empty for a
/// free function or the top level), not under whatever eval method happens to be on the class
/// stack: an eval `__set` that hands `$this` to a generated free function must see that function
/// refused, exactly as php refuses it.
///
/// `value_block` is one writable three-word stack block: word 0 receives an owned Throwable on
/// status two (it is cleared first), word 2 holds the boxed value cell the caller keeps owning —
/// the write retains its own reference. Packing them keeps the call inside the six integer
/// argument registers both supported ABIs share.
///
/// # Safety
/// `object` must be null or a live elephc runtime object pointer that stays borrowed for the
/// call. `name_ptr` must be readable for `name_len` bytes and `scope_ptr` for `scope_len` bytes.
/// `value_block` must point at three writable native words as described above; a Throwable is
/// transferred through word 0 only after Rust has returned, so native unwinding never crosses
/// a Rust frame.
#[cfg(not(test))]
#[no_mangle]
pub unsafe extern "C" fn __elephc_eval_dynamic_object_property_set(
    object: *mut RuntimeCell,
    name_ptr: *const u8,
    name_len: u64,
    value_block: *mut *mut RuntimeCell,
    scope_ptr: *const u8,
    scope_len: u64,
) -> u64 {
    let written = std::panic::catch_unwind(|| unsafe {
        dynamic_object_property_set_inner(
            object, name_ptr, name_len, value_block, scope_ptr, scope_len,
        )
    });
    match written {
        Ok(Ok(status)) => status,
        Ok(Err(_)) | Err(_) => {
            let mut values = ElephcRuntimeOps::with_context(std::ptr::null());
            let _ = values.fatal("Fatal error: eval() property write failed\n");
            std::process::abort()
        }
    }
}

/// Executes the property write after the exported ABI shim has installed a panic boundary.
///
/// # Safety
/// Mirrors `__elephc_eval_dynamic_object_property_set`.
#[cfg(not(test))]
unsafe fn dynamic_object_property_set_inner(
    object: *mut RuntimeCell,
    name_ptr: *const u8,
    name_len: u64,
    value_block: *mut *mut RuntimeCell,
    scope_ptr: *const u8,
    scope_len: u64,
) -> Result<u64, EvalStatus> {
    if value_block.is_null() {
        return Err(EvalStatus::RuntimeFatal);
    }
    let throwable_out = value_block;
    let value = unsafe { *value_block.add(2) };
    unsafe { *throwable_out = std::ptr::null_mut(); }
    if object.is_null() || value.is_null() {
        return Ok(0);
    }
    let identity = object as u64;
    let Some(context) = dynamic_object_owner_context(identity) else {
        return Ok(0);
    };
    let Some(context) = (unsafe { context.as_mut() }) else {
        return Ok(0);
    };
    if context.abi_version() != ABI_VERSION || context.dynamic_object_class(identity).is_none() {
        return Ok(0);
    }
    let name = crate::ffi::util::abi_name_to_string(name_ptr, name_len)?;
    let scope = crate::ffi::util::abi_name_to_string(scope_ptr, scope_len)?;
    let mut values = ElephcRuntimeOps::with_context(context as *const ElephcEvalContext);
    let object_cell = ElephcRuntimeOps::object_from_raw(object)?;
    let previous_throw = context.take_pending_throw();
    // An empty name reads back as "no class scope", which is the free-function answer.
    context.push_class_scope(scope);
    let written = crate::interpreter::eval_property_set_for_ffi(
        object_cell,
        &name,
        RuntimeCellHandle::from_raw(value).borrowed(),
        context,
        &mut values,
    );
    context.pop_class_scope();
    let escaped = take_escaping_throwable(&written, context, &mut values);
    let escaped =
        finish_bridge_ownership(escaped, previous_throw, object_cell, context, &mut values)?;
    if let Some(thrown) = escaped {
        unsafe { *throwable_out = thrown.as_ptr(); }
        return Ok(2);
    }
    written.map(|()| 1)
}

/// Clones an eval-owned object, returning zero for a miss, one for success, or two with an owned Throwable.
///
/// The generated `clone` lowering calls this before its own shallow-clone adapter. A zero
/// status means Magician does not own the identity, so the AOT path continues untouched and
/// ordinary generated objects, stdClass, and enums never change behavior.
///
/// # Safety
/// `object` must be null or a live elephc runtime object pointer that the caller keeps
/// alive for the whole call; it stays borrowed. `properties` must be null or a borrowed
/// boxed PHP array cell. `scope_ptr` must be readable for `scope_len` bytes when
/// `scope_len > 0` and must hold the caller's lexical class name in UTF-8. `clone_out` and
/// `throwable_out` must point at writable cell-pointer slots; both receive owned boxes.
/// Throws are transferred through `throwable_out` only after Rust has returned, so native
/// unwinding never crosses a Rust frame.
#[cfg(not(test))]
#[no_mangle]
pub unsafe extern "C" fn __elephc_eval_dynamic_object_clone(
    object: *mut RuntimeCell,
    properties: *mut RuntimeCell,
    scope_ptr: *const u8,
    scope_len: u64,
    clone_out: *mut *mut RuntimeCell,
    throwable_out: *mut *mut RuntimeCell,
) -> u64 {
    let cloned = std::panic::catch_unwind(|| unsafe {
        dynamic_object_clone_inner(object, properties, scope_ptr, scope_len, clone_out, throwable_out)
    });
    match cloned {
        Ok(Ok(status)) => status,
        Ok(Err(_)) | Err(_) => {
            let mut values = ElephcRuntimeOps::with_context(std::ptr::null());
            let _ = values.fatal("Fatal error: eval() clone failed\n");
            std::process::abort()
        }
    }
}

/// Executes the clone callback body after the exported ABI shim has installed a panic boundary.
///
/// # Safety
/// Mirrors `__elephc_eval_dynamic_object_clone`; callers must pass a live borrowed raw
/// object pointer, a borrowed or null override array box, readable scope bytes, and two
/// writable output slots.
#[cfg(not(test))]
unsafe fn dynamic_object_clone_inner(
    object: *mut RuntimeCell,
    properties: *mut RuntimeCell,
    scope_ptr: *const u8,
    scope_len: u64,
    clone_out: *mut *mut RuntimeCell,
    throwable_out: *mut *mut RuntimeCell,
) -> Result<u64, EvalStatus> {
    if clone_out.is_null() || throwable_out.is_null() {
        return Err(EvalStatus::RuntimeFatal);
    }
    unsafe {
        *clone_out = std::ptr::null_mut();
        *throwable_out = std::ptr::null_mut();
    }
    if object.is_null() {
        return Ok(0);
    }
    let identity = object as u64;
    let Some(context) = dynamic_object_owner_context(identity) else {
        return Ok(0);
    };
    let Some(context) = (unsafe { context.as_mut() }) else {
        return Ok(0);
    };
    if context.abi_version() != ABI_VERSION {
        return Ok(0);
    }
    // Only an identity with live eval class metadata can be cloned here. Closure payloads and
    // generated objects that merely pass through this registry stay on the AOT clone path.
    if context.dynamic_object_class(identity).is_none() {
        return Ok(0);
    }
    let scope = unsafe { aot_invocation_scope(scope_ptr, scope_len) };

    let mut values = ElephcRuntimeOps::with_context(context as *const ElephcEvalContext);
    // The borrowed raw object needs one boxed owner for the shared clone operation; it is
    // retired below so the caller's own reference is the only one that survives the call.
    let object_cell = ElephcRuntimeOps::object_from_raw(object)?;
    // The override array stays owned by the caller for the whole call.
    let overrides =
        (!properties.is_null()).then(|| RuntimeCellHandle::from_raw(properties).borrowed());
    let previous_throw = context.take_pending_throw();
    // PHP checks `__clone()` visibility against the INVOCATION scope, so the caller's lexical
    // AOT class is pushed for the operation instead of letting it observe global scope.
    if let Some(scope) = scope.as_deref() {
        context.push_class_scope(scope);
    }
    let clone_result =
        eval_object_clone_with_properties_for_ffi(object_cell, overrides, None, context, &mut values);
    if scope.is_some() {
        context.pop_class_scope();
    }
    let escaped = take_escaping_throwable(&clone_result, context, &mut values);
    let escaped =
        finish_bridge_ownership(escaped, previous_throw, object_cell, context, &mut values)?;
    if let Some(thrown) = escaped {
        // The operation already released its own unfinished clone before unwinding, so the
        // Throwable box is the only owner handed back.
        unsafe { *throwable_out = thrown.as_ptr(); }
        return Ok(2);
    }
    let clone = clone_result?;
    unsafe { *clone_out = clone.as_ptr(); }
    Ok(1)
}

/// Reads the caller's lexical AOT class name, treating an empty name as global scope.
///
/// # Safety
/// `scope_ptr` must be readable for `scope_len` bytes when `scope_len > 0`.
#[cfg(not(test))]
unsafe fn aot_invocation_scope(scope_ptr: *const u8, scope_len: u64) -> Option<String> {
    if scope_ptr.is_null() || scope_len == 0 {
        return None;
    }
    let scope_len = usize::try_from(scope_len).ok()?;
    let bytes = unsafe { std::slice::from_raw_parts(scope_ptr, scope_len) };
    String::from_utf8(bytes.to_vec()).ok().filter(|scope| !scope.is_empty())
}
