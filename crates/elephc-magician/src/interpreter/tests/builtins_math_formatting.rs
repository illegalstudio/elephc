//! Purpose:
//! Interpreter tests for math and formatting builtins.
//!
//! Called from:
//! - `cargo test -p elephc-magician` through Rust's test harness.
//!
//! Key details:
//! - These cases cover numeric hooks, printf-family formatting, min/max, clamp, and constants.

use super::super::*;
use super::support::*;

/// Verifies eval `abs()` dispatches through runtime numeric hooks directly and by callable.
#[test]
fn execute_program_dispatches_abs_builtin() {
    let program = parse_fragment(
        br#"echo abs(-5); echo ":";
echo abs(-2.5); echo ":";
echo gettype(abs(-2.5)); echo ":";
echo call_user_func("abs", -7); echo ":";
echo call_user_func_array("abs", [-9]);
return function_exists("abs");"#,
    )
    .expect("parse eval fragment");
    let mut scope = ElephcEvalScope::new();
    let mut values = FakeOps::default();

    let result = execute_program(&program, &mut scope, &mut values).expect("execute eval ir");

    assert_eq!(values.output, "5:2.5:double:7:9");
    assert_eq!(values.get(result), FakeValue::Bool(true));
}
/// Verifies eval `floor()` and `ceil()` dispatch as double-returning math builtins.
#[test]
fn execute_program_dispatches_floor_and_ceil_builtins() {
    let program = parse_fragment(
        br#"echo floor(3.7); echo ":";
echo gettype(floor(3)); echo ":";
echo ceil(3.2); echo ":";
echo gettype(ceil(3)); echo ":";
echo call_user_func("floor", 4.9); echo ":";
echo call_user_func_array("ceil", [4.1]);
echo ":"; echo function_exists("floor");
return function_exists("ceil");"#,
    )
    .expect("parse eval fragment");
    let mut scope = ElephcEvalScope::new();
    let mut values = FakeOps::default();

    let result = execute_program(&program, &mut scope, &mut values).expect("execute eval ir");

    assert_eq!(values.output, "3:double:4:double:4:5:1");
    assert_eq!(values.get(result), FakeValue::Bool(true));
}
/// Verifies eval `fdiv()` and `fmod()` dispatch as floating-point binary builtins.
#[test]
fn execute_program_dispatches_float_binary_builtins() {
    let program = parse_fragment(
        br#"echo round(fdiv(10, 4), 2); echo ":";
echo gettype(fdiv(10, 4)); echo ":";
echo round(fmod(10.5, 3.2), 1); echo ":";
echo round(call_user_func("fdiv", 9, 2), 1); echo ":";
echo round(call_user_func_array("fmod", [10.5, 3.2]), 1); echo ":";
echo function_exists("fdiv");
return function_exists("fmod");"#,
    )
    .expect("parse eval fragment");
    let mut scope = ElephcEvalScope::new();
    let mut values = FakeOps::default();
    let result = execute_program(&program, &mut scope, &mut values).expect("execute eval ir");
    assert_eq!(values.output, "2.5:double:0.9:4.5:0.9:1");
    assert_eq!(values.get(result), FakeValue::Bool(true));
}
/// Verifies eval extended scalar math builtins support direct, named, callable, and probe paths.
#[test]
fn execute_program_dispatches_extended_math_builtins() {
    let program = parse_fragment(
        br#"echo sin(0); echo ":";
echo cos(0); echo ":";
echo tan(0); echo ":";
echo round(asin(1), 2); echo ":";
echo acos(1); echo ":";
echo round(atan(1), 2); echo ":";
echo sinh(0); echo ":";
echo cosh(0); echo ":";
echo tanh(0); echo ":";
echo log2(8); echo ":";
echo log10(100); echo ":";
echo exp(0); echo ":";
echo round(deg2rad(180), 2); echo ":";
echo round(rad2deg(pi()), 0); echo ":";
echo log(num: 8, base: 2); echo ":";
echo atan2(y: 0, x: 1); echo ":";
echo hypot(3, 4); echo ":";
echo intdiv(7, 2); echo ":";
echo round(call_user_func("sin", pi() / 2), 0); echo ":";
echo call_user_func_array("intdiv", ["num1" => 9, "num2" => 2]); echo ":";
echo function_exists("sin"); echo function_exists("log"); echo function_exists("intdiv");
return function_exists("hypot");"#,
    )
    .expect("parse eval fragment");
    let mut scope = ElephcEvalScope::new();
    let mut values = FakeOps::default();

    let result = execute_program(&program, &mut scope, &mut values).expect("execute eval ir");

    assert_eq!(
        values.output,
        "0:1:0:1.57:0:0.79:0:1:0:3:2:1:3.14:180:3:0:5:3:1:4:111"
    );
    assert_eq!(values.get(result), FakeValue::Bool(true));
}
/// Verifies eval `pow()` dispatches through the existing exponentiation runtime hook.
#[test]
fn execute_program_dispatches_pow_builtin() {
    let program = parse_fragment(
        br#"echo pow(2, 3); echo ":";
echo gettype(pow(2, 3)); echo ":";
echo call_user_func("pow", 2, 5); echo ":";
echo call_user_func_array("pow", [3, 3]);
return function_exists("pow");"#,
    )
    .expect("parse eval fragment");
    let mut scope = ElephcEvalScope::new();
    let mut values = FakeOps::default();

    let result = execute_program(&program, &mut scope, &mut values).expect("execute eval ir");

    assert_eq!(values.output, "8:double:32:27");
    assert_eq!(values.get(result), FakeValue::Bool(true));
}
/// Verifies eval `round()` supports default and explicit precision through callable paths.
#[test]
fn execute_program_dispatches_round_builtin() {
    let program = parse_fragment(
        br#"echo round(3.5); echo ":";
echo round(3.14159, 2); echo ":";
echo gettype(round(3)); echo ":";
echo call_user_func("round", 2.5); echo ":";
echo call_user_func_array("round", [1.55, 1]);
return function_exists("round");"#,
    )
    .expect("parse eval fragment");
    let mut scope = ElephcEvalScope::new();
    let mut values = FakeOps::default();

    let result = execute_program(&program, &mut scope, &mut values).expect("execute eval ir");

    assert_eq!(values.output, "4:3.14:double:3:1.6");
    assert_eq!(values.get(result), FakeValue::Bool(true));
}
/// Verifies eval `number_format()` groups and rounds numbers through callable paths.
#[test]
fn execute_program_dispatches_number_format_builtin() {
    let program = parse_fragment(
            br#"echo number_format(1234567); echo ":";
echo number_format(1234.5678, 2); echo ":";
echo number_format(num: 1234567.89, decimals: 2, decimal_separator: ",", thousands_separator: "."); echo ":";
echo number_format(1234567.89, 2, ".", ""); echo ":";
echo call_user_func("number_format", -1234.5, 1); echo ":";
echo call_user_func_array("number_format", ["num" => 1234, "decimals" => 0, "decimal_separator" => ".", "thousands_separator" => " "]); echo ":";
return function_exists("number_format");"#,
        )
        .expect("parse eval fragment");
    let mut scope = ElephcEvalScope::new();
    let mut values = FakeOps::default();

    let result = execute_program(&program, &mut scope, &mut values).expect("execute eval ir");

    assert_eq!(
        values.output,
        "1,234,567:1,234.57:1.234.567,89:1234567.89:-1,234.5:1 234:"
    );
    assert_eq!(values.get(result), FakeValue::Bool(true));
}
/// Verifies eval printf-family builtins format, print, and dispatch through callables.
#[test]
fn execute_program_dispatches_printf_family_builtins() {
    let program = parse_fragment(
        br#"echo sprintf("Hello %s", "World"); echo ":";
echo sprintf("%05d", 42); echo ":";
echo sprintf("%.2f", 3.14159); echo ":";
echo sprintf("%-6s|", "hi"); echo ":";
$printed = printf("%s=%d", "n", 42);
echo ":" . $printed . ":";
echo vsprintf("%s/%d/%.1f", ["age", 42, 3]); echo ":";
$vprinted = vprintf("%s-%d", ["v", 7]);
echo ":" . $vprinted . ":";
echo call_user_func("sprintf", "%+d", 42); echo ":";
echo call_user_func_array("vsprintf", ["format" => "%s", "values" => ["spread"]]); echo ":";
echo function_exists("sprintf"); echo is_callable("printf"); echo function_exists("vsprintf");
return is_callable("vprintf");"#,
    )
    .expect("parse eval fragment");
    let mut scope = ElephcEvalScope::new();
    let mut values = FakeOps::default();

    let result = execute_program(&program, &mut scope, &mut values).expect("execute eval ir");

    assert_eq!(
        values.output,
        "Hello World:00042:3.14:hi    |:n=42:4:age/42/3.0:v-7:3:+42:spread:111"
    );
    assert_eq!(values.get(result), FakeValue::Bool(true));
}
/// Verifies eval `sscanf()` returns indexed string matches through callable paths.
#[test]
fn execute_program_dispatches_sscanf_builtin() {
    let program = parse_fragment(
        br#"$result = sscanf("John 1.5 30", "%s %f %d");
echo $result[0] . ":" . $result[1] . ":" . $result[2] . ":";
$named = sscanf(string: "Age: -25", format: "Age: %d");
echo $named[0] . ":";
$call = call_user_func("sscanf", "-2.5e3", "%f");
echo $call[0] . ":";
$spread = call_user_func_array("sscanf", ["string" => "ok %", "format" => "%s %%"]);
echo $spread[0] . ":";
return function_exists("sscanf");"#,
    )
    .expect("parse eval fragment");
    let mut scope = ElephcEvalScope::new();
    let mut values = FakeOps::default();

    let result = execute_program(&program, &mut scope, &mut values).expect("execute eval ir");

    assert_eq!(values.output, "John:1.5:30:-25:-2.5e3:ok:");
    assert_eq!(values.get(result), FakeValue::Bool(true));
}
/// Verifies eval `min()` and `max()` select numeric values directly and by callable.
#[test]
fn execute_program_dispatches_min_max_builtins() {
    let program = parse_fragment(
        br#"echo min(3, 1, 2); echo ":";
echo max(1, 3, 2); echo ":";
echo min(2.5, 1.5); echo ":";
echo max(1.5, 2.5); echo ":";
echo call_user_func("min", 9, 4, 7); echo ":";
echo call_user_func_array("max", [4, 8, 6]); echo ":";
echo function_exists("min");
return function_exists("max");"#,
    )
    .expect("parse eval fragment");
    let mut scope = ElephcEvalScope::new();
    let mut values = FakeOps::default();

    let result = execute_program(&program, &mut scope, &mut values).expect("execute eval ir");

    assert_eq!(values.output, "1:3:1.5:2.5:4:8:1");
    assert_eq!(values.get(result), FakeValue::Bool(true));
}
/// Verifies eval `clamp()` selects numeric values through direct, named, and callable paths.
#[test]
fn execute_program_dispatches_clamp_builtin() {
    let program = parse_fragment(
        br#"echo clamp(5, 0, 10); echo ":";
echo clamp(15, 0, 10); echo ":";
echo clamp(-5, 0, 10); echo ":";
echo clamp(2.75, 1.5, 2.5); echo ":";
echo clamp(value: 8, min: 0, max: 5); echo ":";
echo call_user_func("clamp", -1, 0, 10); echo ":";
echo call_user_func_array("clamp", ["value" => 9, "min" => 0, "max" => 7]); echo ":";
echo function_exists("clamp");
return is_callable("clamp");"#,
    )
    .expect("parse eval fragment");
    let mut scope = ElephcEvalScope::new();
    let mut values = FakeOps::default();

    let result = execute_program(&program, &mut scope, &mut values).expect("execute eval ir");

    assert_eq!(values.output, "5:10:0:2.5:5:0:7:1");
    assert_eq!(values.get(result), FakeValue::Bool(true));
}
/// Verifies eval `clamp()` rejects a lower bound greater than the upper bound.
#[test]
fn execute_program_rejects_clamp_invalid_bounds() {
    let program = parse_fragment(br#"return clamp(5, 10, 0);"#).expect("parse eval fragment");
    let mut scope = ElephcEvalScope::new();
    let mut values = FakeOps::default();

    let err = execute_program(&program, &mut scope, &mut values)
        .expect_err("invalid clamp bounds should fail");

    assert_eq!(err, EvalStatus::RuntimeFatal);
}
/// Verifies eval `pi()` returns a double constant directly and through callable paths.
#[test]
fn execute_program_dispatches_pi_builtin() {
    let program = parse_fragment(
        br#"echo round(pi(), 2); echo ":";
echo gettype(pi()); echo ":";
echo round(call_user_func("pi"), 3); echo ":";
echo round(call_user_func_array("pi", []), 4); echo ":";
return function_exists("pi");"#,
    )
    .expect("parse eval fragment");
    let mut scope = ElephcEvalScope::new();
    let mut values = FakeOps::default();

    let result = execute_program(&program, &mut scope, &mut values).expect("execute eval ir");

    assert_eq!(values.output, "3.14:double:3.142:3.1416:");
    assert_eq!(values.get(result), FakeValue::Bool(true));
}
/// Verifies eval `sqrt()` dispatches through runtime float hooks directly and by callable.
#[test]
fn execute_program_dispatches_sqrt_builtin() {
    let program = parse_fragment(
        br#"echo sqrt(16); echo ":";
echo gettype(sqrt(9)); echo ":";
echo call_user_func("sqrt", 25); echo ":";
echo call_user_func_array("sqrt", [36]);
return function_exists("sqrt");"#,
    )
    .expect("parse eval fragment");
    let mut scope = ElephcEvalScope::new();
    let mut values = FakeOps::default();

    let result = execute_program(&program, &mut scope, &mut values).expect("execute eval ir");

    assert_eq!(values.output, "4:double:5:6");
    assert_eq!(values.get(result), FakeValue::Bool(true));
}
