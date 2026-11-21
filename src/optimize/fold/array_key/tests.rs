//! Purpose:
//! Verifies optimizer key identity and preservation of invalid original key domains.
//!
//! Called from:
//! - Focused constant-folding unit tests.
//!
//! Key details:
//! - Scoped profiles restore independently; packed reads accept integer keys in exact mode.

use super::*;
use crate::parser::ast::{Expr, ExprKind};
use elephc_semantics::SemanticProfile;

/// Builds a literal access through the real expression folder.
fn folded(source: &str) -> Expr {
    let program = crate::parser::parse(&crate::lexer::tokenize(&format!("<?php echo {source};")).unwrap()).unwrap();
    let crate::parser::ast::StmtKind::Echo(expr) = &program[0].kind else { panic!("echo fixture"); };
    crate::optimize::fold_constant_expression(expr.clone())
}

/// Numeric strings remain distinct from integers while actual duplicate keys stay last-wins.
#[test]
fn strict_array_key_folding_uses_selected_typed_identity() {
    for strict in [false, true] {
        let profile = SemanticProfile::PERMISSIVE.with(StrictFeature::ArrayKeys, strict);
        let _profile = crate::optimize::scoped_semantic_profile(profile);
        for (source, expected) in [
            ("[1 => 'int', '1' => 'str', '01' => 'pad'][1]", if strict { "int" } else { "str" }),
            ("[1 => 'int', '1' => 'str']['1']", "str"),
            ("['1' => 'old', '1' => 'new', 1 => 'int']['1']", if strict { "new" } else { "int" }),
            ("['01' => 'pad', 1 => 'int']['01']", "pad"),
            ("['a\\0' => 'text', \"a\\0\" => 'nul'][\"a\\0\"]", "nul"),
        ] {
            assert!(matches!(folded(source).kind, ExprKind::StringLiteral(value) if value == expected), "{source}, strict={strict}");
        }
        assert!(matches!(folded("[10, 20][1]").kind, ExprKind::IntLiteral(20)));
        if strict {
            assert!(matches!(folded("[10, 20]['1']").kind, ExprKind::ArrayAccess { .. }));
        } else {
            assert!(matches!(folded("[10, 20]['1']").kind, ExprKind::IntLiteral(20)));
        }
    }
    assert_eq!(array_key(&ScalarValue::String("1".into())), Some(ArrayKey::Int(1)));
}

/// Folding never erases a rejected boolean, float or null key, including an unrelated entry.
#[test]
fn strict_array_key_folding_keeps_original_domain_failures() {
    let profile = SemanticProfile::PERMISSIVE.with(StrictFeature::ArrayKeys, true);
    let _profile = crate::optimize::scoped_semantic_profile(profile);
    for source in ["[10, 20][true]", "[10, 20][1.0]", "[10, 20][null]",
        "[true => 'bad', 1 => 'good'][1]", "[1.0 => 'bad', 2 => 'good'][2]",
        "[null => 'bad', '' => 'good']['']"] {
        assert!(matches!(folded(source).kind, ExprKind::ArrayAccess { .. }), "{source}");
    }
    for value in [ScalarValue::Bool(false), ScalarValue::Float(0.0), ScalarValue::Null] {
        assert_eq!(array_key(&value), None);
    }
}
