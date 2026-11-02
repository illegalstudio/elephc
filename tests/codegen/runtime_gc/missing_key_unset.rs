//! Purpose:
//! Regressions for mutation-aware cycle collection after array element unsets.
//!
//! Called from:
//! - The runtime GC codegen integration suite.
//!
//! Key details:
//! - Collector run counts distinguish absent keys from actual removals without timing assertions.
//! - A removing `unset()` reaches `__rt_gc_safepoint`, which collects once per
//!   `GC_SAFEPOINT_THRESHOLD` (10,000) safe points, like php-src's root buffer. The fixture
//!   therefore parks the throttle one safe point short of a collection: the next counted
//!   `unset()` runs exactly one collection and an absent key runs none.

use crate::support::*;

/// Parks the safe-point throttle one step short of a collection, then leaves one unreachable
/// cycle so the productive-run counter detects the next full scan.
fn compile_unset_gc_fixture(source: &str) -> ProgramOutput {
    let setup = r#"<?php
class UnsetGarbageCycle { public $next = null; }
function parkUnsetSafepoint(): void {
    $runs = gc_status()["runs"];
    $slot = [0];
    for ($i = 0; $i <= 10000 && gc_status()["runs"] === $runs; $i++) {
        $cycle = new UnsetGarbageCycle();
        $cycle->next = $cycle;
        $cycle = null;
        unset($slot[0]);
        $slot[0] = 0;
    }
    for ($i = 1; $i < 10000; $i++) {
        unset($slot[0]);
        $slot[0] = 0;
    }
}
function leaveUnsetGarbage(): void {
    $node = new UnsetGarbageCycle();
    $node->next = $node;
}
function seedUnsetGarbage(): void {
    parkUnsetSafepoint();
    leaveUnsetGarbage();
}
"#;
    compile_and_run_with_heap_debug(&source.replacen("<?php", setup, 1))
}

/// Missing string and integer keys leave collector runs unchanged for raw and declared arrays.
#[test]
fn test_gc_missing_key_unset_skips_collection() {
    let out = compile_unset_gc_fixture(
        r#"<?php
function removeAbsent(array $headers, string $key): void {
    seedUnsetGarbage();
    $before = gc_status()["runs"];
    unset($headers[strtolower($key)]);
    echo gc_status()["runs"] - $before, ":", count($headers), "|";
    gc_collect_cycles();
}
$headers = ["Content-Type" => "text/html", "nullable" => null];
$copy = $headers;
$key = $argc > 0 ? "Transfer-Encoding" : "Trailer";
seedUnsetGarbage();
$before = gc_status()["runs"];
for ($i = 0; $i < 4; $i++) {
    unset($headers[strtolower($key)], $headers[99]);
}
echo gc_status()["runs"] - $before, ":", count($headers), ":", count($copy), "|";
gc_collect_cycles();
removeAbsent($headers, $key);
removeAbsent([], $key);
$indexed = [1, 2, 3];
seedUnsetGarbage();
$before = gc_status()["runs"];
unset($indexed[99]);
echo gc_status()["runs"] - $before, ":", count($indexed);
gc_collect_cycles();
"#,
    );
    assert!(out.success, "{}", out.stderr);
    assert_eq!(out.stdout, "0:2:2|0:2|0:0|0:3");
    assert!(out.stderr.contains("HEAP DEBUG: leak summary: clean"), "{}", out.stderr);
}

/// Null-valued entries count as removals, and mixed unset lists keep their root-drop safe point.
#[test]
fn test_gc_missing_key_unset_preserves_real_removal_boundaries() {
    let out = compile_unset_gc_fixture(
        r#"<?php
function removePresent(array $headers): void {
    seedUnsetGarbage();
    $before = gc_status()["runs"];
    unset($headers["absent"], $headers["nullable"]);
    echo gc_status()["runs"] - $before, ":", count($headers), "|";
    gc_collect_cycles();
}
$headers = ["nullable" => null, "keep" => 1];
seedUnsetGarbage();
$before = gc_status()["runs"];
unset($headers["absent"], $headers["nullable"]);
echo gc_status()["runs"] - $before, ":", count($headers), "|";
removePresent(["nullable" => null, "keep" => 1]);
$root = [1];
seedUnsetGarbage();
$before = gc_status()["runs"];
unset($headers["absent"], $root);
echo gc_status()["runs"] - $before;
"#,
    );
    assert!(out.success, "{}", out.stderr);
    assert_eq!(out.stdout, "1:1|1:1|1");
    assert!(out.stderr.contains("HEAP DEBUG: leak summary: clean"), "{}", out.stderr);
}

/// Key calls still run once in source order and retain collection even when their key is absent.
#[test]
fn test_gc_missing_key_unset_preserves_key_effects() {
    let out = compile_unset_gc_fixture(
        r#"<?php
function absentKey(string $name): string { echo $name, "|"; return $name; }
function removeComputed(array $headers): void {
    seedUnsetGarbage();
    $before = gc_status()["runs"];
    unset($headers[absentKey("first")], $headers[absentKey("second")]);
    echo gc_status()["runs"] - $before, ":", count($headers), "|";
    gc_collect_cycles();
}
$headers = ["keep" => 1];
seedUnsetGarbage();
$before = gc_status()["runs"];
unset($headers[absentKey("first")], $headers[absentKey("second")]);
echo gc_status()["runs"] - $before, ":", count($headers), "|";
removeComputed($headers);
"#,
    );
    assert!(out.success, "{}", out.stderr);
    assert_eq!(out.stdout, "first|second|1:1|first|second|1:1|");
    assert!(out.stderr.contains("HEAP DEBUG: leak summary: clean"), "{}", out.stderr);
}

/// A nested no-op unset in a destructor cannot erase the outer successful removal flag.
#[test]
fn test_gc_missing_key_unset_preserves_reentrant_removal_flag() {
    let out = compile_unset_gc_fixture(
        r#"<?php
class NestedUnsetDestructor {
    public function __destruct() {
        $nested = ["keep" => 1];
        unset($nested["absent"]);
        echo "destroy|";
    }
}
function removeReentrant(array &$items): void {
    seedUnsetGarbage();
    $before = gc_status()["runs"];
    unset($items["drop"]);
    echo gc_status()["runs"] - $before, ":", count($items), "|";
}
$items = ["drop" => new NestedUnsetDestructor(), "keep" => 1];
seedUnsetGarbage();
$before = gc_status()["runs"];
unset($items["drop"]);
echo gc_status()["runs"] - $before, ":", count($items), "|";
$declared = ["drop" => new NestedUnsetDestructor(), "keep" => 1];
removeReentrant($declared);
"#,
    );
    assert!(out.success, "{}", out.stderr);
    assert_eq!(out.stdout, "destroy|1:1|destroy|1:1|");
    assert!(out.stderr.contains("HEAP DEBUG: leak summary: clean"), "{}", out.stderr);
}

/// Removing the last external owner still collects a cyclic element and runs its destructor.
#[test]
fn test_gc_missing_key_unset_preserves_cyclic_element_destructor() {
    let out = compile_unset_gc_fixture(
        r#"<?php
class UnsetCycleElement {
    public $next = null;
    public function __destruct() { echo "cycle|"; }
}
$node = new UnsetCycleElement();
$node->next = $node;
$items = ["cycle" => $node, "keep" => 1];
unset($node);
parkUnsetSafepoint();
$before = gc_status()["runs"];
unset($items["absent"]);
echo gc_status()["runs"] - $before, "|";
unset($items["cycle"]);
echo gc_status()["runs"] - $before, ":", count($items);
"#,
    );
    assert!(out.success, "{}", out.stderr);
    assert_eq!(out.stdout, "0|cycle|1:1");
    assert!(out.stderr.contains("HEAP DEBUG: leak summary: clean"), "{}", out.stderr);
}

/// Raw hash key temporaries are retired when an element destructor escapes to a catch.
#[test]
fn test_gc_missing_key_unset_computed_key_throw_cleanup() {
    let out = compile_unset_gc_fixture(
        r#"<?php
class UnsetKeyThrowingElement {
    public function __destruct() { throw new RuntimeException("removed"); }
}
$items = ["drop" => new UnsetKeyThrowingElement(), "keep" => 41];
$key = $argc > 0 ? "DROP" : "OTHER";
try { unset($items[strtolower($key)]); }
catch (RuntimeException $error) { echo $error->getMessage(), "|"; unset($error); }
echo count($items), ":", $items["keep"];
"#,
    );
    assert!(out.success, "{}", out.stderr);
    assert_eq!(out.stdout, "removed|1:41");
    assert!(out.stderr.contains("HEAP DEBUG: leak summary: clean"), "{}", out.stderr);
}

/// Mixed reference payloads retain collection after object callbacks, null, and false warnings.
#[test]
fn test_gc_missing_key_unset_mixed_reference_fallbacks() {
    let out = compile_unset_gc_fixture(
        r#"<?php
class UnsetCollectionAccess implements ArrayAccess {
    public function offsetExists(mixed $key): bool { return false; }
    public function offsetGet(mixed $key): mixed { return null; }
    public function offsetSet(mixed $key, mixed $value): void {}
    public function offsetUnset(mixed $key): void { leaveUnsetGarbage(); }
}
function removeMixedOffset(mixed &$value): void { unset($value["absent"]); }
$object = new UnsetCollectionAccess();
parkUnsetSafepoint();
$before = gc_status()["runs"];
removeMixedOffset($object);
echo gc_status()["runs"] - $before, "|";
$nothing = null;
seedUnsetGarbage();
$before = gc_status()["runs"];
removeMixedOffset($nothing);
echo gc_status()["runs"] - $before, "|";
set_error_handler(function (int $level, string $message): bool {
    leaveUnsetGarbage();
    return true;
});
$disabled = false;
parkUnsetSafepoint();
$before = gc_status()["runs"];
removeMixedOffset($disabled);
echo gc_status()["runs"] - $before;
restore_error_handler();
"#,
    );
    assert!(out.success, "{}", out.stderr);
    assert_eq!(out.stdout, "1|1|1");
    assert!(out.stderr.contains("HEAP DEBUG: leak summary: clean"), "{}", out.stderr);
}

/// Float-key warning snapshots still reach their unconditional safe point after detached cleanup.
#[test]
fn test_gc_missing_key_unset_float_key_diagnostics() {
    let out = compile_unset_gc_fixture(
        r#"<?php
function removeFloatOffset(array &$items): void { unset($items[1.5]); }
set_error_handler(function (int $level, string $message): bool {
    leaveUnsetGarbage();
    return true;
});
$raw = ["keep" => 1];
parkUnsetSafepoint();
$before = gc_status()["runs"];
unset($raw[1.5]);
echo gc_status()["runs"] - $before, "|";
$declared = ["keep" => 1];
parkUnsetSafepoint();
$before = gc_status()["runs"];
removeFloatOffset($declared);
echo gc_status()["runs"] - $before;
restore_error_handler();
"#,
    );
    assert!(out.success, "{}", out.stderr);
    assert_eq!(out.stdout, "1|1");
    assert!(out.stderr.contains("HEAP DEBUG: leak summary: clean"), "{}", out.stderr);
}
