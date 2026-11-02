//! Purpose:
//! Interpreter tests pinning that a refused construct SAYS what it was, instead of failing with
//! eleven fixed words.
//!
//! Called from:
//! - `cargo test -p elephc-magician interpreter::tests::refusal_diagnostics`.
//!
//! Key details:
//! - Measured, not guessed: of php-src's 2556-case language corpus, 225 ended in a bare
//!   `Fatal error: eval() runtime failed` with no phase, no name and no line. That is not a
//!   severity problem, it is a GROUPING problem -- a quarter of the corpus could not be sorted
//!   into causes at all, so none of it could be worked on.
//! - Descriptions are first-writer-wins and the innermost frame writes first, so a path that
//!   already names itself keeps its own message. These fallbacks only ever land on a failure that
//!   nothing inside it described.
//! - After the change the same 225 sort into 24 named groups, the largest being
//!   `unsupported Call expression` (50) and `class <N> could not be declared` (40).

use super::super::*;
use super::support::*;

/// Runs a fragment expected to fail and returns the description it left behind.
fn refusal_description(fragment: &[u8]) -> String {
    // A description outlives the run that left it, so clear any stale one first.
    let _ = crate::errors::take_eval_runtime_failure();
    let program = parse_fragment(fragment).expect("parse refusal fragment");
    let mut scope = ElephcEvalScope::new();
    let mut values = FakeOps::default();
    // The STATUS is deliberately not asserted. The fake runtime answers `UnsupportedConstruct`
    // where the compiled one answers `RuntimeFatal` for the same fragment, and the description is
    // what this file is about -- pinning the status here would pin a property of the fixture.
    let _ = execute_program(&program, &mut scope, &mut values)
        .expect_err("fragment should not execute");
    crate::errors::take_eval_runtime_failure()
        .map(|failure| failure.clause())
        .unwrap_or_default()
}

/// Verifies a refused expression names the expression kind.
///
/// Calling a value that is not callable left NO description at all: the bridge printed
/// `Fatal error: eval() runtime failed` and stopped. It is now `unsupported DynamicCall
/// expression`, which is what lets 11 corpus cases group under one cause rather than sitting in
/// an undifferentiated pile of 225.
///
/// The fallback fires for `RuntimeFatal` only, deliberately. `UnsupportedConstruct` already
/// carries its own wording through a different bridge message -- `eval() fragment uses an
/// unsupported construct: call to undefined function error_reporting()` -- and all 225 opaque
/// cases were `RuntimeFatal`. Widening it would double-describe the half that was never the
/// problem.
///
/// The clause has no `in FILE on line N` here because a fragment executed without a call site has
/// no file; that half is what the include-path tests cover, where there is one.
#[test]
fn a_refused_expression_names_the_expression_kind() {
    assert_eq!(
        refusal_description(br#"$a = [1]; $a();"#),
        ": unsupported DynamicCall expression",
    );
}

/// Verifies a class whose parent is missing is no longer a refusal at all.
///
/// It used to be refused with ": class C could not be declared" (the class named deeper than the
/// generic `unsupported ClassDecl statement` fallback). The interpreter now throws php's catchable
/// `Error` (`Class "NoSuchParent" not found`) from `ensure_eval_class_parent_available`, so there
/// is no refusal description to write.
#[test]
fn a_class_with_a_missing_parent_throws_instead_of_being_refused() {
    assert_eq!(refusal_description(br#"class C extends NoSuchParent {}"#), "");
}

/// Verifies a path that already describes itself keeps its own message.
///
/// This is the half that makes the fallbacks safe to add everywhere. `undefined constant "X"` is
/// written by the constant lookup, deeper than either fallback; if first-writer-wins were not
/// respected, this would read `unsupported Const expression` and the change would have made
/// diagnostics WORSE while appearing to make them better.
#[test]
fn a_path_that_names_itself_is_not_overwritten_by_the_fallback() {
    assert_eq!(
        refusal_description(br#"echo UNDEFINED_CONSTANT_NAME;"#),
        ": undefined constant \"UNDEFINED_CONSTANT_NAME\"",
    );
}

