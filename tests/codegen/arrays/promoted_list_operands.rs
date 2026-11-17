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

