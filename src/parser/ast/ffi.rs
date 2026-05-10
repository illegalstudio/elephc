use crate::span::Span;

use super::TypeExpr;

// --- FFI ---

/// C type annotation for extern declarations
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

/// Parameter in an extern function declaration
#[derive(Debug, Clone, PartialEq)]
pub struct ExternParam {
    pub name: String,
    pub c_type: CType,
}

/// Field in an extern class (C struct) declaration
#[derive(Debug, Clone, PartialEq)]
pub struct ExternField {
    pub name: String,
    pub c_type: CType,
}

#[derive(Debug, Clone)]
pub struct PackedField {
    pub name: String,
    pub type_expr: TypeExpr,
    pub span: Span,
}

impl PartialEq for PackedField {
    fn eq(&self, other: &Self) -> bool {
        self.name == other.name && self.type_expr == other.type_expr
    }
}
