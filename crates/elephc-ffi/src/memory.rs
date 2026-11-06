//! Purpose:
//! Implements `FFI::string()`, `memcpy()`, `memcmp()`, and `memset()` over CData,
//! places, pointer values, and PHP string sources.
//!
//! Called from:
//! - `crate::abi` memory entry points.
//!
//! Key details:
//! - A CData source contributes its data address (pointer value for pointer types,
//!   element base for arrays/structs). A pointer source is dereferenced to its target.
//! - PHP string sources are rejected when the requested size exceeds the string length,
//!   matching php-src's "Attempt to read over string boundary".
//! - String results are returned through the bridge's thread-local buffer because CData
//!   contents may be binary and a C string would lose bytes after a NUL.

use crate::context::{
    cdata, data_address, set_error, type_clone, ERROR_EXCEPTION,
};
use crate::place::Place;
use crate::types::{TypeKind};

/// Copies bytes from a CData object into a PHP string result.
pub fn cdata_string(handle: usize, size: Option<usize>) -> Option<Vec<u8>> {
    let data = cdata(handle)?;
    if data.check_alive().is_err() {
        set_error(ERROR_EXCEPTION, "Use after free()");
        return None;
    }
    let address = data_address(data);
    if address == 0 {
        return Some(Vec::new());
    }
    Some(read_string_at(address, size))
}

/// Reads a string from a Place (field or array element).
pub fn place_string(place: &Place, size: Option<usize>) -> Option<Vec<u8>> {
    let address = match type_clone(place.ty).map(|ty| ty.kind) {
        Some(TypeKind::Pointer(_)) => place.pointer_value(),
        _ => place.addr,
    };
    if address == 0 {
        return Some(Vec::new());
    }
    Some(read_string_at(address, size))
}

/// Reads a NUL-terminated or explicitly sized byte string.
fn read_string_at(address: usize, size: Option<usize>) -> Vec<u8> {
    match size {
        Some(size) => {
            let mut bytes = vec![0u8; size];
            unsafe {
                std::ptr::copy_nonoverlapping(address as *const u8, bytes.as_mut_ptr(), size);
            }
            bytes
        }
        None => unsafe {
            let mut length = 0usize;
            while std::ptr::read(address.wrapping_add(length) as *const u8) != 0 {
                length += 1;
                if length > 1 << 30 {
                    break;
                }
            }
            let mut bytes = vec![0u8; length];
            std::ptr::copy_nonoverlapping(address as *const u8, bytes.as_mut_ptr(), length);
            bytes
        },
    }
}

/// Copies `size` bytes from one CData object to another.
pub fn memcpy_cdata(destination: usize, source: usize, size: usize) -> Result<(), ()> {
    let destination = cdata(destination).ok_or_else(|| {
        set_error(ERROR_EXCEPTION, "Invalid FFI\\CData");
    })?;
    let source = cdata(source).ok_or_else(|| {
        set_error(ERROR_EXCEPTION, "Invalid FFI\\CData");
    })?;
    if destination.check_alive().is_err() || source.check_alive().is_err() {
        set_error(ERROR_EXCEPTION, "Use after free()");
        return Err(());
    }
    if destination.readonly {
        set_error(ERROR_EXCEPTION, "Attempt to assign read-only location");
        return Err(());
    }
    let destination_address = data_address(destination);
    let source_address = data_address(source);
    if destination_address == 0 || source_address == 0 {
        set_error(ERROR_EXCEPTION, "null pointer dereference");
        return Err(());
    }
    unsafe {
        libc::memmove(
            destination_address as *mut libc::c_void,
            source_address as *const libc::c_void,
            size,
        );
    }
    Ok(())
}

/// Copies bytes from a PHP string into a CData object.
pub fn memcpy_string(destination: usize, source: &[u8], size: usize) -> Result<(), ()> {
    if size > source.len() {
        set_error(ERROR_EXCEPTION, "Attempt to read over string boundary");
        return Err(());
    }
    let destination = cdata(destination).ok_or_else(|| {
        set_error(ERROR_EXCEPTION, "Invalid FFI\\CData");
    })?;
    if destination.check_alive().is_err() {
        set_error(ERROR_EXCEPTION, "Use after free()");
        return Err(());
    }
    if destination.readonly {
        set_error(ERROR_EXCEPTION, "Attempt to assign read-only location");
        return Err(());
    }
    let address = data_address(destination);
    if address == 0 {
        set_error(ERROR_EXCEPTION, "null pointer dereference");
        return Err(());
    }
    unsafe {
        std::ptr::copy_nonoverlapping(source.as_ptr(), address as *mut u8, size);
    }
    Ok(())
}

/// Compares two CData objects for `memcmp`.
pub fn memcmp_cdata(left: usize, right: usize, size: usize) -> i32 {
    let (Some(left), Some(right)) = (cdata(left), cdata(right)) else {
        set_error(ERROR_EXCEPTION, "Invalid FFI\\CData");
        return 0;
    };
    if left.check_alive().is_err() || right.check_alive().is_err() {
        set_error(ERROR_EXCEPTION, "Use after free()");
        return 0;
    }
    let left_address = data_address(left);
    let right_address = data_address(right);
    if left_address == 0 || right_address == 0 {
        set_error(ERROR_EXCEPTION, "null pointer dereference");
        return 0;
    }
    let result = unsafe {
        libc::memcmp(
            left_address as *const libc::c_void,
            right_address as *const libc::c_void,
            size,
        )
    };
    result.signum()
}

/// Compares a CData object against a PHP string for `memcmp`.
pub fn memcmp_string(left: usize, right: &[u8], size: usize) -> i32 {
    if size > right.len() {
        set_error(ERROR_EXCEPTION, "Attempt to read over string boundary");
        return 0;
    }
    let Some(left) = cdata(left) else {
        set_error(ERROR_EXCEPTION, "Invalid FFI\\CData");
        return 0;
    };
    if left.check_alive().is_err() {
        set_error(ERROR_EXCEPTION, "Use after free()");
        return 0;
    }
    let left_address = data_address(left);
    if left_address == 0 {
        set_error(ERROR_EXCEPTION, "null pointer dereference");
        return 0;
    }
    let result = unsafe {
        libc::memcmp(
            left_address as *const libc::c_void,
            right.as_ptr() as *const libc::c_void,
            size,
        )
    };
    result.signum()
}

/// Fills a CData object's target bytes with `value`.
pub fn memset_cdata(handle: usize, value: i32, size: usize) -> Result<(), ()> {
    let data = cdata(handle).ok_or_else(|| {
        set_error(ERROR_EXCEPTION, "Invalid FFI\\CData");
    })?;
    if data.check_alive().is_err() {
        set_error(ERROR_EXCEPTION, "Use after free()");
        return Err(());
    }
    if data.readonly {
        set_error(ERROR_EXCEPTION, "Attempt to assign read-only location");
        return Err(());
    }
    let address = data_address(data);
    if address == 0 {
        set_error(ERROR_EXCEPTION, "null pointer dereference");
        return Err(());
    }
    unsafe {
        libc::memset(address as *mut libc::c_void, value, size);
    }
    Ok(())
}

/// Fills one place's bytes with `value`.
pub fn memset_place(place: &Place, value: i32, size: usize) -> Result<(), ()> {
    if place.readonly {
        set_error(ERROR_EXCEPTION, "Attempt to assign read-only location");
        return Err(());
    }
    if place.addr == 0 {
        set_error(ERROR_EXCEPTION, "null pointer dereference");
        return Err(());
    }
    unsafe {
        libc::memset(place.addr as *mut libc::c_void, value, size);
    }
    Ok(())
}

/// Copies `size` bytes from a CData object to one place.
pub fn memcpy_place_from_cdata(place: &Place, source: usize, size: usize) -> Result<(), ()> {
    let source = cdata(source).ok_or_else(|| {
        set_error(ERROR_EXCEPTION, "Invalid FFI\\CData");
    })?;
    if source.check_alive().is_err() {
        set_error(ERROR_EXCEPTION, "Use after free()");
        return Err(());
    }
    let source_address = data_address(source);
    if place.addr == 0 || source_address == 0 {
        set_error(ERROR_EXCEPTION, "null pointer dereference");
        return Err(());
    }
    unsafe {
        libc::memmove(
            place.addr as *mut libc::c_void,
            source_address as *const libc::c_void,
            size,
        );
    }
    Ok(())
}

/// Copies `size` bytes from a PHP string to one place.
pub fn memcpy_place_from_string(place: &Place, source: &[u8], size: usize) -> Result<(), ()> {
    if size > source.len() {
        set_error(ERROR_EXCEPTION, "Attempt to read over string boundary");
        return Err(());
    }
    if place.addr == 0 {
        set_error(ERROR_EXCEPTION, "null pointer dereference");
        return Err(());
    }
    unsafe {
        std::ptr::copy_nonoverlapping(source.as_ptr(), place.addr as *mut u8, size);
    }
    Ok(())
}

/// Returns whether a CData pointer place is NULL.
pub fn place_is_null(place: &Place) -> bool {
    let Some(ty) = type_clone(place.ty) else { return false };
    match ty.kind {
        TypeKind::Pointer(_) => place.pointer_value() == 0,
        _ => false,
    }
}
