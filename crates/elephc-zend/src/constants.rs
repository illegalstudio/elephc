//! Purpose:
//! Global constants: `REGISTER_*_CONSTANT` from extension MINIT, lookup
//! (`zend_get_constant`), and the core constants extensions consult.
//!
//! Called from:
//! - Extension code, `crate::startup`, and `crate::describe` (which lists a
//!   module's constants for the generated declarations).
//!
//! Key details:
//! - Constants live in `EG(zend_constants)` under their exact name (case
//!   matters, as in PHP 8); the owning module's number is in the value's
//!   `u2.constant_flags` above the flag byte, read by
//!   `ZEND_CONSTANT_MODULE_NUMBER`.

use std::ffi::{c_char, c_int, CStr};
use std::ptr;

use crate::abi::*;
use crate::alloc::alloc_struct;
use crate::hash;
use crate::startup::eg;
use crate::string::{self, intern};
use crate::zval::*;

/// `zend_register_constant`: copies the constant, interns its name and adds it to the global
/// constant table; on a duplicate name it warns, frees the value and returns NULL.
#[no_mangle]
pub unsafe extern "C" fn zend_register_constant(c: *mut zend_constant) -> *mut zend_constant {
    let stored = alloc_struct::<zend_constant>();
    ptr::copy_nonoverlapping(c, stored, 1);
    (*stored).name = intern(string::bytes((*c).name));
    if hash::zend_hash_add_ptr(eg().zend_constants, (*stored).name, stored.cast()).is_null() {
        crate::error::warning(&format!("Constant {} already defined", string::text((*stored).name)));
        zval_ptr_dtor(&mut (*stored).value);
        return ptr::null_mut();
    }
    stored
}

/// Builds a `zend_constant` from a name, value and flags (module number packed into the upper bits
/// of the flags word) and registers it.
unsafe fn register(name: *const c_char, name_len: usize, mut value: zval, flags: c_int, module_number: c_int) -> *mut zend_constant {
    let mut constant: zend_constant = std::mem::zeroed();
    copy_value(&mut constant.value, &mut value);
    constant.value.u2.constant_flags = ((flags & 0xff) as u32) | ((module_number as u32) << 8);
    constant.name = string::zend_string_init_interned(name, name_len, true);
    zend_register_constant(&mut constant)
}

/// `zend_register_null_constant`: registers a NULL-valued constant.
#[no_mangle]
pub unsafe extern "C" fn zend_register_null_constant(name: *const c_char, name_len: usize, flags: c_int, module_number: c_int) -> *mut zend_constant {
    register(name, name_len, null(), flags, module_number)
}

/// `zend_register_bool_constant`: registers a boolean constant.
#[no_mangle]
pub unsafe extern "C" fn zend_register_bool_constant(
    name: *const c_char,
    name_len: usize,
    bval: bool,
    flags: c_int,
    module_number: c_int,
) -> *mut zend_constant {
    let mut value = zval::UNDEF;
    set_bool(&mut value, bval);
    register(name, name_len, value, flags, module_number)
}

/// `zend_register_long_constant`: registers an integer constant.
#[no_mangle]
pub unsafe extern "C" fn zend_register_long_constant(
    name: *const c_char,
    name_len: usize,
    lval: zend_long,
    flags: c_int,
    module_number: c_int,
) -> *mut zend_constant {
    register(name, name_len, long(lval), flags, module_number)
}

/// `zend_register_double_constant`: registers a float constant.
#[no_mangle]
pub unsafe extern "C" fn zend_register_double_constant(
    name: *const c_char,
    name_len: usize,
    dval: f64,
    flags: c_int,
    module_number: c_int,
) -> *mut zend_constant {
    let mut value = zval::UNDEF;
    set_double(&mut value, dval);
    register(name, name_len, value, flags, module_number)
}

/// `zend_register_stringl_constant`: registers a string constant from a pointer and length, stored
/// as an interned persistent string.
#[no_mangle]
pub unsafe extern "C" fn zend_register_stringl_constant(
    name: *const c_char,
    name_len: usize,
    strval: *const c_char,
    strlen: usize,
    flags: c_int,
    module_number: c_int,
) -> *mut zend_constant {
    let mut value = zval::UNDEF;
    set_str(&mut value, string::zend_string_init_interned(strval, strlen, true));
    register(name, name_len, value, flags, module_number)
}

/// `zend_register_string_constant`: registers a string constant from a NUL-terminated C string.
#[no_mangle]
pub unsafe extern "C" fn zend_register_string_constant(
    name: *const c_char,
    name_len: usize,
    strval: *const c_char,
    flags: c_int,
    module_number: c_int,
) -> *mut zend_constant {
    zend_register_stringl_constant(name, name_len, strval, CStr::from_ptr(strval).to_bytes().len(), flags, module_number)
}

/// The constant `name`: exact first, then `true`/`false`/`null` in any case,
/// and a namespaced name's namespace part case-insensitively.
unsafe fn find(name: &[u8]) -> *mut zval {
    let name = name.strip_prefix(b"\\").unwrap_or(name);
    let table = eg().zend_constants;
    let mut constant = hash::zend_hash_str_find_ptr(table, name.as_ptr().cast(), name.len()).cast::<zend_constant>();
    if constant.is_null() {
        if let Some(slash) = name.iter().rposition(|&b| b == b'\\') {
            let mut lowered = name[..slash].to_ascii_lowercase();
            lowered.extend_from_slice(&name[slash..]);
            constant = hash::zend_hash_str_find_ptr(table, lowered.as_ptr().cast(), lowered.len()).cast();
        }
    }
    if constant.is_null() {
        let lower = name.to_ascii_lowercase();
        if matches!(lower.as_slice(), b"true" | b"false" | b"null") {
            constant = hash::zend_hash_str_find_ptr(table, lower.as_ptr().cast(), lower.len()).cast();
        }
    }
    if constant.is_null() {
        ptr::null_mut()
    } else {
        &mut (*constant).value
    }
}

/// `zend_get_constant_str`: looks up a constant by pointer and length; returns its value or NULL.
#[no_mangle]
pub unsafe extern "C" fn zend_get_constant_str(name: *const c_char, name_len: usize) -> *mut zval {
    find(std::slice::from_raw_parts(name.cast::<u8>(), name_len))
}

/// `zend_get_constant`: looks up a constant by `zend_string` name; returns its value or NULL.
#[no_mangle]
pub unsafe extern "C" fn zend_get_constant(name: *mut zend_string) -> *mut zval {
    find(string::bytes(name))
}

/// `zend_get_constant_ex`: looks up a constant, resolving `Class::NAME` through the class's
/// constants table (NULL when the class or constant is missing) and otherwise as a global constant.
#[no_mangle]
pub unsafe extern "C" fn zend_get_constant_ex(name: *mut zend_string, _scope: *mut zend_class_entry, flags: u32) -> *mut zval {
    let text = string::bytes(name);
    if let Some(at) = text.windows(2).position(|w| w == b"::") {
        let class = crate::class::lookup(&text[..at]);
        if class.is_null() {
            return ptr::null_mut();
        }
        let constant_name = &text[at + 2..];
        let constant = hash::zend_hash_str_find_ptr(&(*class).constants_table, constant_name.as_ptr().cast(), constant_name.len())
            .cast::<zend_class_constant>();
        return if constant.is_null() { ptr::null_mut() } else { &mut (*constant).value };
    }
    let _ = flags;
    find(text)
}

/// The constants a PHP program always has that extensions read.
pub unsafe fn register_core_constants() {
    let persistent = CONST_PERSISTENT | CONST_NO_FILE_CACHE;
    let longs: &[(&str, zend_long)] = &[
        ("PHP_MAJOR_VERSION", PHP_MAJOR_VERSION as zend_long),
        ("PHP_MINOR_VERSION", PHP_MINOR_VERSION as zend_long),
        ("PHP_RELEASE_VERSION", PHP_RELEASE_VERSION as zend_long),
        ("PHP_VERSION_ID", PHP_VERSION_ID as zend_long),
        ("PHP_INT_MAX", ZEND_LONG_MAX),
        ("PHP_INT_MIN", ZEND_LONG_MIN),
        ("PHP_INT_SIZE", 8),
        ("PHP_DEBUG", 0),
        ("PHP_ZTS", 0),
        ("E_ERROR", E_ERROR as zend_long),
        ("E_WARNING", E_WARNING as zend_long),
        ("E_PARSE", E_PARSE as zend_long),
        ("E_NOTICE", E_NOTICE as zend_long),
        ("E_CORE_ERROR", E_CORE_ERROR as zend_long),
        ("E_CORE_WARNING", E_CORE_WARNING as zend_long),
        ("E_COMPILE_ERROR", E_COMPILE_ERROR as zend_long),
        ("E_COMPILE_WARNING", E_COMPILE_WARNING as zend_long),
        ("E_USER_ERROR", E_USER_ERROR as zend_long),
        ("E_USER_WARNING", E_USER_WARNING as zend_long),
        ("E_USER_NOTICE", E_USER_NOTICE as zend_long),
        ("E_RECOVERABLE_ERROR", E_RECOVERABLE_ERROR as zend_long),
        ("E_DEPRECATED", E_DEPRECATED as zend_long),
        ("E_USER_DEPRECATED", E_USER_DEPRECATED as zend_long),
        ("E_ALL", E_ALL as zend_long),
    ];
    for (name, value) in longs {
        zend_register_long_constant(name.as_ptr().cast(), name.len(), *value, persistent, 0);
    }
    let strings: &[(&str, &str)] = &[
        ("PHP_VERSION", PHP_VERSION),
        ("PHP_EXTRA_VERSION", PHP_EXTRA_VERSION),
        ("PHP_OS", PHP_OS),
        ("PHP_OS_FAMILY", PHP_OS_FAMILY),
        ("PHP_EOL", PHP_EOL),
        ("PHP_SAPI", "cli"),
        ("DEFAULT_INCLUDE_PATH", ".:"),
        ("PHP_SHLIB_SUFFIX", PHP_SHLIB_SUFFIX),
    ];
    for (name, value) in strings {
        zend_register_stringl_constant(name.as_ptr().cast(), name.len(), value.as_ptr().cast(), value.len(), persistent, 0);
    }
    zend_register_double_constant(c"PHP_FLOAT_EPSILON".as_ptr(), 17, f64::EPSILON, persistent, 0);
    zend_register_double_constant(c"PHP_FLOAT_MAX".as_ptr(), 13, f64::MAX, persistent, 0);
    zend_register_double_constant(c"PHP_FLOAT_MIN".as_ptr(), 13, f64::MIN_POSITIVE, persistent, 0);
    zend_register_long_constant(c"PHP_FLOAT_DIG".as_ptr(), 13, 15, persistent, 0);
    zend_register_bool_constant(c"true".as_ptr(), 4, true, persistent, 0);
    zend_register_bool_constant(c"false".as_ptr(), 5, false, persistent, 0);
    zend_register_null_constant(c"null".as_ptr(), 4, persistent, 0);
}
