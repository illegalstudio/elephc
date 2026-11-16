//! Purpose:
//! Regression tests for property element unset across readonly and runtime-typed storage.
//!
//! Called from:
//! - The object property codegen integration suite.
//!
//! Key details:
//! - ArrayAccess receives original offsets, while SPL applies its own conversion rules.
//! - Readonly permits object mutation, and false-offset diagnostics may replace the property.

use super::*;

/// Warning-capable hash keys invalidate a global closure that a handler may replace.
#[test]
fn test_unset_hash_key_diagnostic_invalidates_scoped_closure_facts() {
    for key in ["1.5", "scoped_unset_key()"] {
        let source = r#"<?php
class ScopedUnsetVault {
    private string $code = "old";
    public string $label = "new";
}
class ScopedUnsetHolder { public $items = ["keep" => 1]; }
function scoped_unset_key(): mixed { return 1.5; }
function expose_scoped_unset_global(): void { global $peek; }
$holder = new ScopedUnsetHolder();
set_error_handler(function (): bool {
    global $peek;
    $peek = function () { return $this->label; };
    return true;
});
$peek = function () { return $this->code; };
try { unset($holder->items[UNSET_KEY]); } catch (Error $error) {}
$bound = Closure::bind($peek, new ScopedUnsetVault(), ScopedUnsetVault::class);
echo $bound(), "\n";
restore_error_handler();
"#.replace("UNSET_KEY", key);
        let out = compile_and_run_with_heap_debug(&source);
        assert!(out.success, "stdout: {}\nstderr: {}", out.stdout, out.stderr);
        assert_eq!(out.stdout, "new\n");
        assert!(out.stderr.contains("HEAP DEBUG: leak summary: clean"), "{}", out.stderr);
    }
}

/// Raw associative property storage uses the same key-diagnostic snapshot as boxed PHP arrays.
#[test]
fn test_unset_associative_property_key_diagnostic_preserves_replacement() {
    let out = compile_and_run_with_heap_debug(r#"<?php
class AssociativeDiagnosticHolder { public $items = ["x" => "old", "y" => "kept"]; }
function associative_diagnostic_key(): mixed { return 1.5; }
function associative_diagnostic_unset(int $mode): void {
    $holder = new AssociativeDiagnosticHolder();
    set_error_handler(function () use ($holder): bool {
        echo "warning|";
        $holder->items = ["x" => "new", "z" => "fresh"];
        return true;
    });
    if ($mode === 0) { unset($holder->items[1.5]); }
    else { unset($holder->items[associative_diagnostic_key()]); }
    restore_error_handler();
    echo json_encode($holder->items), "\n";
}
associative_diagnostic_unset(0);
associative_diagnostic_unset(1);
"#);
    assert!(out.success, "stdout: {}\nstderr: {}", out.stdout, out.stderr);
    assert_eq!(out.stdout, "warning|{\"x\":\"new\",\"z\":\"fresh\"}\n".repeat(2));
    assert!(out.stderr.contains("HEAP DEBUG: leak summary: clean"), "{}", out.stderr);
}

/// Float-key diagnostics keep the old selected array alive without overwriting a replacement.
#[test]
fn test_unset_array_property_key_diagnostic_preserves_replacement() {
    let out = compile_and_run_with_heap_debug(r#"<?php
class DiagnosticArrayValue {
    public function __construct(public string $name) {}
    public function __destruct() { echo "drop:", $this->name, "|"; }
}
class DiagnosticArrayHolder { public array $items = []; }
function diagnostic_array_key(): mixed { return 1.5; }
function replace_during_unset_key(int $mode): void {
    $holder = new DiagnosticArrayHolder();
    $holder->items = [1 => new DiagnosticArrayValue("one"), 2 => new DiagnosticArrayValue("two")];
    set_error_handler(function () use ($holder): bool {
        echo "before|";
        $holder->items = [1 => "new", 3 => "fresh"];
        echo "after|";
        return true;
    });
    if ($mode === 0) { unset($holder->items[1.5]); }
    else { unset($holder->items[diagnostic_array_key()]); }
    restore_error_handler();
    echo "done|", json_encode($holder->items), "\n";
}
replace_during_unset_key(0);
replace_during_unset_key(1);
"#);
    assert!(out.success, "stdout: {}\nstderr: {}", out.stdout, out.stderr);
    assert_eq!(out.stdout, "before|after|drop:one|drop:two|done|{\"1\":\"new\",\"3\":\"fresh\"}\n".repeat(2));
    assert!(out.stderr.contains("HEAP DEBUG: leak summary: clean"), "{}", out.stderr);
}

/// A throwing key handler releases the retained old hash before the catch resumes.
#[test]
fn test_unset_array_property_throwing_key_diagnostic_releases_selected_cell() {
    let out = compile_and_run_with_heap_debug(r#"<?php
class ThrowingDiagnosticArrayValue {
    public function __construct(public string $name) {}
    public function __destruct() { echo "drop:", $this->name, "|"; }
}
class ThrowingDiagnosticArrayHolder { public array $items = []; }
function throwing_diagnostic_array_key(): mixed { return 1.5; }
function throw_during_unset_key(int $mode): void {
    $holder = new ThrowingDiagnosticArrayHolder();
    $holder->items = [1 => new ThrowingDiagnosticArrayValue("one"), 2 => new ThrowingDiagnosticArrayValue("two")];
    set_error_handler(function () use ($holder): bool {
        $holder->items = [1 => "new"];
        throw new RuntimeException("key");
    });
    try {
        if ($mode === 0) { unset($holder->items[1.5]); }
        else { unset($holder->items[throwing_diagnostic_array_key()]); }
    } catch (RuntimeException $error) { echo "caught:", $error->getMessage(), "|"; }
    restore_error_handler();
    echo json_encode($holder->items), "\n";
}
throw_during_unset_key(0);
throw_during_unset_key(1);
"#);
    assert!(out.success, "stdout: {}\nstderr: {}", out.stdout, out.stderr);
    assert_eq!(out.stdout, "drop:one|drop:two|caught:key|{\"1\":\"new\"}\n".repeat(2));
    assert!(out.stderr.contains("HEAP DEBUG: leak summary: clean"), "{}", out.stderr);
}

/// A still-attached array releases its snapshot before the removed value reenters destruction.
#[test]
fn test_unset_array_property_float_key_preserves_destructor_interleaving() {
    let out = compile_and_run_with_heap_debug(r#"<?php
class OrderedUnsetHolder { public array $items = []; }
class OrderedUnsetValue {
    public function __construct(public string $name, public OrderedUnsetHolder $holder) {}
    public function __destruct() {
        echo "enter:", $this->name, "|";
        if ($this->name === "one") { $this->holder->items = []; echo "cleared|"; }
        echo "exit:", $this->name, "|";
    }
}
function ordered_unset_key(): mixed { return 1; }
function ordered_unset(int $mode): void {
    $holder = new OrderedUnsetHolder();
    $holder->items = [1 => new OrderedUnsetValue("one", $holder), 2 => new OrderedUnsetValue("two", $holder)];
    set_error_handler(function (): bool { return true; });
    if ($mode === 0) { unset($holder->items[1.0]); }
    elseif ($mode === 1) { unset($holder->items[1.5]); }
    else { unset($holder->items[ordered_unset_key()]); }
    restore_error_handler();
    echo "done\n";
}
ordered_unset(0);
ordered_unset(1);
ordered_unset(2);
"#);
    assert!(out.success, "stdout: {}\nstderr: {}", out.stdout, out.stderr);
    assert_eq!(out.stdout, "enter:one|enter:two|exit:two|cleared|exit:one|done\n".repeat(3));
    assert!(out.stderr.contains("HEAP DEBUG: leak summary: clean"), "{}", out.stderr);
}

/// An uninitialized static array skips removal after evaluating and retiring its computed key.
#[test]
fn test_unset_uninitialized_static_array_property_is_quiet() {
    let out = compile_and_run_with_heap_debug(r#"<?php
class QuietStaticOffsetHolder { public static array $items; }
function quiet_static_offset_key(): string { echo "key|"; return str_repeat("k", 2); }
unset(QuietStaticOffsetHolder::$items[quiet_static_offset_key()]);
echo isset(QuietStaticOffsetHolder::$items) ? "initialized|" : "unset|";
QuietStaticOffsetHolder::$items = ["kk" => 1, "keep" => 2];
unset(QuietStaticOffsetHolder::$items[quiet_static_offset_key()]);
echo json_encode(QuietStaticOffsetHolder::$items), "\n";
"#);
    assert!(out.success, "stdout: {}\nstderr: {}", out.stdout, out.stderr);
    assert_eq!(out.stdout, "key|unset|key|{\"keep\":2}\n");
    assert!(out.stderr.contains("HEAP DEBUG: leak summary: clean"), "{}", out.stderr);
}

/// A readonly mixed property permits ArrayAccess mutation and refuses array mutation.
#[test]
fn test_unset_readonly_mixed_property_dispatches_object_offsets() {
    let out = compile_and_run_with_heap_debug(r#"<?php
class ReadonlyOffsetBag implements ArrayAccess {
    public function offsetExists(mixed $key): bool { return false; }
    public function offsetGet(mixed $key): mixed { return null; }
    public function offsetSet(mixed $key, mixed $value): void {}
    public function offsetUnset(mixed $key): void { echo "removed:", $key, "\n"; }
}
class ReadonlyOffsetHolder {
    public readonly mixed $bag;
    public function __construct(mixed $bag) { $this->bag = $bag; }
}
$object = new ReadonlyOffsetHolder(new ReadonlyOffsetBag());
unset($object->bag["key"]);
$array = new ReadonlyOffsetHolder(["key" => 1]);
try { unset($array->bag["key"]); }
catch (Error $error) { echo $error->getMessage(), "\n"; }
echo count($array->bag), "\n";
"#);
    assert!(out.success, "stdout: {}\nstderr: {}", out.stdout, out.stderr);
    assert_eq!(out.stdout, "removed:key\nCannot indirectly modify readonly property ReadonlyOffsetHolder::$bag\n1\n");
    assert!(out.stderr.contains("HEAP DEBUG: leak summary: clean"), "{}", out.stderr);
}

/// Boxed user ArrayAccess objects accept array and object offsets without array-key conversion.
#[test]
fn test_unset_boxed_array_access_accepts_array_and_object_keys() {
    let out = compile_and_run_with_heap_debug(r#"<?php
class OriginalOffsetBag implements ArrayAccess {
    public function offsetExists(mixed $key): bool { return false; }
    public function offsetGet(mixed $key): mixed { return null; }
    public function offsetSet(mixed $key, mixed $value): void {}
    public function offsetUnset(mixed $key): void { echo gettype($key), "\n"; }
}
class OriginalOffsetHolder { public mixed $bag; }
function original_array_offset(): array { return [1, 2]; }
$holder = new OriginalOffsetHolder();
$holder->bag = new OriginalOffsetBag();
unset($holder->bag[original_array_offset()], $holder->bag[new stdClass()]);
unset($holder);
"#);
    assert!(out.success, "stdout: {}\nstderr: {}", out.stdout, out.stderr);
    assert_eq!(out.stdout, "array\nobject\n");
    assert!(out.stderr.contains("HEAP DEBUG: leak summary: clean"), "{}", out.stderr);
}

/// SPL sees the original offset so null errors and fractional-offset deprecations keep PHP types.
#[test]
fn test_unset_boxed_spl_uses_original_offset_conversion() {
    let out = compile_and_run(r#"<?php
class OriginalSplHolder { public mixed $bag; }
$holder = new OriginalSplHolder();
$holder->bag = new SplFixedArray(3);
try { unset($holder->bag[null]); }
catch (TypeError $error) { echo $error->getMessage(), "\n"; }
set_error_handler(function (int $severity, string $message): bool {
    echo $severity, ":", $message, "\n";
    return true;
});
unset($holder->bag[1.5]);
echo "removed\n";
"#);
    assert_eq!(out, "Cannot access offset of type null on SplFixedArray\n8192:Implicit conversion from float 1.5 to int loses precision\nremoved\n");
}

/// A false-offset deprecation observes false and a handler's replacement array survives unchanged.
#[test]
fn test_unset_false_property_reports_deprecation_before_handler_replacement() {
    let out = compile_and_run(r#"<?php
class FalseOffsetHolder { public mixed $value = false; }
$holder = new FalseOffsetHolder();
set_error_handler(function (int $severity, string $message) use ($holder): bool {
    echo $severity, ":", $message, ":", gettype($holder->value), "\n";
    $holder->value = ["key" => 1];
    return true;
});
unset($holder->value["key"]);
echo json_encode($holder->value), "\n";
"#);
    assert_eq!(out, "8192:Automatic conversion of false to array is deprecated:boolean\n{\"key\":1}\n");
}

/// A runtime array receiver rejects array and object offsets without removing integer key zero.
#[test]
fn test_unset_mixed_array_property_rejects_array_and_object_keys() {
    let out = compile_and_run_with_heap_debug(r#"<?php
class InvalidOffsetMarker {}
class InvalidOffsetHolder { public mixed $value = [0 => "keep"]; }
function invalid_unset_key(int $n): mixed { return $n > 0 ? new InvalidOffsetMarker() : [1]; }
$holder = new InvalidOffsetHolder();
for ($i = 0; $i < 2; $i++) {
    try { unset($holder->value[invalid_unset_key($i)]); }
    catch (TypeError $error) { echo $error->getMessage(), "\n"; }
}
echo json_encode($holder->value), "\n";
"#);
    assert!(out.success, "stdout: {}\nstderr: {}", out.stdout, out.stderr);
    assert_eq!(out.stdout, "Cannot unset offset of type array on array\nCannot unset offset of type InvalidOffsetMarker on array\n[\"keep\"]\n");
    assert!(out.stderr.contains("HEAP DEBUG: leak summary: clean"), "{}", out.stderr);
}

/// A callback may replace its property while the selected object remains alive until return.
#[test]
fn test_unset_boxed_object_keeps_receiver_across_callback() {
    let out = compile_and_run_with_heap_debug(r#"<?php
class ReplacedOffsetHolder { public mixed $bag; }
class ReplacedOffsetBag implements ArrayAccess {
    public function __construct(public ReplacedOffsetHolder $holder, public string $name) {}
    public function offsetExists(mixed $key): bool { return false; }
    public function offsetGet(mixed $key): mixed { return null; }
    public function offsetSet(mixed $key, mixed $value): void {}
    public function offsetUnset(mixed $key): void {
        $this->holder->bag = null;
        echo $this->name, "|";
    }
    public function __destruct() { echo "drop|"; }
}
$holder = new ReplacedOffsetHolder();
for ($i = 0; $i < 20; $i++) {
    $holder->bag = new ReplacedOffsetBag($holder, "kept");
    unset($holder->bag["key"]);
    echo "after|";
}
echo "done\n";
"#);
    assert!(out.success, "stdout: {}\nstderr: {}", out.stdout, out.stderr);
    assert_eq!(out.stdout, "kept|drop|after|".repeat(20) + "done\n");
    assert!(out.stderr.contains("HEAP DEBUG: leak summary: clean"), "{}", out.stderr);
}
