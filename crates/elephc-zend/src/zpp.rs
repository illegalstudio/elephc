//! Purpose:
//! Argument parsing: the per-type helpers the fast `ZEND_PARSE_PARAMETERS`
//! macros call, their weak-mode coercions, the classic
//! `zend_parse_parameters("s|l", ...)` spec parser, and PHP's error messages.
//!
//! Called from:
//! - Extension functions (through the macros in `zend_macros.h` and the
//!   variadic entry points in `src/c/variadic.c`).
//!
//! Key details:
//! - Coercion is PHP 8.5's weak mode for internal functions: numeric strings
//!   (whole, whitespace allowed) become numbers, scalars become strings,
//!   a float with a fractional part becomes an int with a deprecation, and
//!   null to a non-nullable scalar is deprecated but accepted.
//! - Messages match PHP word for word: `f() expects exactly 1 argument, 0
//!   given`, `f(): Argument #1 ($x) must be of type string, array given`.

use std::ffi::{c_char, c_int, c_void, CStr};
use std::ptr;

use crate::abi::*;
use crate::convert::{parse_numeric, value_name, Numeric};
use crate::exception::{throw_error, zend_ce_argument_count_error, zend_ce_type_error, zend_ce_value_error};
use crate::startup::eg;
use crate::string::{self, bytes};
use crate::zval::*;

/// What each `Z_EXPECTED_*` failure says the argument had to be.
const EXPECTED: &[&str] = &[
    "of type int",
    "of type ?int",
    "of type bool",
    "of type ?bool",
    "of type string",
    "of type ?string",
    "of type array",
    "of type ?array",
    "of type array|int",
    "of type array|int|null",
    "of type iterable",
    "of type ?iterable",
    "a valid callback",
    "a valid callback or null",
    "of type resource",
    "of type resource or null",
    "of type string",
    "of type ?string",
    "of type object",
    "of type ?object",
    "of type float",
    "of type ?float",
    "of type int|float",
    "of type int|float|null",
    "of type string|int|float",
    "of type string|int|float|null",
    "of type array|string",
    "of type array|string|null",
    "of type string|int",
    "of type string|int|null",
    "an object or a valid class name",
    "an object, a valid class name, or null",
    "of type object|string",
    "of type object|string|null",
];

/// `f(): Argument #n ($name) <message>`.
pub unsafe fn argument_message(arg_num: u32, message: &str) -> String {
    let function = crate::error::active_function_prefix().unwrap_or_else(|| "main".to_string());
    let name = crate::api::get_active_function_arg_name(arg_num);
    let name = if name.is_null() {
        String::new()
    } else {
        format!(" (${})", CStr::from_ptr(name).to_string_lossy())
    };
    format!("{function}(): Argument #{arg_num}{name} {message}")
}

/// Throws a TypeError whose message is the active function's "Argument #N ($name) ..." prefix
/// followed by `message`.
unsafe fn argument_type_error(arg_num: u32, message: &str) {
    throw_error(zend_ce_type_error, &argument_message(arg_num, message));
}

/// The number of arguments passed to the active call, 0 when no call frame is active.
unsafe fn current_num_args() -> u32 {
    let frame = eg().current_execute_data;
    if frame.is_null() {
        0
    } else {
        (*frame).This.u2.num_args
    }
}

// ------------------------------------------------------------ errors

/// `zend_wrong_parameters_none_error`: throws ArgumentCountError "f() expects exactly 0 arguments,
/// N given" for a function that takes none.
#[no_mangle]
pub unsafe extern "C" fn zend_wrong_parameters_none_error() {
    let num_args = current_num_args();
    let function = crate::error::active_function_prefix().unwrap_or_else(|| "main".to_string());
    throw_error(
        zend_ce_argument_count_error,
        &format!("{function}() expects exactly 0 arguments, {num_args} given"),
    );
}

/// `zend_wrong_parameters_count_error`: throws ArgumentCountError for a bad argument count, saying
/// "exactly", "at least" or "at most" with the violated bound and the number given.
#[no_mangle]
pub unsafe extern "C" fn zend_wrong_parameters_count_error(min_num_args: u32, max_num_args: u32) {
    let num_args = current_num_args();
    let function = crate::error::active_function_prefix().unwrap_or_else(|| "main".to_string());
    let bound = if num_args < min_num_args { min_num_args } else { max_num_args };
    let quantifier = if min_num_args == max_num_args {
        "exactly"
    } else if num_args < min_num_args {
        "at least"
    } else {
        "at most"
    };
    throw_error(
        zend_ce_argument_count_error,
        &format!(
            "{function}() expects {quantifier} {bound} argument{}, {num_args} given",
            if bound == 1 { "" } else { "s" }
        ),
    );
}

/// `zend_wrong_param_count`: throws ArgumentCountError "Wrong parameter count for f()".
#[no_mangle]
pub unsafe extern "C" fn zend_wrong_param_count() {
    let function = crate::error::active_function_prefix().unwrap_or_else(|| "main".to_string());
    throw_error(zend_ce_argument_count_error, &format!("Wrong parameter count for {function}()"));
}

/// `zend_wrong_parameter_type_error`: throws the TypeError (or ValueError for a path with NUL
/// bytes) for argument `num` whose type does not match the `Z_EXPECTED_*` description; does nothing
/// if an exception is already pending.
#[no_mangle]
pub unsafe extern "C" fn zend_wrong_parameter_type_error(num: u32, expected_type: zend_expected_type, arg: *mut zval) {
    if !eg().exception.is_null() {
        return;
    }
    if (expected_type == Z_EXPECTED_PATH || expected_type == Z_EXPECTED_PATH_OR_NULL) && type_of(arg) == IS_STRING {
        throw_error(zend_ce_value_error, &argument_message(num, "must not contain any null bytes"));
        return;
    }
    let expected = EXPECTED.get(expected_type as usize).copied().unwrap_or("of a valid type");
    argument_type_error(num, &format!("must be {expected}, {} given", value_name(arg)));
}

/// Throws the TypeError "must be of type Class<suffix>, T given" for argument `num`, unless an
/// exception is already pending.
unsafe fn class_error(num: u32, name: *const c_char, arg: *mut zval, suffix: &str) {
    if !eg().exception.is_null() {
        return;
    }
    let class = CStr::from_ptr(name).to_string_lossy();
    argument_type_error(num, &format!("must be of type {class}{suffix}, {} given", value_name(arg)));
}

/// `zend_wrong_parameter_class_error`: throws the TypeError for an argument that is not an instance
/// of the named class.
#[no_mangle]
pub unsafe extern "C" fn zend_wrong_parameter_class_error(num: u32, name: *const c_char, arg: *mut zval) {
    class_error(num, name, arg, "");
}

/// `zend_wrong_parameter_class_or_null_error`: throws the TypeError for an argument that is neither
/// the named class nor null.
#[no_mangle]
pub unsafe extern "C" fn zend_wrong_parameter_class_or_null_error(num: u32, name: *const c_char, arg: *mut zval) {
    if !eg().exception.is_null() {
        return;
    }
    let class = CStr::from_ptr(name).to_string_lossy();
    argument_type_error(num, &format!("must be of type ?{class}, {} given", value_name(arg)));
}

/// `zend_wrong_parameter_class_or_long_error`: throws the TypeError for an argument that is neither
/// the named class nor an int.
#[no_mangle]
pub unsafe extern "C" fn zend_wrong_parameter_class_or_long_error(num: u32, name: *const c_char, arg: *mut zval) {
    class_error(num, name, arg, "|int");
}

/// `zend_wrong_parameter_class_or_long_or_null_error`: throws the TypeError for an argument that is
/// none of the named class, int or null.
#[no_mangle]
pub unsafe extern "C" fn zend_wrong_parameter_class_or_long_or_null_error(num: u32, name: *const c_char, arg: *mut zval) {
    class_error(num, name, arg, "|int|null");
}

/// `zend_wrong_parameter_class_or_string_error`: throws the TypeError for an argument that is
/// neither the named class nor a string.
#[no_mangle]
pub unsafe extern "C" fn zend_wrong_parameter_class_or_string_error(num: u32, name: *const c_char, arg: *mut zval) {
    class_error(num, name, arg, "|string");
}

/// `zend_wrong_parameter_class_or_string_or_null_error`: throws the TypeError for an argument that
/// is none of the named class, string or null.
#[no_mangle]
pub unsafe extern "C" fn zend_wrong_parameter_class_or_string_or_null_error(num: u32, name: *const c_char, arg: *mut zval) {
    class_error(num, name, arg, "|string|null");
}

/// `zend_wrong_callback_error`: throws the TypeError "must be a valid callback, <detail>" for
/// argument `num`, using the given detail text (may be NULL). The detail is freed whether or not
/// an exception was already pending, as php-src's `efree(error)` is unconditional.
#[no_mangle]
pub unsafe extern "C" fn zend_wrong_callback_error(num: u32, error: *mut c_char) {
    callback_error(num, error, "must be a valid callback");
}

/// `zend_wrong_callback_or_null_error`: throws the TypeError "must be a valid callback or null,
/// <detail>" for argument `num`; the detail is always freed.
#[no_mangle]
pub unsafe extern "C" fn zend_wrong_callback_or_null_error(num: u32, error: *mut c_char) {
    callback_error(num, error, "must be a valid callback or null");
}

/// Throws "<what>, <detail>" for argument `num` unless an exception is already pending, then
/// frees the detail the caller allocated.
unsafe fn callback_error(num: u32, error: *mut c_char, what: &str) {
    if eg().exception.is_null() {
        let detail = if error.is_null() { String::new() } else { CStr::from_ptr(error).to_string_lossy().into_owned() };
        argument_type_error(num, &format!("{what}, {detail}"));
    }
    if !error.is_null() {
        crate::alloc::free(error.cast());
    }
}

/// `zend_unexpected_extra_named_error`: throws ArgumentCountError because the function does not
/// accept unknown named parameters.
#[no_mangle]
pub unsafe extern "C" fn zend_unexpected_extra_named_error() {
    let function = crate::error::active_function_prefix().unwrap_or_else(|| "main".to_string());
    throw_error(
        zend_ce_argument_count_error,
        &format!("{function}() does not accept unknown named parameters"),
    );
}

/// The fast-ZPP failure report.
#[no_mangle]
pub unsafe extern "C" fn zend_wrong_parameter_error(
    error_code: c_int,
    num: u32,
    name: *mut c_char,
    expected_type: zend_expected_type,
    arg: *mut zval,
) {
    match error_code {
        ZPP_ERROR_WRONG_CALLBACK => zend_wrong_callback_error(num, name),
        ZPP_ERROR_WRONG_CALLBACK_OR_NULL => zend_wrong_callback_or_null_error(num, name),
        ZPP_ERROR_WRONG_CLASS => zend_wrong_parameter_class_error(num, name, arg),
        ZPP_ERROR_WRONG_CLASS_OR_NULL => zend_wrong_parameter_class_or_null_error(num, name, arg),
        ZPP_ERROR_WRONG_CLASS_OR_LONG => zend_wrong_parameter_class_or_long_error(num, name, arg),
        ZPP_ERROR_WRONG_CLASS_OR_LONG_OR_NULL => zend_wrong_parameter_class_or_long_or_null_error(num, name, arg),
        ZPP_ERROR_WRONG_CLASS_OR_STRING => zend_wrong_parameter_class_or_string_error(num, name, arg),
        ZPP_ERROR_WRONG_CLASS_OR_STRING_OR_NULL => zend_wrong_parameter_class_or_string_or_null_error(num, name, arg),
        ZPP_ERROR_WRONG_ARG => zend_wrong_parameter_type_error(num, expected_type, arg),
        ZPP_ERROR_UNEXPECTED_EXTRA_NAMED => zend_unexpected_extra_named_error(),
        _ => {}
    }
}

/// Passing null to a non-nullable scalar parameter: a deprecation, then the
/// value is used as if empty. False when the deprecation became an exception.
unsafe fn null_arg_deprecated(fallback_type: &str, arg_num: u32) -> bool {
    let function = crate::error::active_function_prefix().unwrap_or_else(|| "main".to_string());
    let name = crate::api::get_active_function_arg_name(arg_num);
    let name = if name.is_null() {
        String::new()
    } else {
        format!(" (${})", CStr::from_ptr(name).to_string_lossy())
    };
    crate::error::deprecated(&format!(
        "{function}(): Passing null to parameter #{arg_num}{name} of type {fallback_type} is deprecated"
    ));
    eg().exception.is_null()
}

/// `zend_parse_arg_null_deprecated`: emits the deprecation for passing null to a mixed-typed
/// internal parameter and returns false if that raised an exception.
#[no_mangle]
pub unsafe extern "C" fn zend_parse_arg_null_deprecated(arg_num: u32) -> bool {
    null_arg_deprecated("mixed", arg_num)
}

// ------------------------------------------------------------ weak-mode coercions

/// `zend_parse_arg_bool_weak`: weak-mode coercion of an argument to bool (null with a deprecation,
/// scalars and numeric strings by truthiness); false when the type is not coercible.
#[no_mangle]
pub unsafe extern "C" fn zend_parse_arg_bool_weak(arg: *const zval, dest: *mut bool, arg_num: u32) -> bool {
    match type_of(arg) {
        IS_NULL => {
            if !null_arg_deprecated("bool", arg_num) {
                return false;
            }
            *dest = false;
            true
        }
        IS_FALSE | IS_TRUE | IS_LONG | IS_DOUBLE | IS_STRING => {
            *dest = crate::convert::zend_is_true(arg);
            true
        }
        _ => false,
    }
}

/// `zend_parse_arg_bool_slow`: slow path of the bool argument parser; same as
/// `zend_parse_arg_bool_weak`.
#[no_mangle]
pub unsafe extern "C" fn zend_parse_arg_bool_slow(arg: *const zval, dest: *mut bool, arg_num: u32) -> bool {
    zend_parse_arg_bool_weak(arg, dest, arg_num)
}

/// A float as an int parameter: it must fit, and losing a fraction is
/// deprecated.
unsafe fn double_to_long_arg(value: f64, dest: *mut zend_long) -> bool {
    if !value.is_finite() || value >= 9.223372036854775808e18 || value < -9.223372036854775808e18 {
        return false;
    }
    let truncated = value as zend_long;
    if truncated as f64 != value {
        crate::convert::zend_incompatible_double_to_long_error(value);
        if !eg().exception.is_null() {
            return false;
        }
    }
    *dest = truncated;
    true
}

/// `zend_parse_arg_long_weak`: weak-mode coercion of an argument to int: floats must be integral
/// and in range (a lost fraction is deprecated), numeric strings convert, bools and null follow
/// PHP; false when not coercible.
#[no_mangle]
pub unsafe extern "C" fn zend_parse_arg_long_weak(arg: *const zval, dest: *mut zend_long, arg_num: u32) -> bool {
    match type_of(arg) {
        IS_DOUBLE => double_to_long_arg((*arg).value.dval, dest),
        IS_STRING => match parse_numeric(bytes((*arg).value.str), false) {
            Some((Numeric::Long(value), _, _)) => {
                *dest = value;
                true
            }
            Some((Numeric::Double(value), _, _)) => double_to_long_arg(value, dest),
            None => false,
        },
        IS_NULL => {
            if !null_arg_deprecated("int", arg_num) {
                return false;
            }
            *dest = 0;
            true
        }
        IS_FALSE => {
            *dest = 0;
            true
        }
        IS_TRUE => {
            *dest = 1;
            true
        }
        _ => false,
    }
}

/// `zend_parse_arg_long_slow`: slow path of the int argument parser; same as
/// `zend_parse_arg_long_weak`.
#[no_mangle]
pub unsafe extern "C" fn zend_parse_arg_long_slow(arg: *const zval, dest: *mut zend_long, arg_num: u32) -> bool {
    zend_parse_arg_long_weak(arg, dest, arg_num)
}

/// `zend_parse_arg_double_weak`: weak-mode coercion of an argument to float; false when the type is
/// not coercible.
#[no_mangle]
pub unsafe extern "C" fn zend_parse_arg_double_weak(arg: *const zval, dest: *mut f64, arg_num: u32) -> bool {
    match type_of(arg) {
        IS_LONG => {
            *dest = (*arg).value.lval as f64;
            true
        }
        IS_STRING => match parse_numeric(bytes((*arg).value.str), false) {
            Some((Numeric::Long(value), _, _)) => {
                *dest = value as f64;
                true
            }
            Some((Numeric::Double(value), _, _)) => {
                *dest = value;
                true
            }
            None => false,
        },
        IS_NULL => {
            if !null_arg_deprecated("float", arg_num) {
                return false;
            }
            *dest = 0.0;
            true
        }
        IS_FALSE => {
            *dest = 0.0;
            true
        }
        IS_TRUE => {
            *dest = 1.0;
            true
        }
        _ => false,
    }
}

/// `zend_parse_arg_double_slow`: slow path of the float argument parser; ints convert directly,
/// everything else goes through `zend_parse_arg_double_weak`.
#[no_mangle]
pub unsafe extern "C" fn zend_parse_arg_double_slow(arg: *const zval, dest: *mut f64, arg_num: u32) -> bool {
    if type_of(arg) == IS_LONG {
        *dest = (*arg).value.lval as f64;
        return true;
    }
    zend_parse_arg_double_weak(arg, dest, arg_num)
}

/// Converts the argument slot itself to a string, as PHP does, so the
/// returned string lives as long as the argument.
#[no_mangle]
pub unsafe extern "C" fn zend_parse_arg_str_weak(arg: *mut zval, dest: *mut *mut zend_string, arg_num: u32) -> bool {
    match type_of(arg) {
        IS_NULL => {
            if !null_arg_deprecated("string", arg_num) {
                return false;
            }
            crate::convert::convert_to_string(arg);
            *dest = (*arg).value.str;
            true
        }
        IS_FALSE | IS_TRUE | IS_LONG | IS_DOUBLE => {
            crate::convert::convert_to_string(arg);
            *dest = (*arg).value.str;
            true
        }
        IS_OBJECT => {
            let object = (*arg).value.obj;
            let mut result = zval::UNDEF;
            if let Some(cast) = (*(*object).handlers).cast_object {
                if cast(object, &mut result, IS_STRING as c_int) == SUCCESS {
                    crate::object::zend_object_release(object);
                    copy_value(arg, &result);
                    *dest = (*arg).value.str;
                    return true;
                }
            }
            false
        }
        _ => false,
    }
}

/// `zend_parse_arg_str_slow`: slow path of the string argument parser; same as
/// `zend_parse_arg_str_weak`.
#[no_mangle]
pub unsafe extern "C" fn zend_parse_arg_str_slow(arg: *mut zval, dest: *mut *mut zend_string, arg_num: u32) -> bool {
    zend_parse_arg_str_weak(arg, dest, arg_num)
}

/// `zend_parse_arg_number_slow`: coerces an argument to an int or float zval (numeric strings
/// parsed, bools and null converted), storing the number zval in `*dest`; false when not coercible.
#[no_mangle]
pub unsafe extern "C" fn zend_parse_arg_number_slow(arg: *mut zval, dest: *mut *mut zval, arg_num: u32) -> bool {
    match type_of(arg) {
        IS_STRING => {
            let number = parse_numeric(bytes((*arg).value.str), false);
            match number {
                Some((Numeric::Long(value), _, _)) => {
                    zval_ptr_dtor(arg);
                    set_long(arg, value);
                }
                Some((Numeric::Double(value), _, _)) => {
                    zval_ptr_dtor(arg);
                    set_double(arg, value);
                }
                None => return false,
            }
        }
        IS_NULL => {
            if !null_arg_deprecated("int|float", arg_num) {
                return false;
            }
            set_long(arg, 0);
        }
        IS_FALSE => set_long(arg, 0),
        IS_TRUE => set_long(arg, 1),
        _ => return false,
    }
    *dest = arg;
    true
}

/// `zend_parse_arg_number_or_str_slow`: coerces an argument for an `int|float|string` parameter
/// (null and bools become ints, objects go through string conversion), storing the zval in `*dest`;
/// false when not coercible.
#[no_mangle]
pub unsafe extern "C" fn zend_parse_arg_number_or_str_slow(arg: *mut zval, dest: *mut *mut zval, arg_num: u32) -> bool {
    match type_of(arg) {
        IS_NULL => {
            if !null_arg_deprecated("string|int|float", arg_num) {
                return false;
            }
            set_long(arg, 0);
        }
        IS_FALSE => set_long(arg, 0),
        IS_TRUE => set_long(arg, 1),
        IS_OBJECT => {
            let mut text: *mut zend_string = ptr::null_mut();
            if !zend_parse_arg_str_weak(arg, &mut text, arg_num) {
                return false;
            }
        }
        _ => return false,
    }
    *dest = arg;
    true
}

/// `zend_parse_arg_str_or_long_slow`: coerces an argument for a `string|int` parameter: floats,
/// bools and null become an int, anything else is converted to a string; `*dest_str` is NULL when
/// the int form was used.
#[no_mangle]
pub unsafe extern "C" fn zend_parse_arg_str_or_long_slow(
    arg: *mut zval,
    dest_str: *mut *mut zend_string,
    dest_long: *mut zend_long,
    arg_num: u32,
) -> bool {
    *dest_str = ptr::null_mut();
    match type_of(arg) {
        IS_DOUBLE | IS_FALSE | IS_TRUE => zend_parse_arg_long_weak(arg, dest_long, arg_num),
        IS_NULL => zend_parse_arg_long_weak(arg, dest_long, arg_num),
        _ => zend_parse_arg_str_weak(arg, dest_str, arg_num),
    }
}

// ------------------------------------------------------------ fast-ZPP helpers

/// `zend_parse_arg_bool`: parses a bool argument, optionally nullable (`is_null` reports null);
/// other types take the weak coercion path. Returns false on a type error.
#[no_mangle]
pub unsafe extern "C" fn zend_parse_arg_bool(arg: *const zval, dest: *mut bool, is_null: *mut bool, check_null: bool, arg_num: u32) -> bool {
    if check_null {
        *is_null = false;
    }
    match type_of(arg) {
        IS_TRUE => *dest = true,
        IS_FALSE => *dest = false,
        IS_NULL if check_null => {
            *is_null = true;
            *dest = false;
        }
        _ => return zend_parse_arg_bool_slow(arg, dest, arg_num),
    }
    true
}

/// `zend_parse_arg_long`: parses an int argument, optionally nullable (`is_null` reports null);
/// other types take the weak coercion path. Returns false on a type error.
#[no_mangle]
pub unsafe extern "C" fn zend_parse_arg_long(arg: *const zval, dest: *mut zend_long, is_null: *mut bool, check_null: bool, arg_num: u32) -> bool {
    if check_null {
        *is_null = false;
    }
    match type_of(arg) {
        IS_LONG => *dest = (*arg).value.lval,
        IS_NULL if check_null => {
            *is_null = true;
            *dest = 0;
        }
        _ => return zend_parse_arg_long_slow(arg, dest, arg_num),
    }
    true
}

/// `zend_parse_arg_double`: parses a float argument, optionally nullable (`is_null` reports null);
/// other types take the weak coercion path. Returns false on a type error.
#[no_mangle]
pub unsafe extern "C" fn zend_parse_arg_double(arg: *const zval, dest: *mut f64, is_null: *mut bool, check_null: bool, arg_num: u32) -> bool {
    if check_null {
        *is_null = false;
    }
    match type_of(arg) {
        IS_DOUBLE => *dest = (*arg).value.dval,
        IS_NULL if check_null => {
            *is_null = true;
            *dest = 0.0;
        }
        _ => return zend_parse_arg_double_slow(arg, dest, arg_num),
    }
    true
}

/// `zend_parse_arg_number`: parses an `int|float` argument into `*dest`, optionally nullable (NULL
/// for null); strings and other scalars take the slow coercion path.
#[no_mangle]
pub unsafe extern "C" fn zend_parse_arg_number(arg: *mut zval, dest: *mut *mut zval, check_null: bool, arg_num: u32) -> bool {
    match type_of(arg) {
        IS_LONG | IS_DOUBLE => *dest = arg,
        IS_NULL if check_null => *dest = ptr::null_mut(),
        _ => return zend_parse_arg_number_slow(arg, dest, arg_num),
    }
    true
}

/// `zend_parse_arg_number_or_str`: parses an `int|float|string` argument into `*dest`, optionally
/// nullable; other types take the slow coercion path.
#[no_mangle]
pub unsafe extern "C" fn zend_parse_arg_number_or_str(arg: *mut zval, dest: *mut *mut zval, check_null: bool, arg_num: u32) -> bool {
    match type_of(arg) {
        IS_LONG | IS_DOUBLE | IS_STRING => *dest = arg,
        IS_NULL if check_null => *dest = ptr::null_mut(),
        _ => return zend_parse_arg_number_or_str_slow(arg, dest, arg_num),
    }
    true
}

/// `zend_parse_arg_str`: parses a string argument into `*dest`, optionally nullable (NULL for
/// null); other types take the weak coercion path.
#[no_mangle]
pub unsafe extern "C" fn zend_parse_arg_str(arg: *mut zval, dest: *mut *mut zend_string, check_null: bool, arg_num: u32) -> bool {
    match type_of(arg) {
        IS_STRING => *dest = (*arg).value.str,
        IS_NULL if check_null => *dest = ptr::null_mut(),
        _ => return zend_parse_arg_str_slow(arg, dest, arg_num),
    }
    true
}

/// `zend_parse_arg_string`: parses a string argument into a character pointer and length,
/// optionally nullable (NULL and length 0 for null). Returns false on a type error.
#[no_mangle]
pub unsafe extern "C" fn zend_parse_arg_string(
    arg: *mut zval,
    dest: *mut *mut c_char,
    dest_len: *mut usize,
    check_null: bool,
    arg_num: u32,
) -> bool {
    let mut str: *mut zend_string = ptr::null_mut();
    if !zend_parse_arg_str(arg, &mut str, check_null, arg_num) {
        return false;
    }
    if str.is_null() {
        *dest = ptr::null_mut();
        *dest_len = 0;
    } else {
        *dest = (*str).val.as_mut_ptr();
        *dest_len = (*str).len;
    }
    true
}

/// `zend_parse_arg_path_str`: parses a string argument that must not contain NUL bytes, as a path;
/// false otherwise.
#[no_mangle]
pub unsafe extern "C" fn zend_parse_arg_path_str(arg: *mut zval, dest: *mut *mut zend_string, check_null: bool, arg_num: u32) -> bool {
    if !zend_parse_arg_str(arg, dest, check_null, arg_num) {
        return false;
    }
    (*dest).is_null() || !bytes(*dest).contains(&0)
}

/// `zend_parse_arg_path`: parses a path argument into a character pointer and length, optionally
/// nullable; false for a non-string or a string with NUL bytes.
#[no_mangle]
pub unsafe extern "C" fn zend_parse_arg_path(
    arg: *mut zval,
    dest: *mut *mut c_char,
    dest_len: *mut usize,
    check_null: bool,
    arg_num: u32,
) -> bool {
    let mut str: *mut zend_string = ptr::null_mut();
    if !zend_parse_arg_path_str(arg, &mut str, check_null, arg_num) {
        return false;
    }
    if str.is_null() {
        *dest = ptr::null_mut();
        *dest_len = 0;
    } else {
        *dest = (*str).val.as_mut_ptr();
        *dest_len = (*str).len;
    }
    true
}

/// `zend_parse_arg_array`: accepts an array (or an object when `or_object`) argument, optionally
/// null; false otherwise.
#[no_mangle]
pub unsafe extern "C" fn zend_parse_arg_array(arg: *mut zval, dest: *mut *mut zval, check_null: bool, or_object: bool) -> bool {
    match type_of(arg) {
        IS_ARRAY => *dest = arg,
        IS_OBJECT if or_object => *dest = arg,
        IS_NULL if check_null => *dest = ptr::null_mut(),
        _ => return false,
    }
    true
}

/// `zend_parse_arg_array_ht`: accepts an array argument (or the property table of an object when
/// `or_object`), optionally null, yielding its hash table; false otherwise.
#[no_mangle]
pub unsafe extern "C" fn zend_parse_arg_array_ht(
    arg: *const zval,
    dest: *mut *mut HashTable,
    check_null: bool,
    or_object: bool,
    _separate: bool,
) -> bool {
    match type_of(arg) {
        IS_ARRAY => *dest = (*arg).value.arr,
        IS_OBJECT if or_object => *dest = crate::object::zend_elephc_object_properties((*arg).value.obj),
        IS_NULL if check_null => *dest = ptr::null_mut(),
        _ => return false,
    }
    true
}

/// `zend_parse_arg_array_ht_or_long`: accepts an array (hash table) or an int argument, optionally
/// null (reported through `is_null`); other types take the int coercion path.
#[no_mangle]
pub unsafe extern "C" fn zend_parse_arg_array_ht_or_long(
    arg: *mut zval,
    dest_ht: *mut *mut HashTable,
    dest_long: *mut zend_long,
    is_null: *mut bool,
    allow_null: bool,
    arg_num: u32,
) -> bool {
    if allow_null {
        *is_null = false;
    }
    match type_of(arg) {
        IS_ARRAY => *dest_ht = (*arg).value.arr,
        IS_LONG => {
            *dest_ht = ptr::null_mut();
            *dest_long = (*arg).value.lval;
        }
        IS_NULL if allow_null => {
            *dest_ht = ptr::null_mut();
            *is_null = true;
        }
        _ => {
            *dest_ht = ptr::null_mut();
            return zend_parse_arg_long_slow(arg, dest_long, arg_num);
        }
    }
    true
}

/// `zend_parse_arg_object`: accepts an object argument that is an instance of `ce` (any object when
/// `ce` is NULL), optionally null, yielding its zval; false otherwise.
#[no_mangle]
pub unsafe extern "C" fn zend_parse_arg_object(arg: *mut zval, dest: *mut *mut zval, ce: *mut zend_class_entry, check_null: bool) -> bool {
    if type_of(arg) == IS_OBJECT && (ce.is_null() || crate::class::instanceof_function((*(*arg).value.obj).ce, ce)) {
        *dest = arg;
    } else if check_null && type_of(arg) == IS_NULL {
        *dest = ptr::null_mut();
    } else {
        return false;
    }
    true
}

/// `zend_parse_arg_obj`: accepts an object argument that is an instance of `ce` (any object when
/// `ce` is NULL), optionally null, yielding the object pointer; false otherwise.
#[no_mangle]
pub unsafe extern "C" fn zend_parse_arg_obj(arg: *const zval, dest: *mut *mut zend_object, ce: *mut zend_class_entry, check_null: bool) -> bool {
    if type_of(arg) == IS_OBJECT && (ce.is_null() || crate::class::instanceof_function((*(*arg).value.obj).ce, ce)) {
        *dest = (*arg).value.obj;
    } else if check_null && type_of(arg) == IS_NULL {
        *dest = ptr::null_mut();
    } else {
        return false;
    }
    true
}

/// `zend_parse_arg_obj_or_long`: accepts an instance of `ce` or an int argument, optionally null
/// (reported through `is_null`); other types take the int coercion path.
#[no_mangle]
pub unsafe extern "C" fn zend_parse_arg_obj_or_long(
    arg: *mut zval,
    dest_obj: *mut *mut zend_object,
    ce: *mut zend_class_entry,
    dest_long: *mut zend_long,
    is_null: *mut bool,
    allow_null: bool,
    arg_num: u32,
) -> bool {
    if allow_null {
        *is_null = false;
    }
    if type_of(arg) == IS_OBJECT && crate::class::instanceof_function((*(*arg).value.obj).ce, ce) {
        *dest_obj = (*arg).value.obj;
    } else if type_of(arg) == IS_LONG {
        *dest_obj = ptr::null_mut();
        *dest_long = (*arg).value.lval;
    } else if allow_null && type_of(arg) == IS_NULL {
        *dest_obj = ptr::null_mut();
        *is_null = true;
    } else {
        *dest_obj = ptr::null_mut();
        return zend_parse_arg_long_slow(arg, dest_long, arg_num);
    }
    true
}

/// `zend_parse_arg_resource`: accepts a resource argument, optionally null; false otherwise.
#[no_mangle]
pub unsafe extern "C" fn zend_parse_arg_resource(arg: *mut zval, dest: *mut *mut zval, check_null: bool) -> bool {
    match type_of(arg) {
        IS_RESOURCE => *dest = arg,
        IS_NULL if check_null => *dest = ptr::null_mut(),
        _ => return false,
    }
    true
}

/// `zend_parse_arg_str_or_long`: accepts a string or an int argument, optionally null (reported
/// through `is_null`); other types are coerced, and `*dest_str` is NULL when the int was used.
#[no_mangle]
pub unsafe extern "C" fn zend_parse_arg_str_or_long(
    arg: *mut zval,
    dest_str: *mut *mut zend_string,
    dest_long: *mut zend_long,
    is_null: *mut bool,
    allow_null: bool,
    arg_num: u32,
) -> bool {
    if allow_null {
        *is_null = false;
    }
    match type_of(arg) {
        IS_STRING => *dest_str = (*arg).value.str,
        IS_LONG => {
            *dest_str = ptr::null_mut();
            *dest_long = (*arg).value.lval;
        }
        IS_NULL if allow_null => {
            *dest_str = ptr::null_mut();
            *is_null = true;
        }
        _ => return zend_parse_arg_str_or_long_slow(arg, dest_str, dest_long, arg_num),
    }
    true
}

/// `zend_parse_arg_array_ht_or_str`: accepts an array (hash table) or a string argument, optionally
/// null, setting the unused destination to NULL; false otherwise.
#[no_mangle]
pub unsafe extern "C" fn zend_parse_arg_array_ht_or_str(
    arg: *mut zval,
    dest_ht: *mut *mut HashTable,
    dest_str: *mut *mut zend_string,
    allow_null: bool,
    arg_num: u32,
) -> bool {
    match type_of(arg) {
        IS_STRING => {
            *dest_ht = ptr::null_mut();
            *dest_str = (*arg).value.str;
        }
        IS_ARRAY => {
            *dest_ht = (*arg).value.arr;
            *dest_str = ptr::null_mut();
        }
        IS_NULL if allow_null => {
            *dest_ht = ptr::null_mut();
            *dest_str = ptr::null_mut();
        }
        _ => {
            *dest_ht = ptr::null_mut();
            return zend_parse_arg_str_slow(arg, dest_str, arg_num);
        }
    }
    true
}

/// `zend_parse_arg_obj_or_str`: accepts an instance of `base_ce` (any object when NULL) or a string
/// argument, optionally null, setting the unused destination to NULL; other types are coerced to a
/// string.
#[no_mangle]
pub unsafe extern "C" fn zend_parse_arg_obj_or_str(
    arg: *mut zval,
    destination_object: *mut *mut zend_object,
    base_ce: *mut zend_class_entry,
    destination_string: *mut *mut zend_string,
    allow_null: bool,
    arg_num: u32,
) -> bool {
    if type_of(arg) == IS_OBJECT && (base_ce.is_null() || crate::class::instanceof_function((*(*arg).value.obj).ce, base_ce)) {
        *destination_object = (*arg).value.obj;
        *destination_string = ptr::null_mut();
        return true;
    }
    *destination_object = ptr::null_mut();
    zend_parse_arg_str(arg, destination_string, allow_null, arg_num)
}

/// `zend_parse_arg_zval`: yields the argument zval, or NULL for a null argument (also through a
/// reference) when nullable is allowed.
#[no_mangle]
pub unsafe extern "C" fn zend_parse_arg_zval(arg: *mut zval, dest: *mut *mut zval, check_null: bool) {
    let is_null = type_of(arg) == IS_NULL || (type_of(arg) == IS_REFERENCE && type_of(deref(arg)) == IS_NULL);
    *dest = if check_null && is_null { ptr::null_mut() } else { arg };
}

/// `zend_parse_arg_zval_deref`: yields the argument zval, or NULL for a null argument when nullable
/// is allowed.
#[no_mangle]
pub unsafe extern "C" fn zend_parse_arg_zval_deref(arg: *mut zval, dest: *mut *mut zval, check_null: bool) {
    *dest = if check_null && type_of(arg) == IS_NULL { ptr::null_mut() } else { arg };
}

/// `zend_parse_arg_class`: resolves a class-name string argument to its class entry in `*pce`,
/// optionally null. `*pce` on input may name a required base class. It throws a TypeError and
/// returns false for a non-string, an unknown class or a class not derived from the base.
#[no_mangle]
pub unsafe extern "C" fn zend_parse_arg_class(arg: *mut zval, pce: *mut *mut zend_class_entry, num: u32, check_null: bool) -> bool {
    let base = *pce;
    if check_null && type_of(arg) == IS_NULL {
        *pce = ptr::null_mut();
        return true;
    }
    if type_of(arg) != IS_STRING {
        *pce = ptr::null_mut();
        argument_type_error(num, &format!("must be a valid class name, {} given", value_name(arg)));
        return false;
    }
    let ce = crate::class::zend_lookup_class((*arg).value.str);
    if ce.is_null() {
        argument_type_error(num, &format!("must be a valid class name, {} given", string::text((*arg).value.str)));
        *pce = ptr::null_mut();
        return false;
    }
    if !base.is_null() && !crate::class::instanceof_function(ce, base) {
        argument_type_error(
            num,
            &format!("must be a class name derived from {}, {} given", string::text((*base).name), string::text((*arg).value.str)),
        );
        *pce = ptr::null_mut();
        return false;
    }
    *pce = ce;
    true
}

/// Callables cannot cross into a hosted extension: Elephc compiles them to
/// native code the extension has no way to call.
#[no_mangle]
pub unsafe extern "C" fn zend_parse_arg_func(
    arg: *mut zval,
    dest_fci: *mut zend_fcall_info,
    dest_fcc: *mut zend_fcall_info_cache,
    check_null: bool,
    error: *mut *mut c_char,
    _free_trampoline: bool,
) -> bool {
    if check_null && type_of(arg) == IS_NULL {
        (*dest_fci).size = 0;
        (*dest_fcc).function_handler = ptr::null_mut();
        *error = ptr::null_mut();
        return true;
    }
    if crate::host::init_hosted_fcall(arg, dest_fci, dest_fcc) {
        *error = ptr::null_mut();
        return true;
    }
    *error = crate::alloc::estrdup(c"no array or string given".as_ptr());
    false
}

/// `zend_parse_arg_iterable`: accepts an array or Traversable argument, optionally null, yielding
/// its zval; false otherwise.
#[no_mangle]
pub unsafe extern "C" fn zend_parse_arg_iterable(arg: *mut zval, dest: *mut *mut zval, check_null: bool) -> bool {
    if crate::convert::zend_is_iterable(arg) {
        *dest = arg;
        return true;
    }
    if check_null && type_of(arg) == IS_NULL {
        *dest = ptr::null_mut();
        return true;
    }
    false
}

// ------------------------------------------------------------ classic parsing

/// Parses one argument against one spec letter (with its modifiers),
/// consuming its output slots. Returns the expected-type text on failure
/// (`None` when the failure was already reported).
unsafe fn parse_one(
    arg: *mut zval,
    spec: &[u8],
    slots: &mut std::slice::Iter<*mut c_void>,
    arg_num: u32,
    quiet: bool,
) -> Result<(), Option<String>> {
    let letter = spec[0];
    let check_null = spec[1..].contains(&b'!');
    let separate = spec[1..].contains(&b'/');
    let real = arg;
    let arg = deref(arg);
    if separate && type_of(arg) == IS_ARRAY && (*(*arg).value.arr).gc.refcount > 1 {
        let copy = crate::hash::zend_array_dup((*arg).value.arr);
        (*(*arg).value.arr).gc.refcount -= 1;
        set_arr(arg, copy);
    }
    let mut next = || slots.next().copied().unwrap_or(ptr::null_mut());
    let expected = |index: c_int| Err(Some(EXPECTED[index as usize].to_string()));
    match letter {
        b'l' => {
            let dest = next().cast::<zend_long>();
            let mut is_null = false;
            let ok = zend_parse_arg_long(arg, dest, &mut is_null, check_null, arg_num);
            if check_null {
                *next().cast::<bool>() = is_null;
            }
            if !ok {
                return expected(if check_null { Z_EXPECTED_LONG_OR_NULL } else { Z_EXPECTED_LONG });
            }
        }
        b'd' => {
            let dest = next().cast::<f64>();
            let mut is_null = false;
            let ok = zend_parse_arg_double(arg, dest, &mut is_null, check_null, arg_num);
            if check_null {
                *next().cast::<bool>() = is_null;
            }
            if !ok {
                return expected(if check_null { Z_EXPECTED_DOUBLE_OR_NULL } else { Z_EXPECTED_DOUBLE });
            }
        }
        b'b' => {
            let dest = next().cast::<bool>();
            let mut is_null = false;
            let ok = zend_parse_arg_bool(arg, dest, &mut is_null, check_null, arg_num);
            if check_null {
                *next().cast::<bool>() = is_null;
            }
            if !ok {
                return expected(if check_null { Z_EXPECTED_BOOL_OR_NULL } else { Z_EXPECTED_BOOL });
            }
        }
        b's' | b'p' => {
            let dest = next().cast::<*mut c_char>();
            let len = next().cast::<usize>();
            let ok = if letter == b'p' {
                zend_parse_arg_path(arg, dest, len, check_null, arg_num)
            } else {
                zend_parse_arg_string(arg, dest, len, check_null, arg_num)
            };
            if !ok {
                if letter == b'p' && type_of(arg) == IS_STRING {
                    if !quiet {
                        throw_error(zend_ce_value_error, &argument_message(arg_num, "must not contain any null bytes"));
                    }
                    return Err(None);
                }
                return expected(if check_null { Z_EXPECTED_STRING_OR_NULL } else { Z_EXPECTED_STRING });
            }
        }
        b'S' | b'P' => {
            let dest = next().cast::<*mut zend_string>();
            let ok = if letter == b'P' {
                zend_parse_arg_path_str(arg, dest, check_null, arg_num)
            } else {
                zend_parse_arg_str(arg, dest, check_null, arg_num)
            };
            if !ok {
                if letter == b'P' && type_of(arg) == IS_STRING {
                    if !quiet {
                        throw_error(zend_ce_value_error, &argument_message(arg_num, "must not contain any null bytes"));
                    }
                    return Err(None);
                }
                return expected(if check_null { Z_EXPECTED_STRING_OR_NULL } else { Z_EXPECTED_STRING });
            }
        }
        b'n' => {
            let dest = next().cast::<*mut zval>();
            if !zend_parse_arg_number(arg, dest, check_null, arg_num) {
                return expected(if check_null { Z_EXPECTED_NUMBER_OR_NULL } else { Z_EXPECTED_NUMBER });
            }
        }
        b'a' | b'A' => {
            let dest = next().cast::<*mut zval>();
            if !zend_parse_arg_array(arg, dest, check_null, letter == b'A') {
                return expected(if check_null { Z_EXPECTED_ARRAY_OR_NULL } else { Z_EXPECTED_ARRAY });
            }
        }
        b'h' | b'H' => {
            let dest = next().cast::<*mut HashTable>();
            if !zend_parse_arg_array_ht(arg, dest, check_null, letter == b'H', separate) {
                return expected(if check_null { Z_EXPECTED_ARRAY_OR_NULL } else { Z_EXPECTED_ARRAY });
            }
        }
        b'o' => {
            let dest = next().cast::<*mut zval>();
            if !zend_parse_arg_object(arg, dest, ptr::null_mut(), check_null) {
                return expected(if check_null { Z_EXPECTED_OBJECT_OR_NULL } else { Z_EXPECTED_OBJECT });
            }
        }
        b'O' => {
            let dest = next().cast::<*mut zval>();
            let ce = next().cast::<zend_class_entry>();
            if !zend_parse_arg_object(arg, dest, ce, check_null) {
                if ce.is_null() {
                    return expected(if check_null { Z_EXPECTED_OBJECT_OR_NULL } else { Z_EXPECTED_OBJECT });
                }
                let class = string::text((*ce).name);
                return Err(Some(if check_null { format!("of type ?{class}") } else { format!("of type {class}") }));
            }
        }
        b'C' => {
            let dest = next().cast::<*mut zend_class_entry>();
            if !zend_parse_arg_class(arg, dest, arg_num, check_null) {
                return Err(None);
            }
        }
        b'r' => {
            let dest = next().cast::<*mut zval>();
            if !zend_parse_arg_resource(arg, dest, check_null) {
                return expected(if check_null { Z_EXPECTED_RESOURCE_OR_NULL } else { Z_EXPECTED_RESOURCE });
            }
        }
        b'z' => {
            let dest = next().cast::<*mut zval>();
            zend_parse_arg_zval_deref(real, dest, check_null);
        }
        b'f' => {
            let fci = next().cast::<zend_fcall_info>();
            let fcc = next().cast::<zend_fcall_info_cache>();
            let mut error: *mut c_char = ptr::null_mut();
            if !zend_parse_arg_func(arg, fci, fcc, check_null, &mut error, true) {
                // Quiet parsing reports nothing, as php-src's: the detail is only freed.
                if quiet {
                    if !error.is_null() {
                        crate::alloc::free(error.cast());
                    }
                } else if check_null {
                    zend_wrong_callback_or_null_error(arg_num, error);
                } else {
                    zend_wrong_callback_error(arg_num, error);
                }
                return Err(None);
            }
        }
        _ => crate::error::unsupported("zend_parse_parameters() with an unknown type spec"),
    }
    Ok(())
}

/// Splits a spec into one entry per argument letter (letter + modifiers),
/// noting where `|` starts the optional ones and where a variadic is.
fn split_spec(spec: &[u8]) -> (Vec<&[u8]>, usize, Option<(usize, bool)>) {
    let mut entries = Vec::new();
    let mut min = None;
    let mut variadic = None;
    let mut i = 0;
    while i < spec.len() {
        let c = spec[i];
        if c == b'|' {
            min = Some(entries.len());
            i += 1;
            continue;
        }
        let start = i;
        i += 1;
        while i < spec.len() && (spec[i] == b'!' || spec[i] == b'/') {
            i += 1;
        }
        if c == b'*' || c == b'+' {
            variadic = Some((entries.len(), c == b'+'));
        }
        entries.push(&spec[start..i]);
    }
    let min = min.unwrap_or(entries.len());
    (entries, min, variadic)
}

/// The first argument slot of the active call frame, which follows the execute-data header.
unsafe fn frame_args() -> *mut zval {
    let frame = eg().current_execute_data;
    (frame as *mut zval).add(((std::mem::size_of::<zend_execute_data>() + 15) / 16) as usize)
}

/// Parses the active call's arguments against a `zend_parse_parameters` type spec, writing each
/// converted value to the matching destination slot. A leading `O` binds `$this`. Returns SUCCESS
/// or FAILURE, throwing the argument errors unless `ZEND_PARSE_PARAMS_QUIET` is set.
unsafe fn parse_all(flags: c_int, num_args: u32, spec: &[u8], slots: &[*mut c_void], this_ptr: *mut zval) -> zend_result {
    let quiet = flags & ZEND_PARSE_PARAMS_QUIET != 0;
    let mut spec = spec;
    let mut slots = slots.iter();
    if !this_ptr.is_null() && type_of(this_ptr) == IS_OBJECT && spec.first() == Some(&b'O') {
        let dest = slots.next().copied().unwrap_or(ptr::null_mut()).cast::<*mut zval>();
        let ce = slots.next().copied().unwrap_or(ptr::null_mut()).cast::<zend_class_entry>();
        if !ce.is_null() && !crate::class::instanceof_function((*(*this_ptr).value.obj).ce, ce) {
            crate::error::fatal(&format!(
                "{}::{}() must be derived from {}::{}()",
                string::text((*(*(*this_ptr).value.obj).ce).name),
                crate::error::active_function_prefix().unwrap_or_default(),
                string::text((*ce).name),
                crate::error::active_function_prefix().unwrap_or_default()
            ));
        }
        *dest = this_ptr;
        spec = &spec[1..];
    }
    let (entries, min, variadic) = split_spec(spec);
    let fixed = entries.len() - variadic.map_or(0, |_| 1);
    let max = if variadic.is_some() { u32::MAX } else { entries.len() as u32 };
    let min_required = (min as u32).min(fixed as u32) + variadic.map_or(0, |(_, plus)| plus as u32);
    if num_args < min_required || num_args > max {
        if !quiet {
            zend_wrong_parameters_count_error(min_required, if max == u32::MAX { u32::MAX } else { max });
        }
        return FAILURE;
    }
    let args = frame_args();
    let mut arg_index: u32 = 0;
    let post_variadic = variadic.map_or(0, |(at, _)| entries.len() - at - 1) as u32;
    for (entry_index, entry) in entries.iter().enumerate() {
        if entry[0] == b'*' || entry[0] == b'+' {
            let count = num_args.saturating_sub(arg_index + post_variadic);
            let dest = slots.next().copied().unwrap_or(ptr::null_mut()).cast::<*mut zval>();
            let dest_count = slots.next().copied().unwrap_or(ptr::null_mut()).cast::<u32>();
            *dest = if count > 0 { args.add(arg_index as usize) } else { ptr::null_mut() };
            *dest_count = count;
            arg_index += count;
            continue;
        }
        if arg_index >= num_args {
            break;
        }
        let _ = entry_index;
        let arg = args.add(arg_index as usize);
        match parse_one(arg, entry, &mut slots, arg_index + 1, quiet) {
            Ok(()) => {}
            Err(expected) => {
                if !quiet {
                    if let Some(expected) = expected {
                        if eg().exception.is_null() {
                            argument_type_error(arg_index + 1, &format!("must be {expected}, {} given", value_name(arg)));
                        }
                    }
                }
                return FAILURE;
            }
        }
        arg_index += 1;
    }
    SUCCESS
}

/// `zend_parse_parameters` and friends, after the C side gathered the output
/// pointers in spec order.
/// c: hidden
#[no_mangle]
pub unsafe extern "C" fn elephc_zend_parse_args(
    flags: c_int,
    num_args: u32,
    spec: *const c_char,
    slots: *mut *mut c_void,
    count: usize,
    this_ptr: *mut zval,
) -> c_int {
    let spec = CStr::from_ptr(spec).to_bytes();
    let slots = std::slice::from_raw_parts(slots, count);
    parse_all(flags, num_args, spec, slots, this_ptr)
}

/// `zend_parse_parameter`: one argument against one spec.
/// c: hidden
#[no_mangle]
pub unsafe extern "C" fn elephc_zend_parse_one_arg(
    flags: c_int,
    arg_num: u32,
    arg: *mut zval,
    spec: *const c_char,
    slots: *mut *mut c_void,
    count: usize,
) -> c_int {
    let spec = CStr::from_ptr(spec).to_bytes();
    let slots = std::slice::from_raw_parts(slots, count);
    let mut iter = slots.iter();
    match parse_one(arg, spec, &mut iter, arg_num, flags & ZEND_PARSE_PARAMS_QUIET != 0) {
        Ok(()) => SUCCESS,
        Err(expected) => {
            if flags & ZEND_PARSE_PARAMS_QUIET == 0 {
                if let Some(expected) = expected {
                    argument_type_error(arg_num, &format!("must be {expected}, {} given", value_name(arg)));
                }
            }
            FAILURE
        }
    }
}
