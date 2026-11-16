//! Purpose:
//! Regression tests for method dispatch on values typed `object` or `?object`, whose class is
//! known only at run time: dynamic method lookup, undefined-method and null-receiver errors,
//! and nullsafe short-circuiting.
//!
//! Called from:
//! - `cargo test` through Rust's test harness (via `property_access/mod.rs`).
//!
//! Key details:
//! - Error messages must name the runtime class of the receiver, as php does, never a
//!   compile-time candidate class.
//! - Expected output matches php for the same fixture.

use super::*;

/// Verifies calling an undefined method on an `object`-typed return value throws `Error` naming
/// the runtime class: "Call to undefined method GenericRawMissingReceiver::missing()".
#[test]
fn test_generic_object_missing_method_reports_runtime_class() {
    let out = compile_and_run(r#"<?php
class GenericRawMissingReceiver {}
function genericRawMissingReceiver(): object { return new GenericRawMissingReceiver(); }
try { genericRawMissingReceiver()->missing(); }
catch (Error $error) { echo $error->getMessage(); }
"#);
    assert_eq!(out, "Call to undefined method GenericRawMissingReceiver::missing()");
}

/// Verifies a `?object` receiver is not dispatched to the only class that declares `selected()`
/// when the runtime object is another class: the call throws "Call to undefined method
/// GenericOtherReceiver::selected()" instead of running `GenericCandidate::selected()`.
#[test]
fn test_nullable_generic_object_rejects_nonmatching_method_candidate() {
    let out = compile_and_run(r#"<?php
class GenericCandidate { public function selected(): int { return 1; } }
class GenericOtherReceiver {}
function genericCandidate(bool $matching): ?object {
    return $matching ? new GenericCandidate() : new GenericOtherReceiver();
}
try { genericCandidate(false)->selected(); }
catch (Error $error) { echo $error->getMessage(); }
"#);
    assert_eq!(out, "Call to undefined method GenericOtherReceiver::selected()");
}

/// Verifies calling an undefined method on a `?object` return value throws `Error` naming the
/// runtime class: "Call to undefined method GenericMissingReceiver::missing()".
#[test]
fn test_nullable_generic_object_missing_method_reports_runtime_class() {
    let out = compile_and_run(r#"<?php
class GenericMissingReceiver {}
function genericMissingReceiver(): ?object { return new GenericMissingReceiver(); }
try { genericMissingReceiver()->missing(); }
catch (Error $error) { echo $error->getMessage(); }
"#);
    assert_eq!(out, "Call to undefined method GenericMissingReceiver::missing()");
}

/// Verifies a method call on a non-null `?object` return value dispatches to the runtime class's
/// method with its argument, printing "yes".
#[test]
fn test_nullable_generic_object_return_dispatches_runtime_method() {
    let out = compile_and_run(r#"<?php
class GenericReceiver {
    public function has(string $name): bool { return $name === 'item'; }
}
function makeGenericReceiver(): ?object { return new GenericReceiver(); }
echo makeGenericReceiver()->has('item') ? 'yes' : 'no';
"#);
    assert_eq!(out, "yes");
}

/// Verifies `?->` on a `?object` return value short-circuits on null without evaluating the
/// argument expression, and evaluates it once on an object. PHP outputs "0|arg:1".
#[test]
fn test_nullable_generic_object_nullsafe_call_skips_arguments() {
    let out = compile_and_run(r#"<?php
class GenericNullableReceiver {
    public function has(string $name): bool { return $name === 'item'; }
}
function genericReceiver(bool $present): ?object { return $present ? new GenericNullableReceiver() : null; }
function argument(): string { echo 'arg:'; return 'item'; }
echo (int) genericReceiver(false)?->has(argument()), '|';
echo (int) genericReceiver(true)?->has(argument());
"#);
    assert_eq!(out, "0|arg:1");
}

/// Verifies a plain `->` call on a `?object` return value that is null throws `Error` with
/// "Call to a member function has() on null" rather than crashing.
#[test]
fn test_nullable_generic_object_null_method_call_throws_error() {
    let out = compile_and_run(r#"<?php
class GenericNullReceiver { public function has(string $name): bool { return true; } }
function genericNullReceiver(bool $present): ?object { return $present ? new GenericNullReceiver() : null; }
try { genericNullReceiver(false)->has('item'); }
catch (Error $error) { echo $error->getMessage(); }
"#);
    assert_eq!(out, "Call to a member function has() on null");
}
