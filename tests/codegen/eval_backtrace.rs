//! Purpose:
//! End-to-end regressions for opaque eval backtraces crossing native PHP frames.
//!
//! Called from:
//! - `cargo test --test codegen_tests eval_backtrace` through the integration root.
//!
//! Key details:
//! - Default options retain native-frame arguments; `DEBUG_BACKTRACE_IGNORE_ARGS` removes them.

use crate::support::compile_and_run;

/// Opaque eval reads one suspended native frame with both backtrace option modes.
#[test]
fn opaque_eval_backtrace_preserves_native_arguments_and_options() {
    let output = compile_and_run(
        r#"<?php
function outer(string $sentinel, string $source): void {
    eval($source);
}
$source = $argc > 0 ? '
        $full = debug_backtrace();
        $ignored = debug_backtrace(DEBUG_BACKTRACE_IGNORE_ARGS);
        foreach ($full as $index => $frame) {
            if (($frame["function"] ?? "") === "outer") {
                echo $frame["args"][0], ":", isset($ignored[$index]["args"]) ? "bad" : "ignored";
                return;
            }
        }
        echo "missing";
    ' : 'echo "unreachable";';
outer("sentinel", $source);
"#,
    );
    assert_eq!(output, "sentinel:ignored");
}
