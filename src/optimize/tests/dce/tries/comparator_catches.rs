//! Purpose:
//! Preserves catch routing for checked comparator consumers with nonthrowing callback bodies.
//!
//! Called from:
//! - Focused AST optimizer regression tests.
//!
//! Key details:
//! - The consumer can reject an actual result independently of callback-body summaries.

use super::*;

/// Pure callback bodies cannot erase the TypeError edge imposed by checked comparisons.
#[test]
fn strict_comparison_comparator_errors_survive_ast_catch_pruning() {
    let profile = elephc_semantics::SemanticProfile::PERMISSIVE
        .with(elephc_semantics::StrictFeature::Comparisons, true);
    let _profile = crate::optimize::scoped_semantic_profile(profile);
    for call in ["usort($items, pureComparator(...))", "uasort($items, pureComparator(...))",
        "uksort($items, pureComparator(...))", "array_udiff($items, $items, pureComparator(...))",
        "array_uintersect($items, $items, pureComparator(...))"]
    {
        let source = format!("<?php function pureComparator(int $left, int $right): bool {{ return true; }} \
            $items = [2, 1]; try {{ {call}; }} catch (TypeError $error) {{ echo 'caught'; }}");
        let program = crate::parser::parse(&crate::lexer::tokenize(&source).unwrap()).unwrap();
        let optimized = eliminate_dead_code(program);
        assert!(optimized.iter().any(|statement| matches!(&statement.kind, StmtKind::Try { catches, .. }
            if catches.iter().any(|catch| catch.exception_types.iter().any(|name| name == "TypeError")))),
            "{call}: checked comparator catch disappeared");
    }
}
