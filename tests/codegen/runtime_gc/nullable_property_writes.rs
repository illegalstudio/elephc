//! Purpose:
//! Regression tests for adopted scalar property-store boxes through nullable receivers.
//!
//! Called from:
//! - The runtime GC codegen integration suite.
//!
//! Key details:
//! - Looped stores assert both PHP-visible values and a clean heap.
//! - Mixed dispatch, borrowed sources, and throwing guards must keep one cleanup owner.

use crate::support::compile_and_run_with_heap_debug;

/// Checks observable output, successful execution, and ownership after a store loop.
fn assert_clean(source: &str, expected: &str) {
    let out = compile_and_run_with_heap_debug(source);
    assert!(out.success, "program failed: {}", out.stderr);
    assert_eq!(out.stdout, expected, "stderr: {}", out.stderr);
    assert!(out.stderr.contains("HEAP DEBUG: leak summary: clean"), "{}", out.stderr);
}

/// Pins scalar slots without leaking or releasing a caller owner.
#[test]
fn test_nullable_property_write_scalar_slots() {
    assert_clean(
        r#"<?php
class Box { public int $n = 7; public bool $b = false; public float $f = 0.0; }
function write(?Box $o, int $i): void { $o->n = $i; $o->b = $i % 2 === 0; $o->f = $i + 0.5; }
$o = new Box();
for ($i = 0; $i < 40; $i++) { write($o, $i); }
var_dump($o->n, $o->b, $o->f);
"#,
        "int(39)\nbool(false)\nfloat(39.5)\n",
    );
}

/// Pins coalesced receiver without leaking or releasing a caller owner.
#[test]
fn test_nullable_property_write_coalesced_receiver() {
    assert_clean(
        r#"<?php
class Box { public int $n = 7; public bool $b = false; public float $f = 0.0; }
function write(?Box $o, int $i): void { $target = $o ?? null; $target->n = $i; $target->b = $i % 2 === 0; $target->f = $i + 0.5; }
$o = new Box();
for ($i = 0; $i < 40; $i++) { write($o, $i); }
var_dump($o->n, $o->b, $o->f);
"#,
        "int(39)\nbool(false)\nfloat(39.5)\n",
    );
}

/// Pins fresh mixed source without leaking or releasing a caller owner.
#[test]
fn test_nullable_property_write_fresh_mixed_source() {
    assert_clean(
        r#"<?php
class Box { public int $n = 7; public bool $b = false; public float $f = 0.0; }
function value(int $i): mixed { return $i; }
function write(?Box $o, int $i): void { $o->n = value($i); }
$o = new Box();
for ($i = 0; $i < 40; $i++) { write($o, $i); }
var_dump($o->n);
"#,
        "int(39)\n",
    );
}

/// Pins borrowed mixed source without leaking or releasing a caller owner.
#[test]
fn test_nullable_property_write_borrowed_mixed_source() {
    assert_clean(
        r#"<?php
class Box { public int $n = 7; public bool $b = false; public float $f = 0.0; }
function write(?Box $o, mixed $v): void { $o->n = $v; }
function probe(mixed $v): void {
    $o = new Box();
    for ($i = 0; $i < 40; $i++) { write($o, $v); }
    var_dump($v, $o->n);
}
probe("17");
"#,
        "string(2) \"17\"\nint(17)\n",
    );
}

/// Pins weak scalar coercions without leaking or releasing a caller owner.
#[test]
fn test_nullable_property_write_weak_scalar_coercions() {
    assert_clean(
        r#"<?php
class Box { public int $n = 7; public bool $b = false; public float $f = 0.0; }
function write(?Box $o, mixed $v): void { $o->n = $v; $o->b = $v; $o->f = $v; }
$o = new Box();
for ($i = 0; $i < 40; $i++) { write($o, "17"); }
var_dump($o->n, $o->b, $o->f);
"#,
        "int(17)\nbool(true)\nfloat(17)\n",
    );
}

/// Pins mixed named dispatch without leaking or releasing a caller owner.
#[test]
fn test_nullable_property_write_mixed_named_dispatch() {
    assert_clean(
        r#"<?php
class Box { public int $n = 7; public bool $b = false; public float $f = 0.0; }
function write(mixed $o, int $i): void { $o->n = $i; $o->b = $i % 2 === 0; $o->f = $i + 0.5; }
$o = new Box();
for ($i = 0; $i < 40; $i++) { write($o, $i); }
var_dump($o->n, $o->b, $o->f);
"#,
        "int(39)\nbool(false)\nfloat(39.5)\n",
    );
}

/// Pins runtime name dispatch without leaking or releasing a caller owner.
#[test]
fn test_nullable_property_write_runtime_name_dispatch() {
    assert_clean(
        r#"<?php
class Box { public int $n = 7; public bool $b = false; public float $f = 0.0; }
function write(?Box $o, string $key, mixed $v): void { $o->$key = $v; }
$o = new Box();
for ($i = 0; $i < 40; $i++) { write($o, "n", $i); write($o, "b", $i % 2 === 0); write($o, "f", $i + 0.5); }
var_dump($o->n, $o->b, $o->f);
"#,
        "int(39)\nbool(false)\nfloat(39.5)\n",
    );
}

/// Pins rejected store without leaking or releasing a caller owner.
#[test]
fn test_nullable_property_write_rejected_store() {
    assert_clean(
        r#"<?php
class Box { public int $n = 7; public bool $b = false; public float $f = 0.0; }
function write(?Box $o, mixed $v): void { $o->n = $v; }
$o = new Box();
$caught = 0;
for ($i = 0; $i < 40; $i++) {
    try { write($o, "invalid" . $i); }
    catch (TypeError $error) { $caught++; unset($error); }
}
echo $caught, ":", $o->n, "\n";
"#,
        "40:7\n",
    );
}

/// Pins false slot without leaking or releasing a caller owner.
#[test]
fn test_nullable_property_write_false_slot() {
    assert_clean(
        r#"<?php
class Box { public false $b; }
function write(?Box $o, mixed $v): void { $o->b = $v; }
function direct(?Box $o): void { $o->b = false; }
$o = new Box();
for ($i = 0; $i < 40; $i++) { direct($o); write($o, false); }
var_dump($o->b);
"#,
        "bool(false)\n",
    );
}

/// Pins false rejection without leaking or releasing a caller owner.
#[test]
fn test_nullable_property_write_false_rejection() {
    assert_clean(
        r#"<?php
class Box { public false $b; }
function write(?Box $o, mixed $v): void { $o->b = $v; }
$o = new Box();
write($o, false);
$caught = 0;
for ($i = 0; $i < 40; $i++) {
    try { write($o, true); }
    catch (TypeError $error) { $caught++; unset($error); }
}
echo $caught, ":";
var_dump($o->b);
"#,
        "40:bool(false)\n",
    );
}

/// Pins refined untyped without leaking or releasing a caller owner.
#[test]
fn test_nullable_property_write_refined_untyped() {
    assert_clean(
        r#"<?php
class Box { public $n = 7; public $b = false; public $f = 0.0; }
function write(mixed $o, int $i): void { $o->n = $i; $o->b = $i % 2 === 0; $o->f = $i + 0.5; }
$o = new Box();
for ($i = 0; $i < 40; $i++) { write($o, $i); }
var_dump($o->n, $o->b, $o->f);
"#,
        "int(39)\nbool(false)\nfloat(39.5)\n",
    );
}
