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

/// Case-folded summaries stay scoped to their analysis, including builtin and unknown misses.
#[test]
fn test_callable_casefold_lookup_preserves_scoped_effects() {
    use super::*;
    let outer = HashMap::from([("MixedCase".to_string(), Effect::PURE)]);
    with_callable_effects(outer, HashMap::new(), HashMap::new(),
        InstanceDispatchMetadata::default(), || {
            assert_eq!(function_call_effect("MIXEDCASE", &[]), Effect::PURE);
            let inner = HashMap::from([("mixedCASE".to_string(), Effect::PURE.with_may_throw())]);
            with_callable_effects(inner, HashMap::new(), HashMap::new(),
                InstanceDispatchMetadata::default(), || {
                    assert!(function_call_effect("MixedCase", &[]).may_throw);
                    assert!(function_call_effect("missingFunction", &[]).writes_globals);
                    assert!(!function_call_effect("strlen", &[Expr::string_lit("x")]).writes_globals);
                });
            assert_eq!(function_call_effect("mixedcase", &[]), Effect::PURE);
        });
}

/// Different-case relay calls retain their exact exception domain through fixed-point analysis.
#[test]
fn test_callable_casefold_lookup_preserves_exception_domain() {
    use super::*;
    let tokens = crate::lexer::tokenize("<?php function CaseFoldThrow(): never { throw new RuntimeException('x'); } function CaseFoldRelay(): never { CASEFOLDTHROW(); } CASEFOLDRELAY();").unwrap();
    let program = crate::parser::parse(&tokens).unwrap();
    let analysis = CheckerExceptionAnalysis::new(&program);
    let StmtKind::ExprStmt(call) = &program.last().unwrap().kind else { panic!("missing call"); };
    let thrown = analysis.operation_throws(call, None);
    let (caught, remaining) = analysis.split(thrown, &["RuntimeException".to_string()]);
    assert!(!caught.is_empty());
    assert!(remaining.is_empty());
}

/// A declared int wrapper around a never call cannot add a return verification exception.
#[test]
fn test_declared_int_return_does_not_add_throw_after_never_call() {
    use super::*;
    let tokens = crate::lexer::tokenize("<?php function StopResult(): never { throw new LogicException('x'); } function relayResult(): int { return STOPRESULT(); } relayResult();").unwrap();
    let program = crate::parser::parse(&tokens).unwrap();
    let analysis = CheckerExceptionAnalysis::new(&program);
    let StmtKind::ExprStmt(call) = &program.last().unwrap().kind else { panic!("missing call"); };
    let thrown = analysis.operation_throws(call, None);
    let (caught, remaining) = analysis.split(thrown, &["LogicException".to_string()]);
    assert!(!caught.is_empty());
    assert!(remaining.is_empty());
}
