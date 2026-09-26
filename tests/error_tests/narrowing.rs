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

/// Verifies the literal `false` parameter type rejects `true` rather than widening to bool.
#[test]
fn test_literal_false_parameter_rejects_true() {
    expect_error(
        "<?php function onlyFalse(false $value): void {} onlyFalse(true);",
        "Function 'onlyFalse' parameter $value expects false, got bool",
    );
}

/// Verifies the fallthrough after `$value === false` does not remove a full bool member because
/// `true` remains possible.
#[test]
fn test_strict_false_guard_keeps_full_bool_member() {
    expect_error(
        "<?php function requireInt(int|bool $value): int { if ($value === false) { throw new Exception('false'); } return $value; }",
        "Function 'requireInt' return type expects int, got int|bool",
    );
}

/// Verifies a direct property write clears a prior property narrowing before a later return.
#[test]
fn test_property_write_invalidates_narrowing() {
    expect_error(
        "<?php class W {} class Box { public function __construct(public ?W $value) {} } function read(Box $box): W { if (!$box->value instanceof W) { throw new Exception('missing'); } $box->value = null; return $box->value; }",
        "Function 'read' return type expects W, got W|null",
    );
}

/// Verifies rebinding the local receiver clears property facts tied to the old object.
#[test]
fn test_property_receiver_rebinding_invalidates_narrowing() {
    expect_error(
        "<?php class W {} class Box { public function __construct(public ?W $value) {} } function read(Box $box, Box $replacement): W { if (!$box->value instanceof W) { throw new Exception('missing'); } $box = $replacement; return $box->value; }",
        "Function 'read' return type expects W, got W|null",
    );
}

/// Verifies a hooked property is never treated as a stable flow binding across two reads.
#[test]
fn test_property_get_hook_is_not_persistently_narrowed() {
    expect_error(
        "<?php class W {} class Box { private ?W $stored; public function __construct(?W $stored) { $this->stored = $stored; } public ?W $value { get { $result = $this->stored; $this->stored = null; return $result; } } } function read(Box $box): W { if (!$box->value instanceof W) { throw new Exception('missing'); } return $box->value; }",
        "Function 'read' return type expects W, got W|null",
    );
}

/// Verifies an undeclared property served by `__get` is not treated as a stable flow binding.
#[test]
fn test_magic_get_property_is_not_persistently_narrowed() {
    expect_error(
        "<?php class W {} class Box { private ?W $stored; public function __construct(?W $stored) { $this->stored = $stored; } public function __get(string $name): ?W { $result = $this->stored; $this->stored = null; return $result; } } function read(Box $box): W { if (!$box->value instanceof W) { throw new Exception('missing'); } return $box->value; }",
        "Function 'read' return type expects W, got W|null",
    );
}

/// Verifies the post-guard narrowing is NOT kept when a nested branch inside the null guard can
/// fall through: the inner `if` has no `else`, so reaching the code after the guard does not imply
/// the guard was false and `?array` must stay a union (issue #590 negative case).
#[test]
fn test_no_narrow_when_nested_branch_falls_through() {
    expect_error(
        "<?php function consume(?array $entry, bool $flag): void { if ($entry === null) { if ($flag) { return; } } [$key, $value] = $entry; }",
        "List unpacking requires an array",
    );
}

/// Verifies the narrowing is NOT kept when a nested `switch` in the null guard has no `default`, so
/// a subject matching no case falls through to the code after the guard.
#[test]
fn test_no_narrow_when_switch_has_no_default() {
    expect_error(
        "<?php function consume(?array $entry, int $mode): void { if ($entry === null) { switch ($mode) { case 1: return; } } [$key, $value] = $entry; }",
        "List unpacking requires an array",
    );
}

/// Verifies a nested diverging call in only one arm does not make the enclosing `if` terminal.
#[test]
fn test_no_narrow_when_nested_exit_branch_falls_through() {
    expect_error(
        "<?php function consume(?array $entry, bool $flag): void { if ($entry === null) { if ($flag) { exit(1); } } [$key, $value] = $entry; }",
        "List unpacking requires an array",
    );
}

/// Verifies a literal-true loop that can break may still fall through to the code after the guard.
#[test]
fn test_no_narrow_when_literal_true_loop_can_break() {
    expect_error(
        "<?php function consume(?array $entry, bool $flag): void { if ($entry === null) { while (true) { if ($flag) { break; } } } [$key, $value] = $entry; }",
        "List unpacking requires an array",
    );
}

/// Verifies a static-property narrowing does not survive an intervening call that could reassign
/// it: PHP raises a `TypeError` for this program at runtime, so the compiler must keep rejecting it.
#[test]
fn test_static_property_narrowing_dropped_by_intervening_call() {
    expect_error(
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
        "Method 'S::get' return type expects S, got S|null",
    );
}

/// Verifies return-type validation is flow-sensitive: a `return` placed BEFORE the guard that
/// establishes the narrowing must not borrow that later fact.
#[test]
fn test_property_narrowing_does_not_leak_backwards_to_earlier_return() {
    expect_error(
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
        "Method 'A::f' return type expects A, got A|null",
    );
}

/// Verifies a nullable static property with no narrowing at all still fails the non-null return.
#[test]
fn test_unguarded_nullable_static_property_return_still_rejected() {
    expect_error(
        r#"<?php
class S {
    private static ?S $inst = null;
    public static function get(): S { return self::$inst; }
}
"#,
        "Method 'S::get' return type expects S, got S|null",
    );
}

/// Verifies `static::$p` is not narrowed: late static binding can select a subclass that
/// redeclares the property, so the guarded fact does not describe the storage a later read hits.
#[test]
fn test_late_static_bound_property_is_not_narrowed() {
    expect_error(
        r#"<?php
class S {
    protected static ?S $inst = null;
    public static function get(): S {
        if (static::$inst === null) { static::$inst = new S(); }
        return static::$inst;
    }
}
"#,
        "Method 'S::get' return type expects S, got S|null",
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

/// The limit of the rule: a value the BINDING cannot hold either is not the narrowing's doing,
/// and re-binding the name inside a branch is not safe — `Checker::local_binding_is_killable`
/// needs conditional depth 0 — so it stays the error it was.
#[test]
fn test_a_store_the_binding_cannot_hold_is_still_rejected() {
    expect_error(
        r#"<?php
function p(int $n): int|string { return $n === 0 ? 1 : "s"; }
$v = p($argc);
if (is_int($v)) { $v = new stdClass(); }
echo 1;
"#,
        "cannot reassign $v",
    );
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
    // The same answer in both modes, and the same one HEAD gave before this feature: the outer
    // region is not transparent, but its replay from `$a`'s own `int` sees no conflict either,
    // so `mixed_storage_scan` does not mark the name and the checker reports.
    expect_error(source, "cannot reassign $a from string to int");
    expect_error_strict(source, "cannot reassign $a from string to int");
}
