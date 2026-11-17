//! Purpose:
//! Regression tests for the second-round review fixes of the `ir_lower` expression, statement and
//! function lowering (chunks 191-203).
//!
//! Called from:
//! - `cargo test` through Rust's test harness.
//!
//! Key details:
//! - Every expectation is the output of `php -n` for the same program.

use crate::support::*;

/// A by-reference function with no declared type whose return infers `mixed` and that can fall off
/// its end used to fail EIR validation (`ReturnTypeMismatch`) instead of compiling.
#[test]
fn test_undeclared_by_ref_function_with_mixed_return_compiles() {
    let out = compile_and_run(
        r#"<?php
class Store { public array $data = ['k' => 5]; }
function &get(Store $s, string $k) { if (isset($s->data[$k])) { return $s->data[$k]; } }
$s = new Store;
$v = &get($s, 'k');
echo $v, "\n";
"#,
    );
    assert_eq!(out, "5\n");
}

/// A `?C` return of an owned object from a call returning bare `object` released nothing, so the
/// object's destructor never ran.
#[test]
fn test_nullable_object_return_of_an_owned_generic_object_is_released() {
    let out = compile_and_run(
        r#"<?php
class D { function __destruct() { echo "destruct\n"; } }
class G { function make(): object { return new D; } }
function g(G $f): ?D { return $f->make(); }
$x = g(new G);
$x = null;
echo "end\n";
"#,
    );
    assert_eq!(out, "destruct\nend\n");
}

/// A float stored into a declared `int` property follows php's weak mode: integral silently,
/// fractional with the deprecation (no "String offset cast" warning), NaN as a `TypeError`.
#[test]
fn test_float_into_int_property_uses_weak_mode_coercion() {
    let out = compile_and_run(
        r#"<?php
class F { public int $i = 0; }
$f = new F;
$v = 4.0 + $argc - 1;
$f->i = $v;
var_dump($f->i);
$v = NAN;
try { $f->i = $v; } catch (TypeError $e) { echo "TypeError\n"; }
foreach ([2.0 + $argc - 1] as $f->i) { var_dump($f->i); }
"#,
    );
    assert_eq!(out, "int(4)\nTypeError\nint(2)\n");
}

/// The hidden cursor rewind after `$this->p = ...` must not re-run the written property's
/// readonly guard: the first write of a readonly array in the constructor is legal.
#[test]
fn test_readonly_array_first_write_with_cursor_call_in_class() {
    let out = compile_and_run(
        r#"<?php
class A {
    public readonly array $ro;
    function __construct(array $i) { $this->ro = $i; }
    function nx() { return next($this->ro); }
}
$a = new A([1, 2, 3]);
echo count($a->ro), "\n";
"#,
    );
    assert_eq!(out, "3\n");
}

/// A reflector looked up by the public name of a function declared under a condition reports a
/// user-defined function with its real start line.
#[test]
fn test_conditional_function_reflection_reports_user_defined_lines() {
    let out = compile_cli_files_and_run(
        &[(
            "main.php",
            r#"<?php
if (!function_exists('helper')) {
    function helper() { return 1; }
}
$rf = new ReflectionFunction('helper');
var_dump($rf->isUserDefined());
var_dump($rf->getStartLine());
"#,
        )],
        "main.php",
    );
    assert_eq!(out, "bool(true)\nint(3)\n");
}

/// A deep ternary chain of heterogeneous known arm types inside a closure must type in linear
/// time: typing the arms a second time per level made it 2^depth and the build never ended.
#[test]
fn test_deep_heterogeneous_ternary_in_closure_compiles_quickly() {
    let mut expr = String::from("16");
    for i in 0..30 {
        let value = match i % 3 {
            0 => "'s'",
            1 => "2.5",
            _ => "7",
        };
        expr = format!("($x ? {value} : {expr})");
    }
    let source = format!(
        "<?php\n$f = function ($x) {{ return {expr}; }};\necho $f(0), \"\\n\";\n"
    );
    assert_eq!(compile_and_run(&source), "16\n");
}

/// A runtime-named static property read releases the owning name temporary it computed.
#[test]
fn test_runtime_named_static_property_read_releases_the_name_temporary() {
    let out = compile_and_run_with_heap_debug(
        r#"<?php
class R { public static array $a = ['x' => 1]; public static array $b = ['y' => 2]; }
$names = ['a' . $argc, 'b'];
$names[0] = 'a';
$total = 0;
for ($i = 0; $i < 4; $i++) {
    $total += count(R::${$names[$i % 2]});
}
echo $total, "\n";
"#,
    );
    assert!(out.success, "{}", out.stderr);
    assert_eq!(out.stdout, "4\n", "{}", out.stderr);
    assert!(out.stderr.contains("HEAP DEBUG: leak summary: clean"), "{}", out.stderr);
}
