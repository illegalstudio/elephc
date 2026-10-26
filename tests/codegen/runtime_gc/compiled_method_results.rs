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

/// Generic method result boxing retires fresh raw owners and preserves borrowed payloads.
#[test]
fn test_compiled_method_results_box_fresh_and_borrowed_owners() {
    let out = compile_and_run_with_heap_debug(r#"<?php
class CompiledMethodBoxSource {
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
    public function freshArray(): array { return [1, 2]; }
    public function borrowedArray(): array { return $this->items; }
    public function freshObject(): stdClass { return new stdClass(); }
    public function borrowedObject(): stdClass { return $this->object; }
}
class CompiledMethodBoxOther {
    public function freshText(int $n): int { return $n; }
    public function borrowedText(): int { return 0; }
    public function scratchText(string $value): int { return 0; }
    public function freshArray(): int { return 0; }
    public function borrowedArray(): int { return 0; }
    public function freshObject(): int { return 0; }
    public function borrowedObject(): int { return 0; }
}
function useCompiledMethodBox(mixed $source, int $n): void {
    echo strlen($source->freshText($n)), ':', strlen($source->borrowedText()), ':';
    echo $source->scratchText(' x '), ':';
    echo count($source->freshArray()), ':', count($source->borrowedArray()), ':';
    echo get_class($source->freshObject()), ':';
    var_dump($source->borrowedObject() === $source->borrowedObject());
}
$source = new CompiledMethodBoxSource();
for ($i = 0; $i < 3; $i++) { useCompiledMethodBox($source, 10 + $i); }
echo strlen($source->text), ':', count($source->items);
"#);
    assert!(out.success, "stdout={:?}\nstderr={}", out.stdout, out.stderr);
    assert_eq!(out.stdout, "10:32:x:2:2:stdClass:bool(true)\n11:32:x:2:2:stdClass:bool(true)\n12:32:x:2:2:stdClass:bool(true)\n32:2");
    assert!(out.stderr.contains("HEAP DEBUG: leak summary: clean"), "{}", out.stderr);
}
