//! Purpose:
//! Regression tests for issue #686: each recursive compiler phase, called on its own, walks
//! `MAX_COMPILER_NESTING` levels on a small thread.
//!
//! Called from:
//! - `cargo test` through Rust's test harness.
//!
//! Key details:
//! - Every fixture sets its own 256 KiB thread rather than inheriting one, so the codegen
//!   suite's 32 MiB `RUST_MIN_STACK` cannot mask a phase that lost its budget.
//! - Each fixture reports a small summary, so the deep values are dropped inside that thread
//!   too rather than on the parent's larger stack.
//! - A regression aborts the test PROCESS rather than failing an assertion, which is why these
//!   live in their own binary.
//! - The phase-level budget itself is described in `docs/internals/the-parser.md`.

use std::collections::HashSet;

/// The stack an embedder's worker thread plausibly has. Well under what depth 1024 needs.
const EMBEDDER_STACK_BYTES: usize = 256 * 1024;

/// Source nesting at the compiler's own documented limit.
const NESTING_DEPTH: usize = 1024;

/// Runs `body` on a thread with [`EMBEDDER_STACK_BYTES`] of stack and returns what it REPORTS.
///
/// The report is a small owned summary on purpose. Handing the phase's own result back through
/// `join()` would move a deeply nested AST out to the parent thread and drop it THERE, on
/// libtest's much larger stack — so the recursive `Drop` these fixtures are also about would
/// never run on the small stack at all. Returning a `String` keeps every deep value's whole
/// lifetime, destructor included, inside the thread being tested.
fn on_a_small_embedder_stack(body: impl FnOnce() -> String + Send + 'static) -> String {
    std::thread::Builder::new()
        .name("embedder-small-stack".to_string())
        .stack_size(EMBEDDER_STACK_BYTES)
        .spawn(body)
        .expect("spawning the small embedder stack")
        .join()
        .expect("the embedder thread panicked")
}

/// Summarizes a program without keeping it, so the caller reports a `String` and the AST dies
/// where it was built.
fn summarize(program: elephc::parser::ast::Program) -> String {
    format!("{} statements", program.len())
}

/// `$a = [[[…1…]]]` at the compiler's nesting limit.
fn deeply_nested_source() -> String {
    format!(
        "<?php\n$a = {}1{};\necho count($a);\n",
        "[".repeat(NESTING_DEPTH),
        "]".repeat(NESTING_DEPTH)
    )
}

/// Parses the fixture, which every phase below starts from.
fn parse_deeply_nested() -> elephc::parser::ast::Program {
    let source = deeply_nested_source();
    let tokens = elephc::lexer::tokenize(&source).expect("tokenize");
    elephc::parser::parse(&tokens).expect("parse")
}

/// Verifies `parser::parse` walks its own limit on a small stack, with brackets and with
/// parentheses.
///
/// A GUARD rather than a reproduction, and deliberately so: measured, the parser is the
/// shallowest of these walkers and survives 1024 levels of either shape on this stack with its
/// wrapper removed. That is consistent with the issue itself — the aborts were always in the
/// passes below. What this pins is that the entry still carries the budget, so the parser does
/// not become the outlier the day its frames grow.
#[test]
fn parsing_survives_the_nesting_limit_on_a_small_embedder_stack() {
    let bracketed = on_a_small_embedder_stack(|| summarize(parse_deeply_nested()));
    assert_eq!(bracketed, "2 statements");
    let parenthesized = on_a_small_embedder_stack(|| {
        let source = format!(
            "<?php\n$a = {}1{};\necho $a;\n",
            "(".repeat(NESTING_DEPTH),
            ")".repeat(NESTING_DEPTH)
        );
        let tokens = elephc::lexer::tokenize(&source).expect("tokenize");
        summarize(elephc::parser::parse(&tokens).expect("parse"))
    });
    assert_eq!(parenthesized, "2 statements");
}

/// Verifies the magic-constant walker survives the same depth called on its own.
#[test]
fn magic_constant_substitution_survives_the_nesting_limit_on_a_small_embedder_stack() {
    let report = on_a_small_embedder_stack(|| {
        let ast = parse_deeply_nested();
        summarize(elephc::magic_constants::substitute_file_and_scope_constants(
            ast,
            std::path::Path::new("embedder.php"),
        ))
    });
    assert_eq!(report, "2 statements");
}

/// Verifies the constant folder survives the same depth called on its own.
#[test]
fn constant_folding_survives_the_nesting_limit_on_a_small_embedder_stack() {
    let report = on_a_small_embedder_stack(|| {
        let ast = parse_deeply_nested();
        summarize(elephc::optimize::fold_constants(ast))
    });
    assert_eq!(report, "2 statements");
}

/// Verifies the type checker survives the same depth called on its own.
#[test]
fn type_checking_survives_the_nesting_limit_on_a_small_embedder_stack() {
    let report = on_a_small_embedder_stack(|| {
        let ast = parse_deeply_nested();
        let ast = elephc::conditional::apply(ast, &HashSet::new());
        let ast = elephc::name_resolver::resolve(ast).expect("name resolve");
        let ast = elephc::optimize::fold_constants(ast);
        match elephc::types::check(&ast) {
            Ok(_) => summarize(ast),
            Err(error) => error.message,
        }
    });
    assert_eq!(report, "2 statements");
}

/// Verifies constant propagation survives the same depth called on its own.
///
/// This is the pass the first cut of the fix missed: it runs AFTER the checker, so a fixture
/// that stopped at type checking could not reach it, and a full compile went through the
/// whole-run budget instead. Called directly, as an embedder running the optimizer would.
#[test]
fn constant_propagation_survives_the_nesting_limit_on_a_small_embedder_stack() {
    let report = on_a_small_embedder_stack(|| {
        let ast = parse_deeply_nested();
        let ast = elephc::conditional::apply(ast, &HashSet::new());
        let ast = elephc::name_resolver::resolve(ast).expect("name resolve");
        let ast = elephc::optimize::fold_constants(ast);
        let check = elephc::types::check(&ast).expect("type check");
        summarize(elephc::optimize::propagate_constants(
            ast,
            check.mixed_storage_local_names(),
        ))
    });
    assert_eq!(report, "2 statements");
}
