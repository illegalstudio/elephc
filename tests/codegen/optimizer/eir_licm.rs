//! Purpose:
//! Integration tests for the EIR loop-invariant code motion pass (`licm`) driven
//! by the fixed-point pass driver. These exercise real loop CFGs (for, nested,
//! while, and a loop with an invariant-looking subexpression) and assert correct
//! output, guarding that LICM — together with the dominance and loop analyses it
//! builds on — never changes loop behavior.
//!
//! Called from:
//! - `cargo test` through Rust's test harness.
//!
//! Key details:
//! - Mutable PHP loop variables still use effectful `load_local` instructions.
//!   Read-only scalar inputs and single-assignment entry locals can now be proven
//!   immutable and hoisted with dependent pure computations; issue-623 structural
//!   coverage lives in the adjacent `checked_int_sink` module. `$argc` is 1 with
//!   no CLI arguments.

use super::*;

/// A `for` loop accumulating into a slot keeps its result: `$argc + (0+1+2+3+4)`.
#[test]
fn test_licm_for_loop_sum_preserved() {
    let out = compile_and_run("<?php $sum=$argc; for($i=0;$i<5;$i++){$sum=$sum+$i;} echo $sum;");
    assert_eq!(out, "11");
}

/// A nested loop runs the inner body the full product of iterations.
#[test]
fn test_licm_nested_loop_preserved() {
    let out = compile_and_run(
        "<?php $t=0; for($i=0;$i<3;$i++){ for($j=0;$j<3;$j++){ $t=$t+1; } } echo $t;",
    );
    assert_eq!(out, "9");
}

/// A `while` loop doubling a value runs to completion.
#[test]
fn test_licm_while_loop_preserved() {
    let out = compile_and_run("<?php $i=$argc; $p=1; while($i<=4){ $p=$p*2; $i++; } echo $p;");
    assert_eq!(out, "16");
}

/// A loop whose body recomputes an invariant-looking subexpression each iteration
/// still produces the correct total.
#[test]
fn test_licm_invariant_subexpression_preserved() {
    let out = compile_and_run(
        "<?php $n=$argc; $k=$argc; $s=0; for($i=0;$i<3;$i++){ $s=$s+($n+$k); } echo $s;",
    );
    assert_eq!(out, "6");
}

/// Emits the `main` function body for one source fixture with IR optimization enabled.
fn emit_main_ir(source: &str) -> String {
    let dir = make_cli_test_dir("elephc_licm_mbstring_ir");
    let php_path = dir.join("main.php");
    fs::write(&php_path, source).expect("write LICM fixture");
    let output = elephc_cli_command(&dir)
        .arg("--emit-ir")
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

/// An explicit-UTF-8 `mb_strlen` over a single-assignment local is evaluated once
/// in the loop preheader.
#[test]
fn test_licm_hoists_invariant_mb_strlen_call() {
    let source = r#"<?php
$s = str_repeat("abcdef", $argc);
$iters = (int)($argc + 3);
$n = 0;
for ($i = 0; $i < $iters; $i++) { $n ^= mb_strlen($s, "UTF-8") + $i; }
echo $n;
"#;
    let ir = emit_main_ir(source);
    let (preheader, body) = ir.split_once("for.body:").expect("fixture has a for body");
    assert!(
        preheader.contains("runtime.mb_strlen") && preheader.contains("origin: licm"),
        "the invariant call must be hoisted into the preheader:\n{preheader}"
    );
    assert!(
        !body.contains("runtime.mb_strlen"),
        "the loop body must not call mb_strlen:\n{body}"
    );
    assert_eq!(compile_and_run(source), "0");
}

/// Reassigning the subject inside the loop keeps the call in place and preserves
/// the per-iteration result.
#[test]
fn test_licm_does_not_hoist_mb_strlen_over_a_reassigned_subject() {
    let source = r#"<?php
$s = str_repeat("abcdef", $argc);
$iters = (int)($argc + 3);
$n = 0;
for ($i = 0; $i < $iters; $i++) { $s = $i % 2 ? "abc" : "abcdef"; $n ^= mb_strlen($s, "UTF-8") + $i; }
echo $n;
"#;
    let ir = emit_main_ir(source);
    let (_preheader, body) = ir.split_once("for.body:").expect("fixture has a for body");
    assert!(
        body.contains("runtime.mb_strlen"),
        "a reassigned subject must keep the call in the loop:\n{body}"
    );
    assert_eq!(compile_and_run(source), "12");
}

/// An owned `mb_substr` result observed only by `strlen` moves with its length
/// and release, so the loop body performs no allocation.
#[test]
fn test_licm_hoists_owned_mb_substr_length_and_release() {
    let source = r#"<?php
$s = str_repeat("abcdefgh", 16);
$iters = (int)($argc + 3);
$n = 0;
for ($i = 0; $i < $iters; $i++) { $n ^= strlen(mb_substr($s, 10, 20, "UTF-8")) + $i; }
echo $n;
"#;
    let ir = emit_main_ir(source);
    let (preheader, body) = ir.split_once("for.body:").expect("fixture has a for body");
    assert!(
        preheader.contains("runtime.mb_substr")
            && preheader.contains("str_len")
            && preheader.contains("release ")
            && preheader.contains("origin: licm"),
        "the substring, length, and release must move together:\n{preheader}"
    );
    assert!(
        !body.contains("runtime.mb_substr") && !body.contains("release "),
        "the loop body must only read the hoisted length:\n{body}"
    );
    assert_eq!(compile_and_run(source), "0");
}

/// The full decimal numeric-entity map is a literal packed array, so the
/// allocation-free length call and both operand loads move into the preheader.
#[test]
fn test_licm_hoists_numericentity_length_with_the_full_decimal_map() {
    let source = r#"<?php
$s = "héllo wörld";
$map = [0x80, 0x10FFFF, 0, 0x1FFFFF];
$iters = (int)($argc + 3);
$n = 0;
for ($i = 0; $i < $iters; $i++) { $n ^= strlen(mb_encode_numericentity($s, $map, "UTF-8")) + $i; }
echo $n;
"#;
    let ir = emit_main_ir(source);
    let (preheader, body) = ir.split_once("for.body:").expect("fixture has a for body");
    assert!(
        preheader.contains("runtime.mb_encode_numericentity_len")
            && preheader.contains("origin: licm"),
        "the numeric-entity length must move into the preheader:\n{preheader}"
    );
    assert!(
        !body.contains("runtime.mb_encode_numericentity_len"),
        "the loop body must only read the hoisted length:\n{body}"
    );
    assert_eq!(compile_and_run(source), "12");
}

/// A map that is not the full decimal range keeps the call in the loop because
/// the bridge fallback may throw and its per-iteration result is not proven total.
#[test]
fn test_licm_does_not_hoist_numericentity_with_a_nonstandard_map() {
    let source = r#"<?php
$s = "héllo wörld";
$map = [0x80, 0x10FFFF, 0, 0x1FFF];
$iters = (int)($argc + 3);
$n = 0;
for ($i = 0; $i < $iters; $i++) { $n ^= strlen(mb_encode_numericentity($s, $map, "UTF-8")) + $i; }
echo $n;
"#;
    let ir = emit_main_ir(source);
    let (_preheader, body) = ir.split_once("for.body:").expect("fixture has a for body");
    assert!(
        body.contains("runtime.mb_encode_numericentity_len"),
        "a nonstandard map must keep the call in the loop:\n{body}"
    );
}
