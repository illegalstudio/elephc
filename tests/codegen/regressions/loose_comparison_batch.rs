//! Purpose:
//! Regressions for PHP loose equality in switch dispatch and null comparisons.
//!
//! Called from:
//! - The codegen integration test harness.
//!
//! Key details:
//! - Typed parameters keep comparisons observable after frontend optimization.
//! - Multi-case switches exercise dispatch rather than single-case normalization.

use super::*;

/// Issue #2149: a bool on either side selects truthiness, including non-unit integers.
#[test]
fn test_loose_comparison_batch_bool_switch() {
    let out = compile_and_run(r#"<?php
function boolSubject(bool $b): string {
    switch ($b) {
        case 2: return "two";
        case 3: return "three";
        case 0: return "zero";
        default: return "other";
    }
}
function boolLabel(int $n): string {
    switch ($n) {
        case true: return "true";
        case false: return "false";
        default: return "other";
    }
}
echo boolSubject(true), "|", boolSubject(false), "|",
    boolLabel(2), "|", boolLabel(0), "|", boolLabel(-3);
"#);
    assert_eq!(out, "two|zero|true|false|true");
}

/// Issue #2150: arrays in iterable slots never compare equal to integer labels.
#[test]
fn test_loose_comparison_batch_iterable_switch() {
    let output = compile_and_run_with_heap_debug(r#"<?php
function iterableSubject(iterable $x): string {
    switch ($x) {
        case 1: return "one";
        case 0: return "zero";
        case 2: return "two";
        default: return "other";
    }
}
function arraySubject(array $x): string {
    switch ($x) {
        case 1: return "one";
        case 0: return "zero";
        default: return "other";
    }
}
echo iterableSubject([1]), "|", iterableSubject([]), "|",
    iterableSubject(["key" => 2]), "|", arraySubject([1]), "|", arraySubject([]);
"#);
    assert!(output.success, "{}: {} {}", output.status, output.stdout, output.stderr);
    assert_eq!(output.stdout, "other|other|other|other|other");
    assert!(output.stderr.contains("HEAP DEBUG: leak summary: clean"), "{}", output.stderr);
}

/// Issue #2151: boxed object/number equality converts the object and reports its class.
#[test]
fn test_loose_comparison_batch_mixed_object_switch() {
    let out = compile_and_run(r#"<?php
set_error_handler(function(int $level, string $message): bool {
    echo "warn:", $message, "\n";
    return true;
});
function objectSubject(mixed $x): string {
    switch ($x) {
        case 2: return "two";
        case 1: return "one";
        default: return "other";
    }
}
function objectNumbers(mixed $x): void {
    var_dump($x == 1, 1 == $x, $x != 2, 2 != $x);
    var_dump($x == 1.0, 1.0 == $x, $x != 2.0, 2.0 != $x);
}
echo objectSubject(new stdClass()), "\n";
objectNumbers(new stdClass());
"#);
    let int_warning = "warn:Object of class stdClass could not be converted to int\n";
    let float_warning = "warn:Object of class stdClass could not be converted to float\n";
    assert_eq!(out, format!("{}one\n{}{}{}{}",
        int_warning.repeat(2), int_warning.repeat(4), "bool(true)\n".repeat(4),
        float_warning.repeat(4), "bool(true)\n".repeat(4)));
}

/// Issue #2152: NAN/null comparisons are quiet while explicit bool conversion still warns.
#[test]
fn test_loose_comparison_batch_nan_null() {
    let out = compile_and_run_with_php_version(r#"<?php
set_error_handler(function(int $level, string $message): bool {
    echo "warn:", $message, "\n";
    return true;
});
function floatNull(float $x): void {
    var_dump($x == null, null == $x, $x != null, null != $x);
    switch ($x) {
        case null: echo "null\n"; break;
        case 1: echo "one\n"; break;
        default: echo "other\n";
    }
}

function mixedNull(mixed $x): void {
    var_dump($x == null, null == $x, $x != null, null != $x);
}
function realBool(float $x): void { var_dump((bool) $x); }
floatNull(NAN);
floatNull(0.0);
mixedNull(NAN);
realBool(NAN);
"#, elephc::php_version::PhpVersion::Php85);
    assert_eq!(out, concat!(
        "bool(false)\nbool(false)\nbool(true)\nbool(true)\nother\n",
        "bool(true)\nbool(true)\nbool(false)\nbool(false)\nnull\n",
        "bool(false)\nbool(false)\nbool(true)\nbool(true)\n",
        "warn:unexpected NAN value was coerced to bool\nbool(true)\n",
    ));
}

/// Warning handlers can throw from unused equality expressions and switch dispatch.
#[test]
fn test_loose_comparison_batch_throwing_warning_is_heap_clean() {
    let output = compile_and_run_with_heap_debug(r#"<?php
set_error_handler(function(int $level, string $message): bool {
    throw new RuntimeException($message);
});
function unusedComparison(mixed $x): void {
    try { $x == 1; echo "miss"; }
    catch (RuntimeException $e) { echo "caught|"; }
}
function throwingSwitch(mixed $x): void {
    try {
        switch ($x) { case 1: echo "miss"; break; case 2: break; default: break; }
    } catch (RuntimeException $e) { echo "caught|"; }
}
function typedComparison(stdClass $x, int $n): void {
    try { $x == $n; } catch (RuntimeException $e) { echo "caught|"; }
}
function boxedComparison(mixed $x, mixed $n): void {
    try { $x == $n; } catch (RuntimeException $e) { echo "caught|"; }
}
function freshComparison(int $n): void {
    try { (new stdClass()) == $n; } catch (RuntimeException $e) { echo "caught|"; }
}
for ($i = 0; $i < 10; $i++) {
    typedComparison(new stdClass(), 1);
    boxedComparison(new stdClass(), 1);
    freshComparison(1);
    unusedComparison(new stdClass());
    throwingSwitch(new stdClass());
}
restore_error_handler();
"#);
    assert!(output.success, "{}: {} {}", output.status, output.stdout, output.stderr);
    assert_eq!(output.stdout, "caught|".repeat(50));
    assert!(output.stderr.contains("HEAP DEBUG: leak summary: clean"), "{}", output.stderr);
}

/// Temporary switch subjects and labels survive throwing comparison warnings without leaks.
#[test]
fn test_loose_comparison_batch_switch_temporary_owners() {
    let output = compile_and_run_with_heap_debug(r#"<?php
set_error_handler(function(int $level, string $message): bool { throw new RuntimeException($message); });
function freshCase(int $n): void {
    try { switch ($n) { case new stdClass(): echo "miss"; break; case 2: break; default: break; } }
    catch (RuntimeException $e) { echo "caught|"; }
}
function freshSubject(): void {
    try { switch (new stdClass()) { case 1: echo "miss"; break; case 2: break; default: break; } }
    catch (RuntimeException $e) { echo "caught|"; }
}
function mixedCaseValue(): mixed { return new stdClass(); }
function mixedCase(int $n): void {
    try { switch ($n) { case mixedCaseValue(): echo "miss"; break; case 2: break; default: break; } }
    catch (RuntimeException $e) { echo "caught|"; }
}
for ($i=0; $i<10; $i++) { freshCase(1); freshSubject(); mixedCase(1); }
restore_error_handler();
"#);
    assert!(output.success, "{}: {} {}", output.status, output.stdout, output.stderr);
    assert_eq!(output.stdout, "caught|".repeat(30));
    assert!(output.stderr.contains("HEAP DEBUG: leak summary: clean"), "{}", output.stderr);
}

/// An internal operator class must not duplicate the fallback's guard on a shared owner.
#[test]
fn test_loose_comparison_batch_internal_operator_shared_owner() {
    let output = compile_and_run_with_heap_debug(r#"<?php
$marker = new BcMath\Number("0"); echo $marker->compare("0"), "\n";
$shared = new stdClass(); $shared->n = 7;
set_error_handler(function(int $level, string $message): bool { throw new RuntimeException($message); });
function comparisonOwner(): stdClass { global $shared; return $shared; }
function scalarFallback(): void {
    try { comparisonOwner() == 1; }
    catch (RuntimeException $e) { echo "caught|"; }
}
for ($i=0; $i<10; $i++) { scalarFallback(); }
restore_error_handler();
echo $shared->n, "\n";
"#);
    assert!(output.success, "{}: {} {}", output.status, output.stdout, output.stderr);
    assert_eq!(output.stdout, format!("0\n{}7\n", "caught|".repeat(10)));
    assert!(output.stderr.contains("HEAP DEBUG: leak summary: clean"), "{}", output.stderr);
}

/// Recursive array and object equality retires element cells when a nested warning throws.
#[test]
fn test_loose_comparison_batch_recursive_temporary_owners() {
    let output = compile_and_run_with_heap_debug(r#"<?php
class Holder { public mixed $value; function __construct(mixed $value) { $this->value=$value; } }
set_error_handler(function(int $level, string $message): bool { throw new RuntimeException($message); });
function nestedComparison(mixed $x, mixed $y): void {
    try { $x == $y; } catch (RuntimeException $e) { echo "caught|"; }
}
for ($i=0; $i<10; $i++) {
    nestedComparison([new stdClass()], [1]);
    nestedComparison([["nested" => new stdClass()]], [["nested" => 1]]);
    nestedComparison(new Holder(new stdClass()), new Holder(1));
}
restore_error_handler();
"#);
    assert!(output.success, "{}: {} {}", output.status, output.stdout, output.stderr);
    assert_eq!(output.stdout, "caught|".repeat(30));
    assert!(output.stderr.contains("HEAP DEBUG: leak summary: clean"), "{}", output.stderr);
}

/// A warning callback can change the global or referenced variable used by subsequent cases.
#[test]
fn test_loose_comparison_batch_warning_reentry_subject() {
    let out = compile_and_run(r#"<?php
$n = $argc > 0 ? 1 : "unused";
$o = new stdClass();
set_error_handler(function(int $level, string $message): bool { global $n; $n=2; return true; });
function numericLeft(): void { global $n, $o; var_dump($n == $o); }
function numericRight(): void { global $n, $o; var_dump($o == $n); }
function numericSubject(): void {
    global $n;
    switch ($n) { case new stdClass(): echo "one\n"; break; case 2: echo "two\n"; break; default: echo "other\n"; }
}
numericLeft(); var_dump($n);
$n = $argc > 0 ? 1 : "unused";
numericRight(); var_dump($n);
$n = $argc > 0 ? 1 : "unused";
numericSubject(); var_dump($n);
restore_error_handler();
function aliasSubject(bool $flag): void {
    $n = $flag ? 1 : "unused";
    $alias =& $n;
    set_error_handler(function(int $level, string $message) use (&$n): bool { $n=2; return true; });
    switch ($alias) { case new stdClass(): echo "one\n"; break; case 2: echo "two\n"; break; default: echo "other\n"; }
    restore_error_handler();
}
aliasSubject($argc > 0);
"#);
    assert_eq!(out, "bool(false)\nint(2)\nbool(false)\nint(2)\ntwo\nint(2)\ntwo\n");
}

/// Per-case reloads retire owning views when later assignments widen the subject's frame slot.
#[test]
fn test_loose_comparison_batch_widened_subject_views() {
    let output = compile_and_run_with_heap_debug(r#"<?php
function runtimeSwitchString(int $count): string { return str_repeat("x", $count + 70); }
function widenedString(mixed $incoming, int $count): void {
    $s = runtimeSwitchString($count);
    for ($i=0; $i<$count; $i++) {
        switch ($s) { case "miss": echo "miss"; break; case "other": echo "other"; break; default: break; }
    }
    $s = $incoming; echo gettype($s), "\n";
}
function widenedArray(mixed $incoming, int $count): void {
    $s = [runtimeSwitchString($count)];
    for ($i=0; $i<$count; $i++) {
        switch ($s) { case "miss": echo "miss"; break; case "other": echo "other"; break; default: break; }
    }
    $s = $incoming; echo gettype($s), "\n";
}
function widenedObject(mixed $incoming, int $count): void {
    $s = new stdClass(); $s->value = runtimeSwitchString($count);
    for ($i=0; $i<$count; $i++) {
        switch ($s) { case "miss": echo "miss"; break; case "other": echo "other"; break; default: break; }
    }
    $s = $incoming; echo gettype($s), "\n";
}
widenedString(1, $argc * 100);
widenedArray(1, $argc * 100);
widenedObject(1, $argc * 100);
"#);
    assert!(output.success, "{}: {} {}", output.status, output.stdout, output.stderr);
    assert_eq!(output.stdout, "integer\n".repeat(3));
    assert!(output.stderr.contains("HEAP DEBUG: leak summary: clean"), "{}", output.stderr);
}

/// Owning unboxed subject views also retire when a comparison warning unwinds to a catch.
#[test]
fn test_loose_comparison_batch_widened_subject_warning() {
    let output = compile_and_run_with_heap_debug(r#"<?php
function widenedWarning(mixed $incoming, int $count): void {
    set_error_handler(function(int $level, string $message): bool { throw new RuntimeException($message); });
    for ($i=0; $i<$count; $i++) {
        $s = new stdClass(); $s->value = 7;
        try { switch ($s) { case 1: echo "miss"; break; case 2: echo "other"; break; default: break; } }
        catch (RuntimeException $e) { echo "caught|"; }
    }
    restore_error_handler(); echo $s->value, "\n";
    $s = $incoming; echo gettype($s), "\n";
}
widenedWarning(1, $argc * 100);
"#);
    assert!(output.success, "{}: {} {}", output.status, output.stdout, output.stderr);
    assert_eq!(output.stdout, format!("{}7\ninteger\n", "caught|".repeat(100)));
    assert!(output.stderr.contains("HEAP DEBUG: leak summary: clean"), "{}", output.stderr);
}

/// Direct equality and inequality guard owning local views on either side of a throwing warning.
#[test]
fn test_loose_comparison_batch_widened_direct_warning() {
    let output = compile_and_run_with_heap_debug(r#"<?php
function widenedDirectWarning(mixed $incoming, int $count): void {
    set_error_handler(function(int $level, string $message): bool { throw new RuntimeException($message); });
    for ($i=0; $i<$count; $i++) {
        $s = new stdClass(); $s->value = 7;
        try { $s == 1; echo "miss"; } catch (RuntimeException $e) { echo "caught|"; }
        try { var_dump(1 == $s); echo "miss"; } catch (RuntimeException $e) { echo "caught|"; }
        try { $s != 1; echo "miss"; } catch (RuntimeException $e) { echo "caught|"; }
        try { var_dump(1 != $s); echo "miss"; } catch (RuntimeException $e) { echo "caught|"; }
    }
    restore_error_handler(); echo $s->value, "\n";
    $s = $incoming; echo gettype($s), "\n";
}
widenedDirectWarning(1, 10);
widenedDirectWarning(1, 100);
"#);
    assert!(output.success, "{}: {} {}", output.status, output.stdout, output.stderr);
    assert_eq!(output.stdout, format!("{}7\ninteger\n{}7\ninteger\n",
        "caught|".repeat(40), "caught|".repeat(400)));
    assert!(output.stderr.contains("HEAP DEBUG: leak summary: clean"), "{}", output.stderr);
}

/// A widened case-label local owns an unboxed view that a throwing comparison must retire.
#[test]
fn test_loose_comparison_batch_widened_case_warning() {
    let output = compile_and_run_with_heap_debug(r#"<?php
function widenedCaseWarning(mixed $incoming, int $count): void {
    set_error_handler(function(int $level, string $message): bool { throw new RuntimeException($message); });
    for ($i=0; $i<$count; $i++) {
        $s = new stdClass(); $s->value = 7;
        try { switch (1) { case $s: echo "miss"; break; case 2: echo "other"; break; default: break; } }
        catch (RuntimeException $e) { echo "caught|"; }
    }
    restore_error_handler(); echo $s->value, "\n";
    $s = $incoming; echo gettype($s), "\n";
}
widenedCaseWarning(1, 10);
widenedCaseWarning(1, 100);
"#);
    assert!(output.success, "{}: {} {}", output.status, output.stdout, output.stderr);
    assert_eq!(output.stdout, format!("{}7\ninteger\n{}7\ninteger\n",
        "caught|".repeat(10), "caught|".repeat(100)));
    assert!(output.stderr.contains("HEAP DEBUG: leak summary: clean"), "{}", output.stderr);
}

/// Pending local views survive later operand warnings without leaking or consuming borrowed slots.
#[test]
fn test_loose_comparison_batch_pending_local_warning() {
    let output = compile_and_run_with_heap_debug(r#"<?php
function pendingObjectOwner(mixed $incoming, int $count): void {
    set_error_handler(function(int $level, string $message): bool { throw new RuntimeException($message); });
    for ($i=0; $i<$count; $i++) {
        $s = new stdClass(); $s->value = 7;
        try { $s == ((new stdClass()) == 1); echo "miss"; }
        catch (RuntimeException $e) { echo "caught|"; }
    }
    restore_error_handler(); echo $s->value, "\n";
    $s = $incoming; echo gettype($s), "\n";
}
function pendingArrayOwner(mixed $incoming, int $count): void {
    set_error_handler(function(int $level, string $message): bool { throw new RuntimeException($message); });
    for ($i=0; $i<$count; $i++) {
        $s = [$i];
        try { $s == ((new stdClass()) == 1); echo "miss"; }
        catch (RuntimeException $e) { echo "caught|"; }
    }
    restore_error_handler(); echo count($s), "\n";
    $s = $incoming; echo gettype($s), "\n";
}
function pendingBorrowedObject(int $count): void {
    set_error_handler(function(int $level, string $message): bool { throw new RuntimeException($message); });
    for ($i=0; $i<$count; $i++) {
        $s = new stdClass(); $s->value = 7;
        try { $s == ((new stdClass()) == 1); echo "miss"; }
        catch (RuntimeException $e) { echo "caught|"; }
    }
    restore_error_handler(); echo $s->value, "\n";
}
pendingObjectOwner(1, 10); pendingObjectOwner(1, 100);
pendingArrayOwner(1, 10); pendingArrayOwner(1, 100);
pendingBorrowedObject(10); pendingBorrowedObject(100);
"#);
    assert!(output.success, "{}: {} {}", output.status, output.stdout, output.stderr);
    let objects = format!("{}7\ninteger\n{}7\ninteger\n", "caught|".repeat(10), "caught|".repeat(100));
    let arrays = format!("{}1\ninteger\n{}1\ninteger\n", "caught|".repeat(10), "caught|".repeat(100));
    let borrowed = format!("{}7\n{}7\n", "caught|".repeat(10), "caught|".repeat(100));
    assert_eq!(output.stdout, format!("{objects}{arrays}{borrowed}"));
    assert!(output.stderr.contains("HEAP DEBUG: leak summary: clean"), "{}", output.stderr);
}

/// A loose guard cannot fold its complement after a warning handler changes its source.
#[test]
fn test_loose_comparison_batch_warning_invalidates_nested_guard() {
    let output = compile_and_run_with_heap_debug(r#"<?php
$value = $argc > 0 ? new stdClass() : 0;
set_error_handler(function(int $level, string $message): bool {
    global $value; $value = 2; return true;
});
if ($value == 1) {
    if ($value != 1) { echo "changed\n"; }
    else { echo "stale\n"; }
} else { echo "outerfalse\n"; }
restore_error_handler();
"#);
    assert!(output.success, "{}: {} {}", output.status, output.stdout, output.stderr);
    assert_eq!(output.stdout, "changed\n");
    assert!(output.stderr.contains("HEAP DEBUG: leak summary: clean"), "{}", output.stderr);
}
