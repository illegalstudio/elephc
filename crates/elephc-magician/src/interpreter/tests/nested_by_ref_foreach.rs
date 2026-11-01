//! Purpose:
//! Pins the interpreter's by-reference `foreach` writeback for the shape where BOTH the outer
//! and inner loop bind their loop variable by reference -- `foreach ($d as &$row) { foreach
//! ($row as &$cell) { ... } }` -- the exact construct
//! `examples/symfony-app/vendor/symfony/event-dispatcher/EventDispatcher.php`'s
//! `optimizeListeners()` uses.
//!
//! Called from:
//! - `cargo test -p elephc-magician interpreter::tests::nested_by_ref_foreach`.
//!
//! Key details:
//! - Every expected string is `php -n` 8.5.6's output on the same fragment.
//! - The bug: the inner loop's by-reference subject was resolved to a plain `Variable` target
//!   naming the outer loop variable, so a write reached only that variable's OWN local scope
//!   cell and never propagated through the outer variable's own reference target back into the
//!   array it aliases.

use super::super::*;
use super::support::*;

/// Runs one fragment and returns what it echoed.
fn out(fragment: &[u8]) -> String {
    let program = parse_fragment(fragment).expect("parse nested by-ref foreach fragment");
    let mut scope = ElephcEvalScope::new();
    let mut values = FakeOps::default();
    execute_program(&program, &mut scope, &mut values)
        .expect("execute nested by-ref foreach fragment");
    values.output.clone()
}

/// Verifies a write through the INNER by-reference loop variable reaches the outer array when
/// BOTH loop levels bind their loop variable by reference.
///
/// `php -n` 8.5.6 prints `X,Y/Z`. The interpreter printed `x,y/z` instead: the inner write
/// landed only in the outer loop variable's own local scope cell, which the outer loop's next
/// iteration silently discards, so the source array was never touched.
#[test]
fn both_levels_by_reference_write_reaches_the_outer_array() {
    assert_eq!(
        out(
            br#"$d = [["x", "y"], ["z"]];
foreach ($d as &$row) {
    foreach ($row as &$cell) {
        $cell = strtoupper($cell);
    }
    unset($cell);
}
unset($row);
echo implode(",", $d[0]), "/", implode(",", $d[1]);"#
        ),
        "X,Y/Z",
    );
}

/// Verifies the already-working single-level by-reference case stays correct beside the fix.
///
/// `php -n` 8.5.6 prints `X,Y`. This half needs no propagation through an outer reference
/// target at all, so it stays green whether or not the nested fix is present -- it is here to
/// show which line the fix actually moves.
#[test]
fn a_single_level_by_reference_write_keeps_working() {
    assert_eq!(
        out(
            br#"$a = ["x", "y"];
foreach ($a as &$v) {
    $v = strtoupper($v);
}
unset($v);
echo implode(",", $a);"#
        ),
        "X,Y",
    );
}
