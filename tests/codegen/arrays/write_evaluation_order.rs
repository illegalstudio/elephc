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

/// Pins that an element write fetches its receiver after the key and the value (#1653): when
/// either one reassigns the receiver, the write lands in the array the variable holds THEN.
/// elephc used to fetch the receiver first and write into the array the reassignment released,
/// which printed garbage keys, hung, or crashed (this program segfaulted). Covers a keyed
/// write, an indexed write and append, a key that reassigns, a by-reference alias, a
/// by-reference call argument, a global written through a call, and instance and static
/// property elements.
#[test]
fn test_element_write_fetches_the_receiver_after_a_reassigning_operand() {
    let out = compile_and_run(
        r#"<?php
function keyed(): string {
    $m = ["a" => "x"];
    $m["k"] = ($m = ["y" => "z"]) ? "p" : "q";
    return json_encode($m);
}
function indexed(): string {
    $a = [1, 2];
    $a[0] = ($a = [7, 8, 9])[2];
    $b = [1, 2];
    $b[] = ($b = [7, 8, 9])[2];
    return json_encode($a) . json_encode($b);
}
function key_side(): string {
    $m = ["a" => "x"];
    $m[($m = ["w" => "v"]) ? "k" : "j"] = "n";
    return json_encode($m);
}
function alias(): string {
    $m = ["a" => "x"];
    $r = &$m;
    $m["k"] = ($r = ["y" => "z"]) ? "p" : "q";
    return json_encode($m);
}
function reset_by_ref(array &$x): string { $x = ["y" => "z"]; return "k"; }
function by_ref_arg(): string {
    $m = ["a" => "x"];
    $m[reset_by_ref($m)] = "v";
    return json_encode($m);
}
class O { public array $arr = ["a" => "x"]; }
class S { public static array $arr = ["a" => "x"]; }
function props(): string {
    $o = new O();
    $o->arr["k"] = ($o->arr = ["y" => "z"]) ? "p" : "q";
    S::$arr["k"] = (S::$arr = ["y" => "z"]) ? "p" : "q";
    return json_encode($o->arr) . json_encode(S::$arr);
}
function reset_global(): string { global $gm; $gm = ["y" => "z"]; return "k"; }
$gm = ["a" => "x"];
$gm[reset_global()] = "v";
echo keyed(), "\n", indexed(), "\n", key_side(), "\n", alias(), "\n", by_ref_arg(), "\n";
echo props(), "\n", json_encode($gm), "\n";
"#,
    );
    assert_eq!(
        out,
        concat!(
            "{\"y\":\"z\",\"k\":\"p\"}\n",
            "[9,8,9][7,8,9,9]\n",
            "{\"w\":\"v\",\"k\":\"n\"}\n",
            "{\"y\":\"z\",\"k\":\"p\"}\n",
            "{\"y\":\"z\",\"k\":\"v\"}\n",
            "{\"y\":\"z\",\"k\":\"p\"}{\"y\":\"z\",\"k\":\"p\"}\n",
            "{\"y\":\"z\",\"k\":\"v\"}\n",
        )
    );
}

/// The late receiver fetch keeps ownership balanced: reassigning keyed, indexed, boxed and
/// property writes with owned string keys and values run in a loop under `--heap-debug` and
/// leave nothing live (#1653).
#[test]
fn test_element_write_reassigning_operand_is_heap_clean() {
    let out = compile_and_run_with_heap_debug(
        r#"<?php
class O { public array $arr = ["a" => "x"]; }
function run(int $n): string {
    $m = ["a" => "x"];
    $m["k" . $n] = ($m = ["y" => "z" . $n]) ? "p" . $n : "q";
    $a = [1, 2];
    $a[] = ($a = [7, 8, 9])[2];
    $x = $n > 1000 ? 1 : ["a" => "x"];
    $x["k" . $n] = ($x = ["y" => "z" . $n]) ? "p" . $n : "q";
    $o = new O();
    $o->arr["k"] = ($o->arr = ["y" => "z" . $n]) ? "p" : "q";
    return count($m) . count($a) . json_encode($x) . count($o->arr);
}
$out = "";
for ($i = 0; $i < 40 + ($argc > 5 ? 1 : 0); $i++) { $out = run($i); }
echo $out, "\n";
"#,
    );
    assert!(out.success, "program failed: {}", out.stderr);
    assert_eq!(out.stdout, "24{\"y\":\"z39\",\"k39\":\"p39\"}2\n");
    assert!(out.stderr.contains("HEAP DEBUG: leak summary: clean"), "{}", out.stderr);
}

/// Pins the receiver-last order for the review shapes of #1653: a value that writes THROUGH the
/// receiver (`$m[0] = ($m["b"] = 5)`, also in the key), a nested write whose value reassigns or
/// writes into the chain's root (on a `mixed` local and on a property), a property reassigned
/// through its own reference alias, and a key borrowed from the array the value then replaces.
#[test]
fn test_element_write_receiver_order_review_shapes() {
    let out = compile_and_run(
        r#"<?php
function through_mixed(mixed $m) { $m[0] = ($m["b"] = 5); return json_encode($m); }
function through_local(): string { $m = [1, 2]; $m[0] = ($m["x"] = 5); return json_encode($m); }
function through_key(): string { $m = [1, 2]; $m[($m[1] = 7) - 7] = 3; return json_encode($m); }
function nested_reassign(mixed $m) { $m["a"]["b"] = ($m = ["a" => ["c" => 1]]) ? 5 : 6; return json_encode($m); }
function nested_through(mixed $m) { $m["a"]["b"] = ($m["a"] = ["c" => 1]) ? 5 : 6; return json_encode($m); }
class N { public mixed $p = ["a" => ["x" => 0]]; }
function nested_prop(): string { $o = new N(); $o->p["a"]["b"] = ($o->p = ["a" => ["c" => 1]]) ? 5 : 6; return json_encode($o->p); }
class O { public array $arr = ["a" => 1]; }
function prop_alias(): string { $o = new O(); $r = &$o->arr; $o->arr["k"] = ($r = ["y" => "z"]) ? "p" : "q"; return json_encode($o->arr); }
function borrowed(mixed $m) { $m[$m["k"]] = ($m = ["k" => "z"]) ? $m["k"] : "q"; return json_encode($m); }
echo through_mixed([1, 2]), "\n", through_local(), "\n", through_key(), "\n";
echo nested_reassign(["a" => ["x" => 0]]), "\n", nested_through(["a" => ["x" => 0]]), "\n", nested_prop(), "\n";
echo prop_alias(), "\n", borrowed(["k" => "a"]), "\n";
"#,
    );
    assert_eq!(
        out,
        concat!(
            "{\"0\":5,\"1\":2,\"b\":5}\n",
            "{\"0\":5,\"1\":2,\"x\":5}\n",
            "[3,7]\n",
            "{\"a\":{\"c\":1,\"b\":5}}\n",
            "{\"a\":{\"c\":1,\"b\":5}}\n",
            "{\"a\":{\"c\":1,\"b\":5}}\n",
            "{\"y\":\"z\",\"k\":\"p\"}\n",
            "{\"k\":\"z\",\"a\":\"z\"}\n",
        )
    );
}

/// The review shapes leave nothing live under `--heap-debug`, including a key borrowed from the
/// array the value replaces and an owned key pinned across a value that throws on every other
/// iteration (#1653).
#[test]
fn test_element_write_receiver_order_review_shapes_are_heap_clean() {
    let out = compile_and_run_with_heap_debug(
        r#"<?php
class N { public mixed $p = ["a" => ["x" => 0]]; }
class O { public array $arr = ["a" => 1]; }
function key_of(int $n): string { return "k" . $n; }
function boom(int $n): string { if ($n % 2 == 0) { throw new RuntimeException("boom"); } return "v" . $n; }
function run(mixed $m, int $n): string {
    $m[$m["k"] . $n] = ($m = ["k" => "z" . $n]) ? $m["k"] : "q";
    $m[0] = ($m["b"] = "w" . $n);
    $m["a"]["b"] = ($m = ["a" => ["c" => $n]]) ? "d" . $n : "e";
    $o = new N();
    $o->p["a"]["b"] = ($o->p = ["a" => ["c" => $n]]) ? "f" : "g";
    $q = new O();
    $r = &$q->arr;
    $q->arr["k"] = ($r = ["y" => "z" . $n]) ? "p" : "q";
    $t = ["s" => "x"];
    try {
        $t[key_of($n)] = ($t = ["u" => "y"]) ? boom($n) : "none";
    } catch (RuntimeException $e) {
        $t["caught"] = "1";
    }
    return json_encode($m) . json_encode($o->p) . json_encode($q->arr) . json_encode($t);
}
$out = "";
for ($i = 0; $i < 40 + ($argc > 5 ? 1 : 0); $i++) { $out = run(["k" => "a"], $i); }
echo $out, "\n";
"#,
    );
    assert!(out.success, "program failed: {}", out.stderr);
    assert_eq!(
        out.stdout,
        "{\"a\":{\"c\":39,\"b\":\"d39\"}}{\"a\":{\"c\":39,\"b\":\"f\"}}{\"y\":\"z39\",\"k\":\"p\"}{\"u\":\"y\",\"k39\":\"v39\"}\n"
    );
    assert!(out.stderr.contains("HEAP DEBUG: leak summary: clean"), "{}", out.stderr);
}

/// Pins that a compound write to an element READ the element AFTER the right-hand side, so an
/// index other than a plain variable matches PHP too. `$a[0] += ($a = [10])[0]` leaves `[20]`:
/// the reassignment installs `[10]`, the read sees it, and 10 + 10 is written back. A LITERAL
/// index used to read the old element before the right-hand side ran, yielding `[11]`.
#[test]
fn test_compound_write_reads_a_literal_index_after_the_right_hand_side() {
    let out = compile_and_run(
        r#"<?php
$a = [1];
$a[0] += ($a = [10])[0];
echo json_encode($a), ",";
$b = ["k" => 1];
$b["k"] += ($b = ["k" => 10])["k"];
echo json_encode($b);
"#,
    );
    assert_eq!(out, "[20],{\"k\":20}");
}

/// Pins a nested increment/decrement THROUGH-write: `$m[0] = ++$m[1][0]` must fetch the receiver
/// AFTER the value, because the increment stores into `$m`. The increment desugars into an
/// assignment whose element store lives in its prelude, which the receiver gate now walks.
#[test]
fn test_receiver_fetch_for_a_nested_increment_through_write() {
    let prefix = compile_and_run(
        r#"<?php
$m = [9, [3, 4]];
$m[0] = ++$m[1][0];
echo json_encode($m);
"#,
    );
    assert_eq!(prefix, "[4,[4,4]]");

    let postfix = compile_and_run(
        r#"<?php
$m = [9, [3, 4]];
$m[0] = $m[1][0]++;
echo json_encode($m);
"#,
    );
    assert_eq!(postfix, "[3,[4,4]]");
}

/// Pins a variable-index nested write whose value reassigns the root: `$m[$i][$j] = ($m = […]) ? …`
/// must write into the array the reassignment installed (and rebox the retyped root so the
/// Mixed-only nested writer still accepts it). It used to fail to compile entirely.
#[test]
fn test_variable_index_nested_write_reboxes_a_reassigned_root() {
    let assoc = compile_and_run(
        r#"<?php
function v(mixed $m, string $i, string $j): string {
    $m[$i][$j] = ($m = ["a" => ["c" => 1]]) ? 5 : 6;
    return json_encode($m);
}
echo v(["a" => ["x" => 0]], "a", "b");
"#,
    );
    assert_eq!(assoc, "{\"a\":{\"c\":1,\"b\":5}}");

    let indexed = compile_and_run(
        r#"<?php
function v(mixed $m, int $i, int $j): string {
    $m[$i][$j] = ($m = [[0, 0], [0, 0]]) ? 5 : 6;
    return json_encode($m);
}
echo v([[1, 1], [2, 2]], 1, 0);
"#,
    );
    assert_eq!(indexed, "[[0,0],[5,0]]");
}
