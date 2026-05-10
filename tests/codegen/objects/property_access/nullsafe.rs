//! Purpose:
//! Integration or regression tests for end-to-end codegen coverage of object property nullsafe property and method access, including class chained property access, nullsafe property access returns property or null, and nullsafe method call skips arguments when receiver is null.
//!
//! Called from:
//! - `cargo test` through Rust's test harness.
//!
//! Key details:
//! - Inline PHP fixtures are compiled to native binaries and assertions compare stdout or expected failures.

use super::*;

#[test]
fn test_class_chained_property_access() {
    let out = compile_and_run(
        r#"<?php
class Node {
    public $value;
    public $next;
    public function __construct($v) { $this->value = $v; }
}
$a = new Node(1);
$b = new Node(2);
$a->next = $b;
echo $a->next->value;
"#,
    );
    assert_eq!(out, "2");
}

#[test]
fn test_nullsafe_property_access_returns_property_or_null() {
    let out = compile_and_run(
        r#"<?php
class Profile {
    public string $name = "Ada";
}
class User {
    public ?Profile $profile;
}
$with = new User();
$with->profile = new Profile();
$without = new User();
echo $with->profile?->name ?? "none";
echo "|";
echo $without->profile?->name ?? "none";
"#,
    );
    assert_eq!(out, "Ada|none");
}

#[test]
fn test_nullsafe_method_call_skips_arguments_when_receiver_is_null() {
    let out = compile_and_run(
        r#"<?php
function side() {
    echo "bad";
    return "side";
}
class Box {
    public function label($value): string {
        return $value;
    }
}
?Box $box = null;
echo $box?->label(side()) ?? "none";
"#,
    );
    assert_eq!(out, "none");
}

#[test]
fn test_nullsafe_method_call_evaluates_receiver_before_arguments() {
    let out = compile_and_run(
        r#"<?php
function receiver() {
    echo "receiver|";
    return new Box();
}
function side() {
    echo "arg|";
    return "value";
}
class Box {
    public function label($value): string {
        echo "method|";
        return $value;
    }
}
echo receiver()?->label(side());
"#,
    );
    assert_eq!(out, "receiver|arg|method|value");
}

#[test]
fn test_method_call_evaluates_receiver_before_arguments() {
    let out = compile_and_run(
        r#"<?php
function receiver() {
    echo "receiver|";
    return new Box();
}
function side() {
    echo "arg|";
    return "value";
}
class Box {
    public function label($value): string {
        echo "method|";
        return $value;
    }
}
echo receiver()->label(side());
"#,
    );
    assert_eq!(out, "receiver|arg|method|value");
}

#[test]
fn test_nullsafe_chained_access_short_circuits_each_hop() {
    let out = compile_and_run(
        r#"<?php
class Address {
    public string $city = "Rome";
}
class Profile {
    public ?Address $address;
}
class User {
    public ?Profile $profile;
}
$with = new User();
$profile = new Profile();
$profile->address = new Address();
$with->profile = $profile;
$without = new User();
echo $with?->profile?->address?->city ?? "none";
echo "|";
echo $without?->profile?->address?->city ?? "none";
"#,
    );
    assert_eq!(out, "Rome|none");
}

#[test]
fn test_nullsafe_chained_method_result_short_circuits() {
    let out = compile_and_run(
        r#"<?php
class Profile {
    public string $name = "Ada";
}
class User {
    public ?Profile $profile;
    public function profile(): ?Profile {
        return $this->profile;
    }
}
$with = new User();
$with->profile = new Profile();
$without = new User();
echo $with?->profile()?->name ?? "none";
echo "|";
echo $without?->profile()?->name ?? "none";
"#,
    );
    assert_eq!(out, "Ada|none");
}

#[test]
fn test_nullsafe_static_null_receiver_keeps_receiver_side_effects() {
    let out = compile_and_run(
        r#"<?php
function none() {
    echo "receiver|";
    return null;
}
function arg() {
    echo "arg|";
    return "value";
}
echo none()?->name ?? "none";
echo "|";
echo none()?->label(arg()) ?? "none";
"#,
    );
    assert_eq!(out, "receiver|none|receiver|none");
}

#[test]
fn test_mixed_nullsafe_member_chain_skips_rest_when_base_is_null() {
    let out = compile_and_run_capture(
        r#"<?php
class Leaf {
    public string $name = "hit";
}
class Branch {
    public ?Leaf $leaf = null;
}
class Root {
    public ?Branch $branch = null;
}
function read(?Root $root): void {
    echo $root?->branch->leaf->name ?? "fallback";
}
read(null);
"#,
    );
    assert!(out.success, "program failed: {}", out.stderr);
    assert_eq!(out.stdout, "fallback");
    assert_eq!(out.stderr, "");
}

#[test]
fn test_mixed_nullsafe_member_chain_warns_for_real_null_midpoint() {
    let out = compile_and_run_capture(
        r#"<?php
class Leaf {
    public string $name = "hit";
}
class Branch {
    public ?Leaf $leaf = null;
}
class Root {
    public ?Branch $branch = null;
}
$root = new Root();
$root->branch = new Branch();
echo $root?->branch->leaf->name ?? "fallback";
"#,
    );
    assert!(out.success, "program failed: {}", out.stderr);
    assert_eq!(out.stdout, "fallback");
    assert!(
        out.stderr.contains("Warning: Attempt to read property \"name\" on null"),
        "{}",
        out.stderr
    );
}

#[test]
fn test_mixed_nullsafe_member_chain_skips_method_arguments() {
    let out = compile_and_run_capture(
        r#"<?php
function noisy(): string {
    echo "noisy|";
    return "arg";
}
class Branch {
    public function label(string $value): string {
        return $value;
    }
}
class Root {
    public ?Branch $branch = null;
}
function read(?Root $root): void {
    echo $root?->branch->label(noisy()) ?? "fallback";
}
read(null);
"#,
    );
    assert!(out.success, "program failed: {}", out.stderr);
    assert_eq!(out.stdout, "fallback");
    assert_eq!(out.stderr, "");
}

#[test]
fn test_mixed_nullsafe_member_chain_fatals_before_method_arguments_on_real_null() {
    let out = compile_and_run_capture(
        r#"<?php
function noisy(): string {
    echo "noisy|";
    return "arg";
}
class Branch {
    public function label(string $value): string {
        return $value;
    }
}
class Root {
    public ?Branch $branch = null;
}
$root = new Root();
echo $root?->branch->label(noisy()) ?? "fallback";
"#,
    );
    assert!(!out.success, "program unexpectedly succeeded");
    assert_eq!(out.stdout, "");
    assert!(
        out.stderr.contains("Call to a member function label() on null"),
        "{}",
        out.stderr
    );
}

#[test]
fn test_nullsafe_middle_of_member_chain_skips_following_member() {
    let out = compile_and_run_capture(
        r#"<?php
class Leaf {
    public string $name = "hit";
}
class Branch {
    public ?Leaf $leaf = null;
}
class Root {
    public ?Branch $branch = null;
}
$root = new Root();
echo $root->branch?->leaf->name ?? "fallback";
"#,
    );
    assert!(out.success, "program failed: {}", out.stderr);
    assert_eq!(out.stdout, "fallback");
    assert_eq!(out.stderr, "");
}

#[test]
fn test_nullsafe_chain_skips_array_index_expression() {
    let out = compile_and_run_capture(
        r#"<?php
function noisy(): int {
    echo "noisy|";
    return 0;
}
class Root {
    public array $items = [7];
}
function read(?Root $root): void {
    echo $root?->items[noisy()] ?? "fallback";
}
read(null);
"#,
    );
    assert!(out.success, "program failed: {}", out.stderr);
    assert_eq!(out.stdout, "fallback");
    assert_eq!(out.stderr, "");
}

#[test]
fn test_nullsafe_chain_skips_expr_call_arguments() {
    let out = compile_and_run_capture(
        r#"<?php
function noisy(): string {
    echo "noisy|";
    return "arg";
}
class Root {
    public function callback(): callable {
        return function(string $value): string {
            return $value;
        };
    }
}
function read(?Root $root): void {
    echo ($root?->callback())(noisy()) ?? "fallback";
}
read(null);
"#,
    );
    assert!(out.success, "program failed: {}", out.stderr);
    assert_eq!(out.stdout, "fallback");
    assert_eq!(out.stderr, "");
}

#[test]
fn test_nullsafe_chain_calls_loaded_expr_call_on_non_null_receiver() {
    let out = compile_and_run_capture(
        r#"<?php
function noisy(): int {
    echo "noisy|";
    return 20;
}
class Root {
    public function callback(): callable {
        return function(int $value): int {
            return $value + 1;
        };
    }
}
function read(?Root $root): void {
    echo ($root?->callback())(noisy()) ?? "fallback";
}
read(new Root());
"#,
    );
    assert!(out.success, "program failed: {}", out.stderr);
    assert_eq!(out.stdout, "noisy|21");
    assert_eq!(out.stderr, "");
}

