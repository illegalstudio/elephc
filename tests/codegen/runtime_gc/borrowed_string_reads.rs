//! Purpose:
//! Exercises read-only builtin string operands and their aliasing fallbacks.
//!
//! Called from:
//! - The codegen integration suite through `runtime_gc`.
//!
//! Key details:
//! - Large subjects expose whole-string copies; heap poisoning verifies result lifetimes.

use crate::support::compile_and_run_with_heap_debug;

/// Later reference promotion retains ordinary unwind roots for a widened local read.
#[test]
fn test_borrowed_string_reads_later_boxed_local() {
    let out = compile_and_run_with_heap_debug(r#"<?php
function boxReadSubject(&$s): void { $s = $s; }
function run(): void {
    $s = str_repeat("ab", 2000);
    for ($i = 0; $i < 5; $i++) {
        try { strpos($s, "b", 5000); } catch (ValueError $e) { echo "caught|"; }
    }
    boxReadSubject($s);
    echo strlen($s), "\n";
}
run();
"#);
    assert!(out.success, "{}", out.stderr);
    assert_eq!(out.stdout, "caught|caught|caught|caught|caught|4000\n", "{}", out.stderr);
    assert!(out.stderr.contains("HEAP DEBUG: leak summary: clean"), "{}", out.stderr);
}

/// Local and property reads preserve bytes while only the requested slice becomes owned.
#[test]
fn test_borrowed_string_reads_local_and_property() {
    let out = compile_and_run_with_heap_debug(r#"<?php
class ReadSubject {
    public $bytes;
    public function __construct(string $s) { $this->bytes = $s; }
    public function __get(string $name): string { return "fallback"; }
    public function byteAt(int $i): string { return substr($this->bytes, $i, 1); }
    public function findAt(int $i): int { return (int)strpos($this->bytes, "b", $i); }
}
class TypedReadSubject {
    public string $bytes;
    public function __construct(string $s) { $this->bytes = $s; }
    public function byteAt(int $i): string { return substr($this->bytes, $i, 1); }
}
function run(): void {
    $s = str_repeat("ab", 200000);
    $r = new ReadSubject($s);
    $typed = new TypedReadSubject($s);
    $sum = 0;
    for ($i = 0; $i < 40; $i++) {
        $sum += ord(substr($s, $i, 1)) + ord($r->byteAt($i)) + ord($typed->byteAt($i));
    }
    $last = -1;
    echo $sum, "|", $r->findAt(2), "|", \SUBSTR(length: 1, string: $s, offset: $last), "\n";
    $byte = $r->byteAt(0);
    unset($r);
    echo $byte, "\n";
}
run();
"#);
    assert!(out.success, "{}", out.stderr);
    assert_eq!(out.stdout, "11700|3|b\na\n", "{}", out.stderr);
    assert!(out.stderr.contains("HEAP DEBUG: leak summary: clean"), "{}", out.stderr);
}

/// Persisting a borrowed slice cannot adopt user bytes that resemble a transient heap header.
#[test]
fn test_borrowed_string_reads_reject_interior_concat_markers() {
    let out = compile_and_run_with_heap_debug(r#"<?php
function probeInteriorMarker(string $prefix): void {
    $subject = $prefix . str_repeat("Z", 32);
    $before = $subject;
    $slice = substr($subject, 8, 3);
    echo bin2hex($slice), "|", ($subject === $before ? "same" : "changed"), "|";
    $subject = str_repeat("q", 32);
    echo bin2hex($slice), "\n";
}

probeInteriorMarker(str_repeat("\x07", 8));
probeInteriorMarker("\x07\x00\x00\x00HPLE");
probeInteriorMarker("\x07\x00\x00\x00\x00\x00\x00\x00");
"#);
    assert!(out.success, "{}", out.stderr);
    assert_eq!(out.stdout, "5a5a5a|same|5a5a5a\n".repeat(3), "{}", out.stderr);
    assert!(out.stderr.contains("HEAP DEBUG: leak summary: clean"), "{}", out.stderr);
}

/// A later mutating argument still snapshots the original string in PHP evaluation order.
#[test]
fn test_borrowed_string_reads_mutating_argument_fallback() {
    let out = compile_and_run_with_heap_debug(r#"<?php
class MutableReadSubject {
    public string $bytes;
    public function __construct(string $s) { $this->bytes = $s; }
}
function replaceSubject(&$s): int { $s = str_repeat("z", 4000); return 1; }
function replaceProperty(MutableReadSubject $r): int { $r->bytes = str_repeat("z", 4000); return 1; }
function run(): void {
    $s = str_repeat("ab", 2000);
    $r = new MutableReadSubject($s);
    echo substr((string)$s, replaceSubject($s), 1), "|", substr($r->bytes, replaceProperty($r), 1), "\n";
    echo strpos(offset: replaceSubject($s), haystack: $s, needle: "z"), "\n";
}
run();
"#);
    assert!(out.success, "{}", out.stderr);
    assert_eq!(out.stdout, "b|b\n1\n", "{}", out.stderr);
    assert!(out.stderr.contains("HEAP DEBUG: leak summary: clean"), "{}", out.stderr);
}

/// Throwing searches retire their receiver lease and preserve the caller's subject owner.
#[test]
fn test_borrowed_string_reads_throwing_search() {
    let out = compile_and_run_with_heap_debug(r#"<?php
class ThrowReadSubject {
    public string $bytes;
    public function __construct(string $s) { $this->bytes = $s; }
    public function find(int $i): void { strpos($this->bytes, "b", $i); }
}
function run(): void {
    $r = new ThrowReadSubject(str_repeat("ab", 2000));
    for ($i = 0; $i < 5; $i++) {
        try { $r->find(5000); } catch (ValueError $e) { echo "caught|"; }
    }
    echo strlen($r->bytes), "\n";
}
run();
"#);
    assert!(out.success, "{}", out.stderr);
    assert_eq!(out.stdout, "caught|caught|caught|caught|caught|4000\n", "{}", out.stderr);
    assert!(out.stderr.contains("HEAP DEBUG: leak summary: clean"), "{}", out.stderr);
}

/// Transient adoption remains valid after earlier live blocks, frees, coalescing and reuse.
#[test]
fn test_borrowed_string_reads_transients_after_heap_reuse() {
    let out = compile_and_run_with_heap_debug(r#"<?php
function checkTransientReuse(): void {
    $keep = [];
    for ($i = 0; $i < 40; $i++) { $keep[] = str_repeat("q", 32 + $i); }
    $input = str_repeat("a", 70000);
    for ($i = 0; $i < 6; $i++) {
        $result = strip_tags($input);
        echo strlen($result), ":", substr($result, 0, 1), "|";
        $subject = "\x07\x00\x00\x00\x00\x00\x00\x00" . str_repeat("Z", 32);
        $slice = substr($subject, 8, 3);
        unset($result, $subject);
        echo $slice, ";";
    }
    echo count($keep), "\n";
}
checkTransientReuse();
checkTransientReuse();
"#);
    assert!(out.success, "{}", out.stderr);
    assert_eq!(out.stdout, ("70000:a|ZZZ;".repeat(6) + "40\n").repeat(2), "{}", out.stderr);
    assert!(out.stderr.contains("HEAP DEBUG: leak summary: clean"), "{}", out.stderr);
}

/// Masked trim views must be persisted before their source-evaluation pins retire.
#[test]
fn test_masked_trim_argument_view_owners_heap_clean() {
    let out = compile_and_run_with_heap_debug(r#"<?php
function maskedTrimOwner(string $text): void {
    for ($i = 0; $i < 4; $i++) {
        $left = ltrim($text, 'x');
        $right = rtrim($text, 'x');
        $both = trim($text, 'x');
        $chopped = chop($text, 'x');
        echo strlen($left), ':', strlen($right), ':', strlen($both), ':', strlen($chopped), '|';
    }
}
maskedTrimOwner(str_repeat('x', 32) . 'v' . str_repeat('x', 32));
"#);
    assert!(out.success, "stdout={} stderr={}", out.stdout, out.stderr);
    assert_eq!(out.stdout, "33:33:1:33|".repeat(4));
    assert!(out.stderr.contains("HEAP DEBUG: leak summary: clean"), "{}", out.stderr);
}

/// Persisted slices remain independently owned when user functions return boxed string unions.
#[test]
fn test_persisted_string_arguments_nullable_returns_heap_clean() {
    let out = compile_and_run_with_heap_debug(r#"<?php
function maybePersistedString(bool $keep, string $text): string|false {
    if ($keep) { return $text; }
    return false;
}
function nullablePersistedString(string $text): ?string { return $text; }
function inspectPersistedArguments(int $argc): void {
    $subject = str_repeat('ab', 32);
    for ($i = 0; $i < 4; $i++) {
        echo strlen((string)nullablePersistedString(substr($subject, 1, 8))), ':';
        echo strlen((string)maybePersistedString($argc > 0, substr($subject, 2, 8))), ':';
        echo strlen((string)maybePersistedString($argc < 0, substr($subject, 3, 8))), '|';
    }
}
inspectPersistedArguments($argc);
"#);
    assert!(out.success, "stdout={} stderr={}", out.stdout, out.stderr);
    assert_eq!(out.stdout, "8:8:0|".repeat(4));
    assert!(out.stderr.contains("HEAP DEBUG: leak summary: clean"), "{}", out.stderr);
}
/// Boxed user returns do not retain owned string calls or provisional unboxes of widened locals.
#[test]
fn test_owned_string_call_and_widened_local_arguments_boxed_returns_heap_clean() {
    let out = compile_and_run_with_heap_debug(r#"<?php
function freshForwardedString(): string { return str_repeat('s', 40); }
function boxForwardedString(string $text): mixed { return $text; }
function boxForwardedLocal(&$text): void { $text = $text; }
function widenedForwardedStrings(): void {
    for ($i = 0; $i < 4; $i++) {
        $text = freshForwardedString();
        echo strlen((string)boxForwardedString($text)), ':';
        echo strlen((string)boxForwardedString(freshForwardedString())), '|';
        boxForwardedLocal($text);
    }
}

widenedForwardedStrings();
"#);
    assert!(out.success, "stdout={} stderr={}", out.stdout, out.stderr);
    assert_eq!(out.stdout, "40:40|".repeat(4));
    assert!(out.stderr.contains("HEAP DEBUG: leak summary: clean"), "{}", out.stderr);
}

/// Boxed string passthroughs own their result cells even when the argument is borrowed.
#[test]
fn test_boxed_string_passthrough_results_own_return_cells() {
    let out = compile_and_run_with_heap_debug(r#"<?php
function mixedBorrowedString(string $text): mixed { return $text; }
function nullableBorrowedString(string $text): ?string { return $text; }
function unionBorrowedString(string $text): string|false { return $text; }
function inspectBorrowedStringBoxes(string $text): void {
    for ($i = 0; $i < 4; $i++) {
        echo strlen((string)mixedBorrowedString($text)), ':';
        echo strlen((string)nullableBorrowedString($text)), ':';
        echo strlen((string)unionBorrowedString($text)), '|';
    }
}
inspectBorrowedStringBoxes(str_repeat('s', 32));
"#);
    assert!(out.success, "stdout={} stderr={}", out.stdout, out.stderr);
    assert_eq!(out.stdout, "32:32:32|".repeat(4));
    assert!(out.stderr.contains("HEAP DEBUG: leak summary: clean"), "{}", out.stderr);
}

/// A possibly aliasing raw string return cannot retire a borrowed by-value parameter.
#[test]
fn test_conditional_string_returns_preserve_borrowed_caller_argument() {
    let out = compile_and_run_with_heap_debug(r#"<?php
function pickBorrowedString(string $text, int $flag): string {
    if ($flag) { return $text; }
    return str_repeat('z', 40);
}
function useBorrowedString(string $text, int $argc): void {
    echo strlen(pickBorrowedString($text, $argc - 1)), ':', $text, '|';
    echo strlen(pickBorrowedString($text, $argc)), ':', $text, '|';
}
useBorrowedString(str_repeat('a', 40), $argc);
"#);
    assert!(out.success, "stdout={} stderr={}", out.stdout, out.stderr);
    assert_eq!(out.stdout, format!("40:{}|", "a".repeat(40)).repeat(2));
    assert!(out.stderr.contains("HEAP DEBUG: leak summary: clean"), "{}", out.stderr);
}

/// Case transforms transfer their owned bytes into mixed arrays while scratch transforms copy.
#[test]
fn test_owned_case_transform_results_in_mixed_arrays_heap_clean() {
    let out = compile_and_run_with_heap_debug(r#"<?php
function transformedPair(string $text): mixed { return [strtolower($text), 0]; }
function checkCaseArrayOwners(string $text): void {
    for ($i = 0; $i < 4; $i++) {
        $values = [strtolower($text), 0];
        $values[] = strtoupper($text);
        $values[] = strrev($text);
        $values[] = $text;
        $pair = transformedPair($text);
        $noise = strrev(str_repeat('x', 100));
        echo substr((string)$values[0], 0, 2), ':';
        echo substr((string)$values[2], 0, 2), ':';
        echo substr((string)$values[3], 0, 2), ':';
        echo substr((string)$values[4], 0, 2), ':';
        echo substr((string)$pair[0], 0, 2), '|';
    }
}
checkCaseArrayOwners(str_repeat('Ab', 40));
"#);
    assert!(out.success, "stdout={} stderr={}", out.stdout, out.stderr);
    assert_eq!(out.stdout, "ab:AB:bA:Ab:ab|".repeat(4));
    assert!(out.stderr.contains("HEAP DEBUG: leak summary: clean"), "{}", out.stderr);
}

/// Constructorless reflection retires detached class names and preserves named reflector owners.
#[test]
fn test_constructorless_reflection_class_name_owners_heap_clean() {
    let out = compile_and_run_with_heap_debug(r#"<?php
class ReflectedNameBase {
    public function accept(ReflectedNameBase $other): ReflectedNameBase { return $other; }
}
class ReflectedNameOwner extends ReflectedNameBase {
    public string $value = 'ready';
    public function __construct() { echo 'unexpected-constructor|'; }
    public function accept(ReflectedNameBase $other): ReflectedNameBase { return $other; }
}
function makeReflectedNameOwner(): mixed {
    return (new ReflectionClass(ReflectedNameOwner::class))->newInstanceWithoutConstructor();
}
$reflector = new ReflectionClass(ReflectedNameOwner::class);
for ($i = 0; $i < 4; $i++) {
    $inline = makeReflectedNameOwner();
    $named = $reflector->newInstanceWithoutConstructor();
    if ($inline instanceof ReflectedNameOwner && $named instanceof ReflectedNameOwner) {
        echo $inline->value, ':', $named->value, '|';
    }
}
echo $reflector->getName();
"#);
    assert!(out.success, "stdout={} stderr={}", out.stdout, out.stderr);
    assert_eq!(out.stdout, "ready:ready|".repeat(4) + "ReflectedNameOwner");
    assert!(out.stderr.contains("HEAP DEBUG: leak summary: clean"), "{}", out.stderr);
}

/// Reflector class-name leases survive argument and constructor throws without consuming borrows.
#[test]
fn test_reflection_constructor_class_name_owners_heap_clean() {
    let out = compile_and_run_with_heap_debug(r#"<?php
class ReflectedConstructedNameOwner {
    public int $count;
    public function __construct(int $count = 7) {
        if ($count < 0) { throw new RuntimeException('ctor'); }
        $this->count = $count;
    }
}
function throwingReflectedNameArgument(): int { throw new RuntimeException('arg'); }
$reflector = new ReflectionClass(ReflectedConstructedNameOwner::class);
for ($i = 0; $i < 4; $i++) {
    $inline = (new ReflectionClass(ReflectedConstructedNameOwner::class))->newInstance();
    $named = $reflector->newInstance(9);
    $array = (new ReflectionClass(ReflectedConstructedNameOwner::class))->newInstanceArgs([11]);
    if ($inline instanceof ReflectedConstructedNameOwner
        && $named instanceof ReflectedConstructedNameOwner
        && $array instanceof ReflectedConstructedNameOwner) {
        echo $inline->count, ':', $named->count, ':', $array->count, '|';
    }
    try { (new ReflectionClass(ReflectedConstructedNameOwner::class))->newInstance(-1); }
    catch (RuntimeException $error) { echo $error->getMessage(), '|'; }
    try { (new ReflectionClass(ReflectedConstructedNameOwner::class))->newInstance(throwingReflectedNameArgument()); }
    catch (RuntimeException $error) { echo $error->getMessage(), '|'; }
}
echo $reflector->getName();
"#);
    assert!(out.success, "stdout={} stderr={}", out.stdout, out.stderr);
    assert_eq!(out.stdout, "7:9:11|ctor|arg|".repeat(4) + "ReflectedConstructedNameOwner");
    assert!(out.stderr.contains("HEAP DEBUG: leak summary: clean"), "{}", out.stderr);
}
