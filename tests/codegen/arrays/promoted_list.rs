//! Purpose:
//! PHP-differential regression tests for a static `array<T>` whose storage a sparse integer write
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
fn test_promoted_list_group_1() {
    let out = compile_and_run(
        r####"<?php
function mk() { $a = [1, 2]; $a[7] = 3; return $a; }
(function () { $a = mk(); var_dump(in_array(3,$a)); })();
(function () { $a = mk(); var_dump(array_search(3,$a)); })();
(function () { $a = mk(); var_dump(array_key_last($a), array_key_first($a)); })();
(function () { $a = mk(); echo implode(",",$a),"\n"; })();
(function () { $a = mk(); var_dump(max($a), min($a), array_sum($a), array_product($a), count($a)); })();
(function () { $a = mk(); var_dump(array_flip($a)); })();
(function () { $a = mk(); var_dump(array_values($a)); })();
(function () { $a = mk(); var_dump(array_keys($a)); })();
"####,
    );
    assert_eq!(out, r####"bool(true)
int(7)
int(7)
int(0)
1,2,3
int(3)
int(1)
int(6)
int(6)
int(3)
array(3) {
  [1]=>
  int(0)
  [2]=>
  int(1)
  [3]=>
  int(7)
}
array(3) {
  [0]=>
  int(1)
  [1]=>
  int(2)
  [2]=>
  int(3)
}
array(3) {
  [0]=>
  int(0)
  [1]=>
  int(1)
  [2]=>
  int(7)
}
"####);
}

/// Group 2 of builtins over a runtime-promoted list; expected output is php's.
#[test]
fn test_promoted_list_group_2() {
    let out = compile_and_run(
        r####"<?php
function mk() { $a = [1, 2]; $a[7] = 3; return $a; }
(function () { $a = mk(); var_dump(array_keys($a,3)); })();
(function () { $a = mk(); var_dump(array_unique($a)); })();
(function () { $a = mk(); var_dump(array_count_values($a)); })();
(function () { $a = mk(); var_dump(array_combine([1,2,3],$a)); })();
(function () { $a = mk(); var_dump(array_combine($a,[1,2,3])); })();
(function () { $a = mk(); var_dump(array_diff($a,[1])); })();
(function () { $a = mk(); var_dump(array_diff_key($a,[0=>1])); })();
(function () { $a = mk(); var_dump(array_intersect($a,[2,3])); })();
"####,
    );
    assert_eq!(out, r####"array(1) {
  [0]=>
  int(7)
}
array(3) {
  [0]=>
  int(1)
  [1]=>
  int(2)
  [7]=>
  int(3)
}
array(3) {
  [1]=>
  int(1)
  [2]=>
  int(1)
  [3]=>
  int(1)
}
array(3) {
  [1]=>
  int(1)
  [2]=>
  int(2)
  [3]=>
  int(3)
}
array(3) {
  [1]=>
  int(1)
  [2]=>
  int(2)
  [3]=>
  int(3)
}
array(2) {
  [1]=>
  int(2)
  [7]=>
  int(3)
}
array(2) {
  [1]=>
  int(2)
  [7]=>
  int(3)
}
array(2) {
  [1]=>
  int(2)
  [7]=>
  int(3)
}
"####);
}

/// Group 3 of builtins over a runtime-promoted list; expected output is php's.
#[test]
fn test_promoted_list_group_3() {
    let out = compile_and_run(
        r####"<?php
function mk() { $a = [1, 2]; $a[7] = 3; return $a; }
(function () { $a = mk(); var_dump(array_intersect_key($a,[7=>1])); })();
(function () { $a = mk(); foreach($a as $k=>$v) echo "$k=$v,"; echo "\n"; })();
(function () { $a = mk(); foreach($a as $v) echo "$v,"; echo "\n"; })();
(function () { $a = mk(); echo json_encode($a),"\n"; })();
(function () { $a = mk(); var_dump($a); })();
(function () { $a = mk(); print_r($a); var_export($a); })();
(function () { $a = mk(); var_dump(array_reverse($a), array_reverse($a,true)); })();
(function () { $a = mk(); var_dump(array_map(fn($x)=>$x*2,$a)); })();
"####,
    );
    assert_eq!(out, r####"array(1) {
  [7]=>
  int(3)
}
0=1,1=2,7=3,
1,2,3,
{"0":1,"1":2,"7":3}
array(3) {
  [0]=>
  int(1)
  [1]=>
  int(2)
  [7]=>
  int(3)
}
Array
(
    [0] => 1
    [1] => 2
    [7] => 3
)
array (
  0 => 1,
  1 => 2,
  7 => 3,
)array(3) {
  [0]=>
  int(3)
  [1]=>
  int(2)
  [2]=>
  int(1)
}
array(3) {
  [7]=>
  int(3)
  [1]=>
  int(2)
  [0]=>
  int(1)
}
array(3) {
  [0]=>
  int(2)
  [1]=>
  int(4)
  [7]=>
  int(6)
}
"####);
}

/// Group 4 of builtins over a runtime-promoted list; expected output is php's.
#[test]
fn test_promoted_list_group_4() {
    let out = compile_and_run(
        r####"<?php
function mk() { $a = [1, 2]; $a[7] = 3; return $a; }
(function () { $a = mk(); var_dump(array_filter($a,fn($x)=>$x>1)); })();
(function () { $a = mk(); $b=$a; sort($b); var_dump($b); rsort($b); var_dump($b); })();
(function () { $a = mk(); $b=$a; ksort($b); var_dump($b); krsort($b); var_dump($b); asort($b); var_dump($b); arsort($b); var_dump($b); })();
(function () { $a = mk(); $b=$a; usort($b, fn($x,$y)=>$y<=>$x); var_dump($b); })();
(function () { $a = mk(); var_dump(reset($a), end($a)); })();
(function () { $a = mk(); var_dump(isset($a[7]), isset($a[3]), $a[7] ?? 'd', array_key_exists(7,$a)); })();
(function () { $a = mk(); $b=$a; $r=array_splice($b,1,1); var_dump($r,$b); })();
(function () { $a = mk(); $b=$a; $r=array_splice($b,1,1,[8,9]); var_dump($r,$b); })();
"####,
    );
    assert_eq!(out, r####"array(2) {
  [1]=>
  int(2)
  [7]=>
  int(3)
}
array(3) {
  [0]=>
  int(1)
  [1]=>
  int(2)
  [2]=>
  int(3)
}
array(3) {
  [0]=>
  int(3)
  [1]=>
  int(2)
  [2]=>
  int(1)
}
array(3) {
  [0]=>
  int(1)
  [1]=>
  int(2)
  [7]=>
  int(3)
}
array(3) {
  [7]=>
  int(3)
  [1]=>
  int(2)
  [0]=>
  int(1)
}
array(3) {
  [0]=>
  int(1)
  [1]=>
  int(2)
  [7]=>
  int(3)
}
array(3) {
  [7]=>
  int(3)
  [1]=>
  int(2)
  [0]=>
  int(1)
}
array(3) {
  [0]=>
  int(3)
  [1]=>
  int(2)
  [2]=>
  int(1)
}
int(1)
int(3)
bool(true)
bool(false)
int(3)
bool(true)
array(1) {
  [0]=>
  int(2)
}
array(2) {
  [0]=>
  int(1)
  [1]=>
  int(3)
}
array(1) {
  [0]=>
  int(2)
}
array(4) {
  [0]=>
  int(1)
  [1]=>
  int(8)
  [2]=>
  int(9)
  [3]=>
  int(3)
}
"####);
}

/// Group 5 of builtins over a runtime-promoted list; expected output is php's.
#[test]
fn test_promoted_list_group_5() {
    let out = compile_and_run(
        r####"<?php
function mk() { $a = [1, 2]; $a[7] = 3; return $a; }
(function () { $a = mk(); $b=$a; array_splice($b,1,0,[8]); var_dump($b); })();
(function () { $a = mk(); var_dump(array_slice($a,1,2), array_slice($a,1,2,true)); })();
(function () { $a = mk(); $b=$a; array_walk($b,function(&$v,$k){$v=$v+$k;}); var_dump($b); })();
(function () { $a = mk(); var_dump(array_reduce($a,fn($c,$x)=>$c+$x,0)); })();
(function () { $a = mk(); var_dump(array_chunk($a,2), array_chunk($a,2,true)); })();
(function () { $a = mk(); $b=$a; unset($b[1]); var_dump($b); $b[]=4; var_dump($b); })();
(function () { $a = mk(); var_dump($a==[0=>1,1=>2,7=>3], $a===[0=>1,1=>2,7=>3]); })();
(function () { $a = mk(); echo "x{$a[7]}\n"; echo serialize($a),"\n"; })();
"####,
    );
    assert_eq!(out, r####"array(4) {
  [0]=>
  int(1)
  [1]=>
  int(8)
  [2]=>
  int(2)
  [3]=>
  int(3)
}
array(2) {
  [0]=>
  int(2)
  [1]=>
  int(3)
}
array(2) {
  [1]=>
  int(2)
  [7]=>
  int(3)
}
array(3) {
  [0]=>
  int(1)
  [1]=>
  int(3)
  [7]=>
  int(10)
}
int(6)
array(2) {
  [0]=>
  array(2) {
    [0]=>
    int(1)
    [1]=>
    int(2)
  }
  [1]=>
  array(1) {
    [0]=>
    int(3)
  }
}
array(2) {
  [0]=>
  array(2) {
    [0]=>
    int(1)
    [1]=>
    int(2)
  }
  [1]=>
  array(1) {
    [7]=>
    int(3)
  }
}
array(2) {
  [0]=>
  int(1)
  [7]=>
  int(3)
}
array(3) {
  [0]=>
  int(1)
  [7]=>
  int(3)
  [8]=>
  int(4)
}
bool(true)
bool(true)
x3
a:3:{i:0;i:1;i:1;i:2;i:7;i:3;}
"####);
}

/// Group 6 of builtins over a runtime-promoted list; expected output is php's.
#[test]
fn test_promoted_list_group_6() {
    let out = compile_and_run(
        r####"<?php
function mk() { $a = [1, 2]; $a[7] = 3; return $a; }
(function () { $a = mk(); var_dump(array_is_list($a)); })();
(function () { $a = mk(); var_dump([...$a]); })();
(function () { $a = mk(); [$x,$y]=$a; var_dump($x,$y); })();
(function () { $a = mk(); $b=$a; $b[]=4; var_dump($b); array_push($b,5,6); var_dump($b); })();
(function () { $a = mk(); $b=$a; var_dump(array_pop($b), array_shift($b), $b); })();
(function () { $a = mk(); $b=$a; array_unshift($b,0); var_dump($b); })();
(function () { $a = mk(); var_dump(array_column([$a,$a],7)); })();
(function () { $a = mk(); var_dump(array_fill(0,2,$a)); })();
"####,
    );
    assert_eq!(out, r####"bool(false)
array(3) {
  [0]=>
  int(1)
  [1]=>
  int(2)
  [2]=>
  int(3)
}
int(1)
int(2)
array(4) {
  [0]=>
  int(1)
  [1]=>
  int(2)
  [7]=>
  int(3)
  [8]=>
  int(4)
}
array(6) {
  [0]=>
  int(1)
  [1]=>
  int(2)
  [7]=>
  int(3)
  [8]=>
  int(4)
  [9]=>
  int(5)
  [10]=>
  int(6)
}
int(3)
int(1)
array(1) {
  [0]=>
  int(2)
}
array(4) {
  [0]=>
  int(0)
  [1]=>
  int(1)
  [2]=>
  int(2)
  [3]=>
  int(3)
}
array(2) {
  [0]=>
  int(3)
  [1]=>
  int(3)
}
array(2) {
  [0]=>
  array(3) {
    [0]=>
    int(1)
    [1]=>
    int(2)
    [7]=>
    int(3)
  }
  [1]=>
  array(3) {
    [0]=>
    int(1)
    [1]=>
    int(2)
    [7]=>
    int(3)
  }
}
"####);
}

/// Group 7 of builtins over a runtime-promoted list; expected output is php's.
#[test]
fn test_promoted_list_group_7() {
    let out = compile_and_run(
        r####"<?php
function mk() { $a = [1, 2]; $a[7] = 3; return $a; }
(function () { $a = mk(); var_dump(array_rand($a,3)); })();
(function () { $a = mk(); var_dump(array_walk($a,function($v,$k){echo "$k:$v ";})); })();
(function () { $a = mk(); var_dump(array_map(fn($x,$y)=>$x+$y,$a,$a)); })();
(function () { $a = mk(); var_dump(array_key_exists(1,$a), array_flip(array_flip($a))); })();
(function () { $a = mk(); var_dump(current($a), key($a), next($a), key($a), next($a), key($a), next($a), key($a)); })();
(function () { $a = mk(); var_dump(iterator_to_array(new ArrayIterator($a))); })();
(function () { $a = mk(); var_dump(array_merge_recursive($a,$a), array_replace($a,[1=>5]), array_diff_assoc($a,[1,5]), array_intersect_assoc($a,[1,2])); })();
(function () { $a = mk(); var_dump(array_find($a, fn($x)=>$x>1), array_any($a, fn($x)=>$x>2), array_all($a, fn($x)=>$x>0)); })();
"####,
    );
    assert_eq!(out, r####"array(3) {
  [0]=>
  int(0)
  [1]=>
  int(1)
  [2]=>
  int(7)
}
0:1 1:2 7:3 bool(true)
array(3) {
  [0]=>
  int(2)
  [1]=>
  int(4)
  [2]=>
  int(6)
}
bool(true)
array(3) {
  [0]=>
  int(1)
  [1]=>
  int(2)
  [7]=>
  int(3)
}
int(1)
int(0)
int(2)
int(1)
int(3)
int(7)
bool(false)
NULL
array(3) {
  [0]=>
  int(1)
  [1]=>
  int(2)
  [7]=>
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
array(3) {
  [0]=>
  int(1)
  [1]=>
  int(5)
  [7]=>
  int(3)
}
array(2) {
  [1]=>
  int(2)
  [7]=>
  int(3)
}
array(2) {
  [0]=>
  int(1)
  [1]=>
  int(2)
}
int(2)
bool(true)
bool(true)
"####);
}

/// Group 8 of builtins over a runtime-promoted list; expected output is php's.
#[test]
fn test_promoted_list_group_8() {
    let out = compile_and_run(
        r####"<?php
function mk() { $a = [1, 2]; $a[7] = 3; return $a; }
(function () { $a = mk(); $s=0; for($i=0;$i<count($a);$i++){ $s += $a[$i] ?? 100; } var_dump($s); })();
(function () { $a = mk(); var_dump(natsort($a), $a); })();
(function () { $a = mk(); echo implode(",", array_keys($a)), "|", implode(",", array_values($a)), "\n"; })();
(function () { $a = mk(); var_dump(iterator_count(new ArrayIterator($a)), array_flip(array_keys($a))); })();
(function () { $a = mk(); $g=(function() use($a){ foreach($a as $k=>$v) yield $k=>$v; })(); var_dump(iterator_to_array($g)); })();
"####,
    );
    assert_eq!(out, r####"int(103)
bool(true)
array(3) {
  [0]=>
  int(1)
  [1]=>
  int(2)
  [7]=>
  int(3)
}
0,1,7|1,2,3
int(3)
array(3) {
  [0]=>
  int(0)
  [1]=>
  int(1)
  [7]=>
  int(2)
}
array(3) {
  [0]=>
  int(1)
  [1]=>
  int(2)
  [7]=>
  int(3)
}
"####);
}

