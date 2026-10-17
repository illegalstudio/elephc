//! Purpose:
//! Covers normal and throwing writes through nullable/mixed receivers for issue #1643.
//!
//! Called from:
//! - The runtime GC codegen integration module.
//!
//! Key details:
//! - The nested receiver (`$h->next`) is an owned `prop_get`, and the runtime-shaped store narrows
//!   its boxed value into a typed slot; both owners must be retired every iteration.

use crate::support::*;

/// A compound write through a nullable `?Box` link retires the nested receiver and the narrowed
/// box on every iteration.
#[test]
fn test_nested_nullable_property_compound_write_is_heap_clean() {
    let out = compile_and_run_with_heap_debug(
        r#"<?php
class Box { public int $n = 7; public ?Box $next = null; }
function p(Box $h): int {
    if ($h->next === null) { $h->next = new Box(); }
    $h->next->n += 1;
    return $h->next->n;
}
$s = 0;
for ($i = 0; $i < 100; $i++) { $h = new Box(); $s += p($h) + p($h); }
echo $s, "\n";
"#,
    );
    assert!(out.success, "stdout={:?}\nstderr={}", out.stdout, out.stderr);
    assert_eq!(out.stdout, "1700\n", "{}", out.stderr);
    assert!(
        out.stderr.contains("HEAP DEBUG: leak summary: clean"),
        "{}",
        out.stderr
    );
}

/// Retires both in-flight owners when a nullable receiver fails its typed-property guard.
#[test]
fn test_nested_nullable_property_throwing_write_is_heap_clean() {
    let out = compile_and_run_with_heap_debug(
        r#"<?php
class Box { public int $n = 7; public ?Box $next = null; }
function bad_value(): mixed { return "nope"; }
function reject(Box $h): int {
    if ($h->next === null) { $h->next = new Box(); }
    try { $h->next->n = "nope"; }
    catch (TypeError $e) {}
    try { $h->next->n = bad_value(); }
    catch (TypeError $e) { return $h->next->n; }
    return 0;
}
$sum = 0;
for ($i = 0; $i < 30; $i++) { $h = new Box(); $sum += reject($h); }
echo $sum, "\n";
"#,
    );
    assert!(out.success, "stdout={:?}\nstderr={}", out.stdout, out.stderr);
    assert_eq!(out.stdout, "210\n", "{}", out.stderr);
    assert!(
        out.stderr.contains("HEAP DEBUG: leak summary: clean"),
        "{}",
        out.stderr
    );
}

/// Retires a boxed string after the nullable-receiver store copies its payload.
#[test]
fn test_nested_nullable_property_string_write_is_heap_clean() {
    let out = compile_and_run_with_heap_debug(
        r#"<?php
class Box { public string $s = ""; public ?Box $next = null; }
function write(Box $h): int {
    if ($h->next === null) { $h->next = new Box(); }
    $h->next->s = "xy";
    return strlen($h->next->s);
}
$sum = 0;
for ($i = 0; $i < 40; $i++) { $h = new Box(); $sum += write($h); }
echo $sum, "\n";
"#,
    );
    assert!(out.success, "stdout={:?}\nstderr={}", out.stdout, out.stderr);
    assert_eq!(out.stdout, "80\n", "{}", out.stderr);
    assert!(
        out.stderr.contains("HEAP DEBUG: leak summary: clean"),
        "{}",
        out.stderr
    );
}

/// Runtime-shaped stores retire fresh Stringable sources after publishing the string payload.
#[test]
fn test_runtime_shaped_property_stringable_sources_are_heap_clean() {
    let prefix = r#"<?php
class T { public string $s = "old"; public ?T $next = null; }
class Label {
    public function __construct(private T $o) {}
    public function __toString(): string { return "new"; }
    public function __destruct() { echo "drop:", $this->o->s, "\n"; }
}
function make_label(T $o): mixed { return new Label($o); }
"#;
    for body in [
        r#"function write(T $o): void { $o->s = make_label($o); }
$o = new T(); write($o); echo "stored:", $o->s, "\n";
"#,
        r#"function write(mixed $o, T $owner): void { $o->s = make_label($owner); }
$o = new T(); write($o, $o); echo "stored:", $o->s, "\n";
"#,
        r#"function write(T $h): void {
    $next = new T(); $h->next = $next; $h->next->s = make_label($next);
}
$h = new T(); write($h); echo "stored:", $h->next->s, "\n";
"#,
    ] {
        let out = compile_and_run_with_heap_debug(&format!("{}{}", prefix, body));
        assert!(out.success, "stdout={:?}\nstderr={}", out.stdout, out.stderr);
        assert_eq!(out.stdout, "drop:new\nstored:new\n", "{}", out.stderr);
        assert!(out.stderr.contains("HEAP DEBUG: leak summary: clean"), "{}", out.stderr);
    }
}

/// A named write through a mixed receiver retires the call's returned box on every iteration.
#[test]
fn test_mixed_property_fresh_stringable_call_is_heap_clean() {
    let out = compile_and_run_with_heap_debug(r#"<?php
class Label {
    public function __construct(private string $v) {}
    public function __toString(): string { return $this->v; }
}
function m(int $i): mixed { return new Label("o" . $i); }
class T { public string $s = ""; }
function w(mixed $o, int $i): void { $o->s = m($i); }
$t = new T();
for ($i = 0; $i < 40; $i++) { w($t, $i); }
echo $t->s, "\n";
"#);
    assert!(out.success, "stdout={:?}\nstderr={}", out.stdout, out.stderr);
    assert_eq!(out.stdout, "o39\n", "{}", out.stderr);
    assert!(out.stderr.contains("HEAP DEBUG: leak summary: clean"), "{}", out.stderr);
}
