//! Purpose:
//! Executes loop counters whose name a later loop reuses, and checks that each loop keeps the
//! storage its own updates need instead of sharing one boxed frame slot.
//!
//! Called from:
//! - The focused optimizer codegen integration suite.
//!
//! Key details:
//! - Runtime cases run with IR optimization on and off, strict heap validation and the
//!   optional real PHP oracle; the overflow cases cross PHP_INT_MAX and PHP_INT_MIN in either
//!   binding, so a scalar binding never hides PHP's float continuation in the other one.
//! - EIR assertions count the boxed counter opcodes (`str_inc_dec`, `php_rel_cmp`) of the
//!   reused-counter micro-benchmark shapes, including a property-bounded method loop.

use super::*;

/// Functions and a method whose counters are reused by a second loop with a different seed.
const REUSED_COUNTERS: &str = r#"<?php
function reusedCounter(int $n): int {
    $sum = 0;
    for ($i = 0; $i < $n; $i++) { $sum += 1; }
    for ($i = $n - 1; $i >= 0; $i--) { $sum += 1; }
    return $sum;
}
function reversedCounter(int $n): int {
    $sum = 0;
    for ($i = $n - 1; $i >= 0; $i--) { $sum += $i; }
    for ($i = 0; $i < $n; $i++) { $sum += $i; }
    return $sum;
}
final class Boxes {
    private int $count = 0;
    public function __construct(int $count) { $this->count = $count; }
    public function sweep(): int {
        $total = 0;
        for ($box = 0; $box < $this->count; $box++) { $total += $box; }
        for ($box = $this->count - 1; $box >= 0; $box--) { $total -= $box; }
        return $total + $box;
    }
}
echo reusedCounter($argc + 9), '|', reversedCounter($argc + 9), '|',
    (new Boxes($argc + 4))->sweep();
"#;

/// Runs one IR mode with strict heap validation and the installed PHP oracle when requested.
fn run(source: &str, optimized: bool) -> String {
    let dir = make_cli_test_dir("elephc_reused_counter");
    let path = dir.join("main.php");
    fs::write(&path, source).unwrap();
    let mut command = elephc_cli_command(&dir);
    command.args(["--heap-debug", "--gc-stats"]);
    if !optimized {
        command.arg("--no-ir-opt");
    }
    let compiled = command.arg(&path).output().unwrap();
    assert!(compiled.status.success(), "{}", String::from_utf8_lossy(&compiled.stderr));
    let output = run_binary(&path.with_extension(""), &dir);
    let stderr = String::from_utf8_lossy(&output.stderr).to_string();
    assert!(output.status.success(), "{stderr}");
    assert!(stderr.contains("HEAP DEBUG: leak summary: clean"), "{stderr}");
    if std::env::var_os("ELEPHC_PHP_CHECK").is_some() {
        let php = Command::new("php").arg("-n").arg(&path).output().unwrap();
        assert!(php.status.success(), "{}", String::from_utf8_lossy(&php.stderr));
        assert_eq!(output.stdout, php.stdout, "actual PHP oracle");
    }
    let text = String::from_utf8(output.stdout).unwrap();
    fs::remove_dir_all(dir).unwrap();
    text
}

/// Emits the optimized EIR module text of `source`.
fn optimized_ir(source: &str) -> String {
    let dir = make_cli_test_dir("elephc_reused_counter_ir");
    let path = dir.join("main.php");
    fs::write(&path, source).unwrap();
    let output = elephc_cli_command(&dir).arg("--emit-ir").arg(&path).output().unwrap();
    assert!(output.status.success(), "{}", String::from_utf8_lossy(&output.stderr));
    fs::remove_dir_all(dir).unwrap();
    String::from_utf8(output.stdout).unwrap()
}

/// Returns the printed EIR of one function or method of a module dump.
fn function_body<'a>(ir: &'a str, name: &str) -> &'a str {
    ir.split(&format!("function {name}("))
        .nth(1)
        .unwrap_or_else(|| panic!("missing function {name} in:\n{ir}"))
        .split("\n  function ")
        .next()
        .unwrap()
}

/// Returns the instruction lines of `body` that apply `op`.
fn op_lines<'a>(body: &'a str, op: &str) -> Vec<&'a str> {
    let pattern = format!("= {op} ");
    body.lines().filter(|line| line.contains(&pattern)).collect()
}

/// Asserts that the ascending `<` loop is integer and only the descending loop stays boxed.
fn assert_only_descending_loop_boxed(ir: &str, name: &str) {
    let body = function_body(ir, name);
    let increments = op_lines(body, "str_inc_dec");
    assert_eq!(
        increments.len(),
        1,
        "{name}: only the descending loop keeps a boxed update:\n{body}"
    );
    assert!(increments[0].contains(" -1 ;"), "{name}: the boxed update is the decrement:\n{body}");
    let comparisons = op_lines(body, "php_rel_cmp");
    assert_eq!(
        comparisons.len(),
        1,
        "{name}: only the descending loop keeps a boxed test:\n{body}"
    );
    assert!(comparisons[0].contains(" Sge ;"), "{name}: the boxed test is `>= 0`:\n{body}");
    assert!(
        op_lines(body, "icmp").iter().any(|line| line.contains(" Slt ;")),
        "{name}: the ascending loop compares a raw integer counter:\n{body}"
    );
}

/// A counter reused by a later loop keeps its values in both IR modes, like PHP.
#[test]
fn test_reused_loop_counters_match_php() {
    for optimized in [false, true] {
        assert_eq!(run(REUSED_COUNTERS, optimized), "20|90|-1");
    }
}

/// The ascending loop of every reuse shape lowers to an integer recurrence.
///
/// Before each assignment started its own binding, the checked `$n - 1` seed of the descending
/// loop widened the one frame slot both loops shared, and the ascending loop compiled to
/// `php_rel_cmp` and a boxed `str_inc_dec` as well.
#[test]
fn test_reused_loop_counters_keep_the_ascending_loop_integer() {
    let ir = optimized_ir(REUSED_COUNTERS);
    for name in ["reusedCounter", "reversedCounter", "Boxes::sweep"] {
        assert_only_descending_loop_boxed(&ir, name);
    }
}

/// Either binding of a reused counter still promotes to float exactly where PHP does.
///
/// The second binding starts beyond the integer range, climbs past PHP_INT_MAX or falls past
/// PHP_INT_MIN, and the first binding climbs past PHP_INT_MAX before the second one restarts
/// from zero; every read after the overflow must observe the float.
#[test]
fn test_reused_loop_counters_keep_overflow_promotion_per_binding() {
    let source = r#"<?php
function overflowingStart(int $n): string {
    for ($i = 0; $i < 3; $i++) {}
    $first = $i;
    for ($i = $n - 1; $i >= 0; $i--) {}
    return gettype($first) . $first . ':' . gettype($i) . ':' . @(int)$i;
}
function climbingSecond(int $top): string {
    $seen = '';
    for ($i = 0; $i < 2; $i++) { $seen .= $i; }
    $guard = 0;
    for ($i = $top - 1; ; $i++) {
        $seen .= '|' . gettype($i);
        if (++$guard === 4) { break; }
    }
    return $seen . '|' . @(int)$i;
}
function fallingSecond(int $bottom): string {
    $seen = '';
    for ($i = 0; $i < 2; $i++) { $seen .= $i; }
    $guard = 0;
    for ($i = $bottom + 1; ; $i--) {
        $seen .= '|' . gettype($i);
        if (++$guard === 4) { break; }
    }
    return $seen . '|' . @(int)$i;
}
function climbingFirst(int $top): string {
    $seen = '';
    $guard = 0;
    for ($i = $top - 1; ; $i++) {
        $seen .= gettype($i) . '|';
        if (++$guard === 3) { break; }
    }
    for ($i = 0; $i < 2; $i++) { $seen .= $i; }
    return $seen;
}
echo overflowingStart(PHP_INT_MIN + $argc - 1), "\n";
echo climbingSecond(PHP_INT_MAX - 1 + $argc), "\n";
echo fallingSecond(PHP_INT_MIN + $argc - 1), "\n";
echo climbingFirst(PHP_INT_MAX - 1 + $argc), "\n";
"#;
    let expected = "integer3:double:-9223372036854775808\n\
        01|integer|integer|double|double|-9223372036854775808\n\
        01|integer|integer|double|double|-9223372036854775808\n\
        integer|integer|double|01\n";
    for optimized in [false, true] {
        assert_eq!(run(source, optimized), expected);
    }
}

/// A `for` initializer binds its counter unconditionally, so the counter can be retyped or
/// unset after the loop exactly as when the same assignment is written before the loop.
#[test]
fn test_for_initializer_binding_retypes_and_unsets_after_the_loop() {
    let source = r#"<?php
function retyped(int $n): string {
    for ($i = 0; $i < $n; $i++) {}
    $i = "done:" . $n;
    return $i;
}
function killed(int $n): string {
    for ($i = 0; $i < $n; $i++) {}
    unset($i);
    return isset($i) ? 'set' : 'unset';
}
echo retyped($argc + 2), '|', killed($argc + 2);
"#;
    for optimized in [false, true] {
        assert_eq!(run(source, optimized), "done:3|unset");
    }
}

/// A re-bound variable keeps the position of its first binding in `get_defined_vars()`, and a
/// reassigned parameter keeps reporting its current value as a backtrace argument, like PHP.
///
/// A re-bind gives the name a newer frame slot, so ordering by the current slot listed `$a`
/// after `$b`, and splitting a parameter would leave backtraces reading its first slot.
#[test]
fn test_rebinding_keeps_variable_order_and_backtrace_arguments() {
    let source = r#"<?php
function order(int $n): string {
    $a = 0;
    $b = 1;
    $a = $n - 1;
    return implode(',', array_keys(get_defined_vars()));
}
function retypedOrder(int $n): string {
    $a = 0;
    $b = 1;
    $a = "s" . $n;
    return implode(',', array_keys(get_defined_vars()));
}
function killedOrder(int $n): string {
    $a = 0;
    $b = 1;
    unset($a);
    $a = $n - 1;
    return implode(',', array_keys(get_defined_vars()));
}
function traced(int $n): string {
    $n = $n + 1;
    return (string) debug_backtrace()[0]['args'][0];
}
echo order($argc), '|', retypedOrder($argc), '|', killedOrder($argc), '|', traced($argc);
"#;
    for optimized in [false, true] {
        assert_eq!(run(source, optimized), "n,a,b|n,a,b|n,a,b|2");
    }
}

/// Every supported target emits the reused-counter shapes in both IR modes.
#[test]
fn test_reused_loop_counters_emit_on_all_supported_targets() {
    for target in [
        "macos-aarch64",
        "ios-arm64",
        "ios-sim-arm64",
        "linux-aarch64",
        "linux-x86_64",
    ] {
        for optimized in [false, true] {
            let dir = make_cli_test_dir("elephc_reused_counter_target");
            let path = dir.join("main.php");
            fs::write(&path, REUSED_COUNTERS).unwrap();
            let mut command = elephc_cli_command(&dir);
            command.args(["--emit-asm", "--target", target]);
            if target.starts_with("ios-") {
                command.args(["--emit", "staticlib"]);
            }
            if !optimized {
                command.arg("--no-ir-opt");
            }
            let output = command.arg(&path).output().unwrap();
            assert!(
                output.status.success(),
                "{target}: {}",
                String::from_utf8_lossy(&output.stderr)
            );
            assert!(!fs::read_to_string(path.with_extension("s")).unwrap().is_empty());
            fs::remove_dir_all(dir).unwrap();
        }
    }
}
