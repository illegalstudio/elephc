//! Purpose:
//! Checks ownership when compiled methods return concrete values into Mixed result slots.
//!
//! Called from:
//! - The runtime GC codegen integration suite.
//!
//! Key details:
//! - Conflicting candidate signatures force generic dispatch without runtime eval.
//! - Covers fresh, borrowed and scratch strings plus fresh and borrowed containers.

use crate::support::*;

/// Concrete method returns detach borrowed strings and retain borrowed objects before later writes.
#[test]
fn test_concrete_compiled_method_borrowed_static_results_survive_replacement() {
    let out = compile_and_run_with_heap_debug(r#"<?php
class StaticRawReturnPayload {
    public int $value;
    public function __construct(int $value) { $this->value = $value; }
}
class StaticRawReturnState {
    public static string $text = '';
    public static StaticRawReturnPayload $payload;
    public function text(): string { func_get_args(); return self::$text; }
    public function payload(): StaticRawReturnPayload { func_get_args(); return self::$payload; }
}
StaticRawReturnState::$text = str_repeat('x', 32);
StaticRawReturnState::$payload = new StaticRawReturnPayload(7);
$source = new StaticRawReturnState();
$text = $source->text(); $payload = $source->payload();
StaticRawReturnState::$text = str_repeat('z', 32);
StaticRawReturnState::$payload = new StaticRawReturnPayload(9);
echo $text, ':', $payload->value, ':', strlen(StaticRawReturnState::$text), ':', StaticRawReturnState::$payload->value;
"#);
    assert!(out.success, "stdout={:?}\nstderr={}", out.stdout, out.stderr);
    assert_eq!(out.stdout, format!("{}:7:32:9", "x".repeat(32)));
    assert!(out.stderr.contains("HEAP DEBUG: leak summary: clean"), "{}", out.stderr);
}

/// Source and interface adapters preserve genuinely borrowed static-property return paths.
#[test]
fn test_compiled_method_adapters_preserve_borrowed_static_return_owners() {
    let out = compile_and_run_with_heap_debug(r#"<?php
interface StaticAdapterResultProtocol {
    public function items(): mixed;
    public function text(): mixed;
}
class StaticAdapterResultSource implements StaticAdapterResultProtocol {
    public static array $items = [];
    public static string $text = '';
    public function items(): array { func_get_args(); return self::$items; }
    public function text(): string { func_get_args(); return self::$text; }
}
class StaticAdapterResultOther {
    public function items(): int { return 0; }
    public function text(): int { return 0; }
}
function useStaticAdapterResult(mixed $source): void {
    echo count($source->items()), ':', strlen($source->text()), '|';
}
function useStaticAdapterProtocol(StaticAdapterResultProtocol $source): void {
    echo count($source->items()), ':', strlen($source->text()), '|';
}
StaticAdapterResultSource::$items = [1, 2, 3];
StaticAdapterResultSource::$text = str_repeat('t', 32);
$source = new StaticAdapterResultSource();
useStaticAdapterResult($source); useStaticAdapterResult($source);
useStaticAdapterProtocol($source); useStaticAdapterProtocol($source);
echo count(StaticAdapterResultSource::$items), ':', strlen(StaticAdapterResultSource::$text), ':', StaticAdapterResultSource::$text;
"#);
    assert!(out.success, "stdout={:?}\nstderr={}", out.stdout, out.stderr);
    assert_eq!(out.stdout, format!("{}3:32:{}", "3:32|".repeat(4), "t".repeat(32)));
    assert!(out.stderr.contains("HEAP DEBUG: leak summary: clean"), "{}", out.stderr);
}

/// Generic method result boxing retires fresh raw owners and preserves borrowed payloads.
#[test]
fn test_compiled_method_results_box_fresh_and_borrowed_owners() {
    let out = compile_and_run_with_heap_debug(r#"<?php
interface CompiledMethodBoxProtocol {
    public function adaptedText(int $n): mixed;
    public function adaptedBorrowedText(): mixed;
}
class CompiledMethodBoxSource implements CompiledMethodBoxProtocol {
    public string $text;
    public array $items;
    public stdClass $object;
    public function __construct() {
        $this->text = str_repeat('b', 32);
        $this->items = [3, 4];
        $this->object = new stdClass();
    }
    public function freshText(int $n): string { return str_repeat('a', $n); }
    public function borrowedText(): string { return $this->text; }
    public function scratchText(string $value): string { return trim($value); }
    public function concatenatedText(int $n): string { return str_repeat('d', $n) . str_repeat('e', $n); }
    public function adaptedText(int $n): string {
        $args = func_get_args(); return str_repeat('c', (int)$args[0]);
    }
    public function adaptedBorrowedText(): string {
        if (count(func_get_args()) === 0) { return $this->text; }
        return 'unused';
    }
    public function freshArray(): array { return [1, 2]; }
    public function borrowedArray(): array { return $this->items; }
    public function freshObject(): stdClass { return new stdClass(); }
    public function borrowedObject(): stdClass { return $this->object; }
}
class CompiledMethodBoxOther {
    public function freshText(int $n): int { return $n; }
    public function borrowedText(): int { return 0; }
    public function scratchText(string $value): int { return 0; }
    public function concatenatedText(int $n): int { return $n; }
    public function adaptedText(int $n): int { return $n; }
    public function adaptedBorrowedText(): int { return 0; }
    public function freshArray(): int { return 0; }
    public function borrowedArray(): int { return 0; }
    public function freshObject(): int { return 0; }
    public function borrowedObject(): int { return 0; }
}
function useCompiledMethodBox(mixed $source, int $n): void {
    echo strlen($source->freshText($n)), ':', strlen($source->borrowedText()), ':';
    echo $source->scratchText(' x '), ':';
    echo strlen($source->adaptedText($n)), ':', strlen($source->adaptedBorrowedText()), ':';
    echo count($source->freshArray()), ':', count($source->borrowedArray()), ':';
    echo get_class($source->freshObject()), ':';
    var_dump($source->borrowedObject() === $source->borrowedObject());
}
function useCompiledMethodProtocol(CompiledMethodBoxProtocol $source): void {
    echo $source->adaptedText(4), ':', strlen($source->adaptedBorrowedText()), '|';
}
function useCompiledMethodConcat(mixed $source): void {
    $large = $source->concatenatedText(40000);
    $churn = str_repeat('z', 80000);
    echo strlen($large), ':', substr((string)$large, 0, 1), ':', substr((string)$large, -1), '|';
    echo strlen($churn), '|';
}
$source = new CompiledMethodBoxSource();
for ($i = 0; $i < 3; $i++) { useCompiledMethodBox($source, 10 + $i); }
useCompiledMethodProtocol($source);
useCompiledMethodConcat($source);
echo strlen($source->text), ':', count($source->items);
"#);
    assert!(out.success, "stdout={:?}\nstderr={}", out.stdout, out.stderr);
    assert_eq!(out.stdout, "10:32:x:10:32:2:2:stdClass:bool(true)\n11:32:x:11:32:2:2:stdClass:bool(true)\n12:32:x:12:32:2:2:stdClass:bool(true)\ncccc:32|80000:d:e|80000|32:2");
    assert!(out.stderr.contains("HEAP DEBUG: leak summary: clean"), "{}", out.stderr);
}
