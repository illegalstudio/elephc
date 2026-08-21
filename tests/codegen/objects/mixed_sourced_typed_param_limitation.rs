//! Purpose:
//! Regression coverage for objects that reach a typed call parameter through `mixed` storage.
//!
//! Called from:
//! - `cargo test` through Rust's test harness.
//!
//! Key details:
//! - The checker accepts the typed call after `instanceof` narrowing, while EIR still carries
//!   the source in the boxed `Mixed` representation used by array and property storage.
//! - Call materialization must unbox that source before loading the typed-object ABI slot.
//! - Curl's guarded multi-handle path depends on this after its public `mixed` parameter has
//!   been narrowed; the public signature stays `mixed` because the checker still rejects an
//!   unguarded Mixed array element where a typed object parameter is declared.

use crate::support::*;

/// The minimal repro: one object, stored in an array property, read back, and handed to a
/// typed parameter versus a `mixed` one.
///
/// Expected output of the same source under real PHP 8.5:
///
/// ```text
/// identity=same
/// direct-typed=resource/1
/// mixed-param=resource/1
/// typed-param=resource/1
/// ```
///
const MIXED_SOURCED_OBJECT_SOURCE: &str = r#"<?php
final class Holder {
    public mixed $payload = null;
    public bool $flag = false;
    public static function of(mixed $value): Holder { $h = new self(); $h->payload = $value; $h->flag = true; return $h; }
}
final class Registry {
    public array $items = [];
    public function add(Holder $h): void { $this->items[] = $h; }
}
function read_typed(Holder $h): string { $p = $h->payload; return gettype($p) . "/" . ($h->flag ? "1" : "0"); }
function read_mixed(mixed $h): string { $p = $h->payload; return gettype($p) . "/" . ($h->flag ? "1" : "0"); }
$stream = fopen("php://memory", "r+");
$holder = Holder::of($stream);
$registry = new Registry();
$registry->add($holder);
$fromProperty = $registry->items[0];
echo "identity=", ($fromProperty === $holder ? "same" : "other"), "\n";
echo "direct-typed=", read_typed($holder), "\n";
echo "mixed-param=", read_mixed($fromProperty), "\n";
if ($fromProperty instanceof Holder) { echo "typed-param=", read_typed($fromProperty), "\n"; }
"#;

/// Verifies that a Mixed-sourced object reaches a typed parameter as its object payload.
#[test]
fn test_mixed_sourced_object_through_typed_param_is_unboxed() {
    let out = compile_and_run(MIXED_SOURCED_OBJECT_SOURCE);
    assert_eq!(
        out,
        "identity=same\ndirect-typed=resource/1\nmixed-param=resource/1\ntyped-param=resource/1\n"
    );
}

/// Verifies that an explicit Mixed parameter can still guard and read an object dynamically.
#[test]
fn test_mixed_param_with_instanceof_guard_reads_correctly() {
    let out = compile_and_run(
        r#"<?php
final class Holder {
    public mixed $payload = null;
    public static function of(mixed $value): Holder { $h = new self(); $h->payload = $value; return $h; }
}
function guarded(mixed $h): string {
    if (!($h instanceof Holder)) {
        throw new \TypeError("guarded(): Argument #1 (\$h) must be of type Holder");
    }
    $p = $h->payload;
    return gettype($p);
}
$stream = fopen("php://memory", "r+");
$items = [Holder::of($stream)];
echo guarded($items[0]), "\n";
try { guarded(42); } catch (\TypeError $e) { echo "TypeError\n"; }
"#,
    );
    assert_eq!(out, "resource\nTypeError\n");
}
