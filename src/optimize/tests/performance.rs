//! Purpose:
//! Architectural performance regressions for whole-program AST effect analysis.
//!
//! Called from:
//! - `crate::optimize::tests` through Rust's test harness.
//!
//! Key details:
//! - These source-level invariants complement semantic optimizer tests without flaky timing thresholds.

/// Verifies the optimizer pipeline has a single effect-analysis entry instead
/// of recomputing the same whole-program fixed point independently per pass.
#[test]
fn callable_effect_analysis_is_not_recomputed_per_ast_pass() {
    let source = include_str!("../../optimize.rs");
    let calls = source
        .matches("compute_program_callable_effects(&program)")
        .count();

    assert!(
        calls <= 1,
        "the optimizer must share one whole-program effect analysis, found {calls} pass-local calls"
    );
}

/// Verifies effect analysis borrows callable bodies and updates summaries
/// incrementally instead of deep-cloning every body and complete map snapshot.
#[test]
fn callable_effect_analysis_avoids_deep_ast_and_map_snapshot_clones() {
    let source = include_str!("../../optimize.rs");

    for forbidden in [
        "body: body.clone()",
        "body: method.body.clone()",
        "let function_snapshot = function_effects.clone()",
        "let static_method_snapshot = static_method_effects.clone()",
        "let instance_method_snapshot = instance_method_effects.clone()",
        "Option<*const HashMap",
        "unsafe { ptr.as_ref() }",
    ] {
        assert!(
            !source.contains(forbidden),
            "whole-program effect analysis retains an avoidable clone: {forbidden}"
        );
    }
}

/// The exception-flow fixed point recomputes only the bodies that read a summary the previous
/// round changed. A throw at the end of a 30-function call chain moves one link per round, so
/// recomputing every body every round summarized ~30 bodies 31 times; the incremental rounds
/// summarize each link a few times. `ELEPHC_EXCEPTION_FLOW_VERIFY` is on in tests, so the result
/// is also checked against a full recompute.
#[test]
fn exception_flow_rounds_recompute_only_the_bodies_whose_reads_changed() {
    let mut source = String::from("<?php\n");
    for index in 0..30 {
        source.push_str(&format!("function f{index}($x) {{ return f{}($x + 1); }}\n", index + 1));
    }
    source.push_str("function f30($x) { if ($x > 100) { throw new RuntimeException('deep'); } return $x; }\n");
    source.push_str("try { echo f0($argc); } catch (RuntimeException $e) { echo 'caught'; }\n");
    let tokens = crate::lexer::tokenize(&source).expect("fixture must tokenize");
    let program = crate::parser::parse(&tokens).expect("fixture must parse");
    crate::optimize::exception_flow::EXCEPTION_BODY_SUMMARIES.with(|count| count.set(0));
    let program = crate::optimize::eliminate_dead_code(program, Default::default());
    let summaries = crate::optimize::exception_flow::EXCEPTION_BODY_SUMMARIES.with(std::cell::Cell::get);
    eprintln!("EXCEPTION_BODY_SUMMARIES={summaries}");
    let printed = format!("{program:?}");
    assert!(printed.contains("caught"), "the catch around the throwing chain must survive DCE");
    assert!(summaries > 0, "the fixture must reach the exception-flow fixed point");
    assert!(summaries <= 200, "{summaries} body summaries for a 31-function chain");
}
