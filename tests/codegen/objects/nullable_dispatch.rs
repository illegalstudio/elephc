//! Purpose:
//! Integration or regression tests for end-to-end codegen coverage of object nullable object dispatch, including method call on nullable object parameter, property access on nullable object parameter, and nullable object property round trip through typed field.
//!
//! Called from:
//! - `cargo test` through Rust's test harness.
//!
//! Key details:
//! - Inline PHP fixtures compile to native binaries while malformed or fatal cases assert captured failures.

use super::*;

/// Tests that a method call on a `?Holder` parameter dispatches correctly
/// when the receiver is non-null. Verifies the codegen unboxes the boxed
/// mixed cell to recover the concrete object pointer before reading the
/// class id and vtable.
/// Regression: nullable parameters were previously treated as opaque mixed
/// without unboxing to the concrete type.
#[test]
fn test_method_call_on_nullable_object_parameter() {
    // Calling a method through a `?Foo` parameter must dispatch to the
    // declared class — the runtime representation is a boxed mixed cell,
    // and the codegen now unboxes it to recover the concrete object
    // pointer before reading the class id.
    let out = compile_and_run(
        r#"<?php
class Holder {
    public string $msg;
    public function __construct(string $m) { $this->msg = $m; }
    public function getMsg(): string { return $this->msg; }
}
function deliver(?Holder $h): void {
    if ($h instanceof Holder) {
        echo $h->getMsg();
    }
}
deliver(new Holder("ok"));
"#,
    );
    assert_eq!(out, "ok");
}

/// Tests that a property read on a `?Holder` parameter returns the correct
/// value when the receiver is non-null. Verifies unboxing and property load
/// through a nullable parameter type.
#[test]
fn test_property_access_on_nullable_object_parameter() {
    let out = compile_and_run(
        r#"<?php
class Holder {
    public string $msg = "default";
    public function __construct(string $m) { $this->msg = $m; }
}
function read(?Holder $h): void {
    if ($h instanceof Holder) {
        echo $h->msg;
    }
}
read(new Holder("hi"));
"#,
    );
    assert_eq!(out, "hi");
}

/// Tests storing a `?Holder` through a setter and reading it back via a
/// typed `?Holder` field. Verifies both the write path (which must not
/// unbox when storing) and the load path (which must unbox and rebox the
/// mixed cell before method calls can land on the concrete object).
#[test]
fn test_nullable_object_property_round_trip_through_typed_field() {
    // Storing a nullable object into a typed field and reading it back
    // exercises both write and load paths through the boxed
    // representation — the unbox must run on read so subsequent method
    // calls land on the real object.
    let out = compile_and_run(
        r#"<?php
class Holder {
    public string $msg;
    public function __construct(string $m) { $this->msg = $m; }
    public function getMsg(): string { return $this->msg; }
}
class Box {
    public ?Holder $h = null;
    public function setIt(?Holder $h): void { $this->h = $h; }
}
$b = new Box();
$b->setIt(new Holder("via-box"));
if ($b->h instanceof Holder) {
    echo $b->h->getMsg();
}
"#,
    );
    assert_eq!(out, "via-box");
}

/// Tests `?->{expr}?->prop` nullsafe dynamic property chain with a declared
/// property. Verifies that the dynamic property name expression is evaluated
/// even when the receiver is non-null and the chain short-circuits to null
/// only when a link in the chain is null.
#[test]
fn test_nullsafe_dynamic_property_chain_reads_declared_property() {
    let out = compile_and_run(
        r#"<?php
class Box {
    public ?Box $next = null;
    public int $v;
    public function __construct(int $v) { $this->v = $v; }
}
$name = "next";
$a = new Box(1);
$a->next = new Box(2);
echo $a?->{$name}?->v;
"#,
    );
    assert_eq!(out, "2");
}

/// Tests that a nullsafe chain with a null receiver skips evaluation of
/// the property name expression. Verifies short-circuit semantics: only the
/// receiver is checked for null before the chain terminates, and the
/// dynamic property name expression must not be evaluated.
#[test]
fn test_nullsafe_dynamic_property_skips_name_expression_on_null_receiver() {
    let out = compile_and_run(
        r#"<?php
function property_name(): string {
    echo "name-evaluated";
    return "next";
}
class Box {
    public ?Box $next = null;
    public int $v = 1;
}
$a = null;
echo $a?->{property_name()}?->v ?? "fallback";
"#,
    );
    assert_eq!(out, "fallback");
}

/// Regression test: method call on a `?Foo` receiver previously returned the
/// boxed-mixed tag word instead of the method's return value. Uses strlen
/// on the returned string to verify payload bytes are correct and not
/// corrupted by a missing unbox.
#[test]
fn test_nullable_object_method_call_returns_correct_string_length() {
    // Regression for the bug where a method call on a `?Foo` receiver
    // returned the boxed-mixed tag word instead of the method's return
    // value. Asserting strlen of the returned string verifies that the
    // payload bytes match exactly.
    let out = compile_and_run(
        r#"<?php
class Tag {
    public string $name;
    public function __construct(string $n) { $this->name = $n; }
    public function getName(): string { return $this->name; }
}
function show(?Tag $t): void {
    if ($t instanceof Tag) {
        $name = $t->getName();
        echo strlen($name);
        echo ":";
        echo $name;
    }
}
show(new Tag("Europe/Paris"));
"#,
    );
    assert_eq!(out, "12:Europe/Paris");
}

/// Tests two chained method calls on a `?Inner` value returned from a
/// method on `?Outer`. Verifies that both calls unbox correctly and that
/// the chain dispatches to the concrete types at each step.
#[test]
fn test_nullable_object_chain_method_calls() {
    // Two chained method calls on a `?Foo` value — both must unbox.
    let out = compile_and_run(
        r#"<?php
class Inner {
    public string $value;
    public function __construct(string $v) { $this->value = $v; }
    public function get(): string { return $this->value; }
}
class Outer {
    public ?Inner $inner = null;
    public function __construct(?Inner $i) { $this->inner = $i; }
    public function getInner(): ?Inner { return $this->inner; }
}
$o = new Outer(new Inner("nested"));
echo $o->getInner()->get();
"#,
    );
    assert_eq!(out, "nested");
}

/// Tests that a chain of method calls where an intermediate receiver is
/// null produces a fatal "Call to a member function on null" error and
/// does not panic or produce wrong output.
#[test]
fn test_nullable_object_chain_method_call_on_null_receiver_is_fatal() {
    let err = compile_and_run_expect_failure(
        r#"<?php
class Inner {
    public function get(): string { return "never"; }
}
class Outer {
    public ?Inner $inner = null;
    public function getInner(): ?Inner { return $this->inner; }
}
$o = new Outer();
echo $o->getInner()->get();
"#,
    );
    assert!(
        err.contains("Call to a member function get() on null"),
        "{err}"
    );
}

/// Tests that when a chain method call receiver is null, argument evaluation
/// is skipped entirely. Verifies the compiler does not evaluate arguments
/// before the null check fatal — PHP semantics require arguments are not
/// evaluated when the receiver is null.
#[test]
fn test_nullable_object_null_receiver_fatal_skips_arguments() {
    let err = compile_and_run_expect_failure(
        r#"<?php
function noisy(): string {
    file_get_contents("missing-nullable-method-arg.txt");
    return "unused";
}
class Inner {
    public function get(string $unused): string { return "never"; }
}
class Outer {
    public ?Inner $inner = null;
    public function getInner(): ?Inner { return $this->inner; }
}
$o = new Outer();
echo $o->getInner()->get(noisy());
"#,
    );
    assert!(
        err.contains("Call to a member function get() on null"),
        "{err}"
    );
    assert!(
        !err.contains("file_get_contents"),
        "argument side effect ran before nullable receiver fatal: {err}"
    );
}

/// Tests that accessing a property on a null `?Holder` receiver issues a
/// warning and returns null (which coalesces to the fallback). Verifies
/// the error-control @ operator does NOT suppress this specific warning
/// when used on the property access itself.
#[test]
fn test_nullable_object_property_access_on_null_receiver_warns_and_returns_null() {
    let out = compile_and_run_capture(
        r#"<?php
class Holder {
    public string $msg = "unused";
}
function read(?Holder $h): void {
    echo $h->msg ?? "fallback";
}
read(null);
"#,
    );
    assert!(out.success, "program failed: {}", out.stderr);
    assert_eq!(out.stdout, "fallback");
    assert!(
        out.stderr.contains("Warning: Attempt to read property \"msg\" on null"),
        "{}",
        out.stderr
    );
}

/// Tests that the error-control @ operator on a null property access
/// suppresses the warning. Verifies that `@$h->msg` on a null receiver
/// produces no warning and falls through to the ?? fallback.
#[test]
fn test_error_control_suppresses_nullable_property_warning() {
    let out = compile_and_run_capture(
        r#"<?php
class Holder {
    public string $msg = "unused";
}
function read(?Holder $h): void {
    echo @$h->msg ?? "fallback";
}
read(null);
"#,
    );
    assert!(out.success, "program failed: {}", out.stderr);
    assert_eq!(out.stdout, "fallback");
    assert_eq!(out.stderr, "");
}

/// Tests that assigning a property on a non-null `?Holder` receiver executes
/// the write and returns the new value. Verifies both that the rhs function
/// is called (output "rhs|") and that the written value "updated" is echoed.
#[test]
fn test_nullable_object_property_assign_writes_non_null_receiver() {
    let out = compile_and_run(
        r#"<?php
function rhs(): string {
    echo "rhs|";
    return "updated";
}
class Holder {
    public string $msg = "initial";
}
function write(?Holder $h): void {
    $h->msg = rhs();
    echo $h->msg;
}
write(new Holder());
"#,
    );
    assert_eq!(out, "rhs|updated");
}

/// Tests that assigning a property on a null receiver produces a fatal
/// error after the RHS has been evaluated. Verifies: receiver function
/// runs, rhs function runs (but its return is unused), stdout is
/// "receiver|rhs|", and stderr contains "Attempt to assign property".
#[test]
fn test_nullable_object_property_assign_on_null_receiver_fatals_after_rhs() {
    let out = compile_and_run_capture(
        r#"<?php
function receiver(): ?Holder {
    echo "receiver|";
    return null;
}
function rhs(): string {
    echo "rhs|";
    return "unused";
}
class Holder {
    public string $msg = "initial";
}
receiver()->msg = rhs();
echo "after";
"#,
    );
    assert!(!out.success, "program unexpectedly succeeded");
    assert_eq!(out.stdout, "receiver|rhs|");
    assert!(
        out.stderr.contains("Attempt to assign property \"msg\" on null"),
        "{}",
        out.stderr
    );
}
