//! Purpose:
//! End-to-end coverage for the loop-invariant fused filter-count hoist.
//!
//! Called from:
//! - The codegen optimizer suite through `optimizer`.
//!
//! Key details:
//! - A stable packed-int receiver with a literal nonzero divisor moves its pure count to
//!   the loop preheader; a receiver mutated inside the loop keeps the per-iteration call.
//! - Negative divisors and `--ir-opt=off` parity are asserted against PHP.

use crate::support::*;

/// Emits the `main` function body for one source fixture.
fn emit_main_ir(source: &str) -> String {
    let dir = make_cli_test_dir("elephc_filter_count_hoist_ir");
    let php_path = dir.join("main.php");
    fs::write(&php_path, source).expect("write filter hoist fixture");
    let output = elephc_cli_command(&dir)
        .arg("--emit-ir")
        .arg("--ir-opt=on")
        .arg(&php_path)
        .output()
        .expect("run elephc --emit-ir");
    assert!(
        output.status.success(),
        "emit-ir failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let text = String::from_utf8(output.stdout).expect("EIR is UTF-8");
    let main = text
        .split("function main(")
        .nth(1)
        .expect("EIR contains main")
        .split("\n  function ")
        .next()
        .expect("main body")
        .to_string();
    let _ = fs::remove_dir_all(dir);
    main
}

const FIXTURE: &str = r#"<?php
$a = range(1, 100);
$n = 0;
for ($i = 0; $i < 3; $i++) { $n ^= count(array_filter($a, fn($x) => $x % 2 === 0)); }
echo $n, '|';
$neg = range(1, 10);
$n = 0;
for ($i = 0; $i < 3; $i++) { $n ^= count(array_filter($neg, fn($x) => $x % -3 === 0)); }
echo $n, '|';
$mut = range(1, 3);
$n = 0;
for ($i = 0; $i < 3; $i++) { $n ^= count(array_filter($mut, fn($x) => $x % 2 === 0)); $mut[] = 1; }
echo $n, "\n";
"#;

/// Stable and mutated receivers keep PHP's exact counts and heap ownership.
#[test]
fn array_filter_count_hoist_matches_php() {
    let out = compile_and_run_with_heap_debug(FIXTURE);
    assert!(out.success, "program crashed: {}", out.stderr);
    assert_eq!(out.stdout, "50|3|1\n");
    assert!(
        out.stderr.contains("HEAP DEBUG: leak summary: clean"),
        "{}",
        out.stderr
    );
}

/// Disabling EIR optimization leaves the per-iteration fused count.
#[test]
fn array_filter_count_hoist_no_ir_opt_parity() {
    let out = without_ir_opt(|| compile_and_run(FIXTURE));
    assert_eq!(out, "50|3|1\n");
}

/// The stable call moves before `for.body`; the mutated call stays in the body.
#[test]
fn array_filter_count_hoist_ir_is_conservative() {
    let stable = emit_main_ir(
        r#"<?php
$a = range(1, 100);
$n = 0;
for ($i = 0; $i < 3; $i++) { $n ^= count(array_filter($a, fn($x) => $x % 2 === 0)); }
echo $n, "\n";
"#,
    );
    let before_body = stable.split("for.body").next().expect("loop body marker");
    assert!(
        before_body.contains("runtime.array_filter_count_mod"),
        "{stable}"
    );
    let mutated = emit_main_ir(
        r#"<?php
$a = range(1, 3);
$n = 0;
for ($i = 0; $i < 3; $i++) { $n ^= count(array_filter($a, fn($x) => $x % 2 === 0)); $a[] = 1; }
echo $n, "\n";
"#,
    );
    let body = mutated
        .split("for.body")
        .nth(1)
        .expect("mutated loop body");
    assert!(body.contains("runtime.array_filter_count_mod"), "{mutated}");
}
