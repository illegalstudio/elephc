//! Purpose:
//! Verifies profile-aware array key metadata before runtime key conversion.
//!
//! Called from:
//! - Focused compiler unit tests.
//!
//! Key details:
//! - Invalid source kinds remain visible for guards; string offsets keep PHP grammar.

use super::*;
use crate::span::Span;

/// Preserves strings and unions without normalizing away rejected scalar key kinds.
#[test]
fn strict_array_key_metadata_preserves_original_kinds() {
    let php = SemanticProfile::PERMISSIVE;
    let exact = php.with(StrictFeature::ArrayKeys, true);
    let variable = Expr::new(ExprKind::Variable("key".into()), Span::dummy());
    for value in ["0", "1", "01", "-1", "+1", "9223372036854775807", "1\0", "a"] {
        let key = Expr::string_lit(value);
        assert_eq!(normalized_array_key_type(&key, PhpType::Str, exact), PhpType::Str);
        let expected = if is_php_integer_array_key(value) { PhpType::Int } else { PhpType::Str };
        assert_eq!(normalized_array_key_type(&key, PhpType::Str, php), expected);
        assert!(static_array_key_forces_hash_storage(&key, exact));
        assert!(nonempty_array_key_forces_hash_storage(&key, exact));
        assert!(empty_array_key_requires_hash_storage(&key, exact));
        assert!(!nonempty_array_key_stays_in_range(&key, exact));
    }
    for ty in [PhpType::Int, PhpType::Str, PhpType::Bool, PhpType::False,
        PhpType::Float, PhpType::Void, PhpType::Mixed,
        PhpType::Union(vec![PhpType::Int, PhpType::Str]),
        PhpType::Union(vec![PhpType::Int, PhpType::Float])] {
        assert_eq!(normalized_array_key_type(&variable, ty.clone(), exact), ty);
        assert_eq!(array_key_type_from_value_type(ty.clone(), exact), ty);
    }
    assert_eq!(normalized_array_key_type(&variable, PhpType::Str, php), PhpType::Mixed);
    assert_eq!(array_key_type_from_value_type(PhpType::Str, php), PhpType::Mixed);
    for key in [Expr::int_lit(0), Expr::int_lit(1)] {
        assert_eq!(normalized_array_key_type(&key, PhpType::Int, exact), PhpType::Int);
    }
    assert!(!empty_array_key_requires_hash_storage(&Expr::int_lit(0), exact));
    assert!(!empty_array_key_requires_hash_storage(&Expr::string_lit("0"), php));
    assert_eq!(parse_php_string_offset_literal(" +01 "), Some(1));
    assert_eq!(parse_php_string_offset_literal("1\0"), None);
}

/// Nested arrays and return scans use the caller's immutable key policy at every depth.
#[test]
fn strict_array_key_syntactic_recursion_retains_the_profile() {
    use crate::parser::ast::{Stmt, StmtKind};
    use crate::types::checker::{infer_expr_type_syntactic, infer_return_type_syntactic};
    let nested = Expr::new(ExprKind::ArrayLiteralAssoc(vec![(Expr::string_lit("1"),
        Expr::new(ExprKind::ArrayLiteralAssoc(vec![(Expr::string_lit("0"), Expr::int_lit(7))]), Span::dummy()))]), Span::dummy());
    for strict in [false, true] {
        let profile = SemanticProfile::PERMISSIVE.with(StrictFeature::ArrayKeys, strict);
        let key = if strict { PhpType::Str } else { PhpType::Int };
        let expected = PhpType::AssocArray { key: Box::new(key.clone()),
            value: Box::new(PhpType::AssocArray { key: Box::new(key), value: Box::new(PhpType::Int) }) };
        assert_eq!(infer_expr_type_syntactic(&nested, profile), expected);
        let body = vec![Stmt::new(StmtKind::Return(Some(nested.clone())), Span::dummy())];
        assert_eq!(infer_return_type_syntactic(&body, profile), expected);
    }
}
