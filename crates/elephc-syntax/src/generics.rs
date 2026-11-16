//! Purpose:
//! Renders type expressions for parser diagnostics without pulling in the generics engine.
//!
//! Called from:
//! - `crate::names` and parser Pratt type rendering.

use crate::parser::ast::TypeExpr;

/// Renders a `TypeExpr` in canonical diagnostic spelling.
pub fn describe_type(ty: &TypeExpr) -> String {
    describe_with_roots(ty, false)
}

/// Renders parser-owned call arguments without losing explicit roots before name resolution.
pub fn describe_written_type(ty: &TypeExpr) -> String {
    describe_with_roots(ty, true)
}

/// Renders nested type syntax in either canonical identity or unresolved source mode.
fn describe_with_roots(ty: &TypeExpr, preserve_roots: bool) -> String {
    match ty {
        TypeExpr::Int => "int".to_string(),
        TypeExpr::Float => "float".to_string(),
        TypeExpr::Str => "string".to_string(),
        TypeExpr::Bool => "bool".to_string(),
        TypeExpr::False => "false".to_string(),
        TypeExpr::Void => "null".to_string(),
        TypeExpr::Never => "never".to_string(),
        TypeExpr::Iterable => "iterable".to_string(),
        TypeExpr::CallableSig { params, ret } => format!(
            "callable({}): {}",
            params.iter().map(|ty| describe_with_roots(ty, preserve_roots)).collect::<Vec<_>>().join(", "),
            describe_with_roots(ret, preserve_roots)
        ),
        TypeExpr::Named(name) => {
            let root = if (preserve_roots && name.is_fully_qualified())
                || (!name.is_unqualified() && crate::parser::ast::is_c_width_type_name(name.as_str()))
            {
                "\\"
            } else {
                ""
            };
            format!("{}{}", root, name.as_str())
        }
        TypeExpr::Array(inner) => format!("array<{}>", describe_with_roots(inner, preserve_roots)),
        TypeExpr::AssocArray { key, value } => format!(
            "array<{}, {}>",
            describe_with_roots(key, preserve_roots),
            describe_with_roots(value, preserve_roots)
        ),
        TypeExpr::Buffer(inner) => format!("buffer<{}>", describe_with_roots(inner, preserve_roots)),
        TypeExpr::GenericClass { name, args } => {
            let root = if preserve_roots && name.is_fully_qualified() { "\\" } else { "" };
            format!(
                "{}{}<{}>",
                root,
                name.as_str(),
                args.iter().map(|ty| describe_with_roots(ty, preserve_roots)).collect::<Vec<_>>().join(", ")
            )
        }
        TypeExpr::Nullable(inner) => format!("?{}", describe_with_roots(inner, preserve_roots)),
        TypeExpr::Ptr(Some(name)) => {
            let root = if (preserve_roots && name.is_fully_qualified())
                || (!name.is_unqualified() && crate::parser::ast::is_c_width_type_name(name.as_str()))
            {
                "\\"
            } else {
                ""
            };
            format!("ptr<{}{}>", root, name.as_str())
        }
        TypeExpr::Ptr(None) => "ptr".to_string(),
        TypeExpr::Union(members) => members
            .iter()
            .map(|ty| describe_with_roots(ty, preserve_roots))
            .collect::<Vec<_>>()
            .join("|"),
        TypeExpr::Intersection(members) => members
            .iter()
            .map(|ty| describe_with_roots(ty, preserve_roots))
            .collect::<Vec<_>>()
            .join("&"),
    }
}
