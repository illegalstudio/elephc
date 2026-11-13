//! Purpose:
//! Regressions for numeric reference operands replaced by comparison warning handlers.
//!
//! Called from:
//! - The codegen integration harness and the real Elephc CLI.
//!
//! Key details:
//! - Check both replacement values and replacement types, with PHP and heap-debug controls.
//! - The CLI path exercises linker garbage collection of runtime helper sections.

use super::*;

const RETAGGED_NUMERIC_REFS: &str = r#"<?php
function retagNumericRef(mixed $start, mixed $next, bool $right, bool $switch): void {
    $n = $start;
    $o = new stdClass();
    $warnings = 0;
    set_error_handler(function(int $level, string $message) use (&$n, $next, &$warnings): bool {
        $warnings++;
        $n = $next;
        return true;
    });
    if ($switch) {
        switch ($n) {
            case $o: $result = true; break;
            case 2: $result = false; break;
            default: $result = false;
        }
    } else {
        $result = $right ? $o == $n : $n == $o;
    }
    restore_error_handler();
    echo $result ? "true:" : "false:", $warnings, "|";
}
foreach ([false, true] as $right) {
    retagNumericRef(2, "1foo", $right, false);
    retagNumericRef(2, "1", $right, false);
    retagNumericRef(2, "foo", $right, false);
    retagNumericRef(2, false, $right, false);
    retagNumericRef(2, true, $right, false);
    retagNumericRef(2, null, $right, false);
    retagNumericRef(2, [], $right, false);
    retagNumericRef(2, [1], $right, false);
    retagNumericRef(2, 1.0, $right, false);
    retagNumericRef(2, 2.0, $right, false);
    retagNumericRef(2, new stdClass(), $right, false);
    retagNumericRef(2, function(): int { return 1; }, $right, false);
}
retagNumericRef(2, new stdClass(), false, true);
retagNumericRef(2, function(): int { return 1; }, false, true);
function throwAfterNumericRetag(mixed $start): void {
    $n = $start;
    $next = new stdClass();
    $warnings = 0;
    set_error_handler(function(int $level, string $message) use (&$n, $next, &$warnings): bool {
        $warnings++;
        if ($warnings === 2) { throw new RuntimeException("second conversion"); }
        $n = $next;
        return true;
    });
    try { $result = new stdClass() == $n; echo "missed|"; }
    catch (RuntimeException $e) { echo "caught:", $warnings, "|"; }
    restore_error_handler();
}
throwAfterNumericRetag(2);
function directClosureNumeric(mixed $closure): void {
    set_error_handler(function(int $level, string $message): bool { echo $message, "|"; return true; });
    var_dump($closure == 1, 1.0 == $closure);
    restore_error_handler();
    var_dump($closure == $closure);
}
directClosureNumeric(function(): int { return 1; });
"#;

/// Gives the PHP result and warning count for each post-warning runtime tag.
fn retagged_numeric_expected() -> String {
    let cases = concat!(
        "false:1|true:1|false:1|false:1|true:1|false:1|",
        "false:1|false:1|true:1|false:1|true:2|true:2|",
    );
    format!(
        "{}true:2|true:2|caught:2|{}{}bool(true)\nbool(true)\nbool(true)\n",
        cases.repeat(2),
        "Object of class Closure could not be converted to int|",
        "Object of class Closure could not be converted to float|",
    )
}

/// Retagging a number must reapply PHP equality, including a second object warning and throw.
#[test]
fn test_loose_comparison_batch_retyped_numeric_ref_cells() {
    let output = compile_and_run_with_heap_debug(RETAGGED_NUMERIC_REFS);
    assert!(output.success, "{}: {}", output.status, output.stderr);
    assert!(output.stdout.len() < 4096, "unexpected stdout length {}", output.stdout.len());
    assert_eq!(output.stdout, retagged_numeric_expected());
    assert!(output.stderr.contains("HEAP DEBUG: leak summary: clean"), "{}", output.stderr);
}

/// The real CLI link must retain the shared runtime body through section garbage collection.
#[test]
fn test_loose_comparison_batch_ref_cells_cli_link() {
    let dir = make_cli_test_dir("elephc_loose_comparison_ref_cells_cli");
    let result = std::panic::catch_unwind(|| {
        fs::write(dir.join("main.php"), RETAGGED_NUMERIC_REFS).unwrap();
        let compiled = elephc_cli_command(&dir)
            .args(["--quiet", "--heap-debug", "main.php"])
            .output()
            .unwrap();
        assert!(compiled.status.success(), "{:.2000}", String::from_utf8_lossy(&compiled.stderr));
        let output = run_binary(&dir.join("main"), &dir);
        assert!(output.status.success(), "{:.2000}", String::from_utf8_lossy(&output.stderr));
        assert!(output.stdout.len() < 4096, "unexpected stdout length {}", output.stdout.len());
        assert_eq!(String::from_utf8(output.stdout).unwrap(), retagged_numeric_expected());
        assert!(String::from_utf8_lossy(&output.stderr).contains("HEAP DEBUG: leak summary: clean"));
    });
    fs::remove_dir_all(&dir).unwrap();
    result.unwrap();
}

/// Numeric reference cells remain live when warning handlers replace their boxed values.
#[test]
fn test_loose_comparison_batch_replaced_numeric_ref_cells() {
    let output = compile_and_run_with_heap_debug(r#"<?php
function compareNumericRef(mixed $start, mixed $next, bool $right, bool $write): void {
    $n = $start;
    $o = new stdClass();
    set_error_handler(function(int $level, string $message) use (&$n, $next, $write): bool {
        if ($write) { $n = $next; }
        return true;
    });
    var_dump($right ? $o == $n : $n == $o);
    var_dump($n == $o);
    var_dump($n);
    restore_error_handler();
}
foreach ([false, true] as $right) {
    compareNumericRef(1, 1, $right, true);
    compareNumericRef(2, 1, $right, true);
    compareNumericRef(1, 2, $right, true);
    compareNumericRef(1, 2, $right, false);
    compareNumericRef(2.0, 1.0, $right, true);
}
function switchNumericRef(): void {
    $n = 2;
    set_error_handler(function(int $level, string $message) use (&$n): bool { $n = 1; return true; });
    switch ($n) {
        case new stdClass(): echo "object\n"; break;
        case 2: echo "two\n"; break;
        default: echo "other\n";
    }
    restore_error_handler();
    var_dump($n);
}
switchNumericRef();
"#);
    assert!(output.success, "{}: {} {}", output.status, output.stdout, output.stderr);
    let cases = concat!(
        "bool(true)\nbool(true)\nint(1)\n",
        "bool(true)\nbool(true)\nint(1)\n",
        "bool(false)\nbool(false)\nint(2)\n",
        "bool(true)\nbool(true)\nint(1)\n",
        "bool(true)\nbool(true)\nfloat(1)\n",
    );
    assert_eq!(output.stdout, format!("{}object\nint(1)\n", cases.repeat(2)));
    assert!(output.stderr.contains("HEAP DEBUG: leak summary: clean"), "{}", output.stderr);
}
