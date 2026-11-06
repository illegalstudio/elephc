//! Purpose:
//! Verifies that a local assigned from `mb_chr()` and observed only through `strlen()` is
//! retargeted to the allocation-free `MbChrLen` runtime call.
//!
//! Called from:
//! - The codegen optimizer suite through `optimizer`.
//!
//! Key details:
//! - Multiple `strlen()` observations share one retargeted call; UTF-16 and invalid codepoints
//!   keep PHP's byte length and `false` result.
//! - Any observation of the string value, including `echo` or an identity check, keeps the
//!   boxed `mb_chr()` path.

use crate::support::*;

/// Emits the optimized `main` function body for one fixture.
fn emit_main_ir(source: &str) -> String {
    let dir = make_cli_test_dir("elephc_mb_chr_len_local_ir");
    let php_path = dir.join("main.php");
    fs::write(&php_path, source).expect("write mb_chr length fixture");
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

/// UTF-8 locals observed only by `strlen()` use the length helper and keep each byte width.
#[test]
fn test_mb_chr_len_local_sink_fuses_strlen_only_locals() {
    let source = r#"<?php
$a = mb_chr(0x20AC, "UTF-8");
echo strlen($a), "|";
$b = mb_chr(0x1F600, "UTF-8");
echo strlen($b);
"#;
    assert_eq!(compile_and_run(source), "3|4");
    let main = emit_main_ir(source);
    assert_eq!(main.matches("runtime.mb_chr_len").count(), 2, "{main}");
    assert!(!main.contains("runtime.mb_chr "), "{main}");
}

/// Multiple length observations share one retargeted call; UTF-16, invalid codepoints, and
/// the bridge fallback keep PHP's exact byte length.
#[test]
fn test_mb_chr_len_local_sink_multiple_and_fallback_paths() {
    let source = r#"<?php
$a = mb_chr(65, "UTF-8");
echo strlen($a), "|", strlen($a), "|";
$b = mb_chr(65, "UTF-16");
echo strlen($b), "|";
$c = mb_chr(-1, "UTF-8");
echo strlen($c);
"#;
    assert_eq!(compile_and_run(source), "1|1|2|0");
    let main = emit_main_ir(source);
    assert_eq!(main.matches("runtime.mb_chr_len").count(), 3, "{main}");
    assert!(!main.contains("runtime.mb_chr "), "{main}");
}

/// Echoing the string value keeps the boxed call because the length helper no longer returns it.
#[test]
fn test_mb_chr_len_local_sink_keeps_value_observation() {
    let source = r#"<?php
$x = mb_chr(65, "UTF-8");
echo strlen($x), "|", $x;
"#;
    assert_eq!(compile_and_run(source), "1|A");
    let main = emit_main_ir(source);
    assert_eq!(main.matches("runtime.mb_chr ").count(), 1, "{main}");
    assert!(!main.contains("runtime.mb_chr_len"), "{main}");
}

/// A value identity check also keeps the boxed result, including the `false` branch.
#[test]
fn test_mb_chr_len_local_sink_keeps_identity_observation() {
    let source = r#"<?php
$x = mb_chr(-1, "UTF-8");
echo strlen($x), "|", $x === false ? "false" : "string";
"#;
    assert_eq!(compile_and_run(source), "0|false");
    let main = emit_main_ir(source);
    assert_eq!(main.matches("runtime.mb_chr ").count(), 1, "{main}");
    assert!(!main.contains("runtime.mb_chr_len"), "{main}");
}
