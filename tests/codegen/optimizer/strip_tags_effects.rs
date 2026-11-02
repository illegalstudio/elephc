//! Purpose:
//! Preserves strip_tags argument errors and allow-list conversion callbacks through optimization.
//!
//! Called from:
//! - The optimizer codegen integration suite.
//!
//! Key details:
//! - Discarding the return string must not discard a catchable error or user callback.
//! - Every fixture checks native ownership with EIR optimization on and off.

use crate::support::*;

/// Checks observable strip_tags effects and cleanup after a discarded return value.
fn assert_strip_effects(source: &str, expected: &str) {
    let out = compile_and_run_with_heap_debug(source);
    assert!(out.success, "{}", out.stderr);
    assert_eq!(out.stdout, expected, "{}", out.stderr);
    assert!(out.stderr.contains("HEAP DEBUG: leak summary: clean"), "{}", out.stderr);
}

/// An unused strict call must remain inside its catch boundary.
#[test]
fn test_strip_tags_review_discarded_strict_error() {
    assert_strip_effects(r#"<?php
declare(strict_types=1);
try { strip_tags('<b>x</b>', 1); echo 'bad'; }
catch (TypeError $error) { echo 'caught'; }
"#, "caught");
}

/// Discarded strict calls keep their exception without EIR optimization too.
#[test]
fn test_strip_tags_review_discarded_strict_error_no_ir_opt() {
    without_ir_opt(test_strip_tags_review_discarded_strict_error);
}

/// Stringable allow-list entries retain their user-visible conversion callback.
#[test]
fn test_strip_tags_review_discarded_allow_callback() {
    assert_strip_effects(r#"<?php
class AllowedTag { public function __toString(): string { echo 'T'; return 'p'; } }
function discardTags(array $tags): void { strip_tags('<p>x</p>', $tags); }
discardTags([new AllowedTag()]); echo 'done';
"#, "Tdone");
}

/// Allow-list callbacks remain observable without EIR optimization.
#[test]
fn test_strip_tags_review_discarded_allow_callback_no_ir_opt() {
    without_ir_opt(test_strip_tags_review_discarded_allow_callback);
}
