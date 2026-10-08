//! Purpose:
//! End-to-end AOT tests for `http_build_query()`, the injected elephc-PHP prelude in
//! `src/http_build_query_prelude.rs`.
//!
//! Called from:
//! - `cargo test --test codegen_tests http_build_query` through the strings integration module.
//!
//! Key details:
//! - Every fixture runs 20 iterations inside a function under `--heap-debug`, so a string the
//!   prelude keeps without owning it shows up as a leak. Expected stdout is reference PHP 8.5's.

use crate::support::*;

/// Asserts a heap-debug run succeeded, printed `expected`, and released every allocation.
fn assert_clean(source: &str, expected: &str) {
    let out = compile_and_run_with_heap_debug(source);
    assert!(out.success, "stdout={:?}\nstderr={}", out.stdout, out.stderr);
    assert_eq!(out.stdout, expected, "{}", out.stderr);
    assert!(out.stderr.contains("HEAP DEBUG: leak summary: clean"), "{}", out.stderr);
}

/// Scalars, booleans, floats, nulls, nested and empty arrays, the raw top-level numeric prefix,
/// custom and empty separators, and both encoding types match PHP; results stay owned.
#[test]
fn test_http_build_query_scalars_nesting_separators_and_encodings() {
    assert_clean(
        r#"<?php
function run(int $i): string {
    $r = http_build_query(["a" => 1, "b" => "x y" . $i, "c" => null, "d" => true, "e" => false,
        "f" => 0.1, "g" => 1.0, "h" => 1e20, "i" => -0.0, "j" => [1, 2, ["k" => "v"]], "e2" => [],
        5 => "five", "s p" => "&=", "n" => [null], "u" => "é~"]) . "\n";
    $r .= http_build_query([1, 2, "x" => [3, 4]], "p_") . "\n";
    $r .= http_build_query(["a" => "b c", "d" => "~", "n" => ["x y" => 1]], "", "&amp;", PHP_QUERY_RFC3986) . "\n";
    $r .= http_build_query(["a" => 1, "b" => 2], "", "") . "\n";
    $r .= http_build_query([[1], "k" => 2], "a b&", ";") . "\n";
    $r .= http_build_query([]) . "|" . http_build_query(["a" => 1], "", null, 99) . "\n";
    return $r;
}
$out = "";
for ($i = 0; $i < 20; $i++) { $out = run($i); }
echo $out;
"#,
        r#"a=1&b=x+y19&d=1&e=0&f=0.1&g=1&h=1.0E%2B20&i=-0&j%5B0%5D=1&j%5B1%5D=2&j%5B2%5D%5Bk%5D=v&5=five&s+p=%26%3D&u=%C3%A9%7E
p_0=1&p_1=2&x%5B0%5D=3&x%5B1%5D=4
a=b%20c&amp;d=~&amp;n%5Bx%20y%5D=1
a=1b=2
a b&0%5B0%5D=1;k=2
|a=1
"#,
    );
}

/// Object data and object values contribute their public declared and dynamic properties;
/// protected, private and null properties are skipped like PHP.
#[test]
fn test_http_build_query_objects_use_public_properties() {
    assert_clean(
        r#"<?php
class Point { public $x = "x"; protected $hidden = 2; private $secret = 3; public $missing = null; public $tags = ["q" => 1]; }
class Nothing {}
function run(int $i): string {
    $p = new Point();
    $p->x = "x" . $i;
    $dynamic = new stdClass();
    $dynamic->a = "b c";
    $dynamic->list = [1, 2];
    $r = http_build_query($p) . "\n";
    $r .= http_build_query(["p" => $p, "d" => $dynamic, "n" => new Nothing()]) . "\n";
    $r .= "[" . http_build_query(new Nothing()) . "]\n";
    return $r;
}
$out = "";
for ($i = 0; $i < 20; $i++) { $out = run($i); }
echo $out;
"#,
        r#"x=x19&tags%5Bq%5D=1
p%5Bx%5D=x19&p%5Btags%5D%5Bq%5D=1&d%5Ba%5D=b+c&d%5Blist%5D%5B0%5D=1&d%5Blist%5D%5B1%5D=2
[]
"#,
    );
}

/// A non-array, non-object `$data` raises PHP's TypeError; named, case-insensitive, namespaced
/// fallback, callable-string and `function_exists()` forms all reach the injected function.
#[test]
fn test_http_build_query_type_errors_named_namespaced_and_callable_calls() {
    assert_clean(
        r#"<?php
namespace App;

function run(int $i): string {
    $r = "";
    try { http_build_query("s" . $i); } catch (\TypeError $e) { $r .= $e->getMessage() . "\n"; }
    $value = $i >= 0 ? false : [1];
    try { http_build_query($value); } catch (\TypeError $e) { $r .= $e->getMessage() . "\n"; }
    $r .= HTTP_BUILD_QUERY(encoding_type: PHP_QUERY_RFC3986, data: ["k y" => "v~" . $i]) . "\n";
    $r .= \http_build_query(["a" => [1.5, -2, "x" => ["y" => true]]], arg_separator: ";") . "\n";
    $r .= call_user_func("http_build_query", ["c" => "d"]) . "\n";
    $r .= (function_exists("http_build_query") ? "exists" : "missing") . "\n";
    return $r;
}
$out = "";
for ($i = 0; $i < 20; $i++) { $out = run($i); }
echo $out;
"#,
        r#"http_build_query(): Argument #1 ($data) must be of type array, string given
http_build_query(): Argument #1 ($data) must be of type array, false given
k%20y=v~19
a%5B0%5D=1.5;a%5B1%5D=-2;a%5Bx%5D%5By%5D=1
c=d
exists
"#,
    );
}

/// `eval()` code reaches Magician's own `http_build_query()` binding, with the same output as
/// the compiled prelude and PHP, including object values and the `$data` TypeError.
#[test]
fn test_http_build_query_inside_eval() {
    let out = compile_and_run(
        r#"<?php
class O { public $x = 1; protected $y = 2; public $n = null; public $arr = ["q" => 1]; }
$code = 'return http_build_query(["a" => 1, "b" => "x y", "c" => null, "d" => true, "e" => false, "f" => 0.5, "j" => [1, ["k" => "v~"]], 5 => "five", "o" => $o], "p_", ";", PHP_QUERY_RFC3986) . "|" . http_build_query([1, "s p" => "&="]) . "|" . (function_exists("http_build_query") ? "yes" : "no");';
$o = new O;
echo eval($code), "\n";
echo eval('try { http_build_query("s"); } catch (TypeError $e) { return $e->getMessage(); } return "none";'), "\n";
echo eval('return http_build_query(new O);'), "\n";
"#,
    );
    assert_eq!(out, r#"a=1;b=x%20y;d=1;e=0;f=0.5;j%5B0%5D=1;j%5B1%5D%5Bk%5D=v~;p_5=five;o%5Bx%5D=1;o%5Barr%5D%5Bq%5D=1|0=1&s+p=%26%3D|yes
http_build_query(): Argument #1 ($data) must be of type array, string given
x=1&arr%5Bq%5D=1
"#);
}
