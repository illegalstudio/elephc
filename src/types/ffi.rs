//! Purpose:
//! Defines FFI type and declaration metadata used by checking and native call lowering.
//! Maps compiler extension types into ABI-visible shapes for extern declarations.
//!
//! Called from:
//! - `crate::types::checker::extern_decl`
//! - `crate::codegen`
//!
//! Key details:
//! - FFI metadata must preserve target-independent type contracts while leaving register/stack layout to ABI helpers.
//! - Every C integer width maps to the PHP type `int` and C `float` to the PHP type `float`;
//!   `ExternAbiType` keeps the declared width so the call boundary can normalize registers
//!   without widening the PHP type system.
//! - `?string` is the PHP `string|null` union with its own ABI shape (`NullableCStr`), so the call
//!   boundary can pass PHP null as a NULL `char*` and read a NULL return back as PHP null.
//! - Extern class fields use the natural C layout of the same widths (`ctype_field_layout`), so a
//!   struct declared with the header's field types has the header's offsets and `sizeof`.
//! - Packed class fields and `buffer<T>` elements spell the same widths (`ScalarWidth`) as a
//!   storage format: the value still reads as `int` or `float`, and `packed_type_layout` gives
//!   every packed field type its natural size and alignment.

use std::collections::HashMap;

use crate::parser::ast::{CType, TypeExpr};

use super::{PackedClassInfo, PhpType};

/// Register-level shape of one extern parameter or return slot, derived from the declared C type.
///
/// The PHP-visible type of every integer slot is `int`; the width only tells the call boundary
/// how to normalize the register, because both supported ABIs leave the bits above a narrow
/// return value unspecified and expect the caller to extend narrow arguments.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ExternAbiType {
    /// Integer of `bits` width; `signed` selects sign or zero extension at the boundary.
    Int { bits: u8, signed: bool },
    /// C `double` in a floating-point register.
    Float64,
    /// C `float`: a single in the low half of the floating-point register, widened to the PHP double.
    Float32,
    /// C `bool`: only the low byte is defined.
    Bool,
    /// `char*` converted to and from a PHP string at the boundary.
    CStr,
    /// `char*` that may be NULL (`?string`): PHP null crosses as NULL in both directions, and any
    /// other value converts like `CStr`.
    NullableCStr,
    /// Any pointer, typed or opaque.
    Ptr,
    /// C function pointer bound from a PHP callable.
    Callable,
    /// No value (return only).
    Void,
}

impl ExternAbiType {
    /// The 64-bit signed integer shape of a plain `int`.
    pub const INT64: Self = Self::Int { bits: 64, signed: true };
}

/// Maps a parser CType to the ABI shape the call boundary normalizes.
pub fn ctype_to_abi_type(ct: &CType) -> ExternAbiType {
    match ct {
        CType::Int => ExternAbiType::INT64,
        CType::IntWidth(width) => ExternAbiType::Int { bits: width.bits(), signed: width.signed() },
        CType::Float => ExternAbiType::Float64,
        CType::F32 => ExternAbiType::Float32,
        CType::Str => ExternAbiType::CStr,
        CType::NullableStr => ExternAbiType::NullableCStr,
        CType::Bool => ExternAbiType::Bool,
        CType::Void => ExternAbiType::Void,
        CType::Ptr | CType::TypedPtr(_) => ExternAbiType::Ptr,
        CType::Callable => ExternAbiType::Callable,
    }
}

/// Maps a parser CType to its corresponding PhpType for FFI and extern declaration lowering.
///
/// Used when lowering `extern` declarations to connect the parser's C types (Int, IntWidth,
/// Float, F32, Str, NullableStr, Bool, Void, Ptr, TypedPtr, Callable) to the compiler's internal
/// PhpType representation. Every C integer width is a PHP `int`, both float widths are a PHP
/// `float`, and `?string` is the same `string|null` union a PHP `?string` declaration resolves to.
pub fn ctype_to_php_type(ct: &CType) -> PhpType {
    match ct {
        CType::Int | CType::IntWidth(_) => PhpType::Int,
        CType::Float | CType::F32 => PhpType::Float,
        CType::Str => PhpType::Str,
        CType::NullableStr => PhpType::Union(vec![PhpType::Str, PhpType::Void]),
        CType::Bool => PhpType::Bool,
        CType::Void => PhpType::Void,
        CType::Ptr => PhpType::Pointer(None),
        CType::TypedPtr(name) => PhpType::Pointer(Some(crate::names::resolved_pointer_target_tag(name))),
        CType::Callable => PhpType::Callable,
    }
}

/// Returns the natural C size and alignment, in bytes, of an extern class field type.
///
/// Every supported target (macOS and iOS arm64, Linux aarch64 and x86_64) is LP64 with
/// natural alignment for these scalars, so one table serves them all: `int`, `i64`, `u64`,
/// `float`/`f64`, pointers and `string` (`char*`) are 8 bytes, `i32`/`u32`/`f32` 4, `i16`/`u16` 2,
/// and `i8`/`u8`/`bool` (C `_Bool`) one byte, each aligned to its own size. Returns `None` for
/// `void` and `callable`, which cannot be stored in a struct field, and for `?string`, which
/// extern class fields do not support.
pub fn ctype_field_layout(ct: &CType) -> Option<(usize, usize)> {
    match ct {
        CType::Int | CType::Float | CType::Ptr | CType::TypedPtr(_) | CType::Str => Some((8, 8)),
        CType::IntWidth(width) => {
            let bytes = usize::from(width.bits() / 8);
            Some((bytes, bytes))
        }
        CType::F32 => Some((4, 4)),
        CType::Bool => Some((1, 1)),
        CType::Void | CType::Callable | CType::NullableStr => None,
    }
}

/// Rounds `offset` up to the next multiple of `align` (a power of two, at least one).
pub fn align_up(offset: usize, align: usize) -> usize {
    debug_assert!(align.is_power_of_two(), "C alignments are powers of two");
    (offset + align - 1) & !(align - 1)
}

/// Storage width of a packed class field or `buffer<T>` element declared with a C width name.
///
/// A width is a storage format, not a type: a field or element of any integer width reads as
/// the PHP `int` and an `f32` reads as the PHP `float`, with the read and write rules of the
/// extern class field of the same width (`abi()`): signed widths sign-extend, unsigned widths
/// zero-extend, a write keeps the low bits of an out-of-range integer like a C conversion, and
/// a float written to an `f32` rounds to single precision. The 8-byte names (`i64`, `u64`,
/// `f64`) are the plain `int` and `float` and never produce a width.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum ScalarWidth {
    I8,
    U8,
    I16,
    U16,
    I32,
    U32,
    F32,
}

impl ScalarWidth {
    /// Returns the size in bytes, which is also the natural alignment.
    pub fn size(self) -> usize {
        match self {
            Self::I8 | Self::U8 => 1,
            Self::I16 | Self::U16 => 2,
            Self::I32 | Self::U32 | Self::F32 => 4,
        }
    }

    /// Returns the C shape that selects the load extension and store truncation.
    pub fn abi(self) -> ExternAbiType {
        match self {
            Self::I8 => ExternAbiType::Int { bits: 8, signed: true },
            Self::U8 => ExternAbiType::Int { bits: 8, signed: false },
            Self::I16 => ExternAbiType::Int { bits: 16, signed: true },
            Self::U16 => ExternAbiType::Int { bits: 16, signed: false },
            Self::I32 => ExternAbiType::Int { bits: 32, signed: true },
            Self::U32 => ExternAbiType::Int { bits: 32, signed: false },
            Self::F32 => ExternAbiType::Float32,
        }
    }

    /// Returns the PHP type a field or element of this width reads as and is written from.
    pub fn value_type(self) -> PhpType {
        match self {
            Self::F32 => PhpType::Float,
            _ => PhpType::Int,
        }
    }

    /// Returns the width's source spelling.
    pub fn name(self) -> &'static str {
        match self {
            Self::I8 => "i8",
            Self::U8 => "u8",
            Self::I16 => "i16",
            Self::U16 => "u16",
            Self::I32 => "i32",
            Self::U32 => "u32",
            Self::F32 => "f32",
        }
    }
}

/// Resolves a C width name written as a packed field or buffer element type.
///
/// Narrow names become `PhpType::Narrow`; the 8-byte names are the plain `int` and `float`
/// because their storage is the same word. Any other name is not a width (`None`).
pub fn width_type_from_name(name: &str) -> Option<PhpType> {
    let width = match name {
        "i8" => ScalarWidth::I8,
        "u8" => ScalarWidth::U8,
        "i16" => ScalarWidth::I16,
        "u16" => ScalarWidth::U16,
        "i32" => ScalarWidth::I32,
        "u32" => ScalarWidth::U32,
        "f32" => ScalarWidth::F32,
        "i64" | "u64" => return Some(PhpType::Int),
        "f64" => return Some(PhpType::Float),
        _ => return None,
    };
    Some(PhpType::Narrow(width))
}

/// Suggests the width a misspelled packed field or buffer element type probably meant.
///
/// Matches the width names in any letter case (`I32`, `U8`) and the common C spellings
/// (`int32_t`, `uint8`, `double`, `size_t`) on the last segment of a possibly namespaced name.
/// Returns `None` for a name that does not look like a width.
pub fn width_spelling_hint(name: &str) -> Option<&'static str> {
    let last = name.rsplit('\\').next().unwrap_or(name).to_ascii_lowercase();
    Some(match last.as_str() {
        "i8" | "int8" | "int8_t" => "i8",
        "u8" | "uint8" | "uint8_t" | "byte" => "u8",
        "i16" | "int16" | "int16_t" => "i16",
        "u16" | "uint16" | "uint16_t" => "u16",
        "i32" | "int32" | "int32_t" => "i32",
        "u32" | "uint32" | "uint32_t" => "u32",
        "i64" | "int64" | "int64_t" => "i64",
        "u64" | "uint64" | "uint64_t" | "size_t" => "u64",
        "f32" | "float32" => "f32",
        "f64" | "float64" | "double" => "f64",
        _ => return None,
    })
}

/// Resolves a packed field or buffer element type expression spelled as a C width name.
///
/// Only a bare, unqualified name qualifies: `buffer<i32>` and `public u8 $flags;` name a
/// width. `ptr<i32>` is a pointer target resolved separately by the checker, and `?i32` is
/// not a width type. The name resolver leaves a width name unqualified and marks every class it resolved as fully
/// qualified, so `buffer<\u8>`, `use Lib\u8;` and a class named `u8` visible in the current
/// namespace keep naming that class.
pub fn width_type_from_type_expr(type_expr: &TypeExpr) -> Option<PhpType> {
    match type_expr {
        TypeExpr::Named(name) if name.is_unqualified() => width_type_from_name(name.as_str()),
        _ => None,
    }
}

/// Returns the natural size and alignment, in bytes, of a packed field or buffer element type,
/// or `None` if the type has no fixed size.
///
/// `int`, `float`, `bool`, pointers and resources are 8-byte words, a width (`PhpType::Narrow`)
/// takes its own size, and an embedded packed class takes its padded size and the alignment of
/// its widest field.
pub fn packed_type_layout(
    ty: &PhpType,
    packed_classes: &HashMap<String, PackedClassInfo>,
) -> Option<(usize, usize)> {
    match ty {
        PhpType::Int
        | PhpType::Float
        | PhpType::Bool
        | PhpType::False
        | PhpType::Pointer(_)
        | PhpType::Resource(_) => Some((8, 8)),
        PhpType::Narrow(width) => Some((width.size(), width.size())),
        PhpType::Packed(name) => packed_classes
            .get(name)
            .map(|info| (info.total_size, info.align)),
        _ => None,
    }
}

/// Returns the ABI size for a PhpType, or None if the type has no fixed size.
pub fn packed_type_size(
    ty: &PhpType,
    packed_classes: &HashMap<String, PackedClassInfo>,
) -> Option<usize> {
    packed_type_layout(ty, packed_classes).map(|(size, _)| size)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::types::PackedFieldInfo;

    /// Verifies `?string` keeps its own nullable `char*` ABI shape, reads as the PHP
    /// `string|null` union, and has no extern class field layout, while plain `string` is unchanged.
    #[test]
    fn nullable_string_maps_to_its_own_abi_shape_and_the_nullable_union() {
        assert_eq!(ctype_to_abi_type(&CType::NullableStr), ExternAbiType::NullableCStr);
        assert_eq!(
            ctype_to_php_type(&CType::NullableStr),
            PhpType::Union(vec![PhpType::Str, PhpType::Void])
        );
        assert_eq!(ctype_field_layout(&CType::NullableStr), None);
        assert_eq!(ctype_to_abi_type(&CType::Str), ExternAbiType::CStr);
        assert_eq!(ctype_to_php_type(&CType::Str), PhpType::Str);
        assert_eq!(ctype_field_layout(&CType::Str), Some((8, 8)));
    }

    /// Verifies every width name resolves to its storage format and the 8-byte names to the
    /// plain PHP types, while an ordinary class name is not a width.
    #[test]
    fn width_names_resolve_to_storage_formats() {
        assert_eq!(width_type_from_name("u8"), Some(PhpType::Narrow(ScalarWidth::U8)));
        assert_eq!(width_type_from_name("f32"), Some(PhpType::Narrow(ScalarWidth::F32)));
        assert_eq!(width_type_from_name("i64"), Some(PhpType::Int));
        assert_eq!(width_type_from_name("u64"), Some(PhpType::Int));
        assert_eq!(width_type_from_name("f64"), Some(PhpType::Float));
        assert_eq!(width_type_from_name("Vec2"), None);
        assert_eq!(ScalarWidth::I16.size(), 2);
        assert_eq!(ScalarWidth::U32.abi(), ExternAbiType::Int { bits: 32, signed: false });
        assert_eq!(ScalarWidth::F32.value_type(), PhpType::Float);
        assert_eq!(ScalarWidth::I8.value_type(), PhpType::Int);
    }

    /// Verifies the parser's width-name table and the type layer's width resolution agree, so a
    /// width the name resolver keeps bare is always one the checker understands and vice versa.
    #[test]
    fn width_name_table_and_resolution_agree() {
        for name in crate::parser::ast::c_width_type_names().iter().copied() {
            assert!(crate::parser::ast::is_c_width_type_name(name), "{name}");
            assert!(width_type_from_name(name).is_some(), "{name}");
            assert_eq!(width_spelling_hint(name), Some(name), "{name}");
        }
        for name in ["I32", "int32_t", "App\\uint8_t", "double"] {
            assert!(!crate::parser::ast::is_c_width_type_name(name), "{name}");
            assert!(width_type_from_name(name).is_none(), "{name}");
            assert!(width_spelling_hint(name).is_some(), "{name}");
        }
        assert_eq!(width_spelling_hint("Vec2"), None);
    }

    /// Verifies the layout table: words stay 8 bytes, widths take their own size, and an
    /// embedded packed class contributes its padded size with its own alignment.
    #[test]
    fn packed_type_layout_uses_natural_sizes_and_alignments() {
        let mut classes = HashMap::new();
        classes.insert(
            "Rgb".to_string(),
            PackedClassInfo {
                fields: vec![PackedFieldInfo {
                    name: "r".to_string(),
                    php_type: PhpType::Int,
                    offset: 0,
                    width: Some(ScalarWidth::U8),
                }],
                total_size: 3,
                align: 1,
            },
        );
        assert_eq!(packed_type_layout(&PhpType::Int, &classes), Some((8, 8)));
        assert_eq!(packed_type_layout(&PhpType::Bool, &classes), Some((8, 8)));
        assert_eq!(
            packed_type_layout(&PhpType::Narrow(ScalarWidth::U16), &classes),
            Some((2, 2))
        );
        assert_eq!(
            packed_type_layout(&PhpType::Packed("Rgb".to_string()), &classes),
            Some((3, 1))
        );
        assert_eq!(packed_type_size(&PhpType::Packed("Rgb".to_string()), &classes), Some(3));
        assert_eq!(packed_type_layout(&PhpType::Str, &classes), None);
        assert_eq!(
            packed_type_layout(&PhpType::Buffer(Box::new(PhpType::Int)), &classes),
            None
        );
    }
}
