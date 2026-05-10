use crate::names::Name;
use crate::span::Span;

use super::{Expr, Stmt, TypeExpr};

#[derive(Debug, Clone)]
pub struct EnumCaseDecl {
    pub name: String,
    pub value: Option<Expr>,
    pub span: Span,
}

impl PartialEq for EnumCaseDecl {
    fn eq(&self, other: &Self) -> bool {
        self.name == other.name && self.value == other.value
    }
}

// --- OOP ---

#[derive(Debug, Clone, PartialEq)]
pub enum Visibility {
    Public,
    Protected,
    Private,
}

#[derive(Debug, Clone)]
pub struct TraitUse {
    pub trait_names: Vec<Name>,
    pub adaptations: Vec<TraitAdaptation>,
    // Used for trait-flattening diagnostics.
    pub span: Span,
}

impl PartialEq for TraitUse {
    fn eq(&self, other: &Self) -> bool {
        self.trait_names == other.trait_names && self.adaptations == other.adaptations
    }
}

#[derive(Debug, Clone, PartialEq)]
pub enum TraitAdaptation {
    Alias {
        trait_name: Option<Name>,
        method: String,
        alias: Option<String>,
        visibility: Option<Visibility>,
    },
    InsteadOf {
        trait_name: Option<Name>,
        method: String,
        instead_of: Vec<Name>,
    },
}

#[derive(Debug, Clone)]
pub struct ClassProperty {
    pub name: String,
    pub visibility: Visibility,
    pub type_expr: Option<TypeExpr>,
    pub readonly: bool,
    pub is_final: bool,
    pub is_static: bool,
    pub by_ref: bool,
    pub default: Option<Expr>,
    #[allow(dead_code)] // Used for error reporting in future phases
    pub span: Span,
}

impl PartialEq for ClassProperty {
    fn eq(&self, other: &Self) -> bool {
        self.name == other.name && self.visibility == other.visibility
            && self.type_expr == other.type_expr
            && self.readonly == other.readonly
            && self.is_final == other.is_final
            && self.is_static == other.is_static
            && self.by_ref == other.by_ref
    }
}

#[derive(Debug, Clone)]
pub struct ClassMethod {
    pub name: String,
    pub visibility: Visibility,
    pub is_static: bool,
    pub is_abstract: bool,
    pub is_final: bool,
    pub has_body: bool,
    pub params: Vec<(String, Option<TypeExpr>, Option<Expr>, bool)>,
    pub variadic: Option<String>,
    #[allow(dead_code)] // Will be used for return type checking in future phases
    pub return_type: Option<TypeExpr>,
    pub body: Vec<Stmt>,
    #[allow(dead_code)] // Used for error reporting in future phases
    pub span: Span,
}

impl PartialEq for ClassMethod {
    fn eq(&self, other: &Self) -> bool {
        self.name == other.name && self.visibility == other.visibility
            && self.is_static == other.is_static
            && self.is_abstract == other.is_abstract
            && self.is_final == other.is_final
            && self.has_body == other.has_body
    }
}
