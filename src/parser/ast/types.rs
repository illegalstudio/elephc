//! Purpose:
//! Defines parsed type expressions before semantic type checking.
//! Represents named, nullable, union, intersection, callable, iterable, and buffer type syntax.
//!
//! Called from:
//! - `crate::parser::stmt::params`, OOP parsers, and downstream type-resolution passes.
//!
//! Key details:
//! - Names remain syntactic until the name resolver canonicalizes namespace and import context.

use crate::names::Name;

#[derive(Debug, Clone, PartialEq, Eq)]
/// Type expression in PHP syntax.
pub enum TypeExpr {
    Int,
    Float,
    Bool,
    Str,
    Void,
    Never,
    Iterable,
    Ptr(Option<Name>),
    Buffer(Box<TypeExpr>),
    Named(Name),
    Nullable(Box<TypeExpr>),
    Union(Vec<TypeExpr>),
}
