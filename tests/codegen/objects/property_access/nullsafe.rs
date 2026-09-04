//! Purpose:
//! Integration or regression tests for end-to-end codegen coverage of object property nullsafe property and method access, including class chained property access, nullsafe property access returns property or null, and nullsafe method call skips arguments when receiver is null.
//!
//! Called from:
//! - `cargo test` through Rust's test harness.
//!
//! Key details:
//! - Inline PHP fixtures are compiled to native binaries and assertions compare stdout or expected failures.

use super::*;

/// Verifies that chained property access ($a->next->value) works correctly
/// when traversing a linked list of Node objects. Fixture: Node with public
/// $value and $next properties, __construct sets $value.
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

/// Verifies nullsafe (?->) returns the property value when receiver is non-null,
/// or null when receiver is null, using the ?? operator to coalesce. Fixture:
/// User with nullable ?Profile $profile, one instance with profile set, one without.
#[test]
fn test_nullsafe_property_access_returns_property_or_null() {
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

/// Verifies nullsafe (`?->`) does NOT suppress the "must not be accessed before
/// initialization" error for a typed property that is genuinely uninitialized, as opposed to
/// explicitly set to null. `?->` guards a null RECEIVER; it says nothing about the property it
/// then reads, and reference PHP fatals on this exact program.
///
/// The fixture deliberately carries no `??`. It used to, and that made the test assert the
/// opposite of reference PHP: `echo $without?->profile?->name ?? "none";` prints `none` and
/// exits 0 under `php -n`, because `??` DOES suppress it. The suppressing case is the test
/// below; keeping both is what separates the two constructs, which the single `??` fixture
/// could not do.
#[test]
fn test_nullsafe_property_access_does_not_suppress_uninitialized_typed_property() {
    let err = compile_and_run_expect_failure(
        r#"<?php
class Profile {
    public string $name = "Ada";
}
class User {
    public ?Profile $profile;
}
$without = new User();
echo $without?->profile?->name;
"#,
    );
    assert!(
        err.contains("Fatal error: Typed property User::$profile must not be accessed before initialization"),
        "{err}"
    );
}

/// The companion: `??` DOES suppress it, whether the uninitialized property sits at the root
/// of a nullsafe chain or is read on its own.
#[test]
fn test_coalesce_suppresses_an_uninitialized_typed_property() {
    let out = compile_and_run(
        r#"<?php
class Profile {
    public string $name = "Ada";
}
class User {
    public ?Profile $profile;
}
$without = new User();
echo $without?->profile?->name ?? "none";
echo "|";
echo $without->profile ?? "absent";
echo "|";
$with = new User();
$with->profile = new Profile();
echo $with?->profile?->name ?? "none";
"#,
    );
    assert_eq!(out, "none|absent|Ada");
}

/// Verifies nullsafe method call (?->) does not evaluate call arguments when
/// the receiver is null. Fixture: side() function echoes "bad" if called,
/// Box?->label(side()) with null box should output "none" (not "bad").
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

/// Verifies nullsafe method call evaluates receiver expression before arguments,
/// preserving PHP's left-to-right evaluation order. Fixture: receiver() echoes
/// "receiver|", side() echoes "arg|", method echoes "method|"; chained result
/// must be "receiver|arg|method|value".
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

/// Verifies regular (non-nullsafe) method call also evaluates receiver before
/// arguments, matching PHP's left-to-right evaluation order. Same fixture as
/// nullsafe variant but with -> instead of ?-> to confirm consistency.
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

/// Verifies nullsafe (?->) short-circuits at each hop in a chained access:
/// $with?->profile?->address?->city returns "Rome" (all hops non-null),
/// $without?->profile?->address?->city returns "none" (profile is null).
#[test]
fn test_nullsafe_chained_access_short_circuits_each_hop() {
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

/// Verifies nullsafe method call (?->) short-circuits when the method returns
/// null, returning null for the whole expression. Fixture: User with nullable
/// ?Profile $profile and profile() method returning $this->profile; with user
/// has profile set, without user has null profile.
#[test]
fn test_nullsafe_chained_method_result_short_circuits() {
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

/// Verifies nullsafe (?->) evaluates receiver side effects even when receiver
/// is null, but skips property access and following arguments. Fixture: none()
/// echoes "receiver|" and returns null; arg() echoes "arg|" and returns "value";
/// none()?->name evaluates receiver (output "receiver|") then returns "none",
/// none()?->label(arg()) evaluates receiver (output "receiver|") but skips arg().
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

/// Verifies mixed chain (regular -> followed by nullsafe ?->) short-circuits
/// and returns null when the base receiver is null, skipping remaining property
/// accesses. Fixture: Root?->branch->leaf->name with read(null) returns "fallback".
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
    assert_eq!(out.diagnostics, "");
}

/// A mixed chain (nullsafe `?->` then regular `->`) that hits a real null at a non-nullsafe hop
/// yields the `??` fallback and emits NOTHING: `??` suppresses the property-on-null warning
/// wherever in the chain the null appears. Fixture: `Root` has a `Branch`, but `Branch->leaf` is
/// null, so `$root?->branch->leaf->name` reads a property on null mid-chain.
///
/// This asserted the warning WAS emitted until the `??` suppression was made to match `php -n`,
/// which prints `fallback` and leaves stderr empty for this exact program. Written from the
/// implementation, it pinned the divergence.
#[test]
fn test_mixed_nullsafe_member_chain_coalesces_a_real_null_midpoint_without_warning() {
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
    // The two sides of the merge asserted OPPOSITE things here: upstream that the warning IS
    // raised, this branch that `??` suppresses it. The suppression is what `php -n` 8.5.6 does
    // for this exact program — it prints `fallback` and raises nothing — and it is what the test
    // is named for.
    assert_eq!(
        out.diagnostics, "",
        "`??` must suppress the mid-chain property-on-null warning"
    );
}

/// Verifies mixed chain ($root?->branch->label(noisy())) skips noisy() argument
/// evaluation when base receiver is null. Fixture: noisy() echoes "noisy|",
/// Branch->label returns the value; read(null) returns "fallback" with no stderr.
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
    assert_eq!(out.diagnostics, "");
}

/// Verifies mixed chain with non-null base but null mid-hop (Branch->leaf is
/// null) fatals before evaluating method arguments. Fixture: Root with Branch
/// but Branch->leaf is null; $root?->branch->label(noisy()) fatals with
/// "Call to a member function label() on null" and skips noisy() evaluation.
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

/// Verifies nullsafe (?->) in the middle of a chain ($root->branch?->leaf->name)
/// short-circuits when that hop is null, returning null and skipping the
/// following ->leaf->name accesses. Fixture: Root with Branch but Branch->leaf
/// is null; $root->branch?->leaf->name returns "fallback" with no warning.
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
    assert_eq!(out.diagnostics, "");
}

/// Verifies nullsafe (?->) skips array index expression evaluation when
/// receiver is null. Fixture: noisy() echoes "noisy|", Root has array $items,
/// $root?->items[noisy()] with null $root returns "fallback" and does not
/// call noisy().
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
    assert_eq!(out.diagnostics, "");
}

/// Verifies nullsafe (?->) skips callable invocation argument evaluation when
/// receiver is null. Fixture: noisy() echoes "noisy|", Root has callback()
/// returning a closure, $root?->callback()(noisy()) with null $root returns
/// "fallback" and does not call noisy().
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
    assert_eq!(out.diagnostics, "");
}

/// Verifies nullsafe (?->) calls the loaded callable and evaluates arguments
/// when receiver is non-null. Fixture: noisy() echoes "noisy|", Root has
/// callback() returning a closure, ($root?->callback())(noisy()) with non-null
/// $root calls both and returns "noisy|21" (noisy output + value + 1).
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
    assert_eq!(out.diagnostics, "");
}

/// A `?C`-typed receiver represents as a boxed `Mixed`, which `Op::PropInitialized` used to be
/// refused for: `$c->p ?? "d"` therefore stayed on the ordinary read and fatalled on an
/// uninitialized typed property, and `isset($c->p)` did not compile at all
/// (`unsupported EIR backend feature: prop_initialized for receiver PHP type Mixed`). Both are
/// ordinary code — a nullable parameter is the common way to accept an optional object.
///
/// The backend now unboxes such a receiver, and a NULL one answers "not initialized", which is
/// the answer both callers want: `isset(null->p)` is false and `null->p ?? "d"` is the default.
#[test]
fn test_coalesce_and_isset_reach_a_nullable_typed_receiver() {
    let out = compile_and_run(
        r#"<?php
class C {
    public int $p;
    public string $q = "def";
}
function read(?C $c): string { return $c->p ?? "d"; }
function probe(?C $c): string { return isset($c->p) ? "y" : "n"; }
function defaulted(?C $c): string { return $c->q ?? "d"; }
$fresh = new C();
$set = new C();
$set->p = 3;
$cleared = new C();
$cleared->p = 5;
unset($cleared->p);
echo read($fresh), read(null), read($set), read($cleared), "|";
echo probe($fresh), probe(null), probe($set), probe($cleared), "|";
echo defaulted($fresh), defaulted(null);
"#,
    );
    assert_eq!(out, "dd3d|nnyn|defd");
}

/// The slot is resolved from the receiver's DECLARED class, so a subclass instance arriving
/// through the same `?C` parameter must still read its inherited slot — both the probe and the
/// value. A layout mismatch would show here as a wrong number rather than a fatal.
#[test]
fn test_nullable_typed_receiver_probe_follows_an_inherited_slot() {
    let out = compile_and_run(
        r#"<?php
class C { public int $p; }
class D extends C { public int $r = 7; }
function read(?C $c): string { return $c->p ?? "d"; }
$plain = new D();
$filled = new D();
$filled->p = 9;
echo read($plain), ",", read($filled);
"#,
    );
    assert_eq!(out, "d,9");
}

/// `??=` through a nullable receiver exercises the probe AND the write that follows it. The
/// uninitialized branch of the probe used to release the receiver unconditionally; for a boxed
/// `?C` local that release freed the receiver the write then needed, and the program died with
/// "Attempt to assign property on null" while holding a perfectly good object.
#[test]
fn test_coalesce_assign_through_a_nullable_typed_receiver() {
    let out = compile_and_run(
        r#"<?php
class C { public int $p; }
function fill(?C $c): string { $c->p ??= 42; return (string) $c->p; }
$fresh = new C();
$set = new C();
$set->p = 3;
echo fill($fresh), ",", fill($set), ",", $fresh->p;
"#,
    );
    assert_eq!(out, "42,3,42");
}

/// The receiver of the suppressing read is released only when this expression OWNS it. A call
/// result does — `mk()->p ?? "none"` leaked one object per call before it was released on the
/// uninitialized path — and a plain variable does not. Both shapes run here, with a nullable
/// call result covering the boxed case, and the heap must end balanced.
#[test]
fn test_coalesce_receiver_release_is_balanced_for_owned_and_borrowed_receivers() {
    let out = compile_and_run_with_gc_stats(
        r#"<?php
class C { public int $p; }
function mk(): C { return new C(); }
function mkMaybe(bool $real): ?C { return $real ? new C() : null; }
$local = new C();
for ($i = 0; $i < 200; $i++) {
    $a = mk()->p ?? "none";
    $b = mk()?->p ?? "none";
    $c = mkMaybe(true)->p ?? "none";
    $d = mkMaybe(false)->p ?? "none";
    $e = $local->p ?? "none";
}
echo $a, $b, $c, $d, $e;
"#,
    );
    assert_eq!(out.stdout, "nonenonenonenonenone");
    let (allocs, frees) = parse_gc_stats(&out.stderr);
    assert_eq!(allocs, frees, "expected clean heap, got: {}", out.stderr);
}

/// Verifies `??` on a declared `mixed` property, instance and static.
///
/// `public mixed $x;` IS declared and starts uninitialized, but its type is literally `Mixed` —
/// the same value an UNTYPED `public $x;` carries, which is plain null from the start and must
/// stay on the ordinary read. The gate tested the representation and so read the declared one as
/// untyped, and both `??` cases raised where PHP answers the default. The predicates ask the
/// schema now (`property_slot_is_declared` / `declared_static_properties`), which is the question
/// they always meant.
#[test]
fn test_coalesce_on_declared_mixed_property() {
    let instance = compile_and_run(
        r#"<?php
class D { public mixed $x; }
$o = new D();
echo $o->x ?? "def";
"#,
    );
    assert_eq!(instance, "def");

    let statik = compile_and_run(
        r#"<?php
class S { public static mixed $s; }
echo S::$s ?? "def";
"#,
    );
    assert_eq!(statik, "def");
}

/// Verifies `isset($o->p)` on an uninitialized typed property does not leak an OWNING receiver.
///
/// The probe branches before reading, and only the read arm hands the receiver to
/// `lower_property_get_from_value`, which consumes it — so a receiver produced by the expression
/// itself had nowhere to go. Three calls reported `allocs=3 frees=0` where the same program with
/// an INITIALIZED property closed at 3/3.
///
/// Both halves are asserted because the obvious fix breaks the other one: releasing on type alone
/// frees a BORROWED `?C` receiver too, and `isset($c->p); $c->p ??= new P();` then dies with
/// `Attempt to assign property "p" on null`. The release is gated on
/// `value_is_owning_temporary`, the same gate the nullsafe chain uses.
#[test]
fn test_isset_on_uninitialized_property_balances_an_owning_receiver() {
    let out = compile_and_run(
        r#"<?php
class C { public int $p; }
class P { public int $v = 7; }
class B { public ?P $p; }
function mk() { return new C(); }
var_dump(isset(mk()->p));
var_dump(isset(mk()->p));
function f(?B $b) { var_dump(isset($b->p)); $b->p ??= new P(); return $b->p->v; }
echo f(new B());
"#,
    );
    assert_eq!(out, "bool(false)\nbool(false)\nbool(false)\n7");
}

/// Verifies `empty($o->p)` on a declared INSTANCE property, including the uninitialized slot.
///
/// An uninitialized typed slot IS empty in PHP, and the ordinary read that would find that out
/// raises instead — so `empty($o->p)` on `class C { public int $p; }` died with
/// `Typed property C::$p must not be accessed before initialization` where PHP answers
/// `bool(true)`. Only the STATIC path probed; the instance dispatch had no arm for it.
///
/// The matrix is what makes the probe safe rather than merely quiet: an initialized slot must
/// still answer from its VALUE (`$q = 5` is not empty), a declared `mixed` slot counts as
/// uninitialized, `unset()` returns a defaulted slot to that state, and a receiver the expression
/// produced itself must not leak — the arm answers without the read that would have consumed it.
/// Expected output is verbatim `php -n`.
#[test]
fn test_empty_on_declared_instance_property() {
    let out = compile_and_run(
        r#"<?php
class C { public int $p; public int $q = 5; public mixed $m; }
function mk() { return new C(); }
$o = new C();
var_dump(empty($o->p));
var_dump(empty($o->q));
var_dump(empty($o->m));
unset($o->q);
var_dump(empty($o->q));
var_dump(empty(mk()->p));
"#,
    );
    assert_eq!(
        out,
        "bool(true)\nbool(false)\nbool(true)\nbool(true)\nbool(true)\n"
    );
}
