//! Purpose:
//! Regression tests for sound flow-sensitive narrowing diagnostics.
//!
//! Called from:
//! - `cargo test --test error_tests` through Rust's test harness.
//!
//! Key details:
//! - Negative fixtures ensure literal-false and property facts are not retained beyond mutations,
//!   receiver rebindings, or user-code property getters.

use super::*;

/// Guarded union writes are accepted in permissive mode, as in php, and still reported by
/// `--strict-locals`.
#[test]
fn test_guarded_union_assignment_preserves_declared_boundaries() {
    for source in [
        "<?php function update(array $fields): void { if (is_array($fields)) { $fields = mb_split(',', 'a,b'); } } update([]);",
        "<?php function update(array|false $fields): void { if (is_array($fields)) { $fields = 'wrong'; } } update([]);",
    ] {
        expect_no_error(source);
        expect_error_strict(source, "cannot reassign $fields");
    }
}

/// Verifies the literal `false` parameter type rejects `true` rather than widening to bool.
#[test]
fn test_literal_false_parameter_rejects_true() {
    expect_error(
        "<?php function onlyFalse(false $value): void {} onlyFalse(true);",
        "Function 'onlyFalse' parameter $value expects false, got bool",
    );
}

/// The fallthrough after `$value === false` keeps the `true` member, so returning `int|bool` from a function
/// declared `: int` is checked at run time (weak mode coerces `true` to 1), as in php.
#[test]
fn test_strict_false_guard_bool_member_return_is_checked_at_runtime() {
    expect_no_error(
        "<?php function requireInt(int|bool $value): int { if ($value === false) { throw new Exception('false'); } return $value; }",
    );
}

/// A direct property write clears a prior narrowing; returning the (now null) property from a function
/// declared `: W` is a TypeError at run time in php, and in elephc too (checked against php).
#[test]
fn test_property_write_return_is_checked_at_runtime() {
    expect_no_error(
        "<?php class W {} class Box { public function __construct(public ?W $value) {} } function read(Box $box): W { if (!$box->value instanceof W) { throw new Exception('missing'); } $box->value = null; return $box->value; }",
    );
}

/// Rebinding the local receiver clears property facts tied to the old object; the nullable return is
/// checked at run time, as in php.
#[test]
fn test_property_receiver_rebinding_return_is_checked_at_runtime() {
    expect_no_error(
        "<?php class W {} class Box { public function __construct(public ?W $value) {} } function read(Box $box, Box $replacement): W { if (!$box->value instanceof W) { throw new Exception('missing'); } $box = $replacement; return $box->value; }",
    );
}

/// A hooked property is never a stable flow binding across two reads; returning it from a function
/// declared `: W` is checked at run time, as in php.
#[test]
fn test_property_get_hook_return_is_checked_at_runtime() {
    expect_no_error(
        "<?php class W {} class Box { private ?W $stored; public function __construct(?W $stored) { $this->stored = $stored; } public ?W $value { get { $result = $this->stored; $this->stored = null; return $result; } } } function read(Box $box): W { if (!$box->value instanceof W) { throw new Exception('missing'); } return $box->value; }",
    );
}

/// An undeclared property served by `__get` is not a stable flow binding; returning it from a function
/// declared `: W` is checked at run time, as in php.
#[test]
fn test_magic_get_property_return_is_checked_at_runtime() {
    expect_no_error(
        "<?php class W {} class Box { private ?W $stored; public function __construct(?W $stored) { $this->stored = $stored; } public function __get(string $name): ?W { $result = $this->stored; $this->stored = null; return $result; } } function read(Box $box): W { if (!$box->value instanceof W) { throw new Exception('missing'); } return $box->value; }",
    );
}

/// When a nested branch inside the null guard can fall through, `?array` stays a union and the list unpack
/// is checked at run time, as in php (issue #590 negative case).
#[test]
fn test_list_unpack_after_nested_fallthrough_branch_is_checked_at_runtime() {
    expect_no_error(
        "<?php function consume(?array $entry, bool $flag): void { if ($entry === null) { if ($flag) { return; } } [$key, $value] = $entry; }",
    );
}

/// A nested diverging call in only one arm does not make the enclosing `if` terminal, so the list unpack
/// of `?array` is checked at run time, as in php.
#[test]
fn test_list_unpack_after_nested_exit_branch_is_checked_at_runtime() {
    expect_no_error(
        "<?php function consume(?array $entry, bool $flag): void { if ($entry === null) { if ($flag) { exit(1); } } [$key, $value] = $entry; }",
    );
}

/// A literal-true loop that can break may fall through to the code after the guard, so the list unpack
/// of `?array` is checked at run time, as in php.
#[test]
fn test_list_unpack_after_breakable_literal_true_loop_is_checked_at_runtime() {
    expect_no_error(
        "<?php function consume(?array $entry, bool $flag): void { if ($entry === null) { while (true) { if ($flag) { break; } } } [$key, $value] = $entry; }",
    );
}

/// A static-property narrowing does not survive an intervening call that reassigns it; php raises a
/// TypeError for this program when `get()` runs and elephc raises the same TypeError at run time (checked
/// against php).
#[test]
fn test_static_property_return_after_intervening_call_is_checked_at_runtime() {
    expect_no_error(
        r#"<?php
class S {
    private static ?S $inst = null;
    private static function wipe(): void { self::$inst = null; }
    public static function get(): S {
        if (self::$inst === null) { self::$inst = new S(); }
        self::wipe();
        return self::$inst;
    }
}
"#,
    );
}

/// A `return` placed before the guard that narrows `$this->p` does not borrow that later fact; the nullable
/// return is checked at run time, as in php (a null there is a TypeError raised when the call runs).
#[test]
fn test_property_narrowing_does_not_leak_backwards_return_is_checked_at_runtime() {
    expect_no_error(
        r#"<?php
class A {
    public ?A $p = null;
    public function f(bool $c): A {
        if ($c) { return $this->p; }
        if ($this->p === null) { throw new Exception("x"); }
        return $this->p;
    }
}
"#,
    );
}

/// Returning a nullable static property from a function declared `: S` is checked at run time, as in php
/// (a null is a TypeError raised when the call runs).
#[test]
fn test_unguarded_nullable_static_property_return_is_checked_at_runtime() {
    expect_no_error(
        r#"<?php
class S {
    private static ?S $inst = null;
    public static function get(): S { return self::$inst; }
}
"#,
    );
}

/// Returning `static::$inst` from a function declared `: S` is not a compile-time error: a possible null is
/// checked at run time, as in php.
#[test]
fn test_late_static_bound_property_return_is_checked_at_runtime() {
    expect_no_error(
        r#"<?php
class S {
    protected static ?S $inst = null;
    public static function get(): S {
        if (static::$inst === null) { static::$inst = new S(); }
        return static::$inst;
    }
}
"#,
    );
}

/// Verifies an `instanceof` guard narrows bare `$this` to the proven interface in its branch.
#[test]
fn test_this_instanceof_narrows_to_subinterface_method() {
    expect_no_error(
        "<?php interface Base { public function base(): string; } interface Kid extends Base { public function kids(): int; } abstract class NodeBase implements Base { public function go(): int { if ($this instanceof Kid) { return $this->kids(); } return 0; } }",
    );
}

/// Verifies bare-`$this` narrowing still rejects methods absent from the proven interface.
#[test]
fn test_this_instanceof_rejects_method_absent_from_interface() {
    expect_error(
        "<?php interface Base { public function base(): string; } interface Kid extends Base { public function kids(): int; } abstract class NodeBase implements Base { public function go(): int { if ($this instanceof Kid) { return $this->notOnKid(); } return 0; } }",
        "Undefined method: Kid::notOnKid",
    );
}


/// Verifies an undefined method with no `method_exists()` test in the body is still refused.
///
/// The capability guard exists for code that says out loud it does not know whether a method is
/// there. A plain misspelling says nothing, so it keeps the diagnostic.
#[test]
fn test_undefined_method_without_a_guard_is_still_refused() {
    expect_error(
        "<?php interface ParserInterface { public function getName(): string; } class Plain implements ParserInterface { public function getName(): string { return 'plain'; } } function tokensFor(ParserInterface $parser): array { return [$parser->getNmae()]; }",
        "Undefined method: ParserInterface::getNmae",
    );
}

/// Verifies a `method_exists()` guard admits only the receiver it actually tested.
///
/// The fact is keyed by the same place key the type guards use, so testing `$a` says nothing about
/// `$b` — otherwise one guard anywhere in a body would admit every undefined call in it.
#[test]
fn test_method_exists_guard_does_not_admit_a_different_receiver() {
    expect_error(
        "<?php interface ParserInterface { public function getName(): string; } class Plain implements ParserInterface { public function getName(): string { return 'plain'; } } function tokensFor(ParserInterface $a, ParserInterface $b): array { if (method_exists($a, 'getOperatorTokens')) { return $b->getOperatorTokens(); } return [$a->getName()]; }",
        "Undefined method: ParserInterface::getOperatorTokens",
    );
}

/// Issue #509: a store inside a guarded region is measured against the BINDING, not against the
/// guard's view of the value, so the universal PHP fallback idiom type-checks.
///
/// `glob()` returns `array<string>|false`; the guard narrows `$g` to `false` for its branch, and
/// merging an empty array with `false` is what produced
/// `cannot reassign $g from false to array<never>`. The binding holds an empty array perfectly
/// well and no slot is abandoned, so the rule holds under `--strict-locals` too.
#[test]
fn test_false_fallback_store_inside_the_guard_is_accepted() {
    let source = r#"<?php
$g = glob("*.meta");
if ($g === false) { $g = []; }
echo count($g);
"#;
    expect_no_error(source);
    expect_no_error_strict(source);
}

/// The complement an `if`/`else` chain publishes is a guard fact in its own right, so the same
/// idiom with the fallback in the `else` is accepted the same way.
#[test]
fn test_false_fallback_store_on_the_complement_side_is_accepted() {
    let source = r#"<?php
$g = glob("*.meta");
if ($g !== false) { echo count($g); } else { $g = []; }
echo count($g);
"#;
    expect_no_error(source);
    expect_no_error_strict(source);
}

/// The origin is the BINDING's type, not the enclosing guard's view: an inner guard narrows
/// further, and a store inside it is still judged against `int|string`.
#[test]
fn test_nested_guard_store_is_judged_against_the_binding() {
    let source = r#"<?php
function p(int $n): int|string { return $n === 0 ? 1 : "s"; }
$v = p($argc);
if (is_scalar($v)) { if (is_int($v)) { $v = "s"; } }
echo $v;
"#;
    expect_no_error(source);
    expect_no_error_strict(source);
}

/// A value the narrowed binding cannot hold is accepted in permissive mode, as in php (the name is re-bound
/// to the new value); `--strict-locals` still rejects the conflicting store.
#[test]
fn test_a_store_the_binding_cannot_hold_is_accepted_but_strict_rejected() {
    let source = r#"<?php
function p(int $n): int|string { return $n === 0 ? 1 : "s"; }
$v = p($argc);
if (is_int($v)) { $v = new stdClass(); }
echo 1;
"#;
    expect_no_error(source);
    expect_error_strict(source, "cannot reassign $v");
}

/// The guard stops governing a name once the region has bound it: the SECOND store measures
/// itself against what the first one left behind. That is what keeps
/// `$a = 1; if (is_string($a)) { $a = "x"; $a = 2; }` on the branch-divergent `Mixed`-storage
/// path (`type_system::test_guarded_region_shapes_still_error_under_strict`) instead of being
/// waved through on the strength of `$a`'s original `int`.
#[test]
fn test_the_guard_stops_governing_after_the_region_stores() {
    expect_warning(
        "<?php $a = 1; if (is_string($a)) { $a = \"x\"; $a = 2; } echo $a;",
        "boxed mixed storage",
    );
    expect_error_strict(
        "<?php $a = 1; if (is_string($a)) { $a = \"x\"; $a = 2; } echo $a;",
        "cannot reassign $a",
    );
}

/// Raised in review on #509: a store in a NESTED guard must end the enclosing guard's authority
/// too, or nesting becomes a way around the rule above.
///
/// The inner region's entry is the one `record_store_over_flow_narrowing` clears, and the outer
/// entry is restored when that region closes — so the outer view came back intact and the later
/// `$a = 2` was accepted against `$a`'s original `int`. The enclosing region CONTAINS the inner
/// one, so a store the inner one made is a store the enclosing one made:
/// `NarrowedLocalOrigin::stored_in_region` travels outward at `exit_flow_narrowing` and the two
/// spellings agree again.
///
/// Written with `$a = 1.5` rather than `$a = "x"` on purpose: `"x"` does not fit the `int`
/// binding either, so it is rejected at the inner store and never reaches the shape under test.
#[test]
fn a_store_in_a_nested_guard_ends_the_enclosing_guards_authority() {
    let source =
        "<?php $a = 1; if (is_string($a)) { if (is_float($a)) { $a = 1.5; } $a = 2; } echo $a;";
    // Permissive mode accepts the program, as php does (`$a` is an int throughout at run time);
    // `--strict-locals` still reports the conflicting stores.
    expect_no_error(source);
    expect_error_strict(source, "cannot reassign $a from float|string to int");
}

/// Shared classes for the readonly-narrowing fixtures: `Box::$plugin` is readonly, and only
/// `Sold` declares `count()`, so a dropped `instanceof Sold` fact reports `Plugin::count`.
const READONLY_PLUGIN_CLASSES: &str = "<?php
interface Plugin { public function name(): string; }
final class Sold implements Plugin { public function name(): string { return 'sold'; } public function count(): int { return 7; } }
final class Other implements Plugin { public function name(): string { return 'other'; } }
final class Box { public function __construct(public readonly Plugin $plugin) {} }
";

/// Verifies a call still drops a readonly property's narrowing whenever the call can rebind the
/// object it is read through: a local (a `=&` alias or a by-reference closure capture can
/// replace it) and a global. PHP then calls `count()` on whatever object
/// the local holds after the call, so keeping the `Sold` fact compiled the wrong call.
#[test]
fn test_readonly_property_narrowing_drops_for_a_rebindable_root() {
    for body in [
        "function f(Box $box): string { if ($box->plugin instanceof Sold) { $box->plugin->name(); return (string) $box->plugin->count(); } return ''; }",
        "function f(Box $box): void { $alias = &$box; if ($box->plugin instanceof Sold) { $swap = function () use (&$alias): void { $alias = new Box(new Other()); }; $swap(); echo $box->plugin->count(); } }",
        "$box = new Box(new Sold());
function swap(): void { global $box; $box = new Box(new Other()); }
function g(): void { global $box; if ($box->plugin instanceof Sold) { swap(); echo $box->plugin->count(); } }",
    ] {
        expect_error(&format!("{READONLY_PLUGIN_CLASSES}{body}"), "Undefined method: Plugin::count");
    }
}

/// Verifies a call drops a `$this` readonly narrowing when the hierarchy declares `__clone`
/// (PHP 8.3+ can reinitialize a readonly property while cloning, from `__clone` or any method
/// it calls), and when the property is not readonly at all.
#[test]
fn test_this_property_narrowing_drops_when_the_property_can_change() {
    for classes in [
        "class Holder { public function __construct(protected readonly Plugin $plugin) {}
    public function line(): string { if ($this->plugin instanceof Sold) { $this->plugin->name(); return (string) $this->plugin->count(); } return ''; } }
final class Copy extends Holder { public function __clone() {} }",
        "final class Holder { public function __construct(private Plugin $plugin) {}
    public function line(): string { if ($this->plugin instanceof Sold) { $this->plugin->name(); return (string) $this->plugin->count(); } return ''; } }",
    ] {
        expect_error(&format!("{READONLY_PLUGIN_CLASSES}{classes}"), "Undefined method: Plugin::count");
    }
}
