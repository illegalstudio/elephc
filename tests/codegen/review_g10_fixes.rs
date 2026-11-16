//! Purpose:
//! Regression tests for the group-10 review findings: protected first-class callables inside
//! closures, owned cast-string leaks, and php divergences in filter_var, substr_compare, sscanf,
//! unpack, callable string use, spl_object_id and dynamic member_exists.
//!
//! Called from:
//! - The codegen integration harness (`tests/codegen/mod.rs`).
//!
//! Key details:
//! - Expected values were measured with `php -n` (8.5.10).

use crate::support::*;

/// A protected method callable created inside an arrow function resolves through the lexical class.
#[test]
fn test_protected_first_class_callable_inside_closure() {
    let out = compile_and_run_with_heap_debug(r#"<?php
class A { protected function m() { return "A"; }
          public function f() { return (fn() => $this->m(...))(); } }
class B extends A { protected function m() { return "B"; } }
echo ((new B)->f())(), "\n";
"#);
    assert!(out.success, "stdout={:?}\nstderr={}", out.stdout, out.stderr);
    assert_eq!(out.stdout, "B\n", "{}", out.stderr);
}

/// Mixed string names given to filter_var/defined/constant/setlocale leave no owned copy behind.
#[test]
fn test_mixed_cast_strings_do_not_leak() {
    let out = compile_and_run_with_heap_debug(r#"<?php
define('G10_C', 5);
function f(mixed $v) { return filter_var($v); }
function d(mixed $n) { return defined($n) ? constant($n) : null; }
function s(mixed $l) { return setlocale(LC_NUMERIC, $l); }
$n = $argc > 5 ? 1 : 3;
for ($i = 0; $i < $n; $i++) { f("abc" . $i); d("G10_C"); s("C"); }
var_dump(f("abc"), d("G10_C"), s("C"));
"#);
    assert!(out.success, "stdout={:?}\nstderr={}", out.stdout, out.stderr);
    assert_eq!(out.stdout, "string(3) \"abc\"\nint(5)\nstring(1) \"C\"\n", "{}", out.stderr);
    assert!(out.stderr.contains("HEAP DEBUG: leak summary: clean"), "{}", out.stderr);
}

/// filter_var validates a float through its (string) form and refuses an object without __toString.
#[test]
fn test_filter_var_float_string_form_and_objects() {
    let out = compile_and_run_with_heap_debug(r#"<?php
class WithStr { function __toString(): string { return "xyz"; } }
function f(mixed $v) { var_dump(filter_var($v)); }
var_dump(filter_var(1e15, FILTER_VALIDATE_INT));
var_dump(filter_var(123456789012345.0, FILTER_VALIDATE_INT));
var_dump(filter_var(3.0, FILTER_VALIDATE_INT));
var_dump(filter_var(0.1 + 0.2, FILTER_VALIDATE_FLOAT));
var_dump(filter_var(-0.0, FILTER_VALIDATE_BOOL, FILTER_NULL_ON_FAILURE));
var_dump(filter_var(1.0, FILTER_VALIDATE_BOOL));
f(new stdClass);
f(new WithStr);
"#);
    assert!(out.success, "stdout={:?}\nstderr={}", out.stdout, out.stderr);
    assert_eq!(
        out.stdout,
        "bool(false)\nbool(false)\nint(3)\nfloat(0.3)\nNULL\nbool(true)\nbool(false)\nstring(3) \"xyz\"\n",
        "{}",
        out.stderr
    );
}

/// An explicit zero length answers 0 before the offset is validated.
#[test]
fn test_substr_compare_zero_length_ignores_bad_offset() {
    let out = compile_and_run(r#"<?php
var_dump(substr_compare("abc", "a", 100, 0));
try { substr_compare("abc", "a", 100, 1); } catch (ValueError $e) { echo "VE\n"; }
"#);
    assert_eq!(out, "int(0)\nVE\n");
}

/// sscanf leaves outputs without a scanned value untouched.
#[test]
fn test_sscanf_keeps_unmatched_outputs() {
    let out = compile_and_run_with_heap_debug(r#"<?php
$b = "keep";
$n = sscanf("12", "%d %d", $a, $b);
var_dump($n, $a, $b);
"#);
    assert!(out.success, "stdout={:?}\nstderr={}", out.stdout, out.stderr);
    assert_eq!(out.stdout, "int(1)\nint(12)\nstring(4) \"keep\"\n", "{}", out.stderr);
    assert!(out.stderr.contains("HEAP DEBUG: leak summary: clean"), "{}", out.stderr);
}

/// unpack raises php's ValueError for a bad offset and warns for truncated input.
#[test]
fn test_unpack_offset_errors_and_truncation_warning() {
    let out = compile_and_run(r#"<?php
function off(mixed $o) { return $o; }
foreach ([9, -1, 5] as $o) {
    try { unpack('N', 'abcd', $o); } catch (ValueError $e) { echo $e->getMessage(), "\n"; }
}
try { unpack('n*', 'abc', off(9)); } catch (ValueError $e) { echo "VE seq\n"; }
try { unpack('N', 'abcd', off(9)); } catch (ValueError $e) { echo "VE mixed\n"; }
set_error_handler(function ($no, $msg) { echo $msg, "\n"; return true; });
var_dump(unpack('N', 'ab'));
var_dump(unpack('Nx/Ny', 'abcdefg'));
var_dump(unpack('N', 'abcd', 4));
"#);
    assert_eq!(
        out,
        "unpack(): Argument #3 ($offset) must be contained in argument #2 ($data)\n\
         unpack(): Argument #3 ($offset) must be contained in argument #2 ($data)\n\
         unpack(): Argument #3 ($offset) must be contained in argument #2 ($data)\n\
         VE seq\nVE mixed\n\
         unpack(): Type N: not enough input values, need 4 values but only 2 were provided\nbool(false)\n\
         unpack(): Type N: not enough input values, need 4 values but only 3 were provided\nbool(false)\n\
         unpack(): Type N: not enough input values, need 4 values but only 0 were provided\nbool(false)\n"
    );
}

/// A closure used as a string raises Error; spl_object_id works on callables; member_exists names the type.
#[test]
fn test_callable_string_use_spl_id_and_member_exists_text() {
    let out = compile_and_run(r#"<?php
$f = function () {};
function id(mixed $o) { return spl_object_id($o); }
var_dump(spl_object_id($f) === id($f), spl_object_id($f) < 1000);
function me(mixed $o) { try { method_exists($o, 'x'); } catch (TypeError $e) { echo $e->getMessage(), "\n"; } }
me(42);
function ids(mixed $o) { try { spl_object_id($o); } catch (TypeError $e) { echo $e->getMessage(), "\n"; } }
ids("x");
"#);
    assert_eq!(
        out,
        "bool(true)\nbool(true)\nmethod_exists(): Argument #1 ($object_or_class) must be of type object|string, int given\nspl_object_id(): Argument #1 ($object) must be of type object, string given\n"
    );
}
