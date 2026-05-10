//! Purpose:
//! Integration or regression tests for end-to-end codegen coverage of object nullable object dispatch, including method call on nullable object parameter, property access on nullable object parameter, and nullable object property round trip through typed field.
//!
//! Called from:
//! - `cargo test` through Rust's test harness.
//!
//! Key details:
//! - Inline PHP fixtures compile to native binaries while malformed or fatal cases assert captured failures.

use super::*;

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
