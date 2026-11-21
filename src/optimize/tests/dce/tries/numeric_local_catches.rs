//! Purpose:
//! Preserves local membership failures when operand expressions cannot throw.
//!
//! Called from:
//! - Focused AST dead-code elimination regressions.
//!
//! Key details:
//! - Locals owns a TypeError edge independently of arithmetic overflow.

use super::*;

/// Pure writes and updates retain their local TypeError handlers before EIR guards exist.
#[test]
fn strict_numeric_local_errors_survive_ast_catch_pruning() {
    let profile = elephc_semantics::SemanticProfile::PERMISSIVE
        .with(elephc_semantics::StrictFeature::Locals, true);
    let _profile = crate::optimize::scoped_semantic_profile(profile);
    for statement in ["$value = $incoming;", "echo ($value = $incoming);", "++$value;", "$value++;", "$value += 1;"] {
        let source = format!("<?php $value = 1; try {{ {statement} }} catch (TypeError $error) {{ echo 'caught'; }}");
        let program = crate::parser::parse(&crate::lexer::tokenize(&source).unwrap()).unwrap();
        let optimized = eliminate_dead_code(program);
        assert!(optimized.iter().any(|statement| matches!(&statement.kind, StmtKind::Try { catches, .. }
            if catches.iter().any(|catch| catch.exception_types.iter().any(|name| name == "TypeError")))),
            "{statement}: numeric local catch disappeared");
    }
}
