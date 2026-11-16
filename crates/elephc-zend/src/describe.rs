//! Purpose:
//! Introspection: what a started module registered (functions with their
//! arginfo, classes, constants, INI directives), as the JSON document
//! Elephc's extension build reads into an `ExtensionSurface`.
//!
//! Called from:
//! - The introspector program `src/php_ext/build.rs` links against a freshly
//!   built extension (`elephc_php_ext_describe`).
//!
//! Key details:
//! - The surface comes from running the extension, not from parsing it, so an
//!   `#ifdef`, an alias or a registration done in C code cannot be missed.
//! - Types render as PHP's `zend_type_to_string` does (`?string`,
//!   `array|string|null`), which is what the surface parser expects.

use std::ffi::{c_int, c_void, CStr};
use std::fmt::Write as _;

use crate::abi::*;
use crate::hash;
use crate::startup::{cg, eg};
use crate::string::{self, bytes, intern};

extern "C" {
    /// C library `fwrite`: writes `nitems` items of `size` bytes from `ptr` to a C stream,
    /// returning the number of items written.
    fn fwrite(ptr: *const c_void, size: usize, nitems: usize, stream: *mut FILE) -> usize;
    /// C library `fflush`: flushes a C stream's buffered output.
    fn fflush(stream: *mut FILE) -> c_int;
}

/// A JSON string literal for `text` (invalid UTF-8 replaced).
fn json_text(out: &mut String, text: &[u8]) {
    out.push('"');
    for c in String::from_utf8_lossy(text).chars() {
        match c {
            '"' => out.push_str("\\\""),
            '\\' => out.push_str("\\\\"),
            '\n' => out.push_str("\\n"),
            '\r' => out.push_str("\\r"),
            '\t' => out.push_str("\\t"),
            c if (c as u32) < 0x20 => {
                let _ = write!(out, "\\u{:04x}", c as u32);
            }
            c => out.push(c),
        }
    }
    out.push('"');
}

/// PHP's rendering of a declared type, or `None` when the type is unset.
pub unsafe fn type_to_string(t: zend_type) -> Option<String> {
    let mask = t.type_mask;
    if mask & _ZEND_TYPE_MASK == 0 {
        return None;
    }
    let mut parts: Vec<String> = Vec::new();
    if mask & _ZEND_TYPE_LIST_BIT != 0 {
        let list = t.ptr.cast::<zend_type_list>();
        for i in 0..(*list).num_types {
            let single = *(*list).types.as_ptr().add(i as usize);
            if single.type_mask & _ZEND_TYPE_NAME_BIT != 0 {
                parts.push(string::text(single.ptr.cast()));
            } else if single.type_mask & _ZEND_TYPE_LITERAL_NAME_BIT != 0 {
                parts.push(CStr::from_ptr(single.ptr.cast()).to_string_lossy().into_owned());
            }
        }
    } else if mask & _ZEND_TYPE_NAME_BIT != 0 {
        parts.push(string::text(t.ptr.cast()));
    } else if mask & _ZEND_TYPE_LITERAL_NAME_BIT != 0 {
        parts.extend(CStr::from_ptr(t.ptr.cast()).to_string_lossy().split('|').map(str::to_string));
    }
    let pure = mask & _ZEND_TYPE_MAY_BE_MASK;
    if pure == MAY_BE_ANY {
        parts.push("mixed".into());
        return Some(parts.join("|"));
    }
    let names: &[(u32, &str)] = &[
        (MAY_BE_STATIC, "static"),
        (MAY_BE_CALLABLE, "callable"),
        (MAY_BE_OBJECT, "object"),
        (MAY_BE_ARRAY, "array"),
        (MAY_BE_STRING, "string"),
        (MAY_BE_LONG, "int"),
        (MAY_BE_DOUBLE, "float"),
    ];
    for (bit, name) in names {
        if pure & bit != 0 {
            parts.push((*name).into());
        }
    }
    if pure & MAY_BE_BOOL == MAY_BE_BOOL {
        parts.push("bool".into());
    } else if pure & MAY_BE_FALSE != 0 {
        parts.push("false".into());
    } else if pure & MAY_BE_TRUE != 0 {
        parts.push("true".into());
    }
    if pure & MAY_BE_VOID != 0 {
        parts.push("void".into());
    }
    if pure & MAY_BE_NEVER != 0 {
        parts.push("never".into());
    }
    if pure & MAY_BE_NULL != 0 {
        if parts.len() == 1 && !parts[0].contains('|') {
            return Some(format!("?{}", parts[0]));
        }
        parts.push("null".into());
    }
    Some(parts.join("|"))
}

/// `zend_type_to_string_resolved`: renders a `zend_type` as PHP type syntax (such as `?int` or
/// `array|string`) in a new string; the scope is not needed because class names are already
/// resolved, and an empty type gives "".
#[no_mangle]
pub unsafe extern "C" fn zend_type_to_string_resolved(t: zend_type, _scope: *mut zend_class_entry) -> *mut zend_string {
    match type_to_string(t) {
        Some(text) => string::new(text.as_bytes()),
        None => intern(b""),
    }
}

/// `zend_type_to_string`: renders a `zend_type` as PHP type syntax in a new string.
#[no_mangle]
pub unsafe extern "C" fn zend_type_to_string(t: zend_type) -> *mut zend_string {
    zend_type_to_string_resolved(t, std::ptr::null_mut())
}

/// Appends a rendered type to the JSON output as a string literal, or `null` when there is no type.
fn json_type(out: &mut String, rendered: Option<String>) {
    match rendered {
        Some(text) => json_text(out, text.as_bytes()),
        None => out.push_str("null"),
    }
}

/// Appends the JSON description of one internal function or method: name, required argument count,
/// return type, by-reference and deprecation flags, visibility/static/abstract for methods, and
/// each parameter's name, type, by-reference and variadic flags and default text.
unsafe fn describe_function(out: &mut String, function: *mut zend_internal_function) {
    let f = &*function;
    out.push_str("{\"name\":");
    json_text(out, bytes(f.function_name));
    let _ = write!(out, ",\"required\":{}", f.required_num_args);
    let variadic = f.fn_flags & ZEND_ACC_VARIADIC != 0;
    let count = f.num_args + variadic as u32;
    out.push_str(",\"returns\":");
    if f.arg_info.is_null() {
        out.push_str("null");
    } else {
        json_type(out, type_to_string((*f.arg_info.sub(1)).r#type));
    }
    let _ = write!(
        out,
        ",\"returns_by_ref\":{},\"deprecated\":{}",
        f.fn_flags & ZEND_ACC_RETURN_REFERENCE != 0,
        f.fn_flags & ZEND_ACC_DEPRECATED != 0
    );
    if !f.scope.is_null() {
        let visibility = if f.fn_flags & ZEND_ACC_PRIVATE != 0 {
            "private"
        } else if f.fn_flags & ZEND_ACC_PROTECTED != 0 {
            "protected"
        } else {
            "public"
        };
        let _ = write!(
            out,
            ",\"static\":{},\"abstract\":{},\"visibility\":\"{visibility}\"",
            f.fn_flags & ZEND_ACC_STATIC != 0,
            f.fn_flags & ZEND_ACC_ABSTRACT != 0
        );
    }
    out.push_str(",\"params\":[");
    for i in 0..count {
        if f.arg_info.is_null() {
            break;
        }
        let arg = &*f.arg_info.add(i as usize);
        if i > 0 {
            out.push(',');
        }
        out.push_str("{\"name\":");
        json_text(out, CStr::from_ptr(arg.name).to_bytes());
        out.push_str(",\"type\":");
        json_type(out, type_to_string(arg.r#type));
        let mode = (arg.r#type.type_mask >> _ZEND_SEND_MODE_SHIFT) & 3;
        let _ = write!(
            out,
            ",\"by_ref\":{},\"variadic\":{},\"default\":",
            mode != 0,
            arg.r#type.type_mask & _ZEND_IS_VARIADIC_BIT != 0
        );
        if arg.default_value.is_null() {
            out.push_str("null");
        } else {
            json_text(out, CStr::from_ptr(arg.default_value).to_bytes());
        }
        out.push('}');
    }
    out.push_str("]}");
}

/// Appends the JSON description of a constant's value as a `{"type":...,"value":...}` object keyed
/// on the zval type.
unsafe fn describe_constant_value(out: &mut String, value: *const zval) {
    match (*value).u1.v.r#type {
        IS_NULL => out.push_str("{\"type\":\"null\"}"),
        IS_FALSE => out.push_str("{\"type\":\"bool\",\"value\":false}"),
        IS_TRUE => out.push_str("{\"type\":\"bool\",\"value\":true}"),
        IS_LONG => {
            let _ = write!(out, "{{\"type\":\"int\",\"value\":{}}}", (*value).value.lval);
        }
        IS_DOUBLE => {
            out.push_str("{\"type\":\"float\",\"value\":");
            json_text(out, crate::convert::double_repr((*value).value.dval, 17, false).as_bytes());
            out.push('}');
        }
        IS_STRING => {
            out.push_str("{\"type\":\"string\",\"value\":");
            json_text(out, bytes((*value).value.str));
            out.push('}');
        }
        other => {
            let _ = write!(out, "{{\"type\":\"{}\"}}", crate::convert::type_name(other));
        }
    }
}

/// The module's surface as one JSON document.
pub unsafe fn describe(module: *mut zend_module_entry) -> String {
    let m = &*module;
    let mut out = String::from("{\"module\":");
    json_text(&mut out, CStr::from_ptr(m.name).to_bytes());
    out.push_str(",\"version\":");
    let version = if m.version.is_null() { &b""[..] } else { CStr::from_ptr(m.version).to_bytes() };
    json_text(&mut out, version);

    out.push_str(",\"dependencies\":[");
    let mut first = true;
    let mut dep = m.deps;
    while !dep.is_null() && !(*dep).name.is_null() {
        if (*dep).r#type as c_int == MODULE_DEP_REQUIRED {
            if !first {
                out.push(',');
            }
            first = false;
            json_text(&mut out, CStr::from_ptr((*dep).name).to_bytes());
        }
        dep = dep.add(1);
    }

    out.push_str("],\"functions\":[");
    first = true;
    // The running function table, not just `m.functions`: an extension may
    // register more with `zend_register_functions` during MINIT, and each
    // registered function records the module that was starting.
    let functions = cg().function_table;
    for idx in 0..(*functions).nNumUsed {
        let p = hash::bucket(functions, idx);
        if (*p).val.u1.v.r#type == IS_UNDEF {
            continue;
        }
        let function = (*p).val.value.ptr.cast::<zend_internal_function>();
        if (*function).r#type != ZEND_INTERNAL_FUNCTION || (*function).module != module {
            continue;
        }
        if !first {
            out.push(',');
        }
        first = false;
        describe_function(&mut out, function);
    }

    out.push_str("],\"classes\":[");
    first = true;
    let classes = cg().class_table;
    let mut seen = std::collections::HashSet::new();
    for idx in 0..(*classes).nNumUsed {
        let p = hash::bucket(classes, idx);
        if (*p).val.u1.v.r#type == IS_UNDEF {
            continue;
        }
        let ce = (*p).val.value.ptr.cast::<zend_class_entry>();
        if (*ce).info.internal.module != module || !seen.insert(ce as usize) {
            continue;
        }
        if !first {
            out.push(',');
        }
        first = false;
        out.push_str("{\"name\":");
        json_text(&mut out, bytes((*ce).name));
        out.push_str(",\"parent\":");
        let parent = (*ce).parent_u.parent;
        if parent.is_null() {
            out.push_str("null");
        } else {
            json_text(&mut out, bytes((*parent).name));
        }
        let flags = (*ce).ce_flags;
        let _ = write!(
            out,
            ",\"interface\":{},\"abstract\":{},\"final\":{},\"interfaces\":[",
            flags & ZEND_ACC_INTERFACE != 0,
            flags & (ZEND_ACC_EXPLICIT_ABSTRACT_CLASS | ZEND_ACC_IMPLICIT_ABSTRACT_CLASS) != 0,
            flags & ZEND_ACC_FINAL != 0
        );
        for i in 0..(*ce).num_interfaces {
            let iface = *(*ce).interfaces_u.interfaces.add(i as usize);
            if i > 0 {
                out.push(',');
            }
            json_text(&mut out, bytes((*iface).name));
        }
        out.push_str("],\"constants\":[");
        let constants = &mut (*ce).constants_table as *mut HashTable;
        let mut first_constant = true;
        for c_idx in 0..(*constants).nNumUsed {
            let cp = hash::bucket(constants, c_idx);
            if (*cp).val.u1.v.r#type == IS_UNDEF || (*cp).key.is_null() {
                continue;
            }
            let constant = (*cp).val.value.ptr.cast::<zend_class_constant>();
            if (*constant).ce != ce {
                continue;
            }
            if !first_constant {
                out.push(',');
            }
            first_constant = false;
            out.push_str("{\"name\":");
            json_text(&mut out, bytes((*cp).key));
            out.push_str(",\"value\":");
            describe_constant_value(&mut out, &(*constant).value);
            out.push('}');
        }
        out.push_str("],\"methods\":[");
        let methods = &mut (*ce).function_table as *mut HashTable;
        let mut first_method = true;
        for m_idx in 0..(*methods).nNumUsed {
            let mp = hash::bucket(methods, m_idx);
            if (*mp).val.u1.v.r#type == IS_UNDEF {
                continue;
            }
            let method = (*mp).val.value.ptr.cast::<zend_internal_function>();
            if (*method).scope != ce {
                continue;
            }
            if !first_method {
                out.push(',');
            }
            first_method = false;
            describe_function(&mut out, method);
        }
        out.push_str("]}");
    }

    out.push_str("],\"constants\":[");
    first = true;
    let constants = eg().zend_constants;
    for idx in 0..(*constants).nNumUsed {
        let p = hash::bucket(constants, idx);
        if (*p).val.u1.v.r#type == IS_UNDEF {
            continue;
        }
        let constant = (*p).val.value.ptr.cast::<zend_constant>();
        if ((*constant).value.u2.constant_flags >> 8) as c_int != m.module_number {
            continue;
        }
        if !first {
            out.push(',');
        }
        first = false;
        out.push_str("{\"name\":");
        json_text(&mut out, bytes((*constant).name));
        out.push_str(",\"value\":");
        describe_constant_value(&mut out, &(*constant).value);
        out.push('}');
    }

    out.push_str("],\"ini\":[");
    first = true;
    let directives = eg().ini_directives;
    for idx in 0..(*directives).nNumUsed {
        let p = hash::bucket(directives, idx);
        if (*p).val.u1.v.r#type == IS_UNDEF {
            continue;
        }
        let ini = (*p).val.value.ptr.cast::<zend_ini_entry>();
        if (*ini).module_number != m.module_number {
            continue;
        }
        if !first {
            out.push(',');
        }
        first = false;
        out.push_str("{\"name\":");
        json_text(&mut out, bytes((*ini).name));
        out.push_str(",\"default\":");
        let default = if (*ini).modified != 0 { (*ini).orig_value } else { (*ini).value };
        if default.is_null() {
            out.push_str("null");
        } else {
            json_text(&mut out, bytes(default));
        }
        out.push('}');
    }
    out.push_str("]}\n");
    out
}

/// Starts `module` and writes its surface to `out`. 0, or 1 when the module
/// could not start.
/// c: hidden
#[no_mangle]
pub unsafe extern "C" fn elephc_php_ext_describe(handle: *mut c_void, out: *mut FILE) -> c_int {
    let module = handle.cast::<zend_module_entry>();
    if !crate::host::boot_module(module) {
        return 1;
    }
    let text = describe(module);
    fwrite(text.as_ptr().cast(), 1, text.len(), out);
    fflush(out);
    0
}
