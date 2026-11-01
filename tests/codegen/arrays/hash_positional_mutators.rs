//! Purpose:
//! PHP-differential regression tests for the positional by-reference array builtins
//! (`array_splice`, `array_shift`, `array_pop`, `array_unshift`, `sort`, `rsort`, `usort`) over
//! hash receivers and over an `array &$p` parameter that is handed a hash.
//!
//! Called from:
//! - `cargo test` through Rust's test harness.
//!
//! Key details:
//! - Every expected string is verbatim php 8.5 output for the same fixture.
//! - A declared `array &$p` parameter is not specialised to its caller's storage, so a hash
//!   reached the dense-list lowerings; the checker now boxes such a parameter when its body
//!   hands it to one of these builtins, and `array_splice` over a hash or boxed receiver is
//!   answered by an AST-built prelude walk (`src/array_splice_prelude.rs`).

use crate::support::*;

/// Verifies `array_splice()` over hash receivers matches php: positions count in insertion order,
/// integer keys renumber from zero, string keys stay, the replacement lands at the offset, and the
/// removed entries come back with the same key rule. Covers a plain local, a by-reference
/// `array &$p` parameter fed a hash and a list, a `mixed` parameter, and copy-on-write aliasing.
/// Expected output is php 8.5's. The native lowering refused a hash receiver at compile time
/// (`array_pop for PHP type AssocArray`) and dropped a boxed one silently; Laravel's
/// `ResolvesRouteDependencies::spliceIntoParameters` is the by-reference parameter case.
#[test]
fn test_array_splice_on_hashes_matches_php() {
    let out = compile_and_run(
        r#"<?php
function show($label, $removed, $array) {
    echo $label, ' ', json_encode($removed), ' ', json_encode($array), "\n";
}
function h() { return ['a' => 1, 'b' => 2, 5 => 'five', 'c' => 3, 9 => 'nine']; }

$x = h(); $r = array_splice($x, 1); show('off1', $r, $x);
$x = h(); $r = array_splice($x, -2); show('neg2', $r, $x);
$x = h(); $r = array_splice($x, 0, 0, ['new']); show('ins0', $r, $x);
$x = h(); $r = array_splice($x, 2, -1); show('lenneg', $r, $x);
$x = h(); $r = array_splice($x, 99, 1, ['tail']); show('past', $r, $x);
$x = h(); $r = array_splice($x, 1, 2, 'scalar'); show('scalar', $r, $x);
$x = h(); $r = array_splice($x, 1, 1, null); show('null', $r, $x);
$x = h(); $r = array_splice($x, 0, 5); show('all', $r, $x);
$x = h(); $r = array_splice($x, 3, 1, ['p' => 'q', 'r']); show('keyed-repl', $r, $x);
$x = [7 => 'a', 8 => 'b', 9 => 'c']; $r = array_splice($x, 1, 1); show('intkeys', $r, $x);

function bump(array &$p, $off, $len, $rep) { return array_splice($p, $off, $len, $rep); }
function caller(array $p) { $r = bump($p, 1, 1, ['X', 'Y']); show('byref', $r, $p); }
caller(['a' => 1, 'b' => 2, 'c' => 3]);
caller([10, 20, 30]);
caller([]);

function viaMixed(mixed $m) { $r = array_splice($m, 0, 1, ['Z']); show('mixed', $r, $m); }
viaMixed(['k' => 'v', 'w']);
viaMixed([1, 2, 3]);

$list = [1, 2, 3, 4];
$r = array_splice($list, 1, 2, ['a', 'b', 'c']); show('list', $r, $list);
$alias = $x = h(); array_splice($x, 0, 1); show('alias-orig', [], $alias); show('alias-new', [], $x);
"#,
    );
    assert_eq!(
        out,
        "off1 {\"b\":2,\"0\":\"five\",\"c\":3,\"1\":\"nine\"} {\"a\":1}\nneg2 {\"c\":3,\"0\":\"nine\"} {\"a\":1,\"b\":2,\"0\":\"five\"}\nins0 [] {\"0\":\"new\",\"a\":1,\"b\":2,\"1\":\"five\",\"c\":3,\"2\":\"nine\"}\nlenneg {\"0\":\"five\",\"c\":3} {\"a\":1,\"b\":2,\"0\":\"nine\"}\npast [] {\"a\":1,\"b\":2,\"0\":\"five\",\"c\":3,\"1\":\"nine\",\"2\":\"tail\"}\nscalar {\"b\":2,\"0\":\"five\"} {\"a\":1,\"0\":\"scalar\",\"c\":3,\"1\":\"nine\"}\nnull {\"b\":2} {\"a\":1,\"0\":\"five\",\"c\":3,\"1\":\"nine\"}\nall {\"a\":1,\"b\":2,\"0\":\"five\",\"c\":3,\"1\":\"nine\"} []\nkeyed-repl {\"c\":3} {\"a\":1,\"b\":2,\"0\":\"five\",\"1\":\"q\",\"2\":\"r\",\"3\":\"nine\"}\nintkeys [\"b\"] [\"a\",\"c\"]\nbyref {\"b\":2} {\"a\":1,\"0\":\"X\",\"1\":\"Y\",\"c\":3}\nbyref [20] [10,\"X\",\"Y\",30]\nbyref [] [\"X\",\"Y\"]\nmixed {\"k\":\"v\"} [\"Z\",\"w\"]\nmixed [1] [\"Z\",2,3]\nlist [2,3] [1,\"a\",\"b\",\"c\",4]\nalias-orig [] {\"a\":1,\"b\":2,\"5\":\"five\",\"c\":3,\"9\":\"nine\"}\nalias-new [] {\"b\":2,\"0\":\"five\",\"c\":3,\"1\":\"nine\"}\n"
    );
}

/// Verifies the positional by-reference array builtins run over an `array &$p` parameter that is
/// handed a hash. `array_shift`, `array_pop`, `array_unshift`, `sort`, `rsort` and `usort` read the
/// receiver as a dense list: over a hash they answered garbage or took a bus error, and through a
/// shared boxed cell `array_shift`/`array_pop` returned the right element but never updated the
/// caller's array. The same calls on lists, through a forwarding wrapper and through a method are
/// covered. Expected output is php 8.5's.
#[test]
fn test_positional_array_builtins_on_by_ref_array_parameter_accept_hashes() {
    let out = compile_and_run(
        r#"<?php
function t_0(array &$p) { return array_shift($p); }
function f_0(array $params) { $r = t_0($params); echo json_encode($r), json_encode($params), "\n"; }
f_0(['a' => 1, 'b' => 2, 'c' => 3]); f_0([10, 20, 30]);
function t_1(array &$p) { return array_pop($p); }
function f_1(array $params) { $r = t_1($params); echo json_encode($r), json_encode($params), "\n"; }
f_1(['a' => 1, 'b' => 2, 'c' => 3]); f_1([10, 20, 30]);
function t_2(array &$p) { array_unshift($p, 'u'); }
function f_2(array $params) { t_2($params); echo json_encode($params), "\n"; }
f_2(['a' => 1, 'b' => 2, 'c' => 3]); f_2([10, 20, 30]);
function t_3(array &$p) { sort($p); }
function f_3(array $params) { t_3($params); echo json_encode($params), "\n"; }
f_3(['c' => 3, 'a' => 1, 'b' => 2]); f_3([30, 10, 20]);
function t_4(array &$p) { rsort($p); }
function f_4(array $params) { t_4($params); echo json_encode($params), "\n"; }
f_4(['c' => 3, 'a' => 1, 'b' => 2]); f_4([30, 10, 20]);
function t_5(array &$p) { usort($p, fn($x, $y) => $y <=> $x); }
function f_5(array $params) { t_5($params); echo json_encode($params), "\n"; }
f_5(['c' => 3, 'a' => 1, 'b' => 2]); f_5([30, 10, 20]);
function t2_6(array &$p) { return array_shift($p); }
function t_6(array &$p) { return t2_6($p); }
function f_6(array $params) { $r = t_6($params); echo json_encode($r), json_encode($params), "\n"; }
f_6(['a' => 1, 'b' => 2, 'c' => 3]);
class K_7 { function t_7(array &$p) { return array_shift($p); } }
function f_7(array $params) { $r = (new K_7)->t_7($params); echo json_encode($r), json_encode($params), "\n"; }
f_7(['a' => 1, 'b' => 2, 'c' => 3]);
"#,
    );
    assert_eq!(
        out,
        "1{\"b\":2,\"c\":3}\n10[20,30]\n3{\"a\":1,\"b\":2}\n30[10,20]\n{\"0\":\"u\",\"a\":1,\"b\":2,\"c\":3}\n[\"u\",10,20,30]\n[1,2,3]\n[10,20,30]\n[3,2,1]\n[30,20,10]\n[3,2,1]\n[30,20,10]\n1{\"b\":2,\"c\":3}\n1{\"b\":2,\"c\":3}\n"
    );
}
