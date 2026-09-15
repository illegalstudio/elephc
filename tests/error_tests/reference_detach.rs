//! Purpose:
//! Checks the authorization boundary for detaching promoted reference bindings.
//!
//! Called from:
//! - The diagnostic integration test root.
//!
//! Key details:
//! - Detachment ends an ordinary local alias and permits a fresh binding with a new type.
//! - Conditional, global, static, declared and eval-visible bindings retain their existing handling.

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

/// A trailing unconditional unset detaches a captured local first assigned inside a loop.
#[test]
fn test_reference_detach_is_recorded_and_permits_a_fresh_binding() {
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
    expect_no_error(
        "<?php $text = 'x'; $saved = function() use (&$text): string { return $text; }; unset($text); $text = 1;",
    );
}

/// Conditional flow and name-addressed shared storage do not authorize slot abandonment.
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
        "<?php string $text = 'x'; $saved = function() use (&$text) { return $text; }; unset($text); echo $saved();",
        "<?php $values = ['x']; foreach ($values as &$text) { $saved = function() use (&$text) { return $text; }; unset($text); echo $saved(); }",
    ] {
        let result = check_source_full(source)
            .unwrap_or_else(|error| panic!("{}\n{source}", error.message));
        assert!(result.local_ref_detach_sites.is_empty(), "{source}: {:?}", result.local_ref_detach_sites);
    }
}

/// An incoming by-reference parameter can detach its callee-local name from caller storage.
#[test]
fn test_reference_detach_accepts_incoming_reference_storage() {
    let result = check_source_full(
        "<?php function borrowed(string &$text) { $saved = function() use (&$text) { return $text; }; unset($text); $text = 1; echo $saved(); } $text = 'x'; borrowed($text);",
    )
    .expect("the detached parameter must accept a fresh local binding");
    assert_eq!(result.local_ref_detach_sites.len(), 1);
    assert!(
        result
            .local_ref_detach_sites
            .values()
            .any(|names| names.contains("text"))
    );
}
