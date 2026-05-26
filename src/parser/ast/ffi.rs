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

/// C type annotation for extern declarations.
#[derive(Debug, Clone, PartialEq)]
pub enum CType {
    Int,
    Float,
    Str,        // char* (null-terminated)
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
