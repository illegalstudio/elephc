//! Purpose:
//! Covers the callable forms `Closure::fromCallable()` accepts and the identity of the closures
//! it returns.
//!
//! Called from:
//! - The codegen integration test suite.
//!
//! Key details:
//! - Every fixture runs with the EIR optimizer on and off, with heap debug enabled.
//! - Expected output is what PHP 8.5 prints for the unchanged source.

use crate::support::*;

/// Asserts PHP-equivalent stdout and a clean heap with the EIR optimizer on and off.
fn assert_factory_output(source: &str, expected: &str) {
    for ir_opt in [true, false] {
        let out = if ir_opt {
            compile_and_run_with_heap_debug(source)
        } else {
            without_ir_opt(|| compile_and_run_with_heap_debug(source))
        };
        assert!(out.success, "ir_opt={ir_opt} stdout={:?}\nstderr={}", out.stdout, out.stderr);
        assert_eq!(out.stdout, expected, "ir_opt={ir_opt}\n{}", out.stderr);
        assert!(
            out.stderr.contains("HEAP DEBUG: leak summary: clean"),
            "ir_opt={ir_opt}\n{}",
            out.stderr
        );
    }
}

/// A literal `"Class::method"` string names a static method, resolved with the visibility of
/// the class scope that calls the factory.
#[test]
fn test_closure_factory_literal_class_method_string() {
    assert_factory_output(r#"<?php
namespace App;
class K {
    public static function s(int $n): int { return $n + 3; }
    private static function hidden(): string { return "hidden"; }
    public static function expose(): \Closure { return \Closure::fromCallable("App\\K::hidden"); }
    public static function selfForm(): \Closure { return \Closure::fromCallable(self::class . "::hidden"); }
}
function f(): string { return "f"; }
$a = \Closure::fromCallable("App\\K::s"); echo $a(1), "\n";
$b = \Closure::fromCallable("\\App\\K::s"); echo $b(2), "\n";
$c = \Closure::fromCallable('App\K::s'); echo $c(3), "\n";
echo K::expose()(), "\n";
echo K::selfForm()(), "\n";
try { \Closure::fromCallable("App\\K::hidden"); echo "bad\n"; } catch (\TypeError $e) { echo "TypeError\n"; }
try { \Closure::fromCallable("App\\K::absent"); echo "bad\n"; } catch (\TypeError $e) { echo "TypeError\n"; }
$d = \Closure::fromCallable("App\\f"); echo $d(), "\n";
$u = \Closure::fromCallable("strtoupper"); echo $u("x"), "\n";
echo implode(",", array_map(\Closure::fromCallable("App\\K::s"), [1, 2])), "\n";
"#, "4\n5\n6\nhidden\nhidden\nTypeError\nTypeError\nf\nX\n4,5\n");
}
