//! Purpose:
//! Integration tests for end-to-end codegen of `goto`/`label:` (unstructured jumps): forward jumps
//! that skip code, backward jumps that form loops, jumps out of nested blocks, and the Twig
//! `getAttribute` pattern (a `goto` from inside a `catch` to a function-level label).
//!
//! Called from:
//! - `cargo test` through Rust's test harness.
//!
//! Key details:
//! - `goto` lowers to an unconditional EIR branch to the label's (shared) block; a `label:` opens a
//!   block reachable through any `goto`, even when the textually-preceding statement terminated.
//! - Each fixture asserts PHP-equivalent stdout. Several use `$argc` so the jumped-over code is not
//!   constant-folded away before reaching EIR lowering.

use super::*;

/// Verifies a forward `goto` skips the statements between it and its label.
/// `echo "x"` is unreachable; output is `ab`.
#[test]
fn test_goto_forward_skips_code() {
    let out = compile_and_run("<?php echo \"a\"; goto skip; echo \"x\"; skip: echo \"b\";");
    assert_eq!(out, "ab");
}

/// Verifies a backward `goto` forms a loop: the label is re-entered until the guard fails.
/// Prints `012` like a counting loop.
#[test]
fn test_goto_backward_forms_loop() {
    let out = compile_and_run(
        "<?php $i = 0; loop: echo $i; $i = $i + 1; if ($i < 3) { goto loop; }",
    );
    assert_eq!(out, "012");
}

/// Verifies a `goto` jumps out of two nested `if` blocks straight to a later label, skipping the
/// code in between. The inner branch jumps to `done`, so `y` and `z` never print: output is `!`.
#[test]
fn test_goto_jumps_out_of_nested_ifs() {
    let out = compile_and_run(
        "<?php $f = true; if ($f) { if ($f) { goto done; } echo \"y\"; } echo \"z\"; done: echo \"!\";",
    );
    assert_eq!(out, "!");
}

/// Verifies the Twig `getAttribute` pattern: a `goto` inside a `catch` block forward-jumps to a
/// function-level label. With a null object the `catch` recovers and jumps to `method:`.
#[test]
fn test_goto_from_catch_block() {
    let src = r#"<?php
function get_attr($obj) {
    if ($obj === null) {
        try {
            throw new Exception("boom");
        } catch (Exception $e) {
            $obj = "recovered";
            goto method_check;
        }
    }
    return "direct:" . $obj;
    method_check:
    return "method:" . $obj;
}
echo get_attr(null), "|", get_attr("z");
"#;
    let out = compile_and_run(src);
    assert_eq!(out, "method:recovered|direct:z");
}

/// Verifies the same label name in two different functions does not conflict: labels are scoped to
/// their enclosing function body. Both functions return through their own `done:` label.
#[test]
fn test_goto_label_scoped_per_function() {
    let src = r#"<?php
function f() { goto done; return 1; done: return 10; }
function g() { goto done; return 2; done: return 20; }
echo f() + g();
"#;
    let out = compile_and_run(src);
    assert_eq!(out, "30");
}

/// Verifies a `goto` whose target is reached through a runtime-unknown condition still lands on the
/// label. `$argc` is 1 at runtime, so the branch is taken and `skipped` never prints: output `start-end`.
#[test]
fn test_goto_with_runtime_condition() {
    let out = compile_and_run(
        "<?php echo \"start-\"; if ($argc >= 1) { goto fin; } echo \"skipped-\"; fin: echo \"end\";",
    );
    assert_eq!(out, "start-end");
}
