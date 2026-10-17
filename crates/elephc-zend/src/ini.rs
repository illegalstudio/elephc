//! Purpose:
//! INI directives: the configuration `[extension.ini]` sets before an
//! extension starts, the directives an extension registers
//! (`REGISTER_INI_ENTRIES`), their update handlers (`OnUpdateLong`...), and
//! reading them back (`INI_INT`, `INI_STR`).
//!
//! Called from:
//! - `crate::host` (`elephc_php_ext_ini`), extension MINIT code, and
//!   `crate::describe` (each directive's default).
//!
//! Key details:
//! - Like php.ini, a configured value is what the directive starts with; if
//!   its handler refuses it, the default is used instead. A value set after
//!   the directive is registered is applied through its handler at once.
//! - Directives live in `EG(ini_directives)` under their name.

use std::collections::HashMap;
use std::ffi::{c_char, c_int, c_void, CStr};
use std::ptr;

use crate::abi::*;
use crate::alloc::alloc_struct;
use crate::global::Global;
use crate::hash;
use crate::startup::eg;
use crate::string::{self, bytes};

static CONFIGURATION: Global<Option<HashMap<Vec<u8>, *mut zval>>> = Global::new(None);

/// The process-wide map of php.ini settings (name to value zval) recorded before or after
/// extensions register their directives, created on first use.
fn configuration() -> &'static mut HashMap<Vec<u8>, *mut zval> {
    unsafe { CONFIGURATION.get().get_or_insert_with(HashMap::new) }
}

/// Records a php.ini line; applies it at once to a registered directive.
pub unsafe fn set_configuration(name: &[u8], value: &[u8]) {
    let slot = alloc_struct::<zval>();
    crate::zval::set_str(slot, string::intern(value));
    configuration().insert(name.to_vec(), slot);
    let directives = eg().ini_directives;
    if !directives.is_null() && !hash::zend_hash_str_find(directives, name.as_ptr().cast(), name.len()).is_null() {
        let key = string::new(name);
        zend_alter_ini_entry_chars(key, value.as_ptr().cast(), value.len(), ZEND_INI_SYSTEM, ZEND_INI_STAGE_RUNTIME);
        string::zend_string_release(key);
    }
}

/// `zend_get_configuration_directive`: the php.ini value recorded for a setting name, or NULL when
/// none was configured.
#[no_mangle]
pub unsafe extern "C" fn zend_get_configuration_directive(name: *mut zend_string) -> *mut zval {
    configuration().get(bytes(name)).copied().unwrap_or(ptr::null_mut())
}

/// `cfg_get_entry_ex`: configuration lookup by `zend_string` name; same as
/// `zend_get_configuration_directive`.
#[no_mangle]
pub unsafe extern "C" fn cfg_get_entry_ex(name: *mut zend_string) -> *mut zval {
    zend_get_configuration_directive(name)
}

/// `cfg_get_entry`: configuration lookup by pointer and length; NULL when the setting is not
/// configured.
#[no_mangle]
pub unsafe extern "C" fn cfg_get_entry(name: *const c_char, name_length: usize) -> *mut zval {
    let key = std::slice::from_raw_parts(name.cast::<u8>(), name_length);
    configuration().get(key).copied().unwrap_or(ptr::null_mut())
}

/// `cfg_get_long`: reads a configured setting as an integer into `*result`, returning SUCCESS, or
/// FAILURE when it is not configured.
#[no_mangle]
pub unsafe extern "C" fn cfg_get_long(varname: *const c_char, result: *mut zend_long) -> c_int {
    match configuration().get(CStr::from_ptr(varname).to_bytes()) {
        Some(value) => {
            *result = crate::convert::zval_get_long_func(*value, false);
            SUCCESS
        }
        None => {
            *result = 0;
            FAILURE
        }
    }
}

/// `cfg_get_double`: reads a configured setting as a float into `*result`, returning SUCCESS, or
/// FAILURE when it is not configured.
#[no_mangle]
pub unsafe extern "C" fn cfg_get_double(varname: *const c_char, result: *mut f64) -> c_int {
    match configuration().get(CStr::from_ptr(varname).to_bytes()) {
        Some(value) => {
            *result = crate::convert::zval_get_double_func(*value);
            SUCCESS
        }
        None => {
            *result = 0.0;
            FAILURE
        }
    }
}

/// `cfg_get_string`: stores a pointer to a configured setting's string in `*result`, returning
/// SUCCESS, or FAILURE when it is not configured.
#[no_mangle]
pub unsafe extern "C" fn cfg_get_string(varname: *const c_char, result: *mut *mut c_char) -> c_int {
    match configuration().get(CStr::from_ptr(varname).to_bytes()) {
        Some(value) => {
            *result = (*(**value).value.str).val.as_mut_ptr();
            SUCCESS
        }
        None => {
            *result = ptr::null_mut();
            FAILURE
        }
    }
}

/// Runs a directive's `on_modify` handler for a new value at the given stage and reports whether it
/// accepted it; a directive without a handler accepts everything.
unsafe fn modify(entry: *mut zend_ini_entry, value: *mut zend_string, stage: c_int) -> bool {
    match (*entry).on_modify {
        Some(handler) => handler(entry, value, (*entry).mh_arg1, (*entry).mh_arg2, (*entry).mh_arg3, stage) == SUCCESS,
        None => true,
    }
}

/// `zend_register_ini_entries_ex`: registers a module's NULL-terminated array of INI directive
/// definitions. Each directive is added to the directive table (a duplicate name raises an
/// `E_CORE_WARNING` and is skipped) and takes its value from the recorded php.ini configuration
/// when its handler accepts it, otherwise from its default.
#[no_mangle]
pub unsafe extern "C" fn zend_register_ini_entries_ex(ini_entry: *const zend_ini_entry_def, module_number: c_int, _module_type: c_int) -> zend_result {
    let directives = eg().ini_directives;
    let mut def = ini_entry;
    while !(*def).name.is_null() {
        let entry = alloc_struct::<zend_ini_entry>();
        let name = std::slice::from_raw_parts((*def).name.cast::<u8>(), (*def).name_length as usize);
        (*entry).name = string::intern(name);
        (*entry).on_modify = (*def).on_modify;
        (*entry).mh_arg1 = (*def).mh_arg1;
        (*entry).mh_arg2 = (*def).mh_arg2;
        (*entry).mh_arg3 = (*def).mh_arg3;
        (*entry).displayer = (*def).displayer;
        (*entry).modifiable = (*def).modifiable;
        (*entry).orig_modifiable = (*def).modifiable;
        (*entry).module_number = module_number;
        let default = if (*def).value.is_null() {
            ptr::null_mut()
        } else {
            string::intern(std::slice::from_raw_parts((*def).value.cast::<u8>(), (*def).value_length as usize))
        };
        if hash::zend_hash_add_ptr(directives, (*entry).name, entry.cast()).is_null() {
            crate::error::report(
                E_CORE_WARNING,
                format!("Duplicate INI entry {}", String::from_utf8_lossy(name)).as_bytes(),
            );
            def = def.add(1);
            continue;
        }
        let configured = configuration().get(name).map(|value| (**value).value.str);
        let mut applied = false;
        if let Some(value) = configured {
            if modify(entry, value, ZEND_INI_STAGE_STARTUP) {
                (*entry).value = value;
                applied = true;
            }
        }
        if !applied {
            (*entry).value = default;
            if !default.is_null() {
                modify(entry, default, ZEND_INI_STAGE_STARTUP);
            }
        }
        def = def.add(1);
    }
    SUCCESS
}

/// `zend_register_ini_entries`: registers a module's INI directives as persistent.
#[no_mangle]
pub unsafe extern "C" fn zend_register_ini_entries(ini_entry: *const zend_ini_entry_def, module_number: c_int) -> zend_result {
    zend_register_ini_entries_ex(ini_entry, module_number, MODULE_PERSISTENT)
}

/// `zend_unregister_ini_entries_ex`: removes every INI directive that belongs to the given module
/// number.
#[no_mangle]
pub unsafe extern "C" fn zend_unregister_ini_entries_ex(module_number: c_int, _module_type: c_int) {
    let directives = eg().ini_directives;
    let mut doomed = Vec::new();
    for idx in 0..(*directives).nNumUsed {
        let p = hash::bucket(directives, idx);
        if (*p).val.u1.v.r#type == IS_UNDEF {
            continue;
        }
        let entry = (*p).val.value.ptr.cast::<zend_ini_entry>();
        if (*entry).module_number == module_number {
            doomed.push((*p).key);
        }
    }
    for key in doomed {
        hash::zend_hash_del(directives, key);
    }
}

/// `zend_unregister_ini_entries`: removes a persistent module's INI directives.
#[no_mangle]
pub unsafe extern "C" fn zend_unregister_ini_entries(module_number: c_int) {
    zend_unregister_ini_entries_ex(module_number, MODULE_PERSISTENT);
}

/// `display_ini_entries`: no-op, because the engine has no phpinfo output.
#[no_mangle]
pub unsafe extern "C" fn display_ini_entries(_module: *mut zend_module_entry) {}

/// Finds a registered INI directive by name, or NULL when the table does not exist or has no such
/// directive.
unsafe fn entry(name: &[u8]) -> *mut zend_ini_entry {
    let directives = eg().ini_directives;
    if directives.is_null() {
        return ptr::null_mut();
    }
    hash::zend_hash_str_find_ptr(directives, name.as_ptr().cast(), name.len()).cast()
}

/// `zend_alter_ini_entry_ex`: changes an INI directive's value at runtime. Fails when the directive
/// is unknown, when its modifiable level excludes `modify_type` (unless `force_change`), or when
/// its handler rejects the value. The original value is saved on the first change for
/// `zend_restore_ini_entry`.
#[no_mangle]
pub unsafe extern "C" fn zend_alter_ini_entry_ex(
    name: *mut zend_string,
    new_value: *mut zend_string,
    modify_type: c_int,
    stage: c_int,
    force_change: bool,
) -> zend_result {
    let entry = entry(bytes(name));
    if entry.is_null() {
        return FAILURE;
    }
    if !force_change && (*entry).modifiable as c_int & modify_type == 0 {
        return FAILURE;
    }
    let value = string::intern(bytes(new_value));
    if !modify(entry, value, stage) {
        return FAILURE;
    }
    if (*entry).modified == 0 {
        (*entry).orig_value = (*entry).value;
        (*entry).orig_modifiable = (*entry).modifiable;
        (*entry).modified = 1;
    }
    (*entry).value = value;
    SUCCESS
}

/// `zend_alter_ini_entry`: `zend_alter_ini_entry_ex` without forcing the change.
#[no_mangle]
pub unsafe extern "C" fn zend_alter_ini_entry(name: *mut zend_string, new_value: *mut zend_string, modify_type: c_int, stage: c_int) -> zend_result {
    zend_alter_ini_entry_ex(name, new_value, modify_type, stage, false)
}

/// `zend_alter_ini_entry_chars`: alters an INI directive from a pointer and length value, without
/// forcing the change.
#[no_mangle]
pub unsafe extern "C" fn zend_alter_ini_entry_chars(
    name: *mut zend_string,
    value: *const c_char,
    value_length: usize,
    modify_type: c_int,
    stage: c_int,
) -> zend_result {
    let new_value = string::zend_string_init(value, value_length, false);
    let result = zend_alter_ini_entry_ex(name, new_value, modify_type, stage, false);
    string::zend_string_release(new_value);
    result
}

/// `zend_alter_ini_entry_chars_ex`: alters an INI directive from a pointer and length value,
/// optionally forcing the change.
#[no_mangle]
pub unsafe extern "C" fn zend_alter_ini_entry_chars_ex(
    name: *mut zend_string,
    value: *const c_char,
    value_length: usize,
    modify_type: c_int,
    stage: c_int,
    force_change: c_int,
) -> zend_result {
    let new_value = string::zend_string_init(value, value_length, false);
    let result = zend_alter_ini_entry_ex(name, new_value, modify_type, stage, force_change != 0);
    string::zend_string_release(new_value);
    result
}

/// `zend_restore_ini_entry`: puts an altered directive back to its original value and modifiable
/// level; FAILURE when it is unknown or was never changed.
#[no_mangle]
pub unsafe extern "C" fn zend_restore_ini_entry(name: *mut zend_string, stage: c_int) -> zend_result {
    let entry = entry(bytes(name));
    if entry.is_null() || (*entry).modified == 0 {
        return FAILURE;
    }
    modify(entry, (*entry).orig_value, stage);
    (*entry).value = (*entry).orig_value;
    (*entry).modifiable = (*entry).orig_modifiable;
    (*entry).modified = 0;
    SUCCESS
}

/// Finds the string value of an INI directive by name, preferring the original value when `orig` is
/// set and the directive was altered; None when the directive does not exist.
unsafe fn value_of(name: *const c_char, name_length: usize, orig: bool) -> Option<*mut zend_string> {
    let entry = entry(std::slice::from_raw_parts(name.cast::<u8>(), name_length));
    if entry.is_null() {
        return None;
    }
    let value = if orig && (*entry).modified != 0 { (*entry).orig_value } else { (*entry).value };
    Some(value)
}

/// `zend_ini_long`: an INI directive's value as an integer, 0 when the directive is missing or
/// unset.
#[no_mangle]
pub unsafe extern "C" fn zend_ini_long(name: *const c_char, name_length: usize, orig: c_int) -> zend_long {
    match value_of(name, name_length, orig != 0) {
        Some(value) if !value.is_null() => crate::convert::zend_atol((*value).val.as_ptr(), (*value).len),
        _ => 0,
    }
}

/// `zend_ini_double`: an INI directive's value as a float, 0.0 when the directive is missing or
/// unset.
#[no_mangle]
pub unsafe extern "C" fn zend_ini_double(name: *const c_char, name_length: usize, orig: c_int) -> f64 {
    match value_of(name, name_length, orig != 0) {
        Some(value) if !value.is_null() => crate::convert::zend_strtod((*value).val.as_ptr(), ptr::null_mut()),
        _ => 0.0,
    }
}

/// `zend_ini_string_ex`: an INI directive's value as a C string (empty when unset, NULL when the
/// directive does not exist), reporting existence through `exists` when it is non-NULL.
#[no_mangle]
pub unsafe extern "C" fn zend_ini_string_ex(name: *const c_char, name_length: usize, orig: c_int, exists: *mut bool) -> *mut c_char {
    let value = value_of(name, name_length, orig != 0);
    if !exists.is_null() {
        *exists = value.is_some();
    }
    match value {
        Some(value) if !value.is_null() => (*value).val.as_mut_ptr(),
        Some(_) => c"".as_ptr().cast_mut(),
        None => ptr::null_mut(),
    }
}

/// `zend_ini_string`: an INI directive's value as a C string, empty when the directive is missing
/// or unset.
#[no_mangle]
pub unsafe extern "C" fn zend_ini_string(name: *const c_char, name_length: usize, orig: c_int) -> *mut c_char {
    let value = zend_ini_string_ex(name, name_length, orig, ptr::null_mut());
    if value.is_null() {
        c"".as_ptr().cast_mut()
    } else {
        value
    }
}

/// `zend_ini_str_ex`: an INI directive's value as a `zend_string` (possibly NULL), reporting
/// existence through `exists` when it is non-NULL.
#[no_mangle]
pub unsafe extern "C" fn zend_ini_str_ex(name: *const c_char, name_length: usize, orig: bool, exists: *mut bool) -> *mut zend_string {
    let value = value_of(name, name_length, orig);
    if !exists.is_null() {
        *exists = value.is_some();
    }
    value.unwrap_or(ptr::null_mut())
}

/// `zend_ini_str`: an INI directive's value as a `zend_string`, the empty string when the directive
/// is missing or unset.
#[no_mangle]
pub unsafe extern "C" fn zend_ini_str(name: *const c_char, name_length: usize, orig: bool) -> *mut zend_string {
    let value = zend_ini_str_ex(name, name_length, orig, ptr::null_mut());
    if value.is_null() {
        string::zend_empty_string
    } else {
        value
    }
}

/// `zend_ini_get_value`: an INI directive's current value, NULL when the directive does not exist
/// and the empty string when it is unset.
#[no_mangle]
pub unsafe extern "C" fn zend_ini_get_value(name: *mut zend_string) -> *mut zend_string {
    let entry = entry(bytes(name));
    if entry.is_null() {
        ptr::null_mut()
    } else if (*entry).value.is_null() {
        string::zend_empty_string
    } else {
        (*entry).value
    }
}

/// `on`/`yes`/`true` (any case) or a non-zero number.
pub fn parse_bool(text: &[u8]) -> bool {
    let lower = text.to_ascii_lowercase();
    match lower.as_slice() {
        b"true" | b"on" | b"yes" => true,
        _ => {
            let text = String::from_utf8_lossy(&lower);
            let digits: String = text.trim().chars().take_while(|c| c.is_ascii_digit() || *c == '-').collect();
            digits.parse::<i64>().map(|v| v != 0).unwrap_or(false)
        }
    }
}

/// `zend_ini_parse_bool`: parses a `zend_string` INI value as a boolean.
#[no_mangle]
pub unsafe extern "C" fn zend_ini_parse_bool(str: *mut zend_string) -> bool {
    parse_bool(bytes(str))
}

/// An INI quantity: an integer (decimal, `0x`, `0o`, `0b`), optionally
/// followed by `k`, `m` or `g`. Returns the value and an error text.
pub fn parse_quantity(text: &[u8]) -> (i64, Option<String>) {
    let raw = String::from_utf8_lossy(text).into_owned();
    let trimmed = raw.trim();
    if trimmed.is_empty() {
        return (0, None);
    }
    let (sign, body) = match trimmed.as_bytes()[0] {
        b'-' => (-1i64, &trimmed[1..]),
        b'+' => (1, &trimmed[1..]),
        _ => (1, trimmed),
    };
    let (radix, digits) = match body.get(..2).map(|p| p.to_ascii_lowercase()) {
        Some(p) if p == "0x" => (16, &body[2..]),
        Some(p) if p == "0o" => (8, &body[2..]),
        Some(p) if p == "0b" => (2, &body[2..]),
        _ => (10, body),
    };
    let end = digits.find(|c: char| !c.is_digit(radix)).unwrap_or(digits.len());
    if end == 0 {
        return (0, Some(format!("Invalid quantity \"{raw}\": no valid leading digits, interpreting as \"0\" for backwards compatibility")));
    }
    let value = i64::from_str_radix(&digits[..end], radix).unwrap_or(i64::MAX);
    let rest = digits[end..].trim();
    let factor: i64 = match rest.to_ascii_lowercase().as_str() {
        "" => 1,
        "k" => 1 << 10,
        "m" => 1 << 20,
        "g" => 1 << 30,
        _ => {
            return (
                sign * value,
                Some(format!("Invalid quantity \"{raw}\": unknown multiplier \"{}\", interpreting as \"{}\" for backwards compatibility", &rest[..1], sign * value)),
            )
        }
    };
    (sign * value.saturating_mul(factor), None)
}

/// `zend_ini_parse_quantity`: parses an INI quantity (with `k`/`m`/`g` suffixes) from a
/// `zend_string`, returning the value and storing an error message in `*errstr` when the text is
/// invalid.
#[no_mangle]
pub unsafe extern "C" fn zend_ini_parse_quantity(value: *mut zend_string, errstr: *mut *mut zend_string) -> zend_long {
    let (number, error) = parse_quantity(bytes(value));
    if !errstr.is_null() {
        *errstr = error.map_or(ptr::null_mut(), |e| string::new(e.as_bytes()));
    }
    number
}

/// `zend_ini_parse_uquantity`: unsigned form of `zend_ini_parse_quantity`.
#[no_mangle]
pub unsafe extern "C" fn zend_ini_parse_uquantity(value: *mut zend_string, errstr: *mut *mut zend_string) -> zend_ulong {
    zend_ini_parse_quantity(value, errstr) as zend_ulong
}

/// `zend_ini_parse_quantity_warn`: parses an INI quantity and emits an `Invalid "<setting>"
/// setting` warning when it is malformed.
#[no_mangle]
pub unsafe extern "C" fn zend_ini_parse_quantity_warn(value: *mut zend_string, setting: *mut zend_string) -> zend_long {
    let (number, error) = parse_quantity(bytes(value));
    if let Some(error) = error {
        crate::error::warning(&format!("Invalid \"{}\" setting. {error}", string::text(setting)));
    }
    number
}

/// `zend_ini_parse_uquantity_warn`: unsigned form of `zend_ini_parse_quantity_warn`.
#[no_mangle]
pub unsafe extern "C" fn zend_ini_parse_uquantity_warn(value: *mut zend_string, setting: *mut zend_string) -> zend_ulong {
    zend_ini_parse_quantity_warn(value, setting) as zend_ulong
}

// ------------------------------------------------------------ standard update handlers

/// Computes the address of a directive's backing field: `mh_arg2` is the base of the module globals
/// and `mh_arg1` the field offset.
unsafe fn target<T>(mh_arg1: *mut c_void, mh_arg2: *mut c_void) -> *mut T {
    mh_arg2.cast::<u8>().add(mh_arg1 as usize).cast()
}

/// `OnUpdateBool`: standard INI update handler storing the value as a boolean in the directive's
/// backing field.
#[no_mangle]
pub unsafe extern "C" fn OnUpdateBool(
    _entry: *mut zend_ini_entry,
    new_value: *mut zend_string,
    mh_arg1: *mut c_void,
    mh_arg2: *mut c_void,
    _mh_arg3: *mut c_void,
    _stage: c_int,
) -> zend_result {
    *target::<bool>(mh_arg1, mh_arg2) = zend_ini_parse_bool(new_value);
    SUCCESS
}

/// `OnUpdateLong`: standard INI update handler storing the value as an integer (with quantity
/// suffixes) in the backing field.
#[no_mangle]
pub unsafe extern "C" fn OnUpdateLong(
    entry: *mut zend_ini_entry,
    new_value: *mut zend_string,
    mh_arg1: *mut c_void,
    mh_arg2: *mut c_void,
    _mh_arg3: *mut c_void,
    _stage: c_int,
) -> zend_result {
    *target::<zend_long>(mh_arg1, mh_arg2) = zend_ini_parse_quantity_warn(new_value, (*entry).name);
    SUCCESS
}

/// `OnUpdateLongGEZero`: like `OnUpdateLong`, but rejects negative values by returning FAILURE.
#[no_mangle]
pub unsafe extern "C" fn OnUpdateLongGEZero(
    entry: *mut zend_ini_entry,
    new_value: *mut zend_string,
    mh_arg1: *mut c_void,
    mh_arg2: *mut c_void,
    _mh_arg3: *mut c_void,
    _stage: c_int,
) -> zend_result {
    let value = zend_ini_parse_quantity_warn(new_value, (*entry).name);
    if value < 0 {
        return FAILURE;
    }
    *target::<zend_long>(mh_arg1, mh_arg2) = value;
    SUCCESS
}

/// `OnUpdateReal`: standard INI update handler storing the value as a float in the backing field.
#[no_mangle]
pub unsafe extern "C" fn OnUpdateReal(
    _entry: *mut zend_ini_entry,
    new_value: *mut zend_string,
    mh_arg1: *mut c_void,
    mh_arg2: *mut c_void,
    _mh_arg3: *mut c_void,
    _stage: c_int,
) -> zend_result {
    *target::<f64>(mh_arg1, mh_arg2) = crate::convert::zend_strtod((*new_value).val.as_ptr(), ptr::null_mut());
    SUCCESS
}

/// `OnUpdateString`: standard INI update handler storing a pointer to the value's characters (NULL
/// for a NULL value) in the backing field.
#[no_mangle]
pub unsafe extern "C" fn OnUpdateString(
    _entry: *mut zend_ini_entry,
    new_value: *mut zend_string,
    mh_arg1: *mut c_void,
    mh_arg2: *mut c_void,
    _mh_arg3: *mut c_void,
    _stage: c_int,
) -> zend_result {
    *target::<*mut c_char>(mh_arg1, mh_arg2) = if new_value.is_null() { ptr::null_mut() } else { (*new_value).val.as_mut_ptr() };
    SUCCESS
}

/// `OnUpdateStringUnempty`: like `OnUpdateString`, but rejects an empty or NULL value by returning
/// FAILURE.
#[no_mangle]
pub unsafe extern "C" fn OnUpdateStringUnempty(
    entry: *mut zend_ini_entry,
    new_value: *mut zend_string,
    mh_arg1: *mut c_void,
    mh_arg2: *mut c_void,
    mh_arg3: *mut c_void,
    stage: c_int,
) -> zend_result {
    if new_value.is_null() || (*new_value).len == 0 {
        return FAILURE;
    }
    OnUpdateString(entry, new_value, mh_arg1, mh_arg2, mh_arg3, stage)
}

/// `OnUpdateStr`: standard INI update handler storing the `zend_string` itself in the backing
/// field.
#[no_mangle]
pub unsafe extern "C" fn OnUpdateStr(
    _entry: *mut zend_ini_entry,
    new_value: *mut zend_string,
    mh_arg1: *mut c_void,
    mh_arg2: *mut c_void,
    _mh_arg3: *mut c_void,
    _stage: c_int,
) -> zend_result {
    *target::<*mut zend_string>(mh_arg1, mh_arg2) = new_value;
    SUCCESS
}

/// `OnUpdateStrNotEmpty`: like `OnUpdateStr`, but rejects an empty or NULL value by returning
/// FAILURE.
#[no_mangle]
pub unsafe extern "C" fn OnUpdateStrNotEmpty(
    entry: *mut zend_ini_entry,
    new_value: *mut zend_string,
    mh_arg1: *mut c_void,
    mh_arg2: *mut c_void,
    mh_arg3: *mut c_void,
    stage: c_int,
) -> zend_result {
    if new_value.is_null() || (*new_value).len == 0 {
        return FAILURE;
    }
    OnUpdateStr(entry, new_value, mh_arg1, mh_arg2, mh_arg3, stage)
}

/// `zend_ini_boolean_displayer_cb`: no-op, because the engine has no INI display output.
#[no_mangle]
pub unsafe extern "C" fn zend_ini_boolean_displayer_cb(_ini_entry: *mut zend_ini_entry, _type: c_int) {}

/// `zend_ini_color_displayer_cb`: no-op, because the engine has no INI display output.
#[no_mangle]
pub unsafe extern "C" fn zend_ini_color_displayer_cb(_ini_entry: *mut zend_ini_entry, _type: c_int) {}

/// `display_link_numbers`: no-op, because the engine has no INI display output.
#[no_mangle]
pub unsafe extern "C" fn display_link_numbers(_ini_entry: *mut zend_ini_entry, _type: c_int) {}

#[cfg(test)]
mod tests {
    use super::*;

    /// Verifies INI quantity parsing (suffixes, hexadecimal, whitespace, negatives, invalid text)
    /// and boolean parsing (`On`, `1`, `off`) match PHP.
    #[test]
    fn parses_quantities_like_php() {
        assert_eq!(parse_quantity(b"32M").0, 32 << 20);
        assert_eq!(parse_quantity(b"0x10").0, 16);
        assert_eq!(parse_quantity(b" 7 ").0, 7);
        assert_eq!(parse_quantity(b"-1").0, -1);
        assert!(parse_quantity(b"abc").1.is_some());
        assert!(parse_bool(b"On"));
        assert!(parse_bool(b"1"));
        assert!(!parse_bool(b"off"));
    }
}
