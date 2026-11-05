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

thread_local! {
    static LAST_PLACE: std::cell::RefCell<Option<Place>> = const { std::cell::RefCell::new(None) };
}

/// Records a resolved place for the immediate `FFI::typeof`/`addr`/`sizeof` fallback.
fn remember(place: Place) -> Place {
    LAST_PLACE.with(|slot| {
        *slot.borrow_mut() = Some(place.clone());
    });
    place
}

/// Takes the most recently resolved place, if any.
pub fn take_last_place() -> Option<Place> {
    LAST_PLACE.with(|slot| slot.borrow_mut().take())
}

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
    pub bit_offset: u8,
    /// Read-only field/array/const location.
    pub readonly: bool,
    /// PHP-facing field name for diagnostics.
    pub label: Option<String>,
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
        )
    }

    /// Returns a CData for this place, remembering the place address on pointer copies.
    ///
    /// `FFI::free($struct->field)` / `FFI::free($array[$i])` must zero the originating field
    /// or element, not only the temporary CData copy. Aggregates stay views and need no
    /// source stamp.
    pub fn cdata_with_source(&self) -> usize {
        let handle = self.cdata();
        if let Some(data) = crate::context::cdata_mut(handle) {
            if matches!(
                type_clone(self.ty).map(|ty| ty.kind),
                Some(TypeKind::Pointer(_))
            ) {
                data.source = self.addr;
            }
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
            return Ok(remember(place));
        }
        set_error(
            ERROR_EXCEPTION,
            format!("Attempt to access field '{}' of non C struct/union", name),
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
                format!("Attempt to access field '{}' of non C struct/union", name),
            );
            return Err(());
        }
        return Ok(remember(Place {
            addr: data.addr,
            ty: data.ty,
            bit_width: data.bit_width,
            bit_offset: data.bit_offset,
            readonly: data.readonly || ty.is_const,
            label: Some(name.to_string()),
        }));
    }
    set_error(
        ERROR_EXCEPTION,
        format!("Attempt to access field '{}' of non C struct/union", name),
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
                        format!("Attempt to access element {} of C array of size {}", index, len),
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
            set_error(ERROR_EXCEPTION, "Attempt to access element of non C array");
            return Err(());
        }
    };
    let Some(element_ty) = type_clone(element) else {
        set_error(ERROR_EXCEPTION, "Invalid FFI\\CData");
        return Err(());
    };
    let stride = element_ty.size.max(1);
    let addr = (base as isize).wrapping_add((index as isize).wrapping_mul(stride as isize)) as usize;
    Ok(remember(Place {
        addr,
        ty: element,
        bit_width: None,
        bit_offset: 0,
        readonly: data.readonly || element_ty.is_const,
        label: None,
    }))
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
        let bits = usize::from(place.bit_offset) + usize::from(width);
        let raw = read_bits(place.addr, bits);
        let value = raw >> place.bit_offset;
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
        TypeKind::Float { .. } => unsafe { std::ptr::read_unaligned(place.addr as *const f64) },
        _ => read_int(place) as f64,
    }
}

/// Reads a bool place.
pub fn read_bool(place: &Place) -> bool {
    let Some(ty) = type_clone(place.ty) else { return false };
    match ty.kind {
        TypeKind::Bool => read_u8(place.addr) != 0,
        TypeKind::Int { .. } => read_int(place) != 0,
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
        let bits = usize::from(place.bit_offset) + usize::from(width);
        let raw = read_bits(place.addr, bits);
        let mask = if width == 64 { u64::MAX as u128 } else { (1u128 << width) - 1 };
        let shifted = ((value as u128) & mask) << place.bit_offset;
        let merged = (raw & !(mask << place.bit_offset)) | shifted;
        write_bits(place.addr, bits, merged);
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
        TypeKind::Float { .. } => unsafe {
            std::ptr::write_unaligned(place.addr as *mut f64, value);
        },
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
        TypeKind::Int { .. } => write_int(place, i64::from(value))?,
        _ => return Err(()),
    }
    Ok(())
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
        TypeKind::Pointer(_) => {
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
        TypeKind::Bool => write_bool(place, read_bool(&Place {
            addr: value.addr,
            ty: value.ty,
            bit_width: None,
            bit_offset: 0,
            readonly: true,
            label: None,
        })),
        TypeKind::Char { .. } => write_char(place, &[read_u8(value.addr)]),
        TypeKind::Float { .. } => write_float(place, read_float(&Place {
            addr: value.addr,
            ty: value.ty,
            bit_width: None,
            bit_offset: 0,
            readonly: true,
            label: None,
        })),
        TypeKind::Int { .. } | TypeKind::Enum { .. } => write_int(place, read_int(&Place {
            addr: value.addr,
            ty: value.ty,
            bit_width: None,
            bit_offset: 0,
            readonly: true,
            label: None,
        })),
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
    let digits_start = end;
    while end < bytes.len() && bytes[end].is_ascii_digit() {
        end += 1;
    }
    if end == digits_start {
        return &bytes[start..start];
    }
    if end < bytes.len() && bytes[end] == b'.' {
        let fraction_start = end + 1;
        let mut fraction_end = fraction_start;
        while fraction_end < bytes.len() && bytes[fraction_end].is_ascii_digit() {
            fraction_end += 1;
        }
        if fraction_end > fraction_start {
            end = fraction_end;
        }
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

fn read_u8(addr: usize) -> u8 {
    unsafe { std::ptr::read_unaligned(addr as *const u8) }
}

fn write_u8(addr: usize, value: u8) {
    unsafe {
        std::ptr::write_unaligned(addr as *mut u8, value);
    }
}

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

fn read_bits(addr: usize, bits: usize) -> u128 {
    let bytes = bits.div_ceil(8).min(16);
    let mut value = 0u128;
    for index in 0..bytes {
        value |= u128::from(read_u8(addr.wrapping_add(index))) << (index * 8);
    }
    value
}

fn write_bits(addr: usize, bits: usize, value: u128) {
    let bytes = bits.div_ceil(8).min(16);
    for index in 0..bytes {
        write_u8(addr.wrapping_add(index), (value >> (index * 8)) as u8);
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
        let place = Place {
            addr: bytes.as_mut_ptr() as usize,
            ty: 0,
            bit_width: Some(3),
            bit_offset: 1,
            readonly: false,
            label: None,
        };
        for value in -4i64..=3 {
            let masked = value & 0x7;
            let raw = (masked as u128) << 1;
            write_bits(place.addr, 10, raw);
            let read = read_bits(place.addr, 10) >> 1 & 0x7;
            assert_eq!(read, masked as u128, "{value}");
        }
        let _ = bytes;
    }
}
