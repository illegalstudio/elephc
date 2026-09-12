//! Purpose:
//! Checks the authorization boundary for detaching promoted reference bindings.
//!
//! Called from:
//! - The diagnostic integration test root.
//!
//! Key details:
//! - Detachment is a storage decision, not permission to retype escaped references.
//! - Conditional, shared, declared and eval-visible bindings retain their existing handling.

use super::*;

/// Compiler-generated spans cannot authorize detachment at unrelated synthetic unset sites.
#[test]
fn test_reference_detach_at_a_dummy_span_is_not_recorded() {
    use elephc::parser::ast::{ExprKind, StmtKind};
    let source = "<?php $text = 'x'; $saved = function() use (&$text) { return $text; }; unset($text);";
    let mut program = parse(&tokenize(source).unwrap()).unwrap();
    let StmtKind::ExprStmt(expr) = &mut program.last_mut().unwrap().kind else { panic!("unset statement"); };
    let ExprKind::FunctionCall { args, .. } = &mut expr.kind else { panic!("unset call"); };
    args[0].span = elephc::span::Span::dummy();
    let result = types::check(&program).expect("the synthetic unset must remain valid");
    assert!(result.local_ref_detach_sites.is_empty());
}

/// A trailing unconditional unset may detach a captured local first assigned inside a loop.
#[test]
fn test_reference_detach_is_recorded_without_relaxing_binding_kills() {
    let result = check_source_full(r#"<?php
for ($i = 0; $i < 4; $i++) {
    $text = str_repeat("x", $i + 1);
    $saved = function() use (&$text): string { return $text; };
    echo $saved();
}
unset($text);
"#).expect("the captured-loop fixture must type-check");
    assert_eq!(result.local_ref_detach_sites.len(), 1);
    let (span, names) = result.local_ref_detach_sites.iter().next().unwrap();
    assert!(span.identifies_a_node());
    assert_eq!(names, &HashSet::from(["text".to_string()]));
    assert!(result.local_binding_decision_spans().contains(span));
    assert!(!result.local_bind_kill_sites.values().any(|names| names.contains("text")));
    expect_error(
        "<?php $text = 'x'; $saved = function() use (&$text): string { return $text; }; unset($text); $text = 1;",
        "cannot reassign",
    );
}

/// Unsets in conditional flow or storage reachable outside an ordinary local cannot detach it.
#[test]
fn test_reference_detach_rejects_conditional_and_shared_storage() {
    for source in [
        "<?php $text = 'x'; $saved = function() use (&$text) { return $text; }; if ($argc > 1) { unset($text); } echo $saved();",
        "<?php $text = 'x'; $saved = function() use (&$text) { return $text; }; for ($i = 0; $i < 2; $i++) { unset($text); } echo $saved();",
        "<?php $text = 'x'; $saved = function() use (&$text) { return $text; }; $result = $argc > 1 ? unset($text) : null; echo $saved();",
        "<?php $text = 'x'; $saved = function() use (&$text) { return $text; }; eval('return null;'); unset($text); echo $saved();",
        "<?php function shared() { global $text; } $text = 'x'; $saved = function() use (&$text) { return $text; }; unset($text); echo $saved();",
        "<?php $text = 'x'; function shared() { global $text; $saved = function() use (&$text) { return $text; }; unset($text); echo $saved(); } shared();",
        "<?php function shared() { static $text = 'x'; $saved = function() use (&$text) { return $text; }; unset($text); echo $saved(); } shared();",
        "<?php function typed(string $text) { $saved = function() use (&$text) { return $text; }; unset($text); echo $saved(); } typed('x');",
        "<?php function borrowed(string &$text) { $saved = function() use (&$text) { return $text; }; unset($text); echo $saved(); } $text = 'x'; borrowed($text);",
        "<?php string $text = 'x'; $saved = function() use (&$text) { return $text; }; unset($text); echo $saved();",
        "<?php $values = ['x']; foreach ($values as &$text) { $saved = function() use (&$text) { return $text; }; unset($text); echo $saved(); }",
    ] {
        let result = check_source_full(source)
            .unwrap_or_else(|error| panic!("{}\n{source}", error.message));
        assert!(result.local_ref_detach_sites.is_empty(), "{source}: {:?}", result.local_ref_detach_sites);
    }
}
