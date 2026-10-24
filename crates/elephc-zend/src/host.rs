//! Purpose:
//! The contract between Elephc's generated wrappers and the engine: start an
//! extension, call one of its functions, read back the result, by-reference
//! arguments and exceptions.
//!
//! Called from:
//! - The AST prelude (`src/php_ext/prelude.rs` in Elephc), through `extern`
//!   declarations with no library, and the introspector (`describe`).
//!
//! Key details:
//! - Every entry point takes and returns integers, C strings and opaque
//!   pointers, so the wrappers reach it through the ordinary FFI.
//! - Values are COPIED at the boundary in both directions. An argument arrives
//!   in the layout Elephc's `zval_pack` builds (PHP's structures; a list is a
//!   bucket array marked `nTableMask == -2`) and is rebuilt in engine memory,
//!   so the extension may keep it. A result stays in engine memory and the
//!   wrapper walks it; object-free subtrees are exported in one piece into a
//!   malloc'd tree in exactly the layout `zval_unpack` reads.
//! - A call runs inside a bailout frame: a fatal error jumps back here, and
//!   the call reports it (already printed) instead of killing the process
//!   mid-extension.

use std::ffi::{c_char, c_int, c_void, CStr, CString};
use std::ptr;

use crate::abi::*;
use crate::alloc::{alloc_zeroed, free};
use crate::api::{frame_arg, new_frame};
use crate::global::Global;
use crate::hash;
use crate::startup::{eg, startup};
use crate::string;
use crate::zval::*;

/// Status codes returned to the wrapper.
const CALL_OK: i64 = 0;
const CALL_EXCEPTION: i64 = 1;
const CALL_FATAL: i64 = 2;
const CALL_UNSUPPORTED: i64 = 3;

/// Kinds reported by `elephc_php_ext_value_kind`.
const VALUE_PLAIN: i64 = 0;
const VALUE_STDCLASS: i64 = 1;
const VALUE_ARRAY: i64 = 2;
const VALUE_HOSTED_OBJECT: i64 = 3;

/// The mask marking an exported (or imported) list.
const PACKED_MASK: u32 = 0xfffffffe;

struct Call {
    function: *mut zend_function,
    /// The object a method runs on (borrowed from its Elephc wrapper), or
    /// NULL for a function or a static method.
    this: *mut zend_object,
    /// The class a method was called through.
    scope: *mut zend_class_entry,
    /// The property a property access names (NULL for a call).
    property: *mut zend_string,
    /// The callback a dispatch to a program callable carries (NULL otherwise).
    callback: *mut Callback,
    argc: u32,
    frame: *mut zend_execute_data,
    result: zval,
    exports: Vec<*mut zval>,
    error_class: CString,
    error_message: CString,
    error_code: i64,
    error_code_text: CString,
}

static LIVE_CALLS: Global<i64> = Global::new(0);

/// Where the last walk of each table stopped: see `walk_entry`.
const WALK_CURSORS: usize = 32;
#[derive(Clone, Copy)]
struct Cursor {
    table: *mut HashTable,
    index: u32,
    slot: u32,
    used: u64,
}
static CURSORS: Global<[Cursor; WALK_CURSORS]> =
    Global::new([Cursor { table: ptr::null_mut(), index: 0, slot: 0, used: 0 }; WALK_CURSORS]);
static CLOCK: Global<u64> = Global::new(0);

/// Builds a C string from bytes, dropping interior NUL bytes so the conversion cannot fail.
fn cstring(bytes: &[u8]) -> CString {
    CString::new(bytes.iter().copied().filter(|&b| b != 0).collect::<Vec<u8>>()).unwrap_or_default()
}

/// Records a failure on a call: sets the error class and message and a zero error code, for the
/// wrapper to read back as an exception.
unsafe fn fail(call: *mut Call, class_name: &str, message: &str) {
    (*call).error_class = cstring(class_name.as_bytes());
    (*call).error_message = cstring(message.as_bytes());
    (*call).error_code = 0;
}

// ------------------------------------------------------------ startup and modules

/// Applies `[extension.ini]`: as a php.ini line before the extension starts,
/// through the directive's handler after.
/// c: hidden
#[no_mangle]
pub unsafe extern "C" fn elephc_php_ext_ini(name: *const c_char, value: *const c_char) {
    startup();
    crate::ini::set_configuration(CStr::from_ptr(name).to_bytes(), CStr::from_ptr(value).to_bytes());
}

/// Bailout-protected body that registers an extension module, runs its module startup and then its
/// request startup (RINIT), writing success into `*booted`; invoked through
/// `elephc_zend_protected_call`.
unsafe extern "C" fn boot_trampoline(module: *mut c_void, booted: *mut c_void) {
    let module = module.cast::<zend_module_entry>();
    let registered = crate::module::zend_register_internal_module(module);
    let mut ok = false;
    if !registered.is_null() && crate::module::zend_startup_module_ex(registered) == SUCCESS {
        HOSTED.get().push(registered);
        ok = request_startup(registered);
        if ok {
            IN_REQUEST.get().push(registered);
        }
    }
    *booted.cast::<bool>() = ok;
}

/// Runs `module`'s request startup (RINIT); true when it has none or it succeeded.
unsafe fn request_startup(module: *mut zend_module_entry) -> bool {
    match (*module).request_startup_func {
        Some(rinit) => crate::module::with_module(module, || {
            rinit((*module).r#type as i32, (*module).module_number) == SUCCESS
        }),
        None => true,
    }
}

/// Hosted modules whose module startup (MINIT) has run, in start order.
static HOSTED: Global<Vec<*mut zend_module_entry>> = Global::new(Vec::new());
/// Hosted modules whose request startup (RINIT) has run and whose request
/// shutdown (RSHUTDOWN) has not, in start order.
static IN_REQUEST: Global<Vec<*mut zend_module_entry>> = Global::new(Vec::new());

/// What `lifecycle_trampoline` runs.
const PHASE_REQUEST_SHUTDOWN: usize = 0;
const PHASE_REQUEST_STARTUP: usize = 1;
const PHASE_MODULE_SHUTDOWN: usize = 2;

/// Bailout-protected body of one lifecycle phase over the hosted modules, as
/// php-src orders them: request shutdown (RSHUTDOWN, then the post-deactivate
/// hooks) walks the modules that are in a request in reverse start order,
/// request startup (RINIT) walks every hosted module in start order, and module
/// shutdown (MSHUTDOWN, then the globals destructors) walks them in reverse.
/// A phase a fatal error interrupts stops there, as in PHP.
unsafe extern "C" fn lifecycle_trampoline(phase: *mut c_void, _unused: *mut c_void) {
    let call = |module: *mut zend_module_entry, handler: module_func_t| {
        if let Some(handler) = handler {
            crate::module::with_module(module, || {
                handler((*module).r#type as i32, (*module).module_number);
            });
        }
    };
    match phase as usize {
        PHASE_REQUEST_SHUTDOWN => {
            let modules = std::mem::take(IN_REQUEST.get());
            for &module in modules.iter().rev() {
                call(module, (*module).request_shutdown_func);
            }
            for &module in modules.iter().rev() {
                if let Some(post_deactivate) = (*module).post_deactivate_func {
                    crate::module::with_module(module, || post_deactivate());
                }
            }
        }
        PHASE_REQUEST_STARTUP => {
            for &module in HOSTED.get().clone().iter() {
                if !request_startup(module) {
                    // php-src's `zend_activate_modules`: warn, then end the process. The module
                    // never joins the request, so no RSHUTDOWN runs against state it failed to set up.
                    let name = CStr::from_ptr((*module).name).to_string_lossy();
                    crate::error::report(
                        E_WARNING,
                        format!("request_startup() for {name} module failed").as_bytes(),
                    );
                    std::process::exit(1);
                }
                IN_REQUEST.get().push(module);
            }
        }
        _ => {
            let modules = std::mem::take(HOSTED.get());
            for &module in modules.iter().rev() {
                call(module, (*module).module_shutdown_func);
                (*module).module_started = 0;
            }
            for &module in modules.iter().rev() {
                if (*module).globals_size > 0 && !(*module).globals_ptr.is_null() {
                    if let Some(destructor) = (*module).globals_dtor {
                        destructor((*module).globals_ptr);
                    }
                }
            }
        }
    }
}

/// Runs one lifecycle phase inside a bailout frame.
unsafe fn run_lifecycle(phase: usize) {
    elephc_zend_protected_call(
        Some(lifecycle_trampoline),
        phase as *mut c_void,
        ptr::null_mut(),
        &mut eg().bailout,
    );
}

/// Ends the request that finished in a long-lived process (a `--web` worker serving one request
/// after another): request shutdown (RSHUTDOWN) for every module in it. The worker calls it once
/// the request's destructors have run and before its memory is released, as php-src does.
/// Does nothing before any hosted extension has started, or twice for one request.
/// c: hidden
#[no_mangle]
pub unsafe extern "C" fn elephc_php_ext_request_shutdown() {
    if HOSTED.get().is_empty() {
        return;
    }
    run_lifecycle(PHASE_REQUEST_SHUTDOWN);
}

/// Begins the next request in a long-lived process: request startup (RINIT) for every hosted
/// module, so per-request extension state never crosses requests. Does nothing before any hosted
/// extension has started.
/// c: hidden
#[no_mangle]
pub unsafe extern "C" fn elephc_php_ext_request_startup() {
    if HOSTED.get().is_empty() || !IN_REQUEST.get().is_empty() {
        return;
    }
    run_lifecycle(PHASE_REQUEST_STARTUP);
}

/// The compiled program is about to terminate: the request ends (RSHUTDOWN) and
/// then the modules shut down (MSHUTDOWN), in php-src's order. Called by every
/// exit path after output buffers are flushed; calling it again is harmless.
/// c: hidden
#[no_mangle]
pub unsafe extern "C" fn elephc_php_ext_exit_hook() {
    if HOSTED.get().is_empty() {
        return;
    }
    run_lifecycle(PHASE_REQUEST_SHUTDOWN);
    run_lifecycle(PHASE_MODULE_SHUTDOWN);
}

/// Registers and starts `module` (MINIT, then RINIT) the first time it is
/// used. False, with the reason printed, when it cannot start.
pub unsafe fn boot_module(module: *mut zend_module_entry) -> bool {
    startup();
    if (*module).module_started != 0 {
        return true;
    }
    let mut booted = false;
    let bailed = elephc_zend_protected_call(
        Some(boot_trampoline),
        module.cast(),
        (&mut booted as *mut bool).cast(),
        &mut eg().bailout,
    );
    if bailed != 0 || !booted {
        eprintln!(
            "Fatal error: hosted PHP extension '{}' failed to start",
            CStr::from_ptr((*module).name).to_string_lossy()
        );
        return false;
    }
    true
}

// ------------------------------------------------------------ Elephc -> engine

/// Rebuilds an Elephc-owned zval in engine memory. False for a value with no
/// engine counterpart yet (an Elephc object).
unsafe fn import(dst: *mut zval, src: *const zval) -> bool {
    match type_of(src) {
        IS_UNDEF | IS_NULL => set_null(dst),
        IS_FALSE => set_bool(dst, false),
        IS_TRUE => set_bool(dst, true),
        IS_LONG => set_long(dst, (*src).value.lval),
        IS_DOUBLE => set_double(dst, (*src).value.dval),
        IS_STRING => set_bytes(dst, string::bytes((*src).value.str)),
        IS_ARRAY => {
            let source = (*src).value.arr;
            let target = hash::_zend_new_array((*source).nNumUsed);
            set_arr(dst, target);
            let packed = (*source).nTableMask == PACKED_MASK;
            for i in 0..(*source).nNumUsed {
                let bucket = (*source).data.arData.add(i as usize);
                if type_of(&(*bucket).val) == IS_UNDEF {
                    continue;
                }
                let mut value = zval::UNDEF;
                if !import(&mut value, &(*bucket).val) {
                    zval_ptr_dtor(dst);
                    set_null(dst);
                    return false;
                }
                if packed {
                    hash::zend_hash_next_index_insert_new(target, &mut value);
                } else if !(*bucket).key.is_null() {
                    let key = string::new(string::bytes((*bucket).key));
                    hash::zend_symtable_update(target, key, &mut value);
                    string::zend_string_release(key);
                } else {
                    hash::zend_hash_index_update(target, (*bucket).h, &mut value);
                }
            }
            // An unset trailing key still advances the next append, as in the source.
            (*target).nNextFreeElement = (*source).nNextFreeElement;
        }
        _ => {
            set_null(dst);
            return false;
        }
    }
    true
}

// ------------------------------------------------------------ engine -> Elephc

/// True for a property key the outside world can see (mangled private and
/// protected names start with NUL).
unsafe fn visible_key(key: *const zend_string) -> bool {
    key.is_null() || (*key).len == 0 || *(*key).val.as_ptr() != 0
}

/// The property table of an object zval, which stdClass-like values are walked through.
unsafe fn object_properties(zv: *mut zval) -> *mut HashTable {
    crate::object::zend_elephc_object_properties((*zv).value.obj)
}

/// True when `zv` holds an object or resource anywhere below it. Only called
/// on values `representable` accepted, so it never meets a cycle.
unsafe fn contains_objects(zv: *mut zval) -> bool {
    let zv = resolve(zv);
    match type_of(zv) {
        IS_OBJECT | IS_RESOURCE => true,
        IS_ARRAY => {
            let table = (*zv).value.arr;
            (0..(*table).nNumUsed).any(|i| {
                let p = hash::bucket(table, i);
                type_of(&(*p).val) != IS_UNDEF && contains_objects(&mut (*p).val)
            })
        }
        _ => false,
    }
}

/// Allocates a malloc-backed `zend_string` copy of `data` with refcount 1, in the layout
/// `zval_unpack` reads (not engine-interned or hashed).
unsafe fn export_string(data: &[u8]) -> *mut zend_string {
    let s = alloc_zeroed(std::mem::offset_of!(zend_string, val) + data.len() + 1).cast::<zend_string>();
    (*s).gc.refcount = 1;
    (*s).gc.u.type_info = GC_STRING;
    (*s).h = 0;
    (*s).len = data.len();
    ptr::copy_nonoverlapping(data.as_ptr(), (*s).val.as_mut_ptr().cast::<u8>(), data.len());
    s
}

/// Copies an object-free value into `zval_unpack`'s layout.
unsafe fn export(dst: *mut zval, src: *mut zval) {
    let src = resolve(src);
    match type_of(src) {
        IS_FALSE => set_bool(dst, false),
        IS_TRUE => set_bool(dst, true),
        IS_LONG => set_long(dst, (*src).value.lval),
        IS_DOUBLE => set_double(dst, (*src).value.dval),
        IS_STRING => {
            (*dst).value.str = export_string(string::bytes((*src).value.str));
            (*dst).u1.type_info = IS_STRING_EX;
        }
        IS_ARRAY => {
            let source = (*src).value.arr;
            let count = (*source).nNumOfElements;
            let size = count.max(1);
            // The list layout's next key is its length; a list whose next key moved past it
            // (an unset tail) keeps the hash layout, whose header carries the cursor.
            let list = hash::zend_array_is_list(source) && (*source).nNextFreeElement == count as zend_long;
            let ht = alloc_zeroed(std::mem::size_of::<HashTable>()).cast::<HashTable>();
            let buckets = alloc_zeroed(size as usize * std::mem::size_of::<Bucket>()).cast::<Bucket>();
            (*ht).gc.refcount = 1;
            (*ht).gc.u.type_info = GC_ARRAY;
            (*ht).u.flags = if list { HASH_FLAG_PACKED | HASH_FLAG_STATIC_KEYS } else { 0 };
            // zval_unpack tells the two layouts apart by this mask alone; a
            // hash mask must therefore never be -2, whatever the size.
            (*ht).nTableMask = if list { PACKED_MASK } else { (size.max(8) as i32).wrapping_neg() as u32 };
            (*ht).data.arData = buckets;
            (*ht).nTableSize = size;
            let mut i = 0u32;
            for idx in 0..(*source).nNumUsed {
                let p = hash::bucket(source, idx);
                if type_of(&(*p).val) == IS_UNDEF {
                    continue;
                }
                let bucket = buckets.add(i as usize);
                export(&mut (*bucket).val, &mut (*p).val);
                if !(*p).key.is_null() && !list {
                    (*bucket).key = export_string(string::bytes((*p).key));
                    (*bucket).h = 0;
                } else {
                    (*bucket).key = ptr::null_mut();
                    (*bucket).h = if list { i as zend_ulong } else { (*p).h };
                }
                i += 1;
            }
            // The source's own next free key: recomputing it from the keys
            // would overflow at PHP_INT_MAX.
            (*ht).nNextFreeElement = if list { i as zend_long } else { (*source).nNextFreeElement };
            (*ht).nNumUsed = i;
            (*ht).nNumOfElements = i;
            set_arr(dst, ht);
        }
        _ => set_null(dst),
    }
}

/// Frees an exported tree; every node was allocated by `export`.
unsafe fn free_export(value: *mut zval) {
    match type_of(value) {
        IS_STRING => free((*value).value.str.cast()),
        IS_ARRAY => {
            let ht = (*value).value.arr;
            for i in 0..(*ht).nNumUsed {
                let bucket = (*ht).data.arData.add(i as usize);
                if !(*bucket).key.is_null() {
                    free((*bucket).key.cast());
                }
                free_export(&mut (*bucket).val);
            }
            free((*ht).data.arData.cast());
            free(ht.cast());
        }
        _ => {}
    }
    (*value).u1.type_info = IS_UNDEF as u32;
}

// ------------------------------------------------------------ calls

/// Calls prepared and not yet freed: every wrapper path, the error ones
/// included, must bring this back to zero.
/// c: hidden
#[no_mangle]
pub unsafe extern "C" fn elephc_php_ext_live_calls() -> i64 {
    *LIVE_CALLS.get()
}

/// Starts `module` if needed and prepares a call to `name` with `argc` slots,
/// remembering the function for call site `site` so the next call there
/// skips the lookup (`elephc_php_ext_site_call`). NULL when the module
/// cannot start or does not export `name`.
/// c: hidden
#[no_mangle]
pub unsafe extern "C" fn elephc_php_ext_call_new_at(module: *mut c_void, name: *const c_char, argc: i64, site: i64) -> *mut c_void {
    let module = module.cast::<zend_module_entry>();
    if !boot_module(module) {
        return ptr::null_mut();
    }
    let name_bytes = CStr::from_ptr(name).to_bytes();
    let function = hash::zend_hash_str_find_ptr_lc(crate::startup::cg().function_table, name, name_bytes.len())
        .cast::<zend_function>();
    if function.is_null() || (*function).r#type != ZEND_INTERNAL_FUNCTION {
        eprintln!(
            "Fatal error: hosted PHP extension '{}' does not export {}()",
            CStr::from_ptr((*module).name).to_string_lossy(),
            String::from_utf8_lossy(name_bytes)
        );
        return ptr::null_mut();
    }
    remember_site(site, function, ptr::null_mut());
    new_call(function, ptr::null_mut(), ptr::null_mut(), argc)
}

/// Allocates a `Call` for a function or method with an argument frame of `argc` slots and counts it
/// as live; returns the opaque handle the wrapper passes to the other entry points, to be released
/// by the matching free entry point.
unsafe fn new_call(function: *mut zend_function, this: *mut zend_object, scope: *mut zend_class_entry, argc: i64) -> *mut c_void {
    let argc = argc.max(0) as u32;
    let call = Box::new(Call {
        function,
        this,
        scope,
        property: ptr::null_mut(),
        callback: ptr::null_mut(),
        argc,
        frame: new_frame(function, argc),
        result: zval::UNDEF,
        exports: Vec::new(),
        error_class: CString::default(),
        error_message: CString::default(),
        error_code: 0,
        error_code_text: CString::default(),
    });
    *LIVE_CALLS.get() += 1;
    Box::into_raw(call).cast()
}

/// Copies argument `index` (zero-based) into the frame; a by-reference one is
/// wrapped in a reference the callee writes through. 0, or 3 (unsupported)
/// for a value that cannot be passed yet (an object).
/// c: hidden
#[no_mangle]
pub unsafe extern "C" fn elephc_php_ext_call_arg(handle: *mut c_void, index: i64, value: *mut c_void, by_ref: i64) -> i64 {
    let call = handle.cast::<Call>();
    let slot = frame_arg((*call).frame, index as u32 + 1);
    let mut imported = zval::UNDEF;
    if !import(&mut imported, value.cast()) {
        set_null(slot);
        fail(call, "TypeError", "objects cannot be passed to a hosted PHP extension yet");
        return CALL_UNSUPPORTED;
    }
    if by_ref != 0 {
        let reference = zend_elephc_new_reference(&imported);
        (*slot).value.r#ref = reference;
        (*slot).u1.type_info = IS_REFERENCE_EX;
    } else {
        copy_value(slot, &imported);
    }
    CALL_OK
}

/// Makes argument `index` (zero-based) share the reference argument `of` already holds, for two
/// by-reference parameters bound to one caller variable: a write through either is then seen
/// through both, as in PHP, and `elephc_php_ext_call_ref` answers the same final value for each.
/// Argument `of` must have been passed by reference first; otherwise `index` gets a reference of
/// its own holding null.
/// c: hidden
#[no_mangle]
pub unsafe extern "C" fn elephc_php_ext_call_arg_alias(handle: *mut c_void, index: i64, of: i64) {
    let call = handle.cast::<Call>();
    let slot = frame_arg((*call).frame, index as u32 + 1);
    let shared = frame_arg((*call).frame, of as u32 + 1);
    if type_of(shared) == IS_REFERENCE {
        copy(slot, shared);
    } else {
        let mut empty = zval::UNDEF;
        set_null(&mut empty);
        (*slot).value.r#ref = zend_elephc_new_reference(&empty);
        (*slot).u1.type_info = IS_REFERENCE_EX;
    }
}

/// After a call, moves a pending exception (message, code and class chain) from the executor
/// globals into the call record so the wrapper can read it; does nothing when none is pending.
unsafe fn capture_exception(call: *mut Call) {
    let marker = *CALLBACK_MARKER.get();
    *CALLBACK_THREW.get() = !marker.is_null() && eg().exception == marker;
    forget_callback_marker();
    if let Some((message, code, chain)) = crate::exception::take_pending() {
        (*call).error_message = cstring(&message);
        (*call).error_code = code;
        (*call).error_class = cstring(chain.as_bytes());
    }
}

/// Runs the call inside a bailout frame. 0, 1 (exception: read it with the
/// error accessors) or 2 (fatal, already reported).
/// c: hidden
#[no_mangle]
pub unsafe extern "C" fn elephc_php_ext_call_invoke(handle: *mut c_void) -> i64 {
    let call = handle.cast::<Call>();
    let frame = (*call).frame;
    (*frame).func = (*call).function;
    (*frame).This = zval::UNDEF;
    if !(*call).this.is_null() {
        set_obj(&mut (*frame).This, (*call).this);
    } else {
        (*frame).This.value.ce = (*call).scope;
    }
    (*frame).This.u2.num_args = (*call).argc;
    (*frame).prev_execute_data = eg().current_execute_data;
    (*frame).return_value = &mut (*call).result;
    set_null(&mut (*call).result);
    let handler = (*(*call).function).internal_function.handler;
    let outer = eg().current_execute_data;
    eg().current_execute_data = frame;
    let bailed = elephc_zend_protected_call(
        std::mem::transmute::<zif_handler, Option<unsafe extern "C" fn(*mut c_void, *mut c_void)>>(handler),
        frame.cast(),
        (&mut (*call).result as *mut zval).cast(),
        &mut eg().bailout,
    );
    eg().current_execute_data = outer;
    if bailed != 0 {
        settle_without_exception();
        return CALL_FATAL;
    }
    if !eg().exception.is_null() {
        capture_exception(call);
        return CALL_EXCEPTION;
    }
    settle_without_exception();
    CALL_OK
}

/// The engine-owned return value, valid until the call is freed.
/// c: hidden
#[no_mangle]
pub unsafe extern "C" fn elephc_php_ext_call_result(handle: *mut c_void) -> *mut c_void {
    resolve(&mut (*handle.cast::<Call>()).result).cast()
}

/// The final value of by-reference argument `index`, or NULL when that slot
/// was not passed by reference.
/// c: hidden
#[no_mangle]
pub unsafe extern "C" fn elephc_php_ext_call_ref(handle: *mut c_void, index: i64) -> *mut c_void {
    let call = handle.cast::<Call>();
    let arg = frame_arg((*call).frame, index as u32 + 1);
    if type_of(arg) == IS_REFERENCE {
        resolve(arg).cast()
    } else {
        ptr::null_mut()
    }
}

/// c: hidden
#[no_mangle]
pub unsafe extern "C" fn elephc_php_ext_call_error_class(handle: *mut c_void) -> *const c_char {
    (*handle.cast::<Call>()).error_class.as_ptr()
}

/// c: hidden
#[no_mangle]
pub unsafe extern "C" fn elephc_php_ext_call_error_message(handle: *mut c_void) -> *const c_char {
    (*handle.cast::<Call>()).error_message.as_ptr()
}

/// The error's code as decimal text: the FFI returns an `int` as a C `int`,
/// which would cut a 64-bit `zend_long` code to 32 bits. The text lives until
/// the next call to this function on `handle`, or until the call is freed.
/// c: hidden
#[no_mangle]
pub unsafe extern "C" fn elephc_php_ext_call_error_code(handle: *mut c_void) -> *const c_char {
    let call = handle.cast::<Call>();
    (*call).error_code_text = cstring((*call).error_code.to_string().as_bytes());
    (*call).error_code_text.as_ptr()
}

/// Releases the arguments and result (refcount-aware: a value the extension
/// kept survives) and every exported copy. A cyclic result the wrapper
/// refused is not reclaimed: there is no cycle collector.
/// c: hidden
#[no_mangle]
pub unsafe extern "C" fn elephc_php_ext_call_free(handle: *mut c_void) {
    if handle.is_null() {
        return;
    }
    let call = Box::from_raw(handle.cast::<Call>());
    for i in 0..call.argc {
        zval_ptr_dtor(frame_arg(call.frame, i + 1));
    }
    let mut result = call.result;
    zval_ptr_dtor(&mut result);
    if !call.property.is_null() {
        string::zend_string_release(call.property);
    }
    // The cursors name tables by address; one freed here may come back at the
    // same address in the next call's result.
    for cursor in CURSORS.get().iter_mut() {
        cursor.table = ptr::null_mut();
        cursor.used = 0;
    }
    for exported in &call.exports {
        free_export(*exported);
        free((*exported).cast());
    }
    free(call.frame.cast());
    *LIVE_CALLS.get() -= 1;
}

// ------------------------------------------------------------ value walking

/// Whether `zv` can cross into Elephc: objects are stdClass, there is no
/// resource, and no array or object contains itself. A value shared along two
/// paths is accepted (and arrives as two copies); only one on its own path is
/// a cycle. On refusal the reason is left on `call` as its error.
unsafe fn representable(call: *mut Call, zv: *mut zval) -> bool {
    let zv = resolve(zv);
    let kind = type_of(zv);
    if kind == IS_OBJECT && is_hosted_object((*zv).value.obj) {
        // Crosses as a handle, its contents staying in the engine.
        return true;
    }
    if kind == IS_RESOURCE || (kind == IS_OBJECT && (*(*zv).value.obj).ce != crate::class::zend_standard_class_def) {
        let name = if kind == IS_OBJECT { crate::object::class_name((*zv).value.obj) } else { "resource".to_string() };
        fail(
            call,
            "Error",
            &format!("a hosted PHP extension returned a value of type {name}, which Elephc cannot represent yet"),
        );
        return false;
    }
    let (table, guard) = match kind {
        IS_OBJECT => (object_properties(zv), (*zv).value.obj.cast::<zend_refcounted>()),
        IS_ARRAY if (*(*zv).value.arr).gc.u.type_info & GC_IMMUTABLE == 0 => {
            ((*zv).value.arr, (*zv).value.arr.cast::<zend_refcounted>())
        }
        _ => return true,
    };
    if (*guard).gc.u.type_info & GC_PROTECTED != 0 {
        let what = if kind == IS_OBJECT { "object" } else { "array" };
        fail(
            call,
            "Error",
            &format!("a hosted PHP extension returned a recursive {what}, which Elephc cannot represent yet"),
        );
        return false;
    }
    (*guard).gc.u.type_info |= GC_PROTECTED;
    let mut accepted = true;
    for i in 0..(*table).nNumUsed {
        let p = hash::bucket(table, i);
        let mut entry: *mut zval = &mut (*p).val;
        if type_of(entry) == IS_INDIRECT {
            entry = (*entry).value.zv;
        }
        if type_of(entry) == IS_UNDEF {
            continue;
        }
        if !representable(call, entry) {
            accepted = false;
            break;
        }
    }
    (*guard).gc.u.type_info &= !GC_PROTECTED;
    accepted
}

/// Called once on each value the wrapper rebuilds (the result, each by-ref
/// argument) before any walk: 0 when it can cross, 1 with the reason left as
/// the call's error.
/// c: hidden
#[no_mangle]
pub unsafe extern "C" fn elephc_php_ext_value_check(handle: *mut c_void, value: *mut c_void) -> i64 {
    if value.is_null() || representable(handle.cast(), value.cast()) {
        0
    } else {
        1
    }
}

/// How the wrapper must turn `value` into an Elephc value.
/// c: hidden
#[no_mangle]
pub unsafe extern "C" fn elephc_php_ext_value_kind(value: *mut c_void) -> i64 {
    let zv = resolve(value.cast());
    if type_of(zv) == IS_OBJECT && is_hosted_object((*zv).value.obj) {
        VALUE_HOSTED_OBJECT
    } else if type_of(zv) == IS_OBJECT {
        VALUE_STDCLASS
    } else if contains_objects(zv) {
        VALUE_ARRAY
    } else {
        VALUE_PLAIN
    }
}

/// Exports an object-free value in `zval_unpack`'s layout. The copy belongs to
/// the call and is freed with it.
/// c: hidden
#[no_mangle]
pub unsafe extern "C" fn elephc_php_ext_value_export(handle: *mut c_void, value: *mut c_void) -> *mut c_void {
    let call = handle.cast::<Call>();
    let node = alloc_zeroed(std::mem::size_of::<zval>()).cast::<zval>();
    export(node, value.cast());
    (*call).exports.push(node);
    node.cast()
}

/// The table a stdClass or array value is walked through.
unsafe fn walked_table(value: *mut zval) -> *mut HashTable {
    let zv = resolve(value);
    match type_of(zv) {
        IS_OBJECT => object_properties(zv),
        IS_ARRAY => (*zv).value.arr,
        _ => ptr::null_mut(),
    }
}

/// Finds the walk cursor remembered for a table, refreshing its use stamp, or recycles the least
/// recently used of the fixed set of cursors for it, starting at the first entry.
unsafe fn cursor_for(table: *mut HashTable) -> &'static mut Cursor {
    let clock = CLOCK.get();
    *clock += 1;
    let now = *clock;
    let cursors = CURSORS.get();
    if let Some(index) = cursors.iter().position(|c| c.table == table) {
        cursors[index].used = now;
        return &mut cursors[index];
    }
    let oldest = cursors
        .iter()
        .enumerate()
        .min_by_key(|(_, c)| c.used)
        .map(|(index, _)| index)
        .unwrap_or(0);
    cursors[oldest] = Cursor { table, index: 0, slot: 0, used: now };
    &mut cursors[oldest]
}

/// The `index`-th visible entry and its slot. Wrappers walk each table in
/// order (key, then value, entry after entry), so each table's previous
/// answer is remembered and the next lookup is O(1).
unsafe fn walk_entry(value: *mut zval, index: i64) -> *mut Bucket {
    let table = walked_table(value);
    if table.is_null() || index < 0 {
        return ptr::null_mut();
    }
    let is_object = type_of(resolve(value)) == IS_OBJECT;
    let cursor = cursor_for(table);
    let (mut visible, mut slot) = if cursor.index <= index as u32 { (cursor.index, cursor.slot) } else { (0, 0) };
    while slot < (*table).nNumUsed {
        let bucket = hash::bucket(table, slot);
        let mut entry: *mut zval = &mut (*bucket).val;
        if type_of(entry) == IS_INDIRECT {
            entry = (*entry).value.zv;
        }
        if type_of(entry) == IS_UNDEF || (is_object && !visible_key((*bucket).key)) {
            slot += 1;
            continue;
        }
        if visible == index as u32 {
            cursor.index = visible;
            cursor.slot = slot;
            return bucket;
        }
        visible += 1;
        slot += 1;
    }
    ptr::null_mut()
}

/// Number of entries (array) or visible properties (stdClass).
/// c: hidden
#[no_mangle]
pub unsafe extern "C" fn elephc_php_ext_value_count(value: *mut c_void) -> i64 {
    let table = walked_table(value.cast());
    if table.is_null() {
        return 0;
    }
    if type_of(resolve(value.cast())) != IS_OBJECT {
        return (*table).nNumOfElements as i64;
    }
    (0..(*table).nNumUsed)
        .filter(|&i| {
            let p = hash::bucket(table, i);
            let mut entry: *mut zval = &mut (*p).val;
            if type_of(entry) == IS_INDIRECT {
                entry = (*entry).value.zv;
            }
            type_of(entry) != IS_UNDEF && visible_key((*p).key)
        })
        .count() as i64
}

/// True when an array value's keys are exactly 0..n-1 in order.
/// c: hidden
#[no_mangle]
pub unsafe extern "C" fn elephc_php_ext_value_is_list(value: *mut c_void) -> i64 {
    let zv = resolve(value.cast());
    (type_of(zv) == IS_ARRAY && hash::zend_array_is_list((*zv).value.arr)) as i64
}

/// The key of entry `index`, exported like a value: a string key with its
/// full length (it may hold NUL bytes), or an integer key.
/// c: hidden
#[no_mangle]
pub unsafe extern "C" fn elephc_php_ext_value_key(handle: *mut c_void, value: *mut c_void, index: i64) -> *mut c_void {
    let bucket = walk_entry(value.cast(), index);
    let mut key = zval::UNDEF;
    if bucket.is_null() {
        set_null(&mut key);
    } else if !(*bucket).key.is_null() {
        key.value.str = (*bucket).key;
        key.u1.type_info = IS_STRING_EX;
    } else {
        set_long(&mut key, (*bucket).h as zend_long);
    }
    elephc_php_ext_value_export(handle, (&mut key as *mut zval).cast())
}

/// The name of visible property `index` of a stdClass, as a C string.
/// c: hidden
#[no_mangle]
pub unsafe extern "C" fn elephc_php_ext_value_name(value: *mut c_void, index: i64) -> *const c_char {
    let bucket = walk_entry(value.cast(), index);
    if bucket.is_null() || (*bucket).key.is_null() {
        c"".as_ptr()
    } else {
        (*(*bucket).key).val.as_ptr()
    }
}

/// The value of entry `index`, as an engine zval the wrapper walks further.
/// c: hidden
#[no_mangle]
pub unsafe extern "C" fn elephc_php_ext_value_at(value: *mut c_void, index: i64) -> *mut c_void {
    let bucket = walk_entry(value.cast(), index);
    if bucket.is_null() {
        return ptr::null_mut();
    }
    resolve(&mut (*bucket).val).cast()
}

// ------------------------------------------------------------ objects

/// Objects held by Elephc wrappers, by their engine handle (`zend_object.handle`),
/// with how many wrappers hold one reference each. Wrappers name objects by
/// handle, a small integer, rather than by address: the FFI carries an `int`
/// return as a C `int`.
static HELD: Global<Option<std::collections::HashMap<u32, (*mut zend_object, u32)>>> = Global::new(None);

/// The map from engine object handles to objects held by wrappers, created on first use.
unsafe fn held() -> &'static mut std::collections::HashMap<u32, (*mut zend_object, u32)> {
    HELD.get().get_or_insert_with(Default::default)
}

/// Records one more wrapper reference to `object` (the caller already
/// counted it in the object's refcount) and returns its handle.
unsafe fn hold(object: *mut zend_object) -> i64 {
    let entry = held().entry((*object).handle).or_insert((object, 0));
    entry.1 += 1;
    i64::from((*object).handle)
}

/// The object a wrapper names, or NULL.
unsafe fn held_object(handle: i64) -> *mut zend_object {
    u32::try_from(handle).ok().and_then(|handle| held().get(&handle)).map_or(ptr::null_mut(), |entry| entry.0)
}

/// True for an object of a class a hosted extension registered: Elephc
/// declares a wrapper class for each, which holds the object by its address.
/// Throwables are not: Elephc rebuilds a thrown one by class, and refuses one
/// returned as a value.
unsafe fn is_hosted_object(object: *mut zend_object) -> bool {
    let ce = (*object).ce;
    (*ce).r#type == ZEND_INTERNAL_CLASS as c_char
        && !(*ce).info.internal.module.is_null()
        && !crate::module::is_provided((*ce).info.internal.module)
        && !crate::class::instanceof_function(ce, crate::exception::zend_ce_throwable)
}

/// Looks up a class entry by its NUL-terminated name, returning NULL when it is not registered.
unsafe fn find_class(name: *const c_char) -> *mut zend_class_entry {
    crate::class::lookup(CStr::from_ptr(name).to_bytes())
}

/// Bailout-protected body that creates an object of the class `ce` and stores it in `*object` (left
/// untouched on failure); invoked through `elephc_zend_protected_call`.
unsafe extern "C" fn create_trampoline(ce: *mut c_void, object: *mut c_void) {
    let mut value = zval::UNDEF;
    if crate::object::object_init_ex(&mut value, ce.cast()) == SUCCESS {
        *object.cast::<*mut zend_object>() = value.value.obj;
    }
}

/// Starts `module` if needed and creates an object of `class`, as `new` does
/// before running the constructor. Its handle (the wrapper holding its one
/// reference), or 0 with the reason printed.
/// c: hidden
#[no_mangle]
pub unsafe extern "C" fn elephc_php_ext_object_new(module: *mut c_void, class: *const c_char) -> i64 {
    if !boot_module(module.cast()) {
        return 0;
    }
    let ce = find_class(class);
    if ce.is_null() {
        eprintln!("Fatal error: Uncaught Error: Class \"{}\" not found", CStr::from_ptr(class).to_string_lossy());
        return 0;
    }
    let mut object: *mut zend_object = ptr::null_mut();
    let bailed = elephc_zend_protected_call(
        Some(create_trampoline),
        ce.cast(),
        (&mut object as *mut *mut zend_object).cast(),
        &mut eg().bailout,
    );
    if bailed != 0 || object.is_null() {
        if let Some((message, _, _)) = crate::exception::take_pending() {
            eprintln!("Fatal error: Uncaught Error: {}", String::from_utf8_lossy(&message));
        }
        return 0;
    }
    hold(object)
}

/// Drops the wrapper's reference; the last one runs the object's destructor
/// and frees it.
/// c: hidden
#[no_mangle]
pub unsafe extern "C" fn elephc_php_ext_object_release(handle: i64) {
    let object = held_object(handle);
    if object.is_null() {
        return;
    }
    let key = (*object).handle;
    if let Some(entry) = held().get_mut(&key) {
        entry.1 -= 1;
        if entry.1 == 0 {
            held().remove(&key);
        }
    }
    crate::object::zend_object_release(object);
}

/// Prepares a call to method `name` of `class`, on `object` (0 for a static
/// call), remembering the method and its class for call site `site`
/// (`elephc_php_ext_site_call`). NULL, with the reason printed, when the
/// class has no such method.
/// c: hidden
#[no_mangle]
pub unsafe extern "C" fn elephc_php_ext_method_new_at(
    module: *mut c_void,
    class: *const c_char,
    name: *const c_char,
    object: i64,
    argc: i64,
    site: i64,
) -> *mut c_void {
    if !boot_module(module.cast()) {
        return ptr::null_mut();
    }
    let ce = find_class(class);
    let name_bytes = CStr::from_ptr(name).to_bytes();
    let function = if ce.is_null() {
        ptr::null_mut()
    } else {
        hash::zend_hash_str_find_ptr_lc(&(*ce).function_table, name, name_bytes.len()).cast::<zend_function>()
    };
    if function.is_null() || (*function).r#type != ZEND_INTERNAL_FUNCTION {
        eprintln!(
            "Fatal error: Uncaught Error: Call to undefined method {}::{}()",
            CStr::from_ptr(class).to_string_lossy(),
            String::from_utf8_lossy(name_bytes)
        );
        return ptr::null_mut();
    }
    remember_site(site, function, ce);
    let this = held_object(object);
    let scope = if this.is_null() { ce } else { (*this).ce };
    new_call(function, this, scope, argc)
}

// ------------------------------------------------------------ call sites

/// What each wrapper call site resolved on its first call, by site number:
/// the function or method, and the method's class.
static SITES: Global<Vec<(*mut zend_function, *mut zend_class_entry)>> = Global::new(Vec::new());

/// Records the callee resolved for `site`; a negative site is not cached.
unsafe fn remember_site(site: i64, function: *mut zend_function, ce: *mut zend_class_entry) {
    let Ok(index) = usize::try_from(site) else { return };
    let sites = SITES.get();
    if sites.len() <= index {
        sites.resize(index + 1, (ptr::null_mut(), ptr::null_mut()));
    }
    sites[index] = (function, ce);
}

/// Prepares a call at a site whose callee an earlier call resolved: no name
/// to convert or look up, the module already started. `object` is the held
/// object of an instance method, 0 otherwise. NULL when the site has not
/// been resolved yet or the object is gone: the wrapper then takes the
/// resolving path, which reports a released object.
/// c: hidden
#[no_mangle]
pub unsafe extern "C" fn elephc_php_ext_site_call(site: i64, object: i64, argc: i64) -> *mut c_void {
    let Some(&(function, ce)) = usize::try_from(site).ok().and_then(|index| SITES.get().get(index)) else {
        return ptr::null_mut();
    };
    if function.is_null() {
        return ptr::null_mut();
    }
    if object == 0 {
        return new_call(function, ptr::null_mut(), ce, argc);
    }
    let this = held_object(object);
    if this.is_null() {
        return ptr::null_mut();
    }
    new_call(function, this, (*this).ce, argc)
}

/// Passes an `int` argument straight into the frame, with no boxed value.
/// c: hidden
#[no_mangle]
pub unsafe extern "C" fn elephc_php_ext_call_arg_long(handle: *mut c_void, index: i64, value: i64) {
    set_long(frame_arg((*handle.cast::<Call>()).frame, index as u32 + 1), value);
}

/// Passes a `float` argument straight into the frame.
/// c: hidden
#[no_mangle]
pub unsafe extern "C" fn elephc_php_ext_call_arg_double(handle: *mut c_void, index: i64, value: f64) {
    set_double(frame_arg((*handle.cast::<Call>()).frame, index as u32 + 1), value);
}

/// Passes a `bool` argument (0 or 1) straight into the frame.
/// c: hidden
#[no_mangle]
pub unsafe extern "C" fn elephc_php_ext_call_arg_bool(handle: *mut c_void, index: i64, value: i64) {
    set_bool(frame_arg((*handle.cast::<Call>()).frame, index as u32 + 1), value != 0);
}

/// 1 when the call's result is exactly of `kind` (1 int, 2 float, 3 bool),
/// so the wrapper may read it directly instead of rebuilding a value: an
/// int with `ptr_get()` on `elephc_php_ext_call_result` (an extern `int`
/// return keeps only 32 bits), a float or bool with the accessors below.
/// c: hidden
#[no_mangle]
pub unsafe extern "C" fn elephc_php_ext_call_result_is(handle: *mut c_void, kind: i64) -> i64 {
    let result = resolve(&mut (*handle.cast::<Call>()).result);
    let matches = match kind {
        1 => type_of(result) == IS_LONG,
        2 => type_of(result) == IS_DOUBLE,
        3 => matches!(type_of(result), IS_TRUE | IS_FALSE),
        _ => false,
    };
    matches as i64
}

/// The call's `float` result.
/// c: hidden
#[no_mangle]
pub unsafe extern "C" fn elephc_php_ext_call_result_double(handle: *mut c_void) -> f64 {
    (*resolve(&mut (*handle.cast::<Call>()).result)).value.dval
}

/// The call's `bool` result, as 0 or 1.
/// c: hidden
#[no_mangle]
pub unsafe extern "C" fn elephc_php_ext_call_result_bool(handle: *mut c_void) -> i64 {
    (type_of(resolve(&mut (*handle.cast::<Call>()).result)) == IS_TRUE) as i64
}

/// Passes a hosted object (by its handle) as argument `index`.
/// c: hidden
#[no_mangle]
pub unsafe extern "C" fn elephc_php_ext_call_arg_object(handle: *mut c_void, index: i64, object: i64, by_ref: i64) -> i64 {
    let call = handle.cast::<Call>();
    let slot = frame_arg((*call).frame, index as u32 + 1);
    let object = held_object(object);
    if object.is_null() {
        set_null(slot);
        fail(call, "Error", "a hosted object was used after it was released");
        return CALL_UNSUPPORTED;
    }
    (*object).gc.refcount += 1;
    let mut value = zval::UNDEF;
    set_obj(&mut value, object);
    if by_ref != 0 {
        (*slot).value.r#ref = zend_elephc_new_reference(&value);
        (*slot).u1.type_info = IS_REFERENCE_EX;
    } else {
        copy_value(slot, &value);
    }
    CALL_OK
}

/// 1 when the call returned exactly `object` (a method returning `$this`).
/// c: hidden
#[no_mangle]
pub unsafe extern "C" fn elephc_php_ext_call_returned(handle: *mut c_void, object: i64) -> i64 {
    let result = resolve(&mut (*handle.cast::<Call>()).result);
    (type_of(result) == IS_OBJECT && i64::from((*(*result).value.obj).handle) == object) as i64
}

/// The class of a hosted object value.
/// c: hidden
#[no_mangle]
pub unsafe extern "C" fn elephc_php_ext_object_class(value: *mut c_void) -> *const c_char {
    let zv = resolve(value.cast());
    (*(*(*zv).value.obj).ce).name.cast::<zend_string>().as_ref().map_or(c"".as_ptr(), |name| name.val.as_ptr())
}

/// A new reference to a hosted object value, for the wrapper that adopts it.
/// c: hidden
#[no_mangle]
pub unsafe extern "C" fn elephc_php_ext_object_take(value: *mut c_void) -> i64 {
    let zv = resolve(value.cast());
    let object = (*zv).value.obj;
    (*object).gc.refcount += 1;
    hold(object)
}

// ------------------------------------------------------------ properties

/// Prepares an access to property `name` of a held object; a write takes its
/// value as argument 0 (`argc` 1). NULL, with the reason printed, when the
/// object is gone.
/// c: hidden
#[no_mangle]
pub unsafe extern "C" fn elephc_php_ext_property_new(object: i64, name: *const c_char, argc: i64) -> *mut c_void {
    let this = held_object(object);
    if this.is_null() {
        eprintln!("Fatal error: a hosted object was used after it was released");
        return ptr::null_mut();
    }
    let call = new_call(ptr::null_mut(), this, (*this).ce, argc).cast::<Call>();
    (*call).property = string::new(CStr::from_ptr(name).to_bytes());
    call.cast()
}

const PROPERTY_READ: i64 = 0;
const PROPERTY_WRITE: i64 = 1;
const PROPERTY_ISSET: i64 = 2;
const PROPERTY_UNSET: i64 = 3;
const PROPERTY_DEBUG: i64 = 4;
const PROPERTY_CLONE: i64 = 5;

/// Bailout-protected body of a property access: runs the read, write, isset or unset object handler
/// selected by `*op` on the call's object and property, leaving a read or isset result in the call.
unsafe extern "C" fn property_trampoline(handle: *mut c_void, op: *mut c_void) {
    let call = handle.cast::<Call>();
    let object = (*call).this;
    let handlers = (*object).handlers;
    let name = (*call).property;
    match *op.cast::<i64>() {
        PROPERTY_READ => {
            let mut scratch = zval::UNDEF;
            if let Some(read) = (*handlers).read_property {
                let value = read(object, name, BP_VAR_R as c_int, ptr::null_mut(), &mut scratch);
                if value == &mut scratch as *mut zval {
                    (*call).result = scratch;
                } else if !value.is_null() {
                    copy(&mut (*call).result, value);
                }
            }
        }
        PROPERTY_WRITE => {
            if let Some(write) = (*handlers).write_property {
                write(object, name, frame_arg((*call).frame, 1), ptr::null_mut());
            }
        }
        PROPERTY_ISSET => {
            let set = match (*handlers).has_property {
                Some(has) => has(object, name, ZEND_PROPERTY_ISSET as c_int, ptr::null_mut()) != 0,
                None => false,
            };
            set_bool(&mut (*call).result, set);
        }
        PROPERTY_UNSET => {
            if let Some(unset) = (*handlers).unset_property {
                unset(object, name, ptr::null_mut());
            }
        }
        PROPERTY_DEBUG => {
            // What `var_dump()` shows: the debug-info handler's table, else the properties.
            let mut is_temp: c_int = 0;
            let table = match (*handlers).get_debug_info {
                Some(debug) => debug(object, &mut is_temp),
                None => crate::object::zend_elephc_object_properties(object),
            };
            if !table.is_null() {
                let owned = if is_temp != 0 { table } else { hash::zend_array_dup(table) };
                set_arr(&mut (*call).result, owned);
            }
        }
        PROPERTY_CLONE => match (*handlers).clone_obj {
            Some(clone) => {
                let copy = clone(object);
                if !copy.is_null() {
                    set_obj(&mut (*call).result, copy);
                }
            }
            None => {
                let class = string::bytes((*(*object).ce).name);
                let message = format!("Trying to clone an uncloneable object of class {}", String::from_utf8_lossy(class));
                crate::exception::throw_error(crate::exception::zend_ce_error, &message);
            }
        },
        _ => {}
    }
}

/// Runs a prepared property access: `op` 0 read (the result), 1 write, 2
/// isset (a bool result), 3 unset, 4 the debug table (an array result), 5 a
/// clone (an object result). Statuses as `elephc_php_ext_call_invoke`.
/// c: hidden
#[no_mangle]
pub unsafe extern "C" fn elephc_php_ext_property_invoke(handle: *mut c_void, op: i64) -> i64 {
    let call = handle.cast::<Call>();
    set_null(&mut (*call).result);
    let mut op = op;
    let bailed = elephc_zend_protected_call(
        Some(property_trampoline),
        handle,
        (&mut op as *mut i64).cast(),
        &mut eg().bailout,
    );
    if bailed != 0 {
        settle_without_exception();
        return CALL_FATAL;
    }
    if !eg().exception.is_null() {
        capture_exception(call);
        return CALL_EXCEPTION;
    }
    settle_without_exception();
    CALL_OK
}

// ------------------------------------------------------------ iteration

/// Iterations in progress, by id: the call each one keeps its error, its
/// current key and the exported copies in, and the extension's iterator.
static ITERATIONS: Global<Option<std::collections::HashMap<i64, (*mut Call, *mut zend_object_iterator)>>> = Global::new(None);
static NEXT_ITERATION: Global<i64> = Global::new(1);

const ITER_REWIND: i64 = 0;
const ITER_VALID: i64 = 1;
const ITER_NEXT: i64 = 2;
const ITER_CURRENT: i64 = 3;
const ITER_KEY: i64 = 4;

/// The iterations table, created on first use.
unsafe fn iterations() -> &'static mut std::collections::HashMap<i64, (*mut Call, *mut zend_object_iterator)> {
    ITERATIONS.get().get_or_insert_with(Default::default)
}

/// Frees the exported copies a call holds and forgets the walk cursors, so a
/// long iteration keeps only the current element's copies alive.
unsafe fn release_exports(call: *mut Call) {
    for cursor in CURSORS.get().iter_mut() {
        cursor.table = ptr::null_mut();
        cursor.used = 0;
    }
    for exported in (*call).exports.drain(..) {
        free_export(exported);
        free(exported.cast());
    }
}

/// What one protected iteration step reads back.
struct IterStep {
    call: *mut Call,
    iterator: *mut zend_object_iterator,
    op: i64,
    /// `valid`'s answer, or `current`'s value.
    valid: bool,
    value: *mut zval,
}

/// Bailout-protected body of one iteration step: runs the iterator handler `step.op` selects and
/// leaves its answer in the step.
unsafe extern "C" fn iter_trampoline(step: *mut c_void, _unused: *mut c_void) {
    let step = &mut *step.cast::<IterStep>();
    let iterator = step.iterator;
    let funcs = (*iterator).funcs;
    match step.op {
        ITER_REWIND => {
            (*iterator).index = 0;
            if let Some(rewind) = (*funcs).rewind {
                rewind(iterator);
            }
        }
        ITER_VALID => {
            step.valid = match (*funcs).valid {
                Some(valid) => valid(iterator) == SUCCESS,
                None => false,
            };
        }
        ITER_NEXT => {
            if let Some(next) = (*funcs).move_forward {
                next(iterator);
            }
            (*iterator).index += 1;
        }
        ITER_CURRENT => {
            if let Some(current) = (*funcs).get_current_data {
                step.value = current(iterator);
            }
        }
        _ => {
            let key = &mut (*step.call).result;
            zval_ptr_dtor(key);
            match (*funcs).get_current_key {
                Some(get_key) => {
                    set_null(key);
                    get_key(iterator, key);
                }
                None => set_long(key, (*iterator).index as zend_long),
            }
            step.value = key;
        }
    }
}

/// Runs one step under the bailout frame. 0 (with `step` filled), 1 when the
/// extension threw (the error is left on the call), 2 on a fatal error.
unsafe fn iter_run(step: &mut IterStep) -> i64 {
    let bailed = elephc_zend_protected_call(
        Some(iter_trampoline),
        (step as *mut IterStep).cast(),
        ptr::null_mut(),
        &mut eg().bailout,
    );
    if bailed != 0 {
        settle_without_exception();
        return CALL_FATAL;
    }
    if !eg().exception.is_null() {
        capture_exception(step.call);
        return CALL_EXCEPTION;
    }
    settle_without_exception();
    CALL_OK
}

/// Bailout-protected body of `foreach` setup: asks the object's class for its iterator and stores
/// it in `*out` (left NULL when the class has none).
unsafe extern "C" fn get_iterator_trampoline(object: *mut c_void, out: *mut c_void) {
    let object = object.cast::<zend_object>();
    let Some(get_iterator) = (*(*object).ce).get_iterator else { return };
    let mut value = zval::UNDEF;
    set_obj(&mut value, object);
    *out.cast::<*mut zend_object_iterator>() = get_iterator((*object).ce, &mut value, 0);
}

/// Starts iterating a held object through its class's `get_iterator`, as
/// `foreach` does. The iteration's id, 0 when the class cannot be iterated,
/// or -id when starting it threw (its error is on `elephc_php_ext_iter_call`).
/// c: hidden
#[no_mangle]
pub unsafe extern "C" fn elephc_php_ext_iter_new(object: i64) -> i64 {
    let object = held_object(object);
    if object.is_null() || (*(*object).ce).get_iterator.is_none() {
        return 0;
    }
    let call = new_call(ptr::null_mut(), object, (*object).ce, 0).cast::<Call>();
    let mut iterator: *mut zend_object_iterator = ptr::null_mut();
    let bailed = elephc_zend_protected_call(
        Some(get_iterator_trampoline),
        object.cast(),
        (&mut iterator as *mut *mut zend_object_iterator).cast(),
        &mut eg().bailout,
    );
    let id = *NEXT_ITERATION.get();
    *NEXT_ITERATION.get() += 1;
    if bailed != 0 || !eg().exception.is_null() || iterator.is_null() {
        if !eg().exception.is_null() {
            capture_exception(call);
        } else {
            fail(call, "Error", "a hosted PHP extension failed to start iterating an object");
        }
        iterations().insert(id, (call, ptr::null_mut()));
        return -id;
    }
    settle_without_exception();
    iterations().insert(id, (call, iterator));
    id
}

/// The call an iteration keeps its error and exported copies in, for the
/// wrapper's error accessors and value walks. NULL for an unknown id.
/// c: hidden
#[no_mangle]
pub unsafe extern "C" fn elephc_php_ext_iter_call(id: i64) -> *mut c_void {
    iterations().get(&id.abs()).map_or(ptr::null_mut(), |entry| entry.0.cast())
}

/// One iteration step: 0 rewind, 1 valid, 2 next. Returns 0 (for valid:
/// 1 when there is a current element, 0 when done), -1 when the extension
/// threw, -2 on a fatal error already reported.
/// c: hidden
#[no_mangle]
pub unsafe extern "C" fn elephc_php_ext_iter_step(id: i64, op: i64) -> i64 {
    let Some(&(call, iterator)) = iterations().get(&id) else { return -2 };
    if iterator.is_null() {
        return -1;
    }
    if op != ITER_VALID {
        // The previous element was rebuilt into Elephc values already.
        release_exports(call);
    }
    let mut step = IterStep { call, iterator, op, valid: false, value: ptr::null_mut() };
    match iter_run(&mut step) {
        CALL_OK if op == ITER_VALID => i64::from(step.valid),
        CALL_OK => 0,
        CALL_EXCEPTION => -1,
        _ => -2,
    }
}

/// The current element (`op` 3) or key (`op` 4) as an engine zval the wrapper
/// walks with the iteration's call. NULL when the extension threw (the error
/// is on the call: its class is then non-empty) or failed fatally (already
/// reported).
/// c: hidden
#[no_mangle]
pub unsafe extern "C" fn elephc_php_ext_iter_fetch(id: i64, op: i64) -> *mut c_void {
    let Some(&(call, iterator)) = iterations().get(&id) else { return ptr::null_mut() };
    if iterator.is_null() {
        return ptr::null_mut();
    }
    let op = if op == ITER_CURRENT { ITER_CURRENT } else { ITER_KEY };
    let mut step = IterStep { call, iterator, op, valid: false, value: ptr::null_mut() };
    if iter_run(&mut step) != CALL_OK {
        return ptr::null_mut();
    }
    if step.value.is_null() {
        // An iterator with no current data yields null, as PHP's does.
        let result = &mut (*call).result;
        zval_ptr_dtor(result);
        set_null(result);
        return (result as *mut zval).cast();
    }
    resolve(step.value).cast()
}

/// Ends an iteration: destroys the extension's iterator (releasing its
/// reference to the object) and frees the call. Unknown ids are ignored.
/// c: hidden
#[no_mangle]
pub unsafe extern "C" fn elephc_php_ext_iter_free(id: i64) {
    let Some((call, iterator)) = iterations().remove(&id.abs()) else { return };
    if !iterator.is_null() {
        crate::api::zend_iterator_dtor(iterator);
    }
    elephc_php_ext_call_free(call.cast());
}

// ------------------------------------------------------------ callables

/// The wrapper-side function the engine calls back: `(slot, call)` runs the
/// program callable in `slot` with the arguments `call` carries and returns 0,
/// or 1 when it threw (the wrapper keeps the throwable and rethrows it once
/// the extension returns); `(slot, NULL)` releases the slot.
type Dispatcher = unsafe extern "C" fn(slot: i64, call: *mut c_void) -> i64;

static DISPATCHER: Global<Option<Dispatcher>> = Global::new(None);

/// Program callables an extension holds, by `Closure` object: the slot each
/// one names in the wrapper's callable table.
static CLOSURES: Global<Option<std::collections::HashMap<usize, i64>>> = Global::new(None);

/// The handlers of a hosted `Closure`: the standard ones, with a `free_obj`
/// that releases the program's slot.
static CLOSURE_HANDLERS: Global<Option<zend_object_handlers>> = Global::new(None);

/// The function a hosted closure presents as its `fcc.function_handler`, for
/// extensions that read the callable's name or flags before calling it.
static CLOSURE_FUNCTION: Global<Option<Box<zend_internal_function>>> = Global::new(None);

/// What a callback hands to the wrapper: the arguments and where the result goes.
struct Callback {
    params: *mut zval,
    count: u32,
    result: zval,
}

/// The closures table, created on first use.
unsafe fn closures() -> &'static mut std::collections::HashMap<usize, i64> {
    CLOSURES.get().get_or_insert_with(Default::default)
}

/// Releases the program slot of a hosted closure the engine frees.
unsafe extern "C" fn closure_free(object: *mut zend_object) {
    if let Some(slot) = closures().remove(&(object as usize)) {
        if let Some(dispatch) = *DISPATCHER.get() {
            dispatch(slot, ptr::null_mut());
        }
    }
    crate::object::zend_object_std_dtor(object);
}

/// The handlers table hosted closures use, built on first use.
unsafe fn closure_handlers() -> *const zend_object_handlers {
    let handlers = CLOSURE_HANDLERS.get().get_or_insert_with(|| {
        let mut handlers = std::ptr::read(&crate::object::std_object_handlers);
        handlers.free_obj = Some(closure_free);
        handlers.clone_obj = None;
        handlers
    });
    handlers as *const zend_object_handlers
}

/// The slot of `zv` when it is a hosted closure.
pub unsafe fn hosted_closure_slot(zv: *mut zval) -> Option<i64> {
    let zv = resolve(zv);
    if type_of(zv) != IS_OBJECT {
        return None;
    }
    closures().get(&((*zv).value.obj as usize)).copied()
}

/// The `zend_function` a hosted closure presents: `{closure}`, variadic.
unsafe fn closure_function() -> *mut zend_function {
    let function = CLOSURE_FUNCTION.get().get_or_insert_with(|| {
        let mut function: Box<zend_internal_function> = Box::new(std::mem::zeroed());
        function.r#type = ZEND_INTERNAL_FUNCTION as u8;
        function.fn_flags = ZEND_ACC_PUBLIC | ZEND_ACC_VARIADIC | ZEND_ACC_CLOSURE;
        function.function_name = string::intern(b"{closure}");
        function
    });
    (&mut **function as *mut zend_internal_function).cast()
}

/// Fills `fci`/`fcc` for a hosted closure, as `zend_fcall_info_init` would for
/// a real one. False when `callable` is not one.
pub unsafe fn init_hosted_fcall(callable: *mut zval, fci: *mut zend_fcall_info, fcc: *mut zend_fcall_info_cache) -> bool {
    if hosted_closure_slot(callable).is_none() {
        return false;
    }
    let object = (*resolve(callable)).value.obj;
    if !fci.is_null() {
        (*fci).size = std::mem::size_of::<zend_fcall_info>();
        set_obj(&mut (*fci).function_name, object);
        (*fci).retval = ptr::null_mut();
        (*fci).params = ptr::null_mut();
        (*fci).object = ptr::null_mut();
        (*fci).param_count = 0;
        (*fci).named_params = ptr::null_mut();
    }
    if !fcc.is_null() {
        (*fcc).function_handler = closure_function();
        (*fcc).calling_scope = ptr::null_mut();
        (*fcc).called_scope = ptr::null_mut();
        (*fcc).object = ptr::null_mut();
        (*fcc).closure = object;
    }
    true
}

/// Registers the wrapper's dispatcher (once, before the first callable is passed).
/// c: hidden
#[no_mangle]
pub unsafe extern "C" fn elephc_php_ext_set_dispatcher(dispatcher: Dispatcher) {
    *DISPATCHER.get() = Some(dispatcher);
}

/// Passes the program callable in `slot` as argument `index`: a `Closure` the
/// extension may call, keep, or drop (its release frees the slot).
/// c: hidden
#[no_mangle]
pub unsafe extern "C" fn elephc_php_ext_call_arg_callable(handle: *mut c_void, index: i64, slot: i64) -> i64 {
    let call = handle.cast::<Call>();
    let arg = frame_arg((*call).frame, index as u32 + 1);
    let object = crate::object::zend_objects_new(crate::class::zend_ce_closure);
    (*object).handlers = closure_handlers();
    closures().insert(object as usize, slot);
    set_obj(arg, object);
    CALL_OK
}

/// Runs a hosted closure for `zend_call_function`. `None` when the callable is
/// not one (the caller refuses it); otherwise SUCCESS, or FAILURE with an
/// exception pending when the program callable threw.
pub unsafe fn call_hosted_closure(fci: *mut zend_fcall_info, fcc: *mut zend_fcall_info_cache) -> Option<zend_result> {
    let slot = hosted_closure_slot(&mut (*fci).function_name).or_else(|| {
        let closure = if fcc.is_null() { ptr::null_mut() } else { (*fcc).closure };
        (!closure.is_null()).then(|| closures().get(&(closure as usize)).copied()).flatten()
    })?;
    let dispatch = (*DISPATCHER.get())?;
    if !(*fci).named_params.is_null() && (*(*fci).named_params).nNumOfElements > 0 {
        crate::error::unsupported("named arguments in a call from a hosted extension to a PHP callable");
    }
    let call = new_call(ptr::null_mut(), ptr::null_mut(), ptr::null_mut(), 0).cast::<Call>();
    let mut callback = Callback { params: (*fci).params, count: (*fci).param_count, result: zval::UNDEF };
    set_null(&mut callback.result);
    (*call).callback = &mut callback;
    let status = dispatch(slot, call.cast());
    (*call).callback = ptr::null_mut();
    if !(*fci).retval.is_null() {
        copy_value((*fci).retval, &callback.result);
    } else {
        zval_ptr_dtor(&mut callback.result);
    }
    elephc_php_ext_call_free(call.cast());
    if status != 0 {
        // The program callable threw: the extension sees an exception and
        // unwinds; the wrapper rethrows the original once it returns.
        crate::exception::throw_error(crate::exception::zend_ce_exception, "a PHP callable threw");
        forget_callback_marker();
        let marker = eg().exception;
        (*marker).gc.refcount += 1;
        *CALLBACK_MARKER.get() = marker;
        return Some(FAILURE);
    }
    Some(SUCCESS)
}

/// The exception standing in for a throwable a program callable threw, while
/// it may still be the one the extension call ends with. The wrapper rethrows
/// the throwable it kept only if this very object is the engine's pending
/// exception when the call returns: an extension that cleared it (and maybe
/// returned normally) or replaced it with its own exception ends the claim.
/// It holds a reference, so its address cannot be reused while recorded.
static CALLBACK_MARKER: Global<*mut zend_object> = Global::new(ptr::null_mut());

/// Set by `capture_exception` when the exception a call ended with was the
/// callback marker; read (and cleared) by `elephc_php_ext_callback_threw`.
static CALLBACK_THREW: Global<bool> = Global::new(false);

/// Drops the callback marker, if any: `zend_clear_exception`, the end of
/// every call, and a newer marker call it.
pub unsafe fn forget_callback_marker() {
    let marker = std::mem::replace(&mut *CALLBACK_MARKER.get(), ptr::null_mut());
    if !marker.is_null() {
        crate::object::zend_object_release(marker);
    }
}

/// The end of an extension call that left no exception pending: no kept
/// throwable belongs to it.
unsafe fn settle_without_exception() {
    forget_callback_marker();
    *CALLBACK_THREW.get() = false;
}

/// 1 (and clears the flag) when the call just captured ended with the
/// exception that stood for a program callable's throwable.
/// c: hidden
#[no_mangle]
pub unsafe extern "C" fn elephc_php_ext_callback_threw() -> i64 {
    let threw = *CALLBACK_THREW.get();
    *CALLBACK_THREW.get() = false;
    i64::from(threw)
}

/// The number of arguments a callback passes.
/// c: hidden
#[no_mangle]
pub unsafe extern "C" fn elephc_php_ext_callback_argc(handle: *mut c_void) -> i64 {
    let callback = (*handle.cast::<Call>()).callback;
    if callback.is_null() { 0 } else { i64::from((*callback).count) }
}

/// Argument `index` of a callback, as an engine zval the wrapper walks.
/// c: hidden
#[no_mangle]
pub unsafe extern "C" fn elephc_php_ext_callback_arg(handle: *mut c_void, index: i64) -> *mut c_void {
    let callback = (*handle.cast::<Call>()).callback;
    if callback.is_null() || index < 0 || index >= i64::from((*callback).count) {
        return ptr::null_mut();
    }
    resolve((*callback).params.add(index as usize)).cast()
}

/// Sets a callback's result from a value Elephc packed. 0, or 3 for a value
/// with no engine counterpart (an object of the program's own classes).
/// c: hidden
#[no_mangle]
pub unsafe extern "C" fn elephc_php_ext_callback_return(handle: *mut c_void, value: *mut c_void) -> i64 {
    let callback = (*handle.cast::<Call>()).callback;
    if callback.is_null() {
        return CALL_UNSUPPORTED;
    }
    zval_ptr_dtor(&mut (*callback).result);
    if !import(&mut (*callback).result, value.cast()) {
        set_null(&mut (*callback).result);
        return CALL_UNSUPPORTED;
    }
    CALL_OK
}

/// Sets a callback's result to a held object (a wrapper the callable returned).
/// c: hidden
#[no_mangle]
pub unsafe extern "C" fn elephc_php_ext_callback_return_object(handle: *mut c_void, object: i64) -> i64 {
    let callback = (*handle.cast::<Call>()).callback;
    let object = held_object(object);
    if callback.is_null() || object.is_null() {
        return CALL_UNSUPPORTED;
    }
    zval_ptr_dtor(&mut (*callback).result);
    (*object).gc.refcount += 1;
    set_obj(&mut (*callback).result, object);
    CALL_OK
}

// ------------------------------------------------------------ leak accounting

/// How many entries the engine still tracks for the program, by kind: 0 held
/// objects (wrapper references), 1 iterations in progress, 2 program callables
/// an extension holds. Leak tests expect each back at its starting value.
/// c: hidden
#[no_mangle]
pub unsafe extern "C" fn elephc_php_ext_live_entries(kind: i64) -> i64 {
    let count = match kind {
        0 => held().values().map(|entry| entry.1 as usize).sum(),
        1 => iterations().len(),
        _ => closures().len(),
    };
    count as i64
}
