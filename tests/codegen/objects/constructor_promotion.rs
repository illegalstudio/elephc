//! Purpose:
//! Integration or regression tests for end-to-end codegen coverage of object constructor property promotion, including constructor promoted properties, constructor promoted readonly property, and constructor promoted by ref property reads source updates.
//!
//! Called from:
//! - `cargo test` through Rust's test harness.
//!
//! Key details:
//! - Inline PHP fixtures are compiled to native binaries and assertions compare stdout or expected failures.

use super::*;

/// Tests basic constructor property promotion: promoted parameters become properties.
///
/// Fixture: `User` class with `public int $id` and `private string $name = "Ada"` promoted
/// parameters. Verifies that promoted properties are accessible as `$obj->prop` and that
/// default values are applied correctly.
#[test]
fn test_constructor_promoted_properties() {
    let out = compile_and_run(
        r#"<?php
class User {
    public function __construct(public int $id, private string $name = "Ada") {}
    public function name() { return $this->name; }
}
$u = new User(7);
echo $u->id;
echo ":";
echo $u->name();
"#,
    );
    assert_eq!(out, "7:Ada");
}

/// Tests constructor property promotion with the `readonly` modifier.
///
/// Fixture: `Token` class with `public readonly int $id`. Verifies that readonly promoted
/// properties are initialized at construction and accessible via the object.
#[test]
fn test_constructor_promoted_readonly_property() {
    let out = compile_and_run(
        r#"<?php
class Token {
    public function __construct(public readonly int $id) {}
    public function id() { return $this->id; }
}
$token = new Token(42);
echo $token->id();
"#,
    );
    assert_eq!(out, "42");
}

/// Tests that by-reference promoted properties reflect updates to the source variable.
///
/// Fixture: `Box` class with `public int &$value`. After constructing `new Box($value)`,
/// assigning `$value = 2` causes `$box->value` to read as `2` because the property aliases
/// the caller's variable.
#[test]
fn test_constructor_promoted_by_ref_property_reads_source_updates() {
    let out = compile_and_run(
        r#"<?php
class Box {
    public function __construct(public int &$value) {}
}
$value = 1;
$box = new Box($value);
$value = 2;
echo $box->value;
"#,
    );
    assert_eq!(out, "2");
}

/// Tests that writes to a by-reference promoted property propagate back to the source variable.
///
/// Fixture: `Box` class with `public int &$value`. After construction, assigning
/// `$box->value = 3` writes back through the reference so the original `$value` becomes `3`.
#[test]
fn test_constructor_promoted_by_ref_property_writes_to_source() {
    let out = compile_and_run(
        r#"<?php
class Box {
    public function __construct(public int &$value) {}
}
$value = 1;
$box = new Box($value);
$box->value = 3;
echo $value;
"#,
    );
    assert_eq!(out, "3");
}

/// Tests that by-reference promotion works for string-typed properties.
///
/// Fixture: `Box` class with `public string &$name`. Verifies the by-reference aliasing
/// mechanism is not limited to integers.
#[test]
fn test_constructor_promoted_by_ref_string_property_writes_to_source() {
    let out = compile_and_run(
        r#"<?php
class Box {
    public function __construct(public string &$name) {}
}
$name = "Ada";
$box = new Box($name);
$box->name = "Grace";
echo $name;
"#,
    );
    assert_eq!(out, "Grace");
}

/// Tests that a by-reference promoted property with a default value uses an internal
/// reference cell when constructed without an argument.
///
/// Fixture: `Box` class with `public int &$value = 1`. When called as `new Box()` with no
/// argument, the property binds to an internal default cell initialized to `1`.
#[test]
fn test_constructor_promoted_by_ref_property_uses_default_reference_cell() {
    let out = compile_and_run(
        r#"<?php
class Box {
    public function __construct(public int &$value = 1) {}
}
$box = new Box();
echo $box->value;
$box->value = 4;
echo ":";
echo $box->value;
"#,
    );
    assert_eq!(out, "1:4");
}

/// Tests that a by-reference promoted property with a default value still aliases an
/// explicit variable argument when one is provided.
///
/// Fixture: `Box` class with `public int &$value = 1`. When called as `new Box($value)`,
/// the property aliases `$value` rather than using the default cell, so mutations to
/// `$box->value` write back through the original variable.
#[test]
fn test_constructor_promoted_by_ref_property_with_default_still_links_variable_arg() {
    let out = compile_and_run(
        r#"<?php
class Box {
    public function __construct(public int &$value = 1) {}
}
$value = 5;
$box = new Box($value);
$box->value = 7;
echo $value;
$value = 9;
echo ":";
echo $box->value;
"#,
    );
    assert_eq!(out, "7:9");
}
