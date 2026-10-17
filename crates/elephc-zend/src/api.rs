//! Purpose:
//! The rest of the Zend API extensions call: the active-frame queries, the
//! array and property builders (`add_assoc_*`, `add_next_index_*`,
//! `add_property_*`), resources, calls between internal functions, and the
//! callable API (which a compiled program cannot serve).
//!
//! Called from:
//! - Extension code, and `crate::host` (which runs every call through
//!   `call_internal`).
//!
//! Key details:
//! - Calling an internal function builds a PHP-shaped frame (the
//!   `zend_execute_data` followed by its argument zvals) and makes it
//!   `EG(current_execute_data)`, so the callee's argument parsing, `getThis()`
//!   and error messages see what they would under PHP.
//! - PHP callables (closures, `[$obj, 'm']`, function names of PHP code) have
//!   no meaning to a hosted extension: calling one stops the program with
//!   the API's name.

use std::ffi::{c_char, c_int, c_void, CStr};
use std::ptr;

use crate::abi::*;
use crate::alloc::{alloc_zeroed, free};
use crate::global::Global;
use crate::hash;
use crate::startup::eg;
use crate::string::{self, intern};
use crate::zval::*;

// ------------------------------------------------------------ the active frame

/// Returns the function being executed in the given frame, or null when the frame is null.
#[no_mangle]
pub unsafe extern "C" fn zend_active_function_ex(execute_data: *mut zend_execute_data) -> *mut zend_function {
    if execute_data.is_null() {
        ptr::null_mut()
    } else {
        (*execute_data).func
    }
}

/// Returns the function of the current frame (`EG(current_execute_data)`), or null outside any
/// call.
#[no_mangle]
pub unsafe extern "C" fn zend_active_function() -> *mut zend_function {
    zend_active_function_ex(eg().current_execute_data)
}

/// Returns the active function's name as a C string, or null when there is no active named
/// function.
#[no_mangle]
pub unsafe extern "C" fn get_active_function_name() -> *const c_char {
    let function = zend_active_function();
    if function.is_null() || (*function).common.function_name.is_null() {
        return ptr::null();
    }
    (*(*function).common.function_name).val.as_ptr()
}

/// Returns the active function's class name (empty string without a scope) and, through `space`,
/// the separator to print after it (`"::"` or `""`).
#[no_mangle]
pub unsafe extern "C" fn get_active_class_name(space: *mut *const c_char) -> *const c_char {
    let function = zend_active_function();
    let scope = if function.is_null() { ptr::null_mut() } else { (*function).common.scope };
    if !space.is_null() {
        *space = if scope.is_null() { c"".as_ptr() } else { c"::".as_ptr() };
    }
    if scope.is_null() {
        c"".as_ptr()
    } else {
        (*(*scope).name).val.as_ptr()
    }
}

/// Returns an owned `Class::method` name for a function with a scope, a copy of the plain name
/// otherwise, and the interned string `main` when the function is null or unnamed.
#[no_mangle]
pub unsafe extern "C" fn get_function_or_method_name(function: *const zend_function) -> *mut zend_string {
    if function.is_null() || (*function).common.function_name.is_null() {
        return intern(b"main");
    }
    let name = (*function).common.function_name;
    let scope = (*function).common.scope;
    if scope.is_null() {
        string::zend_string_copy(name)
    } else {
        zend_create_member_string((*scope).name, name)
    }
}

/// Returns the `Class::method` (or plain) name of the active function as an owned `zend_string`.
#[no_mangle]
pub unsafe extern "C" fn get_active_function_or_method_name() -> *mut zend_string {
    get_function_or_method_name(zend_active_function())
}

/// Returns the declared name of 1-based argument `arg_num` of an internal function from its arg
/// info (counting the variadic slot), or null when out of range or absent.
#[no_mangle]
pub unsafe extern "C" fn get_function_arg_name(function: *const zend_function, arg_num: u32) -> *const c_char {
    if function.is_null() || arg_num == 0 {
        return ptr::null();
    }
    let internal = ptr::addr_of!((*function).internal_function);
    let mut count = (*internal).num_args;
    if (*internal).fn_flags & ZEND_ACC_VARIADIC != 0 {
        count += 1;
    }
    if arg_num > count || (*internal).arg_info.is_null() {
        return ptr::null();
    }
    (*(*internal).arg_info.add(arg_num as usize - 1)).name
}

/// Argument name lookup for the currently active function; see `get_function_arg_name`.
#[no_mangle]
pub unsafe extern "C" fn get_active_function_arg_name(arg_num: u32) -> *const c_char {
    get_function_arg_name(zend_active_function(), arg_num)
}

/// Builds a new `Class::member` string from two `zend_string`s.
#[no_mangle]
pub unsafe extern "C" fn zend_create_member_string(class_name: *mut zend_string, member_name: *mut zend_string) -> *mut zend_string {
    let mut text = string::bytes(class_name).to_vec();
    text.extend_from_slice(b"::");
    text.extend_from_slice(string::bytes(member_name));
    string::new(&text)
}

/// A compiled program has no PHP file or line of its own to report.
#[no_mangle]
pub unsafe extern "C" fn zend_get_executed_filename_ex() -> *mut zend_string {
    eg().filename_override
}

/// Returns the executing file name as a C string, or `"[no active file]"` when none is set (a
/// compiled program has no PHP file of its own).
#[no_mangle]
pub unsafe extern "C" fn zend_get_executed_filename() -> *const c_char {
    let name = zend_get_executed_filename_ex();
    if name.is_null() {
        c"[no active file]".as_ptr()
    } else {
        (*name).val.as_ptr()
    }
}

/// Returns the overridden executing line number, or 0 when none is set.
#[no_mangle]
pub unsafe extern "C" fn zend_get_executed_lineno() -> u32 {
    if eg().lineno_override > 0 {
        eg().lineno_override as u32
    } else {
        0
    }
}

/// Returns the class scope of the active function, or null.
#[no_mangle]
pub unsafe extern "C" fn zend_get_executed_scope() -> *mut zend_class_entry {
    let function = zend_active_function();
    if function.is_null() {
        ptr::null_mut()
    } else {
        (*function).common.scope
    }
}

/// Returns the class a frame was called on: the object's class when `$this` is set, otherwise the
/// frame's static scope, falling back to the function's own scope; null for a null frame or
/// function.
#[no_mangle]
pub unsafe extern "C" fn zend_get_called_scope(ex: *mut zend_execute_data) -> *mut zend_class_entry {
    if ex.is_null() || (*ex).func.is_null() {
        return ptr::null_mut();
    }
    if type_of(&(*ex).This) == IS_OBJECT {
        return (*(*ex).This.value.obj).ce;
    }
    let ce = (*ex).This.value.ce;
    if ce.is_null() {
        (*(*ex).func).common.scope
    } else {
        ce
    }
}

/// Returns the `$this` object of a frame, or null for a static or null frame.
#[no_mangle]
pub unsafe extern "C" fn zend_get_this_object(ex: *mut zend_execute_data) -> *mut zend_object {
    if !ex.is_null() && type_of(&(*ex).This) == IS_OBJECT {
        (*ex).This.value.obj
    } else {
        ptr::null_mut()
    }
}

/// True while an internal call frame is active (`EG(current_execute_data)` is non-null).
#[no_mangle]
pub unsafe extern "C" fn zend_is_executing() -> bool {
    !eg().current_execute_data.is_null()
}

/// Copies the first `param_count` arguments of the current frame into `argument_array` (taking
/// references); `FAILURE` when there is no frame or fewer arguments were passed.
#[no_mangle]
pub unsafe extern "C" fn zend_get_parameters_array_ex(param_count: u32, argument_array: *mut zval) -> zend_result {
    let frame = eg().current_execute_data;
    if frame.is_null() || (*frame).This.u2.num_args < param_count {
        return FAILURE;
    }
    let args = frame_arg(frame, 1);
    for i in 0..param_count as usize {
        copy_value(argument_array.add(i), args.add(i));
    }
    SUCCESS
}

// ------------------------------------------------------------ calls between internal functions

/// Argument `n` (1-based) of a frame.
pub unsafe fn frame_arg(frame: *mut zend_execute_data, n: u32) -> *mut zval {
    let slot = (std::mem::size_of::<zend_execute_data>() + 15) / 16;
    (frame as *mut zval).add(slot + n as usize - 1)
}

/// A frame for `argc` arguments, zeroed, not yet linked.
pub unsafe fn new_frame(function: *mut zend_function, argc: u32) -> *mut zend_execute_data {
    let slot = (std::mem::size_of::<zend_execute_data>() + 15) / 16;
    let frame = alloc_zeroed((slot + argc.max(1) as usize) * std::mem::size_of::<zval>()).cast::<zend_execute_data>();
    (*frame).func = function;
    (*frame).This.u2.num_args = argc;
    frame
}

/// Runs an internal function on a prepared frame, the frame current while it
/// runs. `return_value` must be initialized (to NULL) by the caller.
pub unsafe fn run(frame: *mut zend_execute_data, return_value: *mut zval) {
    let function = (*frame).func;
    (*frame).prev_execute_data = eg().current_execute_data;
    (*frame).return_value = return_value;
    eg().current_execute_data = frame;
    if let Some(handler) = (*function).internal_function.handler {
        handler(frame, return_value);
    }
    eg().current_execute_data = (*frame).prev_execute_data;
}

/// Calls an internal function (an extension's function or method) with
/// `params`, which keep their references.
#[no_mangle]
pub unsafe extern "C" fn zend_call_known_function(
    function: *mut zend_function,
    object: *mut zend_object,
    called_scope: *mut zend_class_entry,
    retval_ptr: *mut zval,
    param_count: u32,
    params: *mut zval,
    _named_params: *mut HashTable,
) {
    if function.is_null() {
        return;
    }
    if (*function).r#type != ZEND_INTERNAL_FUNCTION {
        crate::error::unsupported("zend_call_known_function (calling back into PHP code)");
    }
    let frame = new_frame(function, param_count);
    for i in 0..param_count {
        copy(frame_arg(frame, i + 1), params.add(i as usize));
    }
    if !object.is_null() {
        set_obj(&mut (*frame).This, object);
        (*frame).This.u2.num_args = param_count;
    } else {
        (*frame).This.value.ce = called_scope;
    }
    let mut result = null();
    run(frame, &mut result);
    for i in 0..param_count {
        zval_ptr_dtor(frame_arg(frame, i + 1));
    }
    free(frame.cast());
    if retval_ptr.is_null() {
        zval_ptr_dtor(&mut result);
    } else {
        copy_value(retval_ptr, &result);
    }
}

/// Calls an internal method on `object` with one parameter through `zend_call_known_function`.
#[no_mangle]
pub unsafe extern "C" fn zend_call_known_instance_method_with_1_params(
    function: *mut zend_function,
    object: *mut zend_object,
    retval_ptr: *mut zval,
    param: *mut zval,
) {
    zend_call_known_function(function, object, (*object).ce, retval_ptr, 1, param, ptr::null_mut());
}

/// Calls an internal method on `object` with two parameters (copied into a temporary array) through
/// `zend_call_known_function`.
#[no_mangle]
pub unsafe extern "C" fn zend_call_known_instance_method_with_2_params(
    function: *mut zend_function,
    object: *mut zend_object,
    retval_ptr: *mut zval,
    param1: *mut zval,
    param2: *mut zval,
) {
    let mut params = [zval::UNDEF; 2];
    copy_value(&mut params[0], param1);
    copy_value(&mut params[1], param2);
    zend_call_known_function(function, object, (*object).ce, retval_ptr, 2, params.as_mut_ptr(), ptr::null_mut());
}

/// Calls a method by name on an object of an extension's class.
#[no_mangle]
pub unsafe extern "C" fn zend_call_method(
    object: *mut zend_object,
    obj_ce: *mut zend_class_entry,
    fn_proxy: *mut *mut zend_function,
    function_name: *const c_char,
    function_name_len: usize,
    retval: *mut zval,
    param_count: u32,
    arg1: *mut zval,
    arg2: *mut zval,
) -> *mut zval {
    let ce = if obj_ce.is_null() { (*object).ce } else { obj_ce };
    let mut function = if fn_proxy.is_null() { ptr::null_mut() } else { *fn_proxy };
    if function.is_null() {
        function = hash::zend_hash_str_find_ptr_lc(&(*ce).function_table, function_name, function_name_len).cast();
        if function.is_null() {
            let name = String::from_utf8_lossy(std::slice::from_raw_parts(function_name.cast::<u8>(), function_name_len));
            crate::exception::throw_error(
                crate::exception::zend_ce_error,
                &format!("Call to undefined method {}::{name}()", string::text((*ce).name)),
            );
            return ptr::null_mut();
        }
        if !fn_proxy.is_null() {
            *fn_proxy = function;
        }
    }
    let mut params = [zval::UNDEF; 2];
    if param_count > 0 {
        copy_value(&mut params[0], arg1);
    }
    if param_count > 1 {
        copy_value(&mut params[1], arg2);
    }
    let mut result = null();
    zend_call_known_function(function, object, ce, &mut result, param_count, params.as_mut_ptr(), ptr::null_mut());
    if retval.is_null() {
        zval_ptr_dtor(&mut result);
        ptr::null_mut()
    } else {
        copy_value(retval, &result);
        retval
    }
}

// ------------------------------------------------------------ callables (not available)

// SAFETY: never written; they exist for extensions to copy from.
unsafe impl Sync for zend_fcall_info {}
unsafe impl Sync for zend_fcall_info_cache {}

#[no_mangle]
pub static empty_fcall_info: zend_fcall_info = unsafe { std::mem::zeroed() };
#[no_mangle]
pub static empty_fcall_info_cache: zend_fcall_info_cache = unsafe { std::mem::zeroed() };

/// Calls a program callable the extension was handed (a hosted `Closure`):
/// SUCCESS with `fci.retval` set, or FAILURE with an exception pending when it
/// threw. Any other callable stops the program with an unsupported-API fatal
/// error, since there is no PHP code to run.
#[no_mangle]
pub unsafe extern "C" fn zend_call_function(fci: *mut zend_fcall_info, fci_cache: *mut zend_fcall_info_cache) -> zend_result {
    match crate::host::call_hosted_closure(fci, fci_cache) {
        Some(result) => result,
        None => crate::error::unsupported("zend_call_function (calling back into PHP code)"),
    }
}

/// Unsupported: `call_user_function` cannot run PHP callables here and stops the program with an
/// unsupported-API fatal error.
#[no_mangle]
pub unsafe extern "C" fn _call_user_function_impl(
    _object: *mut zval,
    _function_name: *mut zval,
    _retval_ptr: *mut zval,
    _param_count: u32,
    _params: *mut zval,
    _named_params: *mut HashTable,
) -> zend_result {
    crate::error::unsupported("call_user_function (calling back into PHP code)")
}

/// Fills the info and cache structs for a program callable (a hosted
/// `Closure`) and returns SUCCESS. Anything else fails: the structs are
/// cleared, `*error` gets an explanatory message (allocated with `estrdup`).
#[no_mangle]
pub unsafe extern "C" fn zend_fcall_info_init(
    callable: *mut zval,
    _check_flags: u32,
    fci: *mut zend_fcall_info,
    fcc: *mut zend_fcall_info_cache,
    _callable_name: *mut *mut zend_string,
    error: *mut *mut c_char,
) -> zend_result {
    if crate::host::init_hosted_fcall(callable, fci, fcc) {
        if !error.is_null() {
            *error = ptr::null_mut();
        }
        return SUCCESS;
    }
    if !fci.is_null() {
        (*fci).size = 0;
    }
    if !fcc.is_null() {
        (*fcc).function_handler = ptr::null_mut();
    }
    if !error.is_null() {
        *error = crate::alloc::estrdup(c"a hosted PHP extension cannot call back into compiled PHP code yet".as_ptr());
    }
    FAILURE
}

/// No-op: argument arrays are never attached to an fcall info.
#[no_mangle]
pub unsafe extern "C" fn zend_fcall_info_args_clear(_fci: *mut zend_fcall_info, _free_mem: bool) {}

/// Always returns `FAILURE`; fcall infos cannot carry arguments here.
#[no_mangle]
pub unsafe extern "C" fn zend_fcall_info_args(_fci: *mut zend_fcall_info, _args: *mut zval) -> zend_result {
    FAILURE
}

/// No-op: a call cache holds no references that need releasing.
#[no_mangle]
pub unsafe extern "C" fn zend_release_fcall_info_cache(_fcc: *mut zend_fcall_info_cache) {}

/// True for a program callable (a hosted `Closure`, named `Closure::__invoke`
/// as PHP names closures); false, with an empty name, for anything else.
#[no_mangle]
pub unsafe extern "C" fn zend_is_callable(callable: *mut zval, _check_flags: u32, callable_name: *mut *mut zend_string) -> bool {
    let hosted = crate::host::hosted_closure_slot(callable).is_some();
    if !callable_name.is_null() {
        *callable_name = if hosted { crate::string::new(b"Closure::__invoke") } else { intern(b"") };
    }
    hosted
}

/// Always false; clears `*error` and defers to `zend_is_callable`.
#[no_mangle]
pub unsafe extern "C" fn zend_is_callable_ex(
    callable: *mut zval,
    _object: *mut zend_object,
    check_flags: u32,
    callable_name: *mut *mut zend_string,
    _fcc: *mut zend_fcall_info_cache,
    error: *mut *mut c_char,
) -> bool {
    if !error.is_null() {
        *error = ptr::null_mut();
    }
    zend_is_callable(callable, check_flags, callable_name)
}

/// `Closure::__invoke` for a program callable, the empty string otherwise.
#[no_mangle]
pub unsafe extern "C" fn zend_get_callable_name(callable: *mut zval) -> *mut zend_string {
    if crate::host::hosted_closure_slot(callable).is_some() {
        crate::string::new(b"Closure::__invoke")
    } else {
        intern(b"")
    }
}

/// Initializes the embedded standard object of an iterator and resets its index to 0.
#[no_mangle]
pub unsafe extern "C" fn zend_iterator_init(iter: *mut zend_object_iterator) {
    crate::object::zend_object_std_init(&mut (*iter).std, ptr::null_mut());
    (*iter).index = 0;
}

/// Drops one reference to an iterator; the last runs its `dtor` callback (which
/// releases what the extension put in `data`) and frees the iterator, as PHP's
/// object store does for the wrapper object an iterator is.
#[no_mangle]
pub unsafe extern "C" fn zend_iterator_dtor(iter: *mut zend_object_iterator) {
    (*iter).std.gc.refcount = (*iter).std.gc.refcount.saturating_sub(1);
    if (*iter).std.gc.refcount > 0 {
        return;
    }
    if let Some(dtor) = (*(*iter).funcs).dtor {
        dtor(iter);
    }
    crate::alloc::free(iter.cast());
}

/// Unsupported: iterating a PHP object stops the program with an unsupported-API fatal error.
#[no_mangle]
pub unsafe extern "C" fn zend_user_it_get_new_iterator(_ce: *mut zend_class_entry, _object: *mut zval, _by_ref: c_int) -> *mut zend_object_iterator {
    crate::error::unsupported("zend_user_it_get_new_iterator (iterating a PHP object)")
}

/// Unsupported: iterating a PHP object stops the program with an unsupported-API fatal error.
#[no_mangle]
pub unsafe extern "C" fn spl_iterator_apply(_obj: *mut zval, _apply_func: spl_iterator_apply_func_t, _puser: *mut c_void) -> zend_result {
    crate::error::unsupported("spl_iterator_apply (iterating a PHP object)")
}

// ------------------------------------------------------------ arrays built by extensions

/// Stores `value` under a string key of the array in `arg` with symbol-table rules (a numeric
/// string becomes an integer key); the table takes over the value's reference.
unsafe fn assoc(arg: *mut zval, key: *const c_char, key_len: usize, mut value: zval) {
    hash::zend_symtable_str_update((*arg).value.arr, key, key_len, &mut value);
}

/// Sets `arg[key] = n` for an integer, as the PHP macro/function of the same name.
#[no_mangle]
pub unsafe extern "C" fn add_assoc_long_ex(arg: *mut zval, key: *const c_char, key_len: usize, n: zend_long) {
    assoc(arg, key, key_len, long(n));
}

/// Sets `arg[key] = null`.
#[no_mangle]
pub unsafe extern "C" fn add_assoc_null_ex(arg: *mut zval, key: *const c_char, key_len: usize) {
    assoc(arg, key, key_len, null());
}

/// Sets `arg[key]` to a boolean.
#[no_mangle]
pub unsafe extern "C" fn add_assoc_bool_ex(arg: *mut zval, key: *const c_char, key_len: usize, b: bool) {
    let mut zv = zval::UNDEF;
    set_bool(&mut zv, b);
    assoc(arg, key, key_len, zv);
}

/// Sets `arg[key]` to a resource (the array takes the caller's reference).
#[no_mangle]
pub unsafe extern "C" fn add_assoc_resource_ex(arg: *mut zval, key: *const c_char, key_len: usize, r: *mut zend_resource) {
    let mut zv = zval::UNDEF;
    set_res(&mut zv, r);
    assoc(arg, key, key_len, zv);
}

/// Sets `arg[key]` to a float.
#[no_mangle]
pub unsafe extern "C" fn add_assoc_double_ex(arg: *mut zval, key: *const c_char, key_len: usize, d: f64) {
    let mut zv = zval::UNDEF;
    set_double(&mut zv, d);
    assoc(arg, key, key_len, zv);
}

/// Sets `arg[key]` to the given `zend_string` (the array takes the caller's reference).
#[no_mangle]
pub unsafe extern "C" fn add_assoc_str_ex(arg: *mut zval, key: *const c_char, key_len: usize, str: *mut zend_string) {
    let mut zv = zval::UNDEF;
    set_str(&mut zv, str);
    assoc(arg, key, key_len, zv);
}

/// Sets `arg[key]` to a copy of a NUL-terminated C string.
#[no_mangle]
pub unsafe extern "C" fn add_assoc_string_ex(arg: *mut zval, key: *const c_char, key_len: usize, str: *const c_char) {
    let mut zv = zval::UNDEF;
    set_bytes(&mut zv, CStr::from_ptr(str).to_bytes());
    assoc(arg, key, key_len, zv);
}

/// Sets `arg[key]` to a copy of the first `length` bytes of `str`.
#[no_mangle]
pub unsafe extern "C" fn add_assoc_stringl_ex(arg: *mut zval, key: *const c_char, key_len: usize, str: *const c_char, length: usize) {
    let mut zv = zval::UNDEF;
    set_str(&mut zv, string::zend_string_init(str, length, false));
    assoc(arg, key, key_len, zv);
}

/// Sets `arg[key]` to a nested array (the array takes the caller's reference).
#[no_mangle]
pub unsafe extern "C" fn add_assoc_array_ex(arg: *mut zval, key: *const c_char, key_len: usize, arr: *mut zend_array) {
    let mut zv = zval::UNDEF;
    set_arr(&mut zv, arr);
    assoc(arg, key, key_len, zv);
}

/// Sets `arg[key]` to an object (the array takes the caller's reference).
#[no_mangle]
pub unsafe extern "C" fn add_assoc_object_ex(arg: *mut zval, key: *const c_char, key_len: usize, obj: *mut zend_object) {
    let mut zv = zval::UNDEF;
    set_obj(&mut zv, obj);
    assoc(arg, key, key_len, zv);
}

/// Sets `arg[key]` to a PHP reference (the array takes the caller's reference).
#[no_mangle]
pub unsafe extern "C" fn add_assoc_reference_ex(arg: *mut zval, key: *const c_char, key_len: usize, reference: *mut zend_reference) {
    let mut zv = zval::UNDEF;
    zv.value.r#ref = reference;
    zv.u1.type_info = IS_REFERENCE_EX;
    assoc(arg, key, key_len, zv);
}

/// Sets `arg[key]` to a copy of the zval `value` (reference-counted).
#[no_mangle]
pub unsafe extern "C" fn add_assoc_zval_ex(arg: *mut zval, key: *const c_char, key_len: usize, value: *mut zval) {
    let mut zv = zval::UNDEF;
    copy_value(&mut zv, value);
    assoc(arg, key, key_len, zv);
}

/// Stores `value` under integer key `idx` of the array in `arg`, replacing any existing entry.
unsafe fn index(arg: *mut zval, idx: zend_ulong, mut value: zval) {
    hash::zend_hash_index_update((*arg).value.arr, idx, &mut value);
}

/// Sets `arg[idx] = n` for an integer.
#[no_mangle]
pub unsafe extern "C" fn add_index_long(arg: *mut zval, idx: zend_ulong, n: zend_long) {
    index(arg, idx, long(n));
}

/// Sets `arg[idx] = null`.
#[no_mangle]
pub unsafe extern "C" fn add_index_null(arg: *mut zval, idx: zend_ulong) {
    index(arg, idx, null());
}

/// Sets `arg[idx]` to a boolean.
#[no_mangle]
pub unsafe extern "C" fn add_index_bool(arg: *mut zval, idx: zend_ulong, b: bool) {
    let mut zv = zval::UNDEF;
    set_bool(&mut zv, b);
    index(arg, idx, zv);
}

/// Sets `arg[idx]` to a resource.
#[no_mangle]
pub unsafe extern "C" fn add_index_resource(arg: *mut zval, idx: zend_ulong, r: *mut zend_resource) {
    let mut zv = zval::UNDEF;
    set_res(&mut zv, r);
    index(arg, idx, zv);
}

/// Sets `arg[idx]` to a float.
#[no_mangle]
pub unsafe extern "C" fn add_index_double(arg: *mut zval, idx: zend_ulong, d: f64) {
    let mut zv = zval::UNDEF;
    set_double(&mut zv, d);
    index(arg, idx, zv);
}

/// Sets `arg[idx]` to the given `zend_string` (the array takes the caller's reference).
#[no_mangle]
pub unsafe extern "C" fn add_index_str(arg: *mut zval, idx: zend_ulong, str: *mut zend_string) {
    let mut zv = zval::UNDEF;
    set_str(&mut zv, str);
    index(arg, idx, zv);
}

/// Sets `arg[idx]` to a copy of a NUL-terminated C string.
#[no_mangle]
pub unsafe extern "C" fn add_index_string(arg: *mut zval, idx: zend_ulong, str: *const c_char) {
    let mut zv = zval::UNDEF;
    set_bytes(&mut zv, CStr::from_ptr(str).to_bytes());
    index(arg, idx, zv);
}

/// Sets `arg[idx]` to a copy of the first `length` bytes of `str`.
#[no_mangle]
pub unsafe extern "C" fn add_index_stringl(arg: *mut zval, idx: zend_ulong, str: *const c_char, length: usize) {
    let mut zv = zval::UNDEF;
    set_str(&mut zv, string::zend_string_init(str, length, false));
    index(arg, idx, zv);
}

/// Sets `arg[idx]` to a nested array.
#[no_mangle]
pub unsafe extern "C" fn add_index_array(arg: *mut zval, idx: zend_ulong, arr: *mut zend_array) {
    let mut zv = zval::UNDEF;
    set_arr(&mut zv, arr);
    index(arg, idx, zv);
}

/// Sets `arg[idx]` to an object.
#[no_mangle]
pub unsafe extern "C" fn add_index_object(arg: *mut zval, idx: zend_ulong, obj: *mut zend_object) {
    let mut zv = zval::UNDEF;
    set_obj(&mut zv, obj);
    index(arg, idx, zv);
}

/// Sets `arg[idx]` to a PHP reference.
#[no_mangle]
pub unsafe extern "C" fn add_index_reference(arg: *mut zval, idx: zend_ulong, reference: *mut zend_reference) {
    let mut zv = zval::UNDEF;
    zv.value.r#ref = reference;
    zv.u1.type_info = IS_REFERENCE_EX;
    index(arg, idx, zv);
}

/// Sets `arg[idx]` to the given zval (taking its reference); returns `FAILURE` if the insert fails.
#[no_mangle]
pub unsafe extern "C" fn add_index_zval(arg: *mut zval, idx: zend_ulong, value: *mut zval) -> zend_result {
    if hash::zend_hash_index_update((*arg).value.arr, idx, value).is_null() {
        FAILURE
    } else {
        SUCCESS
    }
}

/// Appends `value` to the array in `arg` (`$a[] = ...`); if the insert fails, the value is
/// destroyed and `FAILURE` is returned.
unsafe fn next(arg: *mut zval, mut value: zval) -> zend_result {
    if hash::zend_hash_next_index_insert((*arg).value.arr, &mut value).is_null() {
        zval_ptr_dtor(&mut value);
        FAILURE
    } else {
        SUCCESS
    }
}

/// Appends an integer to the array.
#[no_mangle]
pub unsafe extern "C" fn add_next_index_long(arg: *mut zval, n: zend_long) -> zend_result {
    next(arg, long(n))
}

/// Appends `null` to the array.
#[no_mangle]
pub unsafe extern "C" fn add_next_index_null(arg: *mut zval) -> zend_result {
    next(arg, null())
}

/// Appends a boolean to the array.
#[no_mangle]
pub unsafe extern "C" fn add_next_index_bool(arg: *mut zval, b: bool) -> zend_result {
    let mut zv = zval::UNDEF;
    set_bool(&mut zv, b);
    next(arg, zv)
}

/// Appends a resource to the array.
#[no_mangle]
pub unsafe extern "C" fn add_next_index_resource(arg: *mut zval, r: *mut zend_resource) -> zend_result {
    let mut zv = zval::UNDEF;
    set_res(&mut zv, r);
    next(arg, zv)
}

/// Appends a float to the array.
#[no_mangle]
pub unsafe extern "C" fn add_next_index_double(arg: *mut zval, d: f64) -> zend_result {
    let mut zv = zval::UNDEF;
    set_double(&mut zv, d);
    next(arg, zv)
}

/// Appends the given `zend_string` (taking the caller's reference).
#[no_mangle]
pub unsafe extern "C" fn add_next_index_str(arg: *mut zval, str: *mut zend_string) -> zend_result {
    let mut zv = zval::UNDEF;
    set_str(&mut zv, str);
    next(arg, zv)
}

/// Appends a copy of a NUL-terminated C string.
#[no_mangle]
pub unsafe extern "C" fn add_next_index_string(arg: *mut zval, str: *const c_char) -> zend_result {
    let mut zv = zval::UNDEF;
    set_bytes(&mut zv, CStr::from_ptr(str).to_bytes());
    next(arg, zv)
}

/// Appends a copy of the first `length` bytes of `str`.
#[no_mangle]
pub unsafe extern "C" fn add_next_index_stringl(arg: *mut zval, str: *const c_char, length: usize) -> zend_result {
    let mut zv = zval::UNDEF;
    set_str(&mut zv, string::zend_string_init(str, length, false));
    next(arg, zv)
}

/// Appends a nested array.
#[no_mangle]
pub unsafe extern "C" fn add_next_index_array(arg: *mut zval, arr: *mut zend_array) -> zend_result {
    let mut zv = zval::UNDEF;
    set_arr(&mut zv, arr);
    next(arg, zv)
}

/// Appends an object.
#[no_mangle]
pub unsafe extern "C" fn add_next_index_object(arg: *mut zval, obj: *mut zend_object) -> zend_result {
    let mut zv = zval::UNDEF;
    set_obj(&mut zv, obj);
    next(arg, zv)
}

/// Appends a PHP reference.
#[no_mangle]
pub unsafe extern "C" fn add_next_index_reference(arg: *mut zval, reference: *mut zend_reference) -> zend_result {
    let mut zv = zval::UNDEF;
    zv.value.r#ref = reference;
    zv.u1.type_info = IS_REFERENCE_EX;
    next(arg, zv)
}

/// Appends the given zval (taking its reference); `FAILURE` if the insert fails.
#[no_mangle]
pub unsafe extern "C" fn add_next_index_zval(arg: *mut zval, value: *mut zval) -> zend_result {
    if hash::zend_hash_next_index_insert((*arg).value.arr, value).is_null() {
        FAILURE
    } else {
        SUCCESS
    }
}

/// Sets `arg[idx] = l` and stores the new value slot in `*dest` when `dest` is non-null; `FAILURE`
/// if the insert fails.
#[no_mangle]
pub unsafe extern "C" fn add_get_index_long(arg: *mut zval, idx: zend_ulong, l: zend_long, dest: *mut *mut c_void) -> zend_result {
    let mut zv = long(l);
    let slot = hash::zend_hash_index_update((*arg).value.arr, idx, &mut zv);
    if !dest.is_null() {
        *dest = slot.cast();
    }
    if slot.is_null() {
        FAILURE
    } else {
        SUCCESS
    }
}

/// `$ht[$key] = $value` with PHP's key rules (takes `value`'s reference).
#[no_mangle]
pub unsafe extern "C" fn array_set_zval_key(ht: *mut HashTable, key: *mut zval, value: *mut zval) -> zend_result {
    let key = deref(key);
    let result = match type_of(key) {
        IS_STRING => hash::zend_symtable_update(ht, (*key).value.str, value),
        IS_NULL => hash::zend_hash_update(ht, string::zend_empty_string, value),
        IS_RESOURCE => hash::zend_hash_index_update(ht, (*(*key).value.res).handle as zend_ulong, value),
        IS_FALSE => hash::zend_hash_index_update(ht, 0, value),
        IS_TRUE => hash::zend_hash_index_update(ht, 1, value),
        IS_LONG => hash::zend_hash_index_update(ht, (*key).value.lval as zend_ulong, value),
        IS_DOUBLE => hash::zend_hash_index_update(ht, crate::convert::zend_dval_to_lval((*key).value.dval) as zend_ulong, value),
        _ => {
            crate::exception::throw_error(crate::exception::zend_ce_type_error, "Illegal offset type");
            ptr::null_mut()
        }
    };
    if result.is_null() {
        FAILURE
    } else {
        if is_refcounted(result) {
            (*(*result).value.counted).gc.refcount += 1;
        }
        SUCCESS
    }
}

// ------------------------------------------------------------ properties set by extensions

/// Sets a property of the object in `arg` through `zend_update_property` with no scope, then drops
/// the temporary value's own reference.
unsafe fn property(arg: *mut zval, key: *const c_char, key_len: usize, mut value: zval) {
    crate::object::zend_update_property(ptr::null_mut(), (*arg).value.obj, key, key_len, &mut value);
    zval_ptr_dtor(&mut value);
}

/// Sets an integer property on an object.
#[no_mangle]
pub unsafe extern "C" fn add_property_long_ex(arg: *mut zval, key: *const c_char, key_len: usize, l: zend_long) {
    property(arg, key, key_len, long(l));
}

/// Sets a property to `null`.
#[no_mangle]
pub unsafe extern "C" fn add_property_null_ex(arg: *mut zval, key: *const c_char, key_len: usize) {
    property(arg, key, key_len, null());
}

/// Sets a boolean property (any non-zero `b` is true).
#[no_mangle]
pub unsafe extern "C" fn add_property_bool_ex(arg: *mut zval, key: *const c_char, key_len: usize, b: zend_long) {
    let mut zv = zval::UNDEF;
    set_bool(&mut zv, b != 0);
    property(arg, key, key_len, zv);
}

/// Sets a property to a resource.
#[no_mangle]
pub unsafe extern "C" fn add_property_resource_ex(arg: *mut zval, key: *const c_char, key_len: usize, r: *mut zend_resource) {
    let mut zv = zval::UNDEF;
    set_res(&mut zv, r);
    property(arg, key, key_len, zv);
}

/// Sets a float property.
#[no_mangle]
pub unsafe extern "C" fn add_property_double_ex(arg: *mut zval, key: *const c_char, key_len: usize, d: f64) {
    let mut zv = zval::UNDEF;
    set_double(&mut zv, d);
    property(arg, key, key_len, zv);
}

/// Sets a string property to the given `zend_string`.
#[no_mangle]
pub unsafe extern "C" fn add_property_str_ex(arg: *mut zval, key: *const c_char, key_len: usize, str: *mut zend_string) {
    let mut zv = zval::UNDEF;
    set_str(&mut zv, str);
    property(arg, key, key_len, zv);
}

/// Sets a string property to a copy of a NUL-terminated C string.
#[no_mangle]
pub unsafe extern "C" fn add_property_string_ex(arg: *mut zval, key: *const c_char, key_len: usize, str: *const c_char) {
    let mut zv = zval::UNDEF;
    set_bytes(&mut zv, CStr::from_ptr(str).to_bytes());
    property(arg, key, key_len, zv);
}

/// Sets a string property to a copy of the first `length` bytes of `str`.
#[no_mangle]
pub unsafe extern "C" fn add_property_stringl_ex(arg: *mut zval, key: *const c_char, key_len: usize, str: *const c_char, length: usize) {
    let mut zv = zval::UNDEF;
    set_str(&mut zv, string::zend_string_init(str, length, false));
    property(arg, key, key_len, zv);
}

/// Sets a property to `value`, which keeps its reference (the property takes
/// its own).
#[no_mangle]
pub unsafe extern "C" fn add_property_zval_ex(arg: *mut zval, key: *const c_char, key_len: usize, value: *mut zval) {
    crate::object::zend_update_property(ptr::null_mut(), (*arg).value.obj, key, key_len, value);
}

/// Sets a property to an array.
#[no_mangle]
pub unsafe extern "C" fn add_property_array_ex(arg: *mut zval, key: *const c_char, key_len: usize, arr: *mut zend_array) {
    let mut zv = zval::UNDEF;
    set_arr(&mut zv, arr);
    property(arg, key, key_len, zv);
}

/// Sets a property to an object.
#[no_mangle]
pub unsafe extern "C" fn add_property_object_ex(arg: *mut zval, key: *const c_char, key_len: usize, obj: *mut zend_object) {
    let mut zv = zval::UNDEF;
    set_obj(&mut zv, obj);
    property(arg, key, key_len, zv);
}

/// Unsupported on a compiled program: there is no local symbol table, so this always returns
/// `FAILURE`.
#[no_mangle]
pub unsafe extern "C" fn zend_set_local_var(_name: *mut zend_string, _value: *mut zval, _force: bool) -> zend_result {
    FAILURE
}

/// C-string-name form of `zend_set_local_var`; always `FAILURE`.
#[no_mangle]
pub unsafe extern "C" fn zend_set_local_var_str(_name: *const c_char, _len: usize, _value: *mut zval, _force: bool) -> zend_result {
    FAILURE
}

/// Always false: this host has no auto-global (superglobal) registry.
#[no_mangle]
pub unsafe extern "C" fn zend_is_auto_global(_name: *mut zend_string) -> bool {
    false
}

/// C-string-name form of `zend_is_auto_global`; always false.
#[no_mangle]
pub unsafe extern "C" fn zend_is_auto_global_str(_name: *const c_char, _len: usize) -> bool {
    false
}

// ------------------------------------------------------------ resources

struct ResourceType {
    dtor: rsrc_dtor_func_t,
    name: String,
}

static RESOURCE_TYPES: Global<Vec<ResourceType>> = Global::new(Vec::new());
static NEXT_RESOURCE: Global<zend_long> = Global::new(1);

/// Registers a resource type: records its destructor (the regular one, else the persistent one) and
/// name, and returns its 1-based type id.
#[no_mangle]
pub unsafe extern "C" fn zend_register_list_destructors_ex(
    ld: rsrc_dtor_func_t,
    pld: rsrc_dtor_func_t,
    type_name: *const c_char,
    _module_number: c_int,
) -> c_int {
    let types = RESOURCE_TYPES.get();
    types.push(ResourceType {
        dtor: ld.or(pld),
        name: if type_name.is_null() { String::new() } else { CStr::from_ptr(type_name).to_string_lossy().into_owned() },
    });
    types.len() as c_int
}

/// Returns the 1-based id of the registered resource type with the given name, or 0 when none
/// matches.
#[no_mangle]
pub unsafe extern "C" fn zend_fetch_list_dtor_id(type_name: *const c_char) -> c_int {
    let wanted = CStr::from_ptr(type_name).to_string_lossy();
    RESOURCE_TYPES
        .get()
        .iter()
        .position(|t| t.name == wanted)
        .map_or(0, |index| index as c_int + 1)
}

/// Allocates a resource of type `rsrc_type` wrapping `rsrc_pointer`, with refcount 1 and the next
/// handle number.
#[no_mangle]
pub unsafe extern "C" fn zend_register_resource(rsrc_pointer: *mut c_void, rsrc_type: c_int) -> *mut zend_resource {
    let res = crate::alloc::alloc_struct::<zend_resource>();
    (*res).gc.refcount = 1;
    (*res).gc.u.type_info = GC_RESOURCE;
    let next = NEXT_RESOURCE.get();
    (*res).handle = *next;
    *next += 1;
    (*res).r#type = rsrc_type;
    (*res).ptr = rsrc_pointer;
    res
}

/// Same as `zend_register_resource`; the persistent key is ignored because nothing outlives the
/// program.
#[no_mangle]
pub unsafe extern "C" fn zend_register_persistent_resource(
    _key: *const c_char,
    _key_len: usize,
    rsrc_pointer: *mut c_void,
    rsrc_type: c_int,
) -> *mut zend_resource {
    zend_register_resource(rsrc_pointer, rsrc_type)
}

/// Closes a resource: its type's destructor runs once.
#[no_mangle]
pub unsafe extern "C" fn zend_list_close(res: *mut zend_resource) {
    if (*res).r#type < 0 {
        return;
    }
    let types = RESOURCE_TYPES.get();
    let index = (*res).r#type as usize;
    if index >= 1 && index <= types.len() {
        if let Some(dtor) = types[index - 1].dtor {
            dtor(res);
        }
    }
    (*res).ptr = ptr::null_mut();
    (*res).r#type = -1;
}

/// Drops one reference to a resource and releases it (running its type destructor) when the count
/// reaches zero; always `SUCCESS`.
#[no_mangle]
pub unsafe extern "C" fn zend_list_delete(res: *mut zend_resource) -> zend_result {
    (*res).gc.refcount -= 1;
    if (*res).gc.refcount == 0 {
        resource_release(res);
    }
    SUCCESS
}

/// Closes the resource immediately (runs its destructor once) without touching its refcount.
#[no_mangle]
pub unsafe extern "C" fn zend_list_free(res: *mut zend_resource) {
    zend_list_close(res);
}

/// Frees a resource whose count reached zero.
pub unsafe fn resource_release(res: *mut zend_resource) {
    zend_list_close(res);
    free(res.cast());
}

/// Returns the registered type name of a resource as a C string, or null when its type is closed or
/// unknown.
#[no_mangle]
pub unsafe extern "C" fn zend_rsrc_list_get_rsrc_type(res: *mut zend_resource) -> *const c_char {
    let types = RESOURCE_TYPES.get();
    let index = (*res).r#type;
    if index >= 1 && index as usize <= types.len() {
        let name = intern(types[index as usize - 1].name.as_bytes());
        (*name).val.as_ptr()
    } else {
        ptr::null()
    }
}

/// Throws a `TypeError` saying the supplied resource is not a valid resource of the given type
/// name, prefixed with the active function; does nothing when the type name is null.
unsafe fn resource_error(resource_type_name: *const c_char) {
    if resource_type_name.is_null() {
        return;
    }
    let function = crate::error::active_function_prefix().unwrap_or_else(|| "main".to_string());
    crate::exception::throw_error(
        crate::exception::zend_ce_type_error,
        &format!("{function}(): supplied resource is not a valid {} resource", CStr::from_ptr(resource_type_name).to_string_lossy()),
    );
}

/// Returns the pointer of a resource if its type matches `resource_type`, otherwise raises the
/// invalid-resource `TypeError` and returns null.
#[no_mangle]
pub unsafe extern "C" fn zend_fetch_resource(res: *mut zend_resource, resource_type_name: *const c_char, resource_type: c_int) -> *mut c_void {
    if !res.is_null() && (*res).r#type == resource_type {
        return (*res).ptr;
    }
    resource_error(resource_type_name);
    ptr::null_mut()
}

/// Like `zend_fetch_resource` but accepts either of two resource types.
#[no_mangle]
pub unsafe extern "C" fn zend_fetch_resource2(
    res: *mut zend_resource,
    resource_type_name: *const c_char,
    resource_type1: c_int,
    resource_type2: c_int,
) -> *mut c_void {
    if !res.is_null() && ((*res).r#type == resource_type1 || (*res).r#type == resource_type2) {
        return (*res).ptr;
    }
    resource_error(resource_type_name);
    ptr::null_mut()
}

/// Fetches a resource from a zval: a non-resource zval raises the invalid-resource error, otherwise
/// delegates to `zend_fetch_resource`.
#[no_mangle]
pub unsafe extern "C" fn zend_fetch_resource_ex(res: *mut zval, resource_type_name: *const c_char, resource_type: c_int) -> *mut c_void {
    if res.is_null() || type_of(res) != IS_RESOURCE {
        resource_error(resource_type_name);
        return ptr::null_mut();
    }
    zend_fetch_resource((*res).value.res, resource_type_name, resource_type)
}

/// Fetches a resource from a zval accepting either of two types; non-resources raise the
/// invalid-resource error.
#[no_mangle]
pub unsafe extern "C" fn zend_fetch_resource2_ex(
    res: *mut zval,
    resource_type_name: *const c_char,
    resource_type1: c_int,
    resource_type2: c_int,
) -> *mut c_void {
    if res.is_null() || type_of(res) != IS_RESOURCE {
        resource_error(resource_type_name);
        return ptr::null_mut();
    }
    zend_fetch_resource2((*res).value.res, resource_type_name, resource_type1, resource_type2)
}

// ------------------------------------------------------------ names

/// Builds the mangled property name `"\0" class "\0" prop` (used for private/protected names) as a
/// new string; the `internal` flag is ignored.
#[no_mangle]
pub unsafe extern "C" fn zend_mangle_property_name(
    src1: *const c_char,
    src1_length: usize,
    src2: *const c_char,
    src2_length: usize,
    _internal: bool,
) -> *mut zend_string {
    let mut name = vec![0u8];
    name.extend_from_slice(std::slice::from_raw_parts(src1.cast::<u8>(), src1_length));
    name.push(0);
    name.extend_from_slice(std::slice::from_raw_parts(src2.cast::<u8>(), src2_length));
    string::new(&name)
}

/// Splits a mangled property name into class and property parts: sets `*prop_name`/`*prop_len` and,
/// if requested, `*class_name` (null for unmangled names); `FAILURE` for a malformed name lacking
/// the second NUL.
#[no_mangle]
pub unsafe extern "C" fn zend_unmangle_property_name_ex(
    name: *const zend_string,
    class_name: *mut *const c_char,
    prop_name: *mut *const c_char,
    prop_len: *mut usize,
) -> zend_result {
    let text = string::bytes(name);
    let start = (*name).val.as_ptr();
    if text.first() != Some(&0) {
        if !class_name.is_null() {
            *class_name = ptr::null();
        }
        *prop_name = start;
        if !prop_len.is_null() {
            *prop_len = text.len();
        }
        return SUCCESS;
    }
    let Some(end) = text[1..].iter().position(|&b| b == 0) else {
        *prop_name = start;
        return FAILURE;
    };
    if !class_name.is_null() {
        *class_name = start.add(1);
    }
    *prop_name = start.add(end + 2);
    if !prop_len.is_null() {
        *prop_len = text.len() - end - 2;
    }
    SUCCESS
}

/// Returns the plain property-name part of a possibly mangled name.
#[no_mangle]
pub unsafe extern "C" fn zend_get_unmangled_property_name(mangled_prop: *const zend_string) -> *const c_char {
    let mut class_name = ptr::null();
    let mut prop_name = ptr::null();
    zend_unmangle_property_name_ex(mangled_prop, &mut class_name, &mut prop_name, ptr::null_mut());
    prop_name
}

/// Returns the interned type name (`int`, `string`, ...) for an `IS_*` type constant.
#[no_mangle]
pub unsafe extern "C" fn zend_get_type_by_const_name(r#type: c_int) -> *mut zend_string {
    intern(CStr::from_ptr(crate::convert::zend_get_type_by_const(r#type)).to_bytes())
}
