//! Purpose:
//! Regressions for callable source forms in numeric loose comparisons.
//!
//! Called from:
//! - The codegen integration harness and the real Elephc CLI.
//!
//! Key details:
//! - Implicit string/array callables stay quiet; actual Closures convert with warnings.
//! - Forwarding and invocation retain the same descriptor ABI and receiver ownership.

use super::*;

const CALLABLE_NUMERIC_FORMS: &str = r#"<?php
function callableLength(string $text): int { return strlen($text); }
class CallableLengthTarget {
    public static function size(string $text): int { return strlen($text); }
    public function length(string $text): int { return strlen($text); }
}
function forwardNumericCallable(callable $callback): callable { return $callback; }
function boxedCallableNumber(mixed $callback): void {
    var_dump($callback == 1, 1.0 == $callback, $callback != 1.0);
    switch ($callback) { case 1: echo "hit|"; break; default: echo "miss|"; }
}
function inspectNumericCallable(callable $callback): void {
    $warnings = 0;
    set_error_handler(function(int $level, string $message) use (&$warnings): bool {
        $warnings++;
        echo $message, "|";
        return true;
    });
    var_dump($callback == 1, 1.0 == $callback, $callback != 1.0);
    boxedCallableNumber($callback);
    $forwarded = forwardNumericCallable($callback);
    echo $forwarded("abcd"), ":", $warnings, "\n";
    restore_error_handler();
}
inspectNumericCallable("strlen");
inspectNumericCallable(callback: "strlen");
inspectNumericCallable("callableLength");
inspectNumericCallable("CallableLengthTarget::size");
inspectNumericCallable([CallableLengthTarget::class, "size"]);
$receiver = new CallableLengthTarget();
inspectNumericCallable([$receiver, "length"]);
$tracked = [$receiver, "length"];
inspectNumericCallable($tracked);
inspectNumericCallable(strlen(...));
inspectNumericCallable(Closure::fromCallable("strlen"));
inspectNumericCallable(function(string $text): int { return strlen($text); });
inspectNumericCallable(CallableLengthTarget::size(...));
inspectNumericCallable($receiver->length(...));
inspectNumericCallable(...["callback" => strlen(...)]);
inspectNumericCallable(...[strlen(...)]);
"#;

/// Gives the PHP output, including warning order and both concrete numeric kinds.
fn callable_numeric_expected() -> String {
    let scalar = "bool(false)\nbool(false)\nbool(true)\n";
    let integer = "Object of class Closure could not be converted to int|";
    let float = "Object of class Closure could not be converted to float|";
    let object = format!("{integer}{float}{float}bool(true)\nbool(true)\nbool(false)\n");
    format!("{}{}", format!("{scalar}{scalar}miss|4:0\n").repeat(7),
        format!("{object}{object}{integer}hit|4:7\n").repeat(7))
}

/// Only actual Closure objects warn and equal numeric one after callable parameter binding.
#[test]
fn test_loose_comparison_batch_callable_value_forms() {
    let output = compile_and_run_with_heap_debug(CALLABLE_NUMERIC_FORMS);
    assert!(output.success, "{}: {:.2000}", output.status, output.stderr);
    assert!(output.stdout.len() < 8192, "unexpected stdout length {}", output.stdout.len());
    assert_eq!(output.stdout, callable_numeric_expected());
    assert!(output.stderr.contains("HEAP DEBUG: leak summary: clean"), "{:.2000}", output.stderr);
}

/// Production section collection preserves callable invocation, captures, and handle checks.
#[test]
fn test_loose_comparison_batch_callable_value_cli_link() {
    let dir = make_cli_test_dir("elephc_loose_comparison_callable_values_cli");
    let result = std::panic::catch_unwind(|| {
        fs::write(dir.join("main.php"), CALLABLE_NUMERIC_FORMS).unwrap();
        let compiled = elephc_cli_command(&dir)
            .args(["--quiet", "--heap-debug", "main.php"]).output().unwrap();
        assert!(compiled.status.success(), "{:.2000}", String::from_utf8_lossy(&compiled.stderr));
        let output = run_binary(&dir.join("main"), &dir);
        assert!(output.status.success(), "{:.2000}", String::from_utf8_lossy(&output.stderr));
        assert!(output.stdout.len() < 8192, "unexpected stdout length {}", output.stdout.len());
        assert_eq!(String::from_utf8(output.stdout).unwrap(), callable_numeric_expected());
        assert!(String::from_utf8_lossy(&output.stderr).contains("HEAP DEBUG: leak summary: clean"));
    });
    fs::remove_dir_all(&dir).unwrap();
    result.unwrap();
}
