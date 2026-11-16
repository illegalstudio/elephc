//! Purpose:
//! Exceptions: the built-in class hierarchy every extension may extend or
//! throw (Throwable, Exception, Error and their subclasses, SPL's exceptions,
//! the core interfaces, stdClass), throwing, chaining and clearing.
//!
//! Called from:
//! - `crate::startup` (registration), extension code (`zend_throw_exception`,
//!   `zend_ce_exception`...), `src/c/variadic.c` (formatted throws), and
//!   `crate::host`, which captures the pending exception after a call.
//!
//! Key details:
//! - Names, parents and flags follow PHP 8.5, so an extension's subclass and
//!   `instanceof` checks behave as under PHP, and Elephc rethrows the nearest
//!   class it declares.
//! - An exception's properties (`message`, `code`, `previous`, `file`,
//!   `line`, `trace`) are plain entries of its properties table. A compiled
//!   program has no PHP file or backtrace to record: `file` is empty, `line`
//!   is 0, `trace` is an empty array.

use std::ffi::{c_char, c_int, CStr};
use std::ptr;

use crate::abi::*;
use crate::class::*;
use crate::hash;
use crate::object::{find_property, object_init_ex, set_property};
use crate::startup::eg;
use crate::string::{self, intern};
use crate::zval::{copy, set_bytes, set_long, zval_ptr_dtor};

// Built-in throwable classes; set by `register_core_classes`.
#[no_mangle]
pub static mut zend_ce_throwable: *mut zend_class_entry = ptr::null_mut();
#[no_mangle]
pub static mut zend_ce_exception: *mut zend_class_entry = ptr::null_mut();
#[no_mangle]
pub static mut zend_ce_error_exception: *mut zend_class_entry = ptr::null_mut();
#[no_mangle]
pub static mut zend_ce_error: *mut zend_class_entry = ptr::null_mut();
#[no_mangle]
pub static mut zend_ce_compile_error: *mut zend_class_entry = ptr::null_mut();
#[no_mangle]
pub static mut zend_ce_parse_error: *mut zend_class_entry = ptr::null_mut();
#[no_mangle]
pub static mut zend_ce_type_error: *mut zend_class_entry = ptr::null_mut();
#[no_mangle]
pub static mut zend_ce_argument_count_error: *mut zend_class_entry = ptr::null_mut();
#[no_mangle]
pub static mut zend_ce_value_error: *mut zend_class_entry = ptr::null_mut();
#[no_mangle]
pub static mut zend_ce_arithmetic_error: *mut zend_class_entry = ptr::null_mut();
#[no_mangle]
pub static mut zend_ce_division_by_zero_error: *mut zend_class_entry = ptr::null_mut();
#[no_mangle]
pub static mut zend_ce_unhandled_match_error: *mut zend_class_entry = ptr::null_mut();
#[no_mangle]
pub static mut zend_ce_request_parse_body_exception: *mut zend_class_entry = ptr::null_mut();
#[no_mangle]
pub static mut spl_ce_LogicException: *mut zend_class_entry = ptr::null_mut();
#[no_mangle]
pub static mut spl_ce_BadFunctionCallException: *mut zend_class_entry = ptr::null_mut();
#[no_mangle]
pub static mut spl_ce_BadMethodCallException: *mut zend_class_entry = ptr::null_mut();
#[no_mangle]
pub static mut spl_ce_DomainException: *mut zend_class_entry = ptr::null_mut();
#[no_mangle]
pub static mut spl_ce_InvalidArgumentException: *mut zend_class_entry = ptr::null_mut();
#[no_mangle]
pub static mut spl_ce_LengthException: *mut zend_class_entry = ptr::null_mut();
#[no_mangle]
pub static mut spl_ce_OutOfRangeException: *mut zend_class_entry = ptr::null_mut();
#[no_mangle]
pub static mut spl_ce_RuntimeException: *mut zend_class_entry = ptr::null_mut();
#[no_mangle]
pub static mut spl_ce_OutOfBoundsException: *mut zend_class_entry = ptr::null_mut();
#[no_mangle]
pub static mut spl_ce_OverflowException: *mut zend_class_entry = ptr::null_mut();
#[no_mangle]
pub static mut spl_ce_RangeException: *mut zend_class_entry = ptr::null_mut();
#[no_mangle]
pub static mut spl_ce_UnderflowException: *mut zend_class_entry = ptr::null_mut();
#[no_mangle]
pub static mut spl_ce_UnexpectedValueException: *mut zend_class_entry = ptr::null_mut();

/// Registers a built-in class with the given name, parent and flags, using the standard object
/// handlers, and returns its class entry.
unsafe fn define(name: &str, parent: *mut zend_class_entry, flags: u32) -> *mut zend_class_entry {
    let mut entry: zend_class_entry = std::mem::zeroed();
    entry.name = intern(name.as_bytes());
    entry.default_object_handlers = &crate::object::std_object_handlers;
    zend_register_internal_class_with_flags(&mut entry, parent, flags)
}

/// Registers a built-in interface that extends the given parent interfaces and returns its class
/// entry.
unsafe fn interface(name: &str, parents: &[*mut zend_class_entry]) -> *mut zend_class_entry {
    let ce = define(name, ptr::null_mut(), ZEND_ACC_INTERFACE);
    for &parent in parents {
        zend_do_implement_interface(ce, parent);
    }
    ce
}

/// Declares the properties every Throwable class carries, PHP's defaults.
unsafe fn declare_throwable_properties(ce: *mut zend_class_entry) {
    let empty = string::zend_empty_string;
    let mut message = zval::UNDEF;
    crate::zval::set_str(&mut message, empty);
    zend_declare_property_ex(ce, intern(b"message"), &mut message, ZEND_ACC_PROTECTED as c_int, ptr::null_mut());
    let mut text = zval::UNDEF;
    crate::zval::set_str(&mut text, empty);
    zend_declare_property_ex(ce, intern(b"string"), &mut text, ZEND_ACC_PRIVATE as c_int, ptr::null_mut());
    let mut code = crate::zval::long(0);
    zend_declare_property_ex(ce, intern(b"code"), &mut code, ZEND_ACC_PROTECTED as c_int, ptr::null_mut());
    let mut file = zval::UNDEF;
    crate::zval::set_str(&mut file, empty);
    zend_declare_property_ex(ce, intern(b"file"), &mut file, ZEND_ACC_PROTECTED as c_int, ptr::null_mut());
    let mut line = crate::zval::long(0);
    zend_declare_property_ex(ce, intern(b"line"), &mut line, ZEND_ACC_PROTECTED as c_int, ptr::null_mut());
    let mut previous = crate::zval::null();
    zend_declare_property_ex(ce, intern(b"previous"), &mut previous, ZEND_ACC_PRIVATE as c_int, ptr::null_mut());
}

/// Creates an exception object: declared defaults, and an empty trace.
unsafe extern "C" fn exception_new(class_type: *mut zend_class_entry) -> *mut zend_object {
    let object = crate::object::zend_objects_new(class_type);
    crate::object::object_properties_init(object, class_type);
    let mut trace = zval::UNDEF;
    crate::zval::set_arr(&mut trace, hash::_zend_new_array(0));
    set_property(object, b"trace", &mut trace);
    object
}

/// Registers the classes and interfaces of PHP's core, SPL and ext/json that
/// extensions extend, implement or throw. The Core module must be current.
pub unsafe fn register_core_classes() {
    zend_ce_traversable = interface("Traversable", &[]);
    zend_ce_aggregate = interface("IteratorAggregate", &[zend_ce_traversable]);
    zend_ce_iterator = interface("Iterator", &[zend_ce_traversable]);
    zend_ce_arrayaccess = interface("ArrayAccess", &[]);
    zend_ce_serializable = interface("Serializable", &[]);
    zend_ce_countable = interface("Countable", &[]);
    spl_ce_Countable = zend_ce_countable;
    zend_ce_stringable = interface("Stringable", &[]);
    zend_ce_unit_enum = interface("UnitEnum", &[]);
    zend_ce_backed_enum = interface("BackedEnum", &[zend_ce_unit_enum]);
    php_json_serializable_ce = interface("JsonSerializable", &[]);

    zend_standard_class_def = define("stdClass", ptr::null_mut(), ZEND_ACC_ALLOW_DYNAMIC_PROPERTIES);
    zend_ce_closure = define("Closure", ptr::null_mut(), ZEND_ACC_FINAL | ZEND_ACC_NO_DYNAMIC_PROPERTIES | ZEND_ACC_NOT_SERIALIZABLE);
    zend_ce_internal_iterator = define("InternalIterator", ptr::null_mut(), ZEND_ACC_FINAL);
    zend_do_implement_interface(zend_ce_internal_iterator, zend_ce_iterator);
    zend_ce_attribute = define("Attribute", ptr::null_mut(), ZEND_ACC_FINAL);
    zend_ce_allow_dynamic_properties = define("AllowDynamicProperties", ptr::null_mut(), ZEND_ACC_FINAL);
    zend_ce_sensitive_parameter = define("SensitiveParameter", ptr::null_mut(), ZEND_ACC_FINAL);
    php_ce_incomplete_class = define("__PHP_Incomplete_Class", ptr::null_mut(), ZEND_ACC_FINAL | ZEND_ACC_ALLOW_DYNAMIC_PROPERTIES);

    zend_ce_throwable = interface("Throwable", &[zend_ce_stringable]);
    zend_ce_exception = define("Exception", ptr::null_mut(), 0);
    zend_ce_error = define("Error", ptr::null_mut(), 0);
    for base in [zend_ce_exception, zend_ce_error] {
        (*base).create_u.create_object = Some(exception_new);
        declare_throwable_properties(base);
        zend_do_implement_interface(base, zend_ce_throwable);
    }
    zend_ce_error_exception = define("ErrorException", zend_ce_exception, 0);
    let mut severity = crate::zval::long(E_ERROR as zend_long);
    zend_declare_property_ex(zend_ce_error_exception, intern(b"severity"), &mut severity, ZEND_ACC_PROTECTED as c_int, ptr::null_mut());
    zend_ce_compile_error = define("CompileError", zend_ce_error, 0);
    zend_ce_parse_error = define("ParseError", zend_ce_compile_error, 0);
    zend_ce_type_error = define("TypeError", zend_ce_error, 0);
    zend_ce_argument_count_error = define("ArgumentCountError", zend_ce_type_error, 0);
    zend_ce_value_error = define("ValueError", zend_ce_error, 0);
    zend_ce_arithmetic_error = define("ArithmeticError", zend_ce_error, 0);
    zend_ce_division_by_zero_error = define("DivisionByZeroError", zend_ce_arithmetic_error, 0);
    zend_ce_unhandled_match_error = define("UnhandledMatchError", zend_ce_error, 0);
    zend_ce_request_parse_body_exception = define("RequestParseBodyException", zend_ce_exception, 0);
    php_json_exception_ce = define("JsonException", zend_ce_exception, 0);

    spl_ce_LogicException = define("LogicException", zend_ce_exception, 0);
    spl_ce_BadFunctionCallException = define("BadFunctionCallException", spl_ce_LogicException, 0);
    spl_ce_BadMethodCallException = define("BadMethodCallException", spl_ce_BadFunctionCallException, 0);
    spl_ce_DomainException = define("DomainException", spl_ce_LogicException, 0);
    spl_ce_InvalidArgumentException = define("InvalidArgumentException", spl_ce_LogicException, 0);
    spl_ce_LengthException = define("LengthException", spl_ce_LogicException, 0);
    spl_ce_OutOfRangeException = define("OutOfRangeException", spl_ce_LogicException, 0);
    spl_ce_RuntimeException = define("RuntimeException", zend_ce_exception, 0);
    spl_ce_OutOfBoundsException = define("OutOfBoundsException", spl_ce_RuntimeException, 0);
    spl_ce_OverflowException = define("OverflowException", spl_ce_RuntimeException, 0);
    spl_ce_RangeException = define("RangeException", spl_ce_RuntimeException, 0);
    spl_ce_UnderflowException = define("UnderflowException", spl_ce_RuntimeException, 0);
    spl_ce_UnexpectedValueException = define("UnexpectedValueException", spl_ce_RuntimeException, 0);

    spl_ce_RecursiveIterator = interface("RecursiveIterator", &[zend_ce_iterator]);
    spl_ce_SeekableIterator = interface("SeekableIterator", &[zend_ce_iterator]);
    spl_ce_OuterIterator = interface("OuterIterator", &[zend_ce_iterator]);
    spl_ce_ArrayIterator = define("ArrayIterator", ptr::null_mut(), 0);
    for iface in [spl_ce_SeekableIterator, zend_ce_arrayaccess, zend_ce_serializable, zend_ce_countable] {
        zend_do_implement_interface(spl_ce_ArrayIterator, iface);
    }
    spl_ce_SplFixedArray = define("SplFixedArray", ptr::null_mut(), 0);
    for iface in [zend_ce_aggregate, zend_ce_arrayaccess, zend_ce_countable, php_json_serializable_ce] {
        zend_do_implement_interface(spl_ce_SplFixedArray, iface);
    }
}

// ------------------------------------------------------------ throwing

/// Makes `exception` the pending exception, chaining any already pending one
/// as its `previous`. Takes the caller's reference.
#[no_mangle]
pub unsafe extern "C" fn zend_throw_exception_internal(exception: *mut zend_object) {
    if exception.is_null() {
        return;
    }
    let pending = eg().exception;
    if !pending.is_null() {
        if pending == exception {
            return;
        }
        zend_exception_set_previous(exception, pending);
    }
    eg().exception = exception;
}

/// Creates and throws an exception of class `ce` (Exception when NULL).
pub unsafe fn throw(ce: *mut zend_class_entry, message: &[u8], code: zend_long) -> *mut zend_object {
    let ce = if ce.is_null() { zend_ce_exception } else { ce };
    let mut object = zval::UNDEF;
    if object_init_ex(&mut object, ce) != SUCCESS {
        return ptr::null_mut();
    }
    let mut text = zval::UNDEF;
    set_bytes(&mut text, message);
    set_property(object.value.obj, b"message", &mut text);
    if code != 0 {
        let mut value = zval::UNDEF;
        set_long(&mut value, code);
        set_property(object.value.obj, b"code", &mut value);
    }
    zend_throw_exception_internal(object.value.obj);
    object.value.obj
}

/// Throws a new exception or error of class `ce` with a message and code 0, leaving it pending in
/// the executor globals.
pub unsafe fn throw_error(ce: *mut zend_class_entry, message: &str) {
    throw(ce, message.as_bytes(), 0);
}

/// `zend_throw_exception`: throws a new exception of class `exception_ce` with a C-string message
/// (NULL means empty) and a code; returns the exception object.
#[no_mangle]
pub unsafe extern "C" fn zend_throw_exception(exception_ce: *mut zend_class_entry, message: *const c_char, code: zend_long) -> *mut zend_object {
    let text = if message.is_null() { &b""[..] } else { CStr::from_ptr(message).to_bytes() };
    throw(exception_ce, text, code)
}

/// `zend_throw_error_exception`: throws an exception of the given class with a `zend_string`
/// message and code, and sets its `severity` property when the class is an `ErrorException`, as
/// php-src does; returns the object.
#[no_mangle]
pub unsafe extern "C" fn zend_throw_error_exception(
    exception_ce: *mut zend_class_entry,
    message: *mut zend_string,
    code: zend_long,
    severity: c_int,
) -> *mut zend_object {
    let object = throw(exception_ce, string::bytes(message), code);
    if !object.is_null()
        && !exception_ce.is_null()
        && crate::class::instanceof_function(exception_ce, zend_ce_error_exception)
    {
        let mut value = crate::zval::long(severity as zend_long);
        set_property(object, b"severity", &mut value);
    }
    object
}

/// Throws the object held by `exception`, taking its reference.
#[no_mangle]
pub unsafe extern "C" fn zend_throw_exception_object(exception: *mut zval) {
    if exception.is_null() || (*exception).u1.v.r#type != IS_OBJECT {
        throw_error(zend_ce_error, "Need to supply an object when throwing an exception");
        return;
    }
    let object = (*exception).value.obj;
    if !instanceof_function_slow((*object).ce, zend_ce_throwable) {
        throw_error(zend_ce_error, "Cannot throw objects that do not implement Throwable");
        zval_ptr_dtor(exception);
        return;
    }
    zend_throw_exception_internal(object);
}

/// `zend_throw_error()` and friends, after the C side formatted the message.
/// c: hidden
#[no_mangle]
pub unsafe extern "C" fn elephc_zend_throw_message(ce: *mut zend_class_entry, message: *const c_char, len: usize, code: i64) -> *mut zend_object {
    throw(ce, std::slice::from_raw_parts(message.cast(), len), code)
}

/// `zend_argument_error()` after formatting: `f(): Argument #n ($name) ...`.
/// c: hidden
#[no_mangle]
pub unsafe extern "C" fn elephc_zend_throw_argument_message(ce: *mut zend_class_entry, arg_num: u32, message: *const c_char, len: usize) {
    let message = String::from_utf8_lossy(std::slice::from_raw_parts(message.cast::<u8>(), len)).into_owned();
    throw_error(ce, &crate::zpp::argument_message(arg_num, &message));
}

/// c: hidden
#[no_mangle]
pub unsafe extern "C" fn elephc_zend_class_error() -> *mut zend_class_entry {
    zend_ce_error
}

/// c: hidden
#[no_mangle]
pub unsafe extern "C" fn elephc_zend_class_type_error() -> *mut zend_class_entry {
    zend_ce_type_error
}

/// c: hidden
#[no_mangle]
pub unsafe extern "C" fn elephc_zend_class_value_error() -> *mut zend_class_entry {
    zend_ce_value_error
}

/// c: hidden
#[no_mangle]
pub unsafe extern "C" fn elephc_zend_class_argument_count_error() -> *mut zend_class_entry {
    zend_ce_argument_count_error
}

/// `zend_clear_exception`: discards the pending exception, releasing it; a no-op when none is
/// pending. A program callable's throwable the host kept for rethrowing is discarded with it.
#[no_mangle]
pub unsafe extern "C" fn zend_clear_exception() {
    let exception = eg().exception;
    if exception.is_null() {
        return;
    }
    crate::host::forget_callback_marker();
    eg().exception = ptr::null_mut();
    crate::object::zend_object_release(exception);
}

/// Appends `add_previous` at the end of `exception`'s `previous` chain,
/// taking its reference (a cycle is cut instead of formed).
#[no_mangle]
pub unsafe extern "C" fn zend_exception_set_previous(exception: *mut zend_object, add_previous: *mut zend_object) {
    if exception.is_null() || add_previous.is_null() {
        return;
    }
    if exception == add_previous {
        crate::object::zend_object_release(add_previous);
        return;
    }
    let mut current = exception;
    loop {
        let previous = find_property(current, b"previous");
        if previous.is_null() || (*previous).u1.v.r#type != IS_OBJECT {
            let mut value = zval::UNDEF;
            crate::zval::set_obj(&mut value, add_previous);
            set_property(current, b"previous", &mut value);
            return;
        }
        current = (*previous).value.obj;
        if current == add_previous {
            crate::object::zend_object_release(add_previous);
            return;
        }
    }
}

/// `zend_exception_save`: sets the pending exception aside as the previous one (chaining any older
/// saved exception behind it) so cleanup code can run, leaving no exception pending.
#[no_mangle]
pub unsafe extern "C" fn zend_exception_save() {
    let eg = eg();
    if !eg.prev_exception.is_null() && !eg.exception.is_null() {
        zend_exception_set_previous(eg.exception, eg.prev_exception);
    }
    if !eg.exception.is_null() {
        eg.prev_exception = eg.exception;
    }
    eg.exception = ptr::null_mut();
}

/// `zend_exception_restore`: puts back an exception set aside by `zend_exception_save`, chaining it
/// behind any newer pending exception.
#[no_mangle]
pub unsafe extern "C" fn zend_exception_restore() {
    let eg = eg();
    if !eg.prev_exception.is_null() {
        if !eg.exception.is_null() {
            zend_exception_set_previous(eg.exception, eg.prev_exception);
        } else {
            eg.exception = eg.prev_exception;
        }
        eg.prev_exception = ptr::null_mut();
    }
}

/// `zend_exception_get_default`: the base `Exception` class entry.
#[no_mangle]
pub unsafe extern "C" fn zend_exception_get_default() -> *mut zend_class_entry {
    zend_ce_exception
}

/// `zend_get_exception_base`: the base class entry of a throwable object, `Exception` or `Error`.
#[no_mangle]
pub unsafe extern "C" fn zend_get_exception_base(object: *mut zend_object) -> *mut zend_class_entry {
    if instanceof_function_slow((*object).ce, zend_ce_exception) {
        zend_ce_exception
    } else {
        zend_ce_error
    }
}

/// Reports an uncaught exception as PHP does, as a fatal error.
#[no_mangle]
pub unsafe extern "C" fn zend_exception_error(ex: *mut zend_object, severity: c_int) -> zend_result {
    let message = find_property(ex, b"message");
    let text = if !message.is_null() && (*message).u1.v.r#type == IS_STRING {
        string::text((*message).value.str)
    } else {
        String::new()
    };
    let report = format!("Uncaught {}: {text}", crate::object::class_name(ex));
    crate::error::report(severity, report.as_bytes());
    SUCCESS
}

/// `zend_is_unwind_exit`: always false, because unwind-exit exceptions do not exist here.
#[no_mangle]
pub unsafe extern "C" fn zend_is_unwind_exit(_ex: *const zend_object) -> bool {
    false
}

/// `zend_is_graceful_exit`: always false, because graceful-exit exceptions do not exist here.
#[no_mangle]
pub unsafe extern "C" fn zend_is_graceful_exit(_ex: *const zend_object) -> bool {
    false
}

/// The pending exception's message, code and class chain (most derived
/// first, comma-separated), clearing it.
pub unsafe fn take_pending() -> Option<(Vec<u8>, zend_long, String)> {
    let exception = eg().exception;
    if exception.is_null() {
        return None;
    }
    let message = find_property(exception, b"message");
    let message = if !message.is_null() && (*crate::zval::deref(message)).u1.v.r#type == IS_STRING {
        string::bytes((*crate::zval::deref(message)).value.str).to_vec()
    } else {
        Vec::new()
    };
    let code = find_property(exception, b"code");
    let code = if !code.is_null() && (*crate::zval::deref(code)).u1.v.r#type == IS_LONG {
        (*crate::zval::deref(code)).value.lval
    } else {
        0
    };
    let mut chain = Vec::new();
    let mut ce = (*exception).ce;
    while !ce.is_null() {
        chain.push(string::text((*ce).name));
        ce = (*ce).parent_u.parent;
    }
    zend_clear_exception();
    Some((message, code, chain.join(",")))
}

/// Keeps `copy` referenced so the object outlives a borrowed zval.
pub unsafe fn hold(value: *mut zval) -> zval {
    let mut held = zval::UNDEF;
    copy(&mut held, value);
    held
}
