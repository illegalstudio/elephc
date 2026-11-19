//! Purpose:
//! The C ABI boundary between compiled PHP programs and the `elephc_ffi` bridge:
//! contexts, types, CData, memory operations, globals, and the call builder.
//!
//! Called from:
//! - The AST-built FFI prelude's `extern "elephc_ffi"` declarations.
//!
//! Key details:
//! - Handles are pointer-sized integers; 0 is the failure sentinel. CType handles are
//!   arena ids biased by one so the valid `void` type (id 0) remains representable.
//! - Strings cross as NUL-terminated `char*` for inputs (C declarations never contain a
//!   NUL) and as a thread-local pointer+length pair for binary outputs.
//! - Every entry point keeps PHP diagnostics in the bridge's thread-local error slot; the
//!   prelude turns them into `FFI\ParserException`, `FFI\Exception`, or `TypeError`.

use std::ffi::CStr;
use std::os::raw::c_char;
use std::panic::{catch_unwind, AssertUnwindSafe};

use crate::call::{ArgValue, CallBuilder, RESULT_VOID};
use crate::context::{
    self, cdata, cdata_type, set_error, set_result_string, type_clone, ERROR_EXCEPTION,
};
use crate::memory;
use crate::place::{self, Place};
use crate::types::{TypeId, TypeKind};

/// Runs one ABI body, converting a panic into `fallback` and an FFI exception.
pub(crate) fn guard<T>(fallback: T, body: impl FnOnce() -> T) -> T {
    match catch_unwind(AssertUnwindSafe(body)) {
        Ok(value) => value,
        Err(_) => {
            set_error(ERROR_EXCEPTION, "internal FFI bridge panic");
            fallback
        }
    }
}

/// Converts a NUL-terminated input string into a UTF-8 lossy Rust slice.
pub(crate) unsafe fn text<'a>(pointer: *const c_char) -> &'a str {
    if pointer.is_null() {
        return "";
    }
    CStr::from_ptr(pointer).to_str().unwrap_or("")
}

/// Converts a pointer+length pair into bytes.
unsafe fn bytes<'a>(pointer: *const c_char, length: usize) -> &'a [u8] {
    if pointer.is_null() || length == 0 {
        return &[];
    }
    std::slice::from_raw_parts(pointer as *const u8, length)
}

/// Converts an arena id to the biased CType handle.
fn type_handle(id: TypeId) -> usize {
    id + 1
}

/// Converts a CType handle back to an arena id.
fn from_type_handle(handle: usize) -> Option<TypeId> {
    handle.checked_sub(1)
}

/// Stores bytes in the thread-local result buffer and returns its pointer.
fn result_bytes(bytes: &[u8]) -> *const c_char {
    set_result_string(bytes)
}

// --- diagnostics -----------------------------------------------------------------

/// Returns the last diagnostic kind (0 when none).
#[no_mangle]
pub extern "C" fn elephc_ffi_error_kind() -> u32 {
    context::error_kind()
}

/// Returns a pointer to the last diagnostic message.
#[no_mangle]
pub extern "C" fn elephc_ffi_error_ptr() -> *const c_char {
    let message = context::error_message();
    result_bytes(message.as_bytes())
}

/// Returns the last diagnostic message length.
#[no_mangle]
pub extern "C" fn elephc_ffi_error_len() -> usize {
    context::error_message().len()
}

/// Clears the last diagnostic.
#[no_mangle]
pub extern "C" fn elephc_ffi_clear_error() {
    context::clear_error();
}

// --- contexts --------------------------------------------------------------------

/// Parses a C declaration block and returns an FFI context handle.
#[no_mangle]
pub extern "C" fn elephc_ffi_cdef(code: *const c_char, library: *const c_char) -> usize {
    guard(0, || {
        let code = unsafe { text(code) };
        let library = if library.is_null() {
            None
        } else {
            Some(unsafe { text(library) })
        };
        context::cdef(code, library).unwrap_or(0)
    })
}

/// Loads a header file and returns an FFI context handle.
#[no_mangle]
pub extern "C" fn elephc_ffi_load(path: *const c_char) -> usize {
    guard(0, || context::load(unsafe { text(path) }).unwrap_or(0))
}

/// Returns a preloaded `FFI_SCOPE` context handle.
#[no_mangle]
pub extern "C" fn elephc_ffi_scope(name: *const c_char) -> usize {
    guard(0, || context::scope(unsafe { text(name) }).unwrap_or(0))
}

// --- types -----------------------------------------------------------------------

/// Resolves a type name in a context and returns a CType handle.
#[no_mangle]
pub extern "C" fn elephc_ffi_type(handle: usize, name: *const c_char) -> usize {
    guard(0, || {
        context::resolve_type(handle, unsafe { text(name) })
            .map(type_handle)
            .unwrap_or(0)
    })
}

/// Returns a CType handle's display name pointer.
#[no_mangle]
pub extern "C" fn elephc_ffi_type_name(handle: usize) -> *const c_char {
    guard(std::ptr::null(), || {
        let Some(id) = from_type_handle(handle) else {
            set_error(ERROR_EXCEPTION, "Invalid FFI\\CType");
            return std::ptr::null();
        };
        let Some(ty) = type_clone(id) else {
            set_error(ERROR_EXCEPTION, "Invalid FFI\\CType");
            return std::ptr::null();
        };
        result_bytes(ty.display.as_bytes())
    })
}

/// Returns a CType handle's display name length.
#[no_mangle]
pub extern "C" fn elephc_ffi_type_name_len(handle: usize) -> usize {
    guard(0, || {
        from_type_handle(handle)
            .and_then(type_clone)
            .map(|ty| ty.display.len())
            .unwrap_or(0)
    })
}

/// Returns a CType handle's `FFI\CType::TYPE_*` kind.
#[no_mangle]
pub extern "C" fn elephc_ffi_type_kind(handle: usize) -> u32 {
    guard(0, || {
        from_type_handle(handle)
            .and_then(type_clone)
            .map(|ty| ty.kind_code())
            .unwrap_or(0)
    })
}

/// Returns a CType handle's C `sizeof`.
#[no_mangle]
pub extern "C" fn elephc_ffi_type_size(handle: usize) -> usize {
    guard(0, || {
        from_type_handle(handle)
            .and_then(type_clone)
            .map(|ty| ty.size)
            .unwrap_or(0)
    })
}

/// Returns a CType handle's C `alignof`.
#[no_mangle]
pub extern "C" fn elephc_ffi_type_align(handle: usize) -> usize {
    guard(0, || {
        from_type_handle(handle)
            .and_then(type_clone)
            .map(|ty| ty.align)
            .unwrap_or(0)
    })
}

/// Returns a CType handle's php-src attribute bits.
#[no_mangle]
pub extern "C" fn elephc_ffi_type_attributes(handle: usize) -> u32 {
    guard(0, || {
        from_type_handle(handle)
            .and_then(type_clone)
            .map(|ty| ty.attrs)
            .unwrap_or(0)
    })
}

/// Returns the enum underlying `TYPE_*` kind, or -1 after php-src's diagnostic.
#[no_mangle]
pub extern "C" fn elephc_ffi_type_enum_kind(handle: usize) -> i64 {
    guard(-1, || {
        let Some(ty) = from_type_handle(handle).and_then(type_clone) else {
            set_error(ERROR_EXCEPTION, "Invalid FFI\\CType");
            return -1;
        };
        match ty.kind {
            TypeKind::Enum { underlying, .. } => type_clone(underlying)
                .map(|inner| i64::from(inner.kind_code()))
                .unwrap_or(-1),
            _ => {
                set_error(ERROR_EXCEPTION, "FFI\\CType is not an enumeration");
                -1
            }
        }
    })
}

/// Returns the element CType handle of an array type, or 0 after a diagnostic.
#[no_mangle]
pub extern "C" fn elephc_ffi_type_array_element(handle: usize) -> usize {
    guard(0, || {
        let Some(ty) = from_type_handle(handle).and_then(type_clone) else {
            set_error(ERROR_EXCEPTION, "Invalid FFI\\CType");
            return 0;
        };
        match ty.kind {
            TypeKind::Array { element, .. } => type_handle(element),
            _ => {
                set_error(ERROR_EXCEPTION, "FFI\\CType is not an array");
                0
            }
        }
    })
}

/// Returns the pointee CType handle of a pointer type, or 0 after a diagnostic.
#[no_mangle]
pub extern "C" fn elephc_ffi_type_pointer_pointee(handle: usize) -> usize {
    guard(0, || {
        let Some(ty) = from_type_handle(handle).and_then(type_clone) else {
            set_error(ERROR_EXCEPTION, "Invalid FFI\\CType");
            return 0;
        };
        match ty.kind {
            TypeKind::Pointer(inner) => type_handle(inner),
            _ => {
                set_error(ERROR_EXCEPTION, "FFI\\CType is not a pointer");
                0
            }
        }
    })
}

/// Returns the length of an array type, -1 after a diagnostic, or 0 when incomplete.
#[no_mangle]
pub extern "C" fn elephc_ffi_type_array_length(handle: usize) -> i64 {
    guard(-1, || {
        let Some(ty) = from_type_handle(handle).and_then(type_clone) else {
            set_error(ERROR_EXCEPTION, "Invalid FFI\\CType");
            return -1;
        };
        match ty.kind {
            TypeKind::Array { len, .. } => len.map_or(0, |len| len as i64),
            _ => {
                set_error(ERROR_EXCEPTION, "FFI\\CType is not an array");
                -1
            }
        }
    })
}

/// Returns a pointer-to-`handle` CType handle.
#[no_mangle]
pub extern "C" fn elephc_ffi_type_pointer_to(handle: usize) -> usize {
    guard(0, || {
        let Some(id) = from_type_handle(handle) else { return 0 };
        type_handle(context::with_store(|store, _| store.pointer(id)))
    })
}

/// Returns a new array CType handle with one more dimension.
#[no_mangle]
pub extern "C" fn elephc_ffi_array_type(element: usize, length: i64) -> usize {
    guard(0, || {
        let Some(element) = from_type_handle(element) else {
            set_error(ERROR_EXCEPTION, "Invalid FFI\\CType");
            return 0;
        };
        let Ok(length) = usize::try_from(length) else {
            set_error(ERROR_EXCEPTION, "negative array index");
            return 0;
        };
        type_handle(context::with_store(|store, _| store.array(element, Some(length))))
    })
}

/// Returns the number of struct fields, or -1 after php-src's diagnostic.
#[no_mangle]
pub extern "C" fn elephc_ffi_type_struct_field_count(handle: usize) -> i64 {
    guard(-1, || {
        let Some(ty) = from_type_handle(handle).and_then(type_clone) else {
            set_error(ERROR_EXCEPTION, "Invalid FFI\\CType");
            return -1;
        };
        match ty.kind {
            TypeKind::Struct(record) => record.fields.len() as i64,
            _ => {
                set_error(ERROR_EXCEPTION, "FFI\\CType is not a structure");
                -1
            }
        }
    })
}

/// Returns one struct field name as a result-buffer string.
#[no_mangle]
pub extern "C" fn elephc_ffi_type_struct_field_name(handle: usize, index: usize) -> *const c_char {
    guard(std::ptr::null(), || {
        let Some(ty) = from_type_handle(handle).and_then(type_clone) else { return std::ptr::null() };
        let TypeKind::Struct(record) = ty.kind else { return std::ptr::null() };
        let Some(field) = record.fields.get(index) else { return std::ptr::null() };
        result_bytes(field.name.as_deref().unwrap_or("").as_bytes())
    })
}

/// Returns one struct field name length.
#[no_mangle]
pub extern "C" fn elephc_ffi_type_struct_field_name_len(handle: usize, index: usize) -> usize {
    guard(0, || {
        let Some(ty) = from_type_handle(handle).and_then(type_clone) else { return 0 };
        let TypeKind::Struct(record) = ty.kind else { return 0 };
        record.fields.get(index).and_then(|field| field.name.as_ref()).map_or(0, String::len)
    })
}

/// Returns a struct field's byte offset, or `usize::MAX` after php-src's diagnostic.
#[no_mangle]
pub extern "C" fn elephc_ffi_type_struct_field_offset(handle: usize, name: *const c_char) -> usize {
    guard(usize::MAX, || {
        let Some(ty) = from_type_handle(handle).and_then(type_clone) else {
            set_error(ERROR_EXCEPTION, "Invalid FFI\\CType");
            return usize::MAX;
        };
        let TypeKind::Struct(record) = ty.kind else {
            set_error(ERROR_EXCEPTION, "FFI\\CType is not a structure");
            return usize::MAX;
        };
        let name = unsafe { text(name) };
        let Some(field) = record.fields.iter().find(|field| field.name.as_deref() == Some(name)) else {
            set_error(ERROR_EXCEPTION, "Wrong field name");
            return usize::MAX;
        };
        field.offset
    })
}

/// Returns a struct field's CType handle, or 0 after php-src's diagnostic.
#[no_mangle]
pub extern "C" fn elephc_ffi_type_struct_field_type(handle: usize, name: *const c_char) -> usize {
    guard(0, || {
        let Some(ty) = from_type_handle(handle).and_then(type_clone) else {
            set_error(ERROR_EXCEPTION, "Invalid FFI\\CType");
            return 0;
        };
        let TypeKind::Struct(record) = ty.kind else {
            set_error(ERROR_EXCEPTION, "FFI\\CType is not a structure");
            return 0;
        };
        let name = unsafe { text(name) };
        let Some(field) = record.fields.iter().find(|field| field.name.as_deref() == Some(name)) else {
            set_error(ERROR_EXCEPTION, "Wrong field name");
            return 0;
        };
        type_handle(field.ty)
    })
}

/// Returns a function type's return CType handle, or 0 after a diagnostic.
#[no_mangle]
pub extern "C" fn elephc_ffi_type_func_return(handle: usize) -> usize {
    guard(0, || {
        let Some(ty) = from_type_handle(handle).and_then(type_clone) else {
            set_error(ERROR_EXCEPTION, "Invalid FFI\\CType");
            return 0;
        };
        match ty.kind {
            TypeKind::Func(func) => type_handle(func.ret),
            _ => {
                set_error(ERROR_EXCEPTION, "FFI\\CType is not a function");
                0
            }
        }
    })
}

/// Returns a function type's fixed parameter count, or -1 after a diagnostic.
#[no_mangle]
pub extern "C" fn elephc_ffi_type_func_param_count(handle: usize) -> i64 {
    guard(-1, || {
        let Some(ty) = from_type_handle(handle).and_then(type_clone) else {
            set_error(ERROR_EXCEPTION, "Invalid FFI\\CType");
            return -1;
        };
        match ty.kind {
            TypeKind::Func(func) => func.params.len() as i64,
            _ => {
                set_error(ERROR_EXCEPTION, "FFI\\CType is not a function");
                -1
            }
        }
    })
}

/// Returns one function parameter CType handle, or 0 after php-src's diagnostic.
#[no_mangle]
pub extern "C" fn elephc_ffi_type_func_param(handle: usize, index: usize) -> usize {
    guard(0, || {
        let Some(ty) = from_type_handle(handle).and_then(type_clone) else {
            set_error(ERROR_EXCEPTION, "Invalid FFI\\CType");
            return 0;
        };
        match ty.kind {
            TypeKind::Func(func) => match func.params.get(index) {
                Some(param) => type_handle(*param),
                None => {
                    set_error(ERROR_EXCEPTION, "Wrong argument number");
                    0
                }
            },
            _ => {
                set_error(ERROR_EXCEPTION, "FFI\\CType is not a function");
                0
            }
        }
    })
}

/// Returns a function type's php-src ABI code, or -1 after a diagnostic.
#[no_mangle]
pub extern "C" fn elephc_ffi_type_func_abi(handle: usize) -> i64 {
    guard(-1, || {
        let Some(ty) = from_type_handle(handle).and_then(type_clone) else {
            set_error(ERROR_EXCEPTION, "Invalid FFI\\CType");
            return -1;
        };
        match ty.kind {
            TypeKind::Func(func) => i64::from(func.abi),
            _ => {
                set_error(ERROR_EXCEPTION, "FFI\\CType is not a function");
                -1
            }
        }
    })
}

// --- CData allocation and ownership ---------------------------------------------

/// Allocates a CData object by type name.
#[no_mangle]
pub extern "C" fn elephc_ffi_new(handle: usize, name: *const c_char, owned: bool) -> usize {
    guard(0, || {
        let Some(ty) = context::resolve_type(handle, unsafe { text(name) }) else { return 0 };
        context::new_cdata(handle, ty, owned).unwrap_or(0)
    })
}

/// Allocates a CData object by CType handle.
#[no_mangle]
pub extern "C" fn elephc_ffi_new_type(handle: usize, ty: usize, owned: bool) -> usize {
    guard(0, || {
        let Some(ty) = from_type_handle(ty) else { return 0 };
        context::new_cdata(handle, ty, owned).unwrap_or(0)
    })
}

/// Allocates a CData object from either a CType handle or a type name.
#[no_mangle]
pub extern "C" fn elephc_ffi_new_any(
    handle: usize,
    name: *const c_char,
    ty: usize,
    owned: bool,
) -> usize {
    guard(0, || {
        let resolved = if ty != 0 {
            from_type_handle(ty)
        } else {
            context::resolve_type(handle, unsafe { text(name) })
        };
        let Some(ty) = resolved else { return 0 };
        context::new_cdata(handle, ty, owned).unwrap_or(0)
    })
}

/// Creates an owning `FFI::addr()` pointer to a temporary CData.
#[no_mangle]
pub extern "C" fn elephc_ffi_addr_owned(value: usize) -> usize {
    guard(0, || context::addr_owned(value).unwrap_or(0))
}

/// Marks an allocated CData as persistent.
#[no_mangle]
pub extern "C" fn elephc_ffi_set_persistent(value: usize) -> bool {
    guard(false, || context::set_persistent(value))
}

/// Returns whether a CData value is a callable C function pointer.
///
/// php-src's `get_closure` handler checks the declared type (pointer to function) and the
/// CData's storage pointer, not the pointer value. A pointer field read from a NULL slot is
/// PHP `null` rather than CData, so the source-stamped copies a null field produces are not
/// callable; a storage-backed `FFI::new("int(*)(int)")` still is even while it holds NULL.
#[no_mangle]
pub extern "C" fn elephc_ffi_cdata_is_callable(value: usize) -> bool {
    guard(false, || {
        let Some(data) = cdata(value) else { return false };
        if data.freed || data.addr == 0 {
            return false;
        }
        let Some(ty) = type_clone(data.ty) else { return false };
        let TypeKind::Pointer(inner) = ty.kind else { return false };
        if !matches!(type_clone(inner).map(|inner| inner.kind), Some(TypeKind::Func(_))) {
            return false;
        }
        data.source == 0 || data.pointer_value() != 0
    })
}

/// Casts one PHP scalar through a temporary CData of the value's PHP C type.
///
/// php-src's `FFI::cast()` accepts an untyped scalar and casts a temporary `long`/`double`/
/// `bool`/null-pointer value. `kind` is 0 for int, 1 for float, 2 for bool, 3 for null.
#[no_mangle]
pub extern "C" fn elephc_ffi_cast_scalar(
    context_handle: usize,
    name: *const c_char,
    ty: usize,
    kind: u32,
    value: i64,
    float_value: f64,
    bool_value: bool,
) -> usize {
    guard(0, || {
        let target = if ty != 0 {
            from_type_handle(ty)
        } else {
            context::resolve_type(context_handle, unsafe { text(name) })
        };
        let Some(target) = target else { return 0 };
        let source_ty = match kind {
            0 => Some(context::with_store(|store, _| store.int(64, true, "int64_t"))),
            1 => Some(context::with_store(|store, _| store.float(64, "double", 8, 8))),
            2 => Some(context::with_store(|store, _| store.int(8, false, "bool"))),
            _ => Some(context::with_store(|store, _| {
                let character = store.int(8, false, "char");
                store.pointer(character)
            })),
        };
        let source = if kind == 3 {
            source_ty.and_then(|pointer_ty| {
                let pointee = crate::context::type_clone(pointer_ty)
                    .and_then(|pointer| match pointer.kind {
                        crate::types::TypeKind::Pointer(pointee) => Some(pointee),
                        _ => None,
                    })?;
                context::pointer_to(0, pointee)
            })
        } else {
            source_ty.and_then(|ty| context::new_cdata(0, ty, true))
        };
        let Some(source) = source else { return 0 };
        if kind != 3 {
            let Some(data) = context::cdata(source) else {
                context::release_cdata(source);
                return 0;
            };
            let place = crate::place::Place {
                addr: data.addr,
                ty: data.ty,
                bit_width: None,
                bit_offset: 0,
                readonly: false,
                label: None,
                parent: 0,
            };
            let written = match kind {
                0 => crate::place::write_int(&place, value),
                1 => crate::place::write_float(&place, float_value),
                _ => crate::place::write_bool(&place, bool_value),
            };
            if written.is_err() {
                context::release_cdata(source);
                return 0;
            }
        }
        let result = context::cast(target, source);
        context::release_cdata(source);
        result.unwrap_or(0)
    })
}

/// Casts a CData object to a CType handle.
#[no_mangle]
pub extern "C" fn elephc_ffi_cast(ty: usize, value: usize) -> usize {
    guard(0, || {
        let Some(ty) = from_type_handle(ty) else { return 0 };
        context::cast(ty, value).unwrap_or(0)
    })
}

/// Casts a CData object to a type name in a context.
#[no_mangle]
pub extern "C" fn elephc_ffi_cast_name(handle: usize, name: *const c_char, value: usize) -> usize {
    guard(0, || {
        let Some(ty) = context::resolve_type(handle, unsafe { text(name) }) else { return 0 };
        context::cast(ty, value).unwrap_or(0)
    })
}

/// Casts a CData object from either a CType handle or a type name.
#[no_mangle]
pub extern "C" fn elephc_ffi_cast_any(
    handle: usize,
    name: *const c_char,
    ty: usize,
    value: usize,
) -> usize {
    guard(0, || {
        let resolved = if ty != 0 {
            from_type_handle(ty)
        } else {
            context::resolve_type(handle, unsafe { text(name) })
        };
        let Some(ty) = resolved else { return 0 };
        context::cast(ty, value).unwrap_or(0)
    })
}

/// Returns a CData object's CType handle.
#[no_mangle]
pub extern "C" fn elephc_ffi_typeof(value: usize) -> usize {
    guard(0, || cdata_type(value).map(type_handle).unwrap_or(0))
}

/// Returns a CData object's type display name pointer.
#[no_mangle]
pub extern "C" fn elephc_ffi_cdata_type_name(value: usize) -> *const c_char {
    guard(std::ptr::null(), || {
        context::cdata_type_name(value)
            .map(|name| result_bytes(name.as_bytes()))
            .unwrap_or(std::ptr::null())
    })
}

/// Returns a CData object's type display name length.
#[no_mangle]
pub extern "C" fn elephc_ffi_cdata_type_name_len(value: usize) -> usize {
    guard(0, || context::cdata_type_name(value).map_or(0, |name| name.len()))
}

/// Returns a CData object's `sizeof`.
#[no_mangle]
pub extern "C" fn elephc_ffi_sizeof_cdata(value: usize) -> usize {
    guard(0, || context::cdata_size(value).unwrap_or(0))
}

/// Returns a CData object's `alignof`.
#[no_mangle]
pub extern "C" fn elephc_ffi_alignof_cdata(value: usize) -> usize {
    guard(0, || context::cdata_align(value).unwrap_or(0))
}

/// Returns a CType handle's `sizeof`.
#[no_mangle]
pub extern "C" fn elephc_ffi_sizeof_type(handle: usize) -> usize {
    elephc_ffi_type_size(handle)
}

/// Returns a CType handle's `alignof`.
#[no_mangle]
pub extern "C" fn elephc_ffi_alignof_type(handle: usize) -> usize {
    elephc_ffi_type_align(handle)
}

/// Creates an `FFI::addr()` pointer.
#[no_mangle]
pub extern "C" fn elephc_ffi_addr(value: usize) -> usize {
    guard(0, || context::addr(value).unwrap_or(0))
}

/// Frees the storage behind a CData object.
#[no_mangle]
pub extern "C" fn elephc_ffi_free(value: usize) -> bool {
    guard(false, || context::free_cdata(value).is_ok())
}

/// Releases the box backing a CData handle.
#[no_mangle]
pub extern "C" fn elephc_ffi_release_cdata(value: usize) {
    guard((), || context::release_cdata(value))
}

/// Creates a new CData handle referencing the same storage (PHP `clone`).
#[no_mangle]
pub extern "C" fn elephc_ffi_cdata_clone(value: usize) -> usize {
    guard(0, || context::clone_cdata(value).unwrap_or(0))
}

/// Returns whether a pointer CData is NULL.
#[no_mangle]
pub extern "C" fn elephc_ffi_is_null(value: usize) -> bool {
    guard(true, || context::cdata_is_null(value).unwrap_or(true))
}

/// Returns whether a CData object was explicitly freed.
#[no_mangle]
pub extern "C" fn elephc_ffi_is_freed(value: usize) -> bool {
    guard(true, || cdata(value).is_none_or(|data| data.freed))
}

/// Returns a pointer CData's raw pointer value.
#[no_mangle]
pub extern "C" fn elephc_ffi_pointer_value(value: usize) -> u64 {
    guard(0, || context::cdata(value).map_or(0, |data| data.pointer_value() as u64))
}

// --- memory ----------------------------------------------------------------------

/// Converts a PHP signed size, rejecting negative values before native memory is touched.
fn non_negative_size(size: i64) -> Option<usize> {
    if size < 0 {
        set_error(ERROR_EXCEPTION, "Negative size");
        None
    } else {
        Some(size as usize)
    }
}

/// Copies bytes between two CData objects.
#[no_mangle]
pub extern "C" fn elephc_ffi_memcpy(destination: usize, source: usize, size: i64) -> bool {
    guard(false, || {
        let Some(size) = non_negative_size(size) else { return false };
        memory::memcpy_cdata(destination, source, size).is_ok()
    })
}

/// Copies bytes from a PHP string into a CData object.
#[no_mangle]
pub extern "C" fn elephc_ffi_memcpy_string(
    destination: usize,
    source: *const c_char,
    source_len: usize,
    size: i64,
) -> bool {
    guard(false, || {
        let Some(size) = non_negative_size(size) else { return false };
        let source = unsafe { bytes(source, source_len) };
        memory::memcpy_string(destination, source, size).is_ok()
    })
}

/// Compares two CData objects.
#[no_mangle]
pub extern "C" fn elephc_ffi_memcmp(left: usize, right: usize, size: i64) -> i64 {
    guard(0, || {
        let Some(size) = non_negative_size(size) else { return 0 };
        i64::from(memory::memcmp_cdata(left, right, size))
    })
}

/// Compares a CData object against a PHP string.
#[no_mangle]
pub extern "C" fn elephc_ffi_memcmp_string(
    left: usize,
    right: *const c_char,
    right_len: usize,
    size: i64,
) -> i64 {
    guard(0, || {
        let Some(size) = non_negative_size(size) else { return 0 };
        let right = unsafe { bytes(right, right_len) };
        i64::from(memory::memcmp_string(left, right, size))
    })
}

/// Compares two PHP strings for `FFI::memcmp()`.
#[no_mangle]
pub extern "C" fn elephc_ffi_memcmp_strings(
    left: *const c_char,
    left_len: usize,
    right: *const c_char,
    right_len: usize,
    size: i64,
) -> i64 {
    guard(0, || {
        let Some(size) = non_negative_size(size) else { return 0 };
        let left = unsafe { bytes(left, left_len) };
        let right = unsafe { bytes(right, right_len) };
        i64::from(memory::memcmp_strings(left, right, size))
    })
}

/// Compares a PHP string against a CData object for `FFI::memcmp()`.
#[no_mangle]
pub extern "C" fn elephc_ffi_memcmp_string_data(
    left: *const c_char,
    left_len: usize,
    right: usize,
    size: i64,
) -> i64 {
    guard(0, || {
        let Some(size) = non_negative_size(size) else { return 0 };
        let left = unsafe { bytes(left, left_len) };
        i64::from(memory::memcmp_string_data(left, right, size))
    })
}

/// Fills a CData object's target bytes.
#[no_mangle]
pub extern "C" fn elephc_ffi_memset(value: usize, byte: i32, size: i64) -> bool {
    guard(false, || {
        let Some(size) = non_negative_size(size) else { return false };
        memory::memset_cdata(value, byte, size).is_ok()
    })
}

/// Returns a CData object's string data pointer.
#[no_mangle]
pub extern "C" fn elephc_ffi_string(value: usize, size: i64, has_size: bool) -> *const c_char {
    guard(std::ptr::null(), || {
        let size = if has_size {
            let Some(size) = non_negative_size(size) else { return std::ptr::null() };
            Some(size)
        } else {
            None
        };
        memory::cdata_string(value, size)
            .map(|bytes| result_bytes(&bytes))
            .unwrap_or(std::ptr::null())
    })
}

/// Returns the last string result's length.
#[no_mangle]
pub extern "C" fn elephc_ffi_string_len() -> usize {
    context::result_string_len()
}

// --- fields ----------------------------------------------------------------------

/// Returns a field place's value kind.
#[no_mangle]
pub extern "C" fn elephc_ffi_field_kind(value: usize, name: *const c_char) -> u32 {
    guard(0, || {
        place::field_place(value, unsafe { text(name) })
            .map(|place| place.kind())
            .unwrap_or(0)
    })
}

/// Reads a field as an integer.
#[no_mangle]
pub extern "C" fn elephc_ffi_field_int(value: usize, name: *const c_char) -> i64 {
    guard(0, || place::field_place(value, unsafe { text(name) }).map(|place| place::read_int(&place)).unwrap_or(0))
}

/// Reads a field as a float.
#[no_mangle]
pub extern "C" fn elephc_ffi_field_float(value: usize, name: *const c_char) -> f64 {
    guard(0.0, || place::field_place(value, unsafe { text(name) }).map(|place| place::read_float(&place)).unwrap_or(0.0))
}

/// Reads a field as a bool.
#[no_mangle]
pub extern "C" fn elephc_ffi_field_bool(value: usize, name: *const c_char) -> bool {
    guard(false, || place::field_place(value, unsafe { text(name) }).map(|place| place::read_bool(&place)).unwrap_or(false))
}

/// Reads a `char` field as its byte value.
#[no_mangle]
pub extern "C" fn elephc_ffi_field_char(value: usize, name: *const c_char) -> i64 {
    guard(0, || place::field_place(value, unsafe { text(name) }).map(|place| i64::from(place::read_char(&place))).unwrap_or(0))
}

/// Returns a field's CData view.
#[no_mangle]
pub extern "C" fn elephc_ffi_field_cdata(value: usize, name: *const c_char) -> usize {
    guard(0, || place::field_place(value, unsafe { text(name) }).map(|place| place.cdata_with_source()).unwrap_or(0))
}

/// Returns a storage-sharing view of one field place for `@prefer-ref` FFI arguments.
#[no_mangle]
pub extern "C" fn elephc_ffi_field_view(value: usize, name: *const c_char) -> usize {
    guard(0, || {
        let Ok(place) = place::field_place(value, unsafe { text(name) }) else { return 0 };
        crate::context::view_bits(
            place.addr,
            place.ty,
            place.readonly,
            place.bit_width,
            place.bit_offset,
            place.parent,
        )
    })
}

/// Returns a pointer field's pointee CType handle, or 0.
#[no_mangle]
pub extern "C" fn elephc_ffi_field_pointer_type(value: usize, name: *const c_char) -> usize {
    guard(0, || {
        let Some(place) = place::field_place(value, unsafe { text(name) }).ok() else { return 0 };
        let Some(ty) = type_clone(place.ty) else { return 0 };
        match ty.kind {
            TypeKind::Pointer(inner) => type_handle(inner),
            _ => 0,
        }
    })
}

/// Returns whether a pointer field is NULL.
#[no_mangle]
pub extern "C" fn elephc_ffi_field_is_null(value: usize, name: *const c_char) -> bool {
    guard(true, || place::field_place(value, unsafe { text(name) }).map(|place| memory::place_is_null(&place)).unwrap_or(true))
}

/// Returns a pointer CData addressing a field.
#[no_mangle]
pub extern "C" fn elephc_ffi_field_addr(value: usize, name: *const c_char) -> usize {
    guard(0, || place::field_place(value, unsafe { text(name) }).ok().and_then(|place| place::addr_to_place(&place)).unwrap_or(0))
}

/// Returns a field's CType handle.
#[no_mangle]
pub extern "C" fn elephc_ffi_field_type(value: usize, name: *const c_char) -> usize {
    guard(0, || place::field_place(value, unsafe { text(name) }).map(|place| type_handle(place.ty)).unwrap_or(0))
}

/// Returns a field's size.
#[no_mangle]
pub extern "C" fn elephc_ffi_field_size(value: usize, name: *const c_char) -> usize {
    guard(0, || place::field_place(value, unsafe { text(name) }).ok().and_then(|place| type_clone(place.ty)).map_or(0, |ty| ty.size))
}

/// Writes a field as an integer.
#[no_mangle]
pub extern "C" fn elephc_ffi_field_write_int(value: usize, name: *const c_char, input: i64) -> bool {
    guard(false, || place::field_place(value, unsafe { text(name) }).and_then(|place| place::write_int(&place, input)).is_ok())
}

/// Writes a field as a float.
#[no_mangle]
pub extern "C" fn elephc_ffi_field_write_float(value: usize, name: *const c_char, input: f64) -> bool {
    guard(false, || place::field_place(value, unsafe { text(name) }).and_then(|place| place::write_float(&place, input)).is_ok())
}

/// Writes a field as a bool.
#[no_mangle]
pub extern "C" fn elephc_ffi_field_write_bool(value: usize, name: *const c_char, input: bool) -> bool {
    guard(false, || place::field_place(value, unsafe { text(name) }).and_then(|place| place::write_bool(&place, input)).is_ok())
}

/// Writes a `char` field from a PHP string.
#[no_mangle]
pub extern "C" fn elephc_ffi_field_write_char(value: usize, name: *const c_char, input: *const c_char, len: usize) -> bool {
    guard(false, || {
        let input = unsafe { bytes(input, len) };
        place::field_place(value, unsafe { text(name) }).and_then(|place| place::write_char(&place, input)).is_ok()
    })
}

/// Writes a field from a CData object.
#[no_mangle]
pub extern "C" fn elephc_ffi_field_write_cdata(value: usize, name: *const c_char, input: usize) -> bool {
    guard(false, || place::field_place(value, unsafe { text(name) }).and_then(|place| place::write_cdata(&place, input)).is_ok())
}

/// Writes a pointer field from a raw address.
#[no_mangle]
pub extern "C" fn elephc_ffi_field_write_pointer(value: usize, name: *const c_char, input: usize) -> bool {
    guard(false, || place::field_place(value, unsafe { text(name) }).and_then(|place| place::write_pointer(&place, Some(input))).is_ok())
}

/// Writes NULL into a pointer field.
#[no_mangle]
pub extern "C" fn elephc_ffi_field_write_null(value: usize, name: *const c_char) -> bool {
    guard(false, || place::field_place(value, unsafe { text(name) }).and_then(|place| place::write_pointer(&place, None)).is_ok())
}

/// Writes a PHP string into a char field/array.
#[no_mangle]
pub extern "C" fn elephc_ffi_field_write_string(value: usize, name: *const c_char, input: *const c_char, len: usize) -> bool {
    guard(false, || {
        let input = unsafe { bytes(input, len) };
        place::field_place(value, unsafe { text(name) }).and_then(|place| place::write_string(&place, input)).is_ok()
    })
}

/// Returns the number of flattened struct fields (anonymous members promoted).
#[no_mangle]
pub extern "C" fn elephc_ffi_field_count(value: usize) -> usize {
    guard(0, || {
        let Some(data) = cdata(value) else { return 0 };
        let Some(ty) = type_clone(data.ty) else { return 0 };
        let TypeKind::Struct(record) = ty.kind else { return 0 };
        flatten_field_count(&record.fields)
    })
}

/// Counts fields after recursively promoting anonymous struct/union members.
fn flatten_field_count(fields: &[crate::types::Field]) -> usize {
    fields.iter().map(|field| {
        if field.name.is_some() {
            1
        } else {
            type_clone(field.ty).map_or(0, |ty| match &ty.kind {
                TypeKind::Struct(record) => flatten_field_count(&record.fields),
                _ => 0,
            })
        }
    }).sum()
}

/// Returns a flattened struct field name by index.
#[no_mangle]
pub extern "C" fn elephc_ffi_field_name_at(value: usize, index: usize) -> *const c_char {
    guard(std::ptr::null(), || {
        let Some(name) = flatten_field_names(value).get(index).cloned() else { return std::ptr::null() };
        result_bytes(name.as_bytes())
    })
}

/// Returns a flattened struct field name length by index.
#[no_mangle]
pub extern "C" fn elephc_ffi_field_name_at_len(value: usize, index: usize) -> usize {
    guard(0, || flatten_field_names(value).get(index).map_or(0, String::len))
}

/// Returns the flattened field names of one CType handle.
fn flatten_field_names(value: usize) -> Vec<String> {
    let Some(data) = cdata(value) else { return Vec::new() };
    let Some(ty) = type_clone(data.ty) else { return Vec::new() };
    let TypeKind::Struct(record) = ty.kind else { return Vec::new() };
    flatten_names(&record.fields)
}

/// Collects flattened field names, descending through anonymous members.
fn flatten_names(fields: &[crate::types::Field]) -> Vec<String> {
    let mut names = Vec::new();
    for field in fields {
        if let Some(name) = &field.name {
            names.push(name.clone());
        } else if let Some(ty) = type_clone(field.ty) {
            if let TypeKind::Struct(record) = &ty.kind {
                names.extend(flatten_names(&record.fields));
            }
        }
    }
    names
}

// --- offsets ---------------------------------------------------------------------

/// Returns an array/pointer element's value kind.
#[no_mangle]
pub extern "C" fn elephc_ffi_offset_kind(value: usize, index: i64) -> u32 {
    guard(0, || place::offset_place(value, index).map(|place| place.kind()).unwrap_or(0))
}

/// Reads an array/pointer element as an integer.
#[no_mangle]
pub extern "C" fn elephc_ffi_offset_int(value: usize, index: i64) -> i64 {
    guard(0, || place::offset_place(value, index).map(|place| place::read_int(&place)).unwrap_or(0))
}

/// Reads an array/pointer element as a float.
#[no_mangle]
pub extern "C" fn elephc_ffi_offset_float(value: usize, index: i64) -> f64 {
    guard(0.0, || place::offset_place(value, index).map(|place| place::read_float(&place)).unwrap_or(0.0))
}

/// Reads an array/pointer element as a bool.
#[no_mangle]
pub extern "C" fn elephc_ffi_offset_bool(value: usize, index: i64) -> bool {
    guard(false, || place::offset_place(value, index).map(|place| place::read_bool(&place)).unwrap_or(false))
}

/// Reads a char element as its byte value.
#[no_mangle]
pub extern "C" fn elephc_ffi_offset_char(value: usize, index: i64) -> i64 {
    guard(0, || place::offset_place(value, index).map(|place| i64::from(place::read_char(&place))).unwrap_or(0))
}

/// Returns an element's CData view.
#[no_mangle]
pub extern "C" fn elephc_ffi_offset_cdata(value: usize, index: i64) -> usize {
    guard(0, || {
        place::offset_place(value, index)
            .map(|place| place.cdata_with_source())
            .unwrap_or(0)
    })
}

/// Returns a storage-sharing view of one array element place for `@prefer-ref` arguments.
#[no_mangle]
pub extern "C" fn elephc_ffi_offset_view(value: usize, index: i64) -> usize {
    guard(0, || {
        let Ok(place) = place::offset_place(value, index) else { return 0 };
        crate::context::view_bits(
            place.addr,
            place.ty,
            place.readonly,
            place.bit_width,
            place.bit_offset,
            place.parent,
        )
    })
}

/// Returns a pointer element's pointee CType handle, or 0.
#[no_mangle]
pub extern "C" fn elephc_ffi_offset_pointer_type(value: usize, index: i64) -> usize {
    guard(0, || {
        let Some(place) = place::offset_place(value, index).ok() else { return 0 };
        let Some(ty) = type_clone(place.ty) else { return 0 };
        match ty.kind {
            TypeKind::Pointer(inner) => type_handle(inner),
            _ => 0,
        }
    })
}

/// Returns whether a pointer element is NULL.
#[no_mangle]
pub extern "C" fn elephc_ffi_offset_is_null(value: usize, index: i64) -> bool {
    guard(true, || place::offset_place(value, index).map(|place| memory::place_is_null(&place)).unwrap_or(true))
}

/// Returns a pointer CData addressing one element.
#[no_mangle]
pub extern "C" fn elephc_ffi_offset_addr(value: usize, index: i64) -> usize {
    guard(0, || place::offset_place(value, index).ok().and_then(|place| place::addr_to_place(&place)).unwrap_or(0))
}

/// Returns one element's CType handle.
#[no_mangle]
pub extern "C" fn elephc_ffi_offset_type(value: usize, index: i64) -> usize {
    guard(0, || place::offset_place(value, index).map(|place| type_handle(place.ty)).unwrap_or(0))
}

/// Returns one element's size.
#[no_mangle]
pub extern "C" fn elephc_ffi_offset_size(value: usize, index: i64) -> usize {
    guard(0, || place::offset_place(value, index).ok().and_then(|place| type_clone(place.ty)).map_or(0, |ty| ty.size))
}

/// Writes an element as an integer.
#[no_mangle]
pub extern "C" fn elephc_ffi_offset_write_int(value: usize, index: i64, input: i64) -> bool {
    guard(false, || place::offset_place(value, index).and_then(|place| place::write_int(&place, input)).is_ok())
}

/// Writes an element as a float.
#[no_mangle]
pub extern "C" fn elephc_ffi_offset_write_float(value: usize, index: i64, input: f64) -> bool {
    guard(false, || place::offset_place(value, index).and_then(|place| place::write_float(&place, input)).is_ok())
}

/// Writes an element as a bool.
#[no_mangle]
pub extern "C" fn elephc_ffi_offset_write_bool(value: usize, index: i64, input: bool) -> bool {
    guard(false, || place::offset_place(value, index).and_then(|place| place::write_bool(&place, input)).is_ok())
}

/// Writes a char element from a string.
#[no_mangle]
pub extern "C" fn elephc_ffi_offset_write_char(value: usize, index: i64, input: *const c_char, len: usize) -> bool {
    guard(false, || {
        let input = unsafe { bytes(input, len) };
        place::offset_place(value, index).and_then(|place| place::write_char(&place, input)).is_ok()
    })
}

/// Writes an element from CData.
#[no_mangle]
pub extern "C" fn elephc_ffi_offset_write_cdata(value: usize, index: i64, input: usize) -> bool {
    guard(false, || place::offset_place(value, index).and_then(|place| place::write_cdata(&place, input)).is_ok())
}

/// Writes a pointer element from a raw address.
#[no_mangle]
pub extern "C" fn elephc_ffi_offset_write_pointer(value: usize, index: i64, input: usize) -> bool {
    guard(false, || place::offset_place(value, index).and_then(|place| place::write_pointer(&place, Some(input))).is_ok())
}

/// Writes NULL into a pointer element.
#[no_mangle]
pub extern "C" fn elephc_ffi_offset_write_null(value: usize, index: i64) -> bool {
    guard(false, || place::offset_place(value, index).and_then(|place| place::write_pointer(&place, None)).is_ok())
}

/// Writes a string into a char array element.
#[no_mangle]
pub extern "C" fn elephc_ffi_offset_write_string(value: usize, index: i64, input: *const c_char, len: usize) -> bool {
    guard(false, || {
        let input = unsafe { bytes(input, len) };
        place::offset_place(value, index).and_then(|place| place::write_string(&place, input)).is_ok()
    })
}

/// Returns an array's length, or 0 when it is not an array.
#[no_mangle]
pub extern "C" fn elephc_ffi_array_length(value: usize) -> usize {
    guard(0, || {
        let Some(data) = cdata(value) else { return 0 };
        let Some(ty) = type_clone(data.ty) else { return 0 };
        match ty.kind {
            TypeKind::Array { len: Some(len), .. } => len,
            TypeKind::Array { len: None, .. } => 0,
            _ => 0,
        }
    })
}

/// Returns an array/pointer element view for iteration.
#[no_mangle]
pub extern "C" fn elephc_ffi_iter_cdata(value: usize, index: usize) -> usize {
    elephc_ffi_offset_cdata(value, index as i64)
}

/// Reads a CData callback result as the integer a C `int` return expects.
///
/// A pointer CData contributes its pointer value; scalar CData contributes the scalar, not
/// the storage address.
#[no_mangle]
pub extern "C" fn elephc_ffi_cdata_as_int(value: usize) -> i64 {
    guard(0, || {
        let Some(data) = cdata(value) else { return 0 };
        if data.check_alive().is_err() {
            set_error(ERROR_EXCEPTION, "Use after free()");
            return 0;
        }
        let Some(ty) = type_clone(data.ty) else { return 0 };
        let place = Place {
            addr: data.addr,
            ty: data.ty,
            bit_width: None,
            bit_offset: 0,
            readonly: true,
            label: None,
            parent: 0,
        };
        match ty.kind {
            TypeKind::Pointer(_) => data.pointer_value() as i64,
            TypeKind::Float { .. } => crate::place::read_float(&place) as i64,
            _ => crate::place::read_int(&place),
        }
    })
}

/// Creates an independent iteration view over one C array.
///
/// The view shares the array storage (and retains its owner) but carries its own cursor and
/// by-reference mode, so nested `foreach` loops over the same CData do not consume each other.
#[no_mangle]
pub extern "C" fn elephc_ffi_iter_view(handle: usize) -> usize {
    guard(0, || {
        let Some(data) = cdata(handle) else { return 0 };
        if data.check_alive().is_err() {
            set_error(ERROR_EXCEPTION, "Use after free()");
            return 0;
        }
        crate::context::view_bits(
            data.addr,
            data.ty,
            data.readonly,
            data.bit_width,
            data.bit_offset,
            handle,
        )
    })
}

/// Frees one element place, matching `FFI::free($array[$i])`.
#[no_mangle]
pub extern "C" fn elephc_ffi_free_offset(value: usize, index: i64) -> bool {
    guard(false, || place::offset_place(value, index).and_then(|place| place::free_place(&place)).is_ok())
}

/// Frees one field place, matching `FFI::free($struct->field)`.
#[no_mangle]
pub extern "C" fn elephc_ffi_free_field(value: usize, name: *const c_char) -> bool {
    guard(false, || place::field_place(value, unsafe { text(name) }).and_then(|place| place::free_place(&place)).is_ok())
}

/// Reads one element place as a string pointer.
#[no_mangle]
pub extern "C" fn elephc_ffi_offset_string(value: usize, index: i64, size: i64, has_size: bool) -> *const c_char {
    guard(std::ptr::null(), || {
        let Some(place) = place::offset_place(value, index).ok() else { return std::ptr::null() };
        let size = if has_size {
            let Some(size) = non_negative_size(size) else { return std::ptr::null() };
            Some(size)
        } else {
            None
        };
        memory::place_string(&place, size)
            .map(|bytes| result_bytes(&bytes))
            .unwrap_or(std::ptr::null())
    })
}

/// Reads one field place as a string pointer.
#[no_mangle]
pub extern "C" fn elephc_ffi_field_string(value: usize, name: *const c_char, size: i64, has_size: bool) -> *const c_char {
    guard(std::ptr::null(), || {
        let Some(place) = place::field_place(value, unsafe { text(name) }).ok() else { return std::ptr::null() };
        let size = if has_size {
            let Some(size) = non_negative_size(size) else { return std::ptr::null() };
            Some(size)
        } else {
            None
        };
        memory::place_string(&place, size)
            .map(|bytes| result_bytes(&bytes))
            .unwrap_or(std::ptr::null())
    })
}

/// Fills one element place.
#[no_mangle]
pub extern "C" fn elephc_ffi_memset_offset(value: usize, index: i64, byte: i32, size: i64) -> bool {
    guard(false, || {
        let Some(size) = non_negative_size(size) else { return false };
        place::offset_place(value, index).and_then(|place| memory::memset_place(&place, byte, size)).is_ok()
    })
}

/// Fills one field place.
#[no_mangle]
pub extern "C" fn elephc_ffi_memset_field(value: usize, name: *const c_char, byte: i32, size: i64) -> bool {
    guard(false, || {
        let Some(size) = non_negative_size(size) else { return false };
        place::field_place(value, unsafe { text(name) }).and_then(|place| memory::memset_place(&place, byte, size)).is_ok()
    })
}

/// Copies bytes into one element place from a CData source.
#[no_mangle]
pub extern "C" fn elephc_ffi_memcpy_offset(value: usize, index: i64, source: usize, size: i64) -> bool {
    guard(false, || {
        let Some(size) = non_negative_size(size) else { return false };
        place::offset_place(value, index).and_then(|place| memory::memcpy_place_from_cdata(&place, source, size)).is_ok()
    })
}

/// Copies bytes into one field place from a CData source.
#[no_mangle]
pub extern "C" fn elephc_ffi_memcpy_field(value: usize, name: *const c_char, source: usize, size: i64) -> bool {
    guard(false, || {
        let Some(size) = non_negative_size(size) else { return false };
        place::field_place(value, unsafe { text(name) }).and_then(|place| memory::memcpy_place_from_cdata(&place, source, size)).is_ok()
    })
}

// --- constants -------------------------------------------------------------------

/// Returns whether a context declares an enum or object-like constant.
#[no_mangle]
pub extern "C" fn elephc_ffi_constant_found(context_handle: usize, name: *const c_char) -> bool {
    guard(false, || {
        let Some(context) = context::context(context_handle) else { return false };
        context.constants.contains_key(unsafe { text(name) })
    })
}

/// Returns one context constant value (0 when the name is absent).
#[no_mangle]
pub extern "C" fn elephc_ffi_constant_value(context_handle: usize, name: *const c_char) -> i64 {
    guard(0, || {
        let Some(context) = context::context(context_handle) else { return 0 };
        context.constants.get(unsafe { text(name) }).copied().unwrap_or(0)
    })
}

// --- globals ---------------------------------------------------------------------

/// Resolves a declared C global to its addressable place in a context.
pub(crate) fn global_place(context_handle: usize, name: &str) -> Result<Place, ()> {
    let Some(context) = context::context(context_handle) else { return Err(()) };
    let Some(ty) = context.globals.get(name).copied() else {
        set_error(ERROR_EXCEPTION, format!("Attempt to read undefined C variable '{}'", name));
        return Err(());
    };
    let Some(symbol) = context.symbol(name) else {
        set_error(ERROR_EXCEPTION, format!("Attempt to read undefined C variable '{}'", name));
        return Err(());
    };
    let Some(info) = type_clone(ty) else { return Err(()) };
    Ok(Place {
        addr: symbol,
        ty,
        bit_width: None,
        bit_offset: 0,
        readonly: info.is_const,
        label: Some(name.to_string()),
        parent: 0,
    })
}

/// Returns a C global's value kind.
#[no_mangle]
pub extern "C" fn elephc_ffi_global_kind(context_handle: usize, name: *const c_char) -> u32 {
    guard(0, || global_place(context_handle, unsafe { text(name) }).map(|place| place.kind()).unwrap_or(0))
}

/// Returns a C global pointer's pointee CType handle, or 0.
#[no_mangle]
pub extern "C" fn elephc_ffi_global_pointer_type(
    context_handle: usize,
    name: *const c_char,
) -> usize {
    guard(0, || {
        let Some(place) = global_place(context_handle, unsafe { text(name) }).ok() else {
            return 0;
        };
        let Some(ty) = type_clone(place.ty) else { return 0 };
        match ty.kind {
            TypeKind::Pointer(inner) => type_handle(inner),
            _ => 0,
        }
    })
}

/// Reads a C global as an integer.
#[no_mangle]
pub extern "C" fn elephc_ffi_global_int(context_handle: usize, name: *const c_char) -> i64 {
    guard(0, || global_place(context_handle, unsafe { text(name) }).map(|place| place::read_int(&place)).unwrap_or(0))
}

/// Reads a C global as a float.
#[no_mangle]
pub extern "C" fn elephc_ffi_global_float(context_handle: usize, name: *const c_char) -> f64 {
    guard(0.0, || global_place(context_handle, unsafe { text(name) }).map(|place| place::read_float(&place)).unwrap_or(0.0))
}

/// Reads a C global as a bool.
#[no_mangle]
pub extern "C" fn elephc_ffi_global_bool(context_handle: usize, name: *const c_char) -> bool {
    guard(false, || global_place(context_handle, unsafe { text(name) }).map(|place| place::read_bool(&place)).unwrap_or(false))
}

/// Reads a `char` global as its byte value.
#[no_mangle]
pub extern "C" fn elephc_ffi_global_char(context_handle: usize, name: *const c_char) -> i64 {
    guard(0, || global_place(context_handle, unsafe { text(name) }).map(|place| i64::from(place::read_char(&place))).unwrap_or(0))
}

/// Returns a C global's CData view.
#[no_mangle]
pub extern "C" fn elephc_ffi_global_cdata(context_handle: usize, name: *const c_char) -> usize {
    guard(0, || global_place(context_handle, unsafe { text(name) }).map(|place| place.cdata_with_source()).unwrap_or(0))
}

/// Reads a char-pointer C global as a string pointer.
#[no_mangle]
pub extern "C" fn elephc_ffi_global_string(context_handle: usize, name: *const c_char) -> *const c_char {
    guard(std::ptr::null(), || {
        let Some(place) = global_place(context_handle, unsafe { text(name) }).ok() else { return std::ptr::null() };
        memory::place_string(&place, None)
            .map(|bytes| result_bytes(&bytes))
            .unwrap_or(std::ptr::null())
    })
}

/// Writes a C global integer.
#[no_mangle]
pub extern "C" fn elephc_ffi_global_write_int(context_handle: usize, name: *const c_char, input: i64) -> bool {
    guard(false, || global_place(context_handle, unsafe { text(name) }).and_then(|place| place::write_int(&place, input)).is_ok())
}

/// Writes a C global float.
#[no_mangle]
pub extern "C" fn elephc_ffi_global_write_float(context_handle: usize, name: *const c_char, input: f64) -> bool {
    guard(false, || global_place(context_handle, unsafe { text(name) }).and_then(|place| place::write_float(&place, input)).is_ok())
}

/// Writes a C global bool.
#[no_mangle]
pub extern "C" fn elephc_ffi_global_write_bool(context_handle: usize, name: *const c_char, input: bool) -> bool {
    guard(false, || global_place(context_handle, unsafe { text(name) }).and_then(|place| place::write_bool(&place, input)).is_ok())
}

/// Writes a C global from a PHP string, preserving php-src's string-conversion rules.
#[no_mangle]
pub extern "C" fn elephc_ffi_global_write_string(
    context_handle: usize,
    name: *const c_char,
    input: *const c_char,
    len: usize,
) -> bool {
    guard(false, || {
        let input = unsafe { bytes(input, len) };
        global_place(context_handle, unsafe { text(name) })
            .and_then(|place| place::write_string(&place, input))
            .is_ok()
    })
}

/// Writes a C global from a CData object.
#[no_mangle]
pub extern "C" fn elephc_ffi_global_write_cdata(context_handle: usize, name: *const c_char, input: usize) -> bool {
    guard(false, || {
        let Ok(place) = global_place(context_handle, unsafe { text(name) }) else { return false };
        let Some(target) = type_clone(place.ty) else { return false };
        let Some(source) = cdata(input).and_then(|data| type_clone(data.ty)) else {
            set_error(ERROR_EXCEPTION, "Invalid FFI\\CData");
            return false;
        };
        if let (
            TypeKind::Pointer(target_inner),
            TypeKind::Pointer(source_inner),
        ) = (&target.kind, &source.kind)
        {
            if let (
                Some(TypeKind::Func(target_func)),
                Some(TypeKind::Func(source_func)),
            ) = (
                type_clone(*target_inner).map(|ty| ty.kind),
                type_clone(*source_inner).map(|ty| ty.kind),
            ) {
                if !function_signatures_match(&target_func, &source_func) {
                    set_error(ERROR_EXCEPTION, "Incompatible function pointer assignment");
                    return false;
                }
            }
        }
        place::write_cdata(&place, input).is_ok()
    })
}

/// Compares two C function signatures for assignment compatibility.
pub(crate) fn function_signatures_match(
    target: &crate::types::FuncType,
    source: &crate::types::FuncType,
) -> bool {
    target.abi == source.abi
        && target.variadic == source.variadic
        && target.params.len() == source.params.len()
        && type_display_matches(target.ret, source.ret)
        && target
            .params
            .iter()
            .zip(source.params.iter())
            .all(|(target, source)| type_display_matches(*target, *source))
}

/// Compares two type ids by their rendered C spelling.
fn type_display_matches(left: TypeId, right: TypeId) -> bool {
    type_clone(left)
        .zip(type_clone(right))
        .is_some_and(|(left, right)| left.display == right.display)
}

/// Writes a C global pointer from a raw address.
#[no_mangle]
pub extern "C" fn elephc_ffi_global_write_pointer(context_handle: usize, name: *const c_char, input: usize) -> bool {
    guard(false, || global_place(context_handle, unsafe { text(name) }).and_then(|place| place::write_pointer(&place, Some(input))).is_ok())
}

/// Writes NULL into a C global pointer.
#[no_mangle]
pub extern "C" fn elephc_ffi_global_write_null(context_handle: usize, name: *const c_char) -> bool {
    guard(false, || global_place(context_handle, unsafe { text(name) }).and_then(|place| place::write_pointer(&place, None)).is_ok())
}

// --- CData operators -------------------------------------------------------------

/// Returns the result kind (1 int, 2 CData) of one CData binary operation.
#[no_mangle]
pub extern "C" fn elephc_ffi_binary_kind(
    left: i64,
    left_kind: u32,
    right: i64,
    right_kind: u32,
    op: u32,
) -> u32 {
    guard(0, || crate::binary::binary_kind(left, left_kind, right, right_kind, op))
}

/// Returns whether the right CData operand is converted to int with PHP's warning.
#[no_mangle]
pub extern "C" fn elephc_ffi_binary_warns(
    left: i64,
    left_kind: u32,
    right: i64,
    right_kind: u32,
    op: u32,
) -> bool {
    guard(false, || crate::binary::binary_warns(left, left_kind, right, right_kind, op))
}

/// Executes one CData binary operation with an integer result.
#[no_mangle]
pub extern "C" fn elephc_ffi_binary_int(
    left: i64,
    left_kind: u32,
    right: i64,
    right_kind: u32,
    op: u32,
) -> i64 {
    guard(0, || crate::binary::binary_int(left, left_kind, right, right_kind, op))
}

/// Executes one CData binary operation with a CData result.
#[no_mangle]
pub extern "C" fn elephc_ffi_binary_cdata(
    left: i64,
    left_kind: u32,
    right: i64,
    right_kind: u32,
    op: u32,
) -> usize {
    guard(0, || crate::binary::binary_cdata(left, left_kind, right, right_kind, op))
}

/// Compares two CData operands.
#[no_mangle]
pub extern "C" fn elephc_ffi_binary_compare(
    left: i64,
    left_kind: u32,
    right: i64,
    right_kind: u32,
) -> i64 {
    guard(0, || crate::binary::compare(left, left_kind, right, right_kind))
}

/// Returns a CData object's own storage address.
#[no_mangle]
pub extern "C" fn elephc_ffi_cdata_address(value: usize) -> usize {
    guard(0, || cdata(value).map(|data| data.addr).unwrap_or(0))
}

/// Returns the address selected by an array/pointer element.
#[no_mangle]
pub extern "C" fn elephc_ffi_offset_address(value: usize, index: i64) -> usize {
    guard(0, || place::offset_place(value, index).map(|place| place.addr).unwrap_or(0))
}

/// Returns a C global variable's storage address.
#[no_mangle]
pub extern "C" fn elephc_ffi_global_address(context_handle: usize, name: *const c_char) -> usize {
    guard(0, || {
        global_place(context_handle, unsafe { text(name) })
            .map(|place| place.addr)
            .unwrap_or(0)
    })
}

// --- calls -----------------------------------------------------------------------

/// Creates a callable CData function pointer for a declared function.
#[no_mangle]
pub extern "C" fn elephc_ffi_function_cdata(context_handle: usize, name: *const c_char) -> usize {
    guard(0, || {
        let name = unsafe { text(name) };
        let Some(context) = context::context(context_handle) else { return 0 };
        let Some(ty) = context.functions.get(name).copied() else { return 0 };
        let Some(symbol) = context.symbol(name) else { return 0 };
        context::pointer_to(symbol, ty).unwrap_or(0)
    })
}

/// Begins a call to a named C function in a context.
#[no_mangle]
pub extern "C" fn elephc_ffi_call_begin(context_handle: usize, name: *const c_char) -> usize {
    guard(0, || {
        let Some(builder) = CallBuilder::new(context_handle, unsafe { text(name) }) else {
            if context::error_kind() == 0 {
                set_error(ERROR_EXCEPTION, format!("Attempt to call undefined C function '{}'", unsafe { text(name) }));
            }
            return 0;
        };
        Box::into_raw(builder) as usize
    })
}

/// Begins a call through a callable CData function pointer.
#[no_mangle]
pub extern "C" fn elephc_ffi_call_begin_cdata(value: usize) -> usize {
    guard(0, || match CallBuilder::new_cdata(value) {
        Some(builder) => Box::into_raw(builder) as usize,
        None => 0,
    })
}

/// Returns the boxed call builder behind one handle, or `None` when stale.
fn call_mut(handle: usize) -> Option<&'static mut CallBuilder> {
    if handle == 0 {
        return None;
    }
    Some(unsafe { &mut *(handle as *mut CallBuilder) })
}

/// Records one integer argument.
#[no_mangle]
pub extern "C" fn elephc_ffi_call_arg_int(handle: usize, index: usize, value: i64) -> bool {
    guard(false, || {
        let Some(call) = call_mut(handle) else { return false };
        call.set_arg(index, ArgValue::Int(value));
        true
    })
}

/// Records one float argument.
#[no_mangle]
pub extern "C" fn elephc_ffi_call_arg_float(handle: usize, index: usize, value: f64) -> bool {
    guard(false, || {
        let Some(call) = call_mut(handle) else { return false };
        call.set_arg(index, ArgValue::Float(value));
        true
    })
}

/// Records one string argument.
#[no_mangle]
pub extern "C" fn elephc_ffi_call_arg_string(handle: usize, index: usize, value: *const c_char, len: usize) -> bool {
    guard(false, || {
        let Some(call) = call_mut(handle) else { return false };
        call.set_arg(index, ArgValue::String(unsafe { bytes(value, len) }.to_vec()));
        true
    })
}

/// Records one CData argument.
#[no_mangle]
pub extern "C" fn elephc_ffi_call_arg_cdata(handle: usize, index: usize, value: usize) -> bool {
    guard(false, || {
        let Some(call) = call_mut(handle) else { return false };
        call.set_arg(index, ArgValue::CData(value));
        true
    })
}

/// Returns whether one call parameter declares a function pointer.
///
/// The prelude gates callback conversion on this: a string passed to a `char*` stays a
/// string, while the same string passed to a function pointer is a PHP callable.
#[no_mangle]
pub extern "C" fn elephc_ffi_call_param_is_function_pointer(
    handle: usize,
    index: usize,
) -> bool {
    guard(false, || {
        let Some(call) = call_mut(handle) else { return false };
        let Some(ty) = call.param_type(index) else { return false };
        let Some(info) = type_clone(ty) else { return false };
        let TypeKind::Pointer(inner) = info.kind else { return false };
        type_clone(inner).is_some_and(|inner| matches!(inner.kind, TypeKind::Func(_)))
    })
}

/// Prepares a libffi closure for one function-pointer call argument.
///
/// The prelude registers the PHP callable under the returned id, reads its code address
/// with [`crate::callback::elephc_ffi_callback_code`], and passes that address as the
/// argument's pointer value.
#[no_mangle]
pub extern "C" fn elephc_ffi_call_arg_callback(handle: usize, index: usize) -> usize {
    guard(0, || {
        let Some(call) = call_mut(handle) else { return 0 };
        let Some(ty) = call.param_type(index) else {
            set_error(
                ERROR_EXCEPTION,
                "Passing incompatible argument for a C function pointer",
            );
            return 0;
        };
        crate::callback::prepare_argument(ty, "callback").map_or(0, |(id, _)| id)
    })
}

/// Records one raw pointer argument prepared by the callback machinery.
#[no_mangle]
pub extern "C" fn elephc_ffi_call_arg_pointer(handle: usize, index: usize, value: usize) -> bool {
    guard(false, || {
        let Some(call) = call_mut(handle) else { return false };
        call.set_arg(index, ArgValue::Pointer(value));
        true
    })
}

/// Records one null argument.
#[no_mangle]
pub extern "C" fn elephc_ffi_call_arg_null(handle: usize, index: usize) -> bool {
    guard(false, || {
        let Some(call) = call_mut(handle) else { return false };
        call.set_arg(index, ArgValue::Null);
        true
    })
}

/// Records one bool argument.
#[no_mangle]
pub extern "C" fn elephc_ffi_call_arg_bool(handle: usize, index: usize, value: bool) -> bool {
    guard(false, || {
        let Some(call) = call_mut(handle) else { return false };
        call.set_arg(index, ArgValue::Bool(value));
        true
    })
}

/// Invokes the pending call and returns its result kind.
#[no_mangle]
pub extern "C" fn elephc_ffi_call_invoke(handle: usize) -> u32 {
    guard(RESULT_VOID, || call_mut(handle).map_or(RESULT_VOID, CallBuilder::invoke))
}

/// Returns a call's integer result.
#[no_mangle]
pub extern "C" fn elephc_ffi_call_result_int(handle: usize) -> i64 {
    guard(0, || call_mut(handle).map_or(0, |call| call.result_int))
}

/// Returns a call's float result.
#[no_mangle]
pub extern "C" fn elephc_ffi_call_result_float(handle: usize) -> f64 {
    guard(0.0, || call_mut(handle).map_or(0.0, |call| call.result_float))
}

/// Returns a call's bool result.
#[no_mangle]
pub extern "C" fn elephc_ffi_call_result_bool(handle: usize) -> bool {
    guard(false, || call_mut(handle).map_or(false, |call| call.result_bool))
}

/// Returns a call's CData result handle.
#[no_mangle]
pub extern "C" fn elephc_ffi_call_result_cdata(handle: usize) -> usize {
    guard(0, || call_mut(handle).map_or(0, |call| call.result_cdata))
}

/// Returns a call's string result pointer.
#[no_mangle]
pub extern "C" fn elephc_ffi_call_result_string(handle: usize) -> *const c_char {
    guard(std::ptr::null(), || {
        call_mut(handle)
            .map(|call| result_bytes(&call.result_bytes))
            .unwrap_or(std::ptr::null())
    })
}

/// Returns a call's string result length.
#[no_mangle]
pub extern "C" fn elephc_ffi_call_result_string_len(handle: usize) -> usize {
    guard(0, || call_mut(handle).map_or(0, |call| call.result_bytes.len()))
}

/// Releases a call builder.
#[no_mangle]
pub extern "C" fn elephc_ffi_call_release(handle: usize) {
    guard((), || {
        if handle != 0 {
            unsafe {
                drop(Box::from_raw(handle as *mut CallBuilder));
            }
        }
    })
}
