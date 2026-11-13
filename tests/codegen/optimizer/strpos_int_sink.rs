//! Purpose:
//! End-to-end coverage for the allocation-free `(int)strpos()` sink.
//!
//! Called from:
//! - The codegen optimizer suite through `optimizer`.
//!
//! Key details:
//! - Matches, absent needles, positive/negative offsets and the empty needle compare
//!   against PHP; an out-of-range offset keeps the `ValueError`.
//! - The boxed `int|false` call stays in place when any observer other than the integer
//!   cast reads the result.

use crate::support::*;

/// Cast-observed byte offsets match PHP and stay heap-clean.
#[test]
fn strpos_int_sink_matches_php() {
    let out = compile_and_run_with_heap_debug(
        r#"<?php
echo (int)strpos('abcabc', 'b'), '|';
echo (int)strpos('abc', 'z'), '|';
echo (int)strpos('abcabc', 'b', 2), '|';
echo (int)strpos('abcabc', 'b', -3), '|';
echo (int)strpos('abc', ''), '|';
$s = str_repeat('a', 100) . 'needle';
$n = 0;
$iters = $argc + 64;
for ($i = 0; $i < $iters; $i++) { $n ^= (int)strpos($s, 'needle'); }
echo $n, '|';
try { echo (int)strpos('abc', 'a', 10); } catch (ValueError $e) { echo 'VE'; }
echo "\n";
"#,
    );
    assert!(out.success, "program crashed: {}", out.stderr);
    assert_eq!(out.stdout, "1|0|4|4|0|100|VE\n");
    assert!(
        out.stderr.contains("HEAP DEBUG: leak summary: clean"),
        "{}",
        out.stderr
    );
}

/// A strict-false comparison keeps the boxed `int|false` result.
#[test]
fn strpos_boxed_result_survives_false_comparison() {
    let out = compile_and_run(
        r#"<?php
$hit = strpos('abcabc', 'b');
$miss = strpos('abc', 'z');
echo $hit === false ? 'bad' : $hit, '|', $miss === false ? 'miss' : 'bad', "\n";
"#,
    );
    assert_eq!(out, "1|miss\n");
}

/// Disabling EIR optimization leaves the boxed call and produces the same output.
#[test]
fn strpos_int_sink_no_ir_opt_parity() {
    without_ir_opt(|| {
        let out = compile_and_run(
            r#"<?php
echo (int)strpos('abcabc', 'b'), '|', (int)strpos('abc', 'z'), '|';
echo (int)strpos('abcabc', 'b', -3), "\n";
"#,
        );
        assert_eq!(out, "1|0|4\n");
    });
}

/// Every boxed position search can shed its `int|false` cell under an integer cast.
#[test]
fn search_int_sink_covers_all_four_spellings() {
    let out = compile_and_run_with_heap_debug(
        r#"<?php
$s = str_repeat('a', 40) . 'Needle' . str_repeat('a', 20) . 'needle';
echo (int)strpos($s, 'needle'), '|';
echo (int)stripos($s, 'needle'), '|';
echo (int)strrpos($s, 'needle'), '|';
echo (int)strripos($s, 'needle'), '|';
echo (int)strpos($s, 'absent'), '|';
echo (int)stripos($s, 'absent'), '|';
echo (int)strrpos($s, 'absent'), '|';
echo (int)strripos($s, 'absent'), "\n";
"#,
    );
    assert!(out.success, "program crashed: {}", out.stderr);
    assert_eq!(out.stdout, "66|40|66|66|0|0|0|0\n");
    assert!(
        out.stderr.contains("HEAP DEBUG: leak summary: clean"),
        "{}",
        out.stderr
    );
}

/// Disabling EIR optimization keeps the boxed searches and the same integer casts.
#[test]
fn search_int_sink_no_ir_opt_parity_for_all_four() {
    without_ir_opt(|| {
        let out = compile_and_run(
            r#"<?php
$s = str_repeat('a', 40) . 'Needle' . str_repeat('a', 20) . 'needle';
echo (int)strpos($s, 'needle'), '|', (int)stripos($s, 'needle'), '|';
echo (int)strrpos($s, 'needle'), '|', (int)strripos($s, 'needle'), "\n";
"#,
        );
        assert_eq!(out, "66|40|66|66\n");
    });
}

/// Emits the `main` function body for one source fixture.
fn emit_main_ir(source: &str) -> String {
    let dir = make_cli_test_dir("elephc_search_int_ir");
    let php_path = dir.join("main.php");
    fs::write(&php_path, source).expect("write search IR fixture");
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

/// The sink keeps all four searches raw instead of boxing `int|false`.
#[test]
fn search_int_sink_ir_uses_all_raw_variants() {
    let ir = emit_main_ir(
        r#"<?php
$s = str_repeat('a', 10) . 'needle';
echo (int)strpos($s, 'needle'), (int)stripos($s, 'needle');
echo (int)strrpos($s, 'needle'), (int)strripos($s, 'needle');
"#,
    );
    for raw in [
        "runtime.strpos_int",
        "runtime.stripos_int",
        "runtime.strrpos_int",
        "runtime.strripos_int",
    ] {
        assert!(ir.contains(raw), "missing {raw}: {ir}");
    }
    for boxed in ["runtime.strpos ", "runtime.stripos ", "runtime.strrpos ", "runtime.strripos "] {
        assert!(!ir.contains(boxed), "boxed search survived: {boxed}\n{ir}");
    }
}
