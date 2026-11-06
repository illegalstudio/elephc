//! Purpose:
//! Groups the control flow integration test submodules into the parent suite.
//!
//! Called from:
//! - `cargo test` through Rust's test harness.
//!
//! Key details:
//! - Submodules group focused fixtures for branches and loops, alternative (`:` … `endif;`)
//!   syntax, functions, booleans, assignments, nulls, guarded reassignment, and related suites.

use crate::support::*;

#[path = "control_flow/alternative_syntax.rs"]
mod alternative_syntax;
#[path = "control_flow/branches_and_loops.rs"]
mod branches_and_loops;
#[path = "control_flow/functions.rs"]
mod functions;
#[path = "control_flow/booleans.rs"]
mod booleans;
#[path = "control_flow/assignments/mod.rs"]
mod assignments;
#[path = "control_flow/nulls.rs"]
mod nulls;
#[path = "control_flow/ternary.rs"]
mod ternary;
#[path = "control_flow/match_expressions.rs"]
mod match_expressions;
#[path = "control_flow/closures.rs"]
mod closures;
#[path = "control_flow/guarded_reassignment.rs"]
mod guarded_reassignment;

/// Discarded `static function`/`static fn` statements are valid PHP no-ops.
#[test]
fn test_discarded_static_closure_and_arrow_statements() {
    let out = compile_and_run(r#"<?php
static function () { echo "bad"; };
static fn($x) => $x;
echo "ok";
"#);
    assert_eq!(out, "ok");
}

/// A global constant accepts an attribute group, as PHP 8.5 allows.
#[test]
fn test_top_level_const_attribute_compiles() {
    let out = compile_and_run(r#"<?php
#[Attribute] class Marker {}
#[Marker] const X = 2;
echo X;
"#);
    assert_eq!(out, "2");
}

/// A bare braced block is a valid PHP statement and keeps its enclosed statements.
#[test]
fn test_bare_braced_block_runs() {
    let out = compile_and_run(r#"<?php
{ echo 'a'; { echo 'b'; } }
"#);
    assert_eq!(out, "ab");
}

/// PHP expression statements whose result is discarded still compile and run as no-ops.
#[test]
fn test_discarded_literal_and_unary_expression_statements() {
    let out = compile_and_run(r#"<?php
1;
"text";
true;
null;
-1;
!false;
1 + 2;
echo 'ok';
"#);
    assert_eq!(out, "ok");
}
