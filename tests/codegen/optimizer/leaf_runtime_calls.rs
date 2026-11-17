//! Purpose:
//! Regression coverage for skipping diagnostic-site publication before leaf runtime helpers.
//!
//! Called from:
//! - The focused optimizer codegen integration suite.
//!
//! Key details:
//! - A leaf helper has no user-code effects (throw/warn/fatal/output/refcount/I/O) and no
//!   callable or object operand, so its call site cannot observe `_php_diagnostic_*`.
//! - A callback-capable helper keeps the publication because it can re-enter PHP code.

use super::*;

/// Compiles a fixture and returns its function assembly body.
fn function_asm(source: &str, name: &str) -> String {
    let dir = make_cli_test_dir("elephc_leaf_runtime_calls");
    let path = dir.join("main.php");
    fs::write(&path, source).unwrap();
    let mut command = elephc_cli_command(&dir);
    command.arg("--emit-asm");
    let output = command.arg(&path).output().unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let asm = fs::read_to_string(path.with_extension("s")).unwrap();
    fs::remove_dir_all(dir).unwrap();
    asm.split(&format!("@fn name={name}"))
        .nth(1)
        .unwrap_or_else(|| panic!("missing {name} assembly"))
        .split("@fn ")
        .next()
        .unwrap()
        .to_string()
}

/// A pure string search helper does not publish a backtrace/diagnostic site.
#[test]
fn test_leaf_string_search_skips_diagnostic_publication() {
    let function = function_asm(
        r#"<?php
function leaf_search(int $n): int {
    $s = str_repeat("a", $n) . "needle";
    $x = 0;
    for ($i = 0; $i < $n; $i++) {
        if (str_contains($s, "needle")) { $x++; }
    }
    return $x;
}
echo leaf_search($argc + 9), "\n";
"#,
        "leaf_search",
    );
    let body = function
        .split("@block name=for.body")
        .nth(1)
        .expect("search loop body");
    assert!(body.contains("__rt_strpos"), "{body}");
    assert!(
        !body.contains("_php_diagnostic_line"),
        "a leaf search helper must not publish a diagnostic site:\n{body}"
    );
}

/// A callback-capable helper keeps the diagnostic publication for a nested PHP frame.
#[test]
fn test_callback_runtime_call_keeps_diagnostic_publication() {
    let function = function_asm(
        r#"<?php
function callback_loop(int $n): int {
    $a = range(1, 10);
    $cb = $argc > 0 ? fn($v) => $v + 1 : fn($v) => $v - 1;
    $x = 0;
    for ($i = 0; $i < $n; $i++) {
        $x += array_sum(array_map($cb, $a));
    }
    return $x;
}
echo callback_loop($argc + 4), "\n";
"#,
        "callback_loop",
    );
    let body = function
        .split("@block name=for.body")
        .nth(1)
        .expect("callback loop body");
    assert!(
        body.contains("_php_diagnostic_line"),
        "a callback-capable helper must publish a call site:\n{body}"
    );
}
