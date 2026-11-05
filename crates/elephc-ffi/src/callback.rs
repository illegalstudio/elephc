//! Purpose:
//! libffi closures that let a C library call back into a PHP closure registered through
//! `FFI` (function-pointer globals, struct fields, and array elements), plus the stable
//! call-handle ABI the PHP prelude uses to read arguments and report a throw.
//!
//! Called from:
//! - `crate::abi` entry points for closure assignment and dispatcher registration.
//! - The PHP FFI prelude's `extern "elephc_ffi"` declarations.
//!
//! Key details:
//! - A closure's `Cif` and `ffi_closure` live in a process-wide registry; registrations
//!   are never freed because C may retain the code pointer past a PHP assignment.
//! - The PHP side receives `(id, call_handle)`, reads each argument through
//!   `elephc_ffi_callback_arg_*`, and returns the value through its dispatcher signature
//!   (`int`, `float`, or `void`).
//! - Arguments are decoded eagerly: scalars, `const char *` as PHP strings (matching
//!   php-src), and pointer/struct values as CData handles.
//! - A throw from the PHP callback is handled by the prelude (it prints php-src's
//!   "Throwing from FFI callbacks is not allowed" fatal); it must never unwind through
//!   the Rust closure frame.

use std::collections::HashMap;
use std::ffi::CString;
use std::os::raw::{c_char, c_void};
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Mutex, OnceLock};

use libffi::middle::Cif;

use crate::context::{cdata, pointer_to, set_error, type_clone, CData, ERROR_EXCEPTION};
use crate::place::{self, Place};
use crate::types::{TypeId, TypeKind};

/// The C `zend_write` hook the runtime's stdout writer calls before its syscall.
///
/// Zero means "use the built-in syscall path". `FFI::cdef()` exposes this word as the
/// `zend_write` global so engine-hook fixtures can replace it.
#[no_mangle]
pub static elephc_zend_write_current: AtomicUsize = AtomicUsize::new(0);

/// PHP dispatcher for callbacks whose C return type is an integer or pointer.
static DISPATCHER_INT: AtomicUsize = AtomicUsize::new(0);
/// PHP dispatcher for callbacks whose C return type is a float.
static DISPATCHER_FLOAT: AtomicUsize = AtomicUsize::new(0);
/// PHP dispatcher for callbacks whose C return type is void.
static DISPATCHER_VOID: AtomicUsize = AtomicUsize::new(0);
/// PHP dispatcher for callbacks whose C return type is a struct/array by value.
static DISPATCHER_AGGREGATE: AtomicUsize = AtomicUsize::new(0);

/// Monotonic id assigned to each installed closure.
static NEXT_CALLBACK: AtomicUsize = AtomicUsize::new(1);

/// Process-wide closure registry, keyed by the id handed to libffi as userdata.
fn callbacks() -> &'static Mutex<HashMap<usize, Callback>> {
    static CALLBACKS: OnceLock<Mutex<HashMap<usize, Callback>>> = OnceLock::new();
    CALLBACKS.get_or_init(|| Mutex::new(HashMap::new()))
}

thread_local! {
    /// Copy of the last aggregate returned by a PHP callback.
    ///
    /// The PHP dispatcher returns a handle whose CData is released when the dispatcher
    /// function returns. The struct must therefore be copied before that release; the
    /// trampoline consumes this staged buffer immediately after the dispatcher returns.
    static STAGED_AGGREGATE: std::cell::RefCell<Option<Vec<u8>>> =
        const { std::cell::RefCell::new(None) };
}

/// One installed libffi closure.
struct Callback {
    /// The allocation libffi owns; kept alive for the process lifetime.
    _closure: *mut libffi::raw::ffi_closure,
    /// The callable code pointer written into the C location.
    code: *mut c_void,
    /// The CIF backing the closure; libffi retains a pointer to it.
    _cif: Box<Cif>,
    /// Parameter types in declaration order.
    params: Vec<TypeId>,
    /// Declared return type.
    ret: TypeId,
    /// Which PHP dispatcher the closure must call.
    return_kind: ReturnKind,
    /// User-visible function/global/field name for the callback fatal.
    name: CString,
}

// The closure registry is guarded by a Mutex and the raw pointers address immutable
// libffi allocations that are never freed.
unsafe impl Send for Callback {}
unsafe impl Sync for Callback {}

/// The dispatcher family selected by the declared C return type.
#[derive(Clone, Copy)]
enum ReturnKind {
    /// `void`.
    Void,
    /// An integer, enum, character, boolean, or pointer return.
    Int,
    /// A `float`/`double` return.
    Float,
    /// A struct/array returned by value.
    Aggregate,
}

impl ReturnKind {
    /// Classifies one declared return type, or `None` when closures cannot return it.
    fn for_type(ty: TypeId) -> Option<Self> {
        let info = type_clone(ty)?;
        match &info.kind {
            TypeKind::Void => Some(Self::Void),
            TypeKind::Float { .. } => Some(Self::Float),
            TypeKind::Bool
            | TypeKind::Char { .. }
            | TypeKind::Int { .. }
            | TypeKind::Enum { .. }
            | TypeKind::Pointer(_)
            | TypeKind::Func(_) => Some(Self::Int),
            TypeKind::Array { .. } | TypeKind::Struct(_) => Some(Self::Aggregate),
            TypeKind::IncompleteTag { .. } => None,
        }
    }
}

/// One decoded C callback argument.
enum CallbackArg {
    /// A C integer or enum.
    Int(i64),
    /// A C float/double.
    Float(f64),
    /// A C bool.
    Bool(bool),
    /// A C char, carried as its byte value.
    Char(i64),
    /// A `const char *`, copied to a stable NUL-terminated buffer.
    Str(CString),
    /// A pointer or aggregate, wrapped as a CData handle.
    CData(usize),
    /// A null pointer.
    Null,
}

/// One in-flight C callback, addressed by the PHP dispatcher through an opaque handle.
struct CallbackCall {
    /// The global/field name used by php-src's callback fatal.
    name: CString,
    /// Decoded arguments.
    args: Vec<CallbackArg>,
}

/// PHP dispatcher with an integer/pointer return.
type IntDispatcher = unsafe extern "C" fn(u64, usize) -> i64;
/// PHP dispatcher with a float return.
type FloatDispatcher = unsafe extern "C" fn(u64, usize) -> f64;
/// PHP dispatcher with a void return.
type VoidDispatcher = unsafe extern "C" fn(u64, usize);
/// PHP dispatcher returning an aggregate CData handle.
type AggregateDispatcher = unsafe extern "C" fn(u64, usize) -> usize;

/// Registers the PHP dispatcher used for integer/pointer-returning callbacks.
#[no_mangle]
pub extern "C" fn elephc_ffi_set_dispatcher_int(dispatcher: usize) {
    DISPATCHER_INT.store(dispatcher, Ordering::SeqCst);
}

/// Registers the PHP dispatcher used for float-returning callbacks.
#[no_mangle]
pub extern "C" fn elephc_ffi_set_dispatcher_float(dispatcher: usize) {
    DISPATCHER_FLOAT.store(dispatcher, Ordering::SeqCst);
}

/// Registers the PHP dispatcher used for void-returning callbacks.
#[no_mangle]
pub extern "C" fn elephc_ffi_set_dispatcher_void(dispatcher: usize) {
    DISPATCHER_VOID.store(dispatcher, Ordering::SeqCst);
}

/// Registers the PHP dispatcher used for aggregate-returning callbacks.
#[no_mangle]
pub extern "C" fn elephc_ffi_set_dispatcher_aggregate(dispatcher: usize) {
    DISPATCHER_AGGREGATE.store(dispatcher, Ordering::SeqCst);
}

/// Copies one aggregate CData into the thread-local stage and returns its address.
#[no_mangle]
pub extern "C" fn elephc_ffi_stage_aggregate(value: usize) -> usize {
    crate::abi::guard(0, || {
        let Some(data) = cdata(value) else { return 0 };
        let Some(ty) = type_clone(data.ty) else { return 0 };
        let mut bytes = vec![0u8; ty.size];
        if data.addr != 0 {
            unsafe {
                std::ptr::copy_nonoverlapping(
                    data.addr as *const u8,
                    bytes.as_mut_ptr(),
                    ty.size,
                );
            }
        }
        let pointer = bytes.as_ptr() as usize;
        STAGED_AGGREGATE.with(|slot| {
            *slot.borrow_mut() = Some(bytes);
        });
        pointer
    })
}

/// Releases the thread-local aggregate stage.
#[no_mangle]
pub extern "C" fn elephc_ffi_clear_staged_aggregate() {
    STAGED_AGGREGATE.with(|slot| {
        *slot.borrow_mut() = None;
    });
}

/// Installs a libffi closure into a C global function pointer.
///
/// Returns the PHP-side callback id, or 0 when the target is not an installable function
/// pointer (the prelude then keeps its address-registry fallback).
#[no_mangle]
pub extern "C" fn elephc_ffi_global_write_closure(
    context_handle: usize,
    name: *const c_char,
) -> usize {
    crate::abi::guard(0, || {
        let name = unsafe { crate::abi::text(name) };
        let Ok(place) = crate::abi::global_place(context_handle, name) else {
            return 0;
        };
        install(place, name)
    })
}

/// Installs a libffi closure into a scalar function-pointer CData's own storage.
#[no_mangle]
pub extern "C" fn elephc_ffi_cdata_write_closure(value: usize) -> usize {
    crate::abi::guard(0, || {
        let Some(data) = cdata(value) else {
            set_error(ERROR_EXCEPTION, "Invalid FFI\\CData");
            return 0;
        };
        let place = Place {
            addr: data.addr,
            ty: data.ty,
            bit_width: None,
            bit_offset: 0,
            readonly: data.readonly,
            label: None,
        };
        install(place, "callback")
    })
}

/// Installs a libffi closure into a struct/union field function pointer.
#[no_mangle]
pub extern "C" fn elephc_ffi_field_write_closure(value: usize, name: *const c_char) -> usize {
    crate::abi::guard(0, || {
        let name = unsafe { crate::abi::text(name) };
        let Ok(place) = place::field_place(value, name) else {
            return 0;
        };
        install(place, name)
    })
}

/// Installs a libffi closure into an array/pointer element function pointer.
#[no_mangle]
pub extern "C" fn elephc_ffi_offset_write_closure(value: usize, index: i64) -> usize {
    crate::abi::guard(0, || {
        let Ok(place) = place::offset_place(value, index) else {
            return 0;
        };
        install(place, "callback")
    })
}

/// Returns the pointer value stored by one CData handle (the address itself for
/// aggregates), used when a PHP closure returns a pointer.
#[no_mangle]
pub extern "C" fn elephc_ffi_cdata_pointer(value: usize) -> usize {
    crate::abi::guard(0, || {
        let Some(data) = cdata(value) else { return 0 };
        let Some(ty) = type_clone(data.ty) else { return 0 };
        if matches!(ty.kind, TypeKind::Pointer(_)) {
            data.pointer_value()
        } else {
            data.addr
        }
    })
}

/// Terminates the process after the prelude printed php-src's callback fatal.
#[no_mangle]
pub extern "C" fn elephc_ffi_callback_abort() -> ! {
    std::process::exit(255)
}

/// Returns the number of arguments in one in-flight callback.
#[no_mangle]
pub extern "C" fn elephc_ffi_callback_arg_count(call_handle: usize) -> usize {
    crate::abi::guard(0, || {
        call(call_handle).map_or(0, |call| call.args.len())
    })
}

/// Returns one argument's PHP-facing kind.
#[no_mangle]
pub extern "C" fn elephc_ffi_callback_arg_kind(call_handle: usize, index: usize) -> u32 {
    crate::abi::guard(0, || {
        let Some(arg) = call(call_handle).and_then(|call| call.args.get(index)) else {
            return 0;
        };
        match arg {
            CallbackArg::Int(_) => 1,
            CallbackArg::Float(_) => 2,
            CallbackArg::Bool(_) => 3,
            CallbackArg::Char(_) => 4,
            CallbackArg::Str(_) => 5,
            CallbackArg::CData(_) => 6,
            CallbackArg::Null => 7,
        }
    })
}

/// Reads one integer/character/bool argument as an integer.
#[no_mangle]
pub extern "C" fn elephc_ffi_callback_arg_int(call_handle: usize, index: usize) -> i64 {
    crate::abi::guard(0, || {
        match call(call_handle).and_then(|call| call.args.get(index)) {
            Some(CallbackArg::Int(value)) | Some(CallbackArg::Char(value)) => *value,
            Some(CallbackArg::Bool(value)) => i64::from(*value),
            _ => 0,
        }
    })
}

/// Reads one float argument.
#[no_mangle]
pub extern "C" fn elephc_ffi_callback_arg_float(call_handle: usize, index: usize) -> f64 {
    crate::abi::guard(0.0, || {
        match call(call_handle).and_then(|call| call.args.get(index)) {
            Some(CallbackArg::Float(value)) => *value,
            _ => 0.0,
        }
    })
}

/// Returns a string argument's bytes, or a NUL pointer when the argument is not a string.
#[no_mangle]
pub extern "C" fn elephc_ffi_callback_arg_string(call_handle: usize, index: usize) -> *const c_char {
    crate::abi::guard(std::ptr::null(), || {
        match call(call_handle).and_then(|call| call.args.get(index)) {
            Some(CallbackArg::Str(value)) => value.as_ptr(),
            _ => std::ptr::null(),
        }
    })
}

/// Returns a string argument's byte length.
#[no_mangle]
pub extern "C" fn elephc_ffi_callback_arg_string_len(
    call_handle: usize,
    index: usize,
) -> usize {
    crate::abi::guard(0, || {
        match call(call_handle).and_then(|call| call.args.get(index)) {
            Some(CallbackArg::Str(value)) => value.as_bytes().len(),
            _ => 0,
        }
    })
}

/// Returns the CData handle for one pointer/aggregate argument.
#[no_mangle]
pub extern "C" fn elephc_ffi_callback_arg_cdata(call_handle: usize, index: usize) -> usize {
    crate::abi::guard(0, || {
        match call(call_handle).and_then(|call| call.args.get(index)) {
            Some(CallbackArg::CData(value)) => *value,
            _ => 0,
        }
    })
}

/// Returns the innermost C function name currently executing, for the callback trace frame.
///
/// Empty (`len == 0`) when the callback was reached from an ordinary engine hook rather
/// than from an FFI call already on the stack.
#[no_mangle]
pub extern "C" fn elephc_ffi_callback_active_name(_call_handle: usize) -> *const c_char {
    crate::abi::guard(std::ptr::null(), || {
        crate::call::active_call_name_ptr_len().0 as *const c_char
    })
}

/// Returns the active C function name's byte length.
#[no_mangle]
pub extern "C" fn elephc_ffi_callback_active_name_len(_call_handle: usize) -> usize {
    crate::abi::guard(0, || crate::call::active_call_name_ptr_len().1)
}

/// Returns the callback's function/global name for the prelude's fatal report.
#[no_mangle]
pub extern "C" fn elephc_ffi_callback_name(call_handle: usize) -> *const c_char {
    crate::abi::guard(std::ptr::null(), || {
        call(call_handle)
            .map(|call| call.name.as_ptr())
            .unwrap_or(std::ptr::null())
    })
}

/// Returns the callback name's byte length.
#[no_mangle]
pub extern "C" fn elephc_ffi_callback_name_len(call_handle: usize) -> usize {
    crate::abi::guard(0, || {
        call(call_handle)
            .map(|call| call.name.as_bytes().len())
            .unwrap_or(0)
    })
}

/// Borrows one in-flight callback.
fn call(call_handle: usize) -> Option<&'static CallbackCall> {
    if call_handle == 0 {
        return None;
    }
    Some(unsafe { &*(call_handle as *const CallbackCall) })
}

/// One validated closure, registered but not yet published to the call registry.
struct PreparedCallback {
    /// Callback id handed to libffi as userdata.
    id: usize,
    /// Callable entry point libffi filled in.
    code: usize,
    /// Published registry entry.
    callback: Callback,
}

/// Validates a declared pointer-to-function type and prepares a libffi closure for it.
fn prepare(ty: TypeId, name: &str) -> Option<PreparedCallback> {
    let Some(target) = type_clone(ty) else {
        set_error(ERROR_EXCEPTION, "Attempt to assign an invalid callback");
        return None;
    };
    let TypeKind::Pointer(inner) = target.kind else {
        set_error(ERROR_EXCEPTION, "Attempt to assign an invalid callback");
        return None;
    };
    let Some(function) = type_clone(inner).and_then(|ty| match ty.kind {
        TypeKind::Func(function) => Some(function),
        _ => None,
    }) else {
        set_error(ERROR_EXCEPTION, "Attempt to assign an invalid callback");
        return None;
    };
    if function.variadic {
        set_error(ERROR_EXCEPTION, "Variadic function closures are not supported");
        return None;
    }
    let Some(return_kind) = ReturnKind::for_type(function.ret) else {
        set_error(ERROR_EXCEPTION, "Cannot prepare callback CIF");
        return None;
    };
    let mut arg_types = Vec::with_capacity(function.params.len());
    for param in &function.params {
        let Some(ty) = crate::call::declared_ffi_type(*param) else {
            set_error(ERROR_EXCEPTION, "Cannot prepare callback CIF");
            return None;
        };
        arg_types.push(ty);
    }
    let Some(result_type) = crate::call::declared_ffi_type(function.ret) else {
        set_error(ERROR_EXCEPTION, "Cannot prepare callback CIF");
        return None;
    };
    let cif = Box::new(Cif::new(arg_types.into_iter(), result_type));
    let Some((closure, code)) = libffi::low::try_closure_alloc() else {
        set_error(ERROR_EXCEPTION, "Cannot allocate callback");
        return None;
    };
    let id = NEXT_CALLBACK.fetch_add(1, Ordering::Relaxed);
    let name_c = CString::new(name).unwrap_or_else(|_| CString::new("callback").unwrap());
    let status = unsafe {
        libffi::raw::ffi_prep_closure_loc(
            closure,
            cif.as_raw_ptr(),
            Some(trampoline),
            id as *mut c_void,
            code.as_mut_ptr(),
        )
    };
    if status != libffi::raw::ffi_status_FFI_OK {
        unsafe { libffi::raw::ffi_closure_free(closure.cast()) };
        set_error(ERROR_EXCEPTION, "Cannot prepare callback");
        return None;
    }
    let code_pointer = code.as_ptr() as *mut c_void as usize;
    Some(PreparedCallback {
        id,
        code: code_pointer,
        callback: Callback {
            _closure: closure,
            code: code.as_ptr() as *mut c_void,
            _cif: cif,
            params: function.params,
            ret: function.ret,
            return_kind,
            name: name_c,
        },
    })
}

/// Publishes a prepared closure so the dispatcher can resolve it by id.
fn publish(prepared: PreparedCallback) -> usize {
    let id = prepared.id;
    callbacks()
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner())
        .insert(id, prepared.callback);
    id
}

/// Installs a closure for one resolved function-pointer place, returning its id.
fn install(place: Place, name: &str) -> usize {
    let Some(prepared) = prepare(place.ty, name) else {
        return 0;
    };
    let code = prepared.code;
    if place::write_pointer(&place, Some(code)).is_err() {
        unsafe { libffi::raw::ffi_closure_free(prepared.callback._closure.cast()) };
        return 0;
    }
    publish(prepared)
}

/// Prepares a closure for a direct function-pointer call argument.
///
/// Returns the callback id and its code address. php-src keeps call-argument callbacks
/// alive until request shutdown, so the registry owns the libffi closure for the process.
pub(crate) fn prepare_argument(ty: TypeId, name: &str) -> Option<(usize, usize)> {
    let prepared = prepare(ty, name)?;
    let code = prepared.code;
    let id = publish(prepared);
    Some((id, code))
}

/// Returns the code address registered for one callback id.
#[no_mangle]
pub extern "C" fn elephc_ffi_callback_code(id: usize) -> usize {
    crate::abi::guard(0, || {
        callbacks()
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
            .get(&id)
            .map_or(0, |callback| callback.code as usize)
    })
}

/// The libffi closure entry point: decode the C arguments, invoke PHP, write the result.
unsafe extern "C" fn trampoline(
    _cif: *mut libffi::raw::ffi_cif,
    result: *mut c_void,
    args: *mut *mut c_void,
    userdata: *mut c_void,
) {
    let id = userdata as usize;
    let (params, ret, return_kind, name) = {
        let guard = callbacks()
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        let Some(callback) = guard.get(&id) else { return };
        (
            callback.params.clone(),
            callback.ret,
            callback.return_kind,
            callback.name.clone(),
        )
    };
    let mut decoded = Vec::with_capacity(params.len());
    for (index, ty) in params.iter().enumerate() {
        decoded.push(decode_arg(*ty, *args.add(index)));
    }
    let call = Box::new(CallbackCall { name, args: decoded });
    let call_handle = Box::into_raw(call) as usize;
    match return_kind {
        ReturnKind::Void => {
            let dispatcher = DISPATCHER_VOID.load(Ordering::SeqCst);
            if dispatcher != 0 {
                let dispatcher: VoidDispatcher = std::mem::transmute(dispatcher);
                dispatcher(id as u64, call_handle);
            }
        }
        ReturnKind::Float => {
            let dispatcher = DISPATCHER_FLOAT.load(Ordering::SeqCst);
            let value = if dispatcher == 0 {
                0.0
            } else {
                let dispatcher: FloatDispatcher = std::mem::transmute(dispatcher);
                dispatcher(id as u64, call_handle)
            };
            write_float_return(result, ret, value);
        }
        ReturnKind::Int => {
            let dispatcher = DISPATCHER_INT.load(Ordering::SeqCst);
            let value = if dispatcher == 0 {
                0
            } else {
                let dispatcher: IntDispatcher = std::mem::transmute(dispatcher);
                dispatcher(id as u64, call_handle)
            };
            write_int_return(result, ret, value);
        }
        ReturnKind::Aggregate => {
            let dispatcher = DISPATCHER_AGGREGATE.load(Ordering::SeqCst);
            let handle = if dispatcher == 0 {
                0
            } else {
                let dispatcher: AggregateDispatcher = std::mem::transmute(dispatcher);
                dispatcher(id as u64, call_handle)
            };
            write_aggregate_return(result, ret, handle);
        }
    }
    drop(Box::from_raw(call_handle as *mut CallbackCall));
}

/// Decodes one raw libffi argument according to its declared C type.
unsafe fn decode_arg(ty: TypeId, storage: *mut c_void) -> CallbackArg {
    let Some(info) = type_clone(ty) else {
        return CallbackArg::Null;
    };
    match &info.kind {
        TypeKind::Void => CallbackArg::Null,
        TypeKind::Bool => CallbackArg::Bool(*(storage as *const u8) != 0),
        TypeKind::Char { .. } => CallbackArg::Char(i64::from(*(storage as *const u8))),
        TypeKind::Int { bits, signed } => CallbackArg::Int(read_int(storage, *bits, *signed)),
        TypeKind::Enum { underlying, .. } => decode_arg(*underlying, storage),
        TypeKind::Float { bits: 32 } => CallbackArg::Float(f64::from(*(storage as *const f32))),
        TypeKind::Float { .. } => CallbackArg::Float(*(storage as *const f64)),
        TypeKind::Pointer(inner) => {
            let pointer = *(storage as *const usize);
            if pointer == 0 {
                return CallbackArg::Null;
            }
            let inner_info = type_clone(*inner);
            if inner_info
                .as_ref()
                .is_some_and(|inner| matches!(inner.kind, TypeKind::Char { .. }) && inner.is_const)
            {
                let text = std::ffi::CStr::from_ptr(pointer as *const c_char);
                return CallbackArg::Str(text.to_owned());
            }
            match pointer_to(pointer, *inner) {
                Some(handle) => CallbackArg::CData(handle),
                None => CallbackArg::Null,
            }
        }
        TypeKind::Func(_) => {
            let pointer = *(storage as *const usize);
            if pointer == 0 {
                CallbackArg::Null
            } else {
                CallbackArg::CData(pointer_to(pointer, ty).unwrap_or(0))
            }
        }
        TypeKind::Array { .. } | TypeKind::Struct(_) | TypeKind::IncompleteTag { .. } => {
            // libffi only guarantees the argument storage for the duration of the callback.
            // PHP code may retain the CData past the C return, so the aggregate must own a copy
            // instead of viewing libffi's stack area.
            CallbackArg::CData(owned_aggregate(storage as usize, ty))
        }
    }
}

/// Copies one aggregate argument into owned CData storage.
fn owned_aggregate(address: usize, ty: TypeId) -> usize {
    let Some(info) = type_clone(ty) else { return 0 };
    let storage = unsafe { libc::calloc(1, info.size.max(1)) } as usize;
    if storage == 0 {
        set_error(ERROR_EXCEPTION, "Out of memory");
        return 0;
    }
    unsafe {
        std::ptr::copy_nonoverlapping(address as *const u8, storage as *mut u8, info.size);
    }
    let data = CData {
        addr: storage,
        ty,
        allocated: true,
        owned: true,
        freed: false,
        addr_temp: false,
        readonly: false,
        bit_width: None,
        bit_offset: 0,
        owns_pointee: false,
        persistent: false,
        source: 0,
    };
    Box::into_raw(Box::new(data)) as usize
}

/// Reads one integer of the declared width, sign-extending when required.
unsafe fn read_int(storage: *mut c_void, bits: u8, signed: bool) -> i64 {
    match (bits, signed) {
        (8, true) => i64::from(*(storage as *const i8)),
        (8, false) => i64::from(*(storage as *const u8)),
        (16, true) => i64::from(*(storage as *const i16)),
        (16, false) => i64::from(*(storage as *const u16)),
        (32, true) => i64::from(*(storage as *const i32)),
        (32, false) => i64::from(*(storage as *const u32)),
        (_, true) => *(storage as *const i64),
        (_, false) => *(storage as *const u64) as i64,
    }
}

/// Writes an integer/pointer return value with the declared width.
unsafe fn write_int_return(result: *mut c_void, ty: TypeId, value: i64) {
    let Some(info) = type_clone(ty) else { return };
    match &info.kind {
        TypeKind::Void => {}
        TypeKind::Bool => *(result as *mut u8) = u8::from(value != 0),
        TypeKind::Char { .. } => *(result as *mut u8) = value as u8,
        TypeKind::Int { bits, .. } => match bits {
            8 => *(result as *mut u8) = value as u8,
            16 => *(result as *mut u16) = value as u16,
            32 => *(result as *mut u32) = value as u32,
            _ => *(result as *mut u64) = value as u64,
        },
        TypeKind::Enum { underlying, .. } => write_int_return(result, *underlying, value),
        TypeKind::Pointer(_) | TypeKind::Func(_) => *(result as *mut usize) = value as usize,
        TypeKind::Float { bits: 32 } => *(result as *mut f32) = value as f32,
        TypeKind::Float { .. } => *(result as *mut f64) = value as f64,
        TypeKind::Array { .. } | TypeKind::Struct(_) | TypeKind::IncompleteTag { .. } => {}
    }
}

/// Copies a PHP CData result into the libffi aggregate return buffer.
fn write_aggregate_return(result: *mut c_void, ty: TypeId, staged: usize) {
    let Some(info) = type_clone(ty) else { return };
    if result.is_null() {
        STAGED_AGGREGATE.with(|slot| {
            *slot.borrow_mut() = None;
        });
        return;
    }
    if staged != 0 {
        unsafe {
            std::ptr::copy_nonoverlapping(
                staged as *const u8,
                result as *mut u8,
                info.size,
            );
        }
    } else {
        unsafe { std::ptr::write_bytes(result as *mut u8, 0, info.size) };
    }
    STAGED_AGGREGATE.with(|slot| {
        *slot.borrow_mut() = None;
    });
}

/// Writes a float return value with the declared width.
unsafe fn write_float_return(result: *mut c_void, ty: TypeId, value: f64) {
    let Some(info) = type_clone(ty) else { return };
    match &info.kind {
        TypeKind::Float { bits: 32 } => *(result as *mut f32) = value as f32,
        TypeKind::Float { .. } => *(result as *mut f64) = value,
        TypeKind::Enum { underlying, .. } => write_float_return(result, *underlying, value),
        _ => write_int_return(result, ty, value as i64),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::ffi::CString;

    /// Verifies aggregate callback arguments own a copy instead of viewing libffi's frame.
    #[test]
    fn aggregate_arguments_are_copied_into_owned_cdata() {
        let code = CString::new("struct rec { int32_t a; int32_t b; };").expect("cdef");
        let context = crate::abi::elephc_ffi_cdef(code.as_ptr(), std::ptr::null());
        assert_ne!(context, 0, "cdef must parse");
        let ty = crate::context::resolve_type(context, "struct rec").expect("struct type");
        let size = crate::context::type_clone(ty).expect("type").size;
        let mut storage = vec![0u8; size];
        unsafe {
            std::ptr::write_unaligned(storage.as_mut_ptr() as *mut i32, 11);
            std::ptr::write_unaligned(storage.as_mut_ptr().add(4) as *mut i32, 22);
        }
        let argument = unsafe { decode_arg(ty, storage.as_mut_ptr() as *mut c_void) };
        let CallbackArg::CData(handle) = argument else {
            panic!("aggregate argument must decode to CData");
        };
        // libffi may reuse the argument area as soon as the callback returns.
        storage.fill(0);
        let data = cdata(handle).expect("owned CData");
        assert_ne!(data.addr, storage.as_ptr() as usize);
        let first = unsafe { std::ptr::read_unaligned(data.addr as *const i32) };
        let second = unsafe { std::ptr::read_unaligned((data.addr + 4) as *const i32) };
        assert_eq!((first, second), (11, 22));
        crate::context::release_cdata(handle);
    }
}
