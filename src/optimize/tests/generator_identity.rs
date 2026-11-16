//! Purpose:
//! Regressions for generator identity while folding and pruning unreachable yields.
//!
//! Called from:
//! - The optimizer unit-test harness.
//!
//! Key details:
//! - Each pass must keep generator classification while removing unrelated dead code.

use super::*;

/// Parses a fixture without running optimization or type checking.
fn parse(source: &str) -> Program {
    crate::parser::parse(&crate::lexer::tokenize(source).unwrap()).unwrap()
}

/// Keeping the final yield must not undo unrelated rewrites in the callable body.
#[test]
fn generator_identity_retains_optimization_when_last_yield_is_removed() {
    let source = "<?php function probe() {
        echo 1 + 2;
        return;
        yield 1;
        echo 'unreachable_marker';
    }";
    for pass in [prune_constant_control_flow as fn(Program) -> Program, eliminate_dead_code] {
        let program = pass(parse(source));
        let StmtKind::FunctionDecl { body, .. } = &program[0].kind else { panic!("{program:?}") };
        assert!(crate::types::checker::yield_validation::body_contains_yield(body));
        assert!(!format!("{body:?}").contains("unreachable_marker"), "{body:?}");
    }
}

/// Constant match folding keeps classification without retaining the original dead arm.
#[test]
fn generator_identity_folds_dead_match_yield_without_restoring_body() {
    let program = fold_constants(parse("<?php function probe() {
        $value = match (1) { 0 => yield 1, default => 2 };
        return $value;
    }"));
    let StmtKind::FunctionDecl { body, .. } = &program[0].kind else { panic!("{program:?}") };
    assert!(crate::types::checker::yield_validation::body_contains_yield(body));
    assert!(matches!(&body[0].kind, StmtKind::Assign { value: Expr { kind: ExprKind::IntLiteral(2), .. }, .. }),
        "{body:?}");
}

/// Enum method bodies participate in pruning and DCE, retaining one dead yield marker.
#[test]
fn generator_identity_optimizes_enum_methods() {
    let source = "<?php enum Choice { case One; public function values() {
        return;
        yield 1;
        echo 'unreachable_marker';
    } }";
    for pass in [prune_constant_control_flow as fn(Program) -> Program, eliminate_dead_code] {
        let program = pass(parse(source));
        let StmtKind::EnumDecl { methods, .. } = &program[0].kind else { panic!("{program:?}") };
        let body = &methods[0].body;
        assert!(crate::types::checker::yield_validation::body_contains_yield(body));
        assert!(!format!("{body:?}").contains("unreachable_marker"), "{body:?}");
    }
}
