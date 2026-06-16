//! Purpose:
//! Integration tests for the initial `eval()` bridge wiring.
//! Covers language-construct visibility, conditional bridge linking, and the
//! base runtime interpreter path for scalar, branch, indexed-array, and simple builtin eval fragments.
//!
//! Called from:
//! - `cargo test --test codegen_tests eval` through Rust's test harness.
//!
//! Key details:
//! - These tests intentionally cover the first scalar/control-flow/indexed-array subset,
//!   not full PHP eval scope synchronization or dynamic declaration semantics.

use crate::support::*;

/// Verifies `eval` is resolved as a language construct, not a PHP-visible callable function.
#[test]
fn test_eval_is_not_function_exists_or_callable() {
    let out = compile_and_run(
        r#"<?php
echo function_exists("eval") ? "1" : "0";
echo is_callable("eval") ? "1" : "0";
"#,
    );
    assert_eq!(out, "00");
}

/// Verifies a program containing `eval()` references the bridge symbol and requests libelephc-eval.
#[test]
fn test_eval_codegen_requires_eval_bridge() {
    let dir = make_cli_test_dir("elephc_eval_bridge_asm");
    let (user_asm, _runtime_asm, required_libraries) =
        compile_source_to_asm_with_options("<?php eval('$x = 1;');", &dir, 8_388_608, false, false);
    assert!(
        user_asm.contains("__elephc_eval_execute"),
        "user assembly should call the eval bridge:\n{user_asm}"
    );
    assert!(
        user_asm.contains("__elephc_eval_context_new"),
        "user assembly should create a persistent eval context:\n{user_asm}"
    );
    assert!(
        user_asm.contains("__elephc_eval_context_free"),
        "user assembly should free the persistent eval context:\n{user_asm}"
    );
    assert!(
        required_libraries.iter().any(|lib| lib == "elephc_eval"),
        "required libraries should include elephc_eval: {required_libraries:?}"
    );
    let _ = fs::remove_dir_all(&dir);
}

/// Verifies programs without `eval` do not link or reference the optional eval bridge.
#[test]
fn test_non_eval_program_does_not_request_eval_bridge() {
    let dir = make_cli_test_dir("elephc_no_eval_bridge_asm");
    let (user_asm, runtime_asm, required_libraries) =
        compile_source_to_asm_with_options("<?php echo 1 + 2;", &dir, 8_388_608, false, false);
    assert!(
        !user_asm.contains("__elephc_eval_"),
        "non-eval user assembly should not reference eval bridge:\n{user_asm}"
    );
    assert!(
        !runtime_asm.contains("__elephc_eval_"),
        "non-eval runtime assembly should not reference eval bridge:\n{runtime_asm}"
    );
    assert!(
        !required_libraries.iter().any(|lib| lib == "elephc_eval"),
        "non-eval required libraries should not include elephc_eval: {required_libraries:?}"
    );
    let _ = fs::remove_dir_all(&dir);
}

/// Verifies the linked eval bridge can execute scalar echo fragments.
#[test]
fn test_eval_scalar_echo_executes_through_bridge() {
    let out = compile_and_run("<?php eval('echo \"x\";');");
    assert_eq!(out, "x");
}

/// Verifies comma-separated echo expressions inside eval emit in source order.
#[test]
fn test_eval_echo_comma_list_executes_through_bridge() {
    let out = compile_and_run("<?php eval('echo \"a\", \"b\", \"c\";');");
    assert_eq!(out, "abc");
}

/// Verifies print inside eval emits output through the bridge.
#[test]
fn test_eval_print_executes_through_bridge() {
    let out = compile_and_run("<?php eval('print \"x\";');");
    assert_eq!(out, "x");
}

/// Verifies print inside eval returns integer 1 like PHP.
#[test]
fn test_eval_print_return_value_is_one() {
    let out = compile_and_run("<?php echo eval('return print \"x\";');");
    assert_eq!(out, "x1");
}

/// Verifies eval `print_r()` writes supported values and returns true.
#[test]
fn test_eval_dispatches_print_r_builtin_call() {
    let out = compile_and_run(
        r#"<?php
eval('print_r("x"); echo ":";
print_r(value: false); echo ":";
print_r([1, 2]); echo ":";
$call = call_user_func("print_r", true);
$spread = call_user_func_array("print_r", ["value" => "z"]);
echo ":" . ($call ? "call" : "bad") . ":" . ($spread ? "spread" : "bad") . ":";
echo function_exists("print_r");');
"#,
    );
    assert_eq!(out, "x::Array\n:1z:call:spread:1");
}

/// Verifies eval `var_dump()` writes PHP-style diagnostics and returns null.
#[test]
fn test_eval_dispatches_var_dump_builtin_call() {
    let out = compile_and_run(
        r#"<?php
eval('var_dump(42);
var_dump("hi");
var_dump(false);
var_dump(null);
var_dump([10, 20]);
var_dump(["x" => true]);
$call = call_user_func("var_dump", 3.5);
$spread = call_user_func_array("var_dump", ["value" => "z"]);
echo ($call === null ? "call-null" : "bad") . ":" . ($spread === null ? "spread-null" : "bad") . ":";
echo function_exists("var_dump");');
"#,
    );
    assert_eq!(
        out,
        concat!(
            "int(42)\n",
            "string(2) \"hi\"\n",
            "bool(false)\n",
            "NULL\n",
            "array(2) {\n",
            "  [0]=>\n",
            "  int(10)\n",
            "  [1]=>\n",
            "  int(20)\n",
            "}\n",
            "array(1) {\n",
            "  [\"x\"]=>\n",
            "  bool(true)\n",
            "}\n",
            "float(3.5)\n",
            "string(1) \"z\"\n",
            "call-null:spread-null:1",
        )
    );
}

/// Verifies eval fragments accept PHP comments and keep line metadata aligned.
#[test]
fn test_eval_comments_execute_through_bridge() {
    let out = compile_and_run(
        r#"<?php
echo eval("// leading\n# hash\n/* block\ncomment */ return __LINE__;");
"#,
    );
    assert_eq!(out, "4");
}

/// Verifies eval coerces null to an empty fragment and returns null.
#[test]
fn test_eval_null_argument_is_empty_fragment() {
    let out = compile_and_run("<?php echo eval(null);");
    assert_eq!(out, "");
}

/// Verifies non-string scalar eval arguments are coerced before runtime parsing.
#[test]
fn test_eval_integer_argument_is_coerced_then_parse_checked() {
    let err = compile_and_run_expect_failure("<?php eval(123);");
    assert!(
        err.contains("Parse error: eval() fragment is invalid"),
        "stderr did not contain eval parse diagnostic: {err}"
    );
}

/// Verifies the eval bridge routes base numeric operations through runtime Mixed helpers.
#[test]
fn test_eval_scalar_add_executes_through_bridge() {
    let out = compile_and_run("<?php eval('echo 2 + 3 * 4 - 5;');");
    assert_eq!(out, "9");
}

/// Verifies eval division and modulo execute through target-specific bridge wrappers.
#[test]
fn test_eval_division_modulo_execute_through_bridge() {
    let out = compile_and_run(
        r#"<?php
echo eval('return 9 / 2;');
echo ":";
echo eval('return 10 % 4;');
echo ":";
eval('$x = 20; $x /= 2; $x %= 6; echo $x;');
"#,
    );
    assert_eq!(out, "4.5:2:4");
}

/// Verifies eval exponentiation executes through the target-specific bridge wrapper.
#[test]
fn test_eval_exponentiation_execute_through_bridge() {
    let out = compile_and_run(
        r#"<?php
echo eval('return 2 ** 3 ** 2;');
echo ":";
echo eval('return -2 ** 2;');
echo ":";
eval('$x = 2; $x **= 3; echo $x;');
"#,
    );
    assert_eq!(out, "512:-4:8");
}

/// Verifies eval integer bitwise and shift operators execute through bridge wrappers.
#[test]
fn test_eval_bitwise_shift_execute_through_bridge() {
    let out = compile_and_run(
        r#"<?php
eval('echo (5 & 3) . ":" . (5 | 3) . ":" . (5 ^ 3) . ":" . (~0) . ":" . (1 << 4) . ":" . (-16 >> 2);');
echo ":";
eval('$x = 6; $x &= 3; echo $x; echo ","; $x = 4; $x |= 1; echo $x; echo ","; $x = 7; $x ^= 3; echo $x; echo ","; $x = 1; $x <<= 5; echo $x; echo ","; $x = 64; $x >>= 3; echo $x;');
"#,
    );
    assert_eq!(out, "1:7:6:-1:16:-4:2,5,4,32,8");
}

/// Verifies the eval bridge routes concatenation through runtime string helpers.
#[test]
fn test_eval_scalar_concat_executes_through_bridge() {
    let out = compile_and_run("<?php eval('echo \"a\" . \"b\";');");
    assert_eq!(out, "ab");
}

/// Verifies eval comparison operators return boxed booleans through the bridge.
#[test]
fn test_eval_scalar_comparisons_execute_through_bridge() {
    let out = compile_and_run(
        r#"<?php
eval('echo 2 < 3; echo 3 <= 3; echo 4 > 3; echo 4 >= 4; echo 5 != 6; echo 7 == 7;');
"#,
    );
    assert_eq!(out, "111111");
}

/// Verifies eval spaceship comparisons return boxed -1/0/1 integers.
#[test]
fn test_eval_spaceship_execute_through_bridge() {
    let out = compile_and_run(
        r#"<?php
eval('echo 1 <=> 2; echo ":"; echo 2 <=> 2; echo ":"; echo 3 <=> 2;');
"#,
    );
    assert_eq!(out, "-1:0:1");
}

/// Verifies loose scalar equality in eval handles strings and null/empty-string rules.
#[test]
fn test_eval_scalar_loose_equality_executes_through_bridge() {
    let out = compile_and_run(
        r#"<?php
eval('echo "a" == "a"; echo "a" != "b"; echo "" == null; echo "10" == 10; echo "foo" != 0; echo "10" == "1e1";');
"#,
    );
    assert_eq!(out, "111111");
}

/// Verifies strict scalar equality in eval preserves PHP type identity.
#[test]
fn test_eval_scalar_strict_equality_executes_through_bridge() {
    let out = compile_and_run(
        r#"<?php
eval('echo "10" == 10; echo "10" === 10; echo "10" === "10"; echo "10" !== 10; echo null === null;');
"#,
    );
    assert_eq!(out, "1111");
}

/// Verifies eval logical operators short-circuit before evaluating unsupported RHS calls.
#[test]
fn test_eval_logical_operators_short_circuit() {
    let out = compile_and_run(
        r#"<?php
echo "a" . eval('return false && missing_eval_rhs();') . "b";
echo ":";
echo eval('return true || missing_eval_rhs();');
"#,
    );
    assert_eq!(out, "ab:1");
}

/// Verifies eval supports PHP logical keyword operators with PHP precedence.
#[test]
fn test_eval_logical_keyword_operators_execute_through_bridge() {
    let out = compile_and_run(
        r#"<?php
echo eval('return (false || true and false) ? "T" : "F";');
echo ":";
echo eval('return (true xor false) ? "T" : "F";');
echo ":";
echo eval('return (true xor true) ? "T" : "F";');
echo ":";
echo eval('return true or missing_eval_rhs();');
"#,
    );
    assert_eq!(out, "F:T:F:1");
}

/// Verifies eval logical negation returns PHP boolean cells through the bridge.
#[test]
fn test_eval_logical_not_executes_through_bridge() {
    let out = compile_and_run(
        r#"<?php
echo eval('return !false;');
echo ":";
echo eval('return !"x";');
"#,
    );
    assert_eq!(out, "1:");
}

/// Verifies eval ternary operators short-circuit and return the selected branch.
#[test]
fn test_eval_ternary_executes_through_bridge() {
    let out = compile_and_run(
        r#"<?php
echo eval('return true ? "yes" : missing_eval_rhs();');
echo ":";
echo eval('return false ? missing_eval_rhs() : "no";');
echo ":";
echo eval('return "x" ?: "fallback";');
echo ":";
echo eval('return false ?: "fallback";');
"#,
    );
    assert_eq!(out, "yes:no:x:fallback");
}

/// Verifies eval null coalescing returns defaults only for missing or null values.
#[test]
fn test_eval_null_coalesce_executes_through_bridge() {
    let out = compile_and_run(
        r#"<?php
echo eval('return $missing ?? "fallback";');
echo ":";
echo eval('$x = null; return $x ?? "null-fallback";');
echo ":";
echo eval('return "set" ?? missing_eval_rhs();');
"#,
    );
    assert_eq!(out, "fallback:null-fallback:set");
}

/// Verifies eval unary numeric operators execute through runtime numeric helpers.
#[test]
fn test_eval_unary_numeric_operators_execute_through_bridge() {
    let out = compile_and_run("<?php echo eval('return -5 + +2;');");
    assert_eq!(out, "-3");
}

/// Verifies eval simple variable compound assignments execute through existing value hooks.
#[test]
fn test_eval_compound_assignment_executes_through_bridge() {
    let out = compile_and_run(
        r#"<?php
eval('$x = 2; $x += 3; $x *= 4; $x -= 5; $s = "v"; $s .= $x; echo $s;');
echo ":";
eval('for ($i = 0; $i < 3; $i += 1) { echo $i; }');
"#,
    );
    assert_eq!(out, "v15:012");
}

/// Verifies eval simple variable increment and decrement statements execute in loops.
#[test]
fn test_eval_inc_dec_statements_execute_through_bridge() {
    let out = compile_and_run(
        r#"<?php
eval('$i = 1; $i++; ++$i; $i--; --$i; echo $i;');
echo ":";
eval('for ($j = 0; $j < 3; $j++) { echo $j; }');
echo ":";
eval('for ($k = 3; $k > 0; --$k) { echo $k; }');
"#,
    );
    assert_eq!(out, "1:012:321");
}

/// Verifies eval if/else branches use PHP truthiness and update the caller scope.
#[test]
fn test_eval_if_else_updates_scope() {
    let out = compile_and_run(
        r#"<?php
$flag = "0";
eval('if ($flag) { $result = "then"; } else { $result = "else"; }');
echo $result;
"#,
    );
    assert_eq!(out, "else");
}

/// Verifies eval elseif chains execute the first truthy branch.
#[test]
fn test_eval_elseif_updates_scope() {
    let out = compile_and_run(
        r#"<?php
eval('if (false) { $result = "a"; } elseif (true) { $result = "b"; } else { $result = "c"; }');
echo $result;
"#,
    );
    assert_eq!(out, "b");
}

/// Verifies eval accepts PHP's separate `else if` spelling.
#[test]
fn test_eval_else_if_updates_scope() {
    let out = compile_and_run(
        r#"<?php
eval('if (false) { $result = "a"; } else if (true) { $result = "b"; } else { $result = "c"; }');
echo $result;
"#,
    );
    assert_eq!(out, "b");
}

/// Verifies eval accepts braceless single-statement control-flow bodies.
#[test]
fn test_eval_braceless_control_flow_bodies() {
    let out = compile_and_run(
        r#"<?php
$flag = false;
eval('if ($flag) echo "a"; else echo "b"; while (false) echo "x"; do echo "d"; while (false);');
"#,
    );
    assert_eq!(out, "bd");
}

/// Verifies eval while loops repeatedly execute against the materialized scope.
#[test]
fn test_eval_while_updates_scope() {
    let out = compile_and_run(
        r#"<?php
$i = 3;
eval('while ($i) { echo $i; $i = $i - 1; }');
echo $i;
"#,
    );
    assert_eq!(out, "3210");
}

/// Verifies eval do/while loops execute the body before checking the condition.
#[test]
fn test_eval_do_while_runs_body_before_condition() {
    let out = compile_and_run(
        r#"<?php
$i = 0;
eval('do { echo $i; $i = $i + 1; } while (false);');
echo ":" . $i;
"#,
    );
    assert_eq!(out, "0:1");
}

/// Verifies eval switch supports matching, default fallback, and fallthrough.
#[test]
fn test_eval_switch_matches_default_and_fallthrough() {
    let out = compile_and_run(
        r#"<?php
eval('$x = 2; switch ($x) { default: echo "d"; case 2: echo "2"; break; } $x = 3; switch ($x) { default: echo "D"; case 2: echo "F"; break; }');
"#,
    );
    assert_eq!(out, "2DF");
}

/// Verifies eval match expressions use strict comparisons and lazy result arms.
#[test]
fn test_eval_match_expression_dispatches_strict_arms() {
    let out = compile_and_run(
        r#"<?php
eval('$x = "1";
echo match ($x) { 1 => "int", "1" => "string", default => "other" };
echo ":";
echo match (3) { 1, 2 => missing(), default => "fallback" };');
"#,
    );
    assert_eq!(out, "string:fallback");
}

/// Verifies break and continue control a loop interpreted inside eval.
#[test]
fn test_eval_break_and_continue_control_loop() {
    let out = compile_and_run(
        r#"<?php
$i = 3;
eval('while ($i) { $i = $i - 1; if ($i) { continue; } echo "done"; break; }');
echo ":" . $i;
"#,
    );
    assert_eq!(out, "done:0");
}

/// Verifies `for` loops inside eval run init, body, update, and condition in order.
#[test]
fn test_eval_for_loop_updates_scope() {
    let out = compile_and_run(
        r#"<?php
eval('for ($i = 3; $i; $i = $i - 1) { echo $i; }');
echo ":" . $i;
"#,
    );
    assert_eq!(out, "321:0");
}

/// Verifies `continue` inside an eval `for` loop still runs the update clause.
#[test]
fn test_eval_for_continue_runs_update() {
    let out = compile_and_run(
        r#"<?php
eval('for ($i = 3; $i; $i = $i - 1) { if ($i - 1) { continue; } echo "done"; }');
echo ":" . $i;
"#,
    );
    assert_eq!(out, "done:0");
}

/// Verifies eval `for` conditions can use ordered comparisons.
#[test]
fn test_eval_for_loop_uses_less_than_condition() {
    let out = compile_and_run(
        r#"<?php
eval('for ($i = 0; $i < 3; $i = $i + 1) { echo $i; }');
echo ":" . $i;
"#,
    );
    assert_eq!(out, "012:3");
}

/// Verifies value-only foreach loops inside eval iterate indexed array values.
#[test]
fn test_eval_foreach_iterates_indexed_values() {
    let out = compile_and_run(
        r#"<?php
eval('foreach ([1, 2, 3] as $item) { echo $item; }');
echo ":" . $item;
"#,
    );
    assert_eq!(out, "123:3");
}

/// Verifies key-value foreach loops inside eval expose indexed array positions.
#[test]
fn test_eval_foreach_iterates_indexed_keys_and_values() {
    let out = compile_and_run(
        r#"<?php
eval('foreach ([10, 20] as $key => $item) { echo $key . ":" . $item . ";"; }');
echo "|" . $key . ":" . $item;
"#,
    );
    assert_eq!(out, "0:10;1:20;|1:20");
}

/// Verifies eval foreach can iterate an indexed array from the caller scope.
#[test]
fn test_eval_foreach_reads_scope_array() {
    let out = compile_and_run(
        r#"<?php
$items = eval('return ["a", "b"];');
eval('foreach ($items as $item) { echo $item; }');
"#,
    );
    assert_eq!(out, "ab");
}

/// Verifies break and continue control value-only foreach loops inside eval.
#[test]
fn test_eval_foreach_honors_break_and_continue() {
    let out = compile_and_run(
        r#"<?php
eval('foreach ([1, 2, 3] as $item) { if ($item == 1) { continue; } echo $item; break; }');
echo ":" . $item;
"#,
    );
    assert_eq!(out, "2:2");
}

/// Verifies value-only foreach loops inside eval iterate associative array values.
#[test]
fn test_eval_foreach_iterates_assoc_values() {
    let out = compile_and_run(
        r#"<?php
eval('foreach (["a" => 1, "b" => 2] as $item) { echo $item; }');
"#,
    );
    assert_eq!(out, "12");
}

/// Verifies key-value foreach loops inside eval expose associative keys in insertion order.
#[test]
fn test_eval_foreach_iterates_assoc_keys_and_values() {
    let out = compile_and_run(
        r#"<?php
eval('foreach (["a" => 1, "b" => 2] as $key => $item) { echo $key . ":" . $item . ";"; }');
echo "|" . $key . ":" . $item;
"#,
    );
    assert_eq!(out, "a:1;b:2;|b:2");
}

/// Verifies eval indexed-array literals and reads execute through Mixed array helpers.
#[test]
fn test_eval_indexed_array_literal_and_read() {
    let out = compile_and_run("<?php echo eval('return [1, 2, 3][1];');");
    assert_eq!(out, "2");
}

/// Verifies eval accepts PHP's legacy `array(...)` literal syntax.
#[test]
fn test_eval_legacy_array_literal_executes_through_bridge() {
    let out = compile_and_run(
        r#"<?php
echo eval('return array("a", "b",)[1];');
echo ":";
echo eval('return array("name" => "Ada",)["name"];');
echo ":";
eval('$items = array(2 => "two", "tail",); echo $items[3];');
"#,
    );
    assert_eq!(out, "b:Ada:tail");
}

/// Verifies eval indexed-array writes mutate an array visible to native code.
#[test]
fn test_eval_indexed_array_write_is_visible_after_eval() {
    let out = compile_and_run(
        r#"<?php
eval('$items[0] = "a"; $items[1] = "b";');
echo $items[0] . $items[1];
"#,
    );
    assert_eq!(out, "ab");
}

/// Verifies eval mutates an existing native local array instead of replacing it with a fresh one.
#[test]
fn test_eval_mutates_existing_native_array_local() {
    let out = compile_and_run(
        r#"<?php
$items = ["a", "b"];
eval('$items[0] = "z"; $items[] = "c";');
echo $items[0] . ":" . $items[1] . ":" . $items[2] . ":" . count($items);
"#,
    );
    assert_eq!(out, "z:b:c:3");
}

/// Verifies eval indexed-array append syntax writes the next visible element.
#[test]
fn test_eval_indexed_array_append_is_visible_after_eval() {
    let out = compile_and_run(
        r#"<?php
eval('$items = []; $items[] = "a"; $items[] = "b";');
echo $items[0] . ":" . $items[1] . ":" . count($items);
$existing = eval('return ["x"];');
eval('$existing[] = "y";');
echo ":" . $existing[1] . ":" . count($existing);
"#,
    );
    assert_eq!(out, "a:b:2:y:2");
}

/// Verifies eval associative-array append uses PHP's next automatic integer key.
#[test]
fn test_eval_assoc_array_append_uses_php_next_key() {
    let out = compile_and_run(
        r#"<?php
echo eval('$items = ["name" => "Ada"]; $items[] = "Grace"; return $items[0];');
echo ":";
echo eval('$items = [2 => "two", "name" => "Ada"]; $items[] = "tail"; return $items[3];');
echo ":";
echo eval('$items = [-2 => "minus"]; $items[] = "tail"; return $items[-1];');
"#,
    );
    assert_eq!(out, "Grace:tail:tail");
}

/// Verifies eval can read a native Mixed array through runtime array helpers.
#[test]
fn test_eval_reads_native_mixed_array() {
    let out = compile_and_run(
        r#"<?php
$items = eval('return ["a", "b"];');
eval('echo $items[1];');
"#,
    );
    assert_eq!(out, "b");
}

/// Verifies eval can read string-keyed native associative arrays through Mixed helpers.
#[test]
fn test_eval_reads_native_assoc_array_string_key() {
    let out = compile_and_run(
        r#"<?php
$items = ["name" => "Ada"];
eval('echo $items["name"];');
"#,
    );
    assert_eq!(out, "Ada");
}

/// Verifies eval can write string-keyed native associative arrays through Mixed helpers.
#[test]
fn test_eval_writes_native_assoc_array_string_key() {
    let out = compile_and_run(
        r#"<?php
$items = ["name" => "Ada"];
eval('$items["name"] = "Grace";');
echo $items["name"];
"#,
    );
    assert_eq!(out, "Grace");
}

/// Verifies eval can create and read associative array literals with string keys.
#[test]
fn test_eval_assoc_array_literal_and_string_key_read() {
    let out = compile_and_run(r#"<?php echo eval('return ["name" => "Ada"]["name"];');"#);
    assert_eq!(out, "Ada");
}

/// Verifies eval associative-array literals use PHP's next automatic key.
#[test]
fn test_eval_assoc_array_literal_unkeyed_entries_use_next_key() {
    let out = compile_and_run(
        r#"<?php
echo eval('return ["name" => "Ada", "Grace"][0];');
echo ":";
echo eval('return [2 => "two", "tail"][3];');
echo ":";
echo eval('return [-2 => "minus", "tail"][-1];');
echo ":";
echo eval('return ["2" => "two", "tail"][3];');
echo ":";
echo eval('return ["02" => "two", "tail"][0];');
echo ":";
echo eval('return [null => "empty"][""];');
echo ":";
echo eval('return [null => "empty", "tail"][0];');
echo ":";
echo eval('return [true => "yes", "tail"][2];');
echo ":";
echo eval('return [false => "no", "tail"][1];');
echo ":";
echo eval('return [2.7 => "two", "tail"][3];');
"#,
    );
    assert_eq!(out, "Grace:tail:tail:tail:tail:empty:tail:tail:tail:tail");
}

/// Verifies eval-created associative arrays remain visible to native code.
#[test]
fn test_eval_created_assoc_array_is_visible_after_eval() {
    let out = compile_and_run(
        r#"<?php
eval('$items = ["name" => "Ada"];');
echo $items["name"];
"#,
    );
    assert_eq!(out, "Ada");
}

/// Verifies nested eval calls reuse the materialized caller scope.
#[test]
fn test_eval_nested_eval_uses_same_scope() {
    let out = compile_and_run(
        r#"<?php
$x = 1;
eval('eval("$x = $x + 4;");');
echo $x;
"#,
    );
    assert_eq!(out, "5");
}

/// Verifies a nested eval return is the value of the inner eval expression.
#[test]
fn test_eval_nested_eval_return_value_is_expression_result() {
    let out = compile_and_run(r#"<?php echo eval('return eval("return 9;");');"#);
    assert_eq!(out, "9");
}

/// Verifies eval can dispatch simple builtin calls through its dynamic call path.
#[test]
fn test_eval_dispatches_simple_builtin_calls() {
    let out = compile_and_run(
        r#"<?php
eval('echo STRLEN("abcd") . ":" . \strlen("xy") . ":" . count([1, [2, 3], [4]]) . ":";
echo count([1, [2, 3], [4]], COUNT_RECURSIVE) . ":";
echo call_user_func("count", [1, [2]]) . ":";
echo call_user_func_array("count", ["value" => [1, [2]], "mode" => COUNT_RECURSIVE]) . ":";
echo defined("COUNT_RECURSIVE") ? "C" : "bad";');
"#,
    );
    assert_eq!(out, "4:2:3:6:2:3:C");
}

/// Verifies eval `json_encode()` serializes scalar, indexed, and associative values.
#[test]
fn test_eval_dispatches_json_encode_builtin_call() {
    let out = compile_and_run(
        r#"<?php
eval('echo json_encode(["a" => 1, "b" => "x/y"]) . ":";
echo json_encode([1, "q", true, null]) . ":";
echo call_user_func("json_encode", "a/b\"c") . ":";
echo call_user_func_array("json_encode", ["value" => ["k" => false], "flags" => 0, "depth" => 4]) . ":";
echo json_encode("a/b", JSON_UNESCAPED_SLASHES) . ":";
echo call_user_func_array("json_encode", ["value" => "x/y", "flags" => JSON_UNESCAPED_SLASHES]) . ":";
echo function_exists("json_encode");');
"#,
    );
    assert_eq!(
        out,
        r#"{"a":1,"b":"x\/y"}:[1,"q",true,null]:"a\/b\"c":{"k":false}:"a/b":"x/y":1"#
    );
}

/// Verifies eval `json_decode()` materializes scalar, indexed, and associative values.
#[test]
fn test_eval_dispatches_json_decode_builtin_call() {
    let out = compile_and_run(
        r#"<?php
eval('echo json_decode("\"hello\"") . ":";
echo json_decode("42") . ":";
echo (json_decode("true") ? "T" : "bad") . ":";
echo (is_null(json_decode("null")) ? "NULL" : "bad") . ":";
$decoded = json_decode("{\"a\":1,\"b\":[\"x\",false]}", true);
echo $decoded["a"] . ":" . $decoded["b"][0] . ":" . ($decoded["b"][1] ? "bad" : "F") . ":";
$call = call_user_func("json_decode", "[3,4]");
echo $call[1] . ":";
$named = call_user_func_array("json_decode", ["json" => "{\"k\":\"v\"}", "associative" => true, "depth" => 4, "flags" => 0]);
echo $named["k"] . ":";
echo (is_null(json_decode("bad")) ? "BAD" : "wrong") . ":";
echo function_exists("json_decode");');
"#,
    );
    assert_eq!(out, "hello:42:T:NULL:1:x:F:4:v:BAD:1");
}

/// Verifies eval `json_last_error*()` track JSON parse failures and success resets.
#[test]
fn test_eval_dispatches_json_last_error_builtin_calls() {
    let out = compile_and_run(
        r#"<?php
eval('echo json_last_error() . ":" . json_last_error_msg() . ":";
json_decode("bad");
echo json_last_error() . ":" . (json_last_error() === JSON_ERROR_SYNTAX ? "syntax" : "bad") . ":" . json_last_error_msg() . ":";
json_validate("[1]", 1);
echo json_last_error() . ":" . json_last_error_msg() . ":";
json_validate("\"ok\"");
echo json_last_error() . ":" . json_last_error_msg() . ":";
json_validate("\"a" . chr(10) . "b\"");
echo json_last_error() . ":" . json_last_error_msg() . ":";
json_decode("\"\\uD83D\"");
echo json_last_error() . ":" . json_last_error_msg() . ":";
json_decode("\"a" . chr(128) . "b\"");
echo json_last_error() . ":" . json_last_error_msg() . ":";
json_validate("[0]");
echo call_user_func("json_last_error") . ":" . call_user_func_array("json_last_error_msg", []) . ":";
echo function_exists("json_last_error") && function_exists("json_last_error_msg") && defined("JSON_ERROR_SYNTAX");');
"#,
    );
    assert_eq!(
        out,
        "0:No error:4:syntax:Syntax error near location 1:1:1:Maximum stack depth exceeded near location 1:1:0:No error:3:Control character error, possibly incorrectly encoded near location 1:3:10:Single unpaired UTF-16 surrogate in unicode escape near location 1:8:5:Malformed UTF-8 characters, possibly incorrectly encoded near location 1:3:0:No error:1"
    );
}

/// Verifies eval `json_validate()` validates JSON syntax, depth, and dynamic calls.
#[test]
fn test_eval_dispatches_json_validate_builtin_call() {
    let out = compile_and_run(
        r#"<?php
eval('echo (json_validate("{\"a\":[1,true,null,\"caf\\u00e9\"]}") ? "Y" : "N") . ":";
echo (json_validate("bad") ? "bad" : "N") . ":";
echo (json_validate("[1]", 1) ? "bad" : "D") . ":";
echo (call_user_func("json_validate", "\"x\"") ? "C" : "bad") . ":";
echo (call_user_func_array("json_validate", ["json" => "[[1]]", "depth" => 3, "flags" => 0]) ? "A" : "bad") . ":";
echo function_exists("json_validate");');
"#,
    );
    assert_eq!(out, "Y:N:D:C:A:1");
}

/// Verifies eval direct builtin calls bind named arguments and spread arrays.
#[test]
fn test_eval_dispatches_named_and_spread_builtin_calls() {
    let out = compile_and_run(
        r#"<?php
eval('echo strlen(string: "abcd");
echo ":" . (array_key_exists(array: ["name" => 1], key: "name") ? "Y" : "N");
echo ":" . round(precision: 1, num: 3.14);
echo ":" . (str_contains(...["haystack" => "abc", "needle" => "b"]) ? "Y" : "N");');
"#,
    );
    assert_eq!(out, "4:Y:3.1:Y");
}

/// Verifies eval `ord()` returns the first byte and dispatches dynamically.
#[test]
fn test_eval_dispatches_ord_builtin_call() {
    let out = compile_and_run(
        r#"<?php
eval('echo ord("A");
echo ":" . ord("");
echo ":" . call_user_func("ord", "B");
echo ":" . call_user_func_array("ord", ["C"]);
echo ":"; echo function_exists("ord");');
"#,
    );
    assert_eq!(out, "65:0:66:67:1");
}

/// Verifies eval array aggregate builtins iterate values and dispatch dynamically.
#[test]
fn test_eval_dispatches_array_aggregate_builtin_calls() {
    let out = compile_and_run(
        r#"<?php
eval('echo array_sum([1, 2, 3]);
echo ":" . array_product([2, 3, 4]);
echo ":" . array_sum([]);
echo ":" . array_product([]);
echo ":" . array_sum(["a" => 2, "b" => 5]);
echo ":" . call_user_func("array_sum", [3, 4]);
echo ":" . call_user_func_array("array_product", [[2, 5]]);
echo ":"; echo function_exists("array_sum"); echo function_exists("array_product");');
"#,
    );
    assert_eq!(out, "6:24:0:1:7:7:10:11");
}

/// Verifies eval `array_map()` applies callbacks and preserves source keys.
#[test]
fn test_eval_dispatches_array_map_builtin_call() {
    let out = compile_and_run(
        r#"<?php
eval('function eval_map_double($value) { return $value * 2; }
$mapped = array_map("eval_map_double", [1, 2, 3]);
echo $mapped[0] . ":" . $mapped[2] . ":";
$assoc = array_map("strtoupper", ["a" => "x", "b" => "y"]);
echo $assoc["a"] . ":" . $assoc["b"] . ":";
$identity = array_map(null, ["k" => "v"]);
echo $identity["k"] . ":";
function eval_map_pair($left, $right) { return $left . "-" . ($right ?? "N"); }
$pairs = array_map("eval_map_pair", ["a" => "L", "b" => "R"], ["x" => "1"]);
echo $pairs[0] . ":" . $pairs[1] . ":";
$zipped = array_map(null, [1, 2], [3, 4]);
echo $zipped[0][0] . $zipped[0][1] . ":" . $zipped[1][0] . $zipped[1][1] . ":";
$call = call_user_func("array_map", "intval", ["7"]);
echo $call[0] . ":";
$multi_call = call_user_func("array_map", "eval_map_pair", ["Q"], ["9"]);
echo $multi_call[0] . ":";
$spread = call_user_func_array("array_map", ["callback" => "strval", "array" => [8]]);
echo $spread[0] . ":";
echo function_exists("array_map");');
"#,
    );
    assert_eq!(out, "2:6:X:Y:v:L-1:R-N:13:24:7:Q-9:8:1");
}

/// Verifies eval `array_reduce()` folds values through a string callback.
#[test]
fn test_eval_dispatches_array_reduce_builtin_call() {
    let out = compile_and_run(
        r#"<?php
eval('function eval_reduce_sum($carry, $item) { return $carry + $item; }
echo array_reduce([1, 2, 3], "eval_reduce_sum", 10) . ":";
function eval_reduce_join($carry, $item) { return $carry . $item; }
echo array_reduce([4, 5], "eval_reduce_sum") . ":";
echo array_reduce(["a", "b"], "eval_reduce_join", "") . ":";
$named = array_reduce(array: [6, 7], callback: "eval_reduce_sum");
echo $named . ":";
$call = call_user_func("array_reduce", [4, 5], "eval_reduce_sum");
echo $call . ":";
$spread = call_user_func_array("array_reduce", ["array" => [2, 3], "callback" => "eval_reduce_sum", "initial" => 4]);
echo $spread . ":";
echo function_exists("array_reduce");');
"#,
    );
    assert_eq!(out, "16:9:ab:13:9:9:1");
}

/// Verifies eval `array_walk()` invokes string callbacks with value and key cells.
#[test]
fn test_eval_dispatches_array_walk_builtin_call() {
    let out = compile_and_run(
        r#"<?php
eval('function eval_walk_show($value, $key) { echo $key . "=" . $value . ";"; }
echo array_walk(["a" => 2, "b" => 3], "eval_walk_show") ? "T:" : "F:";
$call = call_user_func("array_walk", [4, 5], "eval_walk_show");
$spread = call_user_func_array("array_walk", ["array" => ["z" => 6], "callback" => "eval_walk_show"]);
echo function_exists("array_walk");');
"#,
    );
    assert_eq!(out, "a=2;b=3;T:0=4;1=5;z=6;1");
}

/// Verifies eval `array_pop()` and `array_shift()` mutate direct variable arguments only.
#[test]
fn test_eval_dispatches_array_pop_shift_builtin_calls() {
    let out = compile_and_run(
        r#"<?php
eval('$a = [1, 2, 3];
echo array_pop($a) . ":" . count($a) . ":" . $a[1] . ":";
$b = ["x" => 1, 10 => 2, "y" => 3, 11 => 4];
echo array_shift(array: $b) . ":" . $b[0] . ":" . $b["y"] . ":" . $b[1] . ":";
$c = [4, 5];
echo call_user_func("array_pop", $c) . ":" . count($c) . ":" . $c[1] . ":";
$d = [6, 7];
echo call_user_func_array("array_shift", ["array" => $d]) . ":" . count($d) . ":" . $d[0] . ":";
echo function_exists("array_pop") && function_exists("array_shift");');
"#,
    );
    assert_eq!(out, "3:2:2:1:2:3:4:5:2:5:6:2:6:1");
}

/// Verifies eval `array_push()` and `array_unshift()` mutate direct variable arguments only.
#[test]
fn test_eval_dispatches_array_push_unshift_builtin_calls() {
    let out = compile_and_run(
        r#"<?php
eval('$a = [1];
echo array_push($a, 2, 3) . ":" . $a[2] . ":";
$b = ["x" => 1, 10 => 2];
echo array_push($b, "A") . ":" . $b["x"] . ":" . $b[11] . ":";
$c = [2, 3];
echo array_unshift($c, 0, 1) . ":" . $c[0] . ":" . $c[3] . ":";
$d = ["x" => 1, 10 => 2, "y" => 3];
echo array_unshift($d, "A") . ":" . $d[0] . ":" . $d["x"] . ":" . $d[1] . ":" . $d["y"] . ":";
$e = [5];
echo call_user_func("array_push", $e, 6) . ":" . count($e) . ":" . $e[0] . ":";
$f = [7];
echo call_user_func_array("array_unshift", [$f, 6]) . ":" . count($f) . ":" . $f[0] . ":";
echo function_exists("array_push") && function_exists("array_unshift");');
"#,
    );
    assert_eq!(out, "3:3:3:1:A:4:0:3:4:A:1:2:3:2:1:5:2:1:7:1");
}

/// Verifies eval `array_splice()` mutates direct variable arguments only.
#[test]
fn test_eval_dispatches_array_splice_builtin_call() {
    let out = compile_and_run(
        r#"<?php
eval('$a = [10, 20, 30, 40];
$removed = array_splice($a, 1, 2);
echo count($removed) . ":" . $removed[0] . ":" . $removed[1] . ":" . count($a) . ":" . $a[1] . ":";
$b = ["x" => 1, 10 => 2, "y" => 3, 11 => 4];
$cut = array_splice(array: $b, offset: 1, length: 2);
echo $cut[0] . ":" . $cut["y"] . ":" . $b["x"] . ":" . $b[0] . ":";
$c = [1, 2, 3, 4];
$tail = call_user_func("array_splice", $c, -2, 1);
echo $tail[0] . ":" . count($c) . ":" . $c[2] . ":";
$d = [5, 6, 7];
$all = call_user_func_array("array_splice", ["array" => $d, "offset" => 1]);
echo count($all) . ":" . $all[0] . ":" . $all[1] . ":" . count($d) . ":";
$e = [1, 2, 3, 4];
$rep = array_splice($e, 1, 2, ["A", "B"]);
echo count($rep) . ":" . $rep[0] . ":" . $rep[1] . ":" . $e[0] . ":" . $e[1] . ":" . $e[2] . ":" . $e[3] . ":";
$f = ["x" => 1, 10 => 2, "y" => 3, 11 => 4];
$rep2 = array_splice(array: $f, offset: 1, length: 2, replacement: ["s" => "S", 9 => "N"]);
echo $rep2[0] . ":" . $rep2["y"] . ":" . $f["x"] . ":" . $f[0] . ":" . $f[1] . ":" . $f[2] . ":";
$g = [1, 2, 3];
$rep3 = array_splice($g, offset: 1, replacement: [9]);
echo count($rep3) . ":" . $rep3[0] . ":" . $rep3[1] . ":" . count($g) . ":" . $g[1] . ":";
$h = [1, 2, 3];
$removed2 = call_user_func_array("array_splice", ["array" => $h, "offset" => 1, "replacement" => [9]]);
echo count($removed2) . ":" . $removed2[0] . ":" . $removed2[1] . ":" . count($h) . ":" . $h[1] . ":";
echo function_exists("array_splice");');
"#,
    );
    assert_eq!(
        out,
        "2:20:30:2:40:2:3:1:4:3:4:3:2:6:7:3:2:2:3:1:A:B:4:2:3:1:S:N:4:2:2:3:2:9:2:2:3:3:2:1"
    );
}

/// Verifies eval `sort()` and `rsort()` mutate direct variable arguments only.
#[test]
fn test_eval_dispatches_sort_builtin_calls() {
    let out = compile_and_run(
        r#"<?php
eval('$a = [3, 1, 2];
echo sort($a) . ":" . $a[0] . $a[1] . $a[2] . ":";
$b = ["banana", "apple", "cherry"];
echo rsort(array: $b) . ":" . $b[0] . ":" . $b[2] . ":";
$c = ["x" => 3, "y" => 1, "z" => 2];
sort($c);
echo $c[0] . $c[1] . $c[2] . ":";
$d = [3, 1, 2];
echo call_user_func("sort", $d) . ":" . $d[0] . $d[1] . $d[2] . ":";
$e = [1, 2, 3];
echo call_user_func_array("rsort", ["array" => $e]) . ":" . $e[0] . ":" . $e[2] . ":";
echo function_exists("sort") && function_exists("rsort");');
"#,
    );
    assert_eq!(out, "1:123:1:cherry:apple:123:1:312:1:1:3:1");
}

/// Verifies eval key-preserving sort builtins mutate direct variable arguments only.
#[test]
fn test_eval_dispatches_key_preserving_sort_builtin_calls() {
    let out = compile_and_run(
        r#"<?php
eval('$a = ["x" => 3, "y" => 1, "z" => 2];
echo asort($a) . ":";
foreach ($a as $key => $value) { echo $key . $value; }
echo ":";
$b = ["x" => 1, "y" => 3, "z" => 2];
echo arsort(array: $b) . ":";
foreach ($b as $key => $value) { echo $key . $value; }
echo ":";
$c = ["b" => 1, "a" => 2, 3 => 4];
echo ksort($c) . ":";
foreach ($c as $key => $value) { echo $key . $value; }
echo ":";
$d = ["b" => 1, "a" => 2, 3 => 4];
echo krsort($d) . ":";
foreach ($d as $key => $value) { echo $key . $value; }
echo ":";
$e = ["x" => 2, "y" => 1];
echo call_user_func("asort", $e) . ":" . $e["x"] . $e["y"] . ":";
$f = ["b" => 1, "a" => 2];
echo call_user_func_array("krsort", ["array" => $f]) . ":" . $f["b"] . $f["a"] . ":";
echo function_exists("asort") && function_exists("arsort") && function_exists("ksort") && function_exists("krsort");');
"#,
    );
    assert_eq!(
        out,
        "1:y1z2x3:1:y3z2x1:1:34a2b1:1:b1a234:1:21:1:12:1"
    );
}

/// Verifies eval natural sort builtins preserve keys and use natural string order.
#[test]
fn test_eval_dispatches_natural_sort_builtin_calls() {
    let out = compile_and_run(
        r#"<?php
eval('$a = ["img10", "img2", "img1"];
echo natsort($a) . ":";
foreach ($a as $key => $value) { echo $key . $value . ";"; }
echo ":";
$b = ["b" => "Img10", "a" => "img2", "c" => "IMG1"];
echo natcasesort(array: $b) . ":";
foreach ($b as $key => $value) { echo $key . $value . ";"; }
echo ":";
$c = ["x" => "b", "y" => "a"];
echo call_user_func("natsort", $c) . ":" . $c["x"] . $c["y"] . ":";
echo function_exists("natsort") && function_exists("natcasesort");');
"#,
    );
    assert_eq!(
        out,
        "1:2img1;1img2;0img10;:1:cIMG1;aimg2;bImg10;:1:ba:1"
    );
}

/// Verifies eval `shuffle()` reindexes direct variable arrays only.
#[test]
fn test_eval_dispatches_shuffle_builtin_call() {
    let out = compile_and_run(
        r#"<?php
eval('$a = ["x" => 1, "y" => 2];
echo shuffle($a) . ":" . (isset($a["x"]) ? "bad" : "reindexed") . ":" . count($a) . ":" . array_sum($a) . ":";
$b = ["x" => 1, "y" => 2];
echo call_user_func("shuffle", $b) . ":" . $b["x"] . $b["y"] . ":";
echo function_exists("shuffle");');
"#,
    );
    assert_eq!(out, "1:reindexed:2:3:1:12:1");
}

/// Verifies eval user-comparator sort builtins call callbacks and mutate direct arrays.
#[test]
fn test_eval_dispatches_user_sort_builtin_calls() {
    let out = compile_and_run(
        r#"<?php
eval('function eval_sort_cmp($left, $right) { echo "c"; return $left <=> $right; }
function eval_key_cmp($left, $right) { return strcmp($left, $right); }
$a = [3, 1, 2];
echo usort($a, "eval_sort_cmp") . ":";
foreach ($a as $value) { echo $value; }
echo ":";
$b = ["b" => 1, "a" => 3, "c" => 2];
echo uasort(array: $b, callback: "eval_sort_cmp") . ":";
foreach ($b as $key => $value) { echo $key . $value; }
echo ":";
$c = ["b" => 1, "a" => 2];
echo uksort($c, "eval_key_cmp") . ":";
foreach ($c as $key => $value) { echo $key . $value; }
echo ":";
$d = [2, 1];
echo call_user_func("usort", $d, "eval_sort_cmp") . ":" . $d[0] . $d[1] . ":";
echo function_exists("usort") && function_exists("uasort") && function_exists("uksort");');
"#,
    );
    assert_eq!(out, "ccc1:123:ccc1:b1c2a3:1:a2b1:c1:21:1");
}

/// Verifies eval iterator array helpers dispatch through direct and dynamic calls.
#[test]
fn test_eval_dispatches_iterator_array_builtin_calls() {
    let out = compile_and_run(
        r#"<?php
eval('$items = ["x" => 1, "y" => 2];
$copy = iterator_to_array($items);
echo iterator_count($items) . ":" . $copy["x"] . $copy["y"] . ":";
$values = iterator_to_array($items, false);
echo (isset($values["x"]) ? "bad" : "reindexed") . ":" . $values[0] . $values[1] . ":";
echo call_user_func("iterator_count", $items) . ":";
$spread = call_user_func_array("iterator_to_array", ["iterator" => $items, "preserve_keys" => false]);
echo $spread[0] . $spread[1] . ":";
echo function_exists("iterator_count") && function_exists("iterator_to_array");');
"#,
    );
    assert_eq!(out, "2:12:reindexed:12:2:12:1");
}

/// Verifies eval `iterator_apply()` drives AOT Iterator objects through eval callbacks.
#[test]
fn test_eval_dispatches_iterator_apply_object_builtin() {
    let out = compile_and_run(
        r#"<?php
class EvalApplyRange implements Iterator {
    private int $i;
    private int $end;
    public function __construct(int $end) { $this->i = 0; $this->end = $end; }
    public function rewind(): void { $this->i = 0; }
    public function valid(): bool { return $this->i < $this->end; }
    public function current(): int { return $this->i; }
    public function key(): int { return $this->i; }
    public function next(): void { $this->i = $this->i + 1; }
}
eval('function eval_apply_label($prefix) { echo $prefix; return true; }
$r = new EvalApplyRange(2);
echo iterator_apply($r, "eval_apply_label", ["prefix" => "E"]) . ":";
echo call_user_func("iterator_apply", $r, "eval_apply_label", ["C"]);');
"#,
    );
    assert_eq!(out, "EE2:CC2");
}

/// Verifies eval `array_filter()` removes falsey values and preserves source keys.
#[test]
fn test_eval_dispatches_array_filter_builtin_call() {
    let out = compile_and_run(
        r#"<?php
eval('$filtered = array_filter([0, 1, 2, "", false, null, "0", "ok"]);
echo count($filtered) . ":" . $filtered[1] . ":" . $filtered[2] . ":" . $filtered[7] . ":";
$assoc = array_filter(["a" => 0, "b" => 2, "c" => ""]);
echo (array_key_exists("a", $assoc) ? "bad" : "drop") . ":" . $assoc["b"] . ":";
$null = array_filter([0, 3], null, 1);
echo count($null) . ":" . $null[1] . ":";
$call = call_user_func("array_filter", [0, 4]);
echo count($call) . ":" . $call[1] . ":";
$spread = call_user_func_array("array_filter", ["array" => [0, 5], "callback" => null]);
echo count($spread) . ":" . $spread[1] . ":";
function eval_keep_even($value) { return $value % 2 == 0; }
$evens = array_filter([1, 2, 3, 4], "eval_keep_even");
echo count($evens) . ":" . $evens[1] . ":" . $evens[3] . ":";
function eval_keep_key($key) { return $key === "b"; }
$keyed = array_filter(["a" => 10, "b" => 20], "eval_keep_key", ARRAY_FILTER_USE_KEY);
echo count($keyed) . ":" . $keyed["b"] . ":";
function eval_keep_both($value, $key) { return $key === "c" || $value === 1; }
$both = array_filter(["a" => 1, "b" => 2, "c" => 3], "eval_keep_both", ARRAY_FILTER_USE_BOTH);
echo count($both) . ":" . $both["a"] . ":" . $both["c"] . ":";
$ints = array_filter([1, "x", 2], "is_int");
echo count($ints) . ":" . $ints[0] . ":" . $ints[2] . ":";
echo function_exists("array_filter");');
"#,
    );
    assert_eq!(
        out,
        "3:1:2:ok:drop:2:1:3:1:4:1:5:2:2:4:1:20:2:1:3:2:1:2:1"
    );
}

/// Verifies eval `array_combine()` supports PHP key conversions and callable dispatch.
#[test]
fn test_eval_dispatches_array_combine_builtin_call() {
    let out = compile_and_run(
        r#"<?php
eval('$pairs = array_combine(["a", "b"], [10, 20]);
echo $pairs["a"] . ":" . $pairs["b"];
$numeric = array_combine(["1", "01"], ["n", "z"]);
echo ":" . $numeric[1] . $numeric["01"];
$scalar = array_combine([null, true, false, 2.8], ["n", "t", "f", "d"]);
echo ":" . $scalar[""] . $scalar[1] . $scalar["2.8"];
$named = array_combine(keys: ["k"], values: ["v"]);
echo ":" . $named["k"];
$call = call_user_func("array_combine", ["x"], [7]);
echo ":" . $call["x"];
$spread = call_user_func_array("array_combine", [["y"], [8]]);
echo ":" . $spread["y"] . ":";
echo function_exists("array_combine");');
"#,
    );
    assert_eq!(out, "10:20:nz:ftd:v:7:8:1");
}

/// Verifies eval `array_column()` extracts present row columns and reindexes them.
#[test]
fn test_eval_dispatches_array_column_builtin_call() {
    let out = compile_and_run(
        r#"<?php
eval('$rows = [["name" => "Ada", "score" => 10], ["score" => 20], ["name" => "Lin", "score" => 30], 42];
$names = array_column($rows, "name");
echo count($names) . ":" . $names[0] . ":" . $names[1];
$scores = array_column($rows, "score");
echo ":" . count($scores) . ":" . $scores[0] . $scores[2];
$numeric = array_column([[0 => "zero", 1 => "one"], [1 => "uno"]], 1);
echo ":" . count($numeric) . ":" . $numeric[0] . ":" . $numeric[1];
$named = array_column(array: $rows, column_key: "score");
echo ":" . $named[1];
$call = call_user_func("array_column", [["x" => 5], ["x" => 6]], "x");
echo ":" . $call[1];
$spread = call_user_func_array("array_column", [[["y" => 7], ["z" => 0], ["y" => 9]], "y"]);
echo ":" . count($spread) . ":" . $spread[1] . ":";
echo function_exists("array_column");');
"#,
    );
    assert_eq!(out, "2:Ada:Lin:3:1030:2:one:uno:20:6:2:9:1");
}

/// Verifies eval `array_pad()` and `array_chunk()` build reindexed array shapes.
#[test]
fn test_eval_dispatches_array_shape_builtin_calls() {
    let out = compile_and_run(
        r#"<?php
eval('$right = array_pad([1, 2], 5, 0);
echo count($right) . ":" . $right[0] . $right[1] . $right[2] . $right[4];
$left = array_pad([1, 2], -4, 9);
echo ":" . $left[0] . $left[1] . $left[2] . $left[3];
$copy = array_pad([7, 8], 1, 0);
echo ":" . count($copy) . ":" . $copy[0] . $copy[1];
$chunks = array_chunk([1, 2, 3, 4, 5], 2);
echo ":" . count($chunks) . ":" . $chunks[0][1] . $chunks[2][0];
$named = array_pad(array: ["a"], length: 2, value: "b");
echo ":" . $named[1];
$call = call_user_func("array_chunk", [6, 7, 8], 2);
echo ":" . $call[1][0];
$spread = call_user_func_array("array_pad", [[1], 3, 2]);
echo ":" . $spread[2] . ":";
echo function_exists("array_pad"); echo function_exists("array_chunk");');
"#,
    );
    assert_eq!(out, "5:1200:9912:2:78:3:25:b:8:2:11");
}

/// Verifies eval `array_slice()` observes PHP offset and length bounds.
#[test]
fn test_eval_dispatches_array_slice_builtin_call() {
    let out = compile_and_run(
        r#"<?php
eval('$mid = array_slice([10, 20, 30, 40, 50], 1, 3);
echo count($mid) . ":" . $mid[0] . $mid[1] . $mid[2];
$tail = array_slice([10, 20, 30, 40], -2, 1);
echo ":" . $tail[0];
$open = array_slice([10, 20, 30, 40, 50], 2);
echo ":" . count($open) . ":" . $open[0] . $open[2];
$null_len = array_slice([5, 6, 7], 1, null);
echo ":" . $null_len[0] . $null_len[1];
$negative_len = array_slice([10, 20, 30, 40, 50], 1, -1);
echo ":" . count($negative_len) . ":" . $negative_len[0] . $negative_len[2];
$named = array_slice(array: [1, 2, 3], offset: 1, length: 1);
echo ":" . $named[0];
$call = call_user_func("array_slice", [6, 7, 8], 1, 2);
echo ":" . $call[1];
$spread = call_user_func_array("array_slice", [[9, 10, 11], 1]);
echo ":" . $spread[0] . ":";
echo function_exists("array_slice");');
"#,
    );
    assert_eq!(out, "3:203040:30:3:3050:67:3:2040:2:8:10:1");
}

/// Verifies eval `array_merge()` appends numeric keys and overwrites string keys.
#[test]
fn test_eval_dispatches_array_merge_builtin_call() {
    let out = compile_and_run(
        r#"<?php
eval('$merged = array_merge([1, 2], [3, 4]);
echo count($merged) . ":" . $merged[0] . $merged[1] . $merged[2] . $merged[3];
$left = [1, 2];
$right = [3];
$copy = array_merge($left, $right);
echo ":" . count($left) . ":" . $left[0] . ":" . $copy[2];
$assoc = array_merge(["a" => 1, 2 => "x"], ["a" => 9, 5 => "y", "b" => 3]);
echo ":" . $assoc["a"] . ":" . $assoc[0] . ":" . $assoc[1] . ":" . $assoc["b"];
$call = call_user_func("array_merge", [6], [7, 8]);
echo ":" . $call[2];
$spread = call_user_func_array("array_merge", [[9], [10]]);
echo ":" . $spread[1] . ":";
echo function_exists("array_merge");');
"#,
    );
    assert_eq!(out, "4:1234:2:1:3:9:x:y:3:8:10:1");
}

/// Verifies eval `array_diff()` and `array_intersect()` preserve left keys and compare by string value.
#[test]
fn test_eval_dispatches_array_value_set_builtin_calls() {
    let out = compile_and_run(
        r#"<?php
eval('$diff = array_diff(["a" => 1, "b" => 2, "c" => "2", "d" => 3], [2]);
echo count($diff) . ":" . $diff["a"] . ":" . $diff["d"];
echo ":" . (array_key_exists("b", $diff) ? "bad" : "no-b");
echo ":" . (array_key_exists("c", $diff) ? "bad" : "no-c");
$inter = array_intersect(["a" => 1, "b" => 2, "c" => "2", "d" => 3], ["2", 4]);
echo ":" . count($inter) . ":" . $inter["b"] . ":" . $inter["c"];
$call = call_user_func("array_diff", [1, 2, 3], [2]);
echo ":" . count($call) . ":" . $call[0] . $call[2];
$spread = call_user_func_array("array_intersect", [[1, 2, 3], [3]]);
echo ":" . count($spread) . ":" . $spread[2] . ":";
echo function_exists("array_diff"); echo function_exists("array_intersect");');
"#,
    );
    assert_eq!(out, "2:1:3:no-b:no-c:2:2:2:2:13:1:3:11");
}

/// Verifies eval `array_diff_key()` and `array_intersect_key()` preserve first-array keys.
#[test]
fn test_eval_dispatches_array_key_set_builtin_calls() {
    let out = compile_and_run(
        r#"<?php
eval('$diff = array_diff_key(["a" => 1, "b" => 2, 4 => 3], ["a" => 0, 5 => 0]);
echo count($diff) . ":" . $diff["b"] . ":" . $diff[4];
echo ":" . (array_key_exists("a", $diff) ? "bad" : "no-a");
$inter = array_intersect_key(["a" => 1, "b" => 2, 4 => 3], ["b" => 0, 4 => 0]);
echo ":" . count($inter) . ":" . $inter["b"] . ":" . $inter[4];
$call = call_user_func("array_diff_key", [10, 20, 30], [1 => 0]);
echo ":" . count($call) . ":" . $call[0] . $call[2];
$spread = call_user_func_array("array_intersect_key", [["x" => 7, "y" => 8], ["y" => 0]]);
echo ":" . count($spread) . ":" . $spread["y"] . ":";
echo function_exists("array_diff_key"); echo function_exists("array_intersect_key");');
"#,
    );
    assert_eq!(out, "2:2:3:no-a:2:2:3:2:1030:1:8:11");
}

/// Verifies eval `range()` builds inclusive ascending and descending integer arrays.
#[test]
fn test_eval_dispatches_range_builtin_call() {
    let out = compile_and_run(
        r#"<?php
eval('$up = range(1, 4);
echo count($up) . ":" . $up[0] . $up[3];
$down = range(4, 1);
echo ":" . count($down) . ":" . $down[0] . $down[3];
$single = range(3, 3);
echo ":" . count($single) . ":" . $single[0];
$named = range(start: 2, end: 4);
echo ":" . $named[0] . $named[2];
$call = call_user_func("range", 5, 7);
echo ":" . $call[2];
$spread = call_user_func_array("range", [8, 6]);
echo ":" . count($spread) . ":" . $spread[0] . $spread[2] . ":";
echo function_exists("range");');
"#,
    );
    assert_eq!(out, "4:14:4:41:1:3:24:7:3:86:1");
}

/// Verifies eval `array_rand()` returns a key that exists in the source array.
#[test]
fn test_eval_dispatches_array_rand_builtin_call() {
    let out = compile_and_run(
        r#"<?php
eval('$nums = [10, 20, 30];
$idx = array_rand($nums);
echo ($idx >= 0 && $idx < 3 && array_key_exists($idx, $nums)) ? "idx" : "bad";
$assoc = ["a" => 1, "b" => 2];
$key = array_rand($assoc);
echo ":" . (array_key_exists($key, $assoc) ? "assoc" : "bad");
$named = array_rand(array: [5, 6]);
echo ":" . (($named >= 0 && $named < 2) ? "named" : "bad");
$call = call_user_func("array_rand", [7, 8]);
echo ":" . (($call >= 0 && $call < 2) ? "call" : "bad");
$spread = call_user_func_array("array_rand", [["x" => 1, "y" => 2]]);
echo ":" . (array_key_exists($spread, ["x" => 1, "y" => 2]) ? "spread" : "bad") . ":";
echo function_exists("array_rand");');
"#,
    );
    assert_eq!(out, "idx:assoc:named:call:spread:1");
}

/// Verifies eval random builtins produce values in their PHP-visible ranges.
#[test]
fn test_eval_dispatches_rand_builtin_calls() {
    let out = compile_and_run(
        r#"<?php
eval('$plain = rand();
echo ($plain >= 0 && $plain <= 2147483647) ? "plain" : "bad";
$bounded = rand(2, 4);
echo ":" . (($bounded >= 2 && $bounded <= 4) ? "range" : "bad");
$same = mt_rand(max: 6, min: 6);
echo ":" . ($same === 6 ? "same" : "bad");
$swapped = rand(10, 1);
echo ":" . (($swapped >= 1 && $swapped <= 10) ? "swap" : "bad");
$call = call_user_func("mt_rand", 1, 1);
echo ":" . ($call === 1 ? "call" : "bad");
$spread = call_user_func_array("rand", ["min" => 3, "max" => 3]);
echo ":" . ($spread === 3 ? "spread" : "bad") . ":";
$secure = random_int(max: 4, min: 4);
echo ($secure === 4 ? "random" : "bad") . ":";
$secureCall = call_user_func("random_int", 5, 5);
echo ($secureCall === 5 ? "random-call" : "bad") . ":";
$secureSpread = call_user_func_array("random_int", ["min" => 6, "max" => 6]);
echo ($secureSpread === 6 ? "random-spread" : "bad") . ":";
echo function_exists("rand"); echo function_exists("mt_rand"); echo function_exists("random_int");');
"#,
    );
    assert_eq!(
        out,
        "plain:range:same:swap:call:spread:random:random-call:random-spread:111"
    );
}

/// Verifies eval `spl_classes()` exposes the same static SPL class list as native code.
#[test]
fn test_eval_dispatches_spl_classes_builtin_calls() {
    let out = compile_and_run(
        r#"<?php
eval('$names = spl_classes();
echo count($names) . ":" . $names[0] . ":" . $names[55] . ":";
echo (in_array("Exception", $names) ? "exception" : "bad") . ":";
echo (in_array("SplDoublyLinkedList", $names) ? "list" : "bad") . ":";
$call = call_user_func("spl_classes");
echo (in_array("Throwable", $call) ? "call" : "bad") . ":";
$spread = call_user_func_array("spl_classes", []);
echo (count($spread) === count($names) ? "spread" : "bad") . ":";
echo function_exists("spl_classes"); echo is_callable("spl_classes");');
"#,
    );
    assert_eq!(
        out,
        "61:AppendIterator:Throwable:exception:list:call:spread:11"
    );
}

/// Verifies eval `array_fill()` and `array_fill_keys()` create arrays with PHP key rules.
#[test]
fn test_eval_dispatches_array_fill_builtin_calls() {
    let out = compile_and_run(
        r#"<?php
eval('$filled = array_fill(2, 3, "x");
echo count($filled) . ":" . $filled[2] . $filled[4];
$negative = array_fill(-2, 3, 7);
echo ":" . $negative[-2] . $negative[-1] . $negative[0];
$empty = array_fill(5, 0, "x");
echo ":" . count($empty);
$map = array_fill_keys(["a", "1", "01"], 8);
echo ":" . $map["a"] . ":" . $map[1] . ":" . $map["01"];
$named = array_fill(start_index: 1, count: 2, value: "n");
echo ":" . $named[1] . $named[2];
$call = call_user_func("array_fill", 0, 2, "c");
echo ":" . $call[0] . $call[1];
$spread = call_user_func_array("array_fill_keys", [["x", "y"], "z"]);
echo ":" . $spread["x"] . $spread["y"] . ":";
echo function_exists("array_fill"); echo function_exists("array_fill_keys");');
"#,
    );
    assert_eq!(out, "3:xx:777:0:8:8:8:nn:cc:zz:11");
}

/// Verifies eval `array_flip()` supports PHP key rules and callable dispatch.
#[test]
fn test_eval_dispatches_array_flip_builtin_call() {
    let out = compile_and_run(
        r#"<?php
eval('$flipped = array_flip(["a" => "x", "b" => "y", "c" => "x", "d" => 1, "e" => "01", "skip" => null, "truth" => true]);
echo $flipped["x"] . ":" . $flipped["y"] . ":" . $flipped[1] . ":" . $flipped["01"] . ":" . count($flipped);
$named = array_flip(array: ["k" => "v"]);
echo ":" . $named["v"];
$call = call_user_func("array_flip", ["left" => "right"]);
echo ":" . $call["right"];
$spread = call_user_func_array("array_flip", [["n" => 9]]);
echo ":" . $spread[9] . ":";
echo function_exists("array_flip");');
"#,
    );
    assert_eq!(out, "c:b:d:e:4:k:left:n:1");
}

/// Verifies eval `array_unique()` preserves keys and supports callable dispatch.
#[test]
fn test_eval_dispatches_array_unique_builtin_call() {
    let out = compile_and_run(
        r#"<?php
eval('$unique = array_unique(["a", "b", "a", "2", 2]);
echo count($unique) . ":" . $unique[0] . $unique[1] . $unique[3];
$assoc = array_unique(["x" => "a", "y" => "b", "z" => "a"]);
echo ":" . count($assoc) . ":" . $assoc["x"] . $assoc["y"];
$scalar = array_unique([1, "1", 1.0, true, false, null, ""]);
echo ":" . count($scalar) . ":" . $scalar[0] . ":";
echo $scalar[4] ? "bad" : "F";
$named = array_unique(array: ["k" => "v", "l" => "v"]);
echo ":" . $named["k"] . ":" . count($named);
$call = call_user_func("array_unique", ["q", "q", "r"]);
echo ":" . $call[0] . $call[2];
$spread = call_user_func_array("array_unique", [["s", "s", "t"]]);
echo ":" . $spread[0] . $spread[2] . ":";
echo function_exists("array_unique");');
"#,
    );
    assert_eq!(out, "3:ab2:2:ab:2:1:F:v:1:qr:st:1");
}

/// Verifies eval array projection builtins return indexed key/value arrays.
#[test]
fn test_eval_dispatches_array_projection_builtin_calls() {
    let out = compile_and_run(
        r#"<?php
eval('$values = array_values(["a" => 10, "b" => 20]);
echo $values[0] . ":" . $values[1];
$keys = array_keys(["a" => 10, "b" => 20]);
echo ":" . $keys[0] . ":" . $keys[1];
echo ":" . count(array_values([]));
$call_keys = call_user_func("array_keys", ["z" => 7]);
echo ":" . $call_keys[0];
$call_values = call_user_func_array("array_values", [["q" => 8]]);
echo ":" . $call_values[0];
echo ":"; echo function_exists("array_keys"); echo function_exists("array_values");');
"#,
    );
    assert_eq!(out, "10:20:a:b:0:z:8:11");
}

/// Verifies eval `array_reverse()` supports key rules, named args, and callable dispatch.
#[test]
fn test_eval_dispatches_array_reverse_builtin_call() {
    let out = compile_and_run(
        r#"<?php
eval('$indexed = array_reverse([1, 2, 3]);
echo $indexed[0]; echo $indexed[1]; echo $indexed[2]; echo ":";
$mixed = array_reverse([2 => "a", "k" => "b", 5 => "c"]);
echo $mixed[0]; echo $mixed["k"]; echo $mixed[1]; echo ":";
$preserved = array_reverse([2 => "a", "k" => "b", 5 => "c"], true);
echo $preserved[5]; echo $preserved["k"]; echo $preserved[2]; echo ":";
$named = array_reverse(array: ["x", "y"], preserve_keys: true);
echo $named[1]; echo $named[0]; echo ":";
$call = call_user_func("array_reverse", [4, 5]);
echo $call[0]; echo $call[1]; echo ":";
$spread = call_user_func_array("array_reverse", [[6, 7]]);
echo $spread[0]; echo $spread[1]; echo ":";
echo function_exists("array_reverse");');
"#,
    );
    assert_eq!(out, "321:cba:cba:yx:54:76:1");
}

/// Verifies eval `array_key_exists()` distinguishes present null values from missing keys.
#[test]
fn test_eval_dispatches_array_key_exists_builtin_calls() {
    let out = compile_and_run(
        r#"<?php
eval('$map = ["name" => null, "age" => 30];
echo array_key_exists("name", $map) ? "Y" : "N"; echo ":";
echo array_key_exists("missing", $map) ? "bad" : "N"; echo ":";
echo array_key_exists(1, [10, null]) ? "Y" : "N"; echo ":";
echo array_key_exists(2, [10, null]) ? "bad" : "N"; echo ":";
echo call_user_func("array_key_exists", "age", $map) ? "Y" : "N"; echo ":";
echo call_user_func_array("array_key_exists", ["age", $map]) ? "Y" : "N";
echo ":"; echo function_exists("array_key_exists");');
"#,
    );
    assert_eq!(out, "Y:N:Y:N:Y:Y:1");
}

/// Verifies eval array search builtins return booleans or matching keys.
#[test]
fn test_eval_dispatches_array_search_builtin_calls() {
    let out = compile_and_run(
        r#"<?php
eval('echo in_array(2, [1, 2, 3]) ? "Y" : "bad";
echo ":"; echo in_array(4, [1, 2, 3]) ? "bad" : "N";
echo ":" . array_search(20, [10, 20, 30]);
echo ":" . array_search("Grace", ["name" => "Grace"]);
echo ":"; echo array_search("x", ["name" => "Grace"]) === false ? "F" : "bad";
echo ":"; echo call_user_func("in_array", "b", ["a", "b"]) ? "C" : "bad";
$found = call_user_func_array("array_search", ["v", ["k" => "v"]]);
echo ":" . $found;
echo ":"; echo function_exists("in_array"); echo function_exists("array_search");');
"#,
    );
    assert_eq!(out, "Y:N:1:name:F:C:k:11");
}

/// Verifies eval ASCII case-conversion builtins work directly and by callable dispatch.
#[test]
fn test_eval_dispatches_string_case_builtin_calls() {
    let out = compile_and_run(
        r#"<?php
eval('echo strtoupper("Hello World"); echo ":";
echo strtolower("LOUD"); echo ":";
echo ucfirst("eval"); echo ":";
echo lcfirst("LOUD"); echo ":";
echo call_user_func("strtoupper", "xy"); echo ":";
echo call_user_func_array("strtolower", ["ZZ"]); echo ":";
echo call_user_func("ucfirst", "case"); echo ":";
echo call_user_func_array("lcfirst", ["CASE"]);
echo ":"; echo function_exists("strtoupper"); echo function_exists("strtolower"); echo function_exists("ucfirst"); echo function_exists("lcfirst");');
"#,
    );
    assert_eq!(out, "HELLO WORLD:loud:Eval:lOUD:XY:zz:Case:cASE:1111");
}

/// Verifies eval `ucwords()` capitalizes words directly and by callable dispatch.
#[test]
fn test_eval_dispatches_ucwords_builtin_call() {
    let out = compile_and_run(
        r#"<?php
eval('echo ucwords("hello world"); echo ":";
echo ucwords(string: "hello-world", separators: "-"); echo ":";
echo ucwords("hello\tworld"); echo ":";
echo call_user_func("ucwords", "a b"); echo ":";
echo call_user_func_array("ucwords", ["string" => "a-b", "separators" => "-"]);
echo ":"; echo function_exists("ucwords");');
"#,
    );
    assert_eq!(out, "Hello World:Hello-World:Hello\tWorld:A B:A-B:1");
}

/// Verifies eval `wordwrap()` wraps at word boundaries and can cut long words.
#[test]
fn test_eval_dispatches_wordwrap_builtin_call() {
    let out = compile_and_run(
        r#"<?php
eval('echo wordwrap("The quick brown fox", 10, "|"); echo ":";
echo wordwrap(string: "A verylongword here", width: 8, break: "|"); echo ":";
echo wordwrap("abcdefghij", 4, "|", true); echo ":";
echo wordwrap("preserve\nnewlines here ok", 10, "|"); echo ":";
echo call_user_func("wordwrap", "aaa bbb ccc", 3, "<br>"); echo ":";
echo call_user_func_array("wordwrap", ["string" => "hello world", "width" => 5, "break" => "|"]);
echo ":"; echo function_exists("wordwrap");');
"#,
    );
    assert_eq!(
        out,
        "The quick|brown fox:A|verylongword|here:abcd|efgh|ij:preserve\nnewlines|here ok:aaa<br>bbb<br>ccc:hello|world:1"
    );
}

/// Verifies eval `strrev()` reverses byte strings directly and by callable dispatch.
#[test]
fn test_eval_dispatches_strrev_builtin_call() {
    let out = compile_and_run(
        r#"<?php
eval('echo strrev("Hello"); echo ":";
echo strrev(123); echo ":";
echo call_user_func("strrev", "ABC"); echo ":";
echo call_user_func_array("strrev", ["def"]);
echo ":"; echo function_exists("strrev");');
"#,
    );
    assert_eq!(out, "olleH:321:CBA:fed:1");
}

/// Verifies eval `chr()` returns single-byte strings directly and by callable dispatch.
#[test]
fn test_eval_dispatches_chr_builtin_call() {
    let out = compile_and_run(
        r#"<?php
eval('echo chr(65); echo ":";
echo bin2hex(chr(codepoint: 256)); echo ":";
echo bin2hex(call_user_func("chr", -1)); echo ":";
echo call_user_func_array("chr", ["codepoint" => 321]);
echo ":"; echo function_exists("chr");');
"#,
    );
    assert_eq!(out, "A:00:ff:A:1");
}

/// Verifies eval `str_repeat()` repeats byte strings directly and by callable dispatch.
#[test]
fn test_eval_dispatches_str_repeat_builtin_call() {
    let out = compile_and_run(
        r#"<?php
eval('echo str_repeat("ha", 3); echo ":";
echo strlen(str_repeat(string: "x", times: 0)); echo ":";
echo call_user_func("str_repeat", "ab", 2); echo ":";
echo call_user_func_array("str_repeat", ["string" => "z", "times" => 3]);
echo ":"; echo function_exists("str_repeat");');
"#,
    );
    assert_eq!(out, "hahaha:0:abab:zzz:1");
}

/// Verifies eval `substr()` slices byte strings directly and by callable dispatch.
#[test]
fn test_eval_dispatches_substr_builtin_call() {
    let out = compile_and_run(
        r#"<?php
eval('echo substr("abcdef", 2); echo ":";
echo substr(string: "abcdef", offset: 1, length: -1); echo ":";
echo substr("abcdef", -2); echo ":";
echo call_user_func("substr", "abcdef", 2, -2); echo ":";
echo call_user_func_array("substr", ["string" => "abcdef", "offset" => -4, "length" => 2]);
echo ":"; echo function_exists("substr");');
"#,
    );
    assert_eq!(out, "cdef:bcde:ef:cd:cd:1");
}

/// Verifies eval `substr_replace()` replaces selected byte ranges through callable paths.
#[test]
fn test_eval_dispatches_substr_replace_builtin_call() {
    let out = compile_and_run(
        r#"<?php
eval('echo substr_replace("hello world", "PHP", 6, 5); echo ":";
echo substr_replace(string: "abcdef", replace: "X", offset: 1, length: -1); echo ":";
echo substr_replace("abcdef", "X", -2); echo ":";
echo call_user_func("substr_replace", "abcdef", "X", 99, 1); echo ":";
echo call_user_func_array("substr_replace", ["string" => "abcdef", "replace" => "X", "offset" => -99, "length" => 2]);
echo ":"; echo function_exists("substr_replace");');
"#,
    );
    assert_eq!(out, "hello PHP:aXf:abcdX:abcdefX:Xcdef:1");
}

/// Verifies eval `nl2br()` preserves newline bytes while inserting HTML breaks.
#[test]
fn test_eval_dispatches_nl2br_builtin_call() {
    let out = compile_and_run(
        r#"<?php
eval('echo bin2hex(nl2br("a\nb")); echo ":";
echo bin2hex(nl2br(string: "a\nb", use_xhtml: false)); echo ":";
echo bin2hex(call_user_func("nl2br", "a\r\nb")); echo ":";
echo bin2hex(call_user_func_array("nl2br", ["string" => "a\n\rb", "use_xhtml" => false]));
echo ":"; echo function_exists("nl2br");');
"#,
    );
    assert_eq!(
        out,
        "613c6272202f3e0a62:613c62723e0a62:613c6272202f3e0d0a62:613c62723e0a0d62:1"
    );
}

/// Verifies eval `explode()` and `implode()` bridge byte strings and arrays.
#[test]
fn test_eval_dispatches_explode_implode_builtin_calls() {
    let out = compile_and_run(
        r#"<?php
eval('$parts = explode(",", "a,b,");
echo count($parts); echo ":" . $parts[0] . ":" . $parts[1] . ":" . $parts[2];
echo ":" . implode("|", $parts);
echo ":" . implode(separator: "-", array: ["x", 2, true, null]);
$call_parts = call_user_func("explode", ":", "m:n");
echo ":" . $call_parts[1];
echo ":" . call_user_func_array("implode", ["separator" => "/", "array" => ["p", "q"]]);
echo ":"; echo function_exists("explode");
echo ":"; echo function_exists("implode");');
"#,
    );
    assert_eq!(out, "3:a:b::a|b|:x-2-1-:n:p/q:1:1");
}

/// Verifies eval `str_split()` builds indexed chunk arrays.
#[test]
fn test_eval_dispatches_str_split_builtin_call() {
    let out = compile_and_run(
        r#"<?php
eval('$letters = str_split("abc");
echo count($letters) . ":" . $letters[0] . $letters[1] . $letters[2]; echo ":";
$pairs = str_split(string: "abcd", length: 2);
echo $pairs[0] . "-" . $pairs[1]; echo ":";
$empty = str_split("");
echo count($empty); echo ":";
$call = call_user_func("str_split", "xyz", 2);
echo $call[0] . "-" . $call[1]; echo ":";
$named = call_user_func_array("str_split", ["string" => "pqrs", "length" => 3]);
echo $named[0] . "-" . $named[1];
echo ":"; echo function_exists("str_split");');
"#,
    );
    assert_eq!(out, "3:abc:ab-cd:0:xy-z:pqr-s:1");
}

/// Verifies eval `str_pad()` supports all PHP pad modes and callable dispatch.
#[test]
fn test_eval_dispatches_str_pad_builtin_call() {
    let out = compile_and_run(
        r#"<?php
eval('echo "[" . str_pad("hi", 5) . "]"; echo ":";
echo "[" . str_pad(string: "hi", length: 5, pad_string: "_", pad_type: 0) . "]"; echo ":";
echo "[" . str_pad("x", 6, "ab", 2) . "]"; echo ":";
echo call_user_func("str_pad", "42", 5, "0", 0); echo ":";
echo call_user_func_array("str_pad", ["string" => "x", "length" => 3, "pad_string" => "."]);
echo ":"; echo function_exists("str_pad");');
"#,
    );
    assert_eq!(out, "[hi   ]:[___hi]:[abxaba]:00042:x..:1");
}

/// Verifies eval string replacement builtins support direct and callable dispatch.
#[test]
fn test_eval_dispatches_string_replace_builtin_calls() {
    let out = compile_and_run(
        r#"<?php
eval('echo str_replace("o", "0", "Hello World"); echo ":";
echo str_replace(search: "aa", replace: "b", subject: "aaaa"); echo ":";
echo str_replace("", "x", "abc"); echo ":";
echo str_ireplace("HE", "ye", "Hello he"); echo ":";
echo call_user_func("str_replace", "l", "L", "hello"); echo ":";
echo call_user_func_array("str_ireplace", ["search" => "x", "replace" => "Y", "subject" => "xX"]);
echo ":"; echo function_exists("str_replace");
echo ":"; echo function_exists("str_ireplace");');
"#,
    );
    assert_eq!(out, "Hell0 W0rld:bb:abc:yello ye:heLLo:YY:1:1");
}

/// Verifies eval HTML entity builtins encode, decode, and dispatch as callables.
#[test]
fn test_eval_dispatches_html_entity_builtin_calls() {
    let out = compile_and_run(
        r#"<?php
eval('echo htmlspecialchars("<b>\"Hi\" & \'bye\'</b>"); echo ":";
echo htmlentities(string: "<a>"); echo ":";
echo html_entity_decode("&lt;b&gt;hi&lt;/b&gt;"); echo ":";
echo call_user_func("htmlspecialchars", "<x>"); echo ":";
echo call_user_func_array("html_entity_decode", ["string" => "&quot;q&quot;"]);
echo ":"; echo function_exists("htmlspecialchars");
echo ":"; echo function_exists("htmlentities");
echo ":"; echo function_exists("html_entity_decode");');
"#,
    );
    assert_eq!(
        out,
        "&lt;b&gt;&quot;Hi&quot; &amp; &#039;bye&#039;&lt;/b&gt;:&lt;a&gt;:<b>hi</b>:&lt;x&gt;:\"q\":1:1:1"
    );
}

/// Verifies eval URL codec builtins encode, decode, and dispatch as callables.
#[test]
fn test_eval_dispatches_url_codec_builtin_calls() {
    let out = compile_and_run(
        r#"<?php
eval('echo urlencode("a b&=~"); echo ":";
echo rawurlencode(string: "a b&=~"); echo ":";
echo urldecode("a+b%26%3D%7E"); echo ":";
echo rawurldecode("a+b%26%3D%7E"); echo ":";
echo call_user_func("urlencode", "%zz"); echo ":";
echo call_user_func_array("rawurldecode", ["string" => "x%2By%zz"]);
echo ":"; echo function_exists("urlencode");
echo ":"; echo function_exists("rawurlencode");
echo ":"; echo function_exists("urldecode");
echo ":"; echo function_exists("rawurldecode");');
"#,
    );
    assert_eq!(
        out,
        "a+b%26%3D%7E:a%20b%26%3D~:a b&=~:a+b&=~:%25zz:x+y%zz:1:1:1:1"
    );
}

/// Verifies eval `ctype_*` predicates inspect ASCII byte classes.
#[test]
fn test_eval_dispatches_ctype_builtin_calls() {
    let out = compile_and_run(
        r#"<?php
eval('echo ctype_alpha("abc") ? "A" : "-"; echo ":";
echo ctype_digit(text: "123") ? "D" : "-"; echo ":";
echo ctype_alnum("a1") ? "N" : "-"; echo ":";
echo ctype_space(" \t\n" . chr(11) . chr(12) . "\r") ? "S" : "-"; echo ":";
echo ctype_alpha("") ? "bad" : "empty"; echo ":";
echo call_user_func("ctype_digit", "12x") ? "bad" : "not-digit"; echo ":";
echo call_user_func_array("ctype_space", ["text" => " x"]) ? "bad" : "not-space";
echo ":"; echo function_exists("ctype_alpha");
echo ":"; echo function_exists("ctype_digit");
echo ":"; echo function_exists("ctype_alnum");
echo ":"; echo function_exists("ctype_space");');
"#,
    );
    assert_eq!(out, "A:D:N:S:empty:not-digit:not-space:1:1:1:1");
}

/// Verifies eval `crc32()` returns PHP-compatible non-negative checksums.
#[test]
fn test_eval_dispatches_crc32_builtin_call() {
    let out = compile_and_run(
        r#"<?php
eval('echo crc32(""); echo ":";
echo crc32(string: "123456789"); echo ":";
echo call_user_func("crc32", "hello"); echo ":";
echo call_user_func_array("crc32", ["string" => "The quick brown fox jumps over the lazy dog"]);
echo ":"; echo function_exists("crc32");');
"#,
    );
    assert_eq!(out, "0:3421780262:907060870:1095738169:1");
}

/// Verifies eval `hash_algos()` exposes the native supported hash algorithm list.
#[test]
fn test_eval_dispatches_hash_algos_builtin_call() {
    let out = compile_and_run(
        r#"<?php
eval('$algos = hash_algos();
echo count($algos) . ":" . $algos[0] . ":" . $algos[5] . ":";
echo in_array("crc32c", $algos) ? "crc" : "bad";
$call = call_user_func("hash_algos");
echo ":" . $call[18];
$spread = call_user_func_array("hash_algos", []);
echo ":" . $spread[27] . ":";
echo function_exists("hash_algos") ? "exists" : "missing";');
"#,
    );
    assert_eq!(out, "28:md2:sha256:crc:whirlpool:joaat:exists");
}

/// Verifies eval one-shot hash digest builtins use the crypto bridge.
#[test]
fn test_eval_dispatches_hash_digest_builtin_calls() {
    let out = compile_and_run(
        r#"<?php
eval('echo md5("abc"); echo ":";
echo sha1(string: "abc"); echo ":";
echo hash("sha256", "abc"); echo ":";
echo hash_hmac(algo: "sha256", data: "data", key: "key"); echo ":";
echo bin2hex(md5("abc", true)); echo ":";
echo bin2hex(call_user_func("sha1", "abc", true)); echo ":";
echo call_user_func_array("hash", ["algo" => "md5", "data" => "abc"]); echo ":";
echo call_user_func_array("hash_hmac", ["algo" => "sha256", "data" => "data", "key" => "key"]); echo ":";
file_put_contents("eval-hash-file.txt", "abc");
echo hash_file("sha256", "eval-hash-file.txt"); echo ":";
echo bin2hex(hash_file(algo: "md5", filename: "eval-hash-file.txt", binary: true)); echo ":";
echo call_user_func_array("hash_file", ["algo" => "md5", "filename" => "eval-hash-file.txt"]); echo ":";
echo hash_file("sha256", "eval-hash-file.txt.missing") === false ? "missing" : "bad"; echo ":";
unlink("eval-hash-file.txt");
echo function_exists("md5"); echo function_exists("sha1"); echo function_exists("hash"); echo function_exists("hash_file"); echo function_exists("hash_hmac");');
"#,
    );
    assert_eq!(
        out,
        concat!(
            "900150983cd24fb0d6963f7d28e17f72:",
            "a9993e364706816aba3e25717850c26c9cd0d89d:",
            "ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad:",
            "5031fe3d989c6d1537a013fa6e739da23463fdaec3b70137d828e36ace221bd0:",
            "900150983cd24fb0d6963f7d28e17f72:",
            "a9993e364706816aba3e25717850c26c9cd0d89d:",
            "900150983cd24fb0d6963f7d28e17f72:",
            "5031fe3d989c6d1537a013fa6e739da23463fdaec3b70137d828e36ace221bd0:",
            "ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad:",
            "900150983cd24fb0d6963f7d28e17f72:",
            "900150983cd24fb0d6963f7d28e17f72:",
            "missing:",
            "11111"
        )
    );
}

/// Verifies eval zero-argument system builtins match native runtime conventions.
#[test]
fn test_eval_dispatches_zero_arg_system_builtin_calls() {
    let out = compile_and_run(
        r#"<?php
eval('echo time() > 1000000000 ? "time" : "bad"; echo ":";
echo phpversion(); echo ":";
echo sys_get_temp_dir(); echo ":";
echo strlen(getcwd()) > 0 ? "cwd" : "bad"; echo ":";
echo call_user_func("time") > 1000000000 ? "call-time" : "bad"; echo ":";
echo call_user_func("phpversion"); echo ":";
echo call_user_func_array("getcwd", []) !== "" ? "call-cwd" : "bad"; echo ":";
echo call_user_func_array("sys_get_temp_dir", []); echo ":";
echo function_exists("time"); echo function_exists("phpversion"); echo function_exists("getcwd");
echo function_exists("sys_get_temp_dir");');
"#,
    );
    assert_eq!(
        out,
        format!(
            "time:{}:/tmp:cwd:call-time:{}:call-cwd:/tmp:1111",
            env!("CARGO_PKG_VERSION"),
            env!("CARGO_PKG_VERSION")
        )
    );
}

/// Verifies eval `date()` formats timestamps and `mktime()` creates them.
#[test]
fn test_eval_dispatches_date_mktime_builtin_calls() {
    let out = compile_and_run(
        r#"<?php
eval('$ts = mktime(13, 2, 3, 1, 2, 2024);
echo date("Y-m-d H:i:s", $ts);
echo ":" . date("j-n-G-g-A-a-N-D-M-l-F", $ts);
echo ":" . (date("U", $ts) === strval($ts) ? "U" : "bad");
echo ":" . call_user_func("date", "Y", $ts);
$named = call_user_func_array("mktime", ["hour" => 0, "minute" => 0, "second" => 0, "month" => 1, "day" => 1, "year" => 2000]);
echo ":" . date(format: "Y", timestamp: $named);
echo ":"; echo function_exists("date"); echo function_exists("mktime");');
"#,
    );
    assert_eq!(
        out,
        "2024-01-02 13:02:03:2-1-13-1-PM-pm-2-Tue-Jan-Tuesday-January:U:2024:2000:11"
    );
}

/// Verifies eval `strtotime()` parses supported ISO date strings and rejects others.
#[test]
fn test_eval_dispatches_strtotime_builtin_calls() {
    let out = compile_and_run(
        r#"<?php
eval('$date = strtotime("2024-06-15");
echo date("Y-m-d H:i:s", $date);
$full = strtotime("2024-06-15 12:30:45");
echo ":" . date("Y-m-d H:i:s", $full);
$short = strtotime("2024-06-15T12:30");
echo ":" . date("Y-m-d H:i:s", $short);
echo ":" . (strtotime("2024/06/15") === -1 ? "bad" : "wrong");
$call = call_user_func("strtotime", "2024-01-02 03:04:05");
echo ":" . date("Y-m-d H:i:s", $call);
$spread = call_user_func_array("strtotime", ["datetime" => "2024-01-02"]);
echo ":" . date("Y-m-d", $spread) . ":";
echo function_exists("strtotime");');
"#,
    );
    assert_eq!(
        out,
        "2024-06-15 00:00:00:2024-06-15 12:30:45:2024-06-15 12:30:00:bad:2024-01-02 03:04:05:2024-01-02:1"
    );
}

/// Verifies eval `microtime()` returns a plausible floating timestamp by all call paths.
#[test]
fn test_eval_dispatches_microtime_builtin_call() {
    let out = compile_and_run(
        r#"<?php
eval('echo microtime() > 1000000000 ? "now" : "bad"; echo ":";
echo microtime(as_float: false) > 1000000000 ? "named" : "bad"; echo ":";
echo call_user_func("microtime", true) > 1000000000 ? "call" : "bad"; echo ":";
echo call_user_func_array("microtime", ["as_float" => true]) > 1000000000 ? "array" : "bad";
echo ":"; echo function_exists("microtime");');
"#,
    );
    assert_eq!(out, "now:named:call:array:1");
}

/// Verifies eval realpath-cache builtins expose elephc's empty-cache convention.
#[test]
fn test_eval_dispatches_realpath_cache_builtin_calls() {
    let out = compile_and_run(
        r#"<?php
eval('$cache = realpath_cache_get();
echo count($cache) . ":" . realpath_cache_size() . ":";
$call_cache = call_user_func("realpath_cache_get");
echo count($call_cache) . ":";
echo call_user_func_array("realpath_cache_size", []) . ":";
echo function_exists("realpath_cache_get");
echo function_exists("realpath_cache_size");');
"#,
    );
    assert_eq!(out, "0:0:0:0:11");
}

/// Verifies eval environment builtins read, write, unset, and dispatch as callables.
#[test]
fn test_eval_dispatches_environment_builtin_calls() {
    let out = compile_and_run(
        r#"<?php
eval('putenv("ELEPHC_EVAL_ENV_TEST=direct");
echo getenv("ELEPHC_EVAL_ENV_TEST") . ":";
putenv(assignment: "ELEPHC_EVAL_ENV_TEST=named");
echo getenv(name: "ELEPHC_EVAL_ENV_TEST") . ":";
echo call_user_func("getenv", "ELEPHC_EVAL_ENV_TEST") . ":";
echo call_user_func_array("putenv", ["assignment" => "ELEPHC_EVAL_ENV_TEST=spread"]) ? "set" : "bad";
echo ":" . getenv("ELEPHC_EVAL_ENV_TEST") . ":";
putenv("ELEPHC_EVAL_ENV_TEST");
echo getenv("ELEPHC_EVAL_ENV_TEST") === "" ? "empty" : "bad";
echo ":"; echo function_exists("getenv");
echo function_exists("putenv");');
"#,
    );
    assert_eq!(out, "direct:named:named:set:spread:empty:11");
}

/// Verifies eval sleep builtins dispatch through direct, named, and callable paths.
#[test]
fn test_eval_dispatches_sleep_builtin_calls() {
    let out = compile_and_run(
        r#"<?php
eval('echo sleep(0) . ":";
echo sleep(seconds: 0) . ":";
usleep(0);
echo "u:";
echo call_user_func("sleep", 0) . ":";
echo call_user_func_array("usleep", ["microseconds" => 0]) === null ? "null" : "bad";
echo ":"; echo function_exists("sleep");
echo function_exists("usleep");');
"#,
    );
    assert_eq!(out, "0:0:u:0:null:11");
}

/// Verifies eval `php_uname()` dispatches default, named, mode, and callable calls.
#[test]
fn test_eval_dispatches_php_uname_builtin_call() {
    let out = compile_and_run(
        r#"<?php
eval('echo strlen(php_uname()) > 0 ? "all" : "empty"; echo ":";
echo php_uname() === php_uname("a") ? "same" : "different"; echo ":";
echo strlen(php_uname(mode: "s")) > 0 ? "sys" : "empty"; echo ":";
echo strlen(php_uname("n")) > 0 ? "node" : "empty"; echo ":";
echo strlen(php_uname("r")) > 0 ? "release" : "empty"; echo ":";
echo strlen(php_uname("v")) > 0 ? "version" : "empty"; echo ":";
echo strlen(php_uname("m")) > 0 ? "machine" : "empty"; echo ":";
echo strlen(call_user_func("php_uname", "m")) > 0 ? "call" : "empty"; echo ":";
echo strlen(call_user_func_array("php_uname", ["mode" => "n"])) > 0 ? "spread" : "empty"; echo ":";
echo function_exists("php_uname");');
"#,
    );
    assert_eq!(
        out,
        "all:same:sys:node:release:version:machine:call:spread:1"
    );
}

/// Verifies eval `gethostbyname()` handles IPv4 literals and failed lookups.
#[test]
fn test_eval_dispatches_gethostbyname_builtin_call() {
    let out = compile_and_run(
        r#"<?php
eval('echo gethostbyname("127.0.0.1") . ":";
echo gethostbyname(hostname: "not a host") . ":";
echo call_user_func("gethostbyname", "127.0.0.1") . ":";
echo call_user_func_array("gethostbyname", ["hostname" => "not a host"]) . ":";
echo function_exists("gethostbyname");');
"#,
    );
    assert_eq!(out, "127.0.0.1:not a host:127.0.0.1:not a host:1");
}

/// Verifies eval `gethostname()` dispatches direct and callable zero-arg calls.
#[test]
fn test_eval_dispatches_gethostname_builtin_call() {
    let out = compile_and_run(
        r#"<?php
eval('echo strlen(gethostname()) > 0 ? "host" : "empty"; echo ":";
echo strlen(call_user_func("gethostname")) > 0 ? "call" : "empty"; echo ":";
echo strlen(call_user_func_array("gethostname", [])) > 0 ? "spread" : "empty"; echo ":";
echo function_exists("gethostname");');
"#,
    );
    assert_eq!(out, "host:call:spread:1");
}

/// Verifies eval `gethostbyaddr()` handles valid, malformed, and callable calls.
#[test]
fn test_eval_dispatches_gethostbyaddr_builtin_call() {
    let out = compile_and_run(
        r#"<?php
eval('echo strlen(gethostbyaddr("127.0.0.1")) > 0 ? "direct" : "empty"; echo ":";
echo strlen(gethostbyaddr(ip: "127.0.0.1")) > 0 ? "named" : "empty"; echo ":";
echo gethostbyaddr("not-an-ip-address") === false ? "false" : "bad"; echo ":";
echo strlen(call_user_func("gethostbyaddr", "127.0.0.1")) > 0 ? "call" : "empty"; echo ":";
echo call_user_func_array("gethostbyaddr", ["ip" => "not-an-ip-address"]) === false ? "spread" : "bad"; echo ":";
echo function_exists("gethostbyaddr");');
"#,
    );
    assert_eq!(out, "direct:named:false:call:spread:1");
}

/// Verifies eval protocol and service database lookups dispatch dynamically.
#[test]
fn test_eval_dispatches_protocol_service_builtin_calls() {
    let out = compile_and_run(
        r#"<?php
eval('echo getprotobyname("TCP") . ":";
echo getprotobynumber(6) . ":";
echo getprotobyname("no_such_protocol") === false ? "missing-proto" : "bad"; echo ":";
echo getprotobynumber(999) === false ? "missing-number" : "bad"; echo ":";
echo getservbyname("www", "tcp") . ":";
echo getservbyport(80, "tcp") . ":";
echo getservbyname("no_such_service", "tcp") === false ? "missing-service" : "bad"; echo ":";
echo getservbyport(80, "no_such_proto") === false ? "missing-port" : "bad"; echo ":";
echo call_user_func("getprotobyname", "udp") . ":";
echo call_user_func_array("getprotobynumber", ["protocol" => 17]) . ":";
echo call_user_func("getservbyname", "https", "tcp") . ":";
echo call_user_func_array("getservbyport", ["port" => 443, "protocol" => "tcp"]) . ":";
echo function_exists("getprotobyname"); echo function_exists("getprotobynumber"); echo function_exists("getservbyname"); echo function_exists("getservbyport");');
"#,
    );
    assert_eq!(
        out,
        "6:tcp:missing-proto:missing-number:80:http:missing-service:missing-port:17:udp:443:https:1111"
    );
}

/// Verifies eval stream introspection builtins return native-compatible static lists.
#[test]
fn test_eval_dispatches_stream_introspection_builtin_calls() {
    let out = compile_and_run(
        r#"<?php
eval('$wrappers = stream_get_wrappers();
$transports = stream_get_transports();
$filters = stream_get_filters();
echo count($wrappers) . ":" . $wrappers[0] . ":" . $wrappers[5] . ":";
echo count($transports) . ":" . $transports[0] . ":" . $transports[8] . ":";
echo count($filters) . ":" . $filters[2] . ":";
$call_wrappers = call_user_func("stream_get_wrappers");
echo $call_wrappers[10] . ":";
$call_transports = call_user_func_array("stream_get_transports", []);
echo $call_transports[11] . ":";
$call_filters = call_user_func_array("stream_get_filters", []);
echo $call_filters[13] . ":";
echo function_exists("stream_get_wrappers"); echo function_exists("stream_get_transports"); echo function_exists("stream_get_filters");');
"#,
    );
    assert_eq!(
        out,
        "11:file:https:12:tcp:tlsv1.0:14:string.rot13:glob:tlsv1.3:bzip2.decompress:111"
    );
}

/// Verifies eval IPv4 conversion builtins handle integer, string, and raw-byte forms.
#[test]
fn test_eval_dispatches_ip_conversion_builtin_calls() {
    let out = compile_and_run(
        r#"<?php
eval('echo long2ip(3232235777) . ":";
echo long2ip(ip: 4294967295) . ":";
echo ip2long("192.168.1.1") . ":";
echo ip2long(ip: "1.2.3") === false ? "bad-ip" : "bad"; echo ":";
$packed = inet_pton("1.2.3.4");
echo bin2hex($packed) . ":";
echo inet_pton(ip: "nonsense") === false ? "bad-pton" : "bad"; echo ":";
echo inet_ntop($packed) . ":";
echo inet_ntop(ip: "xx") === false ? "bad-ntop" : "bad"; echo ":";
echo call_user_func("long2ip", 2130706433) . ":";
echo call_user_func_array("ip2long", ["ip" => "0.0.0.0"]) . ":";
echo function_exists("long2ip"); echo function_exists("ip2long");
echo function_exists("inet_pton"); echo function_exists("inet_ntop");');
"#,
    );
    assert_eq!(
        out,
        "192.168.1.1:255.255.255.255:3232235777:bad-ip:01020304:bad-pton:1.2.3.4:bad-ntop:127.0.0.1:0:1111"
    );
}

/// Verifies eval `basename()` and `dirname()` preserve static path edge-case behavior.
#[test]
fn test_eval_dispatches_path_component_builtin_calls() {
    let out = compile_and_run(
        r#"<?php
eval('echo basename("/var/log/syslog.log", ".log") . ":";
echo basename(path: "/usr///") . ":";
echo basename("/", "x") === "" ? "root" : "bad"; echo ":";
echo dirname("/usr/local/bin/tool", 2) . ":";
echo dirname(path: "/usr///local///bin") . ":";
echo call_user_func("basename", "foo.tar.gz", ".bz2") . ":";
echo call_user_func_array("dirname", ["path" => "/usr", "levels" => 3]) . ":";
echo function_exists("basename"); echo function_exists("dirname");');
"#,
    );
    assert_eq!(
        out,
        "syslog:usr:root:/usr/local:/usr///local:foo.tar.gz:/:11"
    );
}

/// Verifies eval `realpath()` returns strings for existing paths and false for missing paths.
#[test]
fn test_eval_dispatches_realpath_builtin_call() {
    let out = compile_and_run(
        r#"<?php
eval('echo realpath(".") !== false ? "resolved" : "bad"; echo ":";
echo realpath(path: "elephc-eval-missing-path") === false ? "false" : "bad"; echo ":";
echo call_user_func("realpath", ".") !== false ? "call" : "bad"; echo ":";
echo call_user_func_array("realpath", ["path" => "elephc-eval-missing-path"]) === false ? "array-false" : "bad";
echo ":"; echo function_exists("realpath");');
"#,
    );
    assert_eq!(out, "resolved:false:call:array-false:1");
}

/// Verifies eval regex builtins handle captures, replacement, callbacks, and splitting.
#[test]
fn test_eval_dispatches_preg_builtin_calls() {
    let out = compile_and_run(
        r#"<?php
eval('$ok = preg_match("/([a-z]+)([0-9]+)/", "id42", $matches);
echo $ok . ":" . count($matches) . ":" . $matches[0] . ":" . $matches[1] . ":" . $matches[2] . ":";
echo preg_match("/xyz/", "id42") . ":";
echo preg_match_all("/[0-9]+/", "a1b22c333") . ":";
$allCount = preg_match_all("/([a-z]+)([0-9]+)/", "a1 b22", $all);
echo $allCount . ":" . count($all) . ":" . $all[0][1] . ":" . $all[1][0] . ":" . $all[2][1] . ":";
$setCount = preg_match_all("/([a-z]+)([0-9]+)/", "a1 b22", $set, PREG_SET_ORDER);
echo $setCount . ":" . count($set) . ":" . $set[0][0] . ":" . $set[0][1] . ":" . $set[1][2] . ":";
preg_match_all("/(x)(y)/", "abc", $none);
echo count($none) . ":" . count($none[0]) . ":" . count($none[1]) . ":" . count($none[2]) . ":";
echo preg_replace("/([a-z])([0-9])/", "$2-$1", "a1 b2") . ":";
function eval_regex_wrap($matches) { return "[" . $matches[0] . "]"; }
echo preg_replace_callback("/[A-Z]/", "eval_regex_wrap", "AB") . ":";
$limited = preg_split("/,/", "a,b,c", 2);
echo count($limited) . ":" . $limited[0] . ":" . $limited[1] . ":";
$kept = preg_split("/,/", "a,,b", 0, PREG_SPLIT_NO_EMPTY);
echo count($kept) . ":" . $kept[1] . ":";
echo call_user_func("preg_match", "/x/", "x") . ":";
$replaced = call_user_func_array("preg_replace", ["pattern" => "/[0-9]+/", "replacement" => "N", "subject" => "a12"]);
echo $replaced . ":";
$captured = preg_split("/(,)/", "a,b", 0, PREG_SPLIT_DELIM_CAPTURE);
echo count($captured) . ":" . $captured[1] . ":";
echo function_exists("preg_match") && function_exists("preg_match_all") && function_exists("preg_replace") && function_exists("preg_replace_callback") && function_exists("preg_split") && defined("PREG_SPLIT_NO_EMPTY") && defined("PREG_SET_ORDER");');
"#,
    );
    assert_eq!(
        out,
        "1:3:id42:id:42:0:3:2:3:b22:a:22:2:2:a1:a:22:3:0:0:0:1-a 2-b:[A][B]:2:a:b,c:2:b:1:aN:3:,:1"
    );
}

/// Verifies eval `fnmatch()` supports wildcards, classes, flags, constants, and callables.
#[test]
fn test_eval_dispatches_fnmatch_builtin_call() {
    let out = compile_and_run(
        r#"<?php
eval('echo fnmatch("*.log", "system.log") ? "match" : "bad"; echo ":";
echo fnmatch("*.log", "logs/system.log", FNM_PATHNAME) ? "bad" : "path"; echo ":";
echo fnmatch("*.LOG", "system.log", FNM_CASEFOLD) ? "case" : "bad"; echo ":";
echo fnmatch("*", ".env", FNM_PERIOD) ? "bad" : "period"; echo ":";
echo fnmatch("[!abc]oo", "doo") && !fnmatch("[!abc]oo", "boo") ? "class" : "bad"; echo ":";
echo fnmatch("a\\\\*b", "a*b") ? "escape" : "bad"; echo ":";
echo fnmatch("a\\\\*b", "a\\\\xxb", FNM_NOESCAPE) ? "noescape" : "bad"; echo ":";
$flags = FNM_PATHNAME | FNM_CASEFOLD;
echo fnmatch("dir/*.TXT", "dir/file.txt", $flags) ? "flags" : "bad"; echo ":";
echo call_user_func("fnmatch", "*.txt", "report.txt") ? "call" : "bad"; echo ":";
echo call_user_func_array("fnmatch", ["pattern" => "*.TXT", "filename" => "report.txt", "flags" => FNM_CASEFOLD]) ? "callarray" : "bad"; echo ":";
echo function_exists("fnmatch"); echo defined("FNM_CASEFOLD");');
"#,
    );
    assert_eq!(
        out,
        "match:path:case:period:class:escape:noescape:flags:call:callarray:11"
    );
}

/// Verifies eval `pathinfo()` supports arrays, component flags, constants, and callables.
#[test]
fn test_eval_dispatches_pathinfo_builtin_call() {
    let out = compile_and_run(
        r#"<?php
eval('$info = pathinfo("/var/log/syslog.log");
echo $info["dirname"] . "|" . $info["basename"] . "|" . $info["extension"] . "|" . $info["filename"] . ":";
echo pathinfo("archive.tar.gz", PATHINFO_EXTENSION) . ":";
echo pathinfo(".bashrc", PATHINFO_FILENAME) === "" ? "dotfile" : "bad"; echo ":";
echo pathinfo("file.", PATHINFO_EXTENSION) === "" ? "trail" : "bad"; echo ":";
echo pathinfo("", PATHINFO_DIRNAME) === "" ? "empty-dir" : "bad"; echo ":";
$plain = pathinfo("/etc/hosts");
echo array_key_exists("extension", $plain) ? "bad" : "no-ext"; echo ":";
echo pathinfo("/a/b.php", PATHINFO_BASENAME | PATHINFO_FILENAME) . ":";
$call = call_user_func("pathinfo", "foo.txt", PATHINFO_ALL);
echo $call["basename"] . ":";
echo call_user_func_array("pathinfo", ["path" => "foo.txt", "flags" => 0]) === "" ? "zero" : "bad";
echo ":"; echo function_exists("pathinfo"); echo defined("PATHINFO_ALL");');
"#,
    );
    assert_eq!(
        out,
        "/var/log|syslog.log|log|syslog:gz:dotfile:trail:empty-dir:no-ext:b.php:foo.txt:zero:11"
    );
}

/// Verifies eval local filesystem builtins read, write, stat, delete, and dispatch.
#[test]
fn test_eval_dispatches_filesystem_builtin_calls() {
    let out = compile_and_run(
        r#"<?php
eval('echo file_put_contents("eval-fs.txt", "hello") . ":";
echo file_get_contents("eval-fs.txt") . ":";
echo file_exists("eval-fs.txt") ? "exists" : "missing"; echo ":";
echo is_file(filename: "eval-fs.txt") ? "file" : "bad"; echo ":";
echo is_dir(".") ? "dir" : "bad"; echo ":";
echo is_readable("eval-fs.txt") ? "readable" : "bad"; echo ":";
echo is_writable("eval-fs.txt") ? "writable" : "bad"; echo ":";
echo is_writeable("eval-fs.txt") ? "writeable" : "bad"; echo ":";
echo filesize("eval-fs.txt") . ":";
echo call_user_func("file_exists", "eval-fs.txt") ? "call-exists" : "bad"; echo ":";
echo call_user_func_array("filesize", ["filename" => "eval-fs.txt"]) . ":";
echo unlink("eval-fs.txt") ? "unlinked" : "bad"; echo ":";
echo file_exists("eval-fs.txt") ? "bad" : "gone"; echo ":";
echo function_exists("file_get_contents"); echo function_exists("file_put_contents");
echo function_exists("file_exists"); echo function_exists("is_file"); echo function_exists("is_dir");
echo function_exists("is_readable"); echo function_exists("is_writable"); echo function_exists("is_writeable");
echo function_exists("filesize"); echo function_exists("unlink");');
"#,
    );
    assert_eq!(
        out,
        "5:hello:exists:file:dir:readable:writable:writeable:5:call-exists:5:unlinked:gone:1111111111"
    );
}

/// Verifies eval disk-space builtins return positive local capacity and zero on failure.
#[test]
fn test_eval_dispatches_disk_space_builtin_calls() {
    let out = compile_and_run(
        r#"<?php
eval('echo disk_free_space(".") > 0 ? "free" : "bad"; echo ":";
echo disk_total_space(directory: ".") > 0 ? "total" : "bad"; echo ":";
echo disk_total_space(".") >= disk_free_space(".") ? "ordered" : "bad"; echo ":";
echo disk_free_space("no/such/path/elephc-eval") === 0.0 ? "missing" : "bad"; echo ":";
echo call_user_func("disk_free_space", ".") > 0 ? "call" : "bad"; echo ":";
echo call_user_func_array("disk_total_space", ["directory" => "."]) > 0 ? "spread" : "bad";
echo ":"; echo function_exists("disk_free_space"); echo function_exists("disk_total_space");');
"#,
    );
    assert_eq!(out, "free:total:ordered:missing:call:spread:11");
}

/// Verifies eval stat metadata builtins return scalar metadata and dispatch dynamically.
#[test]
fn test_eval_dispatches_stat_metadata_builtin_calls() {
    let out = compile_and_run(
        r#"<?php
eval('file_put_contents("eval-stat.txt", "hello");
echo filemtime("eval-stat.txt") > 0 ? "mtime" : "bad"; echo ":";
echo fileatime(filename: "eval-stat.txt") > 0 ? "atime" : "bad"; echo ":";
echo filectime("eval-stat.txt") > 0 ? "ctime" : "bad"; echo ":";
echo fileperms("eval-stat.txt") > 0 ? "perms" : "bad"; echo ":";
echo fileowner("eval-stat.txt") >= 0 ? "owner" : "bad"; echo ":";
echo filegroup("eval-stat.txt") >= 0 ? "group" : "bad"; echo ":";
echo fileinode("eval-stat.txt") > 0 ? "inode" : "bad"; echo ":";
echo filetype("eval-stat.txt") . ":";
echo filetype(".") . ":";
echo is_executable("/bin/sh") ? "exec" : "bad"; echo ":";
echo is_link("eval-stat.txt") ? "bad" : "notlink"; echo ":";
echo fileatime("missing-stat.txt") === false ? "missing-atime" : "bad"; echo ":";
echo filetype("missing-stat.txt") === false ? "missing-type" : "bad"; echo ":";
echo filemtime("missing-stat.txt") === 0 ? "missing-mtime" : "bad"; echo ":";
echo call_user_func("filetype", "eval-stat.txt") . ":";
echo call_user_func_array("fileinode", ["filename" => "eval-stat.txt"]) > 0 ? "callinode" : "bad"; echo ":";
echo function_exists("filemtime"); echo function_exists("fileatime");
echo function_exists("filectime"); echo function_exists("fileperms");
echo function_exists("fileowner"); echo function_exists("filegroup");
echo function_exists("fileinode"); echo function_exists("filetype");
echo function_exists("is_executable"); echo function_exists("is_link");
unlink("eval-stat.txt");');
"#,
    );
    assert_eq!(
        out,
        "mtime:atime:ctime:perms:owner:group:inode:file:dir:exec:notlink:missing-atime:missing-type:missing-mtime:file:callinode:1111111111"
    );
}

/// Verifies eval `stat()` and `lstat()` build PHP-compatible metadata arrays.
#[test]
fn test_eval_dispatches_stat_array_builtin_calls() {
    let out = compile_and_run(
        r#"<?php
eval('file_put_contents("eval-stat-array.txt", "hello");
symlink("eval-stat-array.txt", "eval-lstat-array.txt");
$stat = stat("eval-stat-array.txt");
$lstat = lstat("eval-lstat-array.txt");
echo $stat["size"] === 5 && $stat[7] === $stat["size"] ? "stat" : "bad"; echo ":";
echo ($stat["mode"] & 61440) === 32768 ? "mode" : "bad"; echo ":";
echo ($lstat["mode"] & 61440) === 40960 ? "lstat" : "bad"; echo ":";
echo stat("eval-stat-array-missing.txt") === false && lstat("eval-stat-array-missing.txt") === false ? "missing" : "bad"; echo ":";
$call = call_user_func("stat", "eval-stat-array.txt");
echo $call["mtime"] === filemtime("eval-stat-array.txt") ? "callstat" : "bad"; echo ":";
$call_lstat = call_user_func_array("lstat", ["filename" => "eval-lstat-array.txt"]);
echo $call_lstat["ino"] > 0 ? "calllstat" : "bad"; echo ":";
echo unlink("eval-lstat-array.txt") && unlink("eval-stat-array.txt") ? "cleanup" : "bad"; echo ":";
echo function_exists("stat"); echo function_exists("lstat");
');
"#,
    );
    assert_eq!(out, "stat:mode:lstat:missing:callstat:calllstat:cleanup:11");
}

/// Verifies eval path operation builtins mutate local filesystem state.
#[test]
fn test_eval_dispatches_path_operation_builtin_calls() {
    let out = compile_and_run(
        r#"<?php
eval('file_put_contents("eval-op-src.txt", "hello");
echo mkdir("eval-op-dir") ? "mkdir" : "bad"; echo ":";
echo copy("eval-op-src.txt", "eval-op-copy.txt") ? "copy" : "bad"; echo ":";
echo rename("eval-op-copy.txt", "eval-op-moved.txt") && file_exists("eval-op-moved.txt") ? "rename" : "bad"; echo ":";
echo symlink("eval-op-src.txt", "eval-op-link.txt") ? "symlink" : "bad"; echo ":";
echo readlink("eval-op-link.txt") === "eval-op-src.txt" ? "readlink" : "bad"; echo ":";
echo linkinfo("eval-op-link.txt") >= 0 ? "linkinfo" : "bad"; echo ":";
echo link("eval-op-src.txt", "eval-op-hard.txt") ? "hardlink" : "bad"; echo ":";
echo readlink("eval-op-src.txt") === false ? "readlink-false" : "bad"; echo ":";
echo linkinfo("eval-op-missing.txt") === -1 ? "linkinfo-missing" : "bad"; echo ":";
echo chdir("eval-op-dir") ? "chdir" : "bad"; echo ":";
echo getcwd() !== "" ? "cwd" : "bad"; echo ":";
chdir("..");
echo clearstatcache(true, "eval-op-src.txt") === null ? "cache" : "bad"; echo ":";
echo unlink("eval-op-link.txt") && unlink("eval-op-hard.txt") && unlink("eval-op-moved.txt") && unlink("eval-op-src.txt") && rmdir("eval-op-dir") ? "cleanup" : "bad"; echo ":";
echo call_user_func("mkdir", "eval-op-call-dir") ? "callmkdir" : "bad"; echo ":";
echo call_user_func_array("rmdir", ["directory" => "eval-op-call-dir"]) ? "callrmdir" : "bad"; echo ":";
echo function_exists("mkdir"); echo function_exists("rmdir"); echo function_exists("copy");
echo function_exists("rename"); echo function_exists("symlink"); echo function_exists("link");
echo function_exists("readlink"); echo function_exists("linkinfo"); echo function_exists("clearstatcache");
');
"#,
    );
    assert_eq!(
        out,
        "mkdir:copy:rename:symlink:readlink:linkinfo:hardlink:readlink-false:linkinfo-missing:chdir:cwd:cache:cleanup:callmkdir:callrmdir:111111111"
    );
}

/// Verifies eval file-listing builtins build arrays, stream files, and dispatch dynamically.
#[test]
fn test_eval_dispatches_file_listing_builtin_calls() {
    let out = compile_and_run(
        r#"<?php
eval('file_put_contents("eval-lines.txt", "one\ntwo");
file_put_contents("eval-empty.txt", "");
$lines = file("eval-lines.txt");
echo count($lines) . ":";
echo $lines[0] === "one\n" ? "line0" : "bad"; echo ":";
echo $lines[1] === "two" ? "line1" : "bad"; echo ":";
echo "[";
$bytes = readfile(filename: "eval-empty.txt");
echo "]" . $bytes . ":";
echo readfile("eval-missing.txt") === false ? "missing-readfile" : "bad"; echo ":";
mkdir("eval-list-dir");
file_put_contents("eval-list-dir/a.txt", "a");
file_put_contents("eval-list-dir/b.txt", "b");
$scan = scandir(directory: "eval-list-dir");
echo count($scan) . ":";
echo in_array(".", $scan) && in_array("..", $scan) && in_array("a.txt", $scan) && in_array("b.txt", $scan) ? "scan" : "bad"; echo ":";
$call_lines = call_user_func("file", "eval-lines.txt");
echo $call_lines[0] === "one\n" ? "callfile" : "bad"; echo ":";
$call_scan = call_user_func_array("scandir", ["directory" => "eval-list-dir"]);
echo count($call_scan) . ":";
echo unlink("eval-list-dir/a.txt") && unlink("eval-list-dir/b.txt") && rmdir("eval-list-dir") && unlink("eval-lines.txt") && unlink("eval-empty.txt") ? "cleanup" : "bad"; echo ":";
echo function_exists("file"); echo function_exists("readfile"); echo function_exists("scandir");
');
"#,
    );
    assert_eq!(
        out,
        "2:line0:line1:[]0:missing-readfile:4:scan:callfile:4:cleanup:111"
    );
}

/// Verifies eval `glob()` expands local patterns and dispatches dynamically.
#[test]
fn test_eval_dispatches_glob_builtin_calls() {
    let out = compile_and_run(
        r#"<?php
eval('mkdir("eval-glob-dir");
file_put_contents("eval-glob-dir/a.txt", "a");
file_put_contents("eval-glob-dir/b.log", "b");
file_put_contents("eval-glob-dir/c.txt", "c");
file_put_contents("eval-glob-dir/.hidden.txt", "h");
$matches = glob("eval-glob-dir/*.txt");
echo count($matches) === 2 && basename($matches[0]) === "a.txt" && basename($matches[1]) === "c.txt" ? "glob" : "bad"; echo ":";
echo count(glob("eval-glob-dir/*.none")) === 0 ? "empty" : "bad"; echo ":";
$literal = glob("eval-glob-dir/a.txt");
echo count($literal) === 1 && $literal[0] === "eval-glob-dir/a.txt" ? "literal" : "bad"; echo ":";
$all = glob("eval-glob-dir/*");
echo in_array("eval-glob-dir/.hidden.txt", $all) ? "bad" : "hidden"; echo ":";
$call = call_user_func("glob", "eval-glob-dir/*.log");
echo count($call) === 1 && basename($call[0]) === "b.log" ? "callglob" : "bad"; echo ":";
$call_array = call_user_func_array("glob", ["pattern" => "eval-glob-dir/*.txt"]);
echo count($call_array) === 2 ? "callarray" : "bad"; echo ":";
unlink("eval-glob-dir/.hidden.txt");
unlink("eval-glob-dir/c.txt");
unlink("eval-glob-dir/b.log");
unlink("eval-glob-dir/a.txt");
echo rmdir("eval-glob-dir") ? "cleanup" : "bad"; echo ":";
echo function_exists("glob");
');
"#,
    );
    assert_eq!(
        out,
        "glob:empty:literal:hidden:callglob:callarray:cleanup:1"
    );
}

/// Verifies eval file-modification builtins update modes, masks, temp files, and dispatch.
#[test]
fn test_eval_dispatches_file_modify_builtin_calls() {
    let out = compile_and_run(
        r#"<?php
eval('file_put_contents("eval-mod.txt", "x");
echo chmod(filename: "eval-mod.txt", permissions: 384) ? "chmod" : "bad"; echo ":";
echo (fileperms("eval-mod.txt") & 511) === 384 ? "mode" : "bad"; echo ":";
echo chmod("eval-missing-mod.txt", 384) ? "bad" : "chmod-false"; echo ":";
$tmp = tempnam(directory: ".", prefix: "evm");
echo file_exists($tmp) && str_starts_with(basename($tmp), "evm") ? "tempnam" : "bad"; echo ":";
unlink($tmp);
$previous = umask(mask: 18);
$set = umask($previous);
echo $set === 18 ? "umask" : "bad"; echo ":";
$before = umask(18);
$probe = umask();
$restore = umask($before);
echo $probe === 18 && $restore === 18 ? "probe" : "bad"; echo ":";
echo call_user_func("chmod", "eval-mod.txt", 420) ? "callchmod" : "bad"; echo ":";
$call_tmp = call_user_func_array("tempnam", ["directory" => ".", "prefix" => "evc"]);
echo file_exists($call_tmp) && str_starts_with(basename($call_tmp), "evc") ? "calltempnam" : "bad"; echo ":";
unlink($call_tmp);
echo unlink("eval-mod.txt") ? "cleanup" : "bad"; echo ":";
echo function_exists("chmod"); echo function_exists("tempnam"); echo function_exists("umask");
');
"#,
    );
    assert_eq!(
        out,
        "chmod:mode:chmod-false:tempnam:umask:probe:callchmod:calltempnam:cleanup:111"
    );
}

/// Verifies eval `touch()` creates files, stamps mtimes, and dispatches dynamically.
#[test]
fn test_eval_dispatches_touch_builtin_calls() {
    let out = compile_and_run(
        r#"<?php
eval('echo touch(filename: "eval-touch-created.txt") && file_exists("eval-touch-created.txt") ? "create" : "bad"; echo ":";
file_put_contents("eval-touch-stamped.txt", "x");
echo touch("eval-touch-stamped.txt", 1000000000) ? "mtime" : "bad"; echo ":";
echo filemtime("eval-touch-stamped.txt") === 1000000000 ? "readmtime" : "bad"; echo ":";
echo touch("eval-touch-stamped.txt", 1000000001, null) && filemtime("eval-touch-stamped.txt") === 1000000001 ? "nullatime" : "bad"; echo ":";
echo touch("eval-touch-stamped.txt", 1000000002, 1000000003) && filemtime("eval-touch-stamped.txt") === 1000000002 ? "both" : "bad"; echo ":";
echo touch("eval-touch-missing/x.txt") ? "bad" : "touch-false"; echo ":";
echo call_user_func("touch", "eval-touch-created.txt", 1000000004) ? "calltouch" : "bad"; echo ":";
echo call_user_func_array("touch", ["filename" => "eval-touch-stamped.txt", "mtime" => 1000000005]) ? "callarray" : "bad"; echo ":";
echo unlink("eval-touch-created.txt") && unlink("eval-touch-stamped.txt") ? "cleanup" : "bad"; echo ":";
echo function_exists("touch");
');
"#,
    );
    assert_eq!(
        out,
        "create:mtime:readmtime:nullatime:both:touch-false:calltouch:callarray:cleanup:1"
    );
}

/// Verifies eval `bin2hex()` converts byte strings directly and by callable dispatch.
#[test]
fn test_eval_dispatches_bin2hex_builtin_call() {
    let out = compile_and_run(
        r#"<?php
eval('echo bin2hex("Az"); echo ":";
echo bin2hex(string: "A\n"); echo ":";
echo bin2hex(\'\n\'); echo ":";
echo bin2hex("A\q"); echo ":";
echo bin2hex("A\v\e\f"); echo ":";
echo call_user_func("bin2hex", "!?"); echo ":";
echo call_user_func_array("bin2hex", ["string" => "ok"]);
echo ":"; echo function_exists("bin2hex");');
"#,
    );
    assert_eq!(out, "417a:410a:5c6e:415c71:410b1b0c:213f:6f6b:1");
}

/// Verifies eval `hex2bin()` decodes hex strings directly and by callable dispatch.
#[test]
fn test_eval_dispatches_hex2bin_builtin_call() {
    let out = compile_and_run(
        r#"<?php
eval('echo hex2bin("417a"); echo ":";
echo bin2hex(hex2bin(string: "410a")); echo ":";
echo call_user_func("hex2bin", "213f"); echo ":";
echo call_user_func_array("hex2bin", ["string" => "6f6b"]);
echo ":"; echo function_exists("hex2bin");');
"#,
    );
    assert_eq!(out, "Az:410a:!?:ok:1");
}

/// Verifies eval `addslashes()` and `stripslashes()` use PHP byte escaping semantics.
#[test]
fn test_eval_dispatches_slash_escape_builtin_calls() {
    let out = compile_and_run(
        r#"<?php
eval('$escaped = addslashes("a\"b");
echo bin2hex($escaped); echo ":";
echo bin2hex(stripslashes($escaped)); echo ":";
echo call_user_func("addslashes", "x\"y"); echo ":";
echo call_user_func_array("stripslashes", [addslashes("o\"k")]);
echo ":"; echo function_exists("addslashes") && function_exists("stripslashes");');
"#,
    );
    assert_eq!(out, "615c2262:612262:x\\\"y:o\"k:1");
}

/// Verifies eval `base64_encode()` encodes byte strings directly and by callable dispatch.
#[test]
fn test_eval_dispatches_base64_encode_builtin_call() {
    let out = compile_and_run(
        r#"<?php
eval('echo base64_encode("Hello"); echo ":";
echo base64_encode(string: "Hi"); echo ":";
echo call_user_func("base64_encode", "Test 123!"); echo ":";
echo call_user_func_array("base64_encode", ["string" => ""]);
echo ":"; echo function_exists("base64_encode");');
"#,
    );
    assert_eq!(out, "SGVsbG8=:SGk=:VGVzdCAxMjMh::1");
}

/// Verifies eval `base64_decode()` decodes byte strings directly and by callable dispatch.
#[test]
fn test_eval_dispatches_base64_decode_builtin_call() {
    let out = compile_and_run(
        r#"<?php
eval('echo base64_decode("SGVsbG8="); echo ":";
echo base64_decode(string: "SGk="); echo ":";
echo call_user_func("base64_decode", "VGVzdCAxMjMh"); echo ":";
echo call_user_func_array("base64_decode", ["string" => ""]);
echo ":"; echo function_exists("base64_decode");');
"#,
    );
    assert_eq!(out, "Hello:Hi:Test 123!::1");
}

/// Verifies eval `str_contains()` supports direct and callable byte-string search.
#[test]
fn test_eval_dispatches_str_contains_builtin_call() {
    let out = compile_and_run(
        r#"<?php
eval('echo str_contains("Hello World", "World") ? "Y" : "N";
echo str_contains("Hello", "z") ? "bad" : ":N";
echo str_contains("Hello", "") ? ":E" : "bad";
echo call_user_func("str_contains", "abc", "b") ? ":C" : "bad";
echo call_user_func_array("str_contains", ["abc", "x"]) ? "bad" : ":A";
echo ":"; echo function_exists("str_contains");');
"#,
    );
    assert_eq!(out, "Y:N:E:C:A:1");
}

/// Verifies eval `strpos()` and `strrpos()` return byte offsets or false.
#[test]
fn test_eval_dispatches_string_position_builtin_calls() {
    let out = compile_and_run(
        r#"<?php
eval('echo strpos("banana", "na");
echo ":"; echo strrpos("banana", "na");
echo ":"; echo strpos("abc", "z") === false ? "F" : "bad";
echo ":"; echo strpos("abc", "");
echo ":"; echo strrpos("abc", "");
echo ":"; echo call_user_func("strpos", "abc", "b");
echo ":"; echo call_user_func_array("strrpos", ["ababa", "ba"]);
echo ":"; echo function_exists("strpos"); echo function_exists("strrpos");');
"#,
    );
    assert_eq!(out, "2:4:F:0:3:1:3:11");
}

/// Verifies eval `strstr()` returns matching suffixes, prefixes, and false for misses.
#[test]
fn test_eval_dispatches_strstr_builtin_call() {
    let out = compile_and_run(
        r#"<?php
eval('echo strstr("user@example.com", "@"); echo ":";
echo strstr(haystack: "hello world", needle: "lo", before_needle: true); echo ":";
echo strstr("hello", "x") === false ? "F" : "bad"; echo ":";
echo strstr("hello", ""); echo ":";
echo call_user_func("strstr", "abcabc", "bc"); echo ":";
echo call_user_func_array("strstr", ["haystack" => "abcabc", "needle" => "bc", "before_needle" => true]);
echo ":"; echo function_exists("strstr");');
"#,
    );
    assert_eq!(out, "@example.com:hel:F:hello:bcabc:a:1");
}

/// Verifies eval string boundary builtins support direct and callable byte-string checks.
#[test]
fn test_eval_dispatches_string_boundary_builtin_calls() {
    let out = compile_and_run(
        r#"<?php
eval('echo str_starts_with("Hello World", "Hello") ? "S" : "bad";
echo str_starts_with("Hello", "World") ? "bad" : ":s";
echo str_starts_with("Hello", "") ? ":se" : "bad";
echo str_ends_with("Hello World", "World") ? ":E" : "bad";
echo str_ends_with("Hello", "World") ? "bad" : ":e";
echo str_ends_with("Hello", "") ? ":ee" : "bad";
echo call_user_func("str_starts_with", "abc", "a") ? ":CS" : "bad";
echo call_user_func_array("str_ends_with", ["abc", "c"]) ? ":CE" : "bad";
echo ":"; echo function_exists("str_starts_with"); echo function_exists("str_ends_with");');
"#,
    );
    assert_eq!(out, "S:s:se:E:e:ee:CS:CE:11");
}

/// Verifies eval string comparison builtins return compatible scalar results.
#[test]
fn test_eval_dispatches_string_compare_builtin_calls() {
    let out = compile_and_run(
        r#"<?php
eval('echo strcmp("abc", "abc");
echo ":"; echo strcmp("abc", "abd") < 0 ? "lt" : "bad";
echo ":"; echo strcasecmp("Hello", "hello");
echo ":"; echo call_user_func("strcmp", "b", "a") > 0 ? "gt" : "bad";
echo ":"; echo call_user_func_array("strcasecmp", ["A", "a"]) === 0 ? "ci" : "bad";
echo ":"; echo hash_equals("abc", "abc") ? "heq" : "bad";
echo ":"; echo hash_equals("abc", "abcd") ? "bad" : "hlen";
echo ":"; echo call_user_func("hash_equals", "abc", "abd") ? "bad" : "hneq";
echo ":"; echo function_exists("strcmp"); echo function_exists("strcasecmp"); echo function_exists("hash_equals");');
"#,
    );
    assert_eq!(out, "0:lt:0:gt:ci:heq:hlen:hneq:111");
}

/// Verifies eval trim-like builtins strip default and explicit masks.
#[test]
fn test_eval_dispatches_trim_like_builtin_calls() {
    let out = compile_and_run(
        r#"<?php
eval('echo "[" . trim("  hello  ") . "]";
echo ":[" . ltrim("  left") . "]";
echo ":[" . rtrim("right  ") . "]";
echo ":[" . chop("tail... ", " .") . "]";
echo ":[" . trim("**boxed**", "*") . "]";
echo ":[" . call_user_func("trim", "  cuf  ") . "]";
echo ":[" . call_user_func_array("ltrim", ["0007", "0"]) . "]";
echo ":"; echo function_exists("trim"); echo function_exists("ltrim"); echo function_exists("rtrim"); echo function_exists("chop");');
"#,
    );
    assert_eq!(out, "[hello]:[left]:[right]:[tail]:[boxed]:[cuf]:[7]:1111");
}

/// Verifies eval scalar type-predicate builtins inspect boxed Mixed runtime tags.
#[test]
fn test_eval_dispatches_type_predicate_builtin_calls() {
    let out = compile_and_run(
        r#"<?php
$h = fopen("php://memory", "r+");
eval('echo is_int(1); echo is_integer(1); echo is_long(1);
echo is_float(1.5); echo is_double(1.5); echo is_real(1.5);
echo is_string("x"); echo is_bool(false); echo is_null(null);
echo is_array([1]); echo is_array(["a" => 1]);
echo is_iterable([1]); echo is_iterable(["a" => 1]);
echo is_iterable(1) ? "bad" : "T";
echo is_array(1) ? "bad" : "ok";
echo is_numeric(42); echo is_numeric(3.14); echo is_numeric("42");
echo is_numeric("-5"); echo is_numeric("3.14");
echo is_numeric("abc") ? "bad" : "N";
echo is_numeric(true) ? "bad" : "B";
echo is_resource(1) ? "bad" : "R";
echo is_nan(fdiv(0, 0)) ? "N" : "bad";
echo is_infinite(fdiv(1, 0)) ? "I" : "bad";
echo is_infinite(fdiv(-1, 0)) ? "i" : "bad";
echo is_finite(42) ? "F" : "bad";
echo is_finite(fdiv(1, 0)) ? "bad" : "f";
echo is_resource($h) ? "H" : "bad";
echo ":";
echo call_user_func("is_string", "x");
echo call_user_func_array("is_array", [[1]]);
echo call_user_func("is_numeric", "12");
echo call_user_func("is_iterable", [1]);
echo call_user_func_array("is_iterable", ["value" => 1]) ? "bad" : "t";
echo call_user_func("is_resource", $h);
echo call_user_func_array("is_resource", [$h]);
echo call_user_func("is_nan", fdiv(0, 0)) ? "N" : "bad";
echo call_user_func_array("is_finite", [42]) ? "F" : "bad";
echo function_exists("is_double"); echo function_exists("is_numeric"); echo function_exists("is_resource");
echo function_exists("is_nan"); echo function_exists("is_finite"); echo function_exists("is_iterable"); echo function_exists("is_infinite");');
"#,
    );
    assert_eq!(out, "1111111111111Tok11111NBRNIiFfH:1111t11NF1111111");
}

/// Verifies eval scalar cast builtins return boxed Mixed cells through direct and callable calls.
#[test]
fn test_eval_dispatches_cast_builtin_calls() {
    let out = compile_and_run(
        r#"<?php
eval('echo intval("42"); echo ":";
echo floatval("3.5"); echo ":";
echo strval(12); echo ":";
echo boolval("0") ? "bad" : "false";
echo ":"; echo call_user_func("strval", 7);
echo ":"; echo call_user_func_array("intval", ["9"]);
echo ":"; echo function_exists("boolval");');
"#,
    );
    assert_eq!(out, "42:3.5:12:false:7:9:1");
}

/// Verifies eval `gettype()` maps boxed Mixed runtime tags to PHP type names.
#[test]
fn test_eval_dispatches_gettype_builtin_call() {
    let out = compile_and_run(
        r#"<?php
eval('echo gettype(1); echo ":";
echo gettype(1.5); echo ":";
echo gettype("x"); echo ":";
echo gettype(false); echo ":";
echo gettype(null); echo ":";
echo gettype([1]); echo ":";
echo gettype(["a" => 1]); echo ":";
echo call_user_func("gettype", true); echo ":";
echo call_user_func_array("gettype", [null]);
echo ":"; echo function_exists("gettype");');
"#,
    );
    assert_eq!(
        out,
        "integer:double:string:boolean:NULL:array:array:boolean:NULL:1"
    );
}

/// Verifies eval `define()` and `defined()` share dynamic constant names across fragments.
#[test]
fn test_eval_define_and_defined_dynamic_constants() {
    let out = compile_and_run_capture(
        r#"<?php
echo eval('return define("DynEvalConst", 7) ? "Y" : "N";');
echo eval('return defined("DynEvalConst") ? "Y" : "N";');
echo eval('return DynEvalConst;');
echo eval('return \DynEvalConst;');
echo eval('return defined("dynevalconst") ? "bad" : "N";');
echo eval('return define("DynEvalConst", 8) ? "bad" : "N";');
echo eval('return define(value: 9, constant_name: "DynEvalNamedConst") ? "Y" : "N";');
echo eval('return defined(constant_name: "DynEvalNamedConst") ? "Y" : "N";');
echo eval('return call_user_func("defined", "DynEvalConst") ? "Y" : "N";');
echo eval('return call_user_func_array("defined", ["constant_name" => "DynEvalConst"]) ? "Y" : "N";');
echo eval('return function_exists("define") && function_exists("defined") ? "Y" : "N";');
"#,
    );
    assert!(out.success, "program failed: {}", out.stderr);
    assert_eq!(out.stdout, "YY77NNYYYYY");
    assert!(
        out.stderr
            .contains("Warning: define(): Constant already defined"),
        "expected duplicate eval define warning, got stderr={}",
        out.stderr
    );
}

/// Verifies eval can read predefined runtime constants and protect them from redefinition.
#[test]
fn test_eval_reads_predefined_runtime_constants() {
    let out = compile_and_run_capture(
        r#"<?php
echo eval('return (PHP_EOL === "\n" ? "eol" : "bad") . ":" .
    ((PHP_OS === "Darwin" || PHP_OS === "Linux") ? "os" : "bad") . ":" .
    DIRECTORY_SEPARATOR . ":" .
    (PHP_INT_MAX > 9000000000000000000 ? "int" : "bad") . ":" .
    (defined("PHP_OS") ? "defined" : "bad") . ":" .
    (defined("\\\\PHP_OS") ? "root" : "bad") . ":" .
    (defined("php_os") ? "bad" : "case") . ":" .
    (define("PHP_OS", "x") ? "bad" : "locked");');
"#,
    );
    assert!(out.success, "program failed: {}", out.stderr);
    assert_eq!(out.stdout, "eol:os:/:int:defined:root:case:locked");
    assert!(
        out.stderr
            .contains("Warning: define(): Constant already defined"),
        "expected predefined eval define warning, got stderr={}",
        out.stderr
    );
}

/// Verifies `@eval(...)` suppresses duplicate eval `define()` warnings.
#[test]
fn test_error_control_suppresses_duplicate_eval_define_warning() {
    let out = compile_and_run_capture(
        r#"<?php
eval('define("DynEvalSuppressedConst", 1);');
echo @eval('return define("DynEvalSuppressedConst", 2) ? "bad" : "ok";');
echo eval('return DynEvalSuppressedConst;');
"#,
    );
    assert!(out.success, "program failed: {}", out.stderr);
    assert_eq!(out.stdout, "ok1");
    assert_eq!(out.stderr, "");
}

/// Verifies native `defined()` probes can see constants defined by eval after the barrier.
#[test]
fn test_eval_defined_constant_is_visible_to_native_defined_after_barrier() {
    let out = compile_and_run(
        r#"<?php
echo defined("DynEvalNativeDefinedConst") ? "bad" : "N";
eval('define("DynEvalNativeDefinedConst", 5);');
echo defined("DynEvalNativeDefinedConst") ? "Y" : "N";
echo defined("\\DynEvalNativeDefinedConst") ? "Y" : "N";
echo defined("dynevalnativedefinedconst") ? "bad" : "N";
"#,
    );
    assert_eq!(out, "NYYN");
}

/// Verifies native constant fetch can read eval-defined constants after the barrier.
#[test]
fn test_eval_defined_constant_is_visible_to_native_constant_fetch_after_barrier() {
    let out = compile_and_run(
        r#"<?php
eval('define("DynEvalNativeFetchConst", "dynamic");');
echo DynEvalNativeFetchConst;
"#,
    );
    assert_eq!(out, "dynamic");
}

/// Verifies native constant fetch misses after eval fail through the eval runtime path.
#[test]
fn test_eval_missing_native_dynamic_constant_fetch_fails() {
    let err = compile_and_run_expect_failure("<?php eval(''); echo MissingNativeEvalConst;");
    assert!(
        err.contains("Fatal error: eval() runtime failed"),
        "stderr did not contain eval runtime fatal diagnostic: {err}"
    );
}

/// Verifies missing eval dynamic constants fail through the eval runtime path.
#[test]
fn test_eval_missing_dynamic_constant_fetch_fails() {
    let err = compile_and_run_expect_failure("<?php eval('return MissingEvalConst;');");
    assert!(
        err.contains("Fatal error: eval() runtime failed"),
        "stderr did not contain eval runtime fatal diagnostic: {err}"
    );
}

/// Verifies invalid eval fragments report the dedicated parse-error diagnostic.
#[test]
fn test_eval_parse_error_reports_eval_parse_diagnostic() {
    let err = compile_and_run_expect_failure("<?php eval('if (');");
    assert!(
        err.contains("Parse error: eval() fragment is invalid"),
        "stderr did not contain eval parse-error diagnostic: {err}"
    );
}

/// Verifies eval `abs()` preserves integer/float result typing through direct and callable calls.
#[test]
fn test_eval_dispatches_abs_builtin_call() {
    let out = compile_and_run(
        r#"<?php
eval('echo abs(-5); echo ":";
echo abs(-2.5); echo ":";
echo gettype(abs(-2.5)); echo ":";
echo call_user_func("abs", -7); echo ":";
echo call_user_func_array("abs", [-9]);
echo ":"; echo function_exists("abs");');
"#,
    );
    assert_eq!(out, "5:2.5:double:7:9:1");
}

/// Verifies eval `floor()` and `ceil()` return boxed double cells through direct and callable calls.
#[test]
fn test_eval_dispatches_floor_and_ceil_builtin_calls() {
    let out = compile_and_run(
        r#"<?php
eval('echo floor(3.7); echo ":";
echo gettype(floor(3)); echo ":";
echo ceil(3.2); echo ":";
echo gettype(ceil(3)); echo ":";
echo call_user_func("floor", 4.9); echo ":";
echo call_user_func_array("ceil", [4.1]);
echo ":"; echo function_exists("floor"); echo function_exists("ceil");');
"#,
    );
    assert_eq!(out, "3:double:4:double:4:5:11");
}

/// Verifies eval `fdiv()` and `fmod()` return boxed double cells through direct and callable calls.
#[test]
fn test_eval_dispatches_float_binary_builtin_calls() {
    let out = compile_and_run(
        r#"<?php
eval('echo fdiv(10, 4); echo ":";
echo gettype(fdiv(10, 4)); echo ":";
echo fdiv(1, 0); echo ":";
echo fdiv(0, 0); echo ":";
echo round(fmod(10.5, 3.2), 1); echo ":";
echo round(call_user_func("fdiv", 9, 2), 1); echo ":";
echo round(call_user_func_array("fmod", [10.5, 3.2]), 1);
echo ":"; echo function_exists("fdiv"); echo function_exists("fmod");');
"#,
    );
    assert_eq!(out, "2.5:double:INF:NAN:0.9:4.5:0.9:11");
}

/// Verifies eval extended scalar math builtins through direct, named, callable, and probe paths.
#[test]
fn test_eval_dispatches_extended_math_builtin_calls() {
    let out = compile_and_run(
        r#"<?php
eval('echo sin(0); echo ":";
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
echo function_exists("sin"); echo function_exists("log"); echo function_exists("intdiv"); echo function_exists("hypot");');
"#,
    );
    assert_eq!(
        out,
        "0:1:0:1.57:0:0.79:0:1:0:3:2:1:3.14:180:3:0:5:3:1:4:1111"
    );
}

/// Verifies eval `pow()` reuses exponentiation runtime hooks through direct and callable calls.
#[test]
fn test_eval_dispatches_pow_builtin_call() {
    let out = compile_and_run(
        r#"<?php
eval('echo pow(2, 3); echo ":";
echo gettype(pow(2, 3)); echo ":";
echo call_user_func("pow", 2, 5); echo ":";
echo call_user_func_array("pow", [3, 3]);
echo ":"; echo function_exists("pow");');
"#,
    );
    assert_eq!(out, "8:double:32:27:1");
}

/// Verifies eval `round()` supports default and explicit precision through callable paths.
#[test]
fn test_eval_dispatches_round_builtin_call() {
    let out = compile_and_run(
        r#"<?php
eval('echo round(3.5); echo ":";
echo round(3.14159, 2); echo ":";
echo gettype(round(3)); echo ":";
echo call_user_func("round", 2.5); echo ":";
echo call_user_func_array("round", [1.55, 1]);
echo ":"; echo function_exists("round");');
"#,
    );
    assert_eq!(out, "4:3.14:double:3:1.6:1");
}

/// Verifies eval `number_format()` groups and rounds numbers through callable paths.
#[test]
fn test_eval_dispatches_number_format_builtin_call() {
    let out = compile_and_run(
        r#"<?php
eval('echo number_format(1234567); echo ":";
echo number_format(1234.5678, 2); echo ":";
echo number_format(num: 1234567.89, decimals: 2, decimal_separator: ",", thousands_separator: "."); echo ":";
echo number_format(1234567.89, 2, ".", ""); echo ":";
echo call_user_func("number_format", -1234.5, 1); echo ":";
echo call_user_func_array("number_format", ["num" => 1234, "decimals" => 0, "decimal_separator" => ".", "thousands_separator" => " "]);
echo ":"; echo function_exists("number_format");');
"#,
    );
    assert_eq!(
        out,
        "1,234,567:1,234.57:1.234.567,89:1234567.89:-1,234.5:1 234:1"
    );
}

/// Verifies eval printf-family builtins format, print, and dispatch through callables.
#[test]
fn test_eval_dispatches_printf_family_builtin_calls() {
    let out = compile_and_run(
        r#"<?php
eval('echo sprintf("Hello %s", "World"); echo ":";
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
echo function_exists("sprintf"); echo is_callable("printf"); echo function_exists("vsprintf"); echo is_callable("vprintf");');
"#,
    );
    assert_eq!(
        out,
        "Hello World:00042:3.14:hi    |:n=42:4:age/42/3.0:v-7:3:+42:spread:1111"
    );
}

/// Verifies eval `min()` and `max()` select numeric values directly and through callables.
#[test]
fn test_eval_dispatches_min_max_builtin_calls() {
    let out = compile_and_run(
        r#"<?php
eval('echo min(3, 1, 2); echo ":";
echo max(1, 3, 2); echo ":";
echo min(2.5, 1.5); echo ":";
echo max(1.5, 2.5); echo ":";
echo call_user_func("min", 9, 4, 7); echo ":";
echo call_user_func_array("max", [4, 8, 6]);
echo ":"; echo function_exists("min"); echo function_exists("max");');
"#,
    );
    assert_eq!(out, "1:3:1.5:2.5:4:8:11");
}

/// Verifies eval `clamp()` selects numeric values directly and through callables.
#[test]
fn test_eval_dispatches_clamp_builtin_calls() {
    let out = compile_and_run(
        r#"<?php
eval('echo clamp(5, 0, 10); echo ":";
echo clamp(15, 0, 10); echo ":";
echo clamp(-5, 0, 10); echo ":";
echo clamp(2.75, 1.5, 2.5); echo ":";
echo clamp(value: 8, min: 0, max: 5); echo ":";
echo call_user_func("clamp", -1, 0, 10); echo ":";
echo call_user_func_array("clamp", ["value" => 9, "min" => 0, "max" => 7]);
echo ":"; echo function_exists("clamp"); echo is_callable("clamp");');
"#,
    );
    assert_eq!(out, "5:10:0:2.5:5:0:7:11");
}

/// Verifies eval `pi()` returns the PHP math constant through direct and callable calls.
#[test]
fn test_eval_dispatches_pi_builtin_call() {
    let out = compile_and_run(
        r#"<?php
eval('echo round(pi(), 2); echo ":";
echo gettype(pi()); echo ":";
echo round(call_user_func("pi"), 3); echo ":";
echo round(call_user_func_array("pi", []), 4);
echo ":"; echo function_exists("pi");');
"#,
    );
    assert_eq!(out, "3.14:double:3.142:3.1416:1");
}

/// Verifies eval `sqrt()` returns boxed double cells through direct and callable calls.
#[test]
fn test_eval_dispatches_sqrt_builtin_call() {
    let out = compile_and_run(
        r#"<?php
eval('echo sqrt(16); echo ":";
echo gettype(sqrt(9)); echo ":";
echo call_user_func("sqrt", 25); echo ":";
echo call_user_func_array("sqrt", [36]);
echo ":"; echo function_exists("sqrt");');
"#,
    );
    assert_eq!(out, "4:double:5:6:1");
}

/// Verifies eval `isset()` distinguishes missing, null, and falsey non-null values.
#[test]
fn test_eval_isset_distinguishes_missing_null_and_falsey_values() {
    let out = compile_and_run(
        r#"<?php
$nullish = null;
$zero = 0;
$empty = "";
eval('if (isset($missing)) { echo "1"; } else { echo "0"; }
if (isset($nullish)) { echo "1"; } else { echo "0"; }
if (isset($zero)) { echo "1"; } else { echo "0"; }
if (isset($empty)) { echo "1"; } else { echo "0"; }
if (isset($zero, $empty)) { echo "1"; } else { echo "0"; }
if (isset($zero, $nullish)) { echo "1"; } else { echo "0"; }
echo function_exists("isset") . "x";');
"#,
    );
    assert_eq!(out, "001110x");
}

/// Verifies eval `empty()` uses PHP truthiness without warning on missing variables.
#[test]
fn test_eval_empty_uses_php_truthiness_without_missing_warnings() {
    let out = compile_and_run(
        r#"<?php
$nullish = null;
$zero = 0;
$empty = "";
$zero_string = "0";
$value = "x";
eval('if (empty($missing)) { echo "1"; } else { echo "0"; }
if (empty($nullish)) { echo "1"; } else { echo "0"; }
if (empty($zero)) { echo "1"; } else { echo "0"; }
if (empty($empty)) { echo "1"; } else { echo "0"; }
if (empty($zero_string)) { echo "1"; } else { echo "0"; }
if (empty($value)) { echo "1"; } else { echo "0"; }
echo function_exists("empty") . "x";');
"#,
    );
    assert_eq!(out, "111110x");
}

/// Verifies eval `isset()` and `empty()` use PHP offset semantics for array reads.
#[test]
fn test_eval_isset_and_empty_support_array_offsets() {
    let out = compile_and_run(
        r#"<?php
$map = eval('return [
    "present" => "x",
    "nullish" => null,
    "zero" => 0,
    "empty" => "",
    "child" => ["leaf" => "ok", "null" => null],
];');
eval('echo isset($map["present"]) ? "1" : "0";
echo isset($map["nullish"]) ? "1" : "0";
echo isset($map["missing"]) ? "1" : "0";
echo isset($map["zero"]) ? "1" : "0";
echo isset($map["child"]["leaf"]) ? "1" : "0";
echo isset($map["child"]["null"]) ? "1" : "0";
echo isset($map["missing"]["leaf"]) ? "1" : "0";
echo ":";
echo empty($map["present"]) ? "1" : "0";
echo empty($map["nullish"]) ? "1" : "0";
echo empty($map["missing"]) ? "1" : "0";
echo empty($map["zero"]) ? "1" : "0";
echo empty($map["empty"]) ? "1" : "0";
echo empty($map["child"]["leaf"]) ? "1" : "0";
echo empty($map["child"]["null"]) ? "1" : "0";
echo empty($map["missing"]["leaf"]) ? "1" : "0";');
"#,
    );
    assert_eq!(out, "1001100:01111011");
}

/// Verifies eval builtin dispatch can inspect arrays from the caller scope.
#[test]
fn test_eval_count_reads_scope_array() {
    let out = compile_and_run(
        r#"<?php
$items = eval('return ["a", "b"];');
eval('echo count($items);');
"#,
    );
    assert_eq!(out, "2");
}

/// Verifies eval-declared functions can be called inside the same fragment.
#[test]
fn test_eval_declared_function_can_be_called_in_fragment() {
    let out = compile_and_run(
        r#"<?php
echo eval('function dyn_eval_add($x) { return $x + 1; } return dyn_eval_add(4);');
"#,
    );
    assert_eq!(out, "5");
}

/// Verifies eval-declared functions bind named arguments inside eval fragments.
#[test]
fn test_eval_declared_function_accepts_named_args_in_fragment() {
    let out = compile_and_run(
        r#"<?php
echo eval('function dyn_eval_named($x, $y) { return ($x * 10) + $y; } return dyn_eval_named(y: 2, x: 1);');
"#,
    );
    assert_eq!(out, "12");
}

/// Verifies eval-declared functions unpack spread arguments inside eval fragments.
#[test]
fn test_eval_declared_function_accepts_spread_args_in_fragment() {
    let out = compile_and_run(
        r#"<?php
echo eval('function dyn_eval_spread($x, $y) { return ($x * 10) + $y; } return dyn_eval_spread(...[1, 2]);');
"#,
    );
    assert_eq!(out, "12");
}

/// Verifies eval magic constants use fragment line and eval-declared function metadata.
#[test]
fn test_eval_magic_line_function_and_method_execute_through_bridge() {
    let out = compile_and_run(
        r#"<?php
eval("
echo __LINE__ . ':';
");
eval('function DynEvalMagic() { return __FUNCTION__ . ":" . __METHOD__; } echo dynevalmagic();');
"#,
    );
    assert_eq!(out, "2:DynEvalMagic:DynEvalMagic");
}

/// Verifies eval file-dependent magic constants receive generated call-site metadata.
#[test]
fn test_eval_magic_file_and_dir_execute_through_bridge() {
    let out = compile_and_run(
        r#"<?php
eval('if (strlen(__DIR__) > 0) { echo "D"; } else { echo "d"; }
echo ":";
if (strlen(__FILE__) > strlen(__DIR__)) { echo "F"; } else { echo "f"; }');
"#,
    );
    assert_eq!(out, "D:F");
}

/// Verifies eval scope magic constants are empty even from namespaced method callers.
#[test]
fn test_eval_scope_magic_constants_are_empty_strings() {
    let out = compile_and_run(
        r#"<?php
namespace EvalMagicScope;
class Box {
    public function run() {
        eval('echo "[" . __CLASS__ . "|" . __NAMESPACE__ . "|" . __TRAIT__ . "]";');
    }
}
(new Box())->run();
"#,
    );
    assert_eq!(out, "[||]");
}

/// Verifies eval-declared functions persist across eval calls in the same generated context.
#[test]
fn test_eval_declared_function_persists_across_eval_calls() {
    let out = compile_and_run(
        r#"<?php
eval('function dyn_eval_inc($x) { return $x + 1; }');
eval('echo dyn_eval_inc(4);');
"#,
    );
    assert_eq!(out, "5");
}

/// Verifies native code can call a zero-argument function declared by eval.
#[test]
fn test_eval_declared_function_can_be_called_from_native_code() {
    let out = compile_and_run(
        r#"<?php
eval('function dyn_eval_native() { return 42; }');
echo dyn_eval_native();
"#,
    );
    assert_eq!(out, "42");
}

/// Verifies static locals in eval-declared functions persist between native calls.
#[test]
fn test_eval_declared_function_static_local_persists() {
    let out = compile_and_run(
        r#"<?php
eval('function dyn_eval_static_counter() { static $n = 0; $n++; return $n; }');
echo dyn_eval_static_counter();
echo ":";
echo dyn_eval_static_counter();
"#,
    );
    assert_eq!(out, "1:2");
}

/// Verifies top-level static locals inside separate eval calls are reinitialized like PHP.
#[test]
fn test_eval_top_level_static_var_reinitializes_per_eval_call() {
    let out = compile_and_run(
        r#"<?php
eval('static $n = 0; $n++; echo $n;');
echo ":";
eval('static $n = 0; $n++; echo $n;');
"#,
    );
    assert_eq!(out, "1:1");
}

/// Verifies `global` inside eval can write compiler-known global storage.
#[test]
fn test_eval_global_alias_updates_global_storage() {
    let out = compile_and_run(
        r#"<?php
$g = 1;
function bump_eval_global() {
    global $g;
    eval('global $g; $g = $g + 1;');
}
bump_eval_global();
echo $g;
"#,
    );
    assert_eq!(out, "2");
}

/// Verifies a function can read a global alias after eval mutates that global.
#[test]
fn test_eval_global_alias_read_after_eval_in_same_function() {
    let out = compile_and_run(
        r#"<?php
$g = 1;
function bump_eval_global_and_read() {
    global $g;
    eval('global $g; $g = $g + 1;');
    echo $g;
}
bump_eval_global_and_read();
echo ":" . $g;
"#,
    );
    assert_eq!(out, "2:2");
}

/// Verifies unsetting an eval global alias does not unset the actual global value.
#[test]
fn test_eval_global_alias_unset_keeps_global_storage() {
    let out = compile_and_run(
        r#"<?php
$g = 1;
function unset_eval_global_alias() {
    global $g;
    eval('global $g; unset($g);');
}
unset_eval_global_alias();
echo $g;
"#,
    );
    assert_eq!(out, "1");
}

/// Verifies eval references to global aliases update the source global storage.
#[test]
fn test_eval_reference_alias_to_global_updates_global_storage() {
    let out = compile_and_run(
        r#"<?php
$g = 1;
function ref_eval_global_alias() {
    global $g;
    eval('$alias =& $g; $alias = 4;');
}
ref_eval_global_alias();
echo $g;
"#,
    );
    assert_eq!(out, "4");
}

/// Verifies top-level eval fragments can read CLI `$argc` and `$argv`.
#[test]
fn test_eval_top_level_reads_argc_argv() {
    let out = compile_and_run(
        r#"<?php
eval('echo $argc . ":" . count($argv) . ":" . (strlen($argv[0]) > 0 ? "Y" : "N");');
"#,
    );
    assert_eq!(out, "1:1:Y");
}

/// Verifies top-level eval can replace `$argc` after the eval barrier widens it.
#[test]
fn test_eval_top_level_can_replace_argc_type() {
    let out = compile_and_run(
        r#"<?php
eval('$argc = "changed";');
echo $argc;
"#,
    );
    assert_eq!(out, "changed");
}

/// Verifies eval `global` aliases can read CLI argument globals inside functions.
#[test]
fn test_eval_global_alias_reads_argc_argv_in_function() {
    let out = compile_and_run(
        r#"<?php
function show_eval_process_args() {
    eval('global $argc, $argv; echo $argc . ":" . count($argv) . ":" . (strlen($argv[0]) > 0 ? "Y" : "N");');
}
show_eval_process_args();
"#,
    );
    assert_eq!(out, "1:1:Y");
}

/// Verifies functions declared by eval from a namespace are registered globally.
#[test]
fn test_eval_declared_function_in_namespace_is_global() {
    let out = compile_and_run(
        r#"<?php
namespace EvalNs;
eval('function dyn_eval_ns_global() { return 42; }');
echo function_exists('EvalNs\\dyn_eval_ns_global') ? '1' : '0';
echo ":";
echo function_exists('dyn_eval_ns_global') ? '1' : '0';
echo ":";
echo \dyn_eval_ns_global();
"#,
    );
    assert_eq!(out, "0:1:42");
}

/// Verifies namespace declarations inside eval qualify dynamic declarations and fall back to builtins.
#[test]
fn test_eval_fragment_namespace_declares_qualified_function() {
    let out = compile_and_run(
        r#"<?php
eval('namespace EvalInnerNs;
function dyn_eval_inner_ns() { return __NAMESPACE__ . ":" . __FUNCTION__; }
echo dyn_eval_inner_ns();
echo ":" . strlen("abcd");');
echo ":";
echo function_exists("EvalInnerNs\\dyn_eval_inner_ns") ? "Y" : "N";
echo ":";
echo call_user_func("EvalInnerNs\\dyn_eval_inner_ns");
"#,
    );
    assert_eq!(
        out,
        "EvalInnerNs:EvalInnerNs\\dyn_eval_inner_ns:4:Y:EvalInnerNs:EvalInnerNs\\dyn_eval_inner_ns"
    );
}

/// Verifies native calls can pass positional arguments to eval-declared functions.
#[test]
fn test_eval_declared_function_native_call_accepts_positional_args() {
    let out = compile_and_run(
        r#"<?php
eval('function dyn_eval_native_add($x, $y) { return $x + $y; }');
echo dyn_eval_native_add(4, 5);
"#,
    );
    assert_eq!(out, "9");
}

/// Verifies `call_user_func()` can dispatch to an eval-declared function after the barrier.
#[test]
fn test_eval_declared_function_can_be_called_with_call_user_func() {
    let out = compile_and_run(
        r#"<?php
eval('function dyn_eval_cuf($x) { return $x + 1; }');
echo call_user_func('dyn_eval_cuf', 4);
"#,
    );
    assert_eq!(out, "5");
}

/// Verifies post-barrier `call_user_func_array()` can dispatch to eval-declared functions.
#[test]
fn test_eval_declared_function_can_be_called_with_call_user_func_array() {
    let out = compile_and_run(
        r#"<?php
eval('function dyn_eval_cufa($x, $y) { return ($x * 10) + $y; }');
echo call_user_func_array('dyn_eval_cufa', ['y' => 2, 'x' => 1]);
$args = ['y' => 3, 'x' => 2];
echo ":" . call_user_func_array('dyn_eval_cufa', $args);
"#,
    );
    assert_eq!(out, "12:23");
}

/// Verifies `call_user_func()` inside eval can dispatch to an eval-declared function.
#[test]
fn test_eval_fragment_call_user_func_dispatches_eval_declared_function() {
    let out = compile_and_run(
        r#"<?php
eval('function dyn_eval_inner_cuf($x) { return $x + 1; }
echo call_user_func("dyn_eval_inner_cuf", 4);');
"#,
    );
    assert_eq!(out, "5");
}

/// Verifies `call_user_func()` inside eval can dispatch to supported builtins.
#[test]
fn test_eval_fragment_call_user_func_dispatches_builtin() {
    let out = compile_and_run(
        r#"<?php
eval('echo call_user_func("strlen", "abcd");
echo ":";
echo function_exists("call_user_func");');
"#,
    );
    assert_eq!(out, "4:1");
}

/// Verifies `call_user_func()` inside eval can dispatch to registered AOT functions.
#[test]
fn test_eval_fragment_call_user_func_dispatches_native_user_function() {
    let out = compile_and_run(
        r#"<?php
function native_eval_cuf_add($x, $y) { return $x + $y; }
eval('echo call_user_func("native_eval_cuf_add", 4, 6);');
"#,
    );
    assert_eq!(out, "10");
}

/// Verifies variable call syntax inside eval dispatches supported builtin callables.
#[test]
fn test_eval_fragment_variable_callable_dispatches_builtin() {
    let out = compile_and_run(
        r#"<?php
eval('$fn = "strlen";
echo $fn("abcd") . ":";
$callbacks = ["strtoupper"];
echo $callbacks[0]("xy");');
"#,
    );
    assert_eq!(out, "4:XY");
}

/// Verifies variable call syntax inside eval dispatches eval-declared functions with named args.
#[test]
fn test_eval_fragment_variable_callable_dispatches_eval_declared_function() {
    let out = compile_and_run(
        r#"<?php
eval('function dyn_eval_var_callable($x, $y) { return ($x * 10) + $y; }
$fn = "dyn_eval_var_callable";
echo $fn(y: 2, x: 1);');
"#,
    );
    assert_eq!(out, "12");
}

/// Verifies variable call syntax inside eval dispatches registered AOT user functions.
#[test]
fn test_eval_fragment_variable_callable_dispatches_native_user_function() {
    let out = compile_and_run(
        r#"<?php
function native_eval_var_callable($left, $right) { return $left . ":" . $right; }
eval('$fn = "native_eval_var_callable";
echo $fn(right: "R", left: "L");');
"#,
    );
    assert_eq!(out, "L:R");
}

/// Verifies `call_user_func_array()` inside eval dispatches to eval-declared functions.
#[test]
fn test_eval_fragment_call_user_func_array_dispatches_eval_declared_function() {
    let out = compile_and_run(
        r#"<?php
eval('function dyn_eval_inner_cufa($x, $y) { return $x + $y; }
echo call_user_func_array("dyn_eval_inner_cufa", [4, 5]);');
"#,
    );
    assert_eq!(out, "9");
}

/// Verifies `call_user_func_array()` inside eval binds eval-declared named arguments.
#[test]
fn test_eval_fragment_call_user_func_array_binds_eval_declared_named_args() {
    let out = compile_and_run(
        r#"<?php
eval('function dyn_eval_inner_cufa_named($x, $y) { return ($x * 10) + $y; }
echo call_user_func_array("dyn_eval_inner_cufa_named", ["y" => 2, "x" => 1]);');
"#,
    );
    assert_eq!(out, "12");
}

/// Verifies `call_user_func_array()` inside eval dispatches to supported builtins.
#[test]
fn test_eval_fragment_call_user_func_array_dispatches_builtin() {
    let out = compile_and_run(
        r#"<?php
eval('echo call_user_func_array("strlen", ["abcd"]);
echo ":";
echo function_exists("call_user_func_array");');
"#,
    );
    assert_eq!(out, "4:1");
}

/// Verifies `call_user_func_array()` inside eval dispatches to registered AOT functions.
#[test]
fn test_eval_fragment_call_user_func_array_dispatches_native_user_function() {
    let out = compile_and_run(
        r#"<?php
function native_eval_cufa_add($x, $y) { return $x + $y; }
eval('echo call_user_func_array("native_eval_cufa_add", [4, 6]);');
"#,
    );
    assert_eq!(out, "10");
}

/// Verifies `call_user_func_array()` inside eval binds registered AOT named arguments.
#[test]
fn test_eval_fragment_call_user_func_array_binds_native_user_function_named_args() {
    let out = compile_and_run(
        r#"<?php
function native_eval_cufa_named($left, $right) { return $left . ":" . $right; }
eval('echo call_user_func_array("native_eval_cufa_named", ["right" => "R", "left" => "L"]);');
"#,
    );
    assert_eq!(out, "L:R");
}

/// Verifies eval fragments can call AOT user functions registered in the eval context.
#[test]
fn test_eval_fragment_can_call_native_user_function() {
    let out = compile_and_run(
        r#"<?php
function native_eval_add($x, $y) { return $x + $y; }
eval('echo native_eval_add(4, 6); echo ":"; echo function_exists("native_eval_add");');
"#,
    );
    assert_eq!(out, "10:1");
}

/// Verifies eval fragments bind AOT user function parameters by name.
#[test]
fn test_eval_fragment_can_call_native_user_function_with_named_args() {
    let out = compile_and_run(
        r#"<?php
function native_eval_named($left, $right) { return $left . ":" . $right; }
eval('echo native_eval_named(right: "R", left: "L");');
"#,
    );
    assert_eq!(out, "L:R");
}

/// Verifies eval fragments can unpack arrays into AOT user function calls.
#[test]
fn test_eval_fragment_can_call_native_user_function_with_spread_args() {
    let out = compile_and_run(
        r#"<?php
function native_eval_spread($left, $right) { return $left . ":" . $right; }
eval('echo native_eval_spread(...["L", "R"]);');
"#,
    );
    assert_eq!(out, "L:R");
}

/// Verifies eval fragments called from methods can mutate public properties through `$this`.
#[test]
fn test_eval_fragment_can_mutate_this_public_property() {
    let out = compile_and_run(
        r#"<?php
class EvalPropBox {
    public int $x = 1;

    public function bump(): void {
        eval('$this->x = $this->x + 1;');
    }
}

$box = new EvalPropBox();
$box->bump();
echo $box->x;
"#,
    );
    assert_eq!(out, "2");
}

/// Verifies eval keeps PHP property names case-sensitive while parsing keywords case-insensitively.
#[test]
fn test_eval_fragment_preserves_this_property_case() {
    let out = compile_and_run(
        r#"<?php
class EvalCasePropBox {
    public int $camelName = 42;

    public function read(): void {
        echo eval('RETURN $this->camelName;');
    }
}

$box = new EvalCasePropBox();
$box->read();
"#,
    );
    assert_eq!(out, "42");
}

/// Verifies eval fragments can call public zero-argument AOT methods through `$this`.
#[test]
fn test_eval_fragment_can_call_this_public_zero_arg_method() {
    let out = compile_and_run(
        r#"<?php
class EvalMethodBox {
    public int $x = 41;

    public function answer(): int {
        return $this->x + 1;
    }

    public function run(): void {
        echo eval('return $this->answer();');
    }
}

$box = new EvalMethodBox();
$box->run();
"#,
    );
    assert_eq!(out, "42");
}

/// Verifies eval fragments pass one scalar argument to public AOT methods through `$this`.
#[test]
fn test_eval_fragment_can_call_this_public_one_arg_method() {
    let out = compile_and_run(
        r#"<?php
class EvalMethodArgBox {
    public int $x = 41;

    public function add(int $amount): int {
        return $this->x + $amount;
    }

    public function run(): void {
        echo eval('return $this->add(9);');
    }
}

$box = new EvalMethodArgBox();
$box->run();
"#,
    );
    assert_eq!(out, "50");
}

/// Verifies eval fragments pass two scalar arguments to public AOT methods through `$this`.
#[test]
fn test_eval_fragment_can_call_this_public_two_arg_method() {
    let out = compile_and_run(
        r#"<?php
class EvalMethodTwoArgBox {
    public int $x = 41;

    public function label(int $amount, string $suffix): string {
        return ($this->x + $amount) . $suffix;
    }

    public function run(): void {
        echo eval('return $this->label(9, "!");');
    }
}

$box = new EvalMethodTwoArgBox();
$box->run();
"#,
    );
    assert_eq!(out, "50!");
}

/// Verifies eval fragments can unpack numeric arrays into public AOT method calls.
#[test]
fn test_eval_fragment_can_call_this_public_method_with_spread_args() {
    let out = compile_and_run(
        r#"<?php
class EvalMethodSpreadBox {
    public int $x = 41;

    public function label(int $amount, string $suffix): string {
        return ($this->x + $amount) . $suffix;
    }

    public function run(): void {
        echo eval('return $this->label(...[9, "!"]);');
    }
}

$box = new EvalMethodSpreadBox();
$box->run();
"#,
    );
    assert_eq!(out, "50!");
}

/// Verifies eval callable arrays dispatch public AOT methods through all dynamic call surfaces.
#[test]
fn test_eval_fragment_callable_array_dispatches_this_public_method() {
    let out = compile_and_run(
        r#"<?php
class EvalCallableArrayBox {
    public int $x = 40;

    public function label(int $amount, string $suffix): string {
        return ($this->x + $amount) . $suffix;
    }

    public function run(): void {
        echo eval('$cb = [$this, "label"];
echo $cb(1, "a");
echo ":";
echo call_user_func($cb, 2, "b");
echo ":";
return call_user_func_array($cb, [3, "c"]);');
    }
}

$box = new EvalCallableArrayBox();
$box->run();
"#,
    );
    assert_eq!(out, "41a:42b:43c");
}

/// Verifies native callable probes can see functions declared by eval after the barrier.
#[test]
fn test_eval_declared_function_is_visible_to_callable_probes() {
    let out = compile_and_run(
        r#"<?php
eval('function dyn_eval_probe() { return 1; }');
echo function_exists('dyn_eval_probe') ? '1' : '0';
echo is_callable('DYN_EVAL_PROBE') ? '1' : '0';
echo function_exists('missing_eval_probe') ? '1' : '0';
"#,
    );
    assert_eq!(out, "110");
}

/// Verifies callable probes inside eval inspect dynamic functions and supported builtins.
#[test]
fn test_eval_fragment_function_probes_use_dynamic_context() {
    let out = compile_and_run(
        r#"<?php
eval('function dyn_eval_inner_probe() { return 1; }
echo function_exists("DYN_EVAL_INNER_PROBE") . "x";
echo is_callable("dyn_eval_inner_probe") . "x";
echo function_exists("strlen") . "x";
echo function_exists("eval") . "x";
echo function_exists("missing_eval_inner_probe") . "x";');
"#,
    );
    assert_eq!(out, "1x1x1xxx");
}

/// Verifies eval `class_exists()` probes generated AOT class-name metadata.
#[test]
fn test_eval_fragment_class_exists_probes_aot_classes() {
    let out = compile_and_run(
        r#"<?php
class EvalClassExistsProbe {}
eval('echo class_exists("EvalClassExistsProbe") ? "Y" : "N";
echo class_exists("evalclassexistsprobe") ? "Y" : "N";
echo class_exists("\EvalClassExistsProbe") ? "Y" : "N";
echo call_user_func("class_exists", "EvalClassExistsProbe") ? "Y" : "N";
echo call_user_func_array("class_exists", ["autoload" => false, "class" => "\EvalClassExistsProbe"]) ? "Y" : "N";
echo class_exists(class: "MissingEvalClassExistsProbe", autoload: false) ? "Y" : "N";');
"#,
    );
    assert_eq!(out, "YYYYYN");
}

/// Verifies duplicate eval-declared functions fail through the runtime bridge.
#[test]
fn test_eval_duplicate_declared_function_fails() {
    let err = compile_and_run_expect_failure(
        r#"<?php
eval('function dyn_eval_dup() { return 1; }');
eval('function dyn_eval_dup() { return 2; }');
"#,
    );
    assert!(
        err.contains("Fatal error: eval() runtime failed"),
        "stderr did not contain eval runtime fatal diagnostic: {err}"
    );
}

/// Verifies eval-declared empty classes are registered for later class probes.
#[test]
fn test_eval_declared_empty_class_is_visible_to_class_exists() {
    let out = compile_and_run(
        r#"<?php
eval('class DynEvalClassExists {}');
echo eval('return class_exists("DynEvalClassExists") ? "Y" : "N";');
echo eval('return class_exists("dynevalclassexists") ? "Y" : "N";');
"#,
    );
    assert_eq!(out, "YY");
}

/// Verifies native `class_exists()` probes can see eval-declared classes after the barrier.
#[test]
fn test_eval_declared_empty_class_is_visible_to_native_class_exists_after_barrier() {
    let out = compile_and_run(
        r#"<?php
echo class_exists("DynEvalNativeClassExists") ? "bad" : "N";
eval('class DynEvalNativeClassExists {}');
echo class_exists("DynEvalNativeClassExists") ? "Y" : "N";
echo class_exists("dynevalnativeclassexists") ? "Y" : "N";
echo class_exists("\DynEvalNativeClassExists", false) ? "Y" : "N";
echo class_exists("MissingDynEvalNativeClassExists") ? "bad" : "N";
"#,
    );
    assert_eq!(out, "NYYYN");
}

/// Verifies post-eval native class probes keep AOT class results static.
#[test]
fn test_eval_barrier_keeps_native_class_exists_for_aot_classes() {
    let out = compile_and_run(
        r#"<?php
class EvalNativeClassExistsAot {}
eval('');
echo class_exists("evalnativeclassexistsaot") ? "Y" : "N";
"#,
    );
    assert_eq!(out, "Y");
}

/// Verifies duplicate eval-declared classes fail through the runtime bridge.
#[test]
fn test_eval_duplicate_declared_class_fails() {
    let err = compile_and_run_expect_failure(
        r#"<?php
eval('class DynEvalClassDup {}');
eval('class dynevalclassdup {}');
"#,
    );
    assert!(
        err.contains("Fatal error: eval() runtime failed"),
        "stderr did not contain eval runtime fatal diagnostic: {err}"
    );
}

/// Verifies eval class declarations cannot redeclare an AOT class name.
#[test]
fn test_eval_declared_class_duplicate_aot_class_fails() {
    let err = compile_and_run_expect_failure(
        r#"<?php
class DynEvalAotClassDup {}
eval('class dynevalaotclassdup {}');
"#,
    );
    assert!(
        err.contains("Fatal error: eval() runtime failed"),
        "stderr did not contain eval runtime fatal diagnostic: {err}"
    );
}

/// Verifies non-empty eval class declarations remain unsupported until dynamic class bodies exist.
#[test]
fn test_eval_unsupported_non_empty_class_declaration_fails() {
    let err = compile_and_run_expect_failure(
        "<?php eval('class DynEvalUnsupported { public int $x = 1; }');",
    );
    assert!(
        err.contains("Fatal error: eval() fragment uses an unsupported construct"),
        "stderr did not contain eval unsupported diagnostic: {err}"
    );
}

/// Verifies eval can construct an AOT class with no declared constructor.
#[test]
fn test_eval_dynamic_new_constructs_aot_class() {
    let out = compile_and_run(
        r#"<?php
class EvalDynamicNewSupported {
    public int $x = 7;
}
echo eval('$box = new EvalDynamicNewSupported(); return $box->x;');
"#,
    );
    assert_eq!(out, "7");
}

/// Verifies eval object construction runs an AOT zero-argument constructor.
#[test]
fn test_eval_dynamic_new_runs_zero_arg_constructor() {
    let out = compile_and_run(
        r#"<?php
class EvalDynamicNewZeroArgCtor {
    public int $x = 0;
    public function __construct() { $this->x = 9; }
}
echo eval('$box = new EvalDynamicNewZeroArgCtor(); return $box->x;');
"#,
    );
    assert_eq!(out, "9");
}

/// Verifies eval object construction passes positional arguments to an AOT constructor.
#[test]
fn test_eval_dynamic_new_runs_constructor_with_arg() {
    let out = compile_and_run(
        r#"<?php
class EvalDynamicNewOneArgCtor {
    public int $x = 0;
    public function __construct(int $x) { $this->x = $x; }
}
echo eval('$box = new EvalDynamicNewOneArgCtor(11); return $box->x;');
"#,
    );
    assert_eq!(out, "11");
}

/// Verifies eval follows PHP by accepting constructor arguments when no constructor exists.
#[test]
fn test_eval_dynamic_new_accepts_args_without_constructor() {
    let out = compile_and_run(
        r#"<?php
class EvalDynamicNewNoCtorArgs {
    public int $x = 4;
}
echo eval('$box = new EvalDynamicNewNoCtorArgs(99); return $box->x;');
"#,
    );
    assert_eq!(out, "4");
}

/// Verifies eval object construction fails when no AOT class matches the name.
#[test]
fn test_eval_dynamic_new_missing_class_fails() {
    let err = compile_and_run_expect_failure("<?php eval('new EvalDynamicNewMissingClass();');");
    assert!(
        err.contains("Fatal error: eval() runtime failed"),
        "stderr did not contain eval runtime fatal diagnostic: {err}"
    );
}

/// Verifies eval can construct explicitly qualified namespaced AOT classes.
#[test]
fn test_eval_dynamic_new_constructs_qualified_aot_class() {
    let out = compile_and_run(
        r#"<?php
namespace EvalDynamicNewNs;
class Box {
    public int $x = 13;
}
echo eval('return (new \EvalDynamicNewNs\Box())->x;');
"#,
    );
    assert_eq!(out, "13");
}

/// Verifies eval namespace imports resolve functions, constants, and AOT class aliases.
#[test]
fn test_eval_fragment_namespace_use_imports() {
    let out = compile_and_run(
        r#"<?php
namespace EvalUseBridge;
class Box {
    public int $x = 17;
}
eval('namespace EvalUseExec;
function imported_eval_func($x) { return $x + 1; }
define("EvalUseLib\\VALUE", 5);
use function EvalUseExec\\imported_eval_func as AliasFunc;
use const EvalUseLib\\VALUE as LocalValue;
use EvalUseBridge\\Box as BoxAlias;
$box = new BoxAlias();
echo AliasFunc(LocalValue) . ":" . $box->x;');
"#,
    );
    assert_eq!(out, "6:17");
}

/// Verifies eval grouped namespace imports resolve functions, constants, and AOT class aliases.
#[test]
fn test_eval_fragment_grouped_namespace_use_imports() {
    let out = compile_and_run(
        r#"<?php
namespace EvalGroupedUseBridge;
class Box {
    public int $x = 19;
}
eval('namespace EvalGroupedUseExec;
function imported_eval_func($x) { return $x + 1; }
define("EvalGroupedUseLib\\VALUE", 7);
use EvalGroupedUseBridge\\{Box as BoxAlias};
use function EvalGroupedUseExec\\{imported_eval_func as AliasFunc};
use const EvalGroupedUseLib\\{VALUE as LocalValue};
$box = new BoxAlias();
echo AliasFunc(LocalValue) . ":" . $box->x;');
"#,
    );
    assert_eq!(out, "8:19");
}

/// Verifies eval include executes PHP files through the bridge and shares caller scope.
#[test]
fn test_eval_fragment_include_executes_php_file_and_returns_value() {
    let out = compile_and_run(
        r#"<?php
file_put_contents("eval-include-piece.php", '<?php echo "I"; $x = $x + 1; return $x;');
$x = 4;
echo eval('return include "eval-include-piece.php";');
echo ":" . $x;
"#,
    );
    assert_eq!(out, "I5:5");
}

/// Verifies eval include_once skips files already included and plain files echo as text.
#[test]
fn test_eval_fragment_include_once_and_plain_file() {
    let out = compile_and_run(
        r#"<?php
file_put_contents("eval-once-piece.php", '<?php echo "O";');
file_put_contents("eval-plain-piece.txt", 'RAW');
eval('include_once "eval-once-piece.php"; include_once "eval-once-piece.php"; echo (include_once "eval-once-piece.php") ? "T" : "F";');
echo ":";
echo eval('return include "eval-plain-piece.txt";');
"#,
    );
    assert_eq!(out, "OT:RAW1");
}

/// Verifies missing eval require aborts through the runtime eval fatal path.
#[test]
fn test_eval_fragment_missing_require_fails() {
    let err = compile_and_run_expect_failure("<?php eval('require \"missing-eval-require.php\";');");
    assert!(
        err.contains("Fatal error: eval() runtime failed"),
        "stderr did not contain eval runtime fatal: {err}"
    );
}

/// Verifies eval reference assignments update the referenced caller local.
#[test]
fn test_eval_reference_assignment_updates_caller_local() {
    let out = compile_and_run(
        r#"<?php
$x = 1;
eval('$alias =& $x; $alias = 5;');
echo $x;
"#,
    );
    assert_eq!(out, "5");
}

/// Verifies eval unset breaks a reference alias without unsetting the source variable.
#[test]
fn test_eval_unset_reference_alias_keeps_source_local() {
    let out = compile_and_run(
        r#"<?php
$x = 1;
eval('$alias =& $x; unset($alias); $alias = 9;');
echo $x . ":" . $alias;
"#,
    );
    assert_eq!(out, "1:9");
}

/// Verifies `return` inside eval becomes the expression result of `eval(...)`.
#[test]
fn test_eval_return_value_is_available_to_native_code() {
    let out = compile_and_run("<?php echo eval('return 7;');");
    assert_eq!(out, "7");
}

/// Verifies eval can read and write an existing native local through the materialized scope.
#[test]
fn test_eval_reads_and_writes_existing_local() {
    let out = compile_and_run(
        r#"<?php
$x = 10;
eval('$x = $x + 5;');
echo $x;
"#,
    );
    assert_eq!(out, "15");
}

/// Verifies eval-created variables are visible to later native code in the caller scope.
#[test]
fn test_eval_created_variable_is_visible_after_eval() {
    let out = compile_and_run(
        r#"<?php
eval('$created = "yes";');
echo $created;
"#,
    );
    assert_eq!(out, "yes");
}

/// Verifies a variable created by one eval call is visible to a later eval call.
#[test]
fn test_eval_scope_persists_between_eval_calls() {
    let out = compile_and_run(
        r#"<?php
eval('$created = 2;');
eval('$created = $created + 5;');
echo $created;
"#,
    );
    assert_eq!(out, "7");
}

/// Verifies eval can replace an existing scalar local with a different runtime type.
#[test]
fn test_eval_can_change_existing_local_type() {
    let out = compile_and_run(
        r#"<?php
$x = 1;
eval('$x = "changed";');
echo $x;
"#,
    );
    assert_eq!(out, "changed");
}

/// Verifies eval-created function locals can be returned from native function code.
#[test]
fn test_eval_created_function_local_can_be_returned() {
    let out = compile_and_run(
        r#"<?php
function make_value() {
    eval('$created = "fn";');
    return $created;
}
echo make_value();
"#,
    );
    assert_eq!(out, "fn");
}

/// Verifies eval return is independent from writes it performs to the caller scope.
#[test]
fn test_eval_return_and_scope_write_are_visible() {
    let out = compile_and_run(
        r#"<?php
$x = 1;
$r = eval('$x = 3; return $x + 4;');
echo $x;
echo ":";
echo $r;
"#,
    );
    assert_eq!(out, "3:7");
}

/// Verifies an eval unset does not leave a stale Mixed local value visible.
#[test]
fn test_eval_unset_clears_existing_mixed_local() {
    let out = compile_and_run(
        r#"<?php
$x = eval('return 10;');
eval('unset($x);');
echo $x;
"#,
    );
    assert_eq!(out, "");
}

/// Verifies the eval bridge maps PHP opening tags inside fragments to parse diagnostics.
#[test]
fn test_eval_fragment_with_php_opening_tag_reports_parse_error() {
    let err = compile_and_run_expect_failure("<?php eval('<?php echo 1;');");
    assert!(
        err.contains("Parse error: eval() fragment is invalid"),
        "stderr did not contain eval parse diagnostic: {err}"
    );
}

/// Verifies Throwable objects thrown inside eval cross into the caller's catch block.
#[test]
fn test_eval_throw_crosses_caller_try_catch() {
    let out = compile_and_run(
        r#"<?php
$e = new Exception("eval boom");
try {
    eval('throw $e;');
    echo "bad";
} catch (Exception $caught) {
    echo "caught:" . $caught->getMessage();
}
"#,
    );
    assert_eq!(out, "caught:eval boom");
}

/// Verifies Throwable objects thrown by eval-declared functions cross native call sites.
#[test]
fn test_eval_declared_function_throw_crosses_native_try_catch() {
    let out = compile_and_run(
        r#"<?php
eval('function dyn_eval_throw($e) { throw $e; }');
try {
    dyn_eval_throw(new Exception("dyn boom"));
    echo "bad";
} catch (Exception $caught) {
    echo "caught:" . $caught->getMessage();
}
"#,
    );
    assert_eq!(out, "caught:dyn boom");
}

/// Verifies Throwable objects thrown by nested eval calls keep the original catch target.
#[test]
fn test_eval_nested_throw_crosses_caller_try_catch() {
    let out = compile_and_run(
        r#"<?php
$e = new Exception("nested boom");
try {
    eval('eval("throw $e;");');
    echo "bad";
} catch (Exception $caught) {
    echo "caught:" . $caught->getMessage();
}
"#,
    );
    assert_eq!(out, "caught:nested boom");
}

/// Verifies eval-internal try/catch consumes a thrown Throwable before returning.
#[test]
fn test_eval_try_catch_catches_throwable_inside_eval() {
    let out = compile_and_run(
        r#"<?php
echo eval('try {
    throw new Exception("eval boom");
} catch (Throwable $caught) {
    return 7;
}
return 0;');
"#,
    );
    assert_eq!(out, "7");
}

/// Verifies eval-internal catch clauses can omit the Throwable variable.
#[test]
fn test_eval_try_catch_without_variable_inside_eval() {
    let out = compile_and_run(
        r#"<?php
echo eval('try {
    throw new Exception("eval boom");
} catch (Throwable) {
    return 8;
}
return 0;');
"#,
    );
    assert_eq!(out, "8");
}

/// Verifies eval-internal finally runs before returning from the fragment.
#[test]
fn test_eval_finally_runs_before_eval_return() {
    let out = compile_and_run(
        r#"<?php
echo eval('try {
    return 1;
} finally {
    echo "F";
}');
"#,
    );
    assert_eq!(out, "F1");
}
