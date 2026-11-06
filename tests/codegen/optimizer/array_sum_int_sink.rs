//! Purpose:
//! End-to-end coverage for the allocation-free `array_sum()` int sink.
//!
//! Called from:
//! - The codegen optimizer suite through `optimizer`.
//!
//! Key details:
//! - Runtime-length packed int arrays keep the SIMD gate available; negative values and
//!   overflow-promoted sums exercise the raw scalar/float fallbacks.
//! - Float and associative receivers stay on the boxed aggregate path.

use crate::support::*;

/// Cast-observed packed int sums return the same values with and without heap boxing.
#[test]
fn array_sum_int_sink_matches_php() {
    let out = compile_and_run_with_heap_debug(
        r#"<?php
$a = range(1, $argc + 100);
$n = 0;
for ($i = 0; $i < 2001; $i++) { $n ^= array_sum($a); }
echo $n, '|';
$negative = [-3, 7, -11, 5];
echo array_sum($negative) ^ 0, '|';
$wide = [4294967296, 1, 2, 3, 4, 5, 6, 7, 8];
echo array_sum($wide) ^ 0, '|';
$float = [1.5, 2.5];
echo array_sum($float) ^ 0, '|';
$hash = ['a' => 1, 'b' => 2];
echo array_sum($hash) ^ 0, "\n";
"#,
    );
    assert!(out.success, "program crashed: {}", out.stderr);
    assert_eq!(out.stdout, "5151|-2|4294967332|4|3\n");
    assert!(
        out.stderr.contains("HEAP DEBUG: leak summary: clean"),
        "{}",
        out.stderr
    );
}

/// Emits the `main` function body for one source fixture in the requested optimizer mode.
fn emit_main_ir(source: &str, ir_opt: bool) -> String {
    let dir = make_cli_test_dir("elephc_array_sum_direct_ir");
    let php_path = dir.join("main.php");
    fs::write(&php_path, source).expect("write array-sum IR fixture");
    let mode = if ir_opt { "--ir-opt=on" } else { "--ir-opt=off" };
    let output = elephc_cli_command(&dir)
        .arg("--emit-ir")
        .arg(mode)
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

/// A stable local receiver feeds the raw reducer directly, without a temporary owner frame.
#[test]
fn array_sum_direct_source_removes_the_owner_frame() {
    let ir = emit_main_ir(
        r#"<?php
$a = range(1, 100);
$n = 0;
$n ^= array_sum($a);
$n ^= array_sum($a);
echo $n, "\n";
"#,
        true,
    );
    assert!(ir.contains("runtime.array_sum_int_value"), "{ir}");
    assert!(!ir.contains("push_call_operand_owner"), "{ir}");
    let call = ir
        .lines()
        .find(|line| line.contains("runtime.array_sum_int_value"))
        .expect("raw reducer line");
    let operand = call
        .split("= runtime_call ")
        .nth(1)
        .and_then(|rest| rest.split_whitespace().next())
        .expect("reducer operand");
    let definition = ir
        .lines()
        .find(|line| line.trim_start().starts_with(&format!("{operand}:")))
        .expect("reducer operand definition");
    assert!(definition.contains("load_local"), "{definition}");
}

/// Overflow-promoted sums keep PHP's warned float-to-int cast behavior.
#[test]
fn array_sum_int_sink_preserves_float_cast_warning() {
    let out = compile_and_run_with_heap_debug(
        r#"<?php
$overflow = [PHP_INT_MAX, PHP_INT_MAX];
$s = 0;
for ($i = 0; $i < 3; $i++) { $s ^= array_sum($overflow); }
echo $s, "\n";
"#,
    );
    assert!(out.success, "program crashed: {}", out.stderr);
    assert_eq!(out.stdout, "0\n");
    assert!(
        out.stderr
            .contains("The float 1.8446744073709552E+19 is not representable as an int"),
        "{}",
        out.stderr
    );
    assert!(
        out.stderr.contains("HEAP DEBUG: leak summary: clean"),
        "{}",
        out.stderr
    );
}

/// The loop-invariant cache handles small, wide, negative and mutated receivers.
#[test]
fn array_sum_loop_cache_matches_php() {
    let out = compile_and_run_with_heap_debug(
        r#"<?php
$small = range(1, 100);
$n = 0;
for ($i = 0; $i < 10; $i++) { $n ^= array_sum($small); }
echo $n, '|';
$large = [4294967296, 2];
$n = 0;
for ($i = 0; $i < 3; $i++) { $n ^= array_sum($large); }
echo $n, '|';
$neg = range(-100, -1);
$n = 0;
for ($i = 0; $i < 3; $i++) { $n ^= array_sum($neg); }
echo $n, '|';
$mut = range(1, 3);
$n = 0;
for ($i = 0; $i < 3; $i++) { $n ^= array_sum($mut); $mut[] = 1; }
echo $n, "\n";
"#,
    );
    assert!(out.success, "program crashed: {}", out.stderr);
    assert_eq!(out.stdout, "0|4294967298|-5050|9\n");
    assert!(
        out.stderr.contains("HEAP DEBUG: leak summary: clean"),
        "{}",
        out.stderr
    );
}

/// A stable loop receiver gets the probe/cache; a receiver mutated in the loop does not.
#[test]
fn array_sum_loop_cache_ir_is_conservative() {
    let stable = emit_main_ir(
        r#"<?php
$a = range(1, 100);
$n = 0;
for ($i = 0; $i < 3; $i++) { $n ^= array_sum($a); }
echo $n, "\n";
"#,
        true,
    );
    assert!(stable.contains("runtime.array_sum_probe_small"), "{stable}");
    assert!(stable.contains("runtime.array_sum_int_cached"), "{stable}");
    let mutated = emit_main_ir(
        r#"<?php
$a = range(1, 3);
$n = 0;
for ($i = 0; $i < 3; $i++) { $n ^= array_sum($a); $a[] = 1; }
echo $n, "\n";
"#,
        true,
    );
    assert!(!mutated.contains("runtime.array_sum_probe_small"), "{mutated}");
    assert!(!mutated.contains("runtime.array_sum_int_cached"), "{mutated}");
}

/// Disabling EIR optimization leaves the boxed call and must produce the same output.
#[test]
fn array_sum_int_sink_no_ir_opt_parity() {
    without_ir_opt(|| {
        let out = compile_and_run(
            r#"<?php
$a = range(1, $argc + 100);
$n = 0;
for ($i = 0; $i < 2001; $i++) { $n ^= array_sum($a); }
$negative = [-3, 7, -11, 5];
echo $n, '|', array_sum($negative) ^ 0, "\n";
"#,
        );
        assert_eq!(out, "5151|-2\n");
    });
}

/// The declared-int preflight branch keeps the same output with EIR optimization disabled.
#[test]
fn typed_map_sum_preflight_no_ir_opt_parity() {
    without_ir_opt(|| {
        let out = compile_and_run(
            r#"<?php
$ok = range(1, $argc + 100);
$n = 0;
for ($i = 0; $i < 2001; $i++) { $n ^= array_sum(array_map(fn(int $x): int => $x + 1, $ok)); }
echo $n, "\n";
"#,
        );
        assert_eq!(out, "5252\n");
    });
}
