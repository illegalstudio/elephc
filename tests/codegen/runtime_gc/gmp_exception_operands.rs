//! Purpose:
//! Heap-debug regression for boxed operands abandoned by throwing GMP prelude helpers.
//!
//! Called from:
//! - `cargo test` through the runtime GC integration module.
//!
//! Key details:
//! - Each iteration catches the helper's exception, so the loop exercises the unwind path
//!   repeatedly; a clean summary proves the pre-call boxing was pinned in the unwind chain.

use crate::support::{compile_and_run_with_heap_debug, skip_without_gmp_native};

/// Verifies caught `gmp_init()`, `gmp_strval()` and `gmp_div_q()` exceptions stay heap-clean.
#[test]
fn test_gmp_function_exceptions_release_boxed_operands() {
    if skip_without_gmp_native("test_gmp_function_exceptions_release_boxed_operands") {
        return;
    }
    let out = compile_and_run_with_heap_debug(
        r#"<?php
for ($i = 0; $i < 100; $i++) {
    try { gmp_init('zz'); } catch (Throwable $e) {}
    try { gmp_strval(gmp_init(1), 99); } catch (Throwable $e) {}
    try { gmp_div_q(gmp_init(1), gmp_init(0)); } catch (Throwable $e) {}
}
echo "done\n";
"#,
    );
    assert!(out.stdout.contains("done"), "stdout: {}", out.stdout);
    assert!(
        out.stderr.contains("HEAP DEBUG: leak summary: clean"),
        "{}",
        out.stderr
    );
}
