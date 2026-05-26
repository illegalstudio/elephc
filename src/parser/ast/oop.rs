//! Purpose:
//! Defines AST records for PHP class-like declarations and members.
//! Covers classes, interfaces, traits, enums, properties, methods, visibility, and trait adaptations.
//!
//! Called from:
//! - `crate::parser::stmt::oop` and class-aware resolver, name-resolver, type, and codegen passes.
//!
//! Key details:
//! - Member metadata carries spans and modifiers needed for PHP-compatible diagnostics and lowering.

use crate::names::Name;
use crate::span::Span;

use super::{Expr, Stmt, TypeExpr};

// --- Attributes (PHP 8.0 #[Name(args)]) ---

/// One attribute inside a `#[...]` group: a qualified name followed by
/// optional arguments. Multiple attributes can sit in the same group:
/// `#[A, B(1)]`, and groups stack: `#[A] #[B]`.
#[derive(Debug, Clone)]
pub struct Attribute {
    pub name: Name,
    pub args: Vec<Expr>,
    #[allow(dead_code)] // Used for error reporting in future passes
    pub span: Span,
}

impl PartialEq for Attribute {
    /// Compares two attributes by name and arguments, ignoring span.
    fn eq(&self, other: &Self) -> bool {
        self.name == other.name && self.args == other.args
    }
}

/// One `#[...]` group; PHP allows several comma-separated attributes per
/// group as well as several stacked groups before the same declaration.
/// Both shapes flatten naturally into `Vec<AttributeGroup>` per declaration.
#[derive(Debug, Clone)]
pub struct AttributeGroup {
    pub attributes: Vec<Attribute>,
    #[allow(dead_code)] // Used for error reporting in future passes
    pub span: Span,
}

impl PartialEq for AttributeGroup {
    /// Compares two attribute groups by their attributes, ignoring span.
    fn eq(&self, other: &Self) -> bool {
        self.attributes == other.attributes
    }
}

#[derive(Debug, Clone)]
/// Enum case declaration.
pub struct EnumCaseDecl {
    pub name: String,
    pub value: Option<Expr>,
    pub span: Span,
    pub attributes: Vec<AttributeGroup>,
}

impl PartialEq for EnumCaseDecl {
    /// Compares two enum cases by name, value, and attributes; span is not compared.
    fn eq(&self, other: &Self) -> bool {
        self.name == other.name && self.value == other.value
            && self.attributes == other.attributes
    }
}

// --- OOP ---

#[derive(Debug, Clone, PartialEq)]
/// Visibility modifier.
pub enum Visibility {
    Public,
    Protected,
    Private,
}

#[derive(Debug, Clone, Default, PartialEq)]
/// Property hooks.
pub struct PropertyHooks {
    pub get: bool,
    pub set: bool,
    pub get_by_ref: bool,
}

impl PropertyHooks {
    /// Returns a `PropertyHooks` with all hooks disabled (no get/set/get_by_ref).
    pub fn none() -> Self {
        Self::default()
    }

    /// Returns true if any hook (get, set, or get_by_ref) is present.
    pub fn any(&self) -> bool {
        self.get || self.set || self.get_by_ref
    }

    /// Returns true if a getter is required (either value or by-ref getter present).
    pub fn requires_get(&self) -> bool {
        self.get || self.get_by_ref
    }
}

#[derive(Debug, Clone)]
/// Trait use.
pub struct TraitUse {
    pub trait_names: Vec<Name>,
    pub adaptations: Vec<TraitAdaptation>,
    // Used for trait-flattening diagnostics.
    pub span: Span,
}

impl PartialEq for TraitUse {
    /// Compares two trait uses by trait names and adaptations; span is not compared.
    fn eq(&self, other: &Self) -> bool {
        self.trait_names == other.trait_names && self.adaptations == other.adaptations
    }
}

#[derive(Debug, Clone, PartialEq)]
/// Trait adaptation.
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
/// Class property.
pub struct ClassProperty {
    pub name: String,
    pub visibility: Visibility,
    pub type_expr: Option<TypeExpr>,
    pub hooks: PropertyHooks,
    pub readonly: bool,
    pub is_final: bool,
    pub is_static: bool,
    pub is_abstract: bool,
    pub by_ref: bool,
    pub default: Option<Expr>,
    #[allow(dead_code)] // Used for error reporting in future phases
    pub span: Span,
    pub attributes: Vec<AttributeGroup>,
}

impl PartialEq for ClassProperty {
    /// Compares class properties by name, visibility, type, hooks, modifiers,
    /// by-ref flag, default value, and attributes; span is not compared.
    fn eq(&self, other: &Self) -> bool {
        self.name == other.name && self.visibility == other.visibility
            && self.type_expr == other.type_expr
            && self.hooks == other.hooks
            && self.readonly == other.readonly
            && self.is_final == other.is_final
            && self.is_static == other.is_static
            && self.is_abstract == other.is_abstract
            && self.by_ref == other.by_ref
            && self.default == other.default
            && self.attributes == other.attributes
    }
}

/// `const NAME = expr;` declaration inside a class/interface/trait body.
/// PHP supports per-constant visibility (PHP 7.1+) and the `final`
/// modifier (PHP 8.1+). Per-constant attributes are stored for future
/// `#[\Deprecated]` support.
#[derive(Debug, Clone)]
pub struct ClassConst {
    pub name: String,
    pub visibility: Visibility,
    pub is_final: bool,
    pub value: Expr,
    #[allow(dead_code)] // Used for error reporting in future passes
    pub span: Span,
    #[allow(dead_code)] // Reserved for #[\Deprecated] on class constants
    pub attributes: Vec<AttributeGroup>,
}

impl PartialEq for ClassConst {
    /// Compares class constants by name, visibility, is_final, value, and attributes;
    /// span is not compared.
    fn eq(&self, other: &Self) -> bool {
        self.name == other.name
            && self.visibility == other.visibility
            && self.is_final == other.is_final
            && self.value == other.value
            && self.attributes == other.attributes
    }
}

#[derive(Debug, Clone)]
/// Class method.
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
    pub attributes: Vec<AttributeGroup>,
}

impl PartialEq for ClassMethod {
    /// Compares class methods by name, visibility, static/abstract/final flags,
    /// has_body, and attributes; span, params, return_type, and body are not compared.
    fn eq(&self, other: &Self) -> bool {
        self.name == other.name && self.visibility == other.visibility
            && self.is_static == other.is_static
            && self.is_abstract == other.is_abstract
            && self.is_final == other.is_final
            && self.has_body == other.has_body
            && self.attributes == other.attributes
    }
}
