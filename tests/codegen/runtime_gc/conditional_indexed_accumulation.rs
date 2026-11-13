//! Purpose:
//! Checks ownership of boxed branch results loaded through scalar merge temporaries.
//!
//! Called from:
//! - The runtime GC integration suite.
//!
//! Key details:
//! - Ternary arms combine ordinary indexed reads with a null-coalescing fallback.
//! - Reused and distinct foreach keys must both return to the pre-call heap baseline.

use crate::support::{compile_and_run_with_heap_debug, compile_and_run_with_heap_debug_tagged, without_ir_opt};

/// Both ternary arms and a missing-value default release the physical merge box each iteration.
#[test]
fn test_conditional_indexed_accumulation_releases_merge_boxes() {
    let source = include_str!("../../fixtures/conditional_indexed_accumulation.php");
    assert_eq!(source.matches("as $key => $value").count(), 1);
    let control = source.replace("as $key => $value", "as $previousKey => $value");
    assert!(!control.contains("as $key => $value"));
    for source in [source, control.as_str()] {
        let check = || {
            let out = compile_and_run_with_heap_debug_tagged(source);
            assert!(out.success, "stdout={} stderr={}", out.stdout, out.stderr);
            let expected = format!("10:10:0\n{}", "ordinary=0 silent=0 missing=0\n".repeat(3));
            assert_eq!(out.stdout, expected, "{}", out.stderr);
            assert!(out.stderr.contains("HEAP DEBUG: leak summary: clean"), "{}", out.stderr);
        };
        check();
        without_ir_opt(check);
    }
}

/// Same-storage string, object and array results retain the owner transferred out of a merge.
#[test]
fn test_conditional_indexed_accumulation_preserves_transferred_owners() {
    let check = || {
        let out = compile_and_run_with_heap_debug(r#"<?php
class ConditionalMergeBox { public function __construct(public int $value) {} }
function conditionalString(bool $choose, int $length): string {
    return $choose ? str_repeat('a', $length) : str_repeat('b', $length);
}
function conditionalBox(bool $choose): ConditionalMergeBox {
    return $choose ? new ConditionalMergeBox(3) : new ConditionalMergeBox(7);
}
function conditionalArray(bool $choose): array<int> {
    return $choose ? [1, 2] : [4, 5];
}
for ($i = 0; $i < 4; $i++) {
    $choose = $i % 2 === 0;
    $text = conditionalString($choose, $argc + 1);
    $box = conditionalBox($choose);
    $items = conditionalArray($choose);
    echo $text, ':', $box->value, ':', $items[0], '|';
    unset($text, $box, $items);
}
"#);
        assert!(out.success, "stdout={} stderr={}", out.stdout, out.stderr);
        assert_eq!(out.stdout, "aa:3:1|bb:7:4|aa:3:1|bb:7:4|", "{}", out.stderr);
        assert!(out.stderr.contains("HEAP DEBUG: leak summary: clean"), "{}", out.stderr);
    };
    check();
    without_ir_opt(check);
}
