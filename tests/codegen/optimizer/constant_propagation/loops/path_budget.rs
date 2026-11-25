//! Purpose:
//! Verifies bounded constant propagation for densely branching loop bodies.
//!
//! Called from:
//! - The core codegen integration suite.
//!
//! Key details:
//! - Sequential unknown conditions would otherwise produce billions of paths.
//! - Runtime cases cover zero iterations, both branches, break and continue.

use super::*;

/// Preserves runtime loop results after conservative joining of many branch states.
#[test]
fn test_constant_propagation_bounds_dense_loop_paths() {
    let mut source = String::from(
        "<?php function dense_loop_paths(int $rounds, bool $choice, int $exit): int {
         $total = 0;
for ($i = 0; $i < $rounds; ++$i) {
",
    );
    for _ in 0..36 {
        source.push_str("if ($choice) { $total += 1; } else { $total += 2; }
");
    }
    source.push_str(
        "if ($exit === 1) break;
if ($exit === 2) continue;
         $total += 100;
}
return $total;
}
         echo dense_loop_paths(0, false, 0), ':',
              dense_loop_paths(1, false, 0), ':',
              dense_loop_paths(1, true, 0), ':',
              dense_loop_paths(2, false, 0), ':',
              dense_loop_paths(2, true, 0), ':',
              dense_loop_paths(2, false, 1), ':',
              dense_loop_paths(2, true, 1), ':',
              dense_loop_paths(2, false, 2), ':',
              dense_loop_paths(2, true, 2);",
    );
    assert_eq!(compile_and_run(&source), "0:172:136:344:272:72:36:144:72");
}
