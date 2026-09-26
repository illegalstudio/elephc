//! Purpose:
//! Regression tests for when PHP reads the index of an element write relative to
//! the right-hand side that produces the value.
//!
//! Called from:
//! - `cargo test` through the `codegen_tests` harness via `crate::support`.
//!
//! Key details:
//! - PHP freezes an index *expression* into a temporary before the right-hand
//!   side runs, so its side effects happen first and the destination slot is
//!   whatever that expression returned.
//! - A plain *variable* index in a simple write is not frozen: the store reads
//!   the variable's slot at store time, after the right-hand side. `$i = 0; $a[$i] = ($i = 1);`
//!   therefore writes index 1, and elephc used to write index 0 — silently, with
//!   no diagnostic, in a shape that reads as obviously index 0.
//! - Constant propagation runs ahead of EIR lowering and folded the index against
//!   the pre-right-hand-side environment, so the deferral has to hold in both
//!   passes or the folded literal wins and the lowering rule never applies.
//! - The witnesses assign then READ BACK. An `echo` inside the write proves
//!   nothing about which slot received it.

use crate::support::*;

/// Pins that a plain-variable index is read at store time, so a right-hand side
/// that reassigns it decides which slot is written.
#[test]
fn test_variable_index_is_read_after_the_right_hand_side() {
    let out = compile_and_run(
        r#"<?php
$a = [10, 20];
$i = 0;
$a[$i] = ($i = 1);
echo $a[0], ",", $a[1], ",", $i;
"#,
    );
    assert_eq!(out, "10,1,1");
}

/// Pins that an index *expression* keeps its place: its side effects run before
/// the right-hand side, and the slot it returned is the one written even though
/// the right-hand side goes on to change the variable it read.
#[test]
fn test_index_expression_is_frozen_before_the_right_hand_side() {
    let out = compile_and_run(
        r#"<?php
function idx(): int {
    echo "[idx]";
    return 0;
}
$a = [10, 20];
$i = 0;
$a[idx() + $i] = ($i = 1);
echo "=>", $a[0], ",", $a[1];
"#,
    );
    assert_eq!(out, "[idx]=>1,20");
}

/// Pins the same rule for `??=`, whose write only happens when the key is absent.
/// The key must be MISSING for this to prove anything: with the key present no
/// write occurs at all and both the old and the new lowering "agree".
#[test]
fn test_coalesce_assign_reads_its_variable_index_at_store_time() {
    let out = compile_and_run(
        r#"<?php
$a = [10, 20];
$slot = 2;
$a[$slot] ??= ($slot = 0);
echo $a[0], ",", $a[1], ",", $slot;
"#,
    );
    assert_eq!(out, "0,20,0");
}

/// A compound write reads its element at store time too, so a right-hand side that
/// reassigns a plain-variable index decides which element is READ as well as which
/// is written. `$c = [10, 20]; $k = 0; $c[$k] += ($k = 1);` gives `[10, 21]`: index
/// 1 both times, 20 + 1. The desugar embeds the read inside the right-hand side, so
/// this needs the right-hand side settled into a temporary before the read.
#[test]
fn test_compound_write_reads_its_element_at_the_settled_index() {
    let out = compile_and_run(
        r#"<?php
$c = [10, 20];
$k = 0;
$c[$k] += ($k = 1);
echo $c[0], ",", $c[1], ",", $k;
"#,
    );
    assert_eq!(out, "10,21,1");
}

/// The same rule for the NON-COMMUTATIVE operators. Settling the right-hand side
/// early must not reorder the operands themselves: `.=` still appends and `-=` still
/// subtracts in that direction, so a fix that hoisted the element instead of the
/// right-hand side is visible here and invisible under `+=`.
#[test]
fn test_compound_write_keeps_its_operand_order_at_the_settled_index() {
    let out = compile_and_run(
        r#"<?php
$c = ["a", "b"];
$k = 0;
$c[$k] .= ($k = 1);
$g = [10, 20];
$p = 0;
$g[$p] -= ($p = 1);
echo $c[0], ",", $c[1], ";", $g[0], ",", $g[1];
"#,
    );
    assert_eq!(out, "a,b1;10,19");
}

/// The RHS hoist must not fire for a value that cannot touch the index. Updates
/// still capture a mutable index after the RHS so a diagnostic handler cannot
/// redirect the write. `$c[$k] += $n` and `$c[$k]++` are common forms.
#[test]
fn test_compound_write_is_unchanged_when_the_right_hand_side_cannot_touch_the_index() {
    let out = compile_and_run(
        r#"<?php
function five(): int {
    return 5;
}
$c = [10, 20, 30];
$k = 1;
$n = 2;
$c[$k] += $n;
$c[$k]++;
$c[2] += five();
echo $c[0], ",", $c[1], ",", $c[2], ",", $k;
"#,
    );
    assert_eq!(out, "10,23,35,1");
}

/// An index EXPRESSION stays frozen for a compound write too: `$c[k()] += r()`
/// evaluates `k()` first and stores at whatever it returned, even though the
/// right-hand side runs before the element is read.
#[test]
fn test_compound_write_freezes_an_index_expression() {
    let out = compile_and_run(
        r#"<?php
function k(): int {
    echo "[k]";
    return 0;
}
function r(): int {
    echo "[r]";
    return 5;
}
$c = [10, 20];
$c[k()] += r();
echo "=>", $c[0], ",", $c[1];
"#,
    );
    assert_eq!(out, "[k][r]=>15,20");
}

/// Pins that deferring the index read did not disturb the ordinary case, where
/// the right-hand side leaves the index variable alone.
#[test]
fn test_variable_index_write_is_unchanged_when_the_right_hand_side_leaves_it_alone() {
    let out = compile_and_run(
        r#"<?php
$a = [10, 20, 30];
$i = 1;
$a[$i] = $a[$i] + 5;
$j = 2;
$a[$j] = $i + $j;
echo $a[0], ",", $a[1], ",", $a[2];
"#,
    );
    assert_eq!(out, "10,25,3");
}

/// Verifies a plain-variable index is read at STORE time for PROPERTY and STATIC-property writes.
///
/// PHP freezes an index EXPRESSION into a temporary before the right-hand side runs, but a plain
/// variable index is not frozen — the store reads the variable's slot after the right-hand side.
/// The bare-local write already followed that rule; `$o->a[$i] = ($i = 1)` and `C::$a[$i] = …`
/// did not, and both wrote index 0 where PHP writes index 1.
///
/// TWO authorities have to agree, which is why fixing the lowering alone changed nothing at all:
/// constant propagation runs ahead of EIR lowering and had already folded `$i` to its old value,
/// so the lowering never saw a variable to defer. Both now route through the same helper as the
/// bare-local case.
#[test]
fn test_store_time_index_for_property_and_static_writes() {
    let property = compile_and_run(
        r#"<?php
class O { public array $a = [10, 20]; }
$o = new O();
$i = 0;
$o->a[$i] = ($i = 1);
echo $o->a[0], ":", $o->a[1];
"#,
    );
    assert_eq!(property, "10:1");

    let static_property = compile_and_run(
        r#"<?php
class C { public static array $a = [10, 20]; }
$i = 0;
C::$a[$i] = ($i = 1);
echo C::$a[0], ":", C::$a[1];
"#,
    );
    assert_eq!(static_property, "10:1");
}

/// Verifies a COMPOUND write whose right-hand side mutates the index through a closure.
///
/// `+=` reads its element at store time as well, so the read and the write must see the SAME
/// index — the one the closure left behind. This shape is the witness the earlier gate could not
/// see: that gate settled the right-hand side into a temporary only when it MENTIONED the index
/// by name, and a closure capturing `&$k` mentions nothing. It answers correctly today; without a
/// test, a return to the "mentions the index" rule would pass every existing case and break this
/// one silently.
#[test]
fn test_compound_write_index_mutated_by_a_closure() {
    let out = compile_and_run(
        r#"<?php
$c = [10, 20];
$k = 0;
$c[$k] += (function () use (&$k) { $k = 1; return 5; })();
echo $c[0], ":", $c[1];
"#,
    );
    assert_eq!(out, "10:25");
}

/// Verifies a NESTED write reads every plain-variable index at store time, and only those.
///
/// `$a[$i][$i] = ($i = 1)` reads BOTH indices after the right-hand side, so it must leave
/// `$a[0]` alone — this answered `1:20` where PHP answers `10:20`. The index lives inside the
/// target expression rather than beside it, so the helper the flat writes share does not apply;
/// the rule here defers the WHOLE target, which is sound only because the shape is checked first.
///
/// The second half is what makes that check load-bearing rather than decorative. With an index
/// EXPRESSION the target must keep its place, or a call moves across the right-hand side:
/// `$a[idx()][idx()] = val()` prints `[idx][idx][val]` on both engines. A rule that deferred
/// unconditionally would still pass the first assertion and reorder those calls silently.
#[test]
fn test_store_time_index_for_nested_writes() {
    let bare_variables = compile_and_run(
        r#"<?php
$a = [[10, 20], "s"];
$i = 0;
$a[$i][$i] = ($i = 1);
echo $a[0][0], ":", $a[0][1];
"#,
    );
    assert_eq!(bare_variables, "10:20");

    let index_expressions = compile_and_run(
        r#"<?php
function idx() { echo "[idx]"; return 0; }
function val() { echo "[val]"; return 9; }
$a = [[10, 20], "s"];
$a[idx()][idx()] = val();
echo ":", $a[0][0];
"#,
    );
    assert_eq!(index_expressions, "[idx][idx][val]:9");
}
