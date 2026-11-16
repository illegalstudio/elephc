//! Purpose:
//! Keeps standalone null declarations distinct from void and nullable union syntax.
//!
//! Called from:
//! - The parser integration test root.
//!
//! Key details:
//! - Null-only parameters are legal PHP; void parameters remain invalid downstream.

use super::*;

/// A standalone null parameter and return keep their source type after parsing.
#[test]
fn standalone_null_declarations_preserve_null_type() {
    let program = parse_source("<?php function identity(null $value): null { return $value; }");
    let StmtKind::FunctionDecl { params, return_type, .. } = &program[0].kind else { panic!("function expected"); };
    let expected = TypeExpr::Named(Name::unqualified("null"));
    assert_eq!(params[0].1, Some(expected.clone()));
    assert_eq!(*return_type, Some(expected));
}

/// Null inside a union retains the existing canonical nullable representation.
#[test]
fn null_unions_preserve_nullable_representation() {
    let program = parse_source("<?php function identity(null|int $value): int|null { return $value; }");
    let StmtKind::FunctionDecl { params, return_type, .. } = &program[0].kind else { panic!("function expected"); };
    let expected = TypeExpr::Nullable(Box::new(TypeExpr::Int));
    assert_eq!(params[0].1, Some(expected.clone()));
    assert_eq!(*return_type, Some(expected));
}
