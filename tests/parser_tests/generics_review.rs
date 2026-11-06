//! Purpose:
//! Preserves the null-only property annotation introduced by the review fixes.
//!
//! Called from:
//! - The parser integration test harness.
//!
//! Key details:
//! - Null and void share the runtime sentinel, but their declaration rules differ.

use super::{parse, tokenize, StmtKind};
use elephc::parser::ast::TypeExpr;

/// The parser retains null's spelling for both ordinary and promoted properties.
#[test]
fn test_generics_review_null_property_retains_its_declaration() {
    for source in [
        "<?php class C { public null $v; }",
        "<?php class C { public function __construct(public null $v) {} }",
    ] {
        let tokens = tokenize(source).expect("lex null property");
        let program = parse(&tokens).expect("parse null property");
        let StmtKind::ClassDecl { properties, .. } = &program[0].kind else {
            panic!("expected class");
        };
        assert!(matches!(properties[0].type_expr.as_ref(), Some(TypeExpr::Named(name)) if name.as_str() == "null"));
    }
}
