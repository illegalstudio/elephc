//! Purpose:
//! Interpreter tests for PHP's argument-introspection trio -- `func_num_args()`,
//! `func_get_args()` and `func_get_arg()` -- and for the surplus arguments that make them useful.
//!
//! Called from:
//! - `cargo test -p elephc-magician interpreter::tests::func_args`.
//!
//! Key details:
//! - The compiler answers these by DESUGARING at compile time (`src/func_args/`), so there is no
//!   shared-catalog contract for them and there must not be: a contract would demand an AOT
//!   registry binding that cannot exist. They are interpreter intrinsics, answered before the
//!   function lookup.
//! - Two halves, and neither works alone. The trio was unreachable even once implemented, because
//!   a surplus positional argument was a fatal -- and a surplus argument is the only thing these
//!   functions exist to reach.
//! - Every expected string is `php -n` 8.5.6's output on the same fragment.

use super::super::*;
use super::support::*;

/// Runs one fragment and returns what it echoed.
fn out(fragment: &[u8]) -> String {
    let program = parse_fragment(fragment).expect("parse func args fragment");
    let mut scope = ElephcEvalScope::new();
    let mut values = FakeOps::default();
    execute_program(&program, &mut scope, &mut values).expect("execute func args fragment");
    values.output.clone()
}

/// Verifies all three read the arguments actually passed, including the surplus ones.
///
/// `php -n` 8.5.6 prints `3:3:1,2,3`. The function declares two parameters and is called with
/// three; the third binds to nothing and is reachable only here.
#[test]
fn the_trio_reads_arguments_past_the_declared_parameters() {
    assert_eq!(
        out(
            br#"function three($a, $b) {
    echo func_num_args(), ":";
    echo count(func_get_args()), ":";
    echo func_get_arg(0), ",", func_get_arg(1), ",", func_get_arg(2);
}
three(1, 2, 3);"#
        ),
        "3:3:1,2,3",
    );
}

/// Verifies a surplus positional argument is ACCEPTED rather than fatal.
///
/// `php -n` 8.5.6 prints `ok`. This is the half that made the others unreachable: binding refused
/// any positional argument past the declared list, so the call never reached its own body. php
/// accepts it for a user function, a method and a closure alike -- a builtin is different, and
/// raises ArgumentCountError, but that path does not come through here.
#[test]
fn a_surplus_positional_argument_is_accepted() {
    assert_eq!(
        out(
            br#"function one($a) { echo "ok"; }
one(1, 2, 3);"#
        ),
        "ok",
    );
}

/// Verifies `func_get_args()` reports the CURRENT value of a reassigned parameter.
///
/// `php -n` 8.5.6 prints `changed`, not `original`. So the values cannot simply be the ones the
/// call frame recorded: a declared parameter has to be read from the scope, while an argument
/// past the declared list has no variable to read and can only come from the frame. Both halves
/// are needed, and this is the case that says so.
#[test]
fn a_reassigned_parameter_is_reported_with_its_current_value() {
    assert_eq!(
        out(
            br#"function reassigned($a) {
    $a = "changed";
    $all = func_get_args();
    echo $all[0];
}
reassigned("original");"#
        ),
        "changed",
    );
}

/// Verifies the count reflects what was PASSED, not what was declared.
///
/// `php -n` 8.5.6 prints `1|2`: a defaulted parameter that the caller omitted does not count, and
/// a function declaring nothing at all still sees its arguments.
#[test]
fn the_count_is_what_the_caller_passed() {
    assert_eq!(
        out(
            br#"function fewer($a, $b = "default") { echo func_num_args(); }
fewer("only");
echo "|";
function none() { echo func_num_args(); }
none("x", "y");"#
        ),
        "1|2",
    );
}

/// Verifies methods and closures answer the same way plain functions do.
///
/// `php -n` 8.5.6 prints `2|2`. Each resolves its declared parameter names differently -- a
/// method through its class, a closure through its own metadata -- so a fix that only handled
/// plain functions would pass every test above and fail here.
#[test]
fn methods_and_closures_answer_the_same_way() {
    assert_eq!(
        out(
            br#"class M { public function go($x) { return func_num_args(); } }
$m = new M();
echo $m->go(7, 8), "|";
$c = function ($p) { return func_num_args(); };
echo $c("a", "b");"#
        ),
        "2|2",
    );
}
