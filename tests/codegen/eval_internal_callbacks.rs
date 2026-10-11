//! Purpose:
//! End-to-end coverage for opaque eval calls that nest generated runtime callbacks.
//!
//! Called from:
//! - `cargo test --test codegen_tests eval_internal_callbacks` through the integration root.
//!
//! Key details:
//! - Runtime-selected source prevents literal lowering from bypassing the eval bridge.

use crate::support::compile_and_run;

/// Casts, predicates, arithmetic, and output buffering survive nested callback dispatch.
#[test]
fn opaque_eval_uses_internal_builtin_and_output_callbacks() {
    let output = compile_and_run(
        r#"<?php
$source = $argc > 0 ? '
    ob_start();
    echo boolval(1) ? "T" : "F";
    echo is_array([1]) ? ":A" : ":N";
    echo ":", round(2.6), ":", fdiv(8.0, 2.0);
    $captured = ob_get_clean();
    echo $captured;
' : 'echo "unreachable";';
eval($source);
"#,
    );
    assert_eq!(output, "T:A:3:4");
}
