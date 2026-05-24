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
    // Verifies that chained property access ($a->next->value) works correctly
    // when traversing a linked list of Node objects. Fixture: Node with public
    // $value and $next properties, __construct sets $value.
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
    // Verifies nullsafe (?->) returns the property value when receiver is non-null,
    // or null when receiver is null, using the ?? operator to coalesce. Fixture:
    // User with nullable ?Profile $profile, one instance with profile set, one without.
    let out = compile_and_run(
        r#"<?php
class Profile {
    public string $name = "Ada";
}
class User {
    public ?Profile $profile = null;
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
fn test_nullsafe_property_access_does_not_suppress_uninitialized_typed_property() {
    // Verifies nullsafe (?->) does NOT suppress the "must not be accessed before
    // initialization" error for typed properties that are genuinely uninitialized,
    // as opposed to explicitly set to null. Fixture: User with uninitialized
    // typed property ?Profile $profile (no default, not set in __construct).
    let err = compile_and_run_expect_failure(
        r#"<?php
class Profile {
    public string $name = "Ada";
}
class User {
    public ?Profile $profile;
}
$without = new User();
echo $without?->profile?->name ?? "none";
"#,
    );
    assert!(
        err.contains("Fatal error: Typed property User::$profile must not be accessed before initialization"),
        "{err}"
    );
}

#[test]
fn test_nullsafe_method_call_skips_arguments_when_receiver_is_null() {
    // Verifies nullsafe method call (?->) does not evaluate call arguments when
    // the receiver is null. Fixture: side() function echoes "bad" if called,
    // Box?->label(side()) with null box should output "none" (not "bad").
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
    // Verifies nullsafe method call evaluates receiver expression before arguments,
    // preserving PHP's left-to-right evaluation order. Fixture: receiver() echoes
    // "receiver|", side() echoes "arg|", method echoes "method|"; chained result
    // must be "receiver|arg|method|value".
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
    // Verifies regular (non-nullsafe) method call also evaluates receiver before
    // arguments, matching PHP's left-to-right evaluation order. Same fixture as
    // nullsafe variant but with -> instead of ?-> to confirm consistency.
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
    // Verifies nullsafe (?->) short-circuits at each hop in a chained access:
    // $with?->profile?->address?->city returns "Rome" (all hops non-null),
    // $without?->profile?->address?->city returns "none" (profile is null).
    let out = compile_and_run(
        r#"<?php
class Address {
    public string $city = "Rome";
}
class Profile {
    public ?Address $address = null;
}
class User {
    public ?Profile $profile = null;
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
    // Verifies nullsafe method call (?->) short-circuits when the method returns
    // null, returning null for the whole expression. Fixture: User with nullable
    // ?Profile $profile and profile() method returning $this->profile; with user
    // has profile set, without user has null profile.
    let out = compile_and_run(
        r#"<?php
class Profile {
    public string $name = "Ada";
}
class User {
    public ?Profile $profile = null;
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
    // Verifies nullsafe (?->) evaluates receiver side effects even when receiver
    // is null, but skips property access and following arguments. Fixture: none()
    // echoes "receiver|" and returns null; arg() echoes "arg|" and returns "value";
    // none()?->name evaluates receiver (output "receiver|") then returns "none",
    // none()?->label(arg()) evaluates receiver (output "receiver|") but skips arg().
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
    // Verifies mixed chain (regular -> followed by nullsafe ?->) short-circuits
    // and returns null when the base receiver is null, skipping remaining property
    // accesses. Fixture: Root?->branch->leaf->name with read(null) returns "fallback".
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
    // Verifies mixed chain (nullsafe ?-> followed by regular ->) emits a warning
    // when a real null is encountered at a non-nullsafe hop. Fixture: Root with
    // Branch but Branch->leaf is null; $root?->branch->leaf->name emits
    // "Attempt to read property 'name' on null" warning and returns "fallback".
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
    // Verifies mixed chain ($root?->branch->label(noisy())) skips noisy() argument
    // evaluation when base receiver is null. Fixture: noisy() echoes "noisy|",
    // Branch->label returns the value; read(null) returns "fallback" with no stderr.
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
    // Verifies mixed chain with non-null base but null mid-hop (Branch->leaf is
    // null) fatals before evaluating method arguments. Fixture: Root with Branch
    // but Branch->leaf is null; $root?->branch->label(noisy()) fatals with
    // "Call to a member function label() on null" and skips noisy() evaluation.
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
    // Verifies nullsafe (?->) in the middle of a chain ($root->branch?->leaf->name)
    // short-circuits when that hop is null, returning null and skipping the
    // following ->leaf->name accesses. Fixture: Root with Branch but Branch->leaf
    // is null; $root->branch?->leaf->name returns "fallback" with no warning.
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
    // Verifies nullsafe (?->) skips array index expression evaluation when
    // receiver is null. Fixture: noisy() echoes "noisy|", Root has array $items,
    // $root?->items[noisy()] with null $root returns "fallback" and does not
    // call noisy().
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
    // Verifies nullsafe (?->) skips callable invocation argument evaluation when
    // receiver is null. Fixture: noisy() echoes "noisy|", Root has callback()
    // returning a closure, $root?->callback()(noisy()) with null $root returns
    // "fallback" and does not call noisy().
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
    // Verifies nullsafe (?->) calls the loaded callable and evaluates arguments
    // when receiver is non-null. Fixture: noisy() echoes "noisy|", Root has
    // callback() returning a closure, ($root?->callback())(noisy()) with non-null
    // $root calls both and returns "noisy|21" (noisy output + value + 1).
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
