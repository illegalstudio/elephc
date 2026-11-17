//! Purpose:
//! Verifies a write routed through a set-hook accessor releases its owning RHS temporary when
//! the accessor throws (task 926ca510).
//!
//! Called from:
//! - The runtime GC codegen integration suite.
//!
//! Key details:
//! - The caller passes the assigned value to `__propset_<p>` as a pure SSA temporary; without an
//!   unwind pin, a throw out of the accessor skipped the release after the call.
//! - Fixtures repeat the caught write in a loop so one stranded operand per catch is visible.
//! - The throwing cases cover the typed-property guard on the accessor's raw backing write and
//!   an explicit `throw`, for a boxed checked integer, a fresh array and a fresh object.
//! - The non-throwing case checks the pin retires without releasing on the normal path.

use crate::support::*;

/// A typed-property TypeError raised by the raw slot write inside a set hook releases the
/// caller's boxed checked-integer argument.
///
/// `(PHP_INT_MAX - 5) + $argc` is a heap `Mixed` cell handed to the accessor, which overflows
/// `$value * 10` into a float its `int` backing slot rejects. Before the pin, every caught
/// TypeError kept that cell alive.
#[test]
fn test_set_hook_typed_guard_type_error_releases_the_boxed_argument() {
    let out = compile_and_run_with_heap_debug(
        r#"<?php
final class GuardedHook { public int $n = 1 { set(int $value) { $this->n = $value * 10; } } }
$hooked = new GuardedHook();
$caught = 0;
$message = "";
for ($i = 0; $i < 8; $i++) {
    try { $hooked->n = (PHP_INT_MAX - 5) + $argc; }
    catch (TypeError $error) { $caught++; $message = $error->getMessage(); unset($error); }
}
echo $caught, "|", $message, "|", $hooked->n;
"#,
    );
    assert_eq!(out.stdout, "8|Cannot assign float to property GuardedHook::$n of type int|1");
    assert!(out.stderr.contains("leak summary: clean"), "{}", out.stderr);
}

/// An explicit throw from a set hook releases a fresh array or object the caller assigned.
#[test]
fn test_throwing_set_hook_releases_fresh_array_and_object_arguments() {
    let out = compile_and_run_with_heap_debug(
        r#"<?php
final class HookSetPayload { public function __construct(public int $id) {} }
final class ThrowingSetHooks {
    public array $items = [] {
        set(array $value) {
            if (count($value) > 1) { throw new RuntimeException("items " . count($value)); }
            $this->items = $value;
        }
    }
    public ?HookSetPayload $payload = null {
        set(?HookSetPayload $value) {
            if ($value !== null && $value->id > 0) { throw new RuntimeException("payload " . $value->id); }
            $this->payload = $value;
        }
    }
}
$hooked = new ThrowingSetHooks();
$caught = 0;
for ($i = 0; $i < 8; $i++) {
    try { $hooked->items = [$argc, $i, str_repeat("y", 30 + $i)]; }
    catch (RuntimeException $error) { $caught++; unset($error); }
    try { $hooked->payload = new HookSetPayload($argc + $i); }
    catch (RuntimeException $error) { $caught++; unset($error); }
}
echo $caught, "|", count($hooked->items), "|", $hooked->payload === null ? "null" : "set";
"#,
    );
    assert_eq!(out.stdout, "16|0|null");
    assert!(out.stderr.contains("leak summary: clean"), "{}", out.stderr);
}

/// A set hook that stores normally keeps its arguments balanced: the unwind pin is retired
/// without a release, so nothing is freed twice and the stored values survive.
#[test]
fn test_storing_set_hook_keeps_pinned_arguments_balanced() {
    let out = compile_and_run_with_heap_debug(
        r#"<?php
final class StoringSetHooks {
    public string $label = "" { set(string $value) { $this->label = strtoupper($value); } }
    public int $n = 0 { set(int $value) { $this->n = $value + 1; } }
    public array $items = [] { set(array $value) { $this->items = $value; } }
}
$hooked = new StoringSetHooks();
for ($i = 0; $i < 8; $i++) {
    $hooked->label = str_repeat("ab", 20 + $argc) . $i;
    $hooked->n = (PHP_INT_MAX - 20) + $argc;
    $hooked->items = [$argc, $i, str_repeat("z", 30 + $i)];
}
echo strlen($hooked->label), "|", substr($hooked->label, -3), "|", $hooked->n, "|", strlen($hooked->items[2]);
"#,
    );
    assert_eq!(out.stdout, "43|AB7|9223372036854775789|37");
    assert!(out.stderr.contains("leak summary: clean"), "{}", out.stderr);
}
