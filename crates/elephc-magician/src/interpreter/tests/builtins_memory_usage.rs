//! Purpose:
//! Interpreter tests for `memory_get_usage()` and `memory_get_peak_usage()`: which counter each
//! call asks the runtime for, php's weak-mode truthiness on `$real_usage`, and the arity limit.
//!
//! Called from:
//! - `cargo test -p elephc-magician interpreter::tests::builtins_memory_usage`.
//!
//! Key details:
//! - The BYTE COUNT is not computed in eval, so there is no arithmetic to assert here. What
//!   these tests pin is the DISPATCH: `FakeOps::memory_usage_bytes` returns a different sentinel
//!   for each `(peak, real_usage)` pair, so an implementation that swapped the two flags, or
//!   dropped the argument, changes the echoed number and fails. The real numbers are covered
//!   end-to-end against php by the probes in `scratchpad/mem`.
//! - Every truthiness expectation below is php 8.5.10's own weak-mode bool coercion, checked
//!   with `php -r 'var_dump((bool)1, (bool)0, (bool)"x", (bool)"", (bool)0.0, (bool)1.5);'`
//!   -> true,false,true,false,false,true.

use super::super::*;
use super::support::*;

/// Runs one fragment and returns what it echoed.
fn out(fragment: &[u8]) -> String {
    let program = parse_fragment(fragment).expect("parse memory reporter fragment");
    let mut scope = ElephcEvalScope::new();
    let mut values = FakeOps::default();
    execute_program(&program, &mut scope, &mut values).expect("execute memory reporter fragment");
    values.output.clone()
}

/// Runs one fragment that is expected to fail, returning the status.
fn status(fragment: &[u8]) -> EvalStatus {
    let program = parse_fragment(fragment).expect("parse memory reporter fragment");
    let mut scope = ElephcEvalScope::new();
    let mut values = FakeOps::default();
    execute_program(&program, &mut scope, &mut values)
        .expect_err("fragment was expected to fail")
}

/// Verifies each of the four (peak, real) combinations reaches the runtime distinctly, and that
/// the omitted argument means `false` rather than "unspecified".
///
/// Sentinels: usage/live=1000, usage/real=2000, peak/live=3000, peak/real=4000.
#[test]
fn memory_reporters_select_the_peak_and_real_counters_independently() {
    assert_eq!(
        out(br#"echo memory_get_usage(), ":";
echo memory_get_usage(false), ":";
echo memory_get_usage(true), ":";
echo memory_get_peak_usage(), ":";
echo memory_get_peak_usage(false), ":";
echo memory_get_peak_usage(true);"#),
        "1000:1000:2000:3000:3000:4000",
    );
}

/// Verifies `$real_usage` goes through php's weak-mode bool coercion rather than an
/// is-it-literally-`true` test: `1`, `"x"` and `1.5` all select the real counter, while `0`,
/// `""` and `0.0` do not. php 8.5.10 agrees on every one of these casts.
#[test]
fn memory_reporters_read_real_usage_for_truthiness_not_identity() {
    assert_eq!(
        out(br#"echo memory_get_usage(1), ":";
echo memory_get_usage(0), ":";
echo memory_get_usage("x"), ":";
echo memory_get_usage(""), ":";
echo memory_get_usage(0.0), ":";
echo memory_get_usage(1.5);"#),
        "2000:1000:2000:1000:1000:2000",
    );
}

/// Verifies the reporters answer a plain integer, not a string or a float, so arithmetic on the
/// result behaves as callers expect. php: `int(...)` for every mode.
#[test]
fn memory_reporters_return_integers() {
    assert_eq!(
        out(br#"echo gettype(memory_get_usage()), ":";
echo gettype(memory_get_peak_usage(true));"#),
        "integer:integer",
    );
}

/// Verifies a second argument is refused rather than ignored, with php's catchable
/// `ArgumentCountError`. `php -n` 8.5: `memory_get_usage() expects at most 1 argument, 2 given`,
/// and the same wording for `memory_get_peak_usage()`. Uncaught, it ends the fragment as an
/// uncaught throwable.
#[test]
fn memory_reporters_refuse_a_second_argument() {
    assert_eq!(status(br#"memory_get_usage(true, 1);"#), EvalStatus::UncaughtThrowable);
    assert_eq!(
        status(br#"memory_get_peak_usage(true, 1);"#),
        EvalStatus::UncaughtThrowable
    );
    assert_eq!(
        out(br#"try { memory_get_usage(true, 1); } catch (ArgumentCountError $e) { echo get_class($e), ": ", $e->getMessage(), "|"; }
try { memory_get_peak_usage(true, 1); } catch (ArgumentCountError $e) { echo get_class($e), ": ", $e->getMessage(); }"#),
        "ArgumentCountError: memory_get_usage() expects at most 1 argument, 2 given|\
ArgumentCountError: memory_get_peak_usage() expects at most 1 argument, 2 given",
    );
}
