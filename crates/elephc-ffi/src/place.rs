//! Purpose:
//! Resolves CData struct fields and array/pointer elements into addressable places,
//! then reads and writes those places with PHP's FFI scalar, bit-field, pointer, and
//! aggregate-assignment semantics.
//!
//! Called from:
//! - `crate::abi` CData field/offset entry points and `crate::memory`.
//!
//! Key details:
//! - A place carries the byte address, type, optional bit-field window, and read-only
//!   state. Anonymous struct/union members are promoted by recursive field lookup.
//! - Signed loads sign-extend, unsigned loads zero-extend, and stores keep the low bits
//!   exactly like a C conversion.
//! - Pointer writes accept another pointer (its pointer value), an array/struct (its
//!   data address), or null. Aggregate writes copy bytes after a size check.

use crate::context::{
    cdata, data_address, pointer_to, set_error, type_clone, ERROR_EXCEPTION,
};
use crate::types::{CType, Field, TypeId, TypeKind};

/// Place value kinds returned to the PHP prelude.
pub const KIND_NONE: u32 = 0;
pub const KIND_INT: u32 = 1;
pub const KIND_FLOAT: u32 = 2;
pub const KIND_BOOL: u32 = 3;
pub const KIND_CHAR: u32 = 4;
pub const KIND_CDATA: u32 = 5;
pub const KIND_NULL: u32 = 6;

/// One addressable C storage location.
#[derive(Clone, Debug)]
pub struct Place {
    /// Byte address of the field or element.
    pub addr: usize,
    /// Type of the storage.
    pub ty: TypeId,
    /// Bit-field width, if any.
    pub bit_width: Option<u8>,
    /// Bit offset inside the field's storage unit.
    pub bit_offset: u16,
    /// Read-only field/array/const location.
    pub readonly: bool,
    /// PHP-facing field name for diagnostics.
    pub label: Option<String>,
    /// Parent CData handle whose allocation backs this place (0 for globals/temporaries).
    pub parent: usize,
}

impl Place {
    /// Returns the place's PHP-visible value kind.
    pub fn kind(&self) -> u32 {
        let Some(ty) = type_clone(self.ty) else { return KIND_NONE };
        match ty.kind {
            TypeKind::Void | TypeKind::Func(_) => KIND_NONE,
            TypeKind::Bool => KIND_BOOL,
            TypeKind::Char { .. } => KIND_CHAR,
            TypeKind::Int { .. } | TypeKind::Enum { .. } => KIND_INT,
            TypeKind::Float { .. } => KIND_FLOAT,
            TypeKind::Pointer(_) => {
                let pointer = unsafe { std::ptr::read_unaligned(self.addr as *const usize) };
                if pointer == 0 { KIND_NULL } else { KIND_CDATA }
            }
            TypeKind::Array { .. } | TypeKind::Struct(_) | TypeKind::IncompleteTag { .. } => KIND_CDATA,
        }
    }

    /// Returns a CData value for this place.
    ///
    /// Pointer fields copy the pointer value into independent CData storage, matching PHP: a
    /// CData read from a pointer field keeps pointing at the same object when the field itself
    /// is later rewritten. Aggregate/array fields stay views so nested mutations write through.
    pub fn cdata(&self) -> usize {
        if let Some(ty) = type_clone(self.ty) {
            if let TypeKind::Pointer(inner) = ty.kind {
                return pointer_to(self.pointer_value(), inner).unwrap_or(0);
            }
        }
        crate::context::view_bits(
            self.addr,
            self.ty,
            self.readonly,
            self.bit_width,
            self.bit_offset,
            self.parent,
        )
    }

    /// Returns a CData for this place, remembering the place address on pointer copies.
    ///
    /// `FFI::free($struct->field)` / `FFI::free($array[$i])` must zero the originating field
    /// or element, not only the temporary CData copy. Aggregates stay views and need no
    /// source stamp.
    pub fn cdata_with_source(&self) -> usize {
        let handle = self.cdata();
        if matches!(
            type_clone(self.ty).map(|ty| ty.kind),
            Some(TypeKind::Pointer(_))
        ) {
            if let Some(data) = crate::context::cdata_mut(handle) {
                data.source = self.addr;
            }
            // The copy may write the originating slot when freed. Keep the parent allocation
            // alive so a PHP destructor that released the container cannot leave `source`
            // pointing into freed memory.
            crate::context::retain_source_parent(handle, self.parent);
        }
        handle
    }

    /// Returns the pointer value for a pointer place.
    pub fn pointer_value(&self) -> usize {
        unsafe { std::ptr::read_unaligned(self.addr as *const usize) }
    }
}

/// Resolves a named struct/union field, promoting anonymous members.
pub fn field_place(handle: usize, name: &str) -> Result<Place, ()> {
    let Some(data) = cdata(handle) else {
        set_error(ERROR_EXCEPTION, "Invalid FFI\\CData");
        return Err(());
    };
    if data.check_alive().is_err() {
        set_error(ERROR_EXCEPTION, "Use after free()");
        return Err(());
    }
    let Some(ty) = type_clone(data.ty) else {
        set_error(ERROR_EXCEPTION, "Invalid FFI\\CData");
        return Err(());
    };
    let record_base = match &ty.kind {
        TypeKind::Struct(record) => Some((record.clone(), data.addr, data.readonly)),
        TypeKind::Pointer(inner) => {
            let inner_ty = type_clone(*inner);
            match inner_ty.as_ref().map(|inner| &inner.kind) {
                Some(TypeKind::Struct(record)) => {
                    let base = unsafe { std::ptr::read_unaligned(data.addr as *const usize) };
                    if base == 0 {
                        set_error(ERROR_EXCEPTION, "null pointer dereference");
                        return Err(());
                    }
                    // php-src permits writes through a `const struct S *` CData pointer: the
                    // pointer slot's own const qualifier governs `data.readonly`, not the
                    // pointee declaration.
                    Some((record.clone(), base, data.readonly))
                }
                _ => None,
            }
        }
        _ => None,
    };
    if let Some((record, base, readonly)) = record_base {
        if let Some(mut place) = find_field(&record.fields, base, readonly, name) {
            if readonly {
                place.label = None;
            }
            place.parent = handle;
            return Ok(place);
        }
        set_error(
            ERROR_EXCEPTION,
            format!("Attempt to read undefined field '{}' of C struct/union", name),
        );
        return Err(());
    }
    if name == "cdata" {
        if matches!(
            ty.kind,
            TypeKind::Pointer(_) | TypeKind::Array { .. } | TypeKind::Struct(_)
        ) {
            set_error(
                ERROR_EXCEPTION,
                format!("Attempt to read field '{}' of non C struct/union", name),
            );
            return Err(());
        }
        return Ok(Place {
            addr: data.addr,
            ty: data.ty,
            bit_width: data.bit_width,
            bit_offset: data.bit_offset,
            readonly: data.readonly || ty.is_const,
            label: Some(name.to_string()),
            parent: handle,
        });
    }
    set_error(
        ERROR_EXCEPTION,
        format!("Attempt to read field '{}' of non C struct/union", name),
    );
    Err(())
}

/// Recursively looks up a field in a struct/union, descending through anonymous members.
fn find_field(fields: &[Field], base: usize, parent_readonly: bool, name: &str) -> Option<Place> {
    for field in fields {
        if field.name.as_deref() == Some(name) {
            return Some(Place {
                addr: base.wrapping_add(field.offset),
                ty: field.ty,
                bit_width: field.bit_width,
                bit_offset: field.bit_offset,
                readonly: parent_readonly || field.readonly,
                label: field.name.clone(),
                parent: 0,
            });
        }
        if field.name.is_none() {
            let ty = type_clone(field.ty)?;
            if let TypeKind::Struct(record) = &ty.kind {
                if let Some(place) = find_field(
                    &record.fields,
                    base.wrapping_add(field.offset),
                    parent_readonly || field.readonly,
                    name,
                ) {
                    return Some(place);
                }
            }
        }
    }
    None
}

/// Resolves an array element or pointer target at `index`.
pub fn offset_place(handle: usize, index: i64) -> Result<Place, ()> {
    let Some(data) = cdata(handle) else {
        set_error(ERROR_EXCEPTION, "Invalid FFI\\CData");
        return Err(());
    };
    if data.check_alive().is_err() {
        set_error(ERROR_EXCEPTION, "Use after free()");
        return Err(());
    }
    let Some(ty) = type_clone(data.ty) else {
        set_error(ERROR_EXCEPTION, "Invalid FFI\\CData");
        return Err(());
    };
    let (base, element) = match ty.kind {
        TypeKind::Array { element, len } => {
            if let Some(len) = len {
                if index < 0 || index as usize >= len {
                    set_error(
                        ERROR_EXCEPTION,
                        "C array index out of bounds".to_string(),
                    );
                    return Err(());
                }
            }
            (data.addr, element)
        }
        TypeKind::Pointer(element) => {
            let pointer = unsafe { std::ptr::read_unaligned(data.addr as *const usize) };
            if pointer == 0 {
                set_error(ERROR_EXCEPTION, "null pointer dereference");
                return Err(());
            }
            (pointer, element)
        }
        _ => {
            set_error(ERROR_EXCEPTION, "Attempt to read element of non C array");
            return Err(());
        }
    };
    let Some(element_ty) = type_clone(element) else {
        set_error(ERROR_EXCEPTION, "Invalid FFI\\CData");
        return Err(());
    };
    let stride = element_ty.size.max(1);
    let addr = (base as isize).wrapping_add((index as isize).wrapping_mul(stride as isize)) as usize;
    Ok(Place {
        addr,
        ty: element,
        bit_width: None,
        bit_offset: 0,
        readonly: data.readonly || element_ty.is_const,
        label: None,
        parent: handle,
    })
}

/// Returns a pointer CData addressing one place (the `FFI::addr()` interception path).
pub fn addr_to_place(place: &Place) -> Option<usize> {
    pointer_to(place.addr, place.ty)
}

/// Reads an integer-like place.
pub fn read_int(place: &Place) -> i64 {
    let Some(ty) = type_clone(place.ty) else { return 0 };
    read_int_typed(place, &ty)
}

/// Reads one typed integer place.
fn read_int_typed(place: &Place, ty: &CType) -> i64 {
    if let Some(width) = place.bit_width {
        let value = read_bits(place.addr, place.bit_offset, width);
        let mask = if width == 64 { u64::MAX as u128 } else { (1u128 << width) - 1 };
        let value = (value & mask) as u64;
        let signed = integer_signed(ty);
        if signed && width > 0 {
            let shift = 64 - u32::from(width);
            return ((value << shift) as i64) >> shift;
        }
        return value as i64;
    }
    match &ty.kind {
        TypeKind::Bool => i64::from(read_u8(place.addr) != 0),
        TypeKind::Char { signed } => {
            let byte = read_u8(place.addr);
            if *signed { i64::from(byte as i8) } else { i64::from(byte) }
        }
        TypeKind::Int { bits, signed } => read_integer(place.addr, *bits, *signed),
        TypeKind::Enum { underlying, .. } => type_clone(*underlying)
            .map(|inner| read_int_typed(place, &inner))
            .unwrap_or(0),
        TypeKind::Pointer(_) => unsafe { std::ptr::read_unaligned(place.addr as *const usize) as i64 },
        _ => 0,
    }
}

/// Returns whether an integer type sign-extends on load.
fn integer_signed(ty: &CType) -> bool {
    match ty.kind {
        TypeKind::Int { signed, .. } | TypeKind::Char { signed } => signed,
        TypeKind::Enum { underlying, .. } => {
            type_clone(underlying).is_some_and(|inner| integer_signed(&inner))
        }
        _ => false,
    }
}

/// Reads a float place.
pub fn read_float(place: &Place) -> f64 {
    let Some(ty) = type_clone(place.ty) else { return 0.0 };
    match ty.kind {
        TypeKind::Float { bits: 32 } => f64::from(unsafe { std::ptr::read_unaligned(place.addr as *const f32) }),
        TypeKind::Float { bits: 64 } => unsafe { std::ptr::read_unaligned(place.addr as *const f64) },
        TypeKind::Float { .. } => crate::long_double::read(place.addr, ty.size),
        _ => read_int(place) as f64,
    }
}

/// Reads a bool place.
pub fn read_bool(place: &Place) -> bool {
    let Some(ty) = type_clone(place.ty) else { return false };
    match ty.kind {
        TypeKind::Bool => read_u8(place.addr) != 0,
        TypeKind::Int { .. } | TypeKind::Char { .. } | TypeKind::Enum { .. } | TypeKind::Pointer(_) => {
            read_int(place) != 0
        }
        TypeKind::Float { .. } => read_float(place) != 0.0,
        _ => false,
    }
}

/// Reads a `char` place as its raw byte.
pub fn read_char(place: &Place) -> u8 {
    read_u8(place.addr)
}

/// Writes an integer place, honoring bit-field windows and C truncation.
pub fn write_int(place: &Place, value: i64) -> Result<(), ()> {
    if place.readonly {
        return Err(readonly_error(place));
    }
    let Some(ty) = type_clone(place.ty) else { return Err(()) };
    if let Some(width) = place.bit_width {
        write_bits(place.addr, place.bit_offset, width, value as u128);
        return Ok(());
    }
    match &ty.kind {
        TypeKind::Bool => write_u8(place.addr, u8::from(value != 0)),
        TypeKind::Char { .. } => write_u8(place.addr, value as u8),
        TypeKind::Int { bits, .. } => write_integer(place.addr, *bits, value),
        TypeKind::Enum { underlying, .. } => {
            if let Some(inner) = type_clone(*underlying) {
                match inner.kind {
                    TypeKind::Int { bits, .. } => write_integer(place.addr, bits, value),
                    _ => write_integer(place.addr, 32, value),
                }
            }
        }
        TypeKind::Pointer(_) => unsafe {
            std::ptr::write_unaligned(place.addr as *mut usize, value as usize);
        },
        _ => return Err(()),
    }
    Ok(())
}

/// Writes a float place.
pub fn write_float(place: &Place, value: f64) -> Result<(), ()> {
    if place.readonly {
        return Err(readonly_error(place));
    }
    let Some(ty) = type_clone(place.ty) else { return Err(()) };
    match ty.kind {
        TypeKind::Float { bits: 32 } => unsafe {
            std::ptr::write_unaligned(place.addr as *mut f32, value as f32);
        },
        TypeKind::Float { bits: 64 } => unsafe {
            std::ptr::write_unaligned(place.addr as *mut f64, value);
        },
        TypeKind::Float { .. } => crate::long_double::write(place.addr, ty.size, value),
        TypeKind::Int { .. } => {
            write_int(place, value as i64)?;
        }
        _ => return Err(()),
    }
    Ok(())
}

/// Writes a bool place.
pub fn write_bool(place: &Place, value: bool) -> Result<(), ()> {
    if place.readonly {
        return Err(readonly_error(place));
    }
    let Some(ty) = type_clone(place.ty) else { return Err(()) };
    match ty.kind {
        TypeKind::Bool => write_u8(place.addr, u8::from(value)),
        TypeKind::Int { .. } | TypeKind::Enum { .. } => write_int(place, i64::from(value))?,
        TypeKind::Float { .. } => write_float(place, if value { 1.0 } else { 0.0 })?,
        // PHP's bool-to-string coercion makes `true` the one-byte string "1"; `false`
        // becomes the empty string, which `char` rejects like any other wrong-length value.
        TypeKind::Char { .. } if value => write_char(place, b"1")?,
        _ => return incompatible_bool(&ty, value),
    }
    Ok(())
}

/// Records php-src's incompatible bool-assignment diagnostic for `ty`.
fn incompatible_bool(ty: &CType, value: bool) -> Result<(), ()> {
    set_error(
        ERROR_EXCEPTION,
        format!(
            "Incompatible types when assigning to type '{}' from PHP '{}'",
            ty.display,
            if value { "true" } else { "false" }
        ),
    );
    Err(())
}

/// Writes a `char` place from a one-byte PHP string.
pub fn write_char(place: &Place, bytes: &[u8]) -> Result<(), ()> {
    if place.readonly {
        return Err(readonly_error(place));
    }
    write_u8(place.addr, bytes.first().copied().unwrap_or(0));
    Ok(())
}

/// Writes a pointer place from a CData value, pointer-sized integer, or null.
pub fn write_pointer(place: &Place, value: Option<usize>) -> Result<(), ()> {
    if place.readonly {
        return Err(readonly_error(place));
    }
    let Some(ty) = type_clone(place.ty) else { return Err(()) };
    if !matches!(ty.kind, TypeKind::Pointer(_)) {
        return Err(());
    }
    unsafe {
        std::ptr::write_unaligned(place.addr as *mut usize, value.unwrap_or(0));
    }
    Ok(())
}

/// Builds a read-only place over a standalone CData value.
fn value_place(value: &crate::context::CData) -> Place {
    Place {
        addr: value.addr,
        ty: value.ty,
        bit_width: None,
        bit_offset: 0,
        readonly: true,
        label: None,
        parent: 0,
    }
}

/// Returns whether one CData kind can be converted to a scalar destination.
fn scalar_source(source: &CType) -> bool {
    !matches!(
        source.kind,
        TypeKind::Array { .. }
            | TypeKind::Struct(_)
            | TypeKind::IncompleteTag { .. }
            | TypeKind::Void
    )
}

/// Records php-src's incompatible CData assignment diagnostic.
fn incompatible_cdata(target: &CType, source: &CType) -> Result<(), ()> {
    set_error(
        ERROR_EXCEPTION,
        format!(
            "Incompatible types when assigning to type '{}' from type '{}'",
            target.display, source.display
        ),
    );
    Err(())
}

/// Writes a CData value into a place, choosing pointer/scalar/aggregate semantics.
pub fn write_cdata(place: &Place, value_handle: usize) -> Result<(), ()> {
    if place.readonly {
        return Err(readonly_error(place));
    }
    let Some(value) = cdata(value_handle) else {
        set_error(ERROR_EXCEPTION, "Invalid FFI\\CData");
        return Err(());
    };
    if value.check_alive().is_err() {
        set_error(ERROR_EXCEPTION, "Use after free()");
        return Err(());
    }
    let Some(target) = type_clone(place.ty) else { return Err(()) };
    let Some(source) = type_clone(value.ty) else { return Err(()) };
    match &target.kind {
        TypeKind::Pointer(target_inner) => {
            if let Some(target_func) = type_clone(*target_inner).and_then(|ty| match ty.kind {
                TypeKind::Func(function) => Some(function),
                _ => None,
            }) {
                let source_func = match &source.kind {
                    TypeKind::Pointer(source_inner) => {
                        type_clone(*source_inner).and_then(|ty| match ty.kind {
                            TypeKind::Func(function) => Some(function),
                            _ => None,
                        })
                    }
                    _ => None,
                };
                if !source_func
                    .is_some_and(|source_func| crate::abi::function_signatures_match(&target_func, &source_func))
                {
                    return incompatible_cdata(&target, &source);
                }
            }
            let pointer = if matches!(source.kind, TypeKind::Pointer(_)) {
                value.pointer_value()
            } else {
                data_address(value)
            };
            write_pointer(place, Some(pointer))
        }
        TypeKind::Array { .. } | TypeKind::Struct(_) => {
            if target.size != source.size {
                set_error(
                    ERROR_EXCEPTION,
                    "Incompatible types when assigning to C aggregate",
                );
                return Err(());
            }
            unsafe {
                libc::memmove(place.addr as *mut libc::c_void, value.addr as *const libc::c_void, target.size);
            }
            Ok(())
        }
        TypeKind::Bool => {
            if !scalar_source(&source) {
                return incompatible_cdata(&target, &source);
            }
            write_bool(place, read_bool(&value_place(value)))
        }
        TypeKind::Char { .. } => {
            // php-src only accepts another `char` CData for a character assignment; an
            // integer, float, or aggregate source is an incompatible-types exception.
            if !matches!(source.kind, TypeKind::Char { .. }) {
                return incompatible_cdata(&target, &source);
            }
            write_char(place, &[read_u8(value.addr)])
        }
        TypeKind::Float { .. } => {
            if !scalar_source(&source) {
                return incompatible_cdata(&target, &source);
            }
            let source_place = value_place(value);
            let converted = if matches!(source.kind, TypeKind::Float { .. }) {
                read_float(&source_place)
            } else {
                read_int(&source_place) as f64
            };
            write_float(place, converted)
        }
        TypeKind::Int { .. } | TypeKind::Enum { .. } => {
            if !scalar_source(&source) {
                return incompatible_cdata(&target, &source);
            }
            let source_place = value_place(value);
            let converted = if matches!(source.kind, TypeKind::Float { .. }) {
                read_float(&source_place) as i64
            } else {
                read_int(&source_place)
            };
            write_int(place, converted)
        }
        _ => Err(()),
    }
}

/// Records php-src's incompatible string-assignment diagnostic for `ty`.
fn incompatible_string(ty: &CType) -> Result<(), ()> {
    set_error(
        ERROR_EXCEPTION,
        format!(
            "Incompatible types when assigning to type '{}' from PHP 'string'",
            ty.display
        ),
    );
    Err(())
}

/// Returns the leading PHP numeric-string prefix, or an empty slice when there is none.
fn numeric_prefix(bytes: &[u8]) -> &[u8] {
    let mut start = 0;
    while start < bytes.len() && bytes[start].is_ascii_whitespace() {
        start += 1;
    }
    let mut end = start;
    if end < bytes.len() && (bytes[end] == b'+' || bytes[end] == b'-') {
        end += 1;
    }
    let mut has_digit = false;
    while end < bytes.len() && bytes[end].is_ascii_digit() {
        end += 1;
        has_digit = true;
    }
    if end < bytes.len() && bytes[end] == b'.' {
        let fraction_start = end + 1;
        let mut fraction_end = fraction_start;
        while fraction_end < bytes.len() && bytes[fraction_end].is_ascii_digit() {
            fraction_end += 1;
        }
        if fraction_end > fraction_start {
            end = fraction_end;
            has_digit = true;
        }
    }
    // PHP numeric strings accept a leading decimal point (".5", "-.5") as long as a
    // fractional digit follows; the empty/invalid cases still yield no prefix.
    if !has_digit {
        return &bytes[start..start];
    }
    if end < bytes.len() && (bytes[end] == b'e' || bytes[end] == b'E') {
        let mut exponent_end = end + 1;
        if exponent_end < bytes.len() && (bytes[exponent_end] == b'+' || bytes[exponent_end] == b'-') {
            exponent_end += 1;
        }
        let exponent_digits = exponent_end;
        while exponent_end < bytes.len() && bytes[exponent_end].is_ascii_digit() {
            exponent_end += 1;
        }
        if exponent_end > exponent_digits {
            end = exponent_end;
        }
    }
    &bytes[start..end]
}

/// Parses a PHP numeric-string prefix as an integer, matching `zval_get_long`.
fn parse_numeric_i64(bytes: &[u8]) -> i64 {
    let prefix = numeric_prefix(bytes);
    let Ok(text) = std::str::from_utf8(prefix) else { return 0 };
    if text.is_empty() {
        return 0;
    }
    if text.contains(['.', 'e', 'E']) {
        text.parse::<f64>().map(|value| value as i64).unwrap_or(0)
    } else {
        text.parse::<i64>()
            .or_else(|_| text.parse::<f64>().map(|value| value as i64))
            .unwrap_or(0)
    }
}

/// Parses a PHP numeric-string prefix as a float, matching `zval_get_double`.
fn parse_numeric_f64(bytes: &[u8]) -> f64 {
    let prefix = numeric_prefix(bytes);
    let Ok(text) = std::str::from_utf8(prefix) else { return 0.0 };
    text.parse::<f64>().unwrap_or(0.0)
}

/// Writes a PHP string into a scalar place, matching php-src's assignment conversions.
///
/// php-src accepts a one-byte string for `char` and rejects every string assigned to a C
/// array (including `char[N]`). Numeric scalar places go through PHP's numeric-string prefix
/// conversion; pointers and aggregates raise the engine's incompatible-types exception.
pub fn write_string(place: &Place, bytes: &[u8]) -> Result<(), ()> {
    if place.readonly {
        return Err(readonly_error(place));
    }
    let Some(ty) = type_clone(place.ty) else { return Err(()) };
    match ty.kind {
        TypeKind::Char { .. } => {
            if bytes.len() != 1 {
                return incompatible_string(&ty);
            }
            write_char(place, bytes)
        }
        TypeKind::Array { .. } => incompatible_string(&ty),
        TypeKind::Bool | TypeKind::Int { .. } | TypeKind::Enum { .. } => {
            write_int(place, parse_numeric_i64(bytes))
        }
        TypeKind::Float { .. } => write_float(place, parse_numeric_f64(bytes)),
        _ => incompatible_string(&ty),
    }
}

/// Frees a pointer place's target and zeroes the slot, matching `FFI::free($element)`.
pub fn free_place(place: &Place) -> Result<(), ()> {
    let Some(ty) = type_clone(place.ty) else { return Err(()) };
    if matches!(ty.kind, TypeKind::Pointer(_)) {
        let pointer = unsafe { std::ptr::read_unaligned(place.addr as *const usize) };
        if pointer != 0 {
            unsafe {
                libc::free(pointer as *mut libc::c_void);
            }
        }
        unsafe {
            std::ptr::write_unaligned(place.addr as *mut usize, 0);
        }
        return Ok(());
    }
    set_error(ERROR_EXCEPTION, "FFI::free() expects a C pointer");
    Err(())
}

/// Builds the exact read-only diagnostic for a place.
fn readonly_error(place: &Place) -> () {
    match &place.label {
        Some(field) => set_error(
            ERROR_EXCEPTION,
            format!("Attempt to assign read-only field '{}'", field),
        ),
        None => set_error(ERROR_EXCEPTION, "Attempt to assign read-only location"),
    }
}

/// Reads one unsigned byte from `addr`.
fn read_u8(addr: usize) -> u8 {
    unsafe { std::ptr::read_unaligned(addr as *const u8) }
}

/// Writes one unsigned byte to `addr`.
fn write_u8(addr: usize, value: u8) {
    unsafe {
        std::ptr::write_unaligned(addr as *mut u8, value);
    }
}

/// Reads a little-endian integer of `bits` width, sign-extending when requested.
fn read_integer(addr: usize, bits: u8, signed: bool) -> i64 {
    match bits {
        8 => {
            let value = read_u8(addr);
            if signed { i64::from(value as i8) } else { i64::from(value) }
        }
        16 => {
            let value = unsafe { std::ptr::read_unaligned(addr as *const u16) };
            if signed { i64::from(value as i16) } else { i64::from(value) }
        }
        32 => {
            let value = unsafe { std::ptr::read_unaligned(addr as *const u32) };
            if signed { i64::from(value as i32) } else { i64::from(value) }
        }
        _ => unsafe { std::ptr::read_unaligned(addr as *const i64) },
    }
}

/// Writes the low `bits` of `value` at `addr` in little-endian order.
fn write_integer(addr: usize, bits: u8, value: i64) {
    match bits {
        8 => write_u8(addr, value as u8),
        16 => unsafe {
            std::ptr::write_unaligned(addr as *mut u16, value as u16);
        },
        32 => unsafe {
            std::ptr::write_unaligned(addr as *mut u32, value as u32);
        },
        _ => unsafe {
            std::ptr::write_unaligned(addr as *mut i64, value);
        },
    }
}

/// Reads `bits` contiguous bits starting at `addr` into a little-endian integer.
fn read_bits(addr: usize, bit_offset: u16, width: u8) -> u128 {
    // Walk the window bit by bit: consecutive C bit-fields share a stream that can start past
    // the first 16 bytes (three 64-bit fields put the third at bit 128), where a fixed u128
    // window truncates or overflows.
    let mut value = 0u128;
    for bit in 0..usize::from(width) {
        let position = usize::from(bit_offset) + bit;
        let byte = read_u8(addr.wrapping_add(position / 8));
        value |= u128::from((byte >> (position % 8)) & 1) << bit;
    }
    value
}

/// Writes the low `width` bits of `value` at `bit_offset` without disturbing neighbours.
fn write_bits(addr: usize, bit_offset: u16, width: u8, value: u128) {
    for bit in 0..usize::from(width) {
        let position = usize::from(bit_offset) + bit;
        let address = addr.wrapping_add(position / 8);
        let mask = 1u8 << (position % 8);
        let current = read_u8(address);
        let next = if (value >> bit) & 1 == 1 { current | mask } else { current & !mask };
        write_u8(address, next);
    }
}

#[cfg(test)]
mod tests {
    //! Purpose:
    //! Unit tests for packed and ordinary bit-field place reads/writes.
    //!
    //! Called from:
    //! - `cargo test -p elephc-ffi`.
    //!
    //! Key details:
    //! - The test allocates a zeroed buffer directly, so it isolates bit math from CData.

    use super::*;

    /// A 3-bit field round-trips signed values inside one byte.
    #[test]
    fn bitfield_read_write_round_trips() {
        let mut bytes = [0u8; 4];
        for value in -4i64..=3 {
            let masked = value & 0x7;
            write_bits(bytes.as_mut_ptr() as usize, 1, 3, masked as u128);
            assert_eq!(
                read_bits(bytes.as_ptr() as usize, 1, 3),
                masked as u128,
                "{value}"
            );
        }
    }

    /// A 64-bit field at bit 128 round-trips; consecutive fields cross the old 16-byte window.
    #[test]
    fn bitfield_offsets_past_sixteen_bytes_round_trip() {
        let ty = crate::context::with_store(|_, names| names["uint64_t"]);
        let expected = 0x0123_4567_89ab_cdef_u64;
        let mut bytes = [0u8; 24];
        let place = Place {
            addr: bytes.as_mut_ptr() as usize,
            ty,
            bit_width: Some(64),
            bit_offset: 128,
            readonly: false,
            label: None,
            parent: 0,
        };
        write_int(&place, expected as i64).expect("write");
        assert_eq!(read_int(&place) as u64, expected);
    }
}
