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
//!   the directive is registered is applied through its handler at once and
//!   becomes the directive's configured value.
//! - A runtime change (`zend_alter_ini_entry`, `ini_set`) lasts one request:
//!   `zend_ini_deactivate` restores it at request shutdown, as php-src's
//!   `zend_deactivate` does.
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

/// Records a php.ini line; applies it at once to a registered directive. php-src applies php.ini
/// to a directive whatever level it is modifiable at, so the late form passes every level.
///
/// A late line is configuration, not a runtime change: it becomes the value the directive starts
/// every request with, so request shutdown (`zend_ini_deactivate`) never undoes it. When a script
/// has overridden the directive in the current request, that override stays visible until the
/// request ends and is then restored to this configured value.
pub unsafe fn set_configuration(name: &[u8], value: &[u8]) {
    let slot = alloc_struct::<zval>();
    crate::zval::set_str(slot, string::intern(value));
    configuration().insert(name.to_vec(), slot);
    let entry = entry(name);
    if entry.is_null() {
        return;
    }
    let value = string::intern(value);
    if (*entry).modified != 0 {
        (*entry).orig_value = value;
    } else if modify(entry, value, ZEND_INI_STAGE_RUNTIME) {
        (*entry).value = value;
    }
}

/// `zend_ini_deactivate`: request shutdown's INI phase. Every directive a script or an extension
/// changed during the request goes back to its configured value and modifiable level, through its
/// handler at the deactivation stage, so the next request starts from the configuration.
pub unsafe fn zend_ini_deactivate() {
    let directives = eg().ini_directives;
    if directives.is_null() {
        return;
    }
    for idx in 0..(*directives).nNumUsed {
        let p = hash::bucket(directives, idx);
        if (*p).val.u1.v.r#type == IS_UNDEF {
            continue;
        }
        let entry = (*p).val.value.ptr.cast::<zend_ini_entry>();
        if (*entry).modified == 0 {
            continue;
        }
        modify(entry, (*entry).orig_value, ZEND_INI_STAGE_DEACTIVATE);
        (*entry).value = (*entry).orig_value;
        (*entry).modifiable = (*entry).orig_modifiable;
        (*entry).modified = 0;
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
/// definitions. Each directive is added to the directive table and takes its value from the
/// recorded php.ini configuration when its handler accepts it, otherwise from its default. A
/// name already registered by another module fails the call as in php-src: the module's
/// directives are unregistered and `FAILURE` is returned.
#[no_mangle]
pub unsafe extern "C" fn zend_register_ini_entries_ex(ini_entry: *const zend_ini_entry_def, module_number: c_int, module_type: c_int) -> zend_result {
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
            // As php-src does: the table keeps the first owner's entry, this one was never
            // linked anywhere, and the module loses every directive it registered (this call's
            // earlier ones included). The caller sees FAILURE; MINIT usually ignores it.
            crate::alloc::free(entry.cast());
            zend_unregister_ini_entries_ex(module_number, module_type);
            return FAILURE;
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

/// `zend_is_whitespace`: the bytes an INI quantity may be padded with.
fn is_ini_space(byte: u8) -> bool {
    matches!(byte, b' ' | b'\t' | b'\n' | b'\r' | 0x0b | 0x0c)
}

/// `smart_str_append_escaped` into a Rust string: how quantity messages quote
/// the text, so a NUL or a multibyte suffix byte prints as `\xHH`.
fn escape(bytes: &[u8]) -> String {
    let mut out = String::with_capacity(bytes.len());
    for &byte in bytes {
        match byte {
            b'\n' => out.push_str("\\n"),
            b'\r' => out.push_str("\\r"),
            b'\t' => out.push_str("\\t"),
            0x0b => out.push_str("\\v"),
            0x0c => out.push_str("\\f"),
            0x1b => out.push_str("\\e"),
            b'\\' => out.push_str("\\\\"),
            c if !(32..=126).contains(&c) => out.push_str(&format!("\\x{c:02X}")),
            c => out.push(c as char),
        }
    }
    out
}

/// `ZEND_STRTOUL(digits, &end, base)` on text with no leading space or sign:
/// the value, the bytes consumed and whether it overflowed (ERANGE). Base 0
/// reads a leading `0` as octal and `0x` as hexadecimal; base 16 accepts an
/// optional `0x`, as strtoul does.
fn strtoul(digits: &[u8], base: u32) -> (u64, usize, bool) {
    let hex_prefix = digits.len() > 2 && digits[0] == b'0' && (digits[1] | 0x20) == b'x' && digits[2].is_ascii_hexdigit();
    let (start, radix) = match base {
        0 | 16 if hex_prefix => (2, 16),
        0 if digits.first() == Some(&b'0') => (0, 8),
        0 => (0, 10),
        _ => (0, base),
    };
    let mut value: u64 = 0;
    let mut overflow = false;
    let mut end = start;
    while let Some(digit) = digits.get(end).and_then(|&b| (b as char).to_digit(radix)) {
        match value.checked_mul(u64::from(radix)).and_then(|v| v.checked_add(u64::from(digit))) {
            Some(next) => value = next,
            None => overflow = true,
        }
        end += 1;
    }
    if overflow {
        value = u64::MAX;
    }
    (value, end, overflow)
}

/// `zend_ini_parse_quantity_internal`: an INI quantity (decimal, legacy
/// octal, `0x`, `0o`, `0b`, optionally followed by `k`, `m` or `g`) read the
/// way PHP 8.5 reads it, signed or unsigned, byte by byte so any suffix byte
/// is safe. Returns the value and PHP's error text.
pub fn parse_quantity_as(text: &[u8], signed: bool) -> (u64, Option<String>) {
    let mut digits = 0;
    let mut str_end = text.len();
    while digits < str_end && is_ini_space(text[digits]) {
        digits += 1;
    }
    while digits < str_end && is_ini_space(text[str_end - 1]) {
        str_end -= 1;
    }
    if digits == str_end {
        return (0, None);
    }
    let invalid = escape(text);
    let no_digits = || Some(format!("Invalid quantity \"{invalid}\": no valid leading digits, interpreting as \"0\" for backwards compatibility"));
    let at = |i: usize| if i < str_end { text[i] } else { 0 };
    let mut is_negative = false;
    if text[digits] == b'+' {
        digits += 1;
    } else if text[digits] == b'-' {
        is_negative = true;
        digits += 1;
    }
    if !at(digits).is_ascii_digit() {
        return (0, no_digits());
    }
    let mut base = 0;
    if at(digits) == b'0' && !at(digits + 1).is_ascii_digit() {
        if digits + 1 == str_end {
            return (0, None);
        }
        let prefix = at(digits + 1);
        if !matches!(prefix, b'g' | b'G' | b'm' | b'M' | b'k' | b'K') {
            base = match prefix {
                b'x' | b'X' => 16,
                b'o' | b'O' => 8,
                b'b' | b'B' => 2,
                _ => {
                    let prefix = escape(&[prefix]);
                    return (0, Some(format!("Invalid prefix \"0{prefix}\", interpreting as \"0\" for backwards compatibility")));
                }
            };
            digits += 2;
            if digits == str_end || !at(digits).is_ascii_alphanumeric() {
                return (
                    0,
                    Some(format!("Invalid quantity \"{invalid}\": no digits after base prefix, interpreting as \"0\" for backwards compatibility")),
                );
            }
        }
    }
    let (mut retval, consumed, mut overflow) = strtoul(&text[digits..str_end], base);
    if !overflow {
        if !signed {
            if is_negative {
                // "-1" is commonly used as the maximum (memory_limit=-1).
                if retval == 1 && digits + consumed == str_end {
                    retval = u64::MAX;
                } else {
                    overflow = true;
                }
            }
        } else if is_negative && retval == (i64::MAX as u64) + 1 {
            retval = retval.wrapping_neg();
        } else if (retval as i64) < 0 {
            overflow = true;
        } else if is_negative {
            retval = retval.wrapping_neg();
        }
    }
    if consumed == 0 {
        return (0, no_digits());
    }
    // Whitespace may separate the number from its multiplier.
    let mut digits_end = digits + consumed;
    while digits_end < str_end && is_ini_space(text[digits_end]) {
        digits_end += 1;
    }
    let out_of_range = || Some(format!("Invalid quantity \"{invalid}\": value is out of range, using overflow result for backwards compatibility"));
    if digits_end == str_end {
        return (retval, if overflow { out_of_range() } else { None });
    }
    let suffix = text[str_end - 1];
    let interpreted = escape(&text[..digits_end]);
    let factor: u64 = match suffix {
        b'g' | b'G' => 1 << 30,
        b'm' | b'M' => 1 << 20,
        b'k' | b'K' => 1 << 10,
        _ => {
            let suffix = escape(&[suffix]);
            return (
                retval,
                Some(format!(
                    "Invalid quantity \"{invalid}\": unknown multiplier \"{suffix}\", interpreting as \"{interpreted}\" for backwards compatibility"
                )),
            );
        }
    };
    if digits_end < str_end - 1 {
        let suffix = escape(&[suffix]);
        return (
            retval.wrapping_mul(factor),
            Some(format!("Invalid quantity \"{invalid}\", interpreting as \"{interpreted}{suffix}\" for backwards compatibility")),
        );
    }
    if !overflow {
        overflow = if !signed {
            retval > u64::MAX / factor
        } else if (retval as i64) > 0 {
            (retval as i64) > i64::MAX / factor as i64
        } else {
            (retval as i64) < i64::MIN / factor as i64
        };
    }
    retval = retval.wrapping_mul(factor);
    (retval, if overflow { out_of_range() } else { None })
}

/// A signed INI quantity (`zend_ini_parse_quantity`): the value and PHP's
/// error text.
pub fn parse_quantity(text: &[u8]) -> (i64, Option<String>) {
    let (value, error) = parse_quantity_as(text, true);
    (value as i64, error)
}
/// Parses `value` as a quantity, storing PHP's error text in `*errstr` (NULL
/// when the text is valid).
unsafe fn parse_into(value: *mut zend_string, errstr: *mut *mut zend_string, signed: bool) -> u64 {
    let (number, error) = parse_quantity_as(bytes(value), signed);
    if !errstr.is_null() {
        *errstr = error.map_or(ptr::null_mut(), |e| string::new(e.as_bytes()));
    }
    number
}

/// Parses `value` as a quantity, warning `Invalid "<setting>" setting. ...`
/// as `zend_ini_parse_quantity_warn` does when it is malformed.
unsafe fn parse_warn(value: *mut zend_string, setting: *mut zend_string, signed: bool) -> u64 {
    let (number, error) = parse_quantity_as(bytes(value), signed);
    if let Some(error) = error {
        crate::error::warning(&format!("Invalid \"{}\" setting. {error}", string::text(setting)));
    }
    number
}

/// `zend_ini_parse_quantity`: parses an INI quantity (with `k`/`m`/`g` suffixes) from a
/// `zend_string`, returning the value and storing an error message in `*errstr` when the text is
/// invalid.
#[no_mangle]
pub unsafe extern "C" fn zend_ini_parse_quantity(value: *mut zend_string, errstr: *mut *mut zend_string) -> zend_long {
    parse_into(value, errstr, true) as zend_long
}

/// `zend_ini_parse_uquantity`: unsigned form of `zend_ini_parse_quantity` (only "-1" may be
/// negative, read as the maximum).
#[no_mangle]
pub unsafe extern "C" fn zend_ini_parse_uquantity(value: *mut zend_string, errstr: *mut *mut zend_string) -> zend_ulong {
    parse_into(value, errstr, false)
}

/// `zend_ini_parse_quantity_warn`: parses an INI quantity and emits an `Invalid "<setting>"
/// setting` warning when it is malformed.
#[no_mangle]
pub unsafe extern "C" fn zend_ini_parse_quantity_warn(value: *mut zend_string, setting: *mut zend_string) -> zend_long {
    parse_warn(value, setting, true) as zend_long
}

/// `zend_ini_parse_uquantity_warn`: unsigned form of `zend_ini_parse_quantity_warn`.
#[no_mangle]
pub unsafe extern "C" fn zend_ini_parse_uquantity_warn(value: *mut zend_string, setting: *mut zend_string) -> zend_ulong {
    parse_warn(value, setting, false)
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

    /// Values and messages match `ini_parse_quantity()` in PHP 8.5, including
    /// a multibyte suffix (which used to slice inside a UTF-8 character and
    /// panic across `extern "C"`), legacy octal, and overflow.
    #[test]
    fn quantity_messages_match_php() {
        let cases: [(&[u8], i64, Option<&str>); 12] = [
            ("1é".as_bytes(), 1, Some("Invalid quantity \"1\\xC3\\xA9\": unknown multiplier \"\\xA9\", interpreting as \"1\" for backwards compatibility")),
            (b"abc", 0, Some("Invalid quantity \"abc\": no valid leading digits, interpreting as \"0\" for backwards compatibility")),
            (b"010", 8, None),
            (b"0k", 0, None),
            (b"0z1", 0, Some("Invalid prefix \"0z\", interpreting as \"0\" for backwards compatibility")),
            (b"0x", 0, Some("Invalid quantity \"0x\": no digits after base prefix, interpreting as \"0\" for backwards compatibility")),
            (b" 5 kk", 5120, Some("Invalid quantity \" 5 kk\", interpreting as \" 5 k\" for backwards compatibility")),
            (b"5 k", 5120, None),
            (b"9223372036854775807k", -1024, Some("Invalid quantity \"9223372036854775807k\": value is out of range, using overflow result for backwards compatibility")),
            (b"-9223372036854775808", i64::MIN, None),
            (b"0b101", 5, None),
            (b"99999999999999999999", -1, Some("Invalid quantity \"99999999999999999999\": value is out of range, using overflow result for backwards compatibility")),
        ];
        for (text, value, message) in cases {
            let (got, error) = parse_quantity(text);
            assert_eq!(got, value, "{}", String::from_utf8_lossy(text));
            assert_eq!(error.as_deref(), message, "{}", String::from_utf8_lossy(text));
        }
        assert_eq!(parse_quantity_as(b"-1", false), (u64::MAX, None));
        assert!(parse_quantity_as(b"-2", false).1.is_some());
    }
}
