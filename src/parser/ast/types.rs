//! Purpose:
//! Defines parsed and synthetic type expressions before semantic type checking.
//! Represents named, nullable, union, callable, iterable, buffer, and internal array element syntax.
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
    Array(Box<TypeExpr>),
    Ptr(Option<Name>),
    Buffer(Box<TypeExpr>),
    Named(Name),
    Nullable(Box<TypeExpr>),
    Union(Vec<TypeExpr>),
    /// PHP 8.1 intersection type `A&B`: a value satisfying every member (all are class/interface
    /// types). Represented for the value as its first member; argument boundaries validate that
    /// every member is satisfied.
    Intersection(Vec<TypeExpr>),
}

impl TypeExpr {
    /// Rewrites the relative class types `self`/`static` to `self_class` and `parent` to
    /// `parent_class`, recursing through nullable, union, array, and buffer members.
    ///
    /// `self` and `static` both resolve to the enclosing class (declaring class for `static`);
    /// `parent` resolves to its parent, or is left untouched when `parent_class` is `None` so a
    /// later pass can report "no parent class". The match on the keyword is case-insensitive,
    /// and any non-relative named type is returned unchanged. Applied after inheritance/trait
    /// flattening, when the concrete enclosing class is finally known.
    pub fn substitute_relative_class_types(
        &self,
        self_class: &str,
        parent_class: Option<&str>,
    ) -> TypeExpr {
        match self {
            TypeExpr::Named(name) => match name.as_str().to_ascii_lowercase().as_str() {
                "self" | "static" => TypeExpr::Named(Name::unqualified(self_class)),
                "parent" => match parent_class {
                    Some(parent) => TypeExpr::Named(Name::unqualified(parent)),
                    None => self.clone(),
                },
                _ => self.clone(),
            },
            TypeExpr::Nullable(inner) => TypeExpr::Nullable(Box::new(
                inner.substitute_relative_class_types(self_class, parent_class),
            )),
            TypeExpr::Union(members) => TypeExpr::Union(
                members
                    .iter()
                    .map(|member| member.substitute_relative_class_types(self_class, parent_class))
                    .collect(),
            ),
            TypeExpr::Intersection(members) => TypeExpr::Intersection(
                members
                    .iter()
                    .map(|member| member.substitute_relative_class_types(self_class, parent_class))
                    .collect(),
            ),
            TypeExpr::Array(inner) => TypeExpr::Array(Box::new(
                inner.substitute_relative_class_types(self_class, parent_class),
            )),
            TypeExpr::Buffer(inner) => TypeExpr::Buffer(Box::new(
                inner.substitute_relative_class_types(self_class, parent_class),
            )),
            other => other.clone(),
        }
    }
}
