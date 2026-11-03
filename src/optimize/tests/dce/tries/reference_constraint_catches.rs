//! Purpose:
//! Keeps catch routing around reference increments and closure writes conservative.
//!
//! Called from:
//! - Optimizer regression tests through the try/catch test module.
//!
//! Key details:
//! - Tests each optimization phase before lowering can hide a discarded handler.
//! - By-value controls use the same local spellings to expose leaked callee scope state.

use super::*;

/// Runs all control-flow phases and checks the catch in every source function.
fn assert_reference_catches(source: &str) {
    with_fresh_reference_volatile(|| {
        let tokens = crate::lexer::tokenize(source).unwrap();
        let mut program = crate::parser::parse(&tokens).unwrap();
        let optimizer = PostTypecheckOptimizer::new(&program);
        for phase in ["prune", "normalize", "dce"] {
            program = match phase {
                "prune" => optimizer.prune(program, HashSet::new()),
                "normalize" => optimizer.normalize(program, HashSet::new()),
                _ => optimizer.eliminate_dead_code(program, HashSet::new()),
            };
            let mut checked = 0;
            for stmt in &program {
                let StmtKind::FunctionDecl { name, body, .. } = &stmt.kind else { continue; };
                let caught = body.iter().any(|stmt|
                    matches!(&stmt.kind, StmtKind::Try { catches, .. } if !catches.is_empty()));
                assert_eq!(caught, !name.ends_with("_plain"), "{phase}: {name}");
                checked += 1;
            }
            assert!(checked > 0, "{phase}: retain the fixture declarations");
        }
    });
}

/// Both increment directions and both return-value modes can reject a constrained reference.
#[test]
fn reference_increment_validation_preserves_catches_through_all_phases() {
    for update in ["++$target", "$target++", "--$target", "$target--"] {
        assert_reference_catches(&format!(r#"<?php
function updateReference(mixed &$target): void {{
    try {{ {update}; }} catch (TypeError $error) {{ echo 'reference'; }}
}}
function updateReference_plain(int $target): void {{
    try {{ {update}; }} catch (TypeError $error) {{ echo 'plain'; }}
}}
"#));
    }
}

/// Static-property increments keep their write validation and global mutation effects.
#[test]
fn static_increment_validation_preserves_catches_through_all_phases() {
    for update in ["++IncrementBoundary::$value", "IncrementBoundary::$value++",
        "--IncrementBoundary::$value", "IncrementBoundary::$value--"]
    {
        assert_reference_catches(&format!(r#"<?php
class IncrementBoundary {{ public static int $value = 1; }}
function updateStaticReference(): void {{
    try {{ {update}; }} catch (TypeError $error) {{ echo 'static'; }}
}}
"#));
    }
}

/// Direct closure summaries install reference parameters and captures before visiting the body.
#[test]
fn closure_reference_validation_preserves_callers_through_all_phases() {
    assert_reference_catches(r#"<?php
function callReferenceClosure(mixed &$target): void {
    try { (function (mixed &$inner): void { $inner = true; })($target); }
    catch (TypeError $error) { echo 'parameter'; }
}
function callReferenceCapture(mixed &$target): void {
    try { (function () use (&$target): void { $target = true; })(); }
    catch (TypeError $error) { echo 'capture'; }
}
function callReferenceClosure_plain(mixed $target): void {
    try { (function (mixed $target): void { $target = true; })($target); }
    catch (TypeError $error) { echo 'plain'; }
}
function callReferenceCapture_plain(mixed $target): void {
    try { (function () use ($target): void { $target = true; })(); }
    catch (TypeError $error) { echo 'plain'; }
}
"#);
}
