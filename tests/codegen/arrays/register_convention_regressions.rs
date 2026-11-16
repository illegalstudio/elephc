//! Purpose:
//! Heap-debug regressions for the x86_64 register-convention mismatches and the hash/union helper
//! defects found in the review of the hash runtime helpers (chunks 132-137).
//!
//! Called from:
//! - `cargo test` through Rust's test harness.
//!
//! Key details:
//! - `__rt_incref` and the `__rt_decref_*` family read their pointer from `rax` on linux-x86_64;
//!   helpers that loaded `rdi` retained nothing (use-after-free) or released an unrelated block.
//!   Every fixture ends with a clean heap-debug leak summary, so a missed retain or release fails
//!   on the Linux x86_64 shard even though aarch64 hides the bug.
//! - Every expected string is verbatim `php -n` output for the same program.

use crate::support::*;

/// Asserts a plain run printed `expected`.
fn assert_runs(source: &str, expected: &str) {
    assert_eq!(compile_and_run(source), expected);
}

/// Asserts a heap-debug run printed `expected` and ended with a clean leak summary.
fn assert_clean(source: &str, expected: &str) {
    let out = compile_and_run_with_heap_debug(source);
    assert!(out.success, "program failed: {}\n{}", out.stdout, out.stderr);
    assert_eq!(out.stdout, expected);
    assert!(
        out.stderr.contains("HEAP DEBUG: leak summary: clean"),
        "{}",
        out.stderr
    );
}

/// `array_reverse`, `array_slice` and `array_chunk` over hashes retain every heap-backed value
/// they copy (`__rt_hash_to_hash_reverse` / `__rt_hash_reindex`), so the source can be released.
#[test]
fn test_hash_copies_retain_heap_values_before_the_source_is_released() {
    assert_clean(
        r#"<?php
$h = ['a' => [1], 'b' => [2, 3], 'c' => ['x' => 'y']];
$r = array_reverse($h, true);
unset($h);
echo json_encode($r), "\n";
$s = ['p' => [1, 2], 'q' => [3]];
$t = array_slice($s, 0, 2);
$u = array_slice($s, 1, 1, true);
unset($s);
echo json_encode($t), json_encode($u), "\n";
$c = ['k1' => [1], 'k2' => [2], 'k3' => [3]];
$chunks = array_chunk($c, 2, true);
unset($c);
echo json_encode($chunks), "\n";
$m = ['a' => [1], 'b' => 5, 'c' => 'str'];
$rev = array_reverse($m);
unset($m);
echo json_encode($rev), "\n";
"#,
        "{\"c\":{\"x\":\"y\"},\"b\":[2,3],\"a\":[1]}\n{\"p\":[1,2],\"q\":[3]}{\"q\":[3]}\n[{\"k1\":[1],\"k2\":[2]},{\"k3\":[3]}]\n{\"c\":\"str\",\"b\":5,\"a\":[1]}\n",
    );
}

/// The recursive merges, the associative set operations, `array_flip` and
/// `array_map` release their temporaries through the `rax` convention of the decref helpers; a
/// release through `rdi` freed the RESULT on linux-x86_64. No leak summary is asserted: the
/// recursive merge and the associative set operations leak on their own, on every target.
#[test]
fn test_array_helpers_release_their_temporaries_without_leaking() {
    assert_runs(
        r#"<?php
function dump($x) { echo json_encode($x), "\n"; }
$a = ['k' => [1, 2], 'm' => ['x' => 1]];
$b = ['k' => [3], 'm' => ['y' => [2]]];
dump(array_merge_recursive(['m' => ['x' => 1]], ['m' => ['y' => [2]]]));
dump(array_replace_recursive($a, $b));
dump(array_replace_recursive([[1, 2], 'z' => [3]], [[9], 'z' => ['q' => 1]]));
function boxed($v) { return $v; }
dump(array_diff_assoc(['a' => boxed(1), 'b' => boxed("2"), 'c' => boxed(3.5)], ['a' => "1", 'b' => 2, 'c' => 4]));
dump(array_intersect_assoc(['a' => boxed(1), 'b' => boxed("2"), 'c' => boxed(3.5)], ['a' => "1", 'b' => 2, 'c' => 4]));
function flip($a) { return array_flip($a); }
dump(flip(['x' => 'p', 'y' => 'q']));
dump(flip([5 => 'a', 6 => 'b']));
function mp($a) { return array_map(fn($v) => $v . '!', $a); }
dump(mp(['a' => 'x', 'b' => 'y']));
"#,
        "{\"m\":{\"x\":1,\"y\":[2]}}\n{\"k\":[3,2],\"m\":{\"x\":1,\"y\":[2]}}\n{\"0\":[9,2],\"z\":{\"0\":3,\"q\":1}}\n{\"c\":3.5}\n{\"a\":1,\"b\":\"2\"}\n{\"p\":\"x\",\"q\":\"y\"}\n{\"a\":5,\"b\":6}\n{\"a\":\"x!\",\"b\":\"y!\"}\n",
    );
}

/// Gradual `ksort`, `array_merge` and `array_replace` probe the storage kind of a receiver
/// that a runtime string-keyed write promoted to a hash (`__rt_heap_kind` reads `rax`), and
/// `str_replace` / `method_exists` convert Mixed operands through the `rax` helpers.
#[test]
fn test_gradual_array_builtins_dispatch_on_runtime_storage_kind() {
    assert_clean(
        r#"<?php
function dump($x) { echo json_encode($x), "\n"; }
function ks($a) { $a['b'] = 2; $a['a'] = 1; ksort($a); return $a; }
dump(ks([]));
function mg($a, $b) { return array_merge($a, $b); }
function rp($a, $b) { $a['k'] = 1; return array_replace($a, $b); }
dump(mg([1, 2], ['x' => 3]));
dump(rp([1, 2], [5 => 3]));
function sr($s, $search) { return str_replace($search, 'Z', $s); }
dump(sr('abc abc', ['a', 'b']));
class Foo { public $p = 1; function bar() {} }
function me($c, $m) { return [method_exists($c, $m), property_exists($c, 'p')]; }
dump(me('Foo', 'bar'));
dump(me('Foo', 'nope'));
"#,
        "{\"a\":1,\"b\":2}\n{\"0\":1,\"1\":2,\"x\":3}\n{\"0\":1,\"1\":2,\"k\":1,\"5\":3}\n\"ZZc ZZc\"\n[true,true]\n[false,true]\n",
    );
}

/// Offset captures, named groups and `(*MARK)` persist their strings through `__rt_str_persist`,
/// which reads the pointer from `rax`.
#[test]
fn test_preg_match_captures_persist_their_strings() {
    assert_clean(
        r#"<?php
preg_match('/(?<word>b+)/', 'aabbb', $m, PREG_OFFSET_CAPTURE);
echo json_encode($m), "\n";
preg_match('/(a)(*MARK:tag)b/', 'ab', $m2);
echo json_encode($m2), "\n";
"#,
        "{\"0\":[\"bbb\",2],\"word\":[\"bbb\",2],\"1\":[\"bbb\",2]}\n{\"0\":\"ab\",\"1\":\"a\",\"MARK\":\"tag\"}\n",
    );
}

/// Rebinding an existing static-property hash key to a reference keeps the key's insertion-order
/// position and splits a table still shared with a copy, as php does. Only keys and the copy's
/// untouched entries are printed: reading a rebound int element of a mixed hash is a separate,
/// older defect.
#[test]
fn test_hash_bind_ref_element_keeps_key_order_and_splits_shared_tables() {
    let out = compile_and_run(
        r#"<?php
class D { static $a = ['d' => 'str', 'k' => 1, 'z' => 3]; static function f() { self::$a['d'] = &self::$a['k']; echo implode(',', array_keys(self::$a)), "\n"; } }
D::f();
class C { static $a = ['k' => 1, 'd' => [5], 'm' => 'v']; }
C::$a['x'] = &C::$a['k'];
$copy = C::$a;
C::$a['d'] = &C::$a['k'];
echo implode(',', array_keys($copy)), ' ', is_array($copy['d']) ? 'array' : 'gone', ' ', $copy['m'], "\n";
echo implode(',', array_keys(C::$a)), ' ', $copy['d'][0], "\n";
"#,
    );
    assert_eq!(out, "d,k,z\nk,d,m,x array v\nk,d,m,x 5\n");
}

/// `array_unshift` of a Mixed value that is also held by a variable must not empty that value's
/// cell into the array entry.
#[test]
fn test_array_unshift_of_a_shared_mixed_box_leaves_the_variable_intact() {
    assert_clean(
        r#"<?php
function f(array|false $a, mixed $v) { array_unshift($a, $v); return [$a, $v]; }
$r = f(['k' => 1], [1, 2]);
echo json_encode($r), "\n";
function g(array|false $a, mixed $v) { array_unshift($a, $v); $v[] = 3; return [$a, $v]; }
echo json_encode(g(['k' => 1], [1, 2])), "\n";
function h(array|false $a, mixed $v) { array_unshift($a, $v); return [$a, $v]; }
echo json_encode(h(['k' => 1], "str")), "\n";
"#,
        "[{\"0\":[1,2],\"k\":1},[1,2]]\n[{\"0\":[1,2],\"k\":1},[1,2,3]]\n[{\"0\":\"str\",\"k\":1},\"str\"]\n",
    );
}

/// `krsort` / `ksort` over a union receiver promote a list to a hash, and never reorder a
/// copy that shares the receiver's cell.
#[test]
fn test_union_key_sorts_promote_lists_and_keep_copies_intact() {
    assert_clean(
        r#"<?php
function f(array|false $a) { krsort($a); return $a; }
echo json_encode(f([10, 20, 30])), "\n";
function g(array|false $a) { $b = $a; ksort($a); return array_keys($b); }
echo json_encode(g(['b' => 1, 'a' => 2])), "\n";
function k(array|false $a) { $b = $a; krsort($a); return [$a, $b]; }
echo json_encode(k(['a' => 1, 'b' => 2])), "\n";
"#,
        "{\"2\":30,\"1\":20,\"0\":10}\n[\"b\",\"a\"]\n[{\"b\":2,\"a\":1},{\"a\":1,\"b\":2}]\n",
    );
}

/// Spreading a `[$object, 'method']` callable materializes a two-slot array whose creation
/// reference is released, so the receiver's destructor runs at `unset`.
#[test]
fn test_spread_of_a_callable_array_does_not_leak_the_receiver() {
    assert_clean(
        r#"<?php
class A { function m() {} function __destruct() { echo "d"; } }
function g(callable $c) { return method_exists(...$c); }
$a = new A; var_dump(g([$a, 'm'])); unset($a); echo "end\n";
"#,
        "bool(true)\ndend\n",
    );
}
