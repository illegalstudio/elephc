//! Purpose:
//! Bridges EvalIR value operations to elephc runtime values.
//! The module wires the stateless adapter type while focused submodules own
//! generated C-ABI symbols, trait operations, and opcode mappings.
//!
//! Called from:
//! - `crate::ffi::execute::__elephc_eval_execute()` in non-test builds.
//!
//! Key details:
//! - The wrapper symbols adapt to elephc's target-specific internal helper ABI.
//! - Unit tests do not link the generated runtime object, so real hooks compile
//!   only outside `cfg(test)`.

#[cfg(not(test))]
mod externs;
#[cfg(not(test))]
mod ops;
#[cfg(not(test))]
mod object_owners;
#[cfg(not(test))]
pub(crate) use object_owners::reset_object_owners;
#[cfg(not(test))]
mod tags;
#[cfg(not(test))]
pub(crate) mod release;

/// Publishes the interpreter's current file and line as the call site of the compiled function
/// it is about to call.
///
/// A compiled callee records the caller's file and line in its activation at entry, from the
/// pair a compiled call site publishes; the interpreter published nothing, so a compiled frame
/// called from interpreted code reported whatever compiled call ran last (php: the eval'd-code
/// or included-file line of the call). The descriptor (`{ptr, len}`) outlives every frame that
/// may point at it: one is interned, and leaked, per distinct file.
#[cfg(not(test))]
pub(crate) fn publish_native_call_site(context: &ElephcEvalContext) {
    if let Some((descriptor, line)) = current_site_descriptor(context) {
        unsafe { externs::__elephc_eval_publish_native_call_site(descriptor, line) };
    }
}

/// Publishes the interpreter's current file and line as the location the next runtime warning
/// names. A compiled raise site publishes its own before each instruction; the interpreter
/// published nothing, so a warning raised in eval'd code named the compiled caller's line.
#[cfg(not(test))]
pub(crate) fn publish_diagnostic_site(context: &ElephcEvalContext) {
    if let Some((descriptor, line)) = current_site_descriptor(context) {
        unsafe { externs::__elephc_eval_publish_diagnostic_site(descriptor, line) };
    }
}

/// Returns the interned `{ptr, len}` descriptor of the interpreter's current file, and its line.
///
/// The descriptor outlives every frame and diagnostic that may point at it: one is interned,
/// and leaked, per distinct file.
#[cfg(not(test))]
fn current_site_descriptor(context: &ElephcEvalContext) -> Option<(*const usize, i64)> {
    use std::collections::HashMap;
    use std::sync::{Mutex, OnceLock};
    static DESCRIPTORS: OnceLock<Mutex<HashMap<String, usize>>> = OnceLock::new();
    let (file, _, line, file_magic) = context.call_site();
    let file = file_magic.unwrap_or(file);
    if file.is_empty() {
        return None;
    }
    let descriptor = {
        let mut table = DESCRIPTORS
            .get_or_init(|| Mutex::new(HashMap::new()))
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        *table.entry(file).or_insert_with_key(|file| {
            let bytes: &'static [u8] = Box::leak(file.clone().into_bytes().into_boxed_slice());
            let words: &'static [usize; 2] = Box::leak(Box::new([bytes.as_ptr() as usize, bytes.len()]));
            words.as_ptr() as usize
        })
    };
    Some((descriptor as *const usize, line))
}

/// Unit tests link no generated runtime, so there is no call site to publish.
#[cfg(test)]
pub(crate) fn publish_native_call_site(_context: &crate::abi::ElephcEvalContext) {}

#[cfg(not(test))]
use crate::errors::EvalStatus;
#[cfg(not(test))]
use crate::abi::ElephcEvalContext;
#[cfg(not(test))]
use crate::value::{RuntimeCell, RuntimeCellHandle};
#[cfg(not(test))]
use externs::{
    __elephc_eval_install_dynamic_object_destructor_hook,
    __elephc_eval_install_class_autoload_hook,
    __elephc_eval_install_generator_protocol_hook,
    __elephc_eval_install_array_access_hook, __elephc_eval_install_object_clone_hook,
    __elephc_eval_install_object_relation_hook, __elephc_eval_install_serialize_object_hook,
    __elephc_eval_install_throwable_trace_hook, __elephc_eval_install_unserialize_object_hook,
    __elephc_eval_value_array_new,
    __elephc_eval_install_closure_bind_hook, __elephc_eval_install_dynamic_object_clone_hook,
    __elephc_eval_value_array_set, __elephc_eval_value_int, __elephc_eval_value_object_from_raw,
};

/// Runtime hook adapter that produces and consumes boxed elephc Mixed cells.
#[cfg(not(test))]
pub struct ElephcRuntimeOps {
    context: *const ElephcEvalContext,
}

#[cfg(not(test))]
impl ElephcRuntimeOps {
    /// Creates a runtime hook adapter without caller-sensitive eval context.
    pub const fn new() -> Self {
        Self {
            context: std::ptr::null(),
        }
    }

    /// Creates a runtime hook adapter that can expose the active class scope to generated helpers.
    pub const fn with_context(context: *const ElephcEvalContext) -> Self {
        Self { context }
    }

    /// Converts a runtime wrapper result into an interpreter handle.
    pub(crate) fn handle(ptr: *mut RuntimeCell) -> Result<RuntimeCellHandle, EvalStatus> {
        if ptr.is_null() {
            Err(EvalStatus::RuntimeFatal)
        } else {
            Ok(RuntimeCellHandle::from_raw(ptr))
        }
    }

    /// Boxes one borrowed raw object payload for eval `$this` dispatch.
    pub(crate) fn object_from_raw(
        object: *mut RuntimeCell,
    ) -> Result<RuntimeCellHandle, EvalStatus> {
        Self::handle(unsafe { __elephc_eval_value_object_from_raw(object) })
    }

    /// Packs borrowed arguments, releasing temporary keys and unfinished arrays on failure.
    fn arg_array(&mut self, args: Vec<RuntimeCellHandle>) -> Result<RuntimeCellHandle, EvalStatus> {
        let arg_array = unsafe { __elephc_eval_value_array_new(args.len() as u64) };
        let mut arg_array = Self::handle(arg_array)?;
        let populated = (|| {
            for (index, value) in args.into_iter().enumerate() {
                let index = i64::try_from(index).map_err(|_| EvalStatus::RuntimeFatal)?;
                let key = Self::handle(unsafe { __elephc_eval_value_int(index) })?;
                let inserted = Self::handle(unsafe {
                    __elephc_eval_value_array_set(arg_array.as_ptr(), key.as_ptr(), value.as_ptr())
                });
                self.release_cells([key])?;
                arg_array = inserted?;
            }
            Ok(())
        })();
        if let Err(status) = populated {
            self.release_cells([arg_array])?;
            return Err(status);
        }
        Ok(arg_array)
    }

    /// Returns the active eval class-scope bytes in the generated helper ABI shape.
    fn current_class_scope_abi(&self) -> (*const u8, u64) {
        let Some(context) = (unsafe { self.context.as_ref() }) else {
            return (std::ptr::null(), 0);
        };
        let Some(class_scope) = context.current_class_scope() else {
            return (std::ptr::null(), 0);
        };
        (class_scope.as_ptr(), class_scope.len() as u64)
    }
}

/// Installs eval destruction, object-edge, and array-reference retirement callbacks, plus the
/// owner filter generated code reads before calling into the registry.
#[cfg(not(test))]
pub(crate) unsafe fn install_dynamic_object_destructor_hook(callback: usize, owner_filter: usize) {
    unsafe {
        __elephc_eval_install_dynamic_object_destructor_hook(callback, owner_filter);
        externs::__elephc_eval_install_object_owner_hooks(
            object_owners::object_gc_child as *const () as usize,
            object_owners::release_object_children as *const () as usize,
            crate::ffi::array_references::retire_array_reference_cell_callback as *const () as usize,
        );
    }
}

/// Installs the eval `Generator` protocol callback into runtime data.
///
/// # Safety
/// `callback` must be the address of a
/// `extern "C" fn(*mut RuntimeCell, u64, *mut RuntimeCell, *mut u64) -> u64` following the
/// generator-protocol ABI; the `__rt_gen_*` helpers call through it before touching the
/// receiver's fiber fields.
#[cfg(not(test))]
pub(crate) unsafe fn install_generator_protocol_hook(callback: usize) {
    unsafe {
        __elephc_eval_install_generator_protocol_hook(callback);
    }
}

/// Installs the eval class-autoload callback into runtime data.
///
/// # Safety
/// `callback` must be the address of an `extern "C" fn(*const u8, u64) -> u64` following the
/// class-autoload ABI; the unserialize decoder calls through it for an unknown class name.
#[cfg(not(test))]
pub(crate) unsafe fn install_class_autoload_hook(callback: usize) {
    unsafe {
        __elephc_eval_install_class_autoload_hook(callback);
    }
}

/// Installs the eval unserialize-object callback into runtime data.
///
/// # Safety
/// `callback` must be the address of an
/// `extern "C" fn(*const u8, u64, u64) -> *mut RuntimeCell` following the unserialize-object
/// ABI; the decoder calls through it for a class with no compiled layout.
#[cfg(not(test))]
pub(crate) unsafe fn install_unserialize_object_hook(callback: usize) {
    unsafe {
        __elephc_eval_install_unserialize_object_hook(callback);
    }
}

/// Installs the eval `ArrayAccess` read callback into the generated runtime.
///
/// # Safety
/// `callback` must be the address of an
/// `extern "C" fn(*mut c_void, u64, u64, *mut *mut RuntimeCell) -> u64` following the
/// array-access ABI; `__rt_mixed_array_get` calls through it for an object it cannot index.
#[cfg(not(test))]
pub(crate) unsafe fn install_array_access_hook(callback: usize) {
    unsafe {
        __elephc_eval_install_array_access_hook(callback);
    }
}

/// Installs the compiled-clone adoption callback into the generated runtime.
///
/// # Safety
/// `callback` must be the address of an `extern "C" fn(*mut c_void, *mut c_void)`; generated
/// code calls it after a compiled `clone` with the source and the clone.
#[cfg(not(test))]
pub(crate) unsafe fn install_object_clone_hook(callback: usize) {
    unsafe {
        __elephc_eval_install_object_clone_hook(callback);
    }
}

/// Installs the eval object class-relation callback into the generated runtime.
///
/// # Safety
/// `callback` must be the address of an
/// `extern "C" fn(*mut RuntimeCell, *const u8, u64, u64) -> u64` following the object-relation
/// ABI; `__elephc_eval_value_is_a` calls through it when its AOT metadata answers false.
#[cfg(not(test))]
pub(crate) unsafe fn install_object_relation_hook(callback: usize) {
    unsafe {
        __elephc_eval_install_object_relation_hook(callback);
    }
}

/// Installs the eval serialize-object callback into the generated runtime.
///
/// # Safety
/// `callback` must be the address of an `extern "C" fn(*mut c_void) -> *mut RuntimeCell`
/// following the serialize-object ABI; `__rt_serialize_object` calls through it before reading a
/// class id the interpreter's own objects do not carry.
#[cfg(not(test))]
pub(crate) unsafe fn install_serialize_object_hook(callback: usize) {
    unsafe {
        __elephc_eval_install_serialize_object_hook(callback);
    }
}

/// Installs the eval Throwable-trace callback into the generated runtime.
///
/// # Safety
/// `callback` must be the address of an `extern "C" fn() -> *mut RuntimeCell` following the
/// throwable-trace ABI; `__rt_throwable_capture_trace` calls through it when a compiled `new`
/// creates a Throwable.
#[cfg(not(test))]
pub(crate) unsafe fn install_throwable_trace_hook(callback: usize) {
    unsafe {
        __elephc_eval_install_throwable_trace_hook(callback);
    }
}

/// Installs the eval closure `$this` rebinding callback into the generated runtime.
///
/// # Safety
/// `callback` must be the address of a function with the `__elephc_eval_closure_bind_this` ABI.
#[cfg(not(test))]
pub(crate) unsafe fn install_closure_bind_hook(callback: usize) {
    unsafe {
        __elephc_eval_install_closure_bind_hook(callback);
    }
}

/// Installs the eval dynamic-object clone callback into the generated runtime.
///
/// # Safety
/// `callback` must be the address of a function with the
/// `__elephc_eval_dynamic_object_clone` ABI; the generated `clone` lowering calls
/// through it before its own shallow-clone adapter runs.
#[cfg(not(test))]
pub(crate) unsafe fn install_dynamic_object_clone_hook(callback: usize) {
    unsafe {
        __elephc_eval_install_dynamic_object_clone_hook(callback);
    }
}

/// Reports whether one natively built override entry still belongs to a live PHP reference set.
///
/// Magician's own array-element alias table only knows arrays eval itself built, so an override
/// array that generated code built is read here instead. Every input stays borrowed.
#[cfg(not(test))]
pub(crate) fn array_entry_is_shared_reference(
    array: RuntimeCellHandle,
    key: &str,
    value: Option<RuntimeCellHandle>,
) -> bool {
    if array.is_null() {
        return false;
    }
    let shared = unsafe {
        externs::__elephc_eval_array_entry_is_shared_reference(
            array.as_ptr(),
            key.as_ptr(),
            key.len() as u64,
            value.map_or(std::ptr::null_mut(), RuntimeCellHandle::as_ptr),
        )
    };
    shared != 0
}

/// Unit test builds do not link the generated runtime, so no native entry state can exist.
#[cfg(test)]
pub(crate) fn array_entry_is_shared_reference(
    _array: crate::value::RuntimeCellHandle,
    _key: &str,
    _value: Option<crate::value::RuntimeCellHandle>,
) -> bool {
    false
}

/// Installs the eval output-buffering handler callback into the generated runtime.
///
/// # Safety
/// `callback` accepts a writable `OutputHandlerCallV1` pointer and returns a status.
/// The runtime calls it on buffer flushes and consumes returned owners after Rust exits.
/// `release` accepts a registry id and boxed-Throwable output pointer and returns a status.
#[cfg(not(test))]
pub(crate) unsafe fn install_ob_handler_hook(callback: usize, release: usize) {
    unsafe {
        externs::install_ob_handler_hook_raw(callback, release);
    }
}

/// Reports whether a compiled class has been LOADED, in php's `class_exists($n, false)` sense.
///
/// A closed-world build declares everything it compiled from the first instruction, but a class
/// the compiler pulled in ONLY so an existence probe could be answered is one php would never
/// have loaded. Those classes carry a request-scoped flag; every other compiled class -- and
/// every name the generated table does not list -- is loaded outright.
#[cfg(not(test))]
pub(crate) fn compiled_class_is_loaded(name: &str) -> bool {
    match deferred_class_flag(name) {
        Some(cell) => unsafe { cell.read() != 0 },
        None => true,
    }
}

/// Records that a probe which ALLOWS autoloading has now loaded this class.
#[cfg(not(test))]
pub(crate) fn mark_compiled_class_loaded(name: &str) {
    if let Some(cell) = deferred_class_flag(name) {
        unsafe { cell.write(1) };
    }
}

/// Returns the generated load flag for one class name, folded to php's case-insensitive form.
#[cfg(not(test))]
fn deferred_class_flag(name: &str) -> Option<*mut u64> {
    let folded = name.trim_start_matches('\\').to_ascii_lowercase();
    let cell = unsafe {
        externs::__elephc_eval_class_deferred_lookup(folded.as_ptr(), folded.len() as u64)
    };
    (!cell.is_null()).then_some(cell)
}

/// Performs what including a compiler-included source does to class-like load state.
///
/// Called when an include is skipped on the compiler's mark: php would declare the file's symbols
/// right there, and the compiled program only does so where it first names one of them.
#[cfg(not(test))]
pub(crate) fn activate_compiled_source(path: &std::path::Path) {
    let bytes = path.as_os_str().as_encoded_bytes();
    unsafe { externs::__elephc_eval_source_activate(bytes.as_ptr(), bytes.len() as u64) };
}

#[cfg(test)]
pub(crate) fn activate_compiled_source(_path: &std::path::Path) {}

/// Tells the descriptor invoker about to run that the interpreter is its caller.
///
/// Only then does it catch a native throw and hand it back as a pending status; a compiled
/// caller of the same descriptor needs the throw to keep unwinding to its own `catch`.
#[cfg(not(test))]
pub(crate) fn request_invoker_boundary() {
    unsafe { externs::__elephc_eval_request_invoker_boundary() };
}

#[cfg(test)]
pub(crate) fn request_invoker_boundary() {}

/// Mirrors an interpreted `@` scope into the runtime's suppression depth.
///
/// The interpreter's own depth only silences diagnostics it renders itself; a warning raised by
/// a builtin's runtime helper, a compiled callee or the regex shim goes through the runtime's
/// counter, which php's `@` covers just the same.
#[cfg(not(test))]
pub(crate) fn diag_suppression(enter: bool) {
    unsafe { externs::__elephc_eval_diag_suppression(u64::from(enter)) };
}

#[cfg(test)]
pub(crate) fn diag_suppression(_enter: bool) {}

/// Returns the class a COMPILED `stream_wrapper_register()` bound to `scheme`, if any.
///
/// Compiled code registers wrappers in the generated runtime's own table, which the interpreter's
/// registry never sees; an interpreted `include 'scheme://...'` has to look there too, or a
/// wrapper the compiled program installed is "No such file or directory" to it.
#[cfg(not(test))]
pub(crate) fn compiled_stream_wrapper_class(scheme: &str) -> Option<String> {
    const REGISTRATIONS: usize = 64;
    let table = unsafe { externs::__elephc_eval_user_wrapper_table() };
    if table.is_null() {
        return None;
    }
    for slot in 0..REGISTRATIONS {
        let entry = unsafe { std::slice::from_raw_parts(table.add(slot * 4), 4) };
        let (protocol_ptr, protocol_len, class_ptr, class_len) =
            (entry[0], entry[1], entry[2], entry[3]);
        if protocol_ptr == 0 || class_ptr == 0 {
            continue;
        }
        let protocol = unsafe {
            std::slice::from_raw_parts(protocol_ptr as *const u8, protocol_len as usize)
        };
        if !protocol.eq_ignore_ascii_case(scheme.as_bytes()) {
            continue;
        }
        let class = unsafe { std::slice::from_raw_parts(class_ptr as *const u8, class_len as usize) };
        return Some(
            String::from_utf8_lossy(class)
                .trim_start_matches('\\')
                .to_string(),
        );
    }
    None
}

#[cfg(test)]
pub(crate) fn compiled_stream_wrapper_class(_scheme: &str) -> Option<String> {
    None
}

/// Test builds link no generated table, so every compiled class counts as loaded.
#[cfg(test)]
pub(crate) fn compiled_class_is_loaded(_name: &str) -> bool {
    true
}

#[cfg(test)]
pub(crate) fn mark_compiled_class_loaded(_name: &str) {}

/// Drives one COMPILED generator through the generated runtime's own helpers.
///
/// The interpreter cannot reach a compiled `Generator` through ordinary method dispatch: its
/// methods are runtime helpers, not registered native methods, so `valid()` on one fails. These
/// wrappers are the supported route, and they are what `yield from` uses when its delegate is a
/// generator the interpreter does not own.
///
/// Under `cfg(test)` there is no generated runtime to call, so each one answers the shape an
/// exhausted generator has. A unit test that reaches these has no compiled generator to drive.
#[cfg(not(test))]
pub(crate) fn native_generator_valid(generator: u64) -> bool {
    unsafe { externs::__elephc_eval_gen_valid(generator as *mut RuntimeCell) != 0 }
}

/// Returns an owned copy of a compiled generator's current value.
#[cfg(not(test))]
pub(crate) fn native_generator_current(generator: u64) -> Option<RuntimeCellHandle> {
    let value = unsafe { externs::__elephc_eval_gen_current(generator as *mut RuntimeCell) };
    (!value.is_null()).then(|| RuntimeCellHandle::from_raw(value))
}

/// Returns an owned copy of a compiled generator's current key.
#[cfg(not(test))]
pub(crate) fn native_generator_key(generator: u64) -> Option<RuntimeCellHandle> {
    let value = unsafe { externs::__elephc_eval_gen_key(generator as *mut RuntimeCell) };
    (!value.is_null()).then(|| RuntimeCellHandle::from_raw(value))
}

/// Advances a compiled generator to its next yield.
#[cfg(not(test))]
pub(crate) fn native_generator_next(generator: u64) {
    unsafe { externs::__elephc_eval_gen_next(generator as *mut RuntimeCell) };
}

/// Returns an owned copy of a compiled generator's `getReturn()` value.
#[cfg(not(test))]
pub(crate) fn native_generator_return(generator: u64) -> Option<RuntimeCellHandle> {
    let value = unsafe { externs::__elephc_eval_gen_get_return(generator as *mut RuntimeCell) };
    (!value.is_null()).then(|| RuntimeCellHandle::from_raw(value))
}

#[cfg(test)]
use crate::value::RuntimeCellHandle;

#[cfg(test)]
pub(crate) fn native_generator_valid(_generator: u64) -> bool {
    false
}

#[cfg(test)]
pub(crate) fn native_generator_current(_generator: u64) -> Option<RuntimeCellHandle> {
    None
}

#[cfg(test)]
pub(crate) fn native_generator_key(_generator: u64) -> Option<RuntimeCellHandle> {
    None
}

#[cfg(test)]
pub(crate) fn native_generator_next(_generator: u64) {}

#[cfg(test)]
pub(crate) fn native_generator_return(_generator: u64) -> Option<RuntimeCellHandle> {
    None
}
