//! Purpose:
//! Regression coverage for catches reached through hidden foreach protocol callbacks.
//!
//! Called from:
//! - `crate::optimize::tests::dce::tries` through the Rust test harness.
//!
//! Key details:
//! - Iterator callbacks are not explicit AST calls, so exception-flow analysis
//!   must preserve compatible handlers conservatively.

use super::*;

/// A foreach protocol callback can throw even when its source and body look quiet in the AST.
#[test]
fn foreach_protocol_preserves_a_same_frame_catch() {
    let tokens = crate::lexer::tokenize(
        r#"<?php
try {
    foreach ($items as $value) { echo $value; }
} catch (RuntimeException $exception) {
    echo "caught";
}
"#,
    )
    .expect("fixture must tokenize");
    let program = eliminate_dead_code(crate::parser::parse(&tokens).expect("fixture must parse"));
    let StmtKind::Try { catches, .. } = &program[0].kind else {
        panic!("foreach handler must remain a try statement: {program:?}");
    };
    assert_eq!(catches.len(), 1, "foreach protocol catch was pruned: {program:?}");
}
