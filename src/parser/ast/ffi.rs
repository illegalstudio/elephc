//! Purpose:
//! Defines AST records for elephantsc extern declarations and packed data layouts.
//! Represents C-facing scalar, pointer, buffer, function, global, and struct field metadata.
//!
//! Called from:
//! - `crate::parser::stmt::ffi` and downstream type/codegen FFI handling.
//!
//! Key details:
//! - These nodes describe compiler extensions, not PHP syntax, and must stay explicit in the AST.

use crate::span::Span;

use super::TypeExpr;

// --- FFI ---

/// Explicit C integer width in an extern declaration.
///
/// Plain `int` (and its alias `i64`) is the 64-bit default and stays `CType::Int`; these
/// variants let a declaration name the real width of a narrower or unsigned C integer so
/// the call boundary can normalize registers the C ABI leaves partially unspecified.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CIntWidth {
    I8,
    U8,
    I16,
    U16,
    I32,
    U32,
    U64,
}

impl CIntWidth {
    /// Returns the bit width of the C integer.
    pub fn bits(self) -> u8 {
        match self {
            Self::I8 | Self::U8 => 8,
            Self::I16 | Self::U16 => 16,
            Self::I32 | Self::U32 => 32,
            Self::U64 => 64,
        }
    }

    /// Returns whether the C integer is signed.
    pub fn signed(self) -> bool {
        matches!(self, Self::I8 | Self::I16 | Self::I32)
    }
}

/// The C width type names (`i8`..`u64`, `f32`, `f64`) that extern declarations, packed class
/// fields and `buffer<T>` elements spell their storage with, in the order the docs list them.
const C_WIDTH_TYPE_NAMES: [&str; 10] =
    ["i8", "u8", "i16", "u16", "i32", "u32", "i64", "u64", "f32", "f64"];

/// Returns the C width type names, for the tests that keep the parser's table and the type
/// layer's width resolution in agreement.
pub fn c_width_type_names() -> &'static [&'static str] {
    &C_WIDTH_TYPE_NAMES
}

/// Returns whether `name` is one of the C width type names (`c_width_type_names()`).
///
/// The name resolver keeps such a name bare in a type position unless a class or `use` alias of
/// that name is visible, so `packed class V { public i32 $x; }` and `buffer<u8>` inside a
/// namespace still name the width, while a class a program really calls `u8` resolves as in PHP.
pub fn is_c_width_type_name(name: &str) -> bool {
    C_WIDTH_TYPE_NAMES.contains(&name)
}

/// C type annotation for extern declarations.
#[derive(Debug, Clone, PartialEq)]
pub enum CType {
    Int,                    // int64_t / long, also spelled `i64`
    IntWidth(CIntWidth),    // explicit narrower or unsigned C integer (i8..u32, u64)
    Float,                  // double, also spelled `f64`
    F32,                    // C float; widened to a PHP double at the call boundary
    Str,        // char* (null-terminated)
    NullableStr,            // `?string`: a char* that may be NULL, PHP `?string` on the PHP side
    Bool,
    Void,
    Ptr,                    // opaque void*
    TypedPtr(String),       // ptr<ClassName>
    Callable,               // function pointer
}

/// A parameter in an extern function declaration, with a name and C type.
#[derive(Debug, Clone, PartialEq)]
pub struct ExternParam {
    pub name: String,
    pub c_type: CType,
}

/// A field in an extern class (C struct) declaration, with a name and C type.
#[derive(Debug, Clone, PartialEq)]
pub struct ExternField {
    pub name: String,
    pub c_type: CType,
}

/// A field within a `packed class` layout, with a name, type expression, and source span.
#[derive(Debug, Clone)]
pub struct PackedField {
    pub name: String,
    pub type_expr: TypeExpr,
    pub span: Span,
}

impl PartialEq for PackedField {
    /// Compares two `PackedField`s by name and type expression, ignoring span.
    fn eq(&self, other: &Self) -> bool {
        self.name == other.name && self.type_expr == other.type_expr
    }
}
