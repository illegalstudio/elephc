//! Purpose:
//! End-to-end coverage for the runtime `memory_limit` INI surface: `ini_get()` reports the raw
//! configured string, and `ini_set()` moves the live allocator ceiling.
//!
//! Called from:
//! - `tests/codegen/runtime_gc.rs` in the codegen integration test suite.
//!
//! Key details:
//! - These fixtures compile through the CLI because `ini_get()` / `ini_set()` are prelude
//!   wrappers the CLI injects, which the in-process `compile_and_run` helper does not.
//! - The lowering fixtures assert PHP's fatal wording and exit status, so they run as
//!   subprocesses and inspect stderr instead of requiring a successful exit.

use crate::support::*;

/// Compiles `source` with the given CLI flags, runs it, and returns its captured output.
fn compile_and_run_with_flags(prefix: &str, source: &str, flags: &[&str]) -> ProgramOutput {
    let dir = make_cli_test_dir(prefix);
    let php_path = dir.join("main.php");
    fs::write(&php_path, source).unwrap();
    let compile = elephc_cli_command(&dir)
        .args(flags)
        .arg(&php_path)
        .output()
        .unwrap();
    assert!(
        compile.status.success(),
        "{}",
        String::from_utf8_lossy(&compile.stderr)
    );
    let run = run_binary(&dir.join("main"), &dir);
    fs::remove_dir_all(&dir).unwrap();
    ProgramOutput {
        stdout: String::from_utf8_lossy(&run.stdout).into_owned(),
        stderr: String::from_utf8_lossy(&run.stderr).into_owned(),
        success: run.status.success(),
    }
}

/// `ini_get()` reports the `--ini` spelling verbatim, and `ini_set()` answers the previous raw
/// string while moving `ini_get()` to the new spelling. Directive names stay case-sensitive.
#[test]
fn test_memory_limit_ini_get_and_set_report_raw_values() {
    let source = r#"<?php
echo ini_get('memory_limit'), "\n";
echo ini_set('memory_limit', '24M'), "\n";
echo ini_get('memory_limit'), "\n";
echo ini_set('memory_limit', '-1'), "\n";
echo ini_get('memory_limit'), "\n";
echo ini_get('MEMORY_LIMIT') === false ? "missing" : "found", "\n";
"#;
    let out = compile_and_run_with_flags(
        "elephc_memory_limit_raw",
        source,
        &["--ini", "memory_limit=32M"],
    );
    assert!(out.success, "program failed: {}", out.stderr);
    assert_eq!(
        out.stdout,
        "32M\n32M\n24M\n24M\n-1\nmissing\n",
        "stderr: {}",
        out.stderr
    );
}

/// Lowering the ceiling through `ini_set()` makes the next allocation fail with php-src's
/// `Allowed memory size of N bytes exhausted` fatal and exit status 255.
#[test]
fn test_ini_set_memory_limit_lowers_allocation_ceiling() {
    let source = r#"<?php
var_dump(ini_set('memory_limit', '2M'));
$a = array_fill(0, 400000, 0);
echo "not reached\n";
"#;
    let out = compile_and_run_with_flags("elephc_memory_limit_lower", source, &[]);
    assert!(!out.success, "the lowered ceiling did not fire");
    assert_eq!(out.stdout, "string(4) \"128M\"\n", "stderr: {}", out.stderr);
    assert!(
        out.stderr
            .contains("Fatal error: Allowed memory size of 2097152 bytes exhausted"),
        "{}",
        out.stderr
    );
}

/// A finite request below the current live heap usage is refused with php-src's warning and
/// leaves both the allocator ceiling and the reported raw string untouched.
#[test]
fn test_ini_set_memory_limit_below_live_usage_is_refused() {
    let source = r#"<?php
$a = array_fill(0, 300000, 0);
var_dump(ini_set('memory_limit', '1M'));
echo ini_get('memory_limit'), "\n";
"#;
    let out = compile_and_run_with_flags(
        "elephc_memory_limit_refused",
        source,
        &["--heap-size=67108864", "--ini", "memory_limit=64M"],
    );
    assert!(out.success, "program failed: {}", out.stderr);
    assert_eq!(out.stdout, "bool(false)\n64M\n", "stderr: {}", out.stderr);
    assert!(
        out.stderr.contains(
            "Warning: Failed to set memory limit to 1048576 bytes (Current memory usage is "
        ),
        "{}",
        out.stderr
    );
}

/// `-1` restores the whole reserved arena after a lower limit, so an allocation larger than the
/// previous ceiling succeeds again.
#[test]
fn test_ini_set_memory_limit_unlimited_restores_arena() {
    let source = r#"<?php
ini_set('memory_limit', '2M');
var_dump(ini_set('memory_limit', '-1'));
$a = array_fill(0, 400000, 0);
echo count($a), "\n";
"#;
    let out = compile_and_run_with_flags("elephc_memory_limit_unlimited", source, &[]);
    assert!(out.success, "program failed: {}", out.stderr);
    assert_eq!(out.stdout, "string(2) \"2M\"\n400000\n", "stderr: {}", out.stderr);
}

/// Both target paths for the runtime setter are emitted: ARM64 clamps with `csel`, x86_64 with
/// `cmovg`, and each publishes the result to `_heap_limit`. Cross-target `--emit-asm` proves the
/// non-host backend without needing to execute it.
#[test]
fn test_ini_set_memory_limit_lowering_covers_both_architectures() {
    for (target, clamp) in [("macos-aarch64", "csel"), ("linux-x86_64", "cmovg")] {
        let dir = make_cli_test_dir("elephc_memory_limit_asm");
        let php_path = dir.join("main.php");
        fs::write(
            &php_path,
            "<?php\nini_set('memory_limit', '32M');\necho ini_get('memory_limit');\n",
        )
        .unwrap();
        let output = elephc_cli_command(&dir)
            .args(["--target", target, "--emit-asm"])
            .arg(&php_path)
            .output()
            .expect("failed to emit target assembly");
        assert!(
            output.status.success(),
            "{target} --emit-asm failed: {}",
            String::from_utf8_lossy(&output.stderr)
        );
        let asm = fs::read_to_string(dir.join("main.s")).expect("expected assembly output");
        let _ = fs::remove_dir_all(&dir);
        assert!(
            asm.contains("__elephc_set_memory_limit()"),
            "{target}: the runtime setter was not emitted"
        );
        assert!(
            asm.contains(clamp),
            "{target}: the request was not clamped to the reserved arena"
        );
        assert!(
            asm.contains("_heap_limit"),
            "{target}: the live ceiling was not published"
        );
    }
}

/// Zero (and an empty string) is not PHP's unlimited spelling: only `-1` is. A finite value
/// that leaves no byte available is refused and the previous ceiling survives.
#[test]
fn test_ini_set_memory_limit_rejects_zero() {
    let source = r#"<?php
var_dump(ini_set('memory_limit', '0'));
echo ini_get('memory_limit'), "\n";
var_dump(ini_set('memory_limit', ''));
echo ini_get('memory_limit'), "\n";
"#;
    let out = compile_and_run_with_flags(
        "elephc_memory_limit_zero",
        source,
        &["--ini", "memory_limit=32M"],
    );
    assert!(out.success, "program failed: {}", out.stderr);
    assert_eq!(
        out.stdout,
        "bool(false)\n32M\nbool(false)\n32M\n",
        "stderr: {}",
        out.stderr
    );
    assert!(
        out.stderr
            .contains("Warning: Failed to set memory limit to 0 bytes"),
        "{}",
        out.stderr
    );
}

/// `ini_restore('memory_limit')` reinstalls both the startup raw string and the startup
/// ceiling, so an allocation larger than a previously lowered limit succeeds again.
#[test]
fn test_ini_restore_memory_limit_restores_startup_state() {
    let source = r#"<?php
var_dump(ini_set('memory_limit', '16M'));
var_dump(ini_get('memory_limit'));
ini_restore('memory_limit');
var_dump(ini_get('memory_limit'));
$a = array_fill(0, 3000000, 0);
echo count($a), "\n";
"#;
    let out = compile_and_run_with_flags("elephc_memory_limit_restore", source, &[]);
    assert!(out.success, "program failed: {}", out.stderr);
    assert_eq!(
        out.stdout,
        "string(4) \"128M\"\nstring(3) \"16M\"\nstring(4) \"128M\"\n3000000\n",
        "stderr: {}",
        out.stderr
    );
}

/// `ini_restore()` is injectable on its own, without `ini_get()` / `ini_set()` pulling in the
/// wider INI surface, and the shared state helper is not declared twice when both are present.
#[test]
fn test_ini_restore_is_injected_without_other_ini_wrappers() {
    let source = r#"<?php
var_dump(function_exists('ini_restore'));
ini_restore('memory_limit');
ini_restore('not.a.directive');
echo "ok\n";
"#;
    let out = compile_and_run_with_flags("elephc_memory_limit_restore_alone", source, &[]);
    assert!(out.success, "program failed: {}", out.stderr);
    assert_eq!(out.stdout, "bool(true)\nok\n", "stderr: {}", out.stderr);
}

/// `ini_get_all()` reports the dynamic `memory_limit` row only for the null/Core filters, with
/// the startup value as `global_value` and the live value as `local_value`.
#[test]
fn test_ini_get_all_reports_memory_limit_state() {
    let source = r#"<?php
$all = ini_get_all()['memory_limit'];
echo $all['global_value'], '|', $all['local_value'], '|', $all['access'], "\n";
ini_set('memory_limit', '64M');
$all = ini_get_all()['memory_limit'];
echo $all['global_value'], '|', $all['local_value'], '|', $all['access'], "\n";
echo ini_get_all('core', false)['memory_limit'], "\n";
echo (ini_get_all('mbstring', false)['memory_limit'] ?? 'absent'), "\n";
"#;
    let out = compile_and_run_with_flags(
        "elephc_memory_limit_get_all",
        source,
        &["--ini", "memory_limit=32M"],
    );
    assert!(out.success, "program failed: {}", out.stderr);
    assert_eq!(
        out.stdout,
        "32M|32M|7\n32M|64M|7\n64M\nabsent\n",
        "stderr: {}",
        out.stderr
    );
}

/// Malformed quantities emit php-src's `Invalid "memory_limit" setting` diagnostics before the
/// failed-set warning. Trailing data is still applied (as in PHP), so the raw value moves even
/// though the warning fires.
#[test]
fn test_ini_set_memory_limit_quantity_diagnostics() {
    let source = r#"<?php
foreach (['bogus', '12abc', '0x', '08', '12MM', '12 K junk'] as $v) {
    var_dump(ini_set('memory_limit', $v));
    echo ini_get('memory_limit'), "\n";
}
"#;
    let out = compile_and_run_with_flags(
        "elephc_memory_limit_diagnostics",
        source,
        &["--ini", "memory_limit=128M"],
    );
    assert!(out.success, "program failed: {}", out.stderr);
    assert_eq!(
        out.stdout,
        "bool(false)\n128M\nbool(false)\n128M\nbool(false)\n128M\nbool(false)\n128M\n\
string(4) \"128M\"\n12MM\nbool(false)\n12MM\n",
        "stderr: {}",
        out.stderr
    );
    for warning in [
        "Warning: Invalid \"memory_limit\" setting. Invalid quantity \"bogus\": no valid leading digits, interpreting as \"0\" for backwards compatibility",
        "Warning: Invalid \"memory_limit\" setting. Invalid quantity \"12abc\": unknown multiplier \"c\", interpreting as \"12\" for backwards compatibility",
        "Warning: Invalid \"memory_limit\" setting. Invalid quantity \"0x\": no digits after base prefix, interpreting as \"0\" for backwards compatibility",
        "Warning: Invalid \"memory_limit\" setting. Invalid quantity \"08\": unknown multiplier \"8\", interpreting as \"0\" for backwards compatibility",
        "Warning: Invalid \"memory_limit\" setting. Invalid quantity \"12MM\", interpreting as \"12M\" for backwards compatibility",
        "Warning: Invalid \"memory_limit\" setting. Invalid quantity \"12 K junk\", interpreting as \"12 k\" for backwards compatibility",
    ] {
        assert!(
            out.stderr.contains(warning),
            "missing `{warning}` in:\n{}",
            out.stderr
        );
    }
}

/// The unsigned quantity parser matches php-src at the u64/i64 boundaries: values above
/// `i64::MAX` are accepted without a warning, `u64::MAX` is the unlimited spelling, values above
/// `u64::MAX` warn and use the overflow result, and every other negative value keeps its
/// magnitude and warns.
#[test]
fn test_ini_set_memory_limit_unsigned_boundaries() {
    let source = r#"<?php
foreach ([
    '9223372036854775807',
    '9223372036854775808',
    '18446744073709551615',
    '18446744073709551616',
    '-9223372036854775808',
    '-2',
] as $v) {
    var_dump(ini_set('memory_limit', $v));
    echo ini_get('memory_limit'), "\n";
}
"#;
    let out = compile_and_run_with_flags(
        "elephc_memory_limit_boundaries",
        source,
        &["--ini", "memory_limit=128M"],
    );
    assert!(out.success, "program failed: {}", out.stderr);
    assert_eq!(
        out.stdout,
        "string(4) \"128M\"\n9223372036854775807\n\
string(19) \"9223372036854775807\"\n9223372036854775808\n\
string(19) \"9223372036854775808\"\n18446744073709551615\n\
string(20) \"18446744073709551615\"\n18446744073709551616\n\
string(20) \"18446744073709551616\"\n-9223372036854775808\n\
bool(false)\n-9223372036854775808\n",
        "stderr: {}",
        out.stderr
    );
    for value in [
        "18446744073709551616",
        "-9223372036854775808",
        "-2",
    ] {
        assert!(
            out.stderr.contains(&format!(
                "Warning: Invalid \"memory_limit\" setting. Invalid quantity \"{value}\": value is out of range, using overflow result for backwards compatibility"
            )),
            "missing overflow warning for {value} in:\n{}",
            out.stderr
        );
    }
    for value in ["9223372036854775807", "9223372036854775808", "18446744073709551615"] {
        assert!(
            !out.stderr.contains(&format!("Invalid quantity \"{value}\"")),
            "unexpected quantity warning for {value} in:\n{}",
            out.stderr
        );
    }
}
