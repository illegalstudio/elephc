//! Purpose:
//! End-to-end coverage for references between places of static properties
//! (symfony DebugClassLoader `self::$darwinCache[$dir] = &self::$darwinCache[$k]`), `unset` of
//! static-property elements, and untyped `static` locals that hold arrays, objects or mixed
//! values (symfony LocaleFallbackProvider `static $p; $p ??= require F;`).
//!
//! Called from:
//! - The native codegen suite (`tests/codegen/mod.rs`).
//!
//! Key details:
//! - Expected output was produced by PHP 8.5 from the same source.
//! - The initializer of a `static` local runs once: several tests count its side effects.

use crate::support::*;

/// Verifies the DebugClassLoader shape: an element of a static property bound by reference to
/// another element of the same property, inside `if`/`while`, then written on one side, read on
/// the other, probed with `isset` and removed with `unset`.
#[test]
fn test_static_property_element_reference_darwin_cache() {
    let out = compile_and_run(r#"<?php
class D {
    private static array $c = [];
    public static function f(string $dir, string $k): void {
        if (!isset(self::$c[$k])) {
            while (!isset(self::$c[$k])) {
                self::$c[$k] = [$dir, []];
                self::$c[$dir] = &self::$c[$k];
            }
        }
    }
    public static function dump(): void {
        self::$c['/A'][1][] = 'x';
        echo json_encode(self::$c['/a']), "\n";
        var_dump(isset(self::$c['/A']));
        unset(self::$c['/A']);
        var_dump(isset(self::$c['/A']), self::$c['/a'][1]);
    }
}
D::f('/A', '/a'); D::dump();
"#);
    assert_eq!(out, r#"["\/A",["x"]]
bool(true)
bool(false)
array(1) {
  [0]=>
  string(1) "x"
}
"#);
}

/// Verifies `self::$a[$x] = &$local`, `$local = &self::$a[$x]` and `static::$a[..][..] = &...`
/// on an untyped static property, with writes on either side of each alias.
#[test]
fn test_static_property_element_reference_place_kinds() {
    let out = compile_and_run(r#"<?php
class P {
    public static $a = [];
    public static function run(): void {
        $v = 1;
        self::$a['x'] = &$v;
        $v = 2;
        echo self::$a['x'], "\n";
        self::$a['x'] = 3;
        echo $v, "\n";
        $r = &self::$a['y'];
        $r = 'ry';
        echo self::$a['y'], "\n";
        static::$a['m']['n'] = &self::$a['y'];
        static::$a['m']['n'] = 'mn';
        echo $r, " ", self::$a['y'], "\n";
        unset($r);
        echo json_encode(self::$a), "\n";
    }
}
P::run();
"#);
    assert_eq!(out, "2\n3\nry\nmn mn\n{\"x\":3,\"y\":\"mn\",\"m\":{\"n\":\"mn\"}}\n");
}

/// Verifies element references into an untyped static property whose default is a non-empty
/// array: the slot is widened to the boxed PHP array representation (it crashed before).
#[test]
fn test_reference_into_untyped_static_property_with_array_default() {
    let out = compile_and_run(r#"<?php
class D {
    public static $u = ["a" => 1, "b" => ["x" => 1]];
    static function f() {
        $r = &self::$u["b"]["x"]; $r = 5;
        $q = &self::$u["a"]; $q = 6;
        self::$u["c"] = &self::$u["a"]; self::$u["c"] = 7;
        echo json_encode(self::$u), "\n";
    }
}
D::f();
"#);
    assert_eq!(out, "{\"a\":7,\"b\":{\"x\":5},\"c\":7}\n");
}

/// Verifies a reference to a static-property element bound inside a branch stays visible after it.
#[test]
fn test_conditional_reference_to_static_property_element() {
    let out = compile_and_run(r#"<?php
class D {
    public static $a = ["k" => 1];
    static function f($c) {
        $r = 0;
        if ($c) { $r = &self::$a["k"]; }
        $r = 5;
        echo json_encode(self::$a), "\n";
        if ($c) { self::$a["x"] = &$r; }
        $r = 6;
        echo json_encode(self::$a), "\n";
    }
}
D::f(true); D::f(false);
"#);
    assert_eq!(out, "{\"k\":5}\n{\"k\":6,\"x\":6}\n{\"k\":6,\"x\":6}\n{\"k\":6,\"x\":6}\n");
}

/// Verifies `unset(C::$p[$k])` and `unset(C::$p[$a][$b])` on typed, untyped and packed static
/// arrays, leaving an earlier by-value copy untouched.
#[test]
fn test_unset_static_property_elements() {
    let out = compile_and_run(r#"<?php
class D {
    public static array $c = ['a' => 1, 'b' => 2];
    public static $u = ['a' => 1, 'b' => ['x' => 1, 'y' => 2]];
    public static array $l = [1, 2, 3];
    static function f() {
        $copy = self::$c;
        unset(self::$c['a']);
        unset(static::$u['b']['x']);
        unset(self::$l[1]);
        echo json_encode(self::$c), json_encode(self::$u), json_encode(self::$l), json_encode($copy), "\n";
    }
}
D::f(); D::f();
var_dump(isset(D::$u['b']['y']), isset(D::$u['b']['x']));
"#);
    assert_eq!(out, r#"{"b":2}{"a":1,"b":{"y":2}}{"0":1,"2":3}{"a":1,"b":2}
{"b":2}{"a":1,"b":{"y":2}}{"0":1,"2":3}{"b":2}
bool(true)
bool(false)
"#);
}

/// Verifies an untyped `static` local with no initializer persists an associative array and
/// runs its `??=` value once, plus a string-keyed cache over `static $c = []`.
#[test]
fn test_untyped_static_local_assoc_array_initializer_runs_once() {
    let out = compile_and_run(r#"<?php
function make() { echo "make "; return ['a' => 1]; }
function f() { static $p; $p ??= make(); $p['n'] = ($p['n'] ?? 0) + 1; return $p; }
echo json_encode(f()), json_encode(f()), json_encode(f()), "\n";
function c($k) { static $c = []; $c[$k] = ($c[$k] ?? 0) + 1; return json_encode($c); }
echo c("a"), c("b"), c("a"), "\n";
function q($k) { static $c = []; return $c[$k] ??= strtoupper($k); }
echo q("a"), q("b"), q("a"), "\n";
"#);
    assert_eq!(out, "make {\"a\":1,\"n\":1}{\"a\":1,\"n\":2}{\"a\":1,\"n\":3}\n{\"a\":1}{\"a\":1,\"b\":1}{\"a\":2,\"b\":1}\nABA\n");
}

/// Verifies `static $x = null; if ($x === null) { ... }` initializes once, with arrays, objects,
/// floats and strings, and that a mixed static keeps whatever the previous call stored.
#[test]
fn test_null_defaulted_static_local_initializes_once() {
    let out = compile_and_run(r#"<?php
function h() {
    static $x = null;
    if ($x === null) { echo "init "; $x = ['es_CL' => 'es_419', 'es_419' => 'es']; }
    return $x;
}
echo json_encode(h()), count(h()), "\n";
function o() { static $o; $o ??= new ArrayObject([]); $o['c'] = ($o['c'] ?? 0) + 1; return $o['c']; }
echo o(), o(), o(), "\n";
function m($v) { static $m = null; $prev = $m; $m = $v; return json_encode($prev); }
echo m(1), m('s'), m([1 => 'x']), m(null), "\n";
function s() { static $s = null; $s = ($s ?? "") . "x"; return $s; }
echo s(), s(), s(), "\n";
function d() { static $p = null; if (is_null($p)) { $p = 1.5; } else { $p *= 2; } return $p; }
echo d(), " ", d(), " ", d(), "\n";
"#);
    assert_eq!(out, "init {\"es_CL\":\"es_419\",\"es_419\":\"es\"}2\n123\nnull1\"s\"{\"1\":\"x\"}\nxxxxxx\n1.5 3 6\n");
}

/// Verifies the LocaleFallbackProvider shape: `static $p; $p ??= require F;` includes the file
/// once and keeps the associative array for later calls.
#[test]
fn test_static_local_coalesce_require_includes_once() {
    let out = compile_and_run_files(
        &[
            (
                "main.php",
                r#"<?php
function parents() {
    static $p;
    $p ??= require __DIR__ . '/parents.php';
    return $p;
}
echo json_encode(parents()), "\n";
echo parents()['es_CL'], count(parents()), "\n";
"#,
            ),
            (
                "parents.php",
                r#"<?php
echo "loaded\n";
return ['es_CL' => 'es_419', 'es_419' => 'es'];
"#,
            ),
        ],
        "main.php",
    );
    assert_eq!(out, "loaded\n{\"es_CL\":\"es_419\",\"es_419\":\"es\"}\nes_4192\n");
}

/// Verifies a method's static locals are shared by the class and a subclass that inherits the
/// method, including `parent::` from an override (PHP 8.1+ semantics).
#[test]
fn test_method_static_local_shared_with_inheriting_subclass() {
    let out = compile_and_run(r#"<?php
class A {
    public function c() { static $n = []; $n[static::class] = ($n[static::class] ?? 0) + 1; return json_encode($n); }
    public static function s() { static $x; $x ??= ['k' => static::class]; return $x['k']; }
}
class B extends A {}
class C extends A { public function c() { return 'C' . parent::c(); } }
echo (new A)->c(), (new B)->c(), (new A)->c(), "\n";
echo (new C)->c(), "\n";
echo A::s(), B::s(), "\n";
"#);
    assert_eq!(out, "{\"A\":1}{\"A\":1,\"B\":1}{\"A\":2,\"B\":1}\nC{\"A\":2,\"B\":1,\"C\":1}\nAA\n");
}

/// Verifies element references into a null-defaulted static local (nette ObjectHelpers
/// `$props = &$cache[$class]`) and array builtins over a boxed static array.
#[test]
fn test_element_reference_into_static_local_cache() {
    let out = compile_and_run(r#"<?php
function f($c) { static $cache; $props = &$cache[$c]; if ($props === null) { echo "calc "; $props = strlen($c); } return $props; }
echo f("ab"), f("ab"), f("abc"), "\n";
function g($x) { static $c = []; $c = array_merge($c, [$x]); return implode(",", $c) . count($c) . (in_array(1, $c) ? "y" : "n"); }
echo g(1), g(2), "\n";
"#);
    assert_eq!(out, "calc 22calc 3\n11y1,22y\n");
}

/// Verifies boxed static locals and static-property element references release what they own.
#[test]
fn test_static_ref_and_static_local_heap_debug_clean() {
    let out = compile_and_run_with_heap_debug(r#"<?php
class D {
    private static array $c = [];
    public static $u = ['a' => ['x' => 'y'], 'b' => 'w'];
    public static function f(string $dir, string $k): void {
        self::$c[$k] = [$dir, []];
        self::$c[$dir] = &self::$c[$k];
        self::$c[$dir][1][] = str_repeat('z', 3);
        $r = &self::$u['a']['x'];
        $r = str_repeat('q', 2);
        unset($r);
        unset(self::$u['b']);
    }
}
function make() { return ['a' => str_repeat('m', 4)]; }
function f($k) { static $p; $p ??= make(); static $c = []; $c[$k] = ($c[$k] ?? '') . $k; static $s = null; $s = ($s ?? '') . $k; return count($p) + count($c) + strlen($s); }
D::f('/A', '/a'); D::f('/B', '/b');
echo f('a'), f('b'), f('a'), "\n";
"#);
    assert!(out.success, "{}", out.stderr);
    assert_eq!(out.stdout, "356\n");
    assert!(
        out.stderr.contains("HEAP DEBUG: leak summary: clean"),
        "static-ref leaked: {}",
        out.stderr
    );
}

/// Verifies the static-local array cases: a keyed write accumulates across
/// calls for string and int keys, and `count` sees every key stored by earlier calls.
#[test]
fn test_static_local_keyed_array_accumulates_review_s1_s3() {
    let out = compile_and_run(r#"<?php
function loc($k) { static $c = []; $c[$k] = ($c[$k] ?? 0) + 1; return $c; }
loc('a'); loc(1); echo json_encode(loc('a')), "\n";
function cnt($k) { static $c = []; $c[$k] = 1; return count($c); }
echo cnt('a'), cnt('b'), cnt('a'), "\n";
function loci($k) { static $c = []; $c[$k] = ($c[$k] ?? 0) + 1; return $c[$k]; }
echo loci(1), loci(1), loci(2), "\n";
function locs($k) { static $c = []; $c[$k] = ($c[$k] ?? 0) + 1; return $c[$k]; }
echo locs('a'), locs('a'), locs('b'), "\n";
"#);
    assert_eq!(out, "{\"a\":2,\"1\":1}\n122\n121\n121\n");
}

/// Verifies the static-local case: an int key written into a static local initialized with a
/// string-keyed array keeps both keys (it corrupted the hash to `{"\u0000":0}`).
#[test]
fn test_static_local_string_keyed_initializer_takes_int_key_review_s2() {
    let out = compile_and_run(r#"<?php
function loc2($k) { static $u = ['x' => 0]; $u[$k] = 1; return $u; }
echo json_encode(loc2('y')), json_encode(loc2(3)), "\n";
echo "after\n";
"#);
    assert_eq!(out, "{\"x\":0,\"y\":1}{\"x\":0,\"y\":1,\"3\":1}\nafter\n");
}

/// Verifies `array_push` on a boxed `static` local matches php (the
/// builtin works on a hidden copy that is stored back into the static).
#[test]
fn test_boxed_static_local_by_ref_builtin_array_push() {
    let out = compile_and_run(r#"<?php
function f($x) { static $c = []; $n = array_push($c, $x, $x); return $n . json_encode($c); } echo f("a"), f("b");
"#);
    assert_eq!(out, "2[\"a\",\"a\"]4[\"a\",\"a\",\"b\",\"b\"]");
}

/// Verifies `array_unshift` on a boxed `static` local matches php (the
/// builtin works on a hidden copy that is stored back into the static).
#[test]
fn test_boxed_static_local_by_ref_builtin_array_unshift() {
    let out = compile_and_run(r#"<?php
function f($x) { static $c = []; array_unshift($c, $x); return json_encode($c); } echo f(1), f(2), f(3);
"#);
    assert_eq!(out, "[1][2,1][3,2,1]");
}

/// Verifies `sort` on a boxed `static` local matches php (the
/// builtin works on a hidden copy that is stored back into the static).
#[test]
fn test_boxed_static_local_by_ref_builtin_sort() {
    let out = compile_and_run(r#"<?php
function f($x) { static $c = []; $c[] = $x; sort($c); return json_encode($c); } echo f(3), f(1), f(2);
"#);
    assert_eq!(out, "[3][1,3][1,2,3]");
}

/// Verifies `ksort` on a boxed `static` local matches php (the
/// builtin works on a hidden copy that is stored back into the static).
#[test]
fn test_boxed_static_local_by_ref_builtin_ksort() {
    let out = compile_and_run(r#"<?php
function f($x) { static $c = []; $c[$x] = strlen($x); ksort($c); return json_encode($c); } echo f("b"), f("a"), f("cc");
"#);
    assert_eq!(out, "{\"b\":1}{\"a\":1,\"b\":1}{\"a\":1,\"b\":1,\"cc\":2}");
}

/// Verifies `usort` on a boxed `static` local matches php (the
/// builtin works on a hidden copy that is stored back into the static).
#[test]
fn test_boxed_static_local_by_ref_builtin_usort() {
    let out = compile_and_run(r#"<?php
function f($x) { static $c = []; $c[] = $x; usort($c, fn($a, $b) => $b <=> $a); return json_encode($c); } echo f(1), f(3), f(2);
"#);
    assert_eq!(out, "[1][3,1][3,2,1]");
}

/// Verifies `array_splice` on a boxed `static` local matches php (the
/// builtin works on a hidden copy that is stored back into the static).
#[test]
fn test_boxed_static_local_by_ref_builtin_array_splice() {
    let out = compile_and_run(r#"<?php
function f($x) { static $c = []; $c[] = $x; array_splice($c, 0, count($c) > 2 ? 1 : 0); return json_encode($c); } echo f(1), f(2), f(3);
"#);
    assert_eq!(out, "[1][1,2][2,3]");
}

/// Verifies `end`/`reset` on a boxed `static` local matches php (the
/// builtin works on a hidden copy that is stored back into the static).
#[test]
fn test_boxed_static_local_by_ref_builtin_end_reset() {
    let out = compile_and_run(r#"<?php
function f($x) { static $c = []; $c[] = $x; return end($c) . reset($c); } echo f(1), f(2), f(3);
"#);
    assert_eq!(out, "112131");
}

/// Verifies `array_shift` on a boxed `static` local matches php (the
/// builtin works on a hidden copy that is stored back into the static).
#[test]
fn test_boxed_static_local_by_ref_builtin_array_shift() {
    let out = compile_and_run(r#"<?php
function f($k) { static $c = []; $c[$k] = 1; if (count($c) > 2) { array_shift($c); } return json_encode($c); } echo f("a"), f("b"), f("c");
"#);
    assert_eq!(out, "{\"a\":1}{\"a\":1,\"b\":1}{\"b\":1,\"c\":1}");
}

/// Verifies `array_pop` on a boxed `static` local matches php (the
/// builtin works on a hidden copy that is stored back into the static).
#[test]
fn test_boxed_static_local_by_ref_builtin_array_pop() {
    let out = compile_and_run(r#"<?php
function f($x) { static $c = []; $c[] = $x; $c[] = $x * 10; return array_pop($c) . "/" . json_encode($c); } echo f(1), f(2), f(3);
"#);
    assert_eq!(out, "10/[1]20/[1,2]30/[1,2,3]");
}

/// Verifies `sort_non_empty_initializer` on a boxed `static` local matches php (the
/// builtin works on a hidden copy that is stored back into the static).
#[test]
fn test_boxed_static_local_by_ref_builtin_sort_non_empty_initializer() {
    let out = compile_and_run(r#"<?php
function f($x) { static $c = [3]; $c[] = $x; sort($c); return json_encode($c); } echo f(1), f(2);
"#);
    assert_eq!(out, "[1,3][1,2,3]");
}

/// Verifies `preg_match(..., $m)` with a boxed `static` local `$m` matches php.
#[test]
fn test_boxed_static_local_by_ref_builtin_preg_match() {
    let out = compile_and_run_with_regex(r#"<?php
function f($s) { static $m; $hit = preg_match("/(a)(b)?/", $s, $m); return $hit . json_encode($m); } echo f("ab"), f("a"), f("z");
"#);
    assert_eq!(out, "1[\"ab\",\"a\",\"b\"]1[\"a\",\"a\"]0[]");
}

/// Verifies `array_pop` rewinds the automatic index like php (`$a = [1, 2]; array_pop($a); $a[] = 3;` is `[1, 3]`).
#[test]
fn test_array_pop_rewinds_next_index_on_boxed_array() {
    let out = compile_and_run(r#"<?php
function f(array $c) { $c[] = 1; $c[] = 2; array_pop($c); $c[] = 3; return json_encode($c); }
echo f([]), f(["k" => 0]), "\n";
$h = ["a" => 1, 5 => 2]; array_pop($h); $h[] = 9; echo json_encode($h), "\n";
$o = [7 => 1, 2 => 2]; array_pop($o); $o[] = 9; echo json_encode($o), "\n";
"#);
    assert_eq!(out, "[1,3]{\"k\":0,\"0\":1,\"1\":3}\n{\"a\":1,\"5\":9}\n{\"7\":1,\"8\":9}\n");
}

/// Verifies the hidden copy and write-back of by-reference builtins on boxed static locals
/// release everything they own.
#[test]
fn test_boxed_static_local_by_ref_builtins_heap_debug_clean() {
    let out = compile_and_run_with_heap_debug(r#"<?php
function f($x) {
    static $c = [];
    array_push($c, str_repeat($x, 2));
    array_unshift($c, $x);
    sort($c);
    $p = array_pop($c);
    $s = array_shift($c);
    usort($c, fn($a, $b) => strcmp($b, $a));
    return count($c) . $p . $s;
}
echo f("a"), f("b"), f("c"), "\n";
"#);
    assert!(out.success, "{}", out.stderr);
    assert_eq!(out.stdout, "0aaa0bbb0ccc\n");
    assert!(
        out.stderr.contains("HEAP DEBUG: leak summary: clean"),
        "boxed static builtins leaked: {}",
        out.stderr
    );
}

/// Verifies a never-written static local with a literal array initializer keeps its element types:
/// a nested array element is still an `array` for `: array`, an `array`
/// parameter, `array_map`, `array_merge`, `max` and `sort` on a copy.
#[test]
fn test_read_only_static_local_literal_keeps_element_types() {
    let out = compile_and_run(r#"<?php
function f(): array { static $c = ["a" => ["b" => [1]]]; return $c["a"]["b"]; }
function g(array $a) { return count($a); }
function h() { static $c = ["a" => [3, 1]]; $x = $c["a"]; sort($x); return implode(",", $x) . g($c["a"]) . max($c["a"]); }
function m() { static $c = ["a" => [1, 2]]; return json_encode(array_map(fn($x) => $x + 1, $c["a"])) . json_encode(array_merge($c["a"], [9])); }
echo json_encode(f()), h(), m(), "\n";
"#);
    assert_eq!(out, "[1]1,323[2,3][1,2,9]\n");
}

/// Verifies `static::$p[..] = &...` and `unset(static::$p[..])` from the parent reach the late
/// static binding's own untyped redeclaration (it crashed).
#[test]
fn test_late_static_element_reference_and_unset_on_redeclared_property() {
    let out = compile_and_run(r#"<?php
class P {
    public static $a = ["k" => "P", "j" => "J"];
    static function l() { static::$a["k"] = &static::$a["j"]; static::$a["j"] = "z"; }
    static function v() { $v = "v"; static::$a["v"] = &$v; $v = "w"; }
    static function un() { unset(static::$a["k"]); }
}
class Q extends P { public static $a = ["k" => "Q", "j" => "QJ"]; }
Q::l(); Q::v(); echo P::$a["k"], Q::$a["k"], Q::$a["v"], "\n";
P::un(); echo count(P::$a), count(Q::$a), "\n";
Q::un(); echo count(P::$a), count(Q::$a), "\n";
"#);
    assert_eq!(out, "Pzw\n13\n12\n");
}

/// Verifies `unset(C::$p[$a][$b])` with `$a` absent is a silent no-op that creates no key, and
/// that a scalar intermediate throws php's `Error`.
#[test]
fn test_nested_static_unset_with_missing_intermediate_is_noop() {
    let out = compile_and_run(r#"<?php
class C {
    public static $a = ["a" => ["b" => 1, "c" => 2]];
    public static function f($x, $y) { unset(static::$a[$x][$y]); }
}
unset(C::$a["zz"]["q"]); C::f("q", "b"); C::f("a", "b");
echo json_encode(C::$a), "\n";
try { unset(C::$a["a"]["c"]["deep"]); } catch (\Error $e) { echo get_class($e), ": ", $e->getMessage(), "\n"; }
"#);
    assert_eq!(out, "{\"a\":{\"c\":2}}\nError: Cannot unset offset in a non-array variable\n");
}

/// Verifies an associative static literal whose writes keep its type stays precisely typed:
/// a nested element is still an `array` for an `array` parameter
/// and `sort` on a copy.
#[test]
fn test_type_preserving_writes_keep_assoc_static_precise() {
    let out = compile_and_run(r#"<?php
function g(array $a) { return count($a); }
function f() { static $c = ["a" => [1]]; $c["a"][] = 2; return g($c["a"]); }
function h() { static $c = ["a" => [3, 1]]; $c["b"] = [5, 4]; $x = $c["b"]; sort($x); return implode(",", $x) . count($c); }
echo f(), f(), h(), h(), "\n";
"#);
    assert_eq!(out, "234,524,52\n");
}

/// Verifies an increment through a nested element of a static array works,
/// and that written indexed statics returned by value keep every call's writes (they used to
/// be lost silently).
#[test]
fn test_written_static_arrays_nested_incdec_and_returned_by_value() {
    let out = compile_and_run(r#"<?php
function inc() { static $c = ["a" => ["b" => 1]]; $c["a"]["b"]++; return $c["a"]["b"]; }
function p() { static $c = []; $c[] = 5; return $c; }
function w() { static $c = [1, 2]; $c[0] = $c[0] + 1; return $c; }
function q() { static $c = []; $c[] = [1, 2]; return $c; }
echo inc(), inc(), "\n";
p(); echo json_encode(p()), "\n";
w(); echo json_encode(w()), "\n";
q(); echo json_encode(q()), "\n";
"#);
    assert_eq!(out, "23\n[5,5]\n[3,2]\n[[1,2],[1,2]]\n");
}

/// Verifies a by-value `foreach` over a static local, a `global` and a static property iterates
/// the array as it was when the loop started, even when the body appends to it (before,
/// the loop walked its own appends, and an associative variant exhausted the heap).
#[test]
fn test_foreach_by_value_over_static_global_and_static_property_iterates_snapshot() {
    let out = compile_and_run(r#"<?php
function f() { static $c = [1, 2]; $n = 0; foreach ($c as $v) { $c[] = $v; $n++; } return $n . ":" . count($c); }
function a() { static $c = ["a" => 1, "b" => 2]; $n = 0; foreach ($c as $k => $v) { $c[$k . $n] = $v; $n++; } return $n . ":" . count($c); }
function e() { static $c = []; $c[] = 1; $n = 0; foreach ($c as $v) { $c[] = 9; $n++; } return $n . ":" . count($c); }
$g = [1, 2];
function g() { global $g; $n = 0; foreach ($g as $v) { $g[] = $v; $n++; } return $n . ":" . count($g); }
class D { public static array $a = [1, 2]; static function f() { $n = 0; foreach (self::$a as $v) { self::$a[] = $v; $n++; } return $n . ":" . count(self::$a); } }
echo f(), " ", f(), " ", f(), "\n", a(), " ", a(), " ", a(), "\n", e(), " ", e(), " ", e(), "\n";
echo g(), " ", g(), " ", D::f(), " ", D::f(), "\n";
"#);
    assert_eq!(out, "2:4 4:8 8:16\n2:4 4:6 6:8\n1:2 3:6 7:14\n2:4 4:8 2:4 4:8\n");
}
