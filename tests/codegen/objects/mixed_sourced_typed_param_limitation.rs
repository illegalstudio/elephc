//! Purpose:
//! Pins a KNOWN MISCOMPILE, not a desired behaviour: an object that reaches a call site as a
//! `mixed` (read out of an array, an array property, or any other Mixed-typed storage) and is
//! then passed to a parameter TYPED with its class arrives as the boxed Mixed cell instead of
//! the object, so every property read inside the callee reads the cell's header where the
//! object's slots should be.
//!
//! Called from:
//! - `cargo test` through Rust's test harness.
//!
//! Key details:
//! - **THE ASSERTIONS BELOW DESCRIBE WRONG OUTPUT ON PURPOSE.** Real PHP 8.5 answers
//!   `resource/1` for every read in the fixture; elephc answers `NULL/0` through the typed
//!   parameter. When the miscompile is fixed, this file MUST be updated (the expectations
//!   become PHP's) rather than deleted — the test failing is the signal that it was fixed.
//! - WHY IT IS PINNED AT ALL: the curl multi surface works around it. `curl_multi_info_read()`
//!   returns its `handle` inside a PHP array, so every handle a caller pulls out of that array
//!   is a `mixed`; `curl_multi_add_handle()` / `curl_multi_remove_handle()` /
//!   `curl_multi_getcontent()` therefore declare `mixed $handle` plus a runtime `instanceof`
//!   guard instead of php-src's `CurlHandle $handle` (see `src/curl_prelude.rs`'s header).
//!   Tasks that add `CurlShareHandle` / `CURLFile` will meet the same wall. Without this pin
//!   the workaround looks like an arbitrary divergence.
//! - THE CHECKER ALREADY REFUSES the unguarded form (`Mixed` where `Object(C)` is expected is
//!   a compile error), so the ONLY way to reach the miscompile is the `instanceof` narrowing
//!   the checker accepts — which is what the fixture does, and what makes it silent.
//! - IDENTITY IS NOT AFFECTED: `===` compares the same object either way, which is why the
//!   curl identity map is sound while property reads through a typed parameter are not.

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
/// elephc differs on the LAST line only.
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

/// KNOWN LIMITATION PIN — the last line is WRONG and this test asserts the wrong value.
///
/// `typed-param=NULL/0` is elephc reading the boxed Mixed cell's own words (its tag and its
/// payload pointer) as if they were `Holder`'s `$payload` and `$flag` slots. PHP 8.5 prints
/// `typed-param=resource/1`. Update this expectation — do not delete the test — when the
/// backend passes the object rather than its box.
#[test]
fn test_mixed_sourced_object_through_typed_param_is_miscompiled() {
    let out = compile_and_run(MIXED_SOURCED_OBJECT_SOURCE);
    assert_eq!(
        out,
        "identity=same\ndirect-typed=resource/1\nmixed-param=resource/1\ntyped-param=NULL/0\n",
        "this fixture PINS a known miscompile: PHP answers typed-param=resource/1. \
         If this assertion now fails with the PHP value, the backend was fixed — update \
         the expectation here and drop the `mixed $handle` workaround documented in \
         src/curl_prelude.rs"
    );
}

/// The workaround the curl prelude uses, pinned as WORKING: the same object through a
/// `mixed` parameter with a runtime `instanceof` guard reads correctly, and rejects a
/// non-object argument the way a declared type would.
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
