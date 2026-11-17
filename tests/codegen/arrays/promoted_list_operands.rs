//! Purpose:
//! PHP-differential regression tests (merge, pad, `+`, shuffle, by-reference foreach, second
//! operands, spread) for a static `array<T>` whose storage a sparse integer write
//! promoted to a hash at run time (`$a = [1, 2]; $a[7] = 3;`).
//!
//! Called from:
//! - `cargo test` through Rust's test harness.
//!
//! Key details:
//! - Every expected string is verbatim `php -n` output for the same program.
//! - The local keeps its packed static type, so every builtin, `foreach` and spread over it has to
//!   dispatch on the runtime storage kind instead of reading the hash as slots.

use crate::support::*;

/// Group 1 of builtins over a runtime-promoted list; expected output is php's.
#[test]
fn test_promoted_operands_group_1() {
    let out = compile_and_run(
        r####"<?php
function mk() { $a = [1, 2]; $a[7] = 3; return $a; }
(function () { $a = mk(); var_dump(array_merge($a,[9])); })();
(function () { $a = mk(); var_dump(array_merge([9],$a)); })();
(function () { $a = mk(); var_dump(array_merge($a,$a)); })();
(function () { $a = mk(); var_dump(array_pad($a,5,0)); })();
(function () { $a = mk(); var_dump($a+[5=>1]); })();
(function () { $a = mk(); var_dump([5=>1]+$a); })();
(function () { $a = mk(); $b=$a; shuffle($b); var_dump(count($b), array_sum($b), array_is_list($b)); })();
"####,
    );
    assert_eq!(out, r####"array(4) {
  [0]=>
  int(1)
  [1]=>
  int(2)
  [2]=>
  int(3)
  [3]=>
  int(9)
}
array(4) {
  [0]=>
  int(9)
  [1]=>
  int(1)
  [2]=>
  int(2)
  [3]=>
  int(3)
}
array(6) {
  [0]=>
  int(1)
  [1]=>
  int(2)
  [2]=>
  int(3)
  [3]=>
  int(1)
  [4]=>
  int(2)
  [5]=>
  int(3)
}
array(5) {
  [0]=>
  int(1)
  [1]=>
  int(2)
  [2]=>
  int(3)
  [3]=>
  int(0)
  [4]=>
  int(0)
}
array(4) {
  [0]=>
  int(1)
  [1]=>
  int(2)
  [7]=>
  int(3)
  [5]=>
  int(1)
}
array(4) {
  [5]=>
  int(1)
  [0]=>
  int(1)
  [1]=>
  int(2)
  [7]=>
  int(3)
}
int(3)
int(6)
bool(true)
"####);
}

/// Group 2 of builtins over a runtime-promoted list; expected output is php's.
#[test]
fn test_promoted_operands_group_2() {
    let out = compile_and_run(
        r####"<?php
function mk() { $a = [1, 2]; $a[7] = 3; return $a; }
(function () { $a = mk(); $b=$a; foreach($b as &$v) { $v *= 2; } unset($v); var_dump($b); })();
(function () { $a = mk(); $b=$a; foreach($b as $k=>&$v) { $v += $k; } unset($v); var_dump($b); })();
(function () { $a = mk(); $c=[3,4]; $c[9]=5; var_dump(array_diff($c,$a), array_intersect($c,$a), array_combine($a,$c), array_diff_key($c,$a), array_intersect_key($c,$a)); })();
(function () { $a = mk(); $c=[3,4]; $c[9]=5; var_dump(array_merge($a,$c)); })();
(function () { $a = mk(); var_dump([...$a, ...$a]); })();
"####,
    );
    assert_eq!(out, r####"array(3) {
  [0]=>
  int(2)
  [1]=>
  int(4)
  [7]=>
  int(6)
}
array(3) {
  [0]=>
  int(1)
  [1]=>
  int(3)
  [7]=>
  int(10)
}
array(2) {
  [1]=>
  int(4)
  [9]=>
  int(5)
}
array(1) {
  [0]=>
  int(3)
}
array(3) {
  [1]=>
  int(3)
  [2]=>
  int(4)
  [3]=>
  int(5)
}
array(1) {
  [9]=>
  int(5)
}
array(2) {
  [0]=>
  int(3)
  [1]=>
  int(4)
}
array(6) {
  [0]=>
  int(1)
  [1]=>
  int(2)
  [2]=>
  int(3)
  [3]=>
  int(3)
  [4]=>
  int(4)
  [5]=>
  int(5)
}
array(6) {
  [0]=>
  int(1)
  [1]=>
  int(2)
  [2]=>
  int(3)
  [3]=>
  int(1)
  [4]=>
  int(2)
  [5]=>
  int(3)
}
"####);
}


/// array_map over a runtime-promoted list: the null-callback identity, a callback, and several arrays.
#[test]
fn test_promoted_operands_array_map() {
    let out = compile_and_run(
        r####"<?php
function mk() { $a = [1, 2]; $a[7] = 3; return $a; }
(function () { $a = mk(); var_dump(array_map(null, $a)); })();
(function () { $a = mk(); var_dump(array_map(fn($x) => $x * 2, $a)); })();
(function () { $a = mk(); $b = mk(); var_dump(array_map(null, $a, $b)); })();
(function () { $a = mk(); var_dump(array_map(fn($x, $y) => $x + $y, $a, [10, 20, 30])); })();
"####,
    );
    assert_eq!(out, r####"array(3) {
  [0]=>
  int(1)
  [1]=>
  int(2)
  [7]=>
  int(3)
}
array(3) {
  [0]=>
  int(2)
  [1]=>
  int(4)
  [7]=>
  int(6)
}
array(3) {
  [0]=>
  array(2) {
    [0]=>
    int(1)
    [1]=>
    int(1)
  }
  [1]=>
  array(2) {
    [0]=>
    int(2)
    [1]=>
    int(2)
  }
  [2]=>
  array(2) {
    [0]=>
    int(3)
    [1]=>
    int(3)
  }
}
array(3) {
  [0]=>
  int(11)
  [1]=>
  int(22)
  [2]=>
  int(33)
}
"####);
}

/// Three or more promoted operands are each read through their runtime storage kind.
#[test]
fn test_promoted_operands_third_operand() {
    let out = compile_and_run(
        r####"<?php
function mk() { $a = [1, 2]; $a[7] = 3; return $a; }
(function () { $a = mk(); $b = mk(); $c = mk(); echo json_encode(array_merge($a, $b, $c)), "\n"; })();
(function () { $a = mk(); $b = mk(); $c = mk(); echo json_encode(array_merge($a, [9], $b, $c)), "\n"; })();
(function () { $a = mk(); $b = mk(); $c = mk(); echo json_encode(array_merge($c, $b, $a, [4])), "\n"; })();
(function () { $a = mk(); $b = [3]; $c = mk(); echo json_encode(array_diff($a, $b)), json_encode(array_intersect($a, $c)), "\n"; })();
"####,
    );
    assert_eq!(out, r####"[1,2,3,1,2,3,1,2,3]
[1,2,3,9,1,2,3,1,2,3]
[1,2,3,1,2,3,1,2,3,4]
[1,2]{"0":1,"1":2,"7":3}
"####);
}

/// Whole-array consumers (json, serialize, var_export, destructuring, comparison, typed parameters) over a promoted list.
#[test]
fn test_promoted_operands_whole_array_consumers() {
    let out = compile_and_run(
        r####"<?php
function mk() { $a = [1, 2]; $a[7] = 3; return $a; }
function f(array $x) { return $x; }
function g(iterable $x) { foreach ($x as $k => $v) echo "$k=$v,"; echo "\n"; return $x; }
/** @param array<int> $x @return array<int> */
function h($x) { return $x; }
$a = mk();
echo json_encode(["n" => $a, "m" => [$a]]), "\n";
echo serialize($a), "\n";
var_export($a); echo "\n";
array_walk($a, function (&$v, $k) { $v = $v * 10; });
echo json_encode($a), "\n";
$a = mk();
[$x, $y] = $a; var_dump($x, $y);
[0 => $p, 7 => $q] = $a; var_dump($p, $q);
list($m, $n) = $a; var_dump($m, $n);
var_dump(array_key_exists(5, $a), isset($a[5]), array_key_exists(7, $a), isset($a[7]), isset($a[2]));
var_dump($a == [0 => 1, 1 => 2, 7 => 3]);
var_dump($a === [0 => 1, 1 => 2, 7 => 3]);
var_dump($a === [1 => 2, 0 => 1, 7 => 3]);
var_dump($a == [1, 2, 3]);
var_dump(f($a)); echo json_encode(g($a)), "\n"; echo json_encode(h($a)), "\n";
var_dump(count(f($a)), f($a)[7]);
"####,
    );
    assert_eq!(out, r####"{"n":{"0":1,"1":2,"7":3},"m":[{"0":1,"1":2,"7":3}]}
a:3:{i:0;i:1;i:1;i:2;i:7;i:3;}
array (
  0 => 1,
  1 => 2,
  7 => 3,
)
{"0":10,"1":20,"7":30}
int(1)
int(2)
int(1)
int(3)
int(1)
int(2)
bool(false)
bool(false)
bool(true)
bool(true)
bool(false)
bool(true)
bool(true)
bool(false)
bool(false)
array(3) {
  [0]=>
  int(1)
  [1]=>
  int(2)
  [7]=>
  int(3)
}
0=1,1=2,7=3,
{"0":1,"1":2,"7":3}
{"0":1,"1":2,"7":3}
int(3)
int(3)
"####);
}

/// Builds the user assembly of an `array_merge` over `count` statically-packed operands.
fn merge_asm_len(count: usize) -> usize {
    let dir = make_cli_test_dir("elephc_promoted_merge_asm");
    let mut source = String::from("<?php\nfunction mk(int $n) { $a = [$n, 2]; if ($n > 99) { $a[7] = 3; } return $a; }\n");
    let names: Vec<String> = (0..count).map(|index| format!("$a{index}")).collect();
    for (index, name) in names.iter().enumerate() {
        source.push_str(&format!("{name} = mk({index});\n"));
    }
    source.push_str(&format!("echo count(array_merge({}));\n", names.join(", ")));
    let (user_asm, _runtime_asm, _) =
        compile_source_to_asm_with_options(&source, &dir, 8_388_608, false, false);
    let _ = fs::remove_dir_all(&dir);
    user_asm.len()
}

/// The storage-kind normalization of `array_merge` operands grows linearly, not by doubling.
#[test]
fn test_promoted_operand_dispatch_grows_linearly_with_operand_count() {
    let (two, four, six) = (merge_asm_len(2), merge_asm_len(4), merge_asm_len(6));
    let first_step = four - two;
    let second_step = six - four;
    assert!(
        second_step < first_step * 3 / 2,
        "array_merge code size doubles per operand: 2 -> {two}, 4 -> {four}, 6 -> {six}"
    );
}
