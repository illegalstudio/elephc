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
        status: run.status,
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

/// `ini_restore('memory_limit')` restores a startup value the fixed arena refused by falling
/// back to the arena's actual startup ceiling, and only then reports the startup text again.
/// Regression: a 64 MiB `--heap-size` with the 128 MiB default left the allocator at the lowered
/// 2 MiB while `ini_get()` claimed the restore had succeeded.
#[test]
fn test_ini_restore_memory_limit_restores_the_actual_startup_ceiling() {
    let source = r#"<?php
echo ini_get('memory_limit'), "\n";
ini_set('memory_limit', '2M');
echo ini_get('memory_limit'), "\n";
ini_restore('memory_limit');
echo ini_get('memory_limit'), "\n";
echo strlen(str_repeat("a", 3 * 1024 * 1024)), "\n";
"#;
    let out = compile_and_run_with_flags(
        "elephc_memory_limit_restore",
        source,
        &["--heap-size=67108864"],
    );
    assert!(out.success, "program failed: {}", out.stderr);
    assert_eq!(
        out.stdout,
        "128M\n2M\n128M\n3145728\n",
        "stderr: {}",
        out.stderr
    );
}

/// `ini_restore('memory_limit')` runs the same live-usage refusal as `ini_set()`: a finite
/// startup value below the rounded live footprint is refused, the raised ceiling and raw
/// string stay installed, and the next allocation succeeds.
#[test]
fn test_ini_restore_memory_limit_refuses_below_live_usage() {
    let source = r#"<?php
var_dump(ini_set('memory_limit', '32M'));
$array = array_fill(0, 1500000, 0);
ini_restore('memory_limit');
echo ini_get('memory_limit'), "\n";
$small = array_fill(0, 10, 0);
echo "ok\n";
"#;
    let out = compile_and_run_with_flags(
        "elephc_memory_limit_restore_live",
        source,
        &["--ini", "memory_limit=8M"],
    );
    assert!(out.success, "program failed: {}", out.stderr);
    assert_eq!(out.stdout, "string(2) \"8M\"\n32M\nok\n", "stderr: {}", out.stderr);
    assert!(
        out.stderr.contains(
            "Failed to set memory limit to 8388608 bytes (Current memory usage is "
        ),
        "{}",
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

/// A string built above the runtime `memory_limit` reports php-src's Allowed-memory fatal and
/// exits 255 instead of the reserved-arena overflow diagnostic.
#[test]
fn test_string_growth_above_memory_limit_uses_allowed_memory_fatal() {
    let source = r#"<?php
$s = str_repeat("a", 3 * 1024 * 1024);
echo strlen($s), "\n";
"#;
    let out = compile_and_run_with_flags(
        "elephc_memory_limit_string_growth",
        source,
        &["--ini", "memory_limit=2M"],
    );
    assert!(!out.success, "the string growth did not hit the limit");
    assert!(out.stdout.is_empty(), "stdout: {}", out.stdout);
    assert_eq!(out.status.code(), Some(255));
    // The grown string capacity is target/allocator dependent: this path reserves the exact
    // 3 MiB on Linux and a power-of-two 4 MiB block on macOS.
    assert!(
        out.stderr.contains(
            "Fatal error: Allowed memory size of 2097152 bytes exhausted (tried to allocate "
        ) && out.stderr.contains(" bytes) in Unknown on line 0"),
        "{}",
        out.stderr
    );
}

/// A sprintf width above the memory ceiling raises php-src's fatal instead of killing the
/// process at the hard limit.
#[test]
fn test_sprintf_width_above_memory_limit_reports_allowed_memory_fatal() {
    let source = r#"<?php
$s = sprintf("%10000000s", "");
echo "not reached\n";
"#;
    let out = compile_and_run_with_flags(
        "elephc_memory_limit_sprintf_width",
        source,
        &["--ini", "memory_limit=2M"],
    );
    assert!(!out.success, "the sprintf width did not hit the limit");
    assert_eq!(out.status.code(), Some(255));
    assert!(out.stdout.is_empty(), "stdout: {}", out.stdout);
    assert!(
        out.stderr
            .contains("Fatal error: Allowed memory size of 2097152 bytes exhausted"),
        "{}",
        out.stderr
    );
}

/// A malformed startup quantity that parses below the 2 MiB floor prints PHP's quantity
/// diagnostic and then the failed-set warning, each exactly once. Removing the override during
/// floor processing must not lose the quantity line before the one-warning pass runs.
#[test]
fn test_startup_memory_limit_malformed_below_floor_warns_once() {
    let stderr = compile_cli_file_with_flags_capturing_stderr(
        "<?php echo ini_get('memory_limit');",
        &["--ini", "memory_limit=oops"],
    );
    assert_eq!(
        stderr.matches("Invalid \"memory_limit\" setting.").count(),
        1,
        "quantity diagnostic must not repeat: {stderr}"
    );
    assert_eq!(
        stderr.matches("Failed to set memory limit to 0 bytes").count(),
        1,
        "failed-set warning must not repeat: {stderr}"
    );
}

/// The valid unsigned maximum stays silent, while a value one byte above it keeps the
/// overflow warning instead of being silently wrapped by the signed quantity mirror.
#[test]
fn test_startup_memory_limit_preserves_unsigned_overflow_warning() {
    let quiet = compile_cli_file_with_flags_capturing_stderr(
        "<?php echo 1;",
        &["--ini", "memory_limit=18446744073709551615"],
    );
    assert!(
        !quiet.contains("Invalid \"memory_limit\" setting."),
        "unexpected warning for the valid u64 maximum: {quiet}"
    );

    let overflow = compile_cli_file_with_flags_capturing_stderr(
        "<?php echo 1;",
        &["--ini", "memory_limit=18446744073709551616"],
    );
    assert!(
        overflow.contains(
            "Invalid quantity \"18446744073709551616\": value is out of range, using overflow result"
        ),
        "missing overflow warning: {overflow}"
    );
    assert_eq!(
        overflow.matches("Invalid \"memory_limit\" setting.").count(),
        1,
        "php prints one startup quantity warning, not one per sizing pass: {overflow}"
    );
}

/// A `K`/`M`/`G` suffix applies to the UNSIGNED magnitude modulo 2^64. The signed parser
/// wraps `9223372036854784000K` negative before the suffix; the unsigned result is
/// `(2^63 + 8192) * 1024` = 8 MiB, so a 12 MiB string hits that ceiling instead of the arena.
#[test]
fn test_startup_memory_limit_applies_suffix_to_the_unsigned_magnitude() {
    let source = r#"<?php
echo ini_get('memory_limit'), "\n";
$s = str_repeat("a", 12 * 1024 * 1024);
echo strlen($s), "\n";
"#;
    let out = compile_and_run_with_flags(
        "elephc_memory_limit_unsigned_suffix",
        source,
        &[
            "--heap-size=134217728",
            "--ini",
            "memory_limit=9223372036854784000K",
        ],
    );
    assert!(!out.success, "the 8 MiB ceiling did not fire");
    assert_eq!(
        out.stdout, "9223372036854784000K\n",
        "stderr: {}",
        out.stderr
    );
    assert!(
        out.stderr
            .contains("Fatal error: Allowed memory size of 8388608 bytes exhausted"),
        "{}",
        out.stderr
    );
}

/// A hexadecimal startup quantity is parsed like PHP's INI parser and sizes the live ceiling:
/// `0x800000` is 8 MiB, so a 3 MiB string succeeds while `ini_get()` keeps the raw spelling.
#[test]
fn test_startup_memory_limit_accepts_php_quantity_bases() {
    let source = r#"<?php
echo ini_get('memory_limit'), "\n";
echo strlen(str_repeat("a", 3 * 1024 * 1024)), "\n";
"#;
    let out = compile_and_run_with_flags(
        "elephc_memory_limit_hex",
        source,
        &["--ini", "memory_limit=0x800000"],
    );
    assert!(out.success, "program failed: {}", out.stderr);
    assert_eq!(
        out.stdout,
        "0x800000\n3145728\n",
        "stderr: {}",
        out.stderr
    );
}

/// A lowered limit must not reject a bump allocation merely because the historical bump
/// position is above it: the live budget and reserved arena still have room.
#[test]
fn test_lowered_memory_limit_uses_bump_space_when_live_budget_allows() {
    let source = r#"<?php
$a = array_fill(0, 600000, 0);
$keep = array_fill(0, 1000, 0);
unset($a);
ini_set('memory_limit', '8M');
$b = array_fill(0, 700000, 0);
echo count($b), "\n";
"#;
    let out = compile_and_run_with_flags(
        "elephc_memory_limit_bump_space",
        source,
        &["--heap-size=67108864", "--ini", "memory_limit=64M"],
    );
    assert!(out.success, "program failed: {}", out.stderr);
    assert_eq!(out.stdout, "700000\n", "stderr: {}", out.stderr);
}

/// Two scratch-backed strings that each fit the ceiling must still fail once their combined
/// live scratch footprint exceeds the runtime `memory_limit`.
#[test]
fn test_scratch_strings_share_the_live_memory_budget() {
    let source = r#"<?php
echo strcmp(str_repeat("a", 1500000), str_repeat("b", 1500000)), "\n";
"#;
    let out = compile_and_run_with_flags(
        "elephc_memory_limit_scratch_combined",
        source,
        &["--ini", "memory_limit=2M"],
    );
    assert!(!out.success, "the combined scratch result did not hit the limit");
    assert!(out.stdout.is_empty(), "stdout: {}", out.stdout);
    assert_eq!(out.status.code(), Some(255));
    assert!(
        out.stderr.contains(
            "Fatal error: Allowed memory size of 2097152 bytes exhausted (tried to allocate "
        ),
        "{}",
        out.stderr
    );
}

/// A scratch-backed result also honors the runtime `memory_limit`: printing a 3 MiB
/// `str_repeat()` under a 2 MiB ceiling reports the fatal before writing any output.
#[test]
fn test_scratch_string_above_memory_limit_uses_allowed_memory_fatal() {
    let source = r#"<?php
echo str_repeat("a", 3 * 1024 * 1024), "\n";
"#;
    let out = compile_and_run_with_flags(
        "elephc_memory_limit_scratch_string",
        source,
        &["--ini", "memory_limit=2M"],
    );
    assert!(!out.success, "the scratch string did not hit the limit");
    assert!(out.stdout.is_empty(), "stdout: {}", out.stdout);
    assert_eq!(out.status.code(), Some(255));
    assert!(
        out.stderr.contains(
            "Fatal error: Allowed memory size of 2097152 bytes exhausted (tried to allocate "
        ),
        "{}",
        out.stderr
    );
}

/// Concatenation above the runtime `memory_limit` takes the same Allowed-memory fatal path as
/// other heap allocations, not the reserved-arena overflow trampoline.
#[test]
fn test_concat_above_memory_limit_uses_allowed_memory_fatal() {
    let source = r#"<?php
$a = str_repeat("a", 1024 * 1024);
$b = $a . $a;
echo strlen($b), "\n";
"#;
    let out = compile_and_run_with_flags(
        "elephc_memory_limit_concat",
        source,
        &["--ini", "memory_limit=2M"],
    );
    assert!(!out.success, "the concatenation did not hit the limit");
    assert!(out.stdout.is_empty(), "stdout: {}", out.stdout);
    assert_eq!(out.status.code(), Some(255));
    assert!(
        out.stderr.contains(
            "Fatal error: Allowed memory size of 2097152 bytes exhausted (tried to allocate 2097152 bytes)"
        ),
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

/// A finite request above the reserved arena is refused instead of being silently clamped, so
/// `ini_get()` never reports a ceiling the allocator will not honor. Setting the arena capacity
/// exactly is still accepted.
#[test]
fn test_ini_set_memory_limit_above_arena_is_refused() {
    let source = r#"<?php
echo ini_get('memory_limit'), "\n";
var_dump(ini_set('memory_limit', '128M'));
echo ini_get('memory_limit'), "\n";
var_dump(ini_set('memory_limit', '64M'));
echo ini_get('memory_limit'), "\n";
"#;
    let out = compile_and_run_with_flags(
        "elephc_memory_limit_above_arena",
        source,
        &["--heap-size=67108864", "--ini", "memory_limit=32M"],
    );
    assert!(out.success, "program failed: {}", out.stderr);
    assert_eq!(
        out.stdout,
        "32M\nbool(false)\n32M\nstring(3) \"32M\"\n64M\n",
        "stderr: {}",
        out.stderr
    );
    assert!(
        out.stderr.contains(
            "Warning: Failed to set memory limit to 134217728 bytes (the runtime heap arena \
             cannot grow; rebuild with a larger --heap-size)"
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

/// Both target paths for the runtime setter are emitted: ARM64 compares against `_heap_max`
/// with `cmp x0, x9`, x86_64 with `cmp rax, r10`, and each publishes an accepted request to
/// `_heap_limit`. Cross-target `--emit-asm` proves the non-host backend without executing it.
#[test]
fn test_ini_set_memory_limit_lowering_covers_both_architectures() {
    for (target, capacity_compare) in [
        ("macos-aarch64", "cmp x0, x9"),
        ("linux-x86_64", "cmp rax, r10"),
    ] {
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
            asm.contains(capacity_compare),
            "{target}: the request was not compared against the reserved arena"
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

/// A freed block cannot be reused above a newly lowered `memory_limit`: the allocator must
/// check the live budget on the free-list/small-bin paths, not only when extending the heap.
#[test]
fn test_lowered_memory_limit_applies_to_reused_free_blocks() {
    let source = r#"<?php
$a = array_fill(0, 600000, 0);
$keep = array_fill(0, 1000, 0);
unset($a);
ini_set('memory_limit', '4M');
$b = array_fill(0, 600000, 0);
echo "not reached\n";
"#;
    let out = compile_and_run_with_flags(
        "elephc_memory_limit_reused_block",
        source,
        &["--heap-size=67108864", "--ini", "memory_limit=64M"],
    );
    assert!(!out.success, "the reused block bypassed the lowered limit");
    assert!(out.stdout.is_empty(), "stdout: {}", out.stdout);
    assert_eq!(out.status.code(), Some(255));
    assert!(
        out.stderr
            .contains("Allowed memory size of 4194304 bytes exhausted"),
        "{}",
        out.stderr
    );
}

/// PHP refuses a startup `memory_limit` below its 2 MiB startup allocation and keeps the
/// previous value, so `--ini memory_limit=1M` must report and enforce the default 128M.
#[test]
fn test_startup_memory_limit_below_the_two_mib_floor_is_refused() {
    let source = r#"<?php
var_dump(ini_get('memory_limit'));
$a = array_fill(0, 300000, 0);
echo count($a), "\n";
"#;
    let out = compile_and_run_with_flags(
        "elephc_memory_limit_startup_floor",
        source,
        &["--ini", "memory_limit=1M"],
    );
    assert!(out.success, "program failed: {}", out.stderr);
    assert_eq!(out.stdout, "string(4) \"128M\"\n300000\n", "stderr: {}", out.stderr);
}

/// The compiler reports PHP's startup refusal warning while dropping the refused override.
#[test]
fn test_startup_memory_limit_floor_warning_is_reported() {
    let stderr = compile_cli_file_with_flags_capturing_stderr(
        "<?php echo ini_get('memory_limit');",
        &["--ini", "memory_limit=1M"],
    );
    assert!(
        stderr.contains(
            "Warning: Failed to set memory limit to 1048576 bytes (Current memory usage is 2097152 bytes)"
        ),
        "unexpected compiler stderr: {stderr}"
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
        "bool(false)\n128M\n\
string(4) \"128M\"\n9223372036854775808\n\
string(19) \"9223372036854775808\"\n18446744073709551615\n\
string(20) \"18446744073709551615\"\n18446744073709551616\n\
string(20) \"18446744073709551616\"\n-9223372036854775808\n\
bool(false)\n-9223372036854775808\n",
        "stderr: {}",
        out.stderr
    );
    assert!(
        out.stderr.contains(
            "Warning: Failed to set memory limit to 9223372036854775807 bytes (the runtime heap \
             arena cannot grow; rebuild with a larger --heap-size)"
        ),
        "{}",
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
