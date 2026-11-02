//! Purpose:
//! Retires owned eval method result boxes after conversion to concrete caller ABIs.
//!
//! Called from:
//! - The executable runtime ownership CI suite.
//!
//! Key details:
//! - Escaped eval subclasses exercise interface and Mixed dispatch from ordinary AOT frames.
//! - Repeated void, scalar, string and heap results require a clean final heap.

use crate::support::*;

/// Every concrete eval result survives its box retirement and caught methods release their errors.
#[test]
fn test_escaped_eval_method_result_owners_bundle() {
    let source = r#"<?php
interface OwnedEvalCallResults {
    public function sink(): void;
    public function text(): string;
    public function number(): int;
    public function fraction(): float;
    public function truth(): bool;
    public function items(): array;
    public function identity(): object;
    public function fail(): void;
}
class OwnedEvalCallBase implements OwnedEvalCallResults {
    public function sink(): void {}
    public function text(): string { return 'base'; }
    public function number(): int { return 0; }
    public function fraction(): float { return 0.0; }
    public function truth(): bool { return false; }
    public function items(): array { return []; }
    public function identity(): object { return $this; }
    public function fail(): void { throw new RuntimeException('base-fail'); }
}
function exerciseTypedEvalCallResults(OwnedEvalCallResults $value): void {
    for ($i = 0; $i < 3; $i++) {
        $value->sink();
        echo $value->text(), ':', $value->number(), ':', $value->fraction(), ':', (int)$value->truth(), ':';
        echo json_encode($value->items()), ':', ($value->identity() === $value ? 'S|' : 'wrong|');
        try { $value->fail(); }
        catch (RuntimeException $error) { echo $error->getMessage(), '|'; unset($error); }
    }
}
function exerciseMixedEvalCallResults(mixed $value): void {
    $value->sink(); echo $value->text(), '|';
}
$code = $argc > 0 ? '
class OwnedEvalCallChild extends OwnedEvalCallBase {
    public function sink(): void {}
    public function text(): string { return str_repeat("x", 3); }
    public function number(): int { return 7; }
    public function fraction(): float { return 2.5; }
    public function truth(): bool { return true; }
    public function items(): array { return [4, 5]; }
    public function identity(): object { return $this; }
    public function fail(): void { throw new RuntimeException("eval-fail"); }
}
$child = new OwnedEvalCallChild();
' : '';
eval($code);
exerciseTypedEvalCallResults($child);
exerciseMixedEvalCallResults($child);
"#;
    let (out, assembly) = compile_and_run_with_heap_debug_and_asm(source);
    let expected = "xxx:7:2.5:1:[4,5]:S|eval-fail|".repeat(3) + "xxx|";
    preserve_stdout_mismatches(source, &assembly, std::slice::from_ref(&out), &[&expected]);
    assert!(out.success, "stdout={} stderr={}", out.stdout, out.stderr);
    assert_eq!(out.stdout, expected);
    assert!(out.stderr.contains("HEAP DEBUG: leak summary: clean"), "{}", out.stderr);
}
