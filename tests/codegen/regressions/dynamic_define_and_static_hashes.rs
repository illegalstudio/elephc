//! Purpose:
//! Regression tests for `define()` with a computed name and for function-static hashes holding
//! `mixed` values.
//!
//! Called from:
//! - `cargo test` through Rust's test harness.
//!
//! Key details:
//! - `define($name, ...)` was refused at compile time ("define() first argument must be a string
//!   literal"); computed names now live in a run-time table (`crate::dynamic_define_prelude`)
//!   that `defined()`, `constant()` and bare reads consult after the compiled constants.
//! - A boxed static's load takes no reference and its store releases the previous box, so the
//!   frame-slot mutation sequence freed it twice: `static $v = []; $v[$name] = $mixed;` rebuilt
//!   its hash from freed storage and stored garbage keys on the second call.

use crate::support::*;

/// `define()` with computed names, in a loop and in a helper, is visible to bare reads,
/// `defined()` and `constant()`; a second definition warns and keeps the first value.
#[test]
fn test_define_with_a_computed_name() {
    let out = compile_and_run(
        r#"<?php
$name = "APP_" . "MODE";
define($name, "prod");
echo APP_MODE, " ", constant($name), " ", defined($name) ? "y" : "n", " ", defined("APP_MODE") ? "y" : "n", "\n";
foreach (["A1" => 1, "A2" => [1, 2]] as $k => $v) { define($k, $v); }
echo A1, count(A2), "\n";
function def(string $n, $v): bool { return define($n, $v); }
var_dump(def("FROM_FN", 3.5), FROM_FN);
var_dump(@def("A1", 9), A1);
var_dump(defined("NEVER_" . "SET"), defined("PHP_EOL"));
try { echo constant("NEVER_" . "SET"); } catch (Error $e) { echo get_class($e), ": ", $e->getMessage(), "\n"; }
if (!defined("LAZY")) { define("LA" . "ZY", "z"); }
echo LAZY, "\n";
var_dump(define(42, 100), constant("42"));
"#,
    );
    assert_eq!(out, "prod prod y y\n12\nbool(true)\nfloat(3.5)\nbool(false)\nint(1)\nbool(false)\nbool(true)\nError: Undefined constant \"NEVER_SET\"\nz\nbool(true)\nint(100)\n");
}

/// A function-static hash written with `mixed` values keeps its keys across calls.
#[test]
fn test_static_hash_with_mixed_values_keeps_its_keys() {
    let out = compile_and_run(
        r#"<?php
function tab(int $op, string $name, mixed $value): mixed {
    static $values = [];
    if ($op === 0) {
        if (array_key_exists($name, $values)) { return false; }
        $values[$name] = $value;
        return true;
    }
    if ($op === 1) { return isset($values[$name]); }
    return $values[$name] ?? "none";
}
var_dump(tab(0, "A", "prod"), tab(0, "A", "again"), tab(2, "A", null));
var_dump(tab(0, "B", [1, 2]), tab(1, "B", null), tab(2, "B", null), tab(2, "C", null));
function memo(string $k): string {
    static $cache = [];
    if (isset($cache[$k])) { return "hit:" . $cache[$k]; }
    $cache[$k] = strtoupper($k) . count($cache);
    return "miss:" . $cache[$k];
}
for ($i = 0; $i < 3; $i++) { echo memo("a"), " ", memo("b" . $i), "\n"; }
echo json_encode(tab(2, "A", null)), "\n";
"#,
    );
    assert_eq!(out, "bool(true)\nbool(false)\nstring(4) \"prod\"\nbool(true)\nbool(true)\narray(2) {\n  [0]=>\n  int(1)\n  [1]=>\n  int(2)\n}\nstring(4) \"none\"\nmiss:A0 miss:B01\nhit:A0 miss:B12\nhit:A0 miss:B23\n\"prod\"\n");
}

/// Writing a boxed static hash neither clones it per write nor leaks the replaced container.
#[test]
fn test_static_hash_writes_are_leak_free() {
    let out = compile_and_run_with_heap_debug(
        r#"<?php
function run(): void {
    for ($i = 0; $i < 6; $i++) {
        echo f(), memo("k" . ($i % 2)), "|";
    }
    echo "\n";
}
function f() { static $v = []; $v["k"] = "x" . mt_rand(1, 1); return count($v); }
function memo(string $k): string { static $cache = []; if (isset($cache[$k])) { return $cache[$k]; } $cache[$k] = [$k, "v"][1] . strlen($k); return $cache[$k]; }
run();
"#,
    );
    assert!(out.success, "program failed: {}", out.stderr);
    assert_eq!(out.stdout, "1v2|1v2|1v2|1v2|1v2|1v2|\n");
    assert!(
        out.stderr.contains("HEAP DEBUG: leak summary: clean"),
        "expected a clean heap, got: {}",
        out.stderr
    );
}
