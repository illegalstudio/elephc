//! Purpose:
//! Interpreter tests pinning `TARGET[] = &SOURCE` -- binding a NEWLY APPENDED element rather
//! than storing a value in it -- where the target is reached through another index.
//! `$loops[$k][] = &$pathInLoop;` is `dependency-injection`'s `PhpDumper`.
//!
//! Called from:
//! - `cargo test -p elephc-magician interpreter::tests::append_reference_bind`.
//!
//! Key details:
//! - Every expected string is `php -n` 8.5.6's output on the same fragment.
//! - The bare `$name[] = &$source;` form already worked; only the NESTED target was refused,
//!   because the append branch of the array-write parser went straight to `parse_expr`, which
//!   cannot start with an ampersand. `EvalStmt::ArrayAppendReferenceBind` carried a bare name
//!   for the same reason, and now carries the lvalue.
//! - A reference is SYMMETRIC, so every case writes through BOTH ends. Asserting only that the
//!   source reaches the appended element passes against a copy made at bind time.

use super::super::*;
use super::support::*;

/// Runs one fragment and returns what it echoed.
fn out(fragment: &[u8]) -> String {
    let program = parse_fragment(fragment).expect("parse append reference bind fragment");
    let mut scope = ElephcEvalScope::new();
    let mut values = FakeOps::default();
    execute_program(&program, &mut scope, &mut values)
        .expect("execute append reference bind fragment");
    values.output.clone()
}

/// Verifies an append through ONE index binds, both ways.
///
/// `php -n` 8.5.6 prints `2;3`. This is `PhpDumper`'s `$loops[$k][] = &$pathInLoop;`: the first
/// number reads the appended element after the source grew, the second reads the source after
/// the appended element grew.
#[test]
fn an_append_through_one_index_binds_both_ways() {
    assert_eq!(
        out(
            br#"$path = ["a"];
$loops = [];
$loops["k"][] = &$path;
$path[] = "b";
echo count($loops["k"][0]), ";";
$loops["k"][0][] = "c";
echo count($path);"#
        ),
        "2;3",
    );
}

/// Verifies an append through TWO indexes binds, including the arrays it has to create.
///
/// `php -n` 8.5.6 prints `9`. Neither `$deep["x"]` nor `$deep["x"]["y"]` exists before the
/// statement runs, so this also says the intermediate arrays are autovivified on the binding
/// path and not only on the value path.
#[test]
fn an_append_through_two_indexes_binds_and_autovivifies() {
    assert_eq!(
        out(
            br#"$deep = [];
$v = 1;
$deep["x"]["y"][] = &$v;
$v = 9;
echo $deep["x"]["y"][0];"#
        ),
        "9",
    );
}

/// Verifies the BARE append bind still works, which it did before this change.
///
/// `php -n` 8.5.6 prints `7`. With the fix disabled this is the case that stays green while the
/// nested ones go red, so it says which shapes the change actually bought.
#[test]
fn a_bare_append_bind_still_works() {
    assert_eq!(
        out(
            br#"$plain = [];
$w = 2;
$plain[] = &$w;
$w = 7;
echo $plain[0];"#
        ),
        "7",
    );
}
