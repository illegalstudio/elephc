//! Purpose:
//! Integration tests for integer keys that skip slots of a non-empty packed local array.
//!
//! Called from:
//! - `cargo test` through Rust's test harness, via `arrays/mod.rs`.
//!
//! Key details:
//! - PHP promotes a packed array to a hash the moment an integer key skips a slot or is
//!   negative; packed storage must never zero-fill the gap. Keys the checker can bound (a
//!   literal `0`, the unconditional counter of a `for` loop starting at `0`) stay packed.

use crate::support::*;

/// Literal and variable keys past the end of a typed packed local keep PHP's hash shape.
#[test]
fn test_nonempty_local_skipping_key_promotes_to_hash() {
    let out = compile_and_run(r#"<?php
function lit(): array { $a = [1, 2]; $a[5] = 3; return $a; }
function var_key(int $k, int $n): array { $a = [1]; $a[$k] = $n; return $a; }
function neg(int $k): array { $a = [1, 2]; $a[$k] = 9; return $a; }
function str_then_int(int $k, int $n): array { $a = []; $a[0] = "s"; $a[$k] = $n; return $a; }
function in_range(int $k): array { $a = [1, 2, 3]; $a[$k] = 7; return $a; }
echo json_encode(lit()), "\n";
echo json_encode(var_key(3, 6)), " ", count(var_key(3, 6)), "\n";
echo json_encode(neg(-1)), "\n";
echo json_encode(str_then_int(3, 6)), "\n";
echo json_encode(in_range(1)), " ", json_encode(in_range(3)), "\n";
$a = [10, 20];
$a[4] = 50;
$a[] = 60;
echo json_encode($a), " ", $a[0] + $a[4], "\n";
foreach ($a as $k => $v) { echo $k, "=", $v, " "; }
echo "\n";
"#);
    assert_eq!(
        out,
        "{\"0\":1,\"1\":2,\"5\":3}\n{\"0\":1,\"3\":6} 2\n{\"0\":1,\"1\":2,\"-1\":9}\n{\"0\":\"s\",\"3\":6}\n[1,7,3] [1,2,3,7]\n{\"0\":10,\"1\":20,\"4\":50,\"5\":60} 60\n0=10 1=20 4=50 5=60 \n"
    );
}

/// Skipping writes inside loops and branches promote the local for the code after them.
#[test]
fn test_nonempty_local_skipping_key_in_loops_and_branches() {
    let out = compile_and_run(r#"<?php
function loop_ids(array $ids): array {
    $rows = [0];
    foreach ($ids as $id) { $rows[$id] = $id * 2; }
    return $rows;
}
function branch(int $k): array {
    $a = [1, 2];
    if ($k > 0) { $a[$k] = 5; }
    $a[] = 9;
    return $a;
}
function counter_from(int $start): array {
    $a = [7];
    for ($i = $start; $i < $start + 2; $i++) { $a[$i] = $i; }
    return $a;
}
function int_keys(int $n): array {
    $a = [9];
    for ($i = 0; $i < $n; $i++) { $k = intdiv($i, 1) * 3 + 2; $a[$k] = $k; }
    return $a;
}
function dense(int $n): array {
    $a = [5];
    for ($i = 0; $i < $n; $i++) { $a[$i] = $i * 10; }
    return $a;
}
echo json_encode(loop_ids([3, 7])), "\n";
echo json_encode(branch(4)), " ", json_encode(branch(0)), "\n";
echo json_encode(counter_from(5)), " ", json_encode(counter_from(0)), "\n";
echo json_encode(dense(3)), " ", json_encode(int_keys(3)), "\n";
"#);
    assert_eq!(
        out,
        "{\"0\":0,\"3\":6,\"7\":14}\n{\"0\":1,\"1\":2,\"4\":5,\"5\":9} [1,2,9]\n{\"0\":7,\"5\":5,\"6\":6} [0,1]\n[0,10,20] {\"0\":9,\"2\":2,\"5\":5,\"8\":8}\n"
    );
}

/// A `??=` insert past the end of a non-empty packed local creates the key like PHP.
#[test]
fn test_nonempty_local_skipping_key_null_coalesce_insert() {
    let out = compile_and_run(r#"<?php
function upd(int $k): array {
    $a = [1, 2];
    $a[$k] ??= 4;
    return $a;
}
echo json_encode(upd(6)), " ", json_encode(upd(1)), "\n";
"#);
    assert_eq!(out, "{\"0\":1,\"1\":2,\"6\":4} [1,2]\n");
}

/// Count-bounded sorting loops keep producing PHP's results on their proven packed writes.
#[test]
fn test_count_bounded_loop_writes_sort_correctly() {
    let out = compile_and_run(r#"<?php
function bubble($arr) {
    $n = count($arr);
    for ($i = 0; $i < $n - 1; $i++) {
        for ($j = 0; $j < $n - $i - 1; $j++) {
            if ($arr[$j] > $arr[$j + 1]) { $tmp = $arr[$j]; $arr[$j] = $arr[$j + 1]; $arr[$j + 1] = $tmp; }
        }
    }
    return $arr;
}
function select($a) {
    $n = count($a);
    for ($i = 0; $i < $n; $i++) {
        for ($j = $i + 1; $j < $n; $j++) {
            if ($a[$j] < $a[$i]) { $t = $a[$i]; $a[$i] = $a[$j]; $a[$j] = $t; }
        }
    }
    return $a;
}
function shift_right($a) {
    for ($i = 0; $i < count($a) - 1; $i++) { $a[$i + 1] = $a[$i]; }
    return $a;
}
echo json_encode(bubble([5, 3, 8, 1])), json_encode(select([4, 2, 9, 0])), json_encode(shift_right([7, 1, 2])), "\n";
"#);
    assert_eq!(out, "[1,3,5,8][0,2,4,9][7,7,7]\n");
}

/// Counter-indexed sorts keep typed slots and still sort exactly as PHP does.
#[test]
fn test_promotable_counter_sort_matches_php() {
    let out = compile_and_run(r#"<?php
function insertion(int $n) {
    $arr = [];
    $seed = 7;
    for ($i = 0; $i < $n; $i++) { $seed = ($seed * 31 + 5) % 1000; $arr[] = $seed; }
    for ($i = 1; $i < $n; $i++) {
        $key = $arr[$i];
        $j = $i - 1;
        while ($j >= 0 && $arr[$j] > $key) { $arr[$j + 1] = $arr[$j]; $j--; }
        $arr[$j + 1] = $key;
    }
    return $arr;
}
echo json_encode(insertion(8)), "\n";
"#);
    assert_eq!(out, "[47,222,327,462,502,567,582,887]\n");
}

/// A boxed key that skips a slot promotes a typed local at run time; every later operation sees
/// PHP's hash: reads, `isset`, appends, `count`, `foreach`, whole-value consumers and returns.
#[test]
fn test_promotable_local_promotes_on_a_skipping_boxed_key() {
    let out = compile_and_run(r#"<?php
function churn(int $n) {
    $a = [10, 20, 30];
    for ($i = 0; $i < 3; $i++) { $j = $i + 1; $a[$j] = $a[$i]; }
    $k = $n + 5;
    $a[$k] = 99;
    $a[] = 7;
    $e = isset($a[6]) ? "y" : "n";
    $sum = 0;
    foreach ($a as $v) { $sum += $v; }
    return [json_encode($a), count($a), $a[6], $e, $sum, implode(",", $a)];
}
function negative(int $n) {
    $a = [5, 6];
    $a[$n - 2] = 7;
    return $a;
}
echo implode(" | ", churn(1)), "\n";
echo json_encode(negative(1)), "\n";
"#);
    assert_eq!(
        out,
        "{\"0\":10,\"1\":10,\"2\":10,\"3\":10,\"6\":99,\"7\":7} | 6 | 99 | y | 146 | 10,10,10,10,99,7\n{\"0\":5,\"1\":6,\"-1\":7}\n"
    );
}

/// A missed boxed-key read written back into a promotable local stores PHP's `null`, not `0`.
#[test]
fn test_promotable_local_keeps_null_from_a_missed_read() {
    let out = compile_and_run(r#"<?php
function holes(int $n) {
    $a = [1, 2, 3];
    $j = $n + 9;
    $a[$n] = @$a[$j];
    return [json_encode($a), isset($a[1]) ? "set" : "unset", count($a), $a[1] === null ? "null" : "int"];
}
echo implode(" ", holes(1)), "\n";
"#);
    assert_eq!(out, "[1,null,3] unset 3 null\n");
}

/// Promotable writes, promotions, null entries and whole-value views release every owner.
#[test]
fn test_promotable_local_operations_do_not_leak() {
    let out = compile_and_run_with_heap_debug(r#"<?php
function churn(int $n) {
    $arr = [5, 3, 8, 1, 9, 2];
    for ($i = 1; $i < count($arr); $i++) {
        $key = $arr[$i];
        $j = $i - 1;
        while ($j >= 0 && $arr[$j] > $key) { $arr[$j + 1] = $arr[$j]; $j--; }
        $arr[$j + 1] = $key;
    }
    $sorted = json_encode($arr);
    $k = $n + 20;
    $arr[$k] = @$arr[$k + 1];
    $arr[] = 4;
    return $sorted . " " . json_encode($arr) . " " . count($arr);
}
for ($r = 0; $r < 3; $r++) { echo churn($r), "\n"; }
"#);
    assert!(out.success, "{}", out.stderr);
    assert_eq!(
        out.stdout,
        "[1,2,3,5,8,9] {\"0\":1,\"1\":2,\"2\":3,\"3\":5,\"4\":8,\"5\":9,\"20\":null,\"21\":4} 8\n[1,2,3,5,8,9] {\"0\":1,\"1\":2,\"2\":3,\"3\":5,\"4\":8,\"5\":9,\"21\":null,\"22\":4} 8\n[1,2,3,5,8,9] {\"0\":1,\"1\":2,\"2\":3,\"3\":5,\"4\":8,\"5\":9,\"22\":null,\"23\":4} 8\n",
        "{}",
        out.stderr
    );
    assert!(out.stderr.contains("HEAP DEBUG: leak summary: clean"), "{}", out.stderr);
}

/// A promotable `array<int>` crosses a declared `array<int>` parameter and return raw: the callee
/// keeps typed slots, promotes on its own skipping write, and callers see PHP's arrays.
#[test]
fn test_promotable_array_crosses_declared_array_int_boundaries() {
    let out = compile_and_run_with_heap_debug(r#"<?php
function shift_into(array<int> $p, int $k): array<int> {
    $n = count($p);
    for ($i = 0; $i < $n; $i++) { $j = $i + $k; $p[$j] = $p[$i]; }
    return $p;
}
function total(array<int> $p): int {
    $s = 0;
    foreach ($p as $v) { $s += $v; }
    return $s;
}
function run(int $n) {
    $a = [1, 2, 3];
    for ($i = 0; $i < 2; $i++) { $j = $i + 1; $a[$j] = $a[$i]; }
    $b = shift_into($a, $n);
    $c = shift_into($b, 5);
    return json_encode($a) . " " . json_encode($b) . " " . json_encode($c) . " " . count($c)
        . " " . total($c);
}
echo run(1), "\n";
"#);
    assert!(out.success, "{}", out.stderr);
    assert_eq!(
        out.stdout,
        "[1,1,1] [1,1,1,1] {\"0\":1,\"1\":1,\"2\":1,\"3\":1,\"5\":1,\"6\":1,\"7\":1,\"8\":1} 8 8\n",
        "{}",
        out.stderr
    );
    assert!(out.stderr.contains("HEAP DEBUG: leak summary: clean"), "{}", out.stderr);
}

/// A promotable `array<int>` passed to an undeclared parameter reaches a typed clone raw, while
/// plain and dynamic callers keep the original; PHP sees one function under its own name.
#[test]
fn test_promotable_array_reaches_typed_clone_of_untyped_param() {
    let out = compile_and_run_with_heap_debug(r#"<?php
function shift_into($p, $k) {
    $n = count($p);
    for ($i = 0; $i < $n; $i++) { $j = $i + $k; $p[$j] = $p[$i]; }
    return $p;
}
function where($p) {
    $p[] = 1;
    return __FUNCTION__ . ":" . debug_backtrace()[0]["function"] . ":" . count($p);
}
function run(int $n) {
    $a = [1, 2, 3];
    for ($i = 0; $i < 2; $i++) { $j = $i + 1; $a[$j] = $a[$i]; }
    $b = shift_into($a, $n);
    $c = shift_into([7, 8], 3);
    $f = 'shift_into';
    $d = $f($b, 5);
    return json_encode($a) . " " . json_encode($b) . " " . json_encode($c) . " "
        . json_encode($d) . " " . where($a);
}
echo run(1), "\n";
var_dump(function_exists('shift_into'), function_exists('shift_into#__promotable_0'));
"#);
    assert!(out.success, "{}", out.stderr);
    assert_eq!(
        out.stdout,
        "[1,1,1] [1,1,1,1] {\"0\":7,\"1\":8,\"3\":7,\"4\":8} \
         {\"0\":1,\"1\":1,\"2\":1,\"3\":1,\"5\":1,\"6\":1,\"7\":1,\"8\":1} where:where:4\n\
         bool(true)\nbool(false)\n",
        "{}",
        out.stderr
    );
    assert!(out.stderr.contains("HEAP DEBUG: leak summary: clean"), "{}", out.stderr);
}

/// A boxed untyped parameter that a skipping key promoted to a hash is returned as that hash,
/// not walked as packed slots (which crashed).
#[test]
fn test_untyped_param_skipping_key_returns_its_hash() {
    let out = compile_and_run_with_heap_debug(r#"<?php
function f($p) { $p[3] = 7; return $p; }
function g($p, $k) {
    $n = count($p);
    for ($i = 0; $i < $n; $i++) { $j = $i + $k; $p[$j] = $p[$i]; }
    return $p;
}
echo json_encode(f([7, 8])), " ", json_encode(g([7, 8], 3)), "\n";
"#);
    assert!(out.success, "{}", out.stderr);
    assert_eq!(
        out.stdout,
        "{\"0\":7,\"1\":8,\"3\":7} {\"0\":7,\"1\":8,\"3\":7,\"4\":8}\n",
        "{}",
        out.stderr
    );
    assert!(out.stderr.contains("HEAP DEBUG: leak summary: clean"), "{}", out.stderr);
}
