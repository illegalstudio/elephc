//! Purpose:
//! Verifies literal-source metadata through known eval callables.
//!
//! Called from:
//! - `codegen_tests` through the codegen module tree.
//!
//! Key details:
//! - Keeps existing caller values, literal class fallback, callable mutation and return values.
//! - Checks the native caller scope and heap cleanup in one isolated program.

use crate::support::compile_and_run_with_heap_debug;

/// A known literal eval callable preserves caller scope, class fallback and ownership.
#[test]
fn test_known_callable_literal_eval_scope() {
    let output = compile_and_run_with_heap_debug(r#"<?php
$number = 40; $word = 'a'; $prefix = 'ev'; $invoke = $prefix . 'al';
$result = $invoke('$number += 2; $word .= "b"; class KnownLiteralClass { public function value(): int { return 7; } } $invoke = "strlen"; return $number . ":" . $word . ":" . (new KnownLiteralClass())->value();');
echo $result, '|', $number, ':', $word, '|', $invoke('abc');
"#);
    assert!(output.success, "stdout: {}\nstderr: {}", output.stdout, output.stderr);
    assert_eq!(output.stdout, "42:ab:7|42:ab|3");
    assert!(output.stderr.contains("HEAP DEBUG: leak summary: clean"), "{}", output.stderr);
}
