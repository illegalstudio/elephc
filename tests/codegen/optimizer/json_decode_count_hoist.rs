//! Purpose:
//! End-to-end coverage for the loop-invariant `count(json_decode(...))` probe.
//!
//! Called from:
//! - The codegen optimizer suite through `optimizer`.
//!
//! Key details:
//! - Runtime bounds from `$argc + constant` prove the loop executes at least once.
//! - Invalid JSON keeps the exact count TypeError and JSON error state.
//! - Zero-iteration loops, mutated sources and an earlier body effect reject the hoist.
//! - `--ir-opt=off` keeps the original loop and the same stdout.

use crate::support::*;

/// Emits the `main` function body for one source fixture.
fn emit_main_ir(source: &str) -> String {
    let dir = make_cli_test_dir("elephc_json_decode_count_hoist_ir");
    let php_path = dir.join("main.php");
    fs::write(&php_path, source).expect("write JSON count hoist fixture");
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

/// The probe matches PHP for valid JSON and leaves the loop error state clear.
#[test]
fn json_decode_count_hoist_matches_php() {
    let out = compile_and_run_with_heap_debug(
        r#"<?php
$s = json_encode(["a"=>1,"b"=>[1,2,3],"c"=>"text"]);
$n = 0;
$iters = (int)($argc + 3);
for ($i = 0; $i < $iters; $i++) { $n += count(json_decode($s, true)); }
echo $n, '|', json_last_error(), "\n";
"#,
    );
    assert!(out.success, "program crashed: {}", out.stderr);
    assert_eq!(out.stdout, "12|0\n");
    assert!(
        out.stderr.contains("HEAP DEBUG: leak summary: clean"),
        "{}",
        out.stderr
    );
}

/// Invalid JSON still raises PHP's count TypeError and keeps JSON_ERROR_SYNTAX.
#[test]
fn json_decode_count_hoist_invalid_json_matches_php() {
    let out = compile_and_run(
        r#"<?php
$s = '{"a":';
$iters = (int)($argc + 3);
try {
    for ($i = 0; $i < $iters; $i++) { $n += count(json_decode($s, true)); }
    echo 'no';
} catch (TypeError $e) {
    echo 'caught';
}
echo '|', json_last_error(), "\n";
"#,
    );
    assert_eq!(out, "caught|4\n");
}

/// A zero-iteration loop must not run the probe or change json_last_error.
#[test]
fn json_decode_count_hoist_zero_iterations_is_not_hoisted() {
    let source = r#"<?php
$s = json_encode(["a"=>1]);
$n = 0;
$iters = (int)$argc - 1;
for ($i = 0; $i < $iters; $i++) { $n += count(json_decode($s, true)); }
echo $n, '|', json_last_error();
"#;
    let ir = emit_main_ir(source);
    assert!(!ir.contains("json_decode_count_hoist"), "{ir}");
    let out = compile_and_run(source);
    assert_eq!(out, "0|0");
}

/// A source written in the loop keeps the per-iteration decode.
#[test]
fn json_decode_count_hoist_rejects_mutated_source() {
    let source = r#"<?php
$n = 0;
$iters = (int)($argc + 3);
for ($i = 0; $i < $iters; $i++) {
    $s = $i % 2 ? json_encode(["a"=>1]) : json_encode(["a"=>1,"b"=>2]);
    $n += count(json_decode($s, true));
}
echo $n;
"#;
    let ir = emit_main_ir(source);
    assert!(!ir.contains("json_decode_count_hoist"), "{ir}");
    assert_eq!(compile_and_run(source), "6");
}

/// An earlier body effect rejects the hoist so its ordering stays observable.
#[test]
fn json_decode_count_hoist_rejects_earlier_body_effect() {
    let source = r#"<?php
$s = json_encode(["a"=>1]);
$n = 0;
$iters = (int)($argc + 3);
for ($i = 0; $i < $iters; $i++) {
    json_decode('1');
    $n += count(json_decode($s, true));
}
echo $n;
"#;
    let ir = emit_main_ir(source);
    assert!(!ir.contains("json_decode_count_hoist"), "{ir}");
    assert_eq!(compile_and_run(source), "4");
}

/// Optimized EIR replaces the body decode with one preheader probe.
#[test]
fn json_decode_count_hoist_emits_one_probe() {
    let source = r#"<?php
$s = json_encode(["a"=>1,"b"=>[1,2,3],"c"=>"text"]);
$n = 0;
$iters = (int)($argc + 3);
for ($i = 0; $i < $iters; $i++) { $n += count(json_decode($s, true)); }
echo $n;
"#;
    let ir = emit_main_ir(source);
    assert!(ir.contains("origin: json_decode_count_hoist"), "{ir}");
    assert_eq!(ir.matches("runtime.json_decode_count ").count(), 1, "{ir}");
}

/// Disabling EIR optimization keeps the per-iteration decode and the same output.
#[test]
fn json_decode_count_hoist_no_ir_opt_parity() {
    without_ir_opt(|| {
        let out = compile_and_run(
            r#"<?php
$s = json_encode(["a"=>1,"b"=>[1,2,3],"c"=>"text"]);
$n = 0;
$iters = (int)($argc + 3);
for ($i = 0; $i < $iters; $i++) { $n += count(json_decode($s, true)); }
echo $n, "\n";
"#,
        );
        assert_eq!(out, "12\n");
    });
}
