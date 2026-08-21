//! Purpose:
//! Regression coverage for PHP `__toString` return contracts and the distinct
//! SimpleXML `current()` tentative-return exception.
//!
//! Called from:
//! - `cargo test --test codegen_tests codegen::simplexml_tostring_override`.
//!
//! Key details:
//! - Untyped `__toString` overrides remain valid only for the string contract.
//! - `#[ReturnTypeWillChange]` applies only to native SimpleXML iteration.

use crate::support::{compile_and_run, compile_expect_type_error};

/// Verifies a SimpleXMLElement descendant override is used by casts and direct calls.
#[test]
fn simplexml_subclass_tostring_override_matches_php() {
    let out = compile_and_run(
        r#"<?php
class UberSimpleXML extends SimpleXMLElement {
    public function __toString() {
        return "stringification";
    }
}

$xml = new UberSimpleXML('<xml/>');
var_dump((string) $xml);
var_dump($xml->__toString());
"#,
    );
    assert_eq!(
        out,
        "string(15) \"stringification\"\nstring(15) \"stringification\"\n"
    );
}

/// Verifies an untyped ordinary userland override is accepted like PHP.
#[test]
fn ordinary_tostring_override_without_return_type_matches_php() {
    let out = compile_and_run(
        r#"<?php
class BaseStringable {
    public function __toString(): string { return "base"; }
}
class ChildStringable extends BaseStringable {
    public function __toString() { return "child"; }
}
echo (new ChildStringable), "\n";
"#,
    );
    assert_eq!(out, "child\n");
}

/// Verifies untyped scalar `__toString` returns use PHP string coercion.
#[test]
fn untyped_tostring_scalar_returns_coerce_like_php() {
    let out = compile_and_run(
        r#"<?php
class UntypedIntegerStringable {
    public function __toString() { return 1; }
}
class UntypedBooleanStringable implements Stringable {
    public function __toString() { return true; }
}
echo (new UntypedIntegerStringable), "|", (new UntypedBooleanStringable);
"#,
    );
    assert_eq!(out, "1|1");
}

/// Verifies a declared non-string `__toString` return type remains rejected like PHP.
#[test]
fn tostring_declared_non_string_return_type_is_rejected_like_php() {
    let error = compile_expect_type_error(
        r#"<?php
class InvalidStringable {
    public function __toString(): bool { return true; }
}
"#,
    );
    assert!(
        error.contains("Magic method must return string: InvalidStringable::__toString"),
        "unexpected diagnostic: {error}"
    );
}

/// Verifies an isolated `never` string conversion is reachable and catchable.
#[test]
fn tostring_never_return_type_matches_php() {
    let out = compile_and_run(
        r#"<?php
class NeverStringable {
    public function __toString(): never { throw new Exception("never"); }
}
try {
    echo (string) new NeverStringable;
} catch (Throwable $error) {
    echo $error->getMessage();
}
echo "|after";
"#,
    );
    assert_eq!(out, "never|after");
}

/// Verifies an inherited `never` contract never receives the untyped-string exception.
#[test]
fn tostring_untyped_override_does_not_bypass_never_parent_contract() {
    let error = compile_expect_type_error(
        r#"<?php
class ParentNeverStringable {
    public function __toString(): never { throw new Exception; }
}
class ChildUntypedStringable extends ParentNeverStringable {
    public function __toString() { return "child"; }
}
"#,
    );
    assert!(
        error.contains(
            "Cannot override method ChildUntypedStringable::__toString without declaring a compatible return type (parent returns never)"
        ),
        "unexpected diagnostic: {error}"
    );
}

/// Verifies `ReturnTypeWillChange` does not bypass a userland `current()` contract.
#[test]
fn simplexml_current_return_type_will_change_does_not_bypass_userland_parent() {
    let error = compile_expect_type_error(
        r#"<?php
class ParentCurrentXml extends SimpleXMLElement {
    public function current(): SimpleXMLElement { return $this; }
}
class ChildCurrentXml extends ParentCurrentXml {
    #[\ReturnTypeWillChange]
    public function current(): int { return 42; }
}
"#,
    );
    assert!(
        error.contains(
            "Cannot override method ChildCurrentXml::current with incompatible return type int (parent returns SimpleXMLElement)"
        ),
        "unexpected diagnostic: {error}"
    );
}
