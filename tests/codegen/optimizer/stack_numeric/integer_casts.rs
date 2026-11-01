//! Purpose:
//! Verifies direct integer payload extraction from proven numeric frame values.
//!
//! Called from:
//! - The focused optimizer codegen tests through `stack_numeric`.
//!
//! Key details:
//! - Covers float continuation, exact large integers, aliases and all supported target emitters.

use super::*;

/// Keeps the recurrence boxed while an explicit integer cast observes each numeric epoch.
const COUNTER_CAST: &str = r#"<?php
#[Export]
function frameCounterCast(int $n, int $step): int {
    $value = 3;
    for ($i = 0; $i < $n; $i++) { $value += $step; }
    return (int)$value;
}
#[Export]
function unknownCounterCast($value): int { return (int)$value; }
#[Export]
function referenceCounterCast(int $step): int {
    $value = 3; $alias =& $value; $alias += $step;
    return (int)$value;
}
"#;

/// Extracts one source function's annotated assembly without architecture-specific comment syntax.
fn body<'a>(assembly: &'a str, name: &str) -> &'a str {
    assembly.split(&format!("@fn name={name} symbol=")).nth(1).expect("function marker")
        .split("@fn name=").next().unwrap()
}

/// Every target extracts the integer payload and retains a cold float conversion helper.
#[test]
fn test_stack_numeric_integer_cast_emits_all_targets() {
    for target in ["macos-aarch64", "ios-arm64", "ios-sim-arm64", "linux-aarch64", "linux-x86_64"] {
        let assembly = super::super::checked_numeric_chain::emit_target_assembly(COUNTER_CAST, target);
        let numeric = body(&assembly, "frameCounterCast");
        assert!(numeric.contains("frame_numeric_int_cast_slow"), "{target}");
        assert!(numeric.contains("__rt_mixed_cast_int"), "float fallback: {target}");
        assert!(!body(&assembly, "unknownCounterCast").contains("frame_numeric_int_cast"), "{target}");
        assert!(!body(&assembly, "referenceCounterCast").contains("frame_numeric_int_cast"), "{target}");
    }
}

/// Integer payloads retain full precision and overflow floats retain PHP's conversion rules.
#[test]
fn test_stack_numeric_integer_cast_preserves_overflow_and_float_tags() {
    let source = r#"<?php
function castNumericSteps(int $seed, int $step, int $count): void {
    $value = $seed;
    for ($i = 0; $i < $count; $i++) {
        $value += $step;
        echo @(int)$value, ',';
    }
    echo '|';
}
function castNumericProduct(int $left, int $right): void {
    echo @(int)($left * $right), '|';
    echo @(int)(($left * $right) * NAN), '|';
    echo @(int)(($left * $right) * INF), '|';
}
castNumericSteps(9007199254740991, 1, $argc + 1);
castNumericSteps(PHP_INT_MAX - 1, 1, $argc + 1);
castNumericSteps(PHP_INT_MIN + 1, -1, $argc + 1);
castNumericProduct(PHP_INT_MIN, -$argc);
"#;
    for optimized in [false, true] {
        let output = run(source, optimized).0;
        assert_eq!(output, "9007199254740992,9007199254740993,|9223372036854775807,-9223372036854775808,|-9223372036854775808,-9223372036854775808,|-9223372036854775808|0|0|");
    }
}

/// Unknown tags and reference-backed locals retain ordinary PHP casts and alias updates.
#[test]
fn test_stack_numeric_integer_cast_keeps_unknown_and_reference_fallbacks() {
    let functions = COUNTER_CAST.replace("#[Export]\n", "");
    let source = format!("{functions}\necho frameCounterCast($argc + 1, 2), '|';\n\
        echo unknownCounterCast('12abc'), '|', unknownCounterCast(2.75), '|';\n\
        echo unknownCounterCast(false), '|', unknownCounterCast(null), '|';\n\
        echo referenceCounterCast($argc + 1), '|';\n");
    let source = format!("{source}{}", r#"
function castAfterUnset(int $n): int {
    $value = 0;
    for ($i = 0; $i < $n; $i++) { $value += 2; }
    if ($n > 0) { unset($value); }
    return @(int)$value;
}
echo castAfterUnset($argc), '|';
set_error_handler(function(int $severity, string $message, string $file, int $line): bool {
    echo $severity === E_WARNING && $line > 0 && $message !== '' && $file !== '' ? 'warn|' : 'wrong|';
    return true;
});
function warnStringOffset(int $count): void {
    $value = 0;
    for ($i = 0; $i < $count; $i++) { $value += 0.5; }
    echo 'abc'[$value], '|';
}
warnStringOffset($argc);
restore_error_handler();
"#);
    for optimized in [false, true] {
        assert_eq!(run(&source, optimized).0, "7|12|2|0|0|5|0|warn|a|");
    }
}
