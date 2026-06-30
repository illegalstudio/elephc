//! Purpose:
//! Conversion helpers that bridge `BuiltinSpec` fields (`TypeSpec`, `DefaultSpec`)
//! into the compiler's rich runtime types (`PhpType`, `Expr`) during the migration period.
//!
//! Called from:
//! - `crate::builtins::registry` when populating the legacy dispatch tables.
//!
//! Key details:
//! - `type_spec_to_php` must produce byte-for-byte equivalent `PhpType` values to the
//!   legacy hand-coded type annotations in `src/types/signatures.rs` and the builtin
//!   type-checker files.
//! - `default_spec_to_expr` must produce `Expr` nodes with the same `ExprKind` and
//!   `Span::dummy()` span as the legacy literal helpers (`null_lit`, `int_lit`, etc.)
//!   in `src/types/signatures.rs`.
//! - Only variants that exist in `TypeSpec`/`DefaultSpec` are handled; no speculative
//!   mappings are added (YAGNI).
//! - This module is intentionally private (`mod convert;` without `pub`) and will
//!   shrink as each legacy dispatch point is replaced by direct registry queries.

// Dead-code warnings are expected during the multi-task migration before the
// registry wires these helpers into active dispatch paths.
#![allow(dead_code)]

use crate::builtins::spec::{DefaultSpec, TypeSpec};
use crate::parser::ast::{Expr, ExprKind};
use crate::span::Span;
use crate::types::PhpType;

/// Converts a `TypeSpec` descriptor into the corresponding `PhpType`.
///
/// The mapping is one-to-one for scalar variants. Compound variants (`ArrayOf`,
/// `AssocOf`, `Union`) recurse so nested types are correctly translated.
/// `Null` maps to `PhpType::Void` because `Void` is the null sentinel used by the
/// runtime (stored as 8 bytes). `Void` maps to `PhpType::Void` for functions that
/// do not return a value.
pub fn type_spec_to_php(ty: &TypeSpec) -> PhpType {
    match ty {
        TypeSpec::Int => PhpType::Int,
        TypeSpec::Float => PhpType::Float,
        TypeSpec::Str => PhpType::Str,
        TypeSpec::Bool => PhpType::Bool,
        TypeSpec::Mixed => PhpType::Mixed,
        TypeSpec::Null => PhpType::Void,
        TypeSpec::Void => PhpType::Void,
        TypeSpec::ArrayOf(elem) => PhpType::Array(Box::new(type_spec_to_php(elem))),
        TypeSpec::AssocOf(val) => PhpType::AssocArray {
            key: Box::new(PhpType::Str),
            value: Box::new(type_spec_to_php(val)),
        },
        TypeSpec::Union(members) => {
            PhpType::Union(members.iter().map(type_spec_to_php).collect())
        }
    }
}

/// Converts a `DefaultSpec` descriptor into the `Expr` node the legacy
/// `src/types/signatures.rs` literal helpers would produce.
///
/// Every variant uses `Span::dummy()` to match the convention used by `null_lit()`,
/// `int_lit()`, `bool_lit()`, `string_lit()`, and the inline array/float literals
/// in the legacy signature table. The result is structurally identical to what
/// those helpers return.
pub fn default_spec_to_expr(d: &DefaultSpec) -> Expr {
    match d {
        DefaultSpec::Null => Expr::new(ExprKind::Null, Span::dummy()),
        DefaultSpec::Int(n) => Expr::new(ExprKind::IntLiteral(*n), Span::dummy()),
        DefaultSpec::Bool(b) => Expr::new(ExprKind::BoolLiteral(*b), Span::dummy()),
        DefaultSpec::Float(f) => Expr::new(ExprKind::FloatLiteral(*f), Span::dummy()),
        DefaultSpec::Str(s) => Expr::new(ExprKind::StringLiteral(s.to_string()), Span::dummy()),
        DefaultSpec::IntMax => Expr::new(ExprKind::IntLiteral(i64::MAX), Span::dummy()),
        DefaultSpec::IntMin => Expr::new(ExprKind::IntLiteral(i64::MIN), Span::dummy()),
        DefaultSpec::EmptyArray => Expr::new(ExprKind::ArrayLiteral(Vec::new()), Span::dummy()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::builtins::spec::{DefaultSpec, TypeSpec};
    use crate::types::PhpType;

    /// Verifies scalar TypeSpec maps to the matching PhpType.
    #[test]
    fn scalar_type_spec_converts() {
        assert_eq!(type_spec_to_php(&TypeSpec::Int), PhpType::Int);
        assert_eq!(type_spec_to_php(&TypeSpec::Str), PhpType::Str);
    }

    /// Verifies a null default lowers to the same Expr the legacy `null_lit()` helper produces.
    #[test]
    fn null_default_converts() {
        let e = default_spec_to_expr(&DefaultSpec::Null);
        assert!(matches!(e.kind, crate::parser::ast::ExprKind::Null));
    }

    /// Verifies remaining scalar TypeSpec variants map to their PhpType equivalents.
    #[test]
    fn all_scalar_type_specs_convert() {
        assert_eq!(type_spec_to_php(&TypeSpec::Float), PhpType::Float);
        assert_eq!(type_spec_to_php(&TypeSpec::Bool), PhpType::Bool);
        assert_eq!(type_spec_to_php(&TypeSpec::Mixed), PhpType::Mixed);
        assert_eq!(type_spec_to_php(&TypeSpec::Void), PhpType::Void);
        assert_eq!(type_spec_to_php(&TypeSpec::Null), PhpType::Void);
    }

    /// Verifies ArrayOf TypeSpec recurses correctly into PhpType::Array.
    #[test]
    fn array_of_type_spec_converts() {
        assert_eq!(
            type_spec_to_php(&TypeSpec::ArrayOf(&TypeSpec::Int)),
            PhpType::Array(Box::new(PhpType::Int))
        );
    }

    /// Verifies AssocOf TypeSpec maps to PhpType::AssocArray with Str key and converted value.
    #[test]
    fn assoc_of_type_spec_converts() {
        assert_eq!(
            type_spec_to_php(&TypeSpec::AssocOf(&TypeSpec::Str)),
            PhpType::AssocArray {
                key: Box::new(PhpType::Str),
                value: Box::new(PhpType::Str),
            }
        );
    }

    /// Verifies Union TypeSpec maps to PhpType::Union with all members converted.
    #[test]
    fn union_type_spec_converts() {
        assert_eq!(
            type_spec_to_php(&TypeSpec::Union(&[TypeSpec::Int, TypeSpec::Bool])),
            PhpType::Union(vec![PhpType::Int, PhpType::Bool])
        );
    }

    /// Verifies integer DefaultSpec produces an IntLiteral expression matching int_lit().
    #[test]
    fn int_default_converts() {
        let e = default_spec_to_expr(&DefaultSpec::Int(42));
        assert!(matches!(e.kind, ExprKind::IntLiteral(42)));
    }

    /// Verifies boolean DefaultSpec produces a BoolLiteral expression matching bool_lit().
    #[test]
    fn bool_default_converts() {
        let e = default_spec_to_expr(&DefaultSpec::Bool(true));
        assert!(matches!(e.kind, ExprKind::BoolLiteral(true)));
    }

    /// Verifies float DefaultSpec produces a FloatLiteral expression.
    #[test]
    fn float_default_converts() {
        let e = default_spec_to_expr(&DefaultSpec::Float(1.5));
        assert!(matches!(e.kind, ExprKind::FloatLiteral(_)));
    }

    /// Verifies string DefaultSpec produces a StringLiteral expression matching string_lit().
    #[test]
    fn str_default_converts() {
        let e = default_spec_to_expr(&DefaultSpec::Str("hello"));
        assert!(matches!(e.kind, ExprKind::StringLiteral(ref s) if s == "hello"));
    }

    /// Verifies IntMax DefaultSpec produces IntLiteral(i64::MAX), matching the PHP_INT_MAX literal.
    #[test]
    fn int_max_default_converts() {
        let e = default_spec_to_expr(&DefaultSpec::IntMax);
        assert!(matches!(e.kind, ExprKind::IntLiteral(i64::MAX)));
    }

    /// Verifies IntMin DefaultSpec produces IntLiteral(i64::MIN), matching the PHP_INT_MIN literal.
    #[test]
    fn int_min_default_converts() {
        let e = default_spec_to_expr(&DefaultSpec::IntMin);
        assert!(matches!(e.kind, ExprKind::IntLiteral(i64::MIN)));
    }

    /// Verifies EmptyArray DefaultSpec produces an empty ArrayLiteral expression.
    #[test]
    fn empty_array_default_converts() {
        let e = default_spec_to_expr(&DefaultSpec::EmptyArray);
        assert!(matches!(e.kind, ExprKind::ArrayLiteral(ref v) if v.is_empty()));
    }
}
