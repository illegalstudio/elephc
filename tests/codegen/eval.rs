//! Purpose:
//! Integration tests for the optional `eval()` bridge.
//! Covers language-construct visibility, conditional bridge linking, scope
//! synchronization, dynamic declarations, EvalIR execution, and supported
//! builtin dispatch through end-to-end codegen.
//!
//! Called from:
//! - `cargo test --test codegen_tests eval` through Rust's test harness.
//!
//! Key details:
//! - Fixtures exercise the native/EvalIR boundary rather than the frozen legacy
//!   AST backend, and many cases assert post-barrier native visibility.

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

/// Verifies eval array writes preserve PHP copy-on-write for by-value aliases.
#[test]
fn test_eval_array_write_preserves_native_by_value_alias() {
    let out = compile_and_run(
        r#"<?php
$items = ["a", "b"];
$snapshot = $items;
eval('$items[0] = "z"; $items[] = "c";');
echo $items[0] . ":" . $items[2] . ":" . count($items);
echo "|";
echo $snapshot[0] . ":" . count($snapshot);
"#,
    );
    assert_eq!(out, "z:c:3|a:2");
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
$accent = json_decode("\"\\u00e9\"");
$emoji = json_decode("\"\\ud83d\\ude00\"");
echo bin2hex(json_encode($accent . "/" . $emoji)) . ":";
echo bin2hex(json_encode($accent . "/" . $emoji, JSON_UNESCAPED_UNICODE)) . ":";
echo bin2hex(json_encode([$accent => $emoji])) . ":";
echo bin2hex(json_encode([$accent => $emoji], JSON_UNESCAPED_UNICODE)) . ":";
echo json_encode([1, 2], JSON_FORCE_OBJECT) . ":";
echo json_encode([], JSON_FORCE_OBJECT) . ":";
echo call_user_func_array("json_encode", ["value" => [1, 2], "flags" => JSON_FORCE_OBJECT]) . ":";
echo json_encode("<>&\"" . chr(39), JSON_HEX_TAG | JSON_HEX_AMP | JSON_HEX_APOS | JSON_HEX_QUOT) . ":";
echo json_encode(["01", "+12", "1e3", " 7", "7x"], JSON_NUMERIC_CHECK) . ":";
echo json_encode([1.0, 2.5, -3.0], JSON_PRESERVE_ZERO_FRACTION) . ":";
echo (json_encode(INF) === false ? "false" : "json") . ":";
echo json_last_error() . ":" . json_last_error_msg() . ":";
echo json_encode([1.5, INF, NAN], JSON_PARTIAL_OUTPUT_ON_ERROR) . ":";
echo json_last_error() . ":" . json_last_error_msg() . ":";
$bad = "a" . chr(128) . "b";
echo (json_encode($bad) === false ? "utf8-false" : "bad") . ":";
echo json_last_error() . ":";
echo bin2hex(json_encode($bad, JSON_PARTIAL_OUTPUT_ON_ERROR)) . ":";
echo json_last_error() . ":";
echo json_encode($bad, JSON_INVALID_UTF8_IGNORE) . ":";
echo json_last_error() . ":";
echo bin2hex(json_encode($bad, JSON_INVALID_UTF8_SUBSTITUTE)) . ":";
echo json_last_error() . ":";
echo bin2hex(json_encode($bad, JSON_INVALID_UTF8_SUBSTITUTE | JSON_UNESCAPED_UNICODE)) . ":";
echo json_last_error() . ":";
echo json_encode(["k" . chr(128) => "v" . chr(128)], JSON_PARTIAL_OUTPUT_ON_ERROR) . ":";
echo json_last_error() . ":";
json_encode(3.5);
echo json_last_error() . ":" . json_last_error_msg() . ":";
echo str_replace("\n", "|", json_encode(["a" => [1, 2]], JSON_PRETTY_PRINT)) . ":";
echo function_exists("json_encode");');
"#,
    );
    assert_eq!(
        out,
        r#"{"a":1,"b":"x\/y"}:[1,"q",true,null]:"a\/b\"c":{"k":false}:"a/b":"x/y":225c75303065395c2f5c75643833645c756465303022:22c3a95c2ff09f988022:7b225c7530306539223a225c75643833645c7564653030227d:7b22c3a9223a22f09f9880227d:{"0":1,"1":2}:{}:{"0":1,"1":2}:"\u003C\u003E\u0026\u0022\u0027":[1,12,1000,7,"7x"]:[1.0,2.5,-3.0]:false:7:Inf and NaN cannot be JSON encoded:[1.5,0,0]:7:Inf and NaN cannot be JSON encoded:utf8-false:5:6e756c6c:5:"ab":0:22615c75666666646222:0:2261efbfbd6222:0:{"":null}:5:0:No error:{|    "a": [|        1,|        2|    ]|}:1"#
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
$badJson = "\"a" . chr(128) . "b\"";
echo (is_null(json_decode($badJson)) ? "utf8-null" : "bad") . ":";
echo json_last_error() . ":";
echo bin2hex(json_decode($badJson, true, 512, JSON_INVALID_UTF8_IGNORE)) . ":";
echo json_last_error() . ":";
echo bin2hex(json_decode($badJson, true, 512, JSON_INVALID_UTF8_SUBSTITUTE)) . ":";
echo json_last_error() . ":";
$objSub = json_decode("{\"k" . chr(128) . "\":\"v" . chr(128) . "\"}", true, 512, JSON_INVALID_UTF8_SUBSTITUTE);
$objSubKeys = array_keys($objSub);
echo bin2hex($objSubKeys[0]) . "=" . bin2hex($objSub[$objSubKeys[0]]) . ":";
$objIgnore = json_decode("{\"k" . chr(128) . "\":\"v" . chr(128) . "\"}", true, 512, JSON_INVALID_UTF8_IGNORE);
$objIgnoreKeys = array_keys($objIgnore);
echo bin2hex($objIgnoreKeys[0]) . "=" . bin2hex($objIgnore[$objIgnoreKeys[0]]) . ":";
echo (is_null(json_decode("bad")) ? "BAD" : "wrong") . ":";
$big = json_decode("[9223372036854775808]", true, 512, JSON_BIGINT_AS_STRING);
echo json_decode("9223372036854775808", true, 512, JSON_BIGINT_AS_STRING) . ":";
echo json_decode("-9223372036854775809", true, 512, JSON_BIGINT_AS_STRING) . ":";
echo gettype($big[0]) . ":" . $big[0] . ":";
echo call_user_func_array("json_decode", ["json" => "9223372036854775808", "associative" => true, "depth" => 512, "flags" => JSON_BIGINT_AS_STRING]) . ":";
echo function_exists("json_decode");');
"#,
    );
    assert_eq!(
        out,
        "hello:42:T:NULL:1:x:F:4:v:utf8-null:5:6162:0:61efbfbd62:0:6befbfbd=76efbfbd:6b=76:BAD:9223372036854775808:-9223372036854775809:string:9223372036854775808:9223372036854775808:1"
    );
}

/// Verifies eval `json_decode()` returns `stdClass` objects unless assoc is true.
#[test]
fn test_eval_dispatches_json_decode_stdclass_default() {
    let out = compile_and_run(
        r#"<?php
eval('$object = json_decode("{\"a\":1,\"b\":{\"c\":\"x\"}}");
echo $object->a . ":" . $object->b->c . ":";
$objectFalse = json_decode("{\"z\":2}", false);
echo $objectFalse->z . ":";
$objectNull = json_decode("{\"n\":{\"m\":3}}", null);
echo $objectNull->n->m . ":";
$assoc = json_decode("{\"b\":{\"c\":\"y\"}}", true);
echo $assoc["b"]["c"] . ":";');
$object = eval('return json_decode("{\"a\":1,\"b\":{\"c\":\"x\"}}");');
echo gettype($object) . ":" . $object->a . ":" . $object->b->c;
"#,
    );
    assert_eq!(out, "1:x:2:3:y:object:1:x");
}

/// Verifies eval `json_encode()` serializes stdClass dynamic properties.
#[test]
fn test_eval_dispatches_json_encode_stdclass_object() {
    let out = compile_and_run(
        r#"<?php
eval('$object = json_decode("{\"a\":1,\"b\":{\"c\":\"x\"}}");
echo json_encode($object) . ":";
echo str_replace("\n", "|", json_encode($object, JSON_PRETTY_PRINT)) . ":";
$empty = json_decode("{}");
echo json_encode($empty) . ":";
$empty->a = 7;
echo json_encode($empty);');
"#,
    );
    assert_eq!(
        out,
        r#"{"a":1,"b":{"c":"x"}}:{|    "a": 1,|    "b": {|        "c": "x"|    }|}:{}:{"a":7}"#
    );
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

/// Verifies eval JSON throw flags raise catchable `JsonException` objects.
#[test]
fn test_eval_dispatches_json_throw_on_error() {
    let out = compile_and_run(
        r#"<?php
eval('try {
    json_decode("bad", true, 512, JSON_THROW_ON_ERROR);
    echo "bad";
} catch (Throwable) {
    echo "inner:";
}');
try {
    eval('json_decode("bad", true, 512, JSON_THROW_ON_ERROR);');
    echo "bad";
} catch (Throwable $e) {
    echo "outer:" . get_class($e) . ":" . $e->getCode() . ":" . (str_contains($e->getMessage(), "Syntax error") ? "syntax" : "bad") . ":";
}
try {
    eval('json_encode(INF, JSON_THROW_ON_ERROR);');
    echo "bad";
} catch (Throwable $e) {
    echo "encode:" . get_class($e) . ":" . $e->getCode() . ":" . $e->getMessage() . ":";
}
eval('echo json_encode(INF, JSON_THROW_ON_ERROR | JSON_PARTIAL_OUTPUT_ON_ERROR) . ":";');
eval('$json = chr(34) . "a" . chr(128) . "b" . chr(34); echo json_decode($json, true, 512, JSON_THROW_ON_ERROR | JSON_INVALID_UTF8_IGNORE) . ":";');
"#,
    );
    assert_eq!(
        out,
        "inner:outer:JsonException:4:syntax:encode:JsonException:7:Inf and NaN cannot be JSON encoded:0:ab:"
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
echo (json_validate("\"a" . chr(128) . "b\"", 512, JSON_INVALID_UTF8_IGNORE) ? "I" : "bad") . ":";
echo json_last_error() . ":";
echo (json_validate("bad", 512, JSON_INVALID_UTF8_IGNORE) ? "bad" : "S") . ":";
echo json_last_error() . ":";
echo function_exists("json_validate");');
"#,
    );
    assert_eq!(out, "Y:N:D:C:A:I:0:S:4:1");
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
    assert_eq!(out, "1:y1z2x3:1:y3z2x1:1:34a2b1:1:b1a234:1:21:1:12:1");
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
    assert_eq!(out, "1:2img1;1img2;0img10;:1:cIMG1;aimg2;bImg10;:1:ba:1");
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
    assert_eq!(out, "3:1:2:ok:drop:2:1:3:1:4:1:5:2:2:4:1:20:2:1:3:2:1:2:1");
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
preg_match("/(a)?(b)/", "b", $offsetOne, PREG_OFFSET_CAPTURE);
echo $offsetOne[0][0] . ":" . $offsetOne[0][1] . ":" . $offsetOne[1][0] . ":" . $offsetOne[1][1] . ":" . $offsetOne[2][0] . ":" . $offsetOne[2][1] . ":";
preg_match_all("/([a-z]+)([0-9]+)/", "a1 b22", $offsetAll, PREG_OFFSET_CAPTURE);
echo $offsetAll[0][1][0] . ":" . $offsetAll[0][1][1] . ":" . $offsetAll[1][0][1] . ":" . $offsetAll[2][1][1] . ":";
preg_match_all("/([a-z]+)([0-9]+)/", "a1 b22", $offsetSet, PREG_SET_ORDER | PREG_OFFSET_CAPTURE);
echo $offsetSet[1][0][0] . ":" . $offsetSet[1][0][1] . ":" . $offsetSet[0][2][1] . ":";
preg_match("/(a)?(b)(c)?/", "b", $nullOne, PREG_UNMATCHED_AS_NULL);
echo count($nullOne) . ":" . ($nullOne[1] === null ? "n" : "bad") . ":" . $nullOne[2] . ":" . ($nullOne[3] === null ? "n" : "bad") . ":";
preg_match("/(a)?(b)(c)?/", "b", $nullOffset, PREG_UNMATCHED_AS_NULL | PREG_OFFSET_CAPTURE);
echo ($nullOffset[1][0] === null ? "n" : "bad") . ":" . $nullOffset[1][1] . ":" . ($nullOffset[3][0] === null ? "n" : "bad") . ":" . $nullOffset[3][1] . ":";
preg_match_all("/(a)?(b)(c)?/", "b", $nullAll, PREG_UNMATCHED_AS_NULL);
echo ($nullAll[1][0] === null ? "n" : "bad") . ":" . $nullAll[2][0] . ":" . ($nullAll[3][0] === null ? "n" : "bad") . ":";
preg_match_all("/(a)?(b)(c)?/", "b", $nullSet, PREG_SET_ORDER | PREG_UNMATCHED_AS_NULL | PREG_OFFSET_CAPTURE);
echo ($nullSet[0][1][0] === null ? "n" : "bad") . ":" . $nullSet[0][1][1] . ":" . ($nullSet[0][3][0] === null ? "n" : "bad") . ":" . $nullSet[0][3][1] . ":";
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
$splitOffsets = preg_split("/,/", "a,b,c", 2, PREG_SPLIT_OFFSET_CAPTURE);
echo $splitOffsets[0][0] . ":" . $splitOffsets[0][1] . ":" . $splitOffsets[1][0] . ":" . $splitOffsets[1][1] . ":";
$splitBoth = preg_split("/(,)/", "a,b", 0, PREG_SPLIT_DELIM_CAPTURE | PREG_SPLIT_OFFSET_CAPTURE);
echo count($splitBoth) . ":" . $splitBoth[1][0] . ":" . $splitBoth[1][1] . ":";
$splitNoEmpty = preg_split("/,/", "a,,b", 0, PREG_SPLIT_NO_EMPTY | PREG_SPLIT_OFFSET_CAPTURE);
echo $splitNoEmpty[1][0] . ":" . $splitNoEmpty[1][1] . ":";
echo function_exists("preg_match") && function_exists("preg_match_all") && function_exists("preg_replace") && function_exists("preg_replace_callback") && function_exists("preg_split") && defined("PREG_SPLIT_NO_EMPTY") && defined("PREG_SET_ORDER") && defined("PREG_OFFSET_CAPTURE") && defined("PREG_SPLIT_OFFSET_CAPTURE") && defined("PREG_UNMATCHED_AS_NULL");');
"#,
    );
    assert_eq!(
        out,
        "1:3:id42:id:42:0:3:2:3:b22:a:22:2:2:a1:a:22:b:0::-1:b:0:b22:3:0:4:b22:3:1:4:n:b:n:n:-1:n:-1:n:b:n:n:-1:n:-1:3:0:0:0:1-a 2-b:[A][B]:2:a:b,c:2:b:1:aN:3:,:a:0:b,c:2:3:,:1:b:3:1"
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
$object = json_decode("{}");
echo is_object($object) ? "O" : "bad";
echo is_object([1]) ? "bad" : "o";
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
echo call_user_func("is_object", $object) ? "O" : "bad";
echo call_user_func_array("is_object", ["value" => 1]) ? "bad" : "o";
echo call_user_func("is_nan", fdiv(0, 0)) ? "N" : "bad";
echo call_user_func_array("is_finite", [42]) ? "F" : "bad";
echo function_exists("is_double"); echo function_exists("is_numeric"); echo function_exists("is_object"); echo function_exists("is_resource");
echo function_exists("is_nan"); echo function_exists("is_finite"); echo function_exists("is_iterable"); echo function_exists("is_infinite");');
"#,
    );
    assert_eq!(out, "1111111111111Tok11111NBROoNIiFfH:1111t11OoNF11111111");
}

/// Verifies eval resource introspection builtins inspect boxed runtime resources.
#[test]
fn test_eval_dispatches_resource_introspection_builtin_calls() {
    let out = compile_and_run(
        r#"<?php
$h = fopen("php://memory", "r+");
eval('echo get_resource_type($h);
echo ":"; echo get_resource_id($h) > 0 ? "id" : "bad";
echo ":"; echo call_user_func("get_resource_type", $h);
echo ":"; echo call_user_func_array("get_resource_id", ["resource" => $h]) > 0 ? "id" : "bad";
echo ":"; echo function_exists("get_resource_type"); echo function_exists("get_resource_id");');
"#,
    );
    assert_eq!(out, "stream:id:stream:id:11");
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

/// Verifies eval-declared `__toString()` runs in string contexts through the bridge.
#[test]
fn test_eval_declared_tostring_string_contexts() {
    let out = compile_and_run(
        r#"<?php
eval('class EvalStringableBox {
    public string $name = "Ada";
    public function __toString() {
        return "box:" . $this->name;
    }
    public function accepts(string $value) {
        return "typed:" . $value;
    }
}
$box = new EvalStringableBox();
echo $box; echo ":";
print $box; echo ":";
echo "pre" . $box; echo ":";
echo strval($box); echo ":";
echo call_user_func("strval", $box); echo ":";
echo call_user_func_array("strval", [$box]); echo ":";
echo $box instanceof Stringable ? "S" : "s"; echo ":";
echo $box->accepts($box);');
"#,
    );
    assert_eq!(
        out,
        "box:Ada:box:Ada:prebox:Ada:box:Ada:box:Ada:box:Ada:S:typed:box:Ada"
    );
}

/// Verifies eval `settype()` mutates direct variables and supports named arguments.
#[test]
fn test_eval_dispatches_settype_builtin_call() {
    let out = compile_and_run(
        r#"<?php
eval('$x = 42;
echo settype($x, "string") ? gettype($x) . ":" . $x : "bad";
echo ":";
$y = "0";
echo settype(type: "bool", var: $y) ? gettype($y) . ":" . ($y ? "true" : "false") : "bad";
echo ":";
echo function_exists("settype");');
"#,
    );
    assert_eq!(out, "string:42:boolean:false:1");
}

/// Verifies eval SPL object identity builtins inspect AOT object cells.
#[test]
fn test_eval_dispatches_spl_object_identity_builtin_calls() {
    let out = compile_and_run(
        r#"<?php
class EvalObjectIdentityProbe {}

eval('$a = new EvalObjectIdentityProbe();
$b = new EvalObjectIdentityProbe();
echo (spl_object_id($a) === spl_object_id($a)) ? "stable" : "drift";
echo ":";
echo (spl_object_id($a) !== spl_object_id($b)) ? "unique" : "same";
echo ":";
echo (spl_object_hash(object: $a) === spl_object_hash($a)) ? "hash" : "bad";
echo ":";
echo (call_user_func("spl_object_id", $a) === spl_object_id($a)) ? "call" : "bad";
echo ":";
echo (call_user_func_array("spl_object_hash", ["object" => $b]) === spl_object_hash($b)) ? "array" : "bad";
echo ":";
echo function_exists("spl_object_id"); echo function_exists("spl_object_hash");');
"#,
    );
    assert_eq!(out, "stable:unique:hash:call:array:11");
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

/// Verifies eval `get_class()` resolves stdClass and AOT object runtime names.
#[test]
fn test_eval_dispatches_get_class_builtin_call() {
    let out = compile_and_run(
        r#"<?php
class EvalClassNameProbe {}

eval('$object = json_decode("{}");
echo get_class($object) . ":";
$probe = new EvalClassNameProbe();
echo get_class($probe) . ":";
echo call_user_func("get_class", $object) . ":";
echo call_user_func_array("get_class", ["object" => $probe]) . ":";
echo function_exists("get_class");');
"#,
    );
    assert_eq!(
        out,
        "stdClass:EvalClassNameProbe:stdClass:EvalClassNameProbe:1"
    );
}

/// Verifies eval `get_parent_class()` resolves AOT object and class-string parents.
#[test]
fn test_eval_dispatches_get_parent_class_builtin_call() {
    let out = compile_and_run(
        r#"<?php
class EvalParentBase {}
class EvalParentChild extends EvalParentBase {}

eval('$child = new EvalParentChild();
echo get_parent_class($child) . ":";
echo get_parent_class("EvalParentChild") . ":";
echo get_parent_class("evalparentchild") . ":";
echo call_user_func("get_parent_class", $child) . ":";
echo call_user_func_array("get_parent_class", ["object_or_class" => "EvalParentChild"]) . ":";
echo function_exists("get_parent_class");');
"#,
    );

    assert_eq!(
        out,
        "EvalParentBase:EvalParentBase:EvalParentBase:EvalParentBase:EvalParentBase:1"
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

/// Verifies eval `sscanf()` returns indexed string matches through direct and callable paths.
#[test]
fn test_eval_dispatches_sscanf_builtin_call() {
    let out = compile_and_run(
        r#"<?php
eval('$result = sscanf("John 1.5 30", "%s %f %d");
echo $result[0] . ":" . $result[1] . ":" . $result[2] . ":";
$named = sscanf(string: "Age: -25", format: "Age: %d");
echo $named[0] . ":";
$call = call_user_func("sscanf", "-2.5e3", "%f");
echo $call[0] . ":";
$spread = call_user_func_array("sscanf", ["string" => "ok %", "format" => "%s %%"]);
echo $spread[0] . ":";
echo function_exists("sscanf");');
"#,
    );
    assert_eq!(out, "John:1.5:30:-25:-2.5e3:ok:1");
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
    assert_eq!(out, "0011101x");
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
    assert_eq!(out, "1111101x");
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

/// Verifies eval inside a closure can mutate the closure's by-value capture without touching the outer variable.
#[test]
fn test_eval_inside_closure_updates_by_value_capture_copy() {
    let out = compile_and_run(
        r#"<?php
$x = 1;
$fn = function() use ($x) {
    eval('$x = $x + 4;');
    return $x;
};
echo $fn();
echo ":" . $x;
"#,
    );
    assert_eq!(out, "5:1");
}

/// Verifies eval inside a closure writes through a by-reference capture to the source variable.
#[test]
fn test_eval_inside_closure_updates_by_ref_capture_source() {
    let out = compile_and_run(
        r#"<?php
$x = 1;
$fn = function() use (&$x) {
    eval('$x = $x + 4;');
};
$fn();
echo $x;
"#,
    );
    assert_eq!(out, "5");
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

/// Verifies eval fragments pass more than two fixed scalar arguments to public AOT methods.
#[test]
fn test_eval_fragment_can_call_this_public_many_arg_method() {
    let out = compile_and_run(
        r#"<?php
class EvalMethodManyArgBox {
    public int $x = 10;

    public function label(int $a, int $b, int $c, string $suffix): string {
        return ($this->x + $a + $b + $c) . $suffix;
    }

    public function run(): void {
        echo eval('return $this->label(1, 2, 3, "!");');
    }
}

$box = new EvalMethodManyArgBox();
$box->run();
"#,
    );
    assert_eq!(out, "16!");
}

/// Verifies eval fragments pass AOT method arguments that overflow onto the caller stack.
#[test]
fn test_eval_fragment_can_call_this_public_method_with_stack_string_arg() {
    let out = compile_and_run(
        r#"<?php
class EvalMethodStackStringArgBox {
    public function join4(string $a, string $b, string $c, string $d): string {
        return $a . $b . $c . $d;
    }

    public function run(): void {
        echo eval('return $this->join4("A", "B", "C", "D");');
    }
}

$box = new EvalMethodStackStringArgBox();
$box->run();
"#,
    );
    assert_eq!(out, "ABCD");
}

/// Verifies eval fragments pass boxed Mixed values to public AOT methods.
#[test]
fn test_eval_fragment_can_call_this_public_mixed_arg_method() {
    let out = compile_and_run(
        r#"<?php
class EvalMethodMixedArgBox {
    public function identity(mixed $value): mixed {
        return $value;
    }

    public function run(): void {
        echo eval('return $this->identity("mixed-ok");');
    }
}

$box = new EvalMethodMixedArgBox();
$box->run();
"#,
    );
    assert_eq!(out, "mixed-ok");
}

/// Verifies eval fragments can pass object-typed arguments to public AOT methods.
#[test]
fn test_eval_fragment_can_call_aot_method_with_object_arg() {
    let out = compile_and_run(
        r#"<?php
class EvalMethodObjectArgItem {
    public string $name;

    public function __construct(string $name) {
        $this->name = $name;
    }
}

class EvalMethodObjectArgBox {
    public function describe(EvalMethodObjectArgItem $item): string {
        return $item->name;
    }

    public static function describeStatic(EvalMethodObjectArgItem $item): string {
        return $item->name . "!";
    }

    public function run() {
        $item = new EvalMethodObjectArgItem("Obj");
        return eval('return $this->describe($item) . ":" . EvalMethodObjectArgBox::describeStatic($item);');
    }
}

echo (new EvalMethodObjectArgBox())->run();
"#,
    );
    assert_eq!(out, "Obj:Obj!");
}

/// Verifies eval fragments inherit lexical `self::` from an AOT instance method.
#[test]
fn test_eval_fragment_in_aot_method_resolves_self_scope() {
    let out = compile_and_run(
        r#"<?php
class EvalAotScopeSelfBox {
    public static function tag(): string {
        return "self";
    }

    public function run() {
        return eval('return self::class . ":" . self::tag();');
    }
}

echo (new EvalAotScopeSelfBox())->run();
"#,
    );
    assert_eq!(out, "EvalAotScopeSelfBox:self");
}

/// Verifies eval fragments inherit late-static `static::` from an AOT instance method.
#[test]
fn test_eval_fragment_in_aot_method_resolves_late_static_scope() {
    let out = compile_and_run(
        r#"<?php
class EvalAotScopeStaticBase {
    public static function tag(): string {
        return "base";
    }

    public function run(): void {
        echo eval('return self::class . ":" . static::class . ":" . static::tag();');
    }
}

class EvalAotScopeStaticChild extends EvalAotScopeStaticBase {
    public static function tag(): string {
        return "child";
    }
}

(new EvalAotScopeStaticChild())->run();
"#,
    );
    assert_eq!(out, "EvalAotScopeStaticBase:EvalAotScopeStaticChild:child");
}

/// Verifies eval fragments resolve `parent::` through AOT parent metadata.
#[test]
fn test_eval_fragment_in_aot_method_resolves_parent_scope() {
    let out = compile_and_run(
        r#"<?php
class EvalAotScopeParentBase {
    public static function tag(): string {
        return "parent";
    }
}

class EvalAotScopeParentChild extends EvalAotScopeParentBase {
    public function run() {
        return eval('return parent::tag();');
    }
}

echo (new EvalAotScopeParentChild())->run();
"#,
    );
    assert_eq!(out, "parent");
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

/// Verifies eval static calls and static callables dispatch public AOT static methods.
#[test]
fn test_eval_fragment_dispatches_aot_static_methods() {
    let out = compile_and_run_capture(
        r#"<?php
class EvalAotStaticBox {
    public static function join(string $left, string $right): string {
        return $left . $right;
    }

    public static function sum4(int $a, int $b, int $c, int $d): int {
        return $a + $b + $c + $d;
    }

    public static function sum(int $left, int $right): int {
        return $left + $right;
    }
}

eval('echo EvalAotStaticBox::join("A", "B"); echo ":";
$cb = ["EvalAotStaticBox", "join"];
echo call_user_func($cb, "C", "D"); echo ":";
$named = "EvalAotStaticBox::join";
echo $named("E", "F"); echo ":";
echo call_user_func_array(["EvalAotStaticBox", "sum"], [2, 5]); echo ":";
echo EvalAotStaticBox::sum4(1, 2, 3, 4);');
"#,
    );
    assert!(
        out.success,
        "program failed: stdout={:?} stderr={}",
        out.stdout, out.stderr
    );
    assert_eq!(out.stdout, "AB:CD:EF:7:10");
}

/// Verifies eval static dispatch passes AOT static method arguments on the caller stack.
#[test]
fn test_eval_fragment_dispatches_aot_static_method_with_stack_string_arg() {
    let out = compile_and_run(
        r#"<?php
class EvalAotStaticStackStringBox {
    public static function join4(string $a, string $b, string $c, string $d): string {
        return $a . $b . $c . $d;
    }
}

eval('echo EvalAotStaticStackStringBox::join4("G", "H", "I", "J");');
"#,
    );
    assert_eq!(out, "GHIJ");
}

/// Verifies eval binds named arguments before dispatching an AOT instance method.
#[test]
fn test_eval_fragment_dispatches_aot_instance_method_with_named_args() {
    let out = compile_and_run(
        r#"<?php
class EvalAotNamedMethodBox {
    public function run() {
        return eval('return $this->join(right: "B", left: "A");');
    }

    public function join(string $left, string $right): string {
        return $left . $right;
    }
}

echo (new EvalAotNamedMethodBox())->run();
"#,
    );
    assert_eq!(out, "AB");
}

/// Verifies eval binds named arguments before dispatching an AOT static method.
#[test]
fn test_eval_fragment_dispatches_aot_static_method_with_named_args() {
    let out = compile_and_run(
        r#"<?php
class EvalAotNamedStaticBox {
    public static function join(string $left, string $right): string {
        return $left . $right;
    }
}

eval('echo EvalAotNamedStaticBox::join(right: "D", left: "C");');
"#,
    );
    assert_eq!(out, "CD");
}

/// Verifies eval binds named arguments before dispatching an AOT constructor.
#[test]
fn test_eval_dynamic_new_runs_constructor_with_named_args() {
    let out = compile_and_run(
        r#"<?php
class EvalDynamicNewNamedCtor {
    public string $label = "";
    public function __construct(string $left, string $right) {
        $this->label = $left . $right;
    }
}

echo eval('$box = new EvalDynamicNewNamedCtor(right: "F", left: "E"); return $box->label;');
"#,
    );
    assert_eq!(out, "EF");
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

/// Verifies eval `interface_exists()` probes generated AOT interface metadata.
#[test]
fn test_eval_fragment_interface_exists_probes_aot_interfaces() {
    let out = compile_and_run(
        r#"<?php
interface EvalInterfaceExistsProbe {}
class EvalInterfaceExistsImpl implements EvalInterfaceExistsProbe {}

eval('echo interface_exists("EvalInterfaceExistsProbe") ? "Y" : "N";
echo interface_exists("evalinterfaceexistsprobe") ? "Y" : "N";
echo interface_exists("\EvalInterfaceExistsProbe") ? "Y" : "N";
echo interface_exists("EvalInterfaceExistsImpl") ? "Y" : "N";
echo call_user_func("interface_exists", "EvalInterfaceExistsProbe") ? "Y" : "N";
echo call_user_func_array("interface_exists", ["autoload" => false, "interface" => "\EvalInterfaceExistsProbe"]) ? "Y" : "N";
echo function_exists("interface_exists");');
"#,
    );
    assert_eq!(out, "YYYNYY1");
}

/// Verifies eval `trait_exists()` and `enum_exists()` probe generated AOT metadata.
#[test]
fn test_eval_fragment_trait_enum_exists_probe_aot_metadata() {
    let out = compile_and_run(
        r#"<?php
trait EvalTraitExistsProbe {}
enum EvalEnumExistsProbe { case Ready; }

eval('echo trait_exists("EvalTraitExistsProbe") ? "T" : "t";
echo trait_exists("evaltraitexistsprobe") ? "T" : "t";
echo trait_exists("\EvalEnumExistsProbe") ? "T" : "t";
echo enum_exists("EvalEnumExistsProbe") ? "E" : "e";
echo enum_exists("evalenumexistsprobe") ? "E" : "e";
echo enum_exists("EvalTraitExistsProbe") ? "E" : "e";
echo call_user_func("trait_exists", "EvalTraitExistsProbe") ? "T" : "t";
echo call_user_func_array("enum_exists", ["autoload" => false, "enum" => "\EvalEnumExistsProbe"]) ? "E" : "e";
echo trait_exists(trait: "MissingEvalTrait", autoload: false) ? "T" : "t";
echo enum_exists(enum: "MissingEvalEnum", autoload: false) ? "E" : "e";
echo function_exists("trait_exists"); echo function_exists("enum_exists");');
"#,
    );
    assert_eq!(out, "TTtEEeTEte11");
}

/// Verifies eval fragments can declare and use backed enums through the bridge.
#[test]
fn test_eval_fragment_declares_enum_cases_and_methods() {
    let out = compile_and_run(
        r#"<?php
eval('interface EvalDynLabel { function label(); }
enum EvalDynColor: string implements EvalDynLabel {
    case Red = "r";
    case Green = "g";
    public const PREFIX = "color";
    public function label() { return self::PREFIX . ":" . $this->name . ":" . $this->value; }
    public static function fallback() { return self::Red; }
}
$cases = EvalDynColor::cases();
echo enum_exists("evaldyncolor") ? "E" : "e";
echo class_exists("EvalDynColor") ? "C" : "c";
echo count($cases);
echo $cases[1] === EvalDynColor::Green ? "G" : "g";
echo EvalDynColor::Green->label();
echo EvalDynColor::from("r") === EvalDynColor::Red ? "F" : "f";
echo is_null(EvalDynColor::tryFrom("missing")) ? "N" : "n";
echo is_a(EvalDynColor::Red, "EvalDynLabel") ? "I" : "i";');
"#,
    );
    assert_eq!(out, "EC2Gcolor:Green:gFNI");
}

/// Verifies eval enum `from()` misses throw catchable `ValueError` objects.
#[test]
fn test_eval_fragment_enum_from_miss_throws_value_error() {
    let out = compile_and_run(
        r#"<?php
eval('enum EvalDynStatus: string {
    case Draft = "draft";
}
try {
    EvalDynStatus::from("live");
    echo "bad";
} catch (ValueError $e) {
    echo get_class($e), ":", $e->getMessage();
}');
"#,
    );
    assert_eq!(
        out,
        "ValueError:\"live\" is not a valid backing value for enum EvalDynStatus"
    );
}

/// Verifies eval `is_a()` and `is_subclass_of()` use generated AOT relation metadata.
#[test]
fn test_eval_fragment_is_a_relation_probes_aot_metadata() {
    let out = compile_and_run(
        r#"<?php
interface EvalRelationIface {}
class EvalRelationParent {}
class EvalRelationChild extends EvalRelationParent implements EvalRelationIface {}

eval('$object = new EvalRelationChild();
echo is_a($object, "EvalRelationChild") ? "Y" : "N";
echo is_a($object, "EvalRelationParent") ? "Y" : "N";
echo is_a($object, "EvalRelationIface") ? "Y" : "N";
echo is_subclass_of($object, "EvalRelationChild") ? "Y" : "N";
echo is_subclass_of($object, "EvalRelationParent") ? "Y" : "N";
echo is_subclass_of($object, "EvalRelationIface") ? "Y" : "N";
echo call_user_func("is_a", $object, "EvalRelationParent") ? "Y" : "N";
echo call_user_func_array("is_subclass_of", ["object_or_class" => $object, "class" => "EvalRelationParent"]) ? "Y" : "N";
echo is_a(object_or_class: $object, class: "MissingEvalRelation", allow_string: false) ? "Y" : "N";
echo function_exists("is_a"); echo function_exists("is_subclass_of");');
"#,
    );
    assert_eq!(out, "YYYNYYYYN11");
}

/// Verifies eval `instanceof` probes AOT and eval-declared class metadata.
#[test]
fn test_eval_fragment_instanceof_probes_class_metadata() {
    let out = compile_and_run_capture(
        r#"<?php
interface EvalInstanceAotIface {}
class EvalInstanceAotParent {}
class EvalInstanceAotChild extends EvalInstanceAotParent implements EvalInstanceAotIface {}

eval('interface EvalInstanceDynIface {}
class EvalInstanceDynBase {}
class EvalInstanceDynChild extends EvalInstanceDynBase implements EvalInstanceDynIface {}
$aot = new EvalInstanceAotChild();
$dyn = new EvalInstanceDynChild();
$dynName = "EvalInstanceDynChild";
$dynTargets = ["EvalInstanceDynIface"];
$prefix = "EvalInstanceDyn";
$suffix = "Base";
$dynTargetObject = new EvalInstanceDynChild();
echo $aot instanceof EvalInstanceAotChild ? "A" : "a";
echo $aot instanceof EvalInstanceAotParent ? "P" : "p";
echo $aot instanceof EvalInstanceAotIface ? "I" : "i";
echo $dyn instanceof EvalInstanceDynChild ? "C" : "c";
echo $dyn instanceof EvalInstanceDynBase ? "B" : "b";
echo $dyn instanceof EvalInstanceDynIface ? "F" : "f";
echo $dyn instanceof $dynName ? "D" : "d";
echo $dyn instanceof $dynTargets[0] ? "T" : "t";
echo $dyn instanceof ($prefix . $suffix) ? "X" : "x";
echo $dyn instanceof $dynTargetObject ? "O" : "o";
echo 7 instanceof MissingEvalInstance ? "bad" : "S";');
"#,
    );
    assert!(
        out.success,
        "program failed: stdout={:?} stderr={}",
        out.stdout, out.stderr
    );
    assert_eq!(out.stdout, "APICBFDTXOS");
}

/// Verifies eval-declared class inheritance uses dynamic methods and metadata.
#[test]
fn test_eval_declared_class_inherits_methods_and_metadata() {
    let out = compile_and_run_capture(
        r#"<?php
interface EvalDynIface {}

eval('class EvalDynBase {
    public int $base = 1;
    public function __construct($base) { $this->base = $base; }
    public function sum($n) { return $this->base + $this->tail + $n; }
}
class EvalDynChild extends EvalDynBase implements EvalDynIface {
    public int $tail = 4;
    public function read($n) { return $this->sum($n); }
}
$box = new EvalDynChild(3);
echo $box->read(5) . ":";
echo get_parent_class($box) . ":";
echo is_a($box, "EvalDynBase") ? "isa" : "bad"; echo ":";
echo is_a($box, "EvalDynIface") ? "iface" : "bad"; echo ":";
echo is_subclass_of($box, "EvalDynChild") ? "bad" : "self"; echo ":";
echo is_subclass_of($box, "EvalDynBase") ? "sub" : "bad"; echo ":";
$parents = class_parents($box);
echo count($parents) . ":" . $parents["EvalDynBase"] . ":";
$implements = class_implements("EvalDynChild");
echo count($implements) . ":" . $implements["EvalDynIface"];');
"#,
    );
    assert!(
        out.success,
        "program failed: stdout={:?} stderr={}",
        out.stdout, out.stderr
    );
    assert_eq!(
        out.stdout,
        "12:EvalDynBase:isa:iface:self:sub:1:EvalDynBase:1:EvalDynIface"
    );
}

/// Verifies eval-declared interfaces are usable by eval-declared classes.
#[test]
fn test_eval_declared_interface_metadata_and_implementation() {
    let out = compile_and_run_capture(
        r#"<?php
eval('interface EvalDynReader {
    function read($n);
}
interface EvalDynNamedReader extends EvalDynReader {
    function label();
}
class EvalDynReaderBox implements EvalDynNamedReader {
    public function read($n) { return $n + 1; }
    public function label() { return "box"; }
}
$box = new EvalDynReaderBox();
echo interface_exists("EvalDynReader") ? "iface" : "bad"; echo ":";
echo class_exists("EvalDynReader") ? "bad" : "notclass"; echo ":";
echo count(get_declared_interfaces()) . ":";
echo $box->read(4) . ":";
echo $box->label() . ":";
echo is_a($box, "EvalDynNamedReader") ? "isa" : "bad"; echo ":";
echo is_subclass_of("EvalDynReaderBox", "EvalDynReader") ? "str" : "bad"; echo ":";
$implements = class_implements($box);
echo count($implements) . ":" . $implements["EvalDynNamedReader"] . ":" . $implements["EvalDynReader"];');
"#,
    );
    assert!(
        out.success,
        "program failed: stdout={:?} stderr={}",
        out.stdout, out.stderr
    );
    assert_eq!(
        out.stdout,
        "iface:notclass:2:5:box:isa:str:2:EvalDynNamedReader:EvalDynReader"
    );
}

/// Verifies eval-declared method overrides enforce covariant return types.
#[test]
fn test_eval_declared_method_return_type_override_contracts() {
    let out = compile_and_run_capture(
        r#"<?php
eval('class EvalReturnBase {
    public function id(): ?int { return 1; }
    public function make(): EvalReturnBase { return $this; }
    public function selfType(): self { return $this; }
}
class EvalReturnChild extends EvalReturnBase {
    public function id(): int { return 2; }
    public function make(): EvalReturnChild { return $this; }
    public function selfType(): static { return $this; }
}
class EvalReturnParentRoot {}
class EvalReturnParentBase extends EvalReturnParentRoot {
    public function parentKeyword(): EvalReturnParentRoot { return new EvalReturnParentRoot(); }
}
class EvalReturnParentChild extends EvalReturnParentBase {
    public function parentKeyword(): parent { return new EvalReturnParentBase(); }
}
class EvalReturnMixedBase {
    public function maybe(): mixed { return null; }
}
class EvalReturnMixedChild extends EvalReturnMixedBase {
    public function maybe(): ?int { return null; }
}
$child = new EvalReturnChild();
echo $child->id();');
"#,
    );
    assert!(
        out.success,
        "program failed: stdout={:?} stderr={}",
        out.stdout, out.stderr
    );
    assert_eq!(out.stdout, "2");

    let err = compile_and_run_expect_failure(
        r#"<?php
eval('class EvalReturnNarrowBase {
    public function id(): int { return 1; }
}
class EvalReturnWiderNullable extends EvalReturnNarrowBase {
    public function id(): ?int { return 2; }
}');
"#,
    );
    assert!(
        err.contains("Fatal error: eval() runtime failed"),
        "stderr did not contain eval runtime fatal diagnostic: {err}"
    );

    let err = compile_and_run_expect_failure(
        r#"<?php
eval('class EvalReturnStaticBase {
    public function make(): static { return $this; }
}
class EvalReturnSelfChild extends EvalReturnStaticBase {
    public function make(): self { return $this; }
}');
"#,
    );
    assert!(
        err.contains("Fatal error: eval() runtime failed"),
        "stderr did not contain eval runtime fatal diagnostic: {err}"
    );

    let err = compile_and_run_expect_failure(
        r#"<?php
eval('class EvalReturnNullableBase {
    public function maybe(): ?int { return null; }
}
class EvalReturnMixedChildBad extends EvalReturnNullableBase {
    public function maybe(): mixed { return null; }
}');
"#,
    );
    assert!(
        err.contains("Fatal error: eval() runtime failed"),
        "stderr did not contain eval runtime fatal diagnostic: {err}"
    );
}

/// Verifies eval-declared interface methods enforce covariant return types.
#[test]
fn test_eval_declared_interface_return_type_contracts() {
    let out = compile_and_run_capture(
        r#"<?php
eval('interface EvalReturnReadable {
    function read(): int|string;
}
class EvalReturnReader implements EvalReturnReadable {
    public function read(): int {
        return 7;
    }
}
interface EvalReturnRootSelf {
    function linked(): self;
}
interface EvalReturnChildSelf extends EvalReturnRootSelf {}
class EvalReturnSelfImpl implements EvalReturnChildSelf {
    public function linked(): EvalReturnRootSelf {
        return $this;
    }
}
$reader = new EvalReturnReader();
echo $reader->read();');
"#,
    );
    assert!(
        out.success,
        "program failed: stdout={:?} stderr={}",
        out.stdout, out.stderr
    );
    assert_eq!(out.stdout, "7");

    let err = compile_and_run_expect_failure(
        r#"<?php
eval('interface EvalNeedsReturn {
    function read(): string;
}
class EvalMissingReturnImpl implements EvalNeedsReturn {
    public function read() { return "bad"; }
}');
"#,
    );
    assert!(
        err.contains("Fatal error: eval() runtime failed"),
        "stderr did not contain eval runtime fatal diagnostic: {err}"
    );

    let err = compile_and_run_expect_failure(
        r#"<?php
eval('interface EvalNeedsStringReturn {
    function read(): string;
}
class EvalWiderReturnImpl implements EvalNeedsStringReturn {
    public function read(): int|string { return "bad"; }
}');
"#,
    );
    assert!(
        err.contains("Fatal error: eval() runtime failed"),
        "stderr did not contain eval runtime fatal diagnostic: {err}"
    );
}

/// Verifies eval-declared methods enforce declared return values at runtime.
#[test]
fn test_eval_declared_method_return_type_values() {
    let out = compile_and_run_capture(
        r#"<?php
eval('class EvalReturnRuntimeBase {
    public function id(): int { return "12"; }
    public function makeSelf(): self { return new EvalReturnRuntimeBase(); }
    public function done(): void { return; }
}
class EvalReturnRuntimeChild extends EvalReturnRuntimeBase {}
$child = new EvalReturnRuntimeChild();
echo $child->id();
echo ":" . get_class($child->makeSelf()) . ":";
$child->done();');
"#,
    );
    assert!(
        out.success,
        "program failed: stdout={:?} stderr={}",
        out.stdout, out.stderr
    );
    assert_eq!(out.stdout, "12:EvalReturnRuntimeBase:");

    let err = compile_and_run_expect_failure(
        r#"<?php
eval('class EvalReturnBadScalar {
    public function id(): int { return "nope"; }
}
$box = new EvalReturnBadScalar();
echo $box->id();');
"#,
    );
    assert!(
        err.contains("Fatal error: eval() runtime failed"),
        "stderr did not contain eval runtime fatal diagnostic: {err}"
    );

    let err = compile_and_run_expect_failure(
        r#"<?php
eval('class EvalReturnBadVoid {
    public function done(): void { return null; }
}
$box = new EvalReturnBadVoid();
$box->done();');
"#,
    );
    assert!(
        err.contains("Fatal error: eval() runtime failed"),
        "stderr did not contain eval runtime fatal diagnostic: {err}"
    );

    let err = compile_and_run_expect_failure(
        r#"<?php
eval('class EvalReturnStaticRuntimeBase {
    public function make(): static { return new EvalReturnStaticRuntimeBase(); }
}
class EvalReturnStaticRuntimeChild extends EvalReturnStaticRuntimeBase {}
$child = new EvalReturnStaticRuntimeChild();
$child->make();');
"#,
    );
    assert!(
        err.contains("Fatal error: eval() runtime failed"),
        "stderr did not contain eval runtime fatal diagnostic: {err}"
    );

    let err = compile_and_run_expect_failure(
        r#"<?php
eval('class EvalReturnImplicitBad {
    public function id(): ?int {}
}
$box = new EvalReturnImplicitBad();
$box->id();');
"#,
    );
    assert!(
        err.contains("Fatal error: eval() runtime failed"),
        "stderr did not contain eval runtime fatal diagnostic: {err}"
    );
}

/// Verifies eval-declared abstract classes can defer interface methods to concrete children.
#[test]
fn test_eval_declared_abstract_class_and_final_method_contracts() {
    let out = compile_and_run_capture(
        r#"<?php
eval('interface EvalAbstractContract {
    function read($n);
}
abstract class EvalAbstractBase implements EvalAbstractContract {
    abstract public function read($n);
    final public function label() { return "base"; }
    public function wrap($n) { return $this->read($n) + 1; }
}
class EvalAbstractChild extends EvalAbstractBase {
    public function read($n) { return $n + 2; }
}
$box = new EvalAbstractChild();
echo $box->wrap(5) . ":";
echo $box->label() . ":";
echo is_a($box, "EvalAbstractContract") ? "iface" : "bad"; echo ":";
echo is_subclass_of($box, "EvalAbstractBase") ? "abstract" : "bad";');
"#,
    );
    assert!(
        out.success,
        "program failed: stdout={:?} stderr={}",
        out.stdout, out.stderr
    );
    assert_eq!(out.stdout, "8:base:iface:abstract");
}

/// Verifies eval-declared final classes cannot be extended.
#[test]
fn test_eval_declared_final_class_extension_fails() {
    let err = compile_and_run_expect_failure(
        r#"<?php
eval('final class EvalFinalBase {}
class EvalFinalChild extends EvalFinalBase {}');
"#,
    );
    assert!(
        err.contains("Fatal error: eval() runtime failed"),
        "stderr did not contain eval runtime fatal diagnostic: {err}"
    );
}

/// Verifies eval-declared traits contribute methods, properties, and metadata through the bridge.
#[test]
fn test_eval_declared_trait_methods_properties_and_metadata() {
    let out = compile_and_run_capture(
        r#"<?php
eval('trait EvalDynamicTrait {
    public int $seed = 2;
    public function add($n) { return $this->seed + $n; }
}
class EvalDynamicTraitBox {
    use EvalDynamicTrait;
    public function read($n) { return $this->add($n) + 1; }
}
$box = new EvalDynamicTraitBox();
echo $box->read(4) . ":";
echo trait_exists("EvalDynamicTrait") ? "trait" : "bad"; echo ":";
$traits = get_declared_traits();
echo count($traits) . ":" . $traits[0] . ":";
$uses = class_uses($box);
echo count($uses) . ":" . $uses["EvalDynamicTrait"] . ":";
echo $box->seed;');
"#,
    );
    assert!(
        out.success,
        "program failed: stdout={:?} stderr={}",
        out.stdout, out.stderr
    );
    assert_eq!(
        out.stdout,
        "7:trait:1:EvalDynamicTrait:1:EvalDynamicTrait:2"
    );
}

/// Verifies eval-declared trait adaptations resolve conflicts, aliases, and visibility.
#[test]
fn test_eval_declared_trait_adaptations() {
    let out = compile_and_run_capture(
        r#"<?php
eval('trait EvalAdaptA {
    public function talk() { return "A"; }
    public function hidden() { return "secret"; }
}
trait EvalAdaptB {
    public function talk() { return "B"; }
}
class EvalAdaptBox {
    use EvalAdaptA, EvalAdaptB {
        EvalAdaptA::talk insteadof EvalAdaptB;
        EvalAdaptB::talk as talkB;
        EvalAdaptA::hidden as private;
    }
    public function read() {
        return $this->talk() . ":" . $this->talkB() . ":" . $this->hidden();
    }
}
$box = new EvalAdaptBox();
echo $box->read() . ":";
echo $box->talk();');
"#,
    );
    assert!(
        out.success,
        "program failed: stdout={:?} stderr={}",
        out.stdout, out.stderr
    );
    assert_eq!(out.stdout, "A:B:secret:A");
}

/// Verifies eval-declared trait visibility adaptations affect bridge access checks.
#[test]
fn test_eval_declared_trait_visibility_adaptation_fails() {
    let err = compile_and_run_expect_failure(
        r#"<?php
eval('trait EvalAdaptHidden {
    public function hidden() { return "secret"; }
}
class EvalAdaptHiddenBox {
    use EvalAdaptHidden {
        EvalAdaptHidden::hidden as private;
    }
}
$box = new EvalAdaptHiddenBox();
echo $box->hidden();');
"#,
    );
    assert!(
        err.contains("Fatal error: eval() runtime failed"),
        "stderr did not contain eval runtime fatal diagnostic: {err}"
    );
}

/// Verifies eval-declared trait abstract methods must be implemented by concrete classes.
#[test]
fn test_eval_declared_trait_abstract_method_requirement_fails() {
    let err = compile_and_run_expect_failure(
        r#"<?php
eval('trait EvalTraitNeedsConcrete {
    abstract public function read();
}
class EvalTraitMissingConcrete {
    use EvalTraitNeedsConcrete;
}');
"#,
    );
    assert!(
        err.contains("Fatal error: eval() runtime failed"),
        "stderr did not contain eval runtime fatal diagnostic: {err}"
    );
}

/// Verifies eval-declared private/protected members are usable from valid class scopes.
#[test]
fn test_eval_declared_private_and_protected_members() {
    let out = compile_and_run_capture(
        r#"<?php
eval('class EvalVisibilityBase {
    private int $secret = 4;
    protected int $base = 5;
    private function bump($n) { return $this->secret + $n; }
    protected function add($n) { return $this->base + $n; }
    public function readPrivate($n) { return $this->bump($n); }
}
class EvalVisibilityChild extends EvalVisibilityBase {
    public function readProtected($n) { return $this->add($n); }
}
$box = new EvalVisibilityChild();
echo $box->readPrivate(3) . ":";
echo $box->readProtected(2);');
"#,
    );
    assert!(
        out.success,
        "program failed: stdout={:?} stderr={}",
        out.stdout, out.stderr
    );
    assert_eq!(out.stdout, "7:7");
}

/// Verifies eval OOP introspection builtins preserve PHP visibility and scope rules.
#[test]
fn test_eval_declared_oop_introspection_builtins() {
    let out = compile_and_run_capture(
        r#"<?php
eval('class EvalOopIntrospectBase {
    private $baseSecret = "bp";
    protected $baseProtected = "bq";
    public $basePublic = "br";
    private function basePrivate() {}
    protected function baseProtectedMethod() {}
    public function basePublicMethod() {}
    public function parentView() {
        $vars = get_object_vars($this);
        ksort($vars);
        echo implode(",", array_keys($vars));
    }
}
class EvalOopIntrospectChild extends EvalOopIntrospectBase {
    private $childSecret = "cp";
    protected $childProtected = "cq";
    public $childPublic = "cr";
    private function childPrivate() {}
    protected function childProtectedMethod() {}
    public function childPublicMethod() {}
    public function childView() {
        $methods = get_class_methods($this);
        sort($methods);
        echo implode(",", $methods); echo "|";
        $vars = get_object_vars($this);
        ksort($vars);
        echo implode(",", array_keys($vars));
    }
}
$object = new EvalOopIntrospectChild();
$object->dynamic = "dyn";
echo method_exists("EvalOopIntrospectChild", "basePrivate") ? "bad" : "noParentPrivateMethod"; echo ":";
echo method_exists($object, "basePrivate") ? "objectParentPrivateMethod" : "bad"; echo ":";
echo method_exists("EvalOopIntrospectChild", "baseProtectedMethod") ? "classProtectedMethod" : "bad"; echo ":";
echo property_exists("EvalOopIntrospectChild", "baseSecret") ? "bad" : "noParentPrivateProperty"; echo ":";
echo property_exists($object, "baseSecret") ? "bad" : "noObjectParentPrivateProperty"; echo ":";
echo property_exists($object, "dynamic") ? "dynamicProperty" : "bad"; echo ":";
$methods = get_class_methods("EvalOopIntrospectChild");
sort($methods);
echo implode(",", $methods); echo ":";
$vars = get_object_vars($object);
ksort($vars);
echo implode(",", array_keys($vars)); echo ":";
$object->childView(); echo ":";
$object->parentView(); echo ":";
echo call_user_func("method_exists", $object, "childPrivate") ? "callMethod" : "bad"; echo ":";
echo call_user_func_array("property_exists", ["property" => "dynamic", "object_or_class" => $object]) ? "namedProperty" : "bad"; echo ":";
echo function_exists("method_exists"); echo function_exists("property_exists");
echo function_exists("get_class_methods"); echo function_exists("get_object_vars");');
"#,
    );
    assert!(
        out.success,
        "program failed: stdout={:?} stderr={}",
        out.stdout, out.stderr
    );
    assert_eq!(
        out.stdout,
        "noParentPrivateMethod:objectParentPrivateMethod:classProtectedMethod:noParentPrivateProperty:noObjectParentPrivateProperty:dynamicProperty:basePublicMethod,childPublicMethod,childView,parentView:basePublic,childPublic,dynamic:baseProtectedMethod,basePublicMethod,childPrivate,childProtectedMethod,childPublicMethod,childView,parentView|baseProtected,basePublic,childProtected,childPublic,childSecret,dynamic:baseProtected,basePublic,baseSecret,childProtected,childPublic,dynamic:callMethod:namedProperty:1111"
    );
}

/// Verifies eval-declared private parent properties keep separate storage when a child shadows them.
#[test]
fn test_eval_declared_private_parent_property_shadowing() {
    let out = compile_and_run_capture(
        r#"<?php
eval('class EvalShadowGrand {
    private $value = 1;
    public function grandValue() { return $this->value; }
}
class EvalShadowParent extends EvalShadowGrand {
    public $value = 2;
    public function parentValue() { return $this->value; }
}
class EvalShadowChild extends EvalShadowParent {
    public $value = 3;
}
$box = new EvalShadowChild();
echo $box->grandValue() . ":";
echo $box->parentValue() . ":";
echo $box->value;');
"#,
    );
    assert!(
        out.success,
        "program failed: stdout={:?} stderr={}",
        out.stdout, out.stderr
    );
    assert_eq!(out.stdout, "1:3:3");
}

/// Verifies eval-declared readonly properties can be initialized only in constructors.
#[test]
fn test_eval_declared_readonly_property_rules() {
    let out = compile_and_run_capture(
        r#"<?php
eval('class EvalReadonlyBox {
    public readonly int $id;
    public function __construct($id) { $this->id = $id; }
    public function id() { return $this->id; }
}
$box = new EvalReadonlyBox(7);
echo $box->id();');
"#,
    );
    assert!(
        out.success,
        "program failed: stdout={:?} stderr={}",
        out.stdout, out.stderr
    );
    assert_eq!(out.stdout, "7");

    let err = compile_and_run_expect_failure(
        r#"<?php
eval('class EvalReadonlyFailBox {
    public readonly int $id;
    public function __construct($id) { $this->id = $id; }
    public function replace($id) { $this->id = $id; }
}
$box = new EvalReadonlyFailBox(7);
$box->replace(8);');
"#,
    );
    assert!(
        err.contains("Fatal error: eval() runtime failed"),
        "stderr did not contain eval runtime fatal diagnostic: {err}"
    );
}

/// Verifies eval-declared readonly classes mirror instance/static property rules.
#[test]
fn test_eval_declared_readonly_class_rules() {
    let out = compile_and_run_capture(
        r#"<?php
eval('readonly class EvalReadonlyClassBox {
    public int $id;
    public static int $count = 1;
    public function __construct($id) { $this->id = $id; }
    public function id() { return $this->id; }
}
readonly class EvalReadonlyClassChild extends EvalReadonlyClassBox {}
$box = new EvalReadonlyClassBox(7);
$child = new EvalReadonlyClassChild(9);
EvalReadonlyClassBox::$count = 5;
echo $box->id() . ":" . EvalReadonlyClassBox::$count . ":" . $child->id();');
"#,
    );
    assert!(
        out.success,
        "program failed: stdout={:?} stderr={}",
        out.stdout, out.stderr
    );
    assert_eq!(out.stdout, "7:5:9");

    let err = compile_and_run_expect_failure(
        r#"<?php
eval('readonly class EvalReadonlyClassFailBox {
    public int $id;
    public function __construct($id) { $this->id = $id; }
    public function replace($id) { $this->id = $id; }
}
$box = new EvalReadonlyClassFailBox(7);
$box->replace(8);');
"#,
    );
    assert!(
        err.contains("Fatal error: eval() runtime failed"),
        "stderr did not contain eval runtime fatal diagnostic: {err}"
    );

    let parent_err = compile_and_run_expect_failure(
        r#"<?php
eval('class EvalReadonlyClassBase {}
readonly class EvalReadonlyClassChild extends EvalReadonlyClassBase {}');
"#,
    );
    assert!(
        parent_err.contains("Fatal error: eval() runtime failed"),
        "stderr did not contain eval runtime fatal diagnostic: {parent_err}"
    );
}

/// Verifies eval-declared property hooks route get/set access through accessors.
#[test]
fn test_eval_declared_property_hooks() {
    let out = compile_and_run_capture(
        r#"<?php
eval('class EvalHookName {
    public string $value {
        get => $this->value;
        set { $this->value = $value . "!"; }
    }
}
class EvalHookChild extends EvalHookName {
    public function shout() { return $this->value . "?"; }
}
$box = new EvalHookChild();
$box->value = "Ada";
echo $box->value . ":" . $box->shout();');
"#,
    );
    assert!(
        out.success,
        "program failed: stdout={:?} stderr={}",
        out.stdout, out.stderr
    );
    assert_eq!(out.stdout, "Ada!:Ada!?");

    let err = compile_and_run_expect_failure(
        r#"<?php
eval('class EvalHookReadOnly {
    public int $answer {
        get => 42;
    }
}
$box = new EvalHookReadOnly();
$box->answer = 7;');
"#,
    );
    assert!(
        err.contains("Fatal error: eval() runtime failed"),
        "stderr did not contain eval runtime fatal diagnostic: {err}"
    );
}

/// Verifies eval-declared magic property methods handle missing and inaccessible properties.
#[test]
fn test_eval_declared_magic_property_methods() {
    let out = compile_and_run_capture(
        r#"<?php
eval('class EvalMagicPropertyBox {
    private string $secret = "raw";
    public string $events = "";
    public function readOwn() { return $this->secret; }
    public function __get($name) {
        $this->events = $this->events . "get:" . $name . ";";
        return "read:" . $name;
    }
    public function __set($name, $value) {
        $this->events = $this->events . "set:" . $name . "=" . $value . ";";
    }
}
$box = new EvalMagicPropertyBox();
echo $box->readOwn() . ":";
echo $box->secret . ":";
echo $box->missing . ":";
$box->secret = "new";
$box->other = "B";
$box->events = $box->events . "public;";
echo $box->events;');
"#,
    );
    assert!(
        out.success,
        "program failed: stdout={:?} stderr={}",
        out.stdout, out.stderr
    );
    assert_eq!(
        out.stdout,
        "raw:read:secret:read:missing:get:secret;get:missing;set:secret=new;set:other=B;public;"
    );
}

/// Verifies eval reads existing dynamic properties before falling back to `__get`.
#[test]
fn test_eval_declared_magic_get_preserves_existing_dynamic_property() {
    let out = compile_and_run_capture(
        r#"<?php
eval('class EvalMagicExistingDynamicBox {
    public function __get($name) {
        return "magic:" . $name;
    }
}
$box = new EvalMagicExistingDynamicBox();
$box->known = "plain";
echo $box->known . ":";
echo $box->missing;');
"#,
    );
    assert!(
        out.success,
        "program failed: stdout={:?} stderr={}",
        out.stdout, out.stderr
    );
    assert_eq!(out.stdout, "plain:magic:missing");
}

/// Verifies eval property probes and unsets dispatch through `__isset` and `__unset`.
#[test]
fn test_eval_declared_magic_isset_empty_and_unset_property_methods() {
    let out = compile_and_run_capture(
        r#"<?php
eval('class EvalMagicPropertyProbeBox {
    public string $events = "";
    public string $present = "ready";
    public $nullish = null;
    private string $secret = "raw";
    public function __isset($name) {
        $this->events = $this->events . "isset:" . $name . ";";
        return $name !== "no";
    }
    public function __get($name) {
        $this->events = $this->events . "get:" . $name . ";";
        return $name === "empty" ? "" : "value:" . $name;
    }
    public function __unset($name) {
        $this->events = $this->events . "unset:" . $name . ";";
    }
}
$box = new EvalMagicPropertyProbeBox();
echo isset($box->present) ? "P" : "p"; echo ":";
echo isset($box->nullish) ? "N" : "n"; echo ":";
echo isset($box->secret) ? "S" : "s"; echo ":";
echo isset($box->no) ? "bad" : "no"; echo ":";
echo empty($box->secret) ? "bad" : "filled"; echo ":";
echo empty($box->empty) ? "empty" : "bad"; echo ":";
unset($box->present);
unset($box->secret);
unset($box->missing);
echo isset($box->present) ? "bad" : "unset"; echo ":";
echo $box->events;');
"#,
    );
    assert!(
        out.success,
        "program failed: stdout={:?} stderr={}",
        out.stdout, out.stderr
    );
    assert_eq!(
        out.stdout,
        "P:n:S:no:filled:empty:unset:isset:secret;isset:no;isset:secret;get:secret;isset:empty;get:empty;unset:secret;unset:missing;"
    );
}

/// Verifies eval-declared interface property hook contracts validate class properties.
#[test]
fn test_eval_declared_interface_property_hook_contracts() {
    let out = compile_and_run_capture(
        r#"<?php
eval('interface EvalIfaceHookContract {
    public string $value { get; set; }
}
interface EvalIfaceNamedHookContract extends EvalIfaceHookContract {
    public string $name { get; }
}
class EvalIfaceHookBox implements EvalIfaceNamedHookContract {
    public string $name = "box";
    public string $value {
        get => $this->value;
        set { $this->value = $value . "!"; }
    }
}
class EvalIfacePlainBox implements EvalIfaceHookContract {
    public string $value = "Grace";
}
$box = new EvalIfaceHookBox();
$box->value = "Ada";
$plain = new EvalIfacePlainBox();
echo $box->name . ":" . $box->value . ":" . $plain->value;');
"#,
    );
    assert!(
        out.success,
        "program failed: stdout={:?} stderr={}",
        out.stdout, out.stderr
    );
    assert_eq!(out.stdout, "box:Ada!:Grace");

    let err = compile_and_run_expect_failure(
        r#"<?php
eval('interface EvalIfaceHookSetContract {
    public int $answer { get; set; }
}
class EvalIfaceHookReadOnlyBox implements EvalIfaceHookSetContract {
    public int $answer {
        get => 42;
    }
}');
"#,
    );
    assert!(
        err.contains("Fatal error: eval() runtime failed"),
        "stderr did not contain eval runtime fatal diagnostic: {err}"
    );
}

/// Verifies eval-declared abstract property hook contracts validate concrete subclasses.
#[test]
fn test_eval_declared_abstract_property_hook_contracts() {
    let out = compile_and_run_capture(
        r#"<?php
eval('abstract class EvalAbstractHookBase {
    abstract public string $value { get; set; }
}
class EvalAbstractHookBox extends EvalAbstractHookBase {
    public string $value {
        get => $this->value;
        set { $this->value = $value . "!"; }
    }
}
class EvalPlainAbstractHookBox extends EvalAbstractHookBase {
    public string $value = "Grace";
}
$box = new EvalAbstractHookBox();
$box->value = "Ada";
$plain = new EvalPlainAbstractHookBox();
echo $box->value . ":" . $plain->value;');
"#,
    );
    assert!(
        out.success,
        "program failed: stdout={:?} stderr={}",
        out.stdout, out.stderr
    );
    assert_eq!(out.stdout, "Ada!:Grace");

    let err = compile_and_run_expect_failure(
        r#"<?php
eval('abstract class EvalMissingAbstractHookBase {
    abstract public string $value { get; }
}
class EvalMissingAbstractHookBox extends EvalMissingAbstractHookBase {}');
"#,
    );
    assert!(
        err.contains("Fatal error: eval() runtime failed"),
        "stderr did not contain eval runtime fatal diagnostic: {err}"
    );
}

/// Verifies eval-declared static properties and static methods work through the bridge.
#[test]
fn test_eval_declared_static_members_and_late_static_binding() {
    let out = compile_and_run_capture(
        r#"<?php
eval('class EvalStaticCounter {
    public static int $count = 1;
    public static function bump($step) {
        self::$count += $step;
        return self::$count;
    }
}
class EvalStaticBase {
    protected static int $n = 2;
    public static function add($x) {
        static::$n += $x;
        return static::$n;
    }
    public static function baseRead() {
        return self::$n;
    }
}
class EvalStaticChild extends EvalStaticBase {
    protected static int $n = 10;
}
echo EvalStaticCounter::$count . ":";
echo EvalStaticCounter::bump(2) . ":";
echo EvalStaticCounter::$count . ":";
echo EvalStaticChild::add(4) . ":";
echo EvalStaticBase::add(3) . ":";
echo EvalStaticBase::baseRead();');
"#,
    );
    assert!(
        out.success,
        "program failed: stdout={:?} stderr={}",
        out.stdout, out.stderr
    );
    assert_eq!(out.stdout, "1:3:3:14:5:5");
}

/// Verifies eval-declared static interface methods are validated and reflected.
#[test]
fn test_eval_declared_static_interface_methods() {
    let out = compile_and_run_capture(
        r#"<?php
eval('interface EvalStaticContract {
    public static function make($value);
}
class EvalStaticContractImpl implements EvalStaticContract {
    public static function make($value) {
        return "S:" . $value;
    }
}
echo EvalStaticContractImpl::make("box") . ":";
$listed = (new ReflectionClass(EvalStaticContract::class))->getMethods()[0];
echo $listed->getName() . ":";
echo $listed->isStatic() ? "static" : "instance";
echo ":";
$method = new ReflectionMethod(EvalStaticContract::class, "make");
echo $method->isStatic() ? "S" : "s";');
"#,
    );
    assert!(
        out.success,
        "program failed: stdout={:?} stderr={}",
        out.stdout, out.stderr
    );
    assert_eq!(out.stdout, "S:box:make:static:S");

    let err = compile_and_run_expect_failure(
        r#"<?php
eval('interface EvalStaticMismatch {
    public static function read();
}
class EvalStaticMismatchImpl implements EvalStaticMismatch {
    public function read() {}
}');
"#,
    );
    assert!(
        err.contains("Fatal error: eval() runtime failed"),
        "stderr did not contain eval runtime fatal diagnostic: {err}"
    );
}

/// Verifies eval-declared constructors and methods bind named arguments.
#[test]
fn test_eval_declared_method_named_arguments() {
    let out = compile_and_run_capture(
        r#"<?php
eval('class EvalNamedMethodBox {
    public function __construct($left, $right) {
        $this->label = $left . $right;
    }
    public function read($left, $right) {
        return $this->label . ":" . $left . ":" . $right;
    }
    public static function join($left, $right) {
        return $left . "-" . $right;
    }
}
$box = new EvalNamedMethodBox(right: "B", left: "A");
echo $box->read(right: "D", left: "C") . ":";
$args = ["right" => "F", "left" => "E"];
echo $box->read(...$args) . ":";
echo EvalNamedMethodBox::join(right: "H", left: "G");');
"#,
    );
    assert!(
        out.success,
        "program failed: stdout={:?} stderr={}",
        out.stdout, out.stderr
    );
    assert_eq!(out.stdout, "AB:C:D:AB:E:F:G-H");
}

/// Verifies eval-declared constructors and methods bind constant-expression defaults.
#[test]
fn test_eval_declared_method_constant_default_arguments() {
    let out = compile_and_run_capture(
        r#"<?php
eval('define("EVAL_METHOD_DEFAULT_GLOBAL", "G");
class EvalDefaultConstBase {
    const LABEL = "base";
}
interface EvalDefaultConstIface {
    const WORD = "iface";
}
class EvalDefaultConstDep {
    public function __construct($label = "dep") {
        $this->label = $label;
    }
    public function read() {
        return $this->label;
    }
}
class EvalDefaultConstBox extends EvalDefaultConstBase {
    const LABEL = "box";
    public function __construct($label = self::LABEL) {
        $this->label = $label;
    }
    public function read($global = EVAL_METHOD_DEFAULT_GLOBAL, $parent = parent::LABEL, $iface = EvalDefaultConstIface::WORD, $class = self::class, $parentClass = parent::class, $items = [self::LABEL => 1 + 2, "fallback" => null ?? "fallback"], $method = __METHOD__, $dep = new EvalDefaultConstDep(label: "dep"), $clone = new self("inner")) {
        return $this->label . ":" . $global . ":" . $parent . ":" . $iface . ":" . $class . ":" . $parentClass . ":" . $items[self::LABEL] . ":" . $items["fallback"] . ":" . $method . ":" . $dep->read() . ":" . $clone->label;
    }
    public static function join($label = self::LABEL, $parent = parent::LABEL) {
        return $label . "-" . $parent;
    }
}
$box = new EvalDefaultConstBox();
echo $box->read() . ":";
echo EvalDefaultConstBox::join();');
"#,
    );
    assert!(
        out.success,
        "program failed: stdout={:?} stderr={}",
        out.stdout, out.stderr
    );
    assert_eq!(
        out.stdout,
        "box:G:base:iface:EvalDefaultConstBox:EvalDefaultConstBase:3:fallback:EvalDefaultConstBox::read:dep:inner:box-base"
    );
}

/// Verifies eval-declared constructors and methods bind variadic arguments.
#[test]
fn test_eval_declared_method_variadic_arguments() {
    let out = compile_and_run_capture(
        r#"<?php
eval('class EvalVariadicMethodBox {
    public function __construct(...$parts) {
        $this->label = $parts[0] . $parts["right"];
    }
    public function read($head, ...$tail) {
        echo count($tail) . ":";
        return $this->label . ":" . $head . ":" . $tail[0] . ":" . $tail["named"] . ":" . $tail["tail"];
    }
    public static function join(...$items) {
        return $items[0] . $items[1];
    }
}
$box = new EvalVariadicMethodBox("A", right: "B");
echo $box->read("C", "D", named: "E", tail: "F") . ":";
echo EvalVariadicMethodBox::join("G", "H");');
"#,
    );
    assert!(
        out.success,
        "program failed: stdout={:?} stderr={}",
        out.stdout, out.stderr
    );
    assert_eq!(out.stdout, "3:AB:C:D:E:F:GH");
}

/// Verifies eval-declared method parameter type hints are enforced through the bridge.
#[test]
fn test_eval_declared_method_parameter_type_hints() {
    let out = compile_and_run_capture(
        r#"<?php
eval('interface EvalTypedReadable {}
class EvalTypedDep implements EvalTypedReadable {}
class EvalTypedMethodBox {
    public function read(EvalTypedReadable $dep, int ...$items) {
        echo get_class($dep) . ":";
        return $items[0] + $items[1];
    }
}
$dep = new EvalTypedDep();
$box = new EvalTypedMethodBox();
echo $box->read($dep, "3", 4);');
"#,
    );
    assert!(
        out.success,
        "program failed: stdout={:?} stderr={}",
        out.stdout, out.stderr
    );
    assert_eq!(out.stdout, "EvalTypedDep:7");
}

/// Verifies eval-declared methods write back by-reference arguments through compiled eval calls.
#[test]
fn test_eval_declared_method_by_ref_arguments() {
    let out = compile_and_run_capture(
        r#"<?php
eval('class EvalByRefMethodBox {
    public function change(&$value) {
        $value = $value . "-method";
    }
    public static function changeStatic(&$value) {
        $value = $value . "-static";
    }
    public function changeVariadic(&...$items) {
        $items[0] = $items[0] . "-variadic";
        $items["named"] = $items["named"] . "-named";
    }
}
class EvalByRefPropertyBox {
    public string $value = "D";
}
$box = new EvalByRefMethodBox();
$value = "A";
$box->change($value);
EvalByRefMethodBox::changeStatic($value);
$named = "B";
$box->changeVariadic($value, named: $named);
$items = ["k" => "C"];
$box->change($items["k"]);
$prop = new EvalByRefPropertyBox();
$box->change($prop->value);
echo $value . ":" . $named . ":" . $items["k"] . ":" . $prop->value;');
"#,
    );
    assert!(
        out.success,
        "program failed: stdout={:?} stderr={}",
        out.stdout, out.stderr
    );
    assert_eq!(
        out.stdout,
        "A-method-static-variadic:B-named:C-method:D-method"
    );
}

/// Verifies eval dynamic static callables dispatch eval-declared static methods.
#[test]
fn test_eval_declared_static_method_dynamic_callables() {
    let out = compile_and_run_capture(
        r#"<?php
eval('class EvalStaticCallableBox {
    public static function join($left, $right) {
        return $left . $right;
    }
}
$cb = ["EvalStaticCallableBox", "join"];
echo $cb(right: "B", left: "A") . ":";
echo call_user_func($cb, "C", "D") . ":";
echo call_user_func_array($cb, ["right" => "F", "left" => "E"]) . ":";
$named = "EvalStaticCallableBox::join";
echo $named(right: "H", left: "G");');
"#,
    );
    assert!(
        out.success,
        "program failed: stdout={:?} stderr={}",
        out.stdout, out.stderr
    );
    assert_eq!(out.stdout, "AB:CD:EF:GH");
}

/// Verifies eval invokable objects dispatch through variable and callback call paths.
#[test]
fn test_eval_declared_invokable_object_dynamic_callables() {
    let out = compile_and_run_capture(
        r#"<?php
eval('class EvalInvokableBox {
    public function __construct($label = "box") {
        $this->label = $label;
    }
    public function __invoke($left = "A", $right = "B") {
        return $this->label . ":" . $left . $right;
    }
}
class EvalPlainCallableProbe {}
$box = new EvalInvokableBox("box");
$plain = new EvalPlainCallableProbe();
echo is_callable($box) ? "Y:" : "N:";
echo is_callable($plain) ? "bad:" : "plain:";
echo $box(right: "D", left: "C") . ":";
echo (new EvalInvokableBox("new"))("E", "F") . ":";
echo call_user_func($box, "G", "H") . ":";
echo call_user_func_array($box, ["right" => "J", "left" => "I"]);');
"#,
    );
    assert!(
        out.success,
        "program failed: stdout={:?} stderr={}",
        out.stdout, out.stderr
    );
    assert_eq!(out.stdout, "Y:plain:box:CD:new:EF:box:GH:box:IJ");
}

/// Verifies eval object method fallback dispatches missing and inaccessible methods through `__call`.
#[test]
fn test_eval_declared_magic_call_method_fallback() {
    let out = compile_and_run_capture(
        r#"<?php
eval('class EvalMagicCallBox {
    private function hidden($value) { return "bad"; }
    public function __call($method, $args) {
        return $method . ":" . $args[0] . ":" . $args["name"];
    }
}
$box = new EvalMagicCallBox();
echo $box->DoThing("A", name: "B") . ":";
echo $box->hidden("C", name: "D");');
"#,
    );
    assert!(
        out.success,
        "program failed: stdout={:?} stderr={}",
        out.stdout, out.stderr
    );
    assert_eq!(out.stdout, "DoThing:A:B:hidden:C:D");
}

/// Verifies eval static method fallback dispatches missing and inaccessible methods through `__callStatic`.
#[test]
fn test_eval_declared_magic_call_static_method_fallback() {
    let out = compile_and_run_capture(
        r#"<?php
eval('class EvalMagicStaticBox {
    private static function hidden($value) { return "bad"; }
    public static function __callStatic($method, $args) {
        return $method . ":" . $args[0] . ":" . $args["name"];
    }
}
echo EvalMagicStaticBox::DoStatic("A", name: "B") . ":";
echo EvalMagicStaticBox::Hidden("C", name: "D");');
"#,
    );
    assert!(
        out.success,
        "program failed: stdout={:?} stderr={}",
        out.stdout, out.stderr
    );
    assert_eq!(out.stdout, "DoStatic:A:B:Hidden:C:D");
}

/// Verifies eval rejects invalid magic method contracts during dynamic class declaration.
#[test]
fn test_eval_rejects_invalid_magic_method_contracts() {
    let err = compile_and_run_expect_failure(
        r#"<?php
eval('class EvalInvalidMagic {
    public function __call($method, ...$args) {
        return "bad";
    }
}');
"#,
    );
    assert!(
        err.contains("Fatal error: eval() runtime failed"),
        "stderr did not contain eval runtime fatal diagnostic: {err}"
    );
}

/// Verifies eval object-method callable arrays bind named arguments.
#[test]
fn test_eval_declared_object_method_callable_array_named_args() {
    let out = compile_and_run_capture(
        r#"<?php
eval('class EvalObjectCallableArrayBox {
    public function join($left, $right) {
        return $left . $right;
    }
}
$box = new EvalObjectCallableArrayBox();
$cb = [$box, "join"];
echo is_callable($cb) ? "Y:" : "N:";
echo call_user_func_array($cb, ["right" => "B", "left" => "A"]);');
"#,
    );
    assert!(
        out.success,
        "program failed: stdout={:?} stderr={}",
        out.stdout, out.stderr
    );
    assert_eq!(out.stdout, "Y:AB");
}

/// Verifies eval-declared class constants work through the bridge.
#[test]
fn test_eval_declared_class_constants_and_scoped_fetches() {
    let out = compile_and_run_capture(
        r#"<?php
eval('class EvalConstBase {
    public const SEED = 2;
    protected const HIDDEN = 5;
    public static function read() {
        return self::SEED + static::SEED;
    }
    public static function hidden() {
        return self::HIDDEN;
    }
}
class EvalConstChild extends EvalConstBase {
    public const SEED = 7;
    public static function readParent() {
        return parent::SEED;
    }
}
echo EvalConstBase::SEED . ":";
echo EvalConstChild::SEED . ":";
echo EvalConstChild::read() . ":";
echo EvalConstChild::readParent() . ":";
echo EvalConstChild::hidden();');
"#,
    );
    assert!(
        out.success,
        "program failed: stdout={:?} stderr={}",
        out.stdout, out.stderr
    );
    assert_eq!(out.stdout, "2:7:9:2:5");
}

/// Verifies eval-declared final class constants cannot be redeclared.
#[test]
fn test_eval_declared_final_class_constant_override_fails() {
    let err = compile_and_run_expect_failure(
        r#"<?php
eval('class EvalFinalConstBase {
    final public const SEED = 1;
}
class EvalFinalConstChild extends EvalFinalConstBase {
    public const SEED = 2;
}');
"#,
    );
    assert!(
        err.contains("Fatal error: eval() runtime failed"),
        "stderr did not contain eval runtime fatal diagnostic: {err}"
    );
}

/// Verifies eval-declared final private class constants are rejected.
#[test]
fn test_eval_declared_final_private_class_constant_fails() {
    let err = compile_and_run_expect_failure(
        r#"<?php
eval('class EvalFinalPrivateConst {
    final private const SEED = 1;
}');
"#,
    );
    assert!(
        err.contains("Fatal error: eval() runtime failed"),
        "stderr did not contain eval runtime fatal diagnostic: {err}"
    );
}

/// Verifies eval class-name literals work for class-like receivers.
#[test]
fn test_eval_declared_class_name_literals() {
    let out = compile_and_run_capture(
        r#"<?php
eval('class EvalClassNameBase {
    public static function selfName() { return self::class; }
    public static function staticName() { return static::class; }
}
class EvalClassNameChild extends EvalClassNameBase {}
interface EvalClassNameIface {}
trait EvalClassNameTrait {}
echo EvalClassNameChild::class . ":";
echo EvalClassNameIface::class . ":";
echo EvalClassNameTrait::class . ":";
echo EvalClassNameChild::selfName() . ":";
echo EvalClassNameChild::staticName();');
"#,
    );
    assert!(
        out.success,
        "program failed: stdout={:?} stderr={}",
        out.stdout, out.stderr
    );
    assert_eq!(
        out.stdout,
        "EvalClassNameChild:EvalClassNameIface:EvalClassNameTrait:EvalClassNameBase:EvalClassNameChild"
    );
}

/// Verifies eval-declared class attributes expose names and supported literal args.
#[test]
fn test_eval_declared_class_attribute_metadata() {
    let out = compile_and_run_capture(
        r#"<?php
eval('#[Route("/home", -1, true, null)]
#[Tag("first"), Tag("second")]
class EvalAttrMeta {}
$names = class_attribute_names("EvalAttrMeta");
echo count($names) . ":" . $names[0] . ":" . $names[1] . ":" . $names[2] . ":";
$args = class_attribute_args("EvalAttrMeta", "route");
echo count($args) . ":" . $args[0] . ":" . $args[1] . ":";
echo ($args[2] ? "T" : "F") . ":" . (is_null($args[3]) ? "N" : "bad") . ":";
$tag = class_attribute_args("evalattrmeta", "Tag");
echo $tag[0] . ":";
$attrs = class_get_attributes("EvalAttrMeta");
echo count($attrs) . ":" . $attrs[0]->getName() . ":";
$attrArgs = $attrs[0]->getArguments();
echo count($attrArgs) . ":" . $attrArgs[0] . ":" . $attrArgs[1] . ":";
echo ($attrArgs[2] ? "T" : "F") . ":" . (is_null($attrArgs[3]) ? "N" : "bad") . ":";
$tagArgs = $attrs[1]->getArguments();
echo $attrs[1]->getName() . ":" . $tagArgs[0] . ":";
echo is_null($attrs[0]->newInstance()) ? "N" : "bad";');
"#,
    );
    assert!(
        out.success,
        "program failed: stdout={:?} stderr={}",
        out.stdout, out.stderr
    );
    assert_eq!(
        out.stdout,
        "3:Route:Tag:Tag:4:/home:-1:T:N:first:3:Route:4:/home:-1:T:N:Tag:first:N"
    );
}

/// Verifies eval ReflectionAttribute::newInstance builds eval-declared attribute objects.
#[test]
fn test_eval_reflection_attribute_new_instance_for_eval_class() {
    let out = compile_and_run_capture(
        r#"<?php
eval('class EvalRoute {
    public $path;
    public $code;
    public $enabled;
    public function __construct($path, $code, $enabled) {
        $this->path = $path;
        $this->code = $code;
        $this->enabled = $enabled;
    }
    public function summary() {
        return $this->path . ":" . $this->code . ":" . ($this->enabled ? "T" : "F");
    }
}
#[EvalRoute("/home", -7, true)]
class EvalRouteTarget {}
$attrs = class_get_attributes("EvalRouteTarget");
$instance = $attrs[0]->newInstance();
echo get_class($instance) . ":" . $instance->summary();');
"#,
    );
    assert!(
        out.success,
        "program failed: stdout={:?} stderr={}",
        out.stdout, out.stderr
    );
    assert_eq!(out.stdout, "EvalRoute:/home:-7:T");
}

/// Verifies eval ReflectionClass/Method/Property expose eval-declared attributes.
#[test]
fn test_eval_reflection_member_attributes() {
    let out = compile_and_run_capture(
        r#"<?php
eval('class EvalMarker {
    public $name;
    public function __construct($name) {
        $this->name = $name;
    }
    public function label() {
        return $this->name;
    }
}
#[EvalMarker("class")]
class EvalReflectTarget {
    #[EvalMarker("method")]
    public function handle() {}
    #[EvalMarker("property")]
    public $id;
}
$classAttrs = (new ReflectionClass("EvalReflectTarget"))->getAttributes();
echo count($classAttrs) . ":" . (new ReflectionClass("EvalReflectTarget"))->getName() . ":";
echo $classAttrs[0]->getName() . ":" . $classAttrs[0]->newInstance()->label() . ":";
$methodAttrs = (new ReflectionMethod("EvalReflectTarget", "handle"))->getAttributes();
echo count($methodAttrs) . ":" . (new ReflectionMethod("EvalReflectTarget", "handle"))->getName() . ":";
echo $methodAttrs[0]->getName() . ":";
echo $methodAttrs[0]->getArguments()[0] . ":" . $methodAttrs[0]->newInstance()->label() . ":";
$propertyAttrs = (new ReflectionProperty("EvalReflectTarget", "id"))->getAttributes();
echo count($propertyAttrs) . ":" . (new ReflectionProperty("EvalReflectTarget", "id"))->getName() . ":";
echo $propertyAttrs[0]->getName() . ":";
echo $propertyAttrs[0]->getArguments()[0] . ":" . $propertyAttrs[0]->newInstance()->label();');
"#,
    );
    assert!(
        out.success,
        "program failed: stdout={:?} stderr={}",
        out.stdout, out.stderr
    );
    assert_eq!(
        out.stdout,
        "1:EvalReflectTarget:EvalMarker:class:1:handle:EvalMarker:method:method:1:id:EvalMarker:property:property"
    );
}

/// Verifies eval ReflectionAttribute exposes owner target and repetition metadata.
#[test]
fn test_eval_reflection_attribute_target_and_repetition() {
    let out = compile_and_run_capture(
        r#"<?php
eval('class EvalTargetMarker {
    public function __construct($name = null) {}
}
#[EvalTargetMarker("class-a"), EvalTargetMarker("class-b")]
class EvalReflectAttributeTarget {
    #[EvalTargetMarker("method")]
    public function run(#[EvalTargetMarker("param")] $id) {}
    #[EvalTargetMarker("property")]
    public $id;
    #[EvalTargetMarker("const")]
    public const ANSWER = 42;
}
enum EvalReflectAttributeEnum {
    #[EvalTargetMarker("case")]
    case Ready;
}
$classAttrs = (new ReflectionClass("EvalReflectAttributeTarget"))->getAttributes();
echo $classAttrs[0]->getTarget() . "/" . ($classAttrs[0]->isRepeated() ? "R" : "r") . ":";
echo $classAttrs[1]->getTarget() . "/" . ($classAttrs[1]->isRepeated() ? "R" : "r") . ":";
$methodAttr = (new ReflectionMethod("EvalReflectAttributeTarget", "run"))->getAttributes()[0];
echo $methodAttr->getTarget() . "/" . ($methodAttr->isRepeated() ? "R" : "r") . ":";
$propertyAttr = (new ReflectionProperty("EvalReflectAttributeTarget", "id"))->getAttributes()[0];
echo $propertyAttr->getTarget() . "/" . ($propertyAttr->isRepeated() ? "R" : "r") . ":";
$paramAttr = (new ReflectionMethod("EvalReflectAttributeTarget", "run"))->getParameters()[0]->getAttributes()[0];
echo $paramAttr->getTarget() . "/" . ($paramAttr->isRepeated() ? "R" : "r") . ":";
$constAttr = (new ReflectionClassConstant("EvalReflectAttributeTarget", "ANSWER"))->getAttributes()[0];
echo $constAttr->getTarget() . "/" . ($constAttr->isRepeated() ? "R" : "r") . ":";
$caseAttr = (new ReflectionEnumUnitCase("EvalReflectAttributeEnum", "Ready"))->getAttributes()[0];
echo $caseAttr->getTarget() . "/" . ($caseAttr->isRepeated() ? "R" : "r") . ":";
echo method_exists($classAttrs[0], "getTarget") ? "Y" : "n";
echo method_exists($classAttrs[0], "isRepeated") ? "Y" : "n";');
"#,
    );
    assert!(
        out.success,
        "program failed: stdout={:?} stderr={}",
        out.stdout, out.stderr
    );
    assert_eq!(out.stdout, "1/R:1/R:4/r:8/r:32/r:16/r:16/r:YY");
}

/// Verifies eval ReflectionClass exposes namespace-derived class-name parts.
#[test]
fn test_eval_reflection_class_name_parts() {
    let out = compile_and_run_capture(
        r#"<?php
eval('namespace Eval\Ns;
class Thing {}
$ref = new \ReflectionClass(Thing::class);
echo $ref->getName() . ":";
echo $ref->getShortName() . ":";
echo $ref->getNamespaceName() . ":";
echo $ref->inNamespace() ? "Y" : "N";');
"#,
    );
    assert!(
        out.success,
        "program failed: stdout={:?} stderr={}",
        out.stdout, out.stderr
    );
    assert_eq!(out.stdout, "Eval\\Ns\\Thing:Thing:Eval\\Ns:Y");
}

/// Verifies eval ReflectionClass exposes implemented interface and used trait names.
#[test]
fn test_eval_reflection_class_relation_names() {
    let out = compile_and_run_capture(
        r#"<?php
eval('interface EvalRelationIface {}
trait EvalRelationTrait {
    public function primary() {}
}
trait EvalRelationOtherTrait {
    public function other() {}
}
class EvalRelationTarget implements EvalRelationIface {
    use EvalRelationTrait, EvalRelationOtherTrait {
        EvalRelationTrait::primary as relationAlias;
        EvalRelationOtherTrait::other as private hiddenOther;
        EvalRelationOtherTrait::other as protected;
    }
}
class EvalRelationInherited extends EvalRelationTarget {}
interface EvalRelationParent {}
interface EvalRelationChild extends EvalRelationParent {}
$ref = new ReflectionClass("EvalRelationTarget");
$interfaces = $ref->getInterfaceNames();
$traits = $ref->getTraitNames();
echo count($interfaces) . ":" . $interfaces[0] . ":";
echo count($traits) . ":" . $traits[0] . ":" . $traits[1] . ":";
$parentInterfaces = (new ReflectionClass("EvalRelationChild"))->getInterfaceNames();
echo count($parentInterfaces) . ":" . $parentInterfaces[0] . ":";
$interfaceObjects = $ref->getInterfaces();
echo count($interfaceObjects) . ":" . $interfaceObjects["EvalRelationIface"]->getName() . ":";
$traitObjects = $ref->getTraits();
echo count($traitObjects) . ":" . $traitObjects["EvalRelationTrait"]->getName() . ":" . $traitObjects["EvalRelationOtherTrait"]->getName() . ":";
$parentInterfaceObjects = (new ReflectionClass("EvalRelationChild"))->getInterfaces();
echo count($parentInterfaceObjects) . ":" . $parentInterfaceObjects["EvalRelationParent"]->getName() . ":";
$aliases = $ref->getTraitAliases();
echo count($aliases) . ":" . $aliases["relationAlias"] . ":" . $aliases["hiddenOther"] . ":";
$inheritedAliases = (new ReflectionClass("EvalRelationInherited"))->getTraitAliases();
echo count($inheritedAliases);');
"#,
    );
    assert!(
        out.success,
        "program failed: stdout={:?} stderr={}",
        out.stdout, out.stderr
    );
    assert_eq!(
        out.stdout,
        "1:EvalRelationIface:2:EvalRelationTrait:EvalRelationOtherTrait:1:EvalRelationParent:1:EvalRelationIface:2:EvalRelationTrait:EvalRelationOtherTrait:1:EvalRelationParent:2:EvalRelationTrait::primary:EvalRelationOtherTrait::other:0"
    );
}

/// Verifies eval ReflectionClass exposes generated/AOT implemented interface names.
#[test]
fn test_eval_reflection_class_get_interface_names_for_aot_class() {
    let out = compile_and_run_capture(
        r#"<?php
interface EvalAotReflectIfaceBase {}
interface EvalAotReflectIfaceChild extends EvalAotReflectIfaceBase {}
class EvalAotReflectIfaceTarget implements EvalAotReflectIfaceChild {}
eval('$interfaces = (new ReflectionClass("EvalAotReflectIfaceTarget"))->getInterfaceNames();
sort($interfaces);
echo count($interfaces) . ":";
echo implode(",", $interfaces) . ":";
$interfaceObjects = (new ReflectionClass("EvalAotReflectIfaceTarget"))->getInterfaces();
ksort($interfaceObjects);
echo count($interfaceObjects) . ":" . implode(",", array_keys($interfaceObjects)) . ":";
echo $interfaceObjects["EvalAotReflectIfaceBase"]->getName();');
"#,
    );
    assert!(
        out.success,
        "program failed: stdout={:?} stderr={}",
        out.stdout, out.stderr
    );
    assert_eq!(
        out.stdout,
        "2:EvalAotReflectIfaceBase,EvalAotReflectIfaceChild:2:EvalAotReflectIfaceBase,EvalAotReflectIfaceChild:EvalAotReflectIfaceBase"
    );
}

/// Verifies eval ReflectionClass::implementsInterface reports class, enum, and
/// interface metadata through the bridge.
#[test]
fn test_eval_reflection_class_implements_interface_predicate() {
    let out = compile_and_run_capture(
        r#"<?php
eval('interface EvalImplBase {}
interface EvalImplChild extends EvalImplBase {}
class EvalImplTarget implements EvalImplChild {}
enum EvalImplEnum implements EvalImplBase { case Ready; }
trait EvalImplTrait {}
echo (new ReflectionClass("EvalImplTarget"))->implementsInterface("EvalImplChild") ? "C" : "c";
echo (new ReflectionClass("EvalImplTarget"))->implementsInterface("evalimplbase") ? "B" : "b";
echo (new ReflectionClass("EvalImplEnum"))->implementsInterface("EvalImplBase") ? "E" : "e";
echo (new ReflectionClass("EvalImplChild"))->implementsInterface("EvalImplChild") ? "I" : "i";
echo (new ReflectionClass("EvalImplChild"))->implementsInterface("EvalImplBase") ? "P" : "p";
echo (new ReflectionClass("EvalImplTrait"))->implementsInterface("EvalImplBase") ? "T" : "t";');
"#,
    );
    assert!(
        out.success,
        "program failed: stdout={:?} stderr={}",
        out.stdout, out.stderr
    );
    assert_eq!(out.stdout, "CBEIPt");
}

/// Verifies eval ReflectionClass::implementsInterface uses generated/AOT relations.
#[test]
fn test_eval_reflection_class_implements_interface_for_aot_class() {
    let out = compile_and_run_capture(
        r#"<?php
interface EvalAotReflectImplBase {}
interface EvalAotReflectImplChild extends EvalAotReflectImplBase {}
class EvalAotReflectImplTarget implements EvalAotReflectImplChild {}
eval('$ref = new ReflectionClass("EvalAotReflectImplTarget");
echo $ref->implementsInterface("EvalAotReflectImplChild") ? "C" : "c";
echo $ref->implementsInterface("evalaotreflectimplbase") ? "B" : "b";
echo $ref->implementsInterface("Iterator") ? "I" : "i";');
"#,
    );
    assert!(
        out.success,
        "program failed: stdout={:?} stderr={}",
        out.stdout, out.stderr
    );
    assert_eq!(out.stdout, "CBi");
}

/// Verifies eval `ReflectionClass::implementsInterface()` throws ReflectionException
/// for missing or non-interface argument names.
#[test]
fn test_eval_reflection_class_implements_interface_rejects_non_interfaces() {
    let out = compile_and_run_capture(
        r#"<?php
eval('interface EvalImplRejectIface {}
interface EvalImplRejectOther {}
class EvalImplRejectTarget implements EvalImplRejectIface {}
class EvalImplRejectClass {}
trait EvalImplRejectTrait {}
enum EvalImplRejectEnum { case Ready; }
$ref = new ReflectionClass("EvalImplRejectTarget");
echo $ref->implementsInterface("EvalImplRejectOther") ? "T" : "F";
try {
    $ref->implementsInterface("EvalImplRejectClass");
    echo ":ok";
} catch (ReflectionException $e) {
    echo ":" . get_class($e) . ":" . $e->getMessage();
}
try {
    $ref->implementsInterface("EvalImplRejectTrait");
    echo ":ok";
} catch (ReflectionException $e) {
    echo ":" . get_class($e) . ":" . $e->getMessage();
}
try {
    $ref->implementsInterface("EvalImplRejectEnum");
    echo ":ok";
} catch (ReflectionException $e) {
    echo ":" . get_class($e) . ":" . $e->getMessage();
}
try {
    $ref->implementsInterface("EvalImplRejectMissing");
    echo ":ok";
} catch (ReflectionException $e) {
    echo ":" . get_class($e) . ":" . $e->getMessage();
}');
"#,
    );
    assert!(
        out.success,
        "program failed: stdout={:?} stderr={}",
        out.stdout, out.stderr
    );
    assert_eq!(
        out.stdout,
        "F:ReflectionException:EvalImplRejectClass is not an interface:ReflectionException:EvalImplRejectTrait is not an interface:ReflectionException:EvalImplRejectEnum is not an interface:ReflectionException:Interface \"EvalImplRejectMissing\" does not exist"
    );
}

/// Verifies eval ReflectionClass::isSubclassOf reports parent and interface
/// metadata through the linked eval bridge.
#[test]
fn test_eval_reflection_class_is_subclass_of_predicate() {
    let out = compile_and_run_capture(
        r#"<?php
eval('interface EvalSubclassIface {}
interface EvalSubclassChildIface extends EvalSubclassIface {}
class EvalSubclassBase {}
class EvalSubclassParent extends EvalSubclassBase {}
class EvalSubclassChild extends EvalSubclassParent implements EvalSubclassChildIface {}
trait EvalSubclassTrait {}
enum EvalSubclassEnum implements EvalSubclassIface { case Ready; }
$ref = new ReflectionClass("EvalSubclassChild");
echo $ref->isSubclassOf("EvalSubclassParent") ? "P" : "p";
echo $ref->isSubclassOf("evalsubclassbase") ? "B" : "b";
echo $ref->isSubclassOf("EvalSubclassIface") ? "I" : "i";
echo $ref->isSubclassOf("EvalSubclassChild") ? "S" : "s";
echo (new ReflectionClass("EvalSubclassChildIface"))->isSubclassOf("EvalSubclassIface") ? "J" : "j";
echo (new ReflectionClass("EvalSubclassIface"))->isSubclassOf("EvalSubclassIface") ? "X" : "x";
echo $ref->isSubclassOf("EvalSubclassTrait") ? "T" : "t";
echo $ref->isSubclassOf("EvalSubclassEnum") ? "Q" : "q";
echo (new ReflectionClass("EvalSubclassEnum"))->isSubclassOf("EvalSubclassIface") ? "E" : "e";
try {
    $ref->isSubclassOf("EvalSubclassMissing");
    echo ":bad";
} catch (ReflectionException $e) {
    echo ":missing";
}');
"#,
    );
    assert!(
        out.success,
        "program failed: stdout={:?} stderr={}",
        out.stdout, out.stderr
    );
    assert_eq!(out.stdout, "PBIsJxtqE:missing");
}

/// Verifies eval ReflectionClass::isSubclassOf can query generated AOT class
/// relations when the reflected class was declared outside the eval fragment.
#[test]
fn test_eval_reflection_class_is_subclass_of_aot_class() {
    let out = compile_and_run_capture(
        r#"<?php
class EvalAotSubclassParent {}
class EvalAotSubclassChild extends EvalAotSubclassParent {}
interface EvalAotSubclassIface {}
class EvalAotSubclassImpl implements EvalAotSubclassIface {}
eval('$child = new ReflectionClass("EvalAotSubclassChild");
echo $child->isSubclassOf("EvalAotSubclassParent") ? "P" : "p";
echo $child->isSubclassOf("EvalAotSubclassChild") ? "S" : "s";
$impl = new ReflectionClass("EvalAotSubclassImpl");
echo $impl->isSubclassOf("EvalAotSubclassIface") ? "I" : "i";');
"#,
    );
    assert!(
        out.success,
        "program failed: stdout={:?} stderr={}",
        out.stdout, out.stderr
    );
    assert_eq!(out.stdout, "PsI");
}

/// Verifies eval ReflectionClass::isInstance reports eval-declared object
/// relations through the linked eval bridge.
#[test]
fn test_eval_reflection_class_is_instance_predicate() {
    let out = compile_and_run_capture(
        r#"<?php
eval('interface EvalInstanceIface {}
class EvalInstanceBase {}
class EvalInstanceChild extends EvalInstanceBase implements EvalInstanceIface {}
trait EvalInstanceTrait {}
enum EvalInstanceEnum implements EvalInstanceIface { case Ready; }
$base = new ReflectionClass("EvalInstanceBase");
$child = new ReflectionClass("EvalInstanceChild");
$iface = new ReflectionClass("EvalInstanceIface");
$trait = new ReflectionClass("EvalInstanceTrait");
$enum = new ReflectionClass("EvalInstanceEnum");
$childObj = new EvalInstanceChild();
$objectRef = new ReflectionClass($childObj);
echo $objectRef->getName(); echo ":";
echo $objectRef->getParentClass()->getName(); echo ":";
echo $objectRef->isInstance($childObj) ? "O" : "o"; echo ":";
echo $base->isInstance($childObj) ? "B" : "b";
echo $child->isInstance(new EvalInstanceBase()) ? "C" : "c";
echo $iface->isInstance($childObj) ? "I" : "i";
echo $trait->isInstance($childObj) ? "T" : "t";
echo $enum->isInstance(EvalInstanceEnum::Ready) ? "E" : "e";
echo $iface->isInstance(EvalInstanceEnum::Ready) ? "N" : "n";');
"#,
    );
    assert!(
        out.success,
        "program failed: stdout={:?} stderr={}",
        out.stdout, out.stderr
    );
    assert_eq!(out.stdout, "EvalInstanceChild:EvalInstanceBase:O:BcItEN");
}

/// Verifies eval ReflectionClass::isInstance can query generated AOT object
/// relations when the reflected class was declared outside the eval fragment.
#[test]
fn test_eval_reflection_class_is_instance_aot_class() {
    let out = compile_and_run_capture(
        r#"<?php
class EvalAotInstanceParent {}
class EvalAotInstanceChild extends EvalAotInstanceParent {}
interface EvalAotInstanceIface {}
class EvalAotInstanceImpl implements EvalAotInstanceIface {}
eval('$parent = new ReflectionClass("EvalAotInstanceParent");
echo $parent->isInstance(new EvalAotInstanceChild()) ? "P" : "p";
$child = new ReflectionClass("EvalAotInstanceChild");
echo $child->isInstance(new EvalAotInstanceParent()) ? "C" : "c";
$iface = new ReflectionClass("EvalAotInstanceIface");
$objectRef = new ReflectionClass(new EvalAotInstanceChild());
echo $iface->isInstance(new EvalAotInstanceImpl()) ? "I" : "i"; echo ":";
echo $objectRef->getName(); echo ":";
echo $objectRef->getParentClass()->getName();');
"#,
    );
    assert!(
        out.success,
        "program failed: stdout={:?} stderr={}",
        out.stdout, out.stderr
    );
    assert_eq!(out.stdout, "PcI:EvalAotInstanceChild:EvalAotInstanceParent");
}

/// Verifies eval ReflectionClass::getParentClass crosses the generated runtime bridge.
#[test]
fn test_eval_reflection_class_get_parent_class() {
    let out = compile_and_run_capture(
        r#"<?php
eval('class EvalBridgeParent {}
class EvalBridgeChild extends EvalBridgeParent {}
$parent = (new ReflectionClass("EvalBridgeChild"))->getParentClass();
echo $parent->getName() . ":";
$root = (new ReflectionClass("EvalBridgeParent"))->getParentClass();
if ($root === false) {
    echo "false";
} else {
    echo "bad";
}');
"#,
    );
    assert!(
        out.success,
        "program failed: stdout={:?} stderr={}",
        out.stdout, out.stderr
    );
    assert_eq!(out.stdout, "EvalBridgeParent:false");
}

/// Verifies eval ReflectionClass::getParentClass materializes generated/AOT parents.
#[test]
fn test_eval_reflection_class_get_parent_class_for_aot_class() {
    let out = compile_and_run_capture(
        r#"<?php
class EvalAotReflectParentBase {}
class EvalAotReflectParentChild extends EvalAotReflectParentBase {}
eval('$parent = (new ReflectionClass("EvalAotReflectParentChild"))->getParentClass();
if ($parent === false) {
    echo "missing";
} else {
    echo $parent->getName();
}
echo ":";
$root = (new ReflectionClass("EvalAotReflectParentBase"))->getParentClass();
echo $root === false ? "false" : "bad";');
"#,
    );
    assert!(
        out.success,
        "program failed: stdout={:?} stderr={}",
        out.stdout, out.stderr
    );
    assert_eq!(out.stdout, "EvalAotReflectParentBase:false");
}

/// Verifies eval ReflectionClass::getConstructor crosses the generated runtime bridge.
#[test]
fn test_eval_reflection_class_get_constructor() {
    let out = compile_and_run_capture(
        r#"<?php
eval('class EvalBridgeCtorBase {
    public function __construct($required, $optional = 2) {}
}
class EvalBridgeCtorChild extends EvalBridgeCtorBase {}
class EvalBridgeCtorPlain {}
interface EvalBridgeCtorInterface {
    public function __construct($required);
}
trait EvalBridgeCtorTrait {
    public function __construct($required, $optional = null, ...$rest) {}
}
$base = (new ReflectionClass("EvalBridgeCtorBase"))->getConstructor();
echo $base->getName() . "/" . $base->getNumberOfParameters();
echo "/" . $base->getNumberOfRequiredParameters() . ":";
$child = (new ReflectionClass("EvalBridgeCtorChild"))->getConstructor();
echo $child->getName() . "/" . $child->getNumberOfParameters();
echo "/" . $child->getNumberOfRequiredParameters() . ":";
$plain = (new ReflectionClass("EvalBridgeCtorPlain"))->getConstructor();
echo $plain === null ? "null" : "bad";
echo ":";
$interface = (new ReflectionClass("EvalBridgeCtorInterface"))->getConstructor();
echo $interface->getName() . "/" . $interface->getNumberOfParameters();
echo "/" . $interface->getNumberOfRequiredParameters() . ":";
$trait = (new ReflectionClass("EvalBridgeCtorTrait"))->getConstructor();
echo $trait->getName() . "/" . $trait->getNumberOfParameters();
echo "/" . $trait->getNumberOfRequiredParameters();
');
"#,
    );
    assert!(
        out.success,
        "program failed: stdout={:?} stderr={}",
        out.stdout, out.stderr
    );
    assert_eq!(
        out.stdout,
        "__construct/2/1:__construct/2/1:null:__construct/1/1:__construct/3/1"
    );
}

/// Verifies eval ReflectionClass reports class-like final and abstract flags.
#[test]
fn test_eval_reflection_class_modifier_flags() {
    let out = compile_and_run_capture(
        r#"<?php
eval('abstract class EvalAbstractReflect {}
final class EvalFinalReflect {}
interface EvalIfaceReflect {}
trait EvalTraitReflect {}
enum EvalEnumReflect { case Ready; }
echo (new ReflectionClass("EvalAbstractReflect"))->isAbstract() ? "A" : "a";
echo (new ReflectionClass("EvalAbstractReflect"))->isFinal() ? "F" : "f";
echo (new ReflectionClass("EvalAbstractReflect"))->isInterface() ? "I" : "i";
echo (new ReflectionClass("EvalAbstractReflect"))->isTrait() ? "T" : "t";
echo (new ReflectionClass("EvalAbstractReflect"))->isEnum() ? "E" : "e"; echo ":";
echo (new ReflectionClass("EvalFinalReflect"))->isAbstract() ? "A" : "a";
echo (new ReflectionClass("EvalFinalReflect"))->isFinal() ? "F" : "f";
echo (new ReflectionClass("EvalFinalReflect"))->isInterface() ? "I" : "i";
echo (new ReflectionClass("EvalFinalReflect"))->isTrait() ? "T" : "t";
echo (new ReflectionClass("EvalFinalReflect"))->isEnum() ? "E" : "e"; echo ":";
echo (new ReflectionClass("EvalEnumReflect"))->isAbstract() ? "A" : "a";
echo (new ReflectionClass("EvalEnumReflect"))->isFinal() ? "F" : "f";
echo (new ReflectionClass("EvalEnumReflect"))->isInterface() ? "I" : "i";
echo (new ReflectionClass("EvalEnumReflect"))->isTrait() ? "T" : "t";
echo (new ReflectionClass("EvalEnumReflect"))->isEnum() ? "E" : "e"; echo ":";
echo (new ReflectionClass("EvalIfaceReflect"))->isAbstract() ? "A" : "a";
echo (new ReflectionClass("EvalIfaceReflect"))->isFinal() ? "F" : "f";
echo (new ReflectionClass("EvalIfaceReflect"))->isInterface() ? "I" : "i";
echo (new ReflectionClass("EvalIfaceReflect"))->isTrait() ? "T" : "t";
echo (new ReflectionClass("EvalIfaceReflect"))->isEnum() ? "E" : "e"; echo ":";
echo (new ReflectionClass("EvalTraitReflect"))->isAbstract() ? "A" : "a";
echo (new ReflectionClass("EvalTraitReflect"))->isFinal() ? "F" : "f";
echo (new ReflectionClass("EvalTraitReflect"))->isInterface() ? "I" : "i";
echo (new ReflectionClass("EvalTraitReflect"))->isTrait() ? "T" : "t";
echo (new ReflectionClass("EvalTraitReflect"))->isEnum() ? "E" : "e";');
"#,
    );
    assert!(
        out.success,
        "program failed: stdout={:?} stderr={}",
        out.stdout, out.stderr
    );
    assert_eq!(out.stdout, "Afite:aFite:aFitE:afIte:afiTe");
}

/// Verifies eval ReflectionClass reports PHP modifier bitmasks through the bridge.
#[test]
fn test_eval_reflection_class_modifier_bitmask() {
    let out = compile_and_run_capture(
        r#"<?php
eval('abstract class EvalModifierAbstract {}
final class EvalModifierFinal {}
readonly class EvalModifierReadonly {}
final readonly class EvalModifierFinalReadonly {}
enum EvalModifierEnum { case Ready; }
interface EvalModifierIface {}
trait EvalModifierTrait {}
echo (new ReflectionClass("EvalModifierAbstract"))->getModifiers() . ":";
echo (new ReflectionClass("EvalModifierFinal"))->getModifiers() . ":";
echo (new ReflectionClass("EvalModifierReadonly"))->getModifiers() . ":";
echo (new ReflectionClass("EvalModifierFinalReadonly"))->getModifiers() . ":";
echo (new ReflectionClass("EvalModifierEnum"))->getModifiers() . ":";
echo (new ReflectionClass("EvalModifierIface"))->getModifiers() . ":";
echo (new ReflectionClass("EvalModifierTrait"))->getModifiers();');
"#,
    );
    assert!(
        out.success,
        "program failed: stdout={:?} stderr={}",
        out.stdout, out.stderr
    );
    assert_eq!(out.stdout, "64:32:65536:65568:32:0:0");
}

/// Verifies eval ReflectionClass reports readonly class status through the bridge.
#[test]
fn test_eval_reflection_class_readonly_predicate() {
    let out = compile_and_run_capture(
        r#"<?php
eval('class EvalReadonlyPlain {}
readonly class EvalReadonlyReflect {}
final readonly class EvalReadonlyFinalReflect {}
enum EvalReadonlyEnumReflect { case Ready; }
interface EvalReadonlyIface {}
trait EvalReadonlyTrait {}
echo (new ReflectionClass("EvalReadonlyPlain"))->isReadOnly() ? "R" : "r";
echo (new ReflectionClass("EvalReadonlyReflect"))->isReadOnly() ? "R" : "r";
echo (new ReflectionClass("EvalReadonlyFinalReflect"))->isReadOnly() ? "R" : "r";
echo (new ReflectionClass("EvalReadonlyEnumReflect"))->isReadOnly() ? "R" : "r";
echo (new ReflectionClass("EvalReadonlyIface"))->isReadOnly() ? "R" : "r";
echo (new ReflectionClass("EvalReadonlyTrait"))->isReadOnly() ? "R" : "r";');
"#,
    );
    assert!(
        out.success,
        "program failed: stdout={:?} stderr={}",
        out.stdout, out.stderr
    );
    assert_eq!(out.stdout, "rRRrrr");
}

/// Verifies eval ReflectionClass reports instantiability through the bridge.
#[test]
fn test_eval_reflection_class_instantiable_predicate() {
    let out = compile_and_run_capture(
        r#"<?php
eval('abstract class EvalInstAbstract {}
class EvalInstPublic {}
final class EvalInstFinal {}
class EvalInstPrivate { private function __construct() {} }
class EvalInstProtected { protected function __construct() {} }
interface EvalInstIface {}
trait EvalInstTrait {}
enum EvalInstEnum { case Ready; }
echo (new ReflectionClass("EvalInstAbstract"))->isInstantiable() ? "A" : "a";
echo (new ReflectionClass("EvalInstPublic"))->isInstantiable() ? "B" : "b";
echo (new ReflectionClass("EvalInstFinal"))->isInstantiable() ? "C" : "c";
echo (new ReflectionClass("EvalInstPrivate"))->isInstantiable() ? "P" : "p";
echo (new ReflectionClass("EvalInstProtected"))->isInstantiable() ? "R" : "r";
echo (new ReflectionClass("EvalInstIface"))->isInstantiable() ? "I" : "i";
echo (new ReflectionClass("EvalInstTrait"))->isInstantiable() ? "T" : "t";
echo (new ReflectionClass("EvalInstEnum"))->isInstantiable() ? "E" : "e";');
"#,
    );
    assert!(
        out.success,
        "program failed: stdout={:?} stderr={}",
        out.stdout, out.stderr
    );
    assert_eq!(out.stdout, "aBCprite");
}

/// Verifies eval ReflectionClass reports named eval class-like symbols as non-anonymous through
/// the generated reflection-owner bridge.
#[test]
fn test_eval_reflection_class_anonymous_predicate() {
    let out = compile_and_run_capture(
        r#"<?php
eval('class EvalAnonReflect {}
interface EvalAnonIface {}
trait EvalAnonTrait {}
enum EvalAnonEnum { case Ready; }
echo (new ReflectionClass("EvalAnonReflect"))->isAnonymous() ? "C" : "c";
echo (new ReflectionClass("EvalAnonIface"))->isAnonymous() ? "I" : "i";
echo (new ReflectionClass("EvalAnonTrait"))->isAnonymous() ? "T" : "t";
echo (new ReflectionClass("EvalAnonEnum"))->isAnonymous() ? "E" : "e";');
"#,
    );
    assert!(
        out.success,
        "program failed: stdout={:?} stderr={}",
        out.stdout, out.stderr
    );
    assert_eq!(out.stdout, "cite");
}

/// Verifies eval anonymous class expressions instantiate and reflect as anonymous through the bridge.
#[test]
fn test_eval_anonymous_class_expression_runtime_and_reflection() {
    let out = compile_and_run_capture(
        r#"<?php
eval('interface EvalRuntimeAnonLabel {
    function label();
}
class EvalRuntimeAnonBase {
    protected string $prefix;
    public function __construct($prefix) { $this->prefix = $prefix; }
}
function eval_runtime_anon_make($prefix) {
    return new class($prefix) extends EvalRuntimeAnonBase implements EvalRuntimeAnonLabel {
        public function label() { return $this->prefix . ":anon"; }
    };
}
$first = eval_runtime_anon_make("A");
$second = eval_runtime_anon_make("B");
echo $first->label(); echo ":";
echo $second->label(); echo ":";
echo get_class($first) === get_class($second) ? "same" : "different"; echo ":";
$ref = new ReflectionClass(get_class($first));
echo $ref->isAnonymous() ? "anonymous" : "named"; echo ":";
echo $ref->implementsInterface("EvalRuntimeAnonLabel") ? "iface" : "bad";');
"#,
    );
    assert!(
        out.success,
        "program failed: stdout={:?} stderr={}",
        out.stdout, out.stderr
    );
    assert_eq!(out.stdout, "A:anon:B:anon:same:anonymous:iface");
}

/// Verifies eval ReflectionClass reports method, property, and constant membership through the bridge.
#[test]
fn test_eval_reflection_class_member_existence() {
    let out = compile_and_run_capture(
        r#"<?php
eval('class EvalMemberParent {
    const PARENT_CONST = 1;
    private function hiddenParent() {}
    protected static function parentStatic() {}
    private $hiddenProp;
    protected static $parentStaticProp;
}
interface EvalMemberClassIface {
    const CLASS_LIMIT = 10;
}
class EvalMemberChild extends EvalMemberParent implements EvalMemberClassIface {
    const CHILD_CONST = 2;
    public function ChildMethod() {}
    public $childProp;
}
interface EvalMemberIfaceParent {
    const PARENT_LIMIT = 10;
    public function parentRequirement();
}
interface EvalMemberIface extends EvalMemberIfaceParent {
    const CHILD_LIMIT = 20;
    public function childRequirement();
    public string $hook { get; }
}
trait EvalMemberTrait {
    const TRAIT_CONST = 30;
    private function traitHidden() {}
    public $traitProp;
}
enum EvalMemberPureEnum {
    case Ready;
    const LEVEL = 40;
    public function label() { return "ok"; }
}
enum EvalMemberBackedEnum: string {
    case Ready = "ready";
}
$child = new ReflectionClass("EvalMemberChild");
echo $child->hasMethod("childmethod") ? "M" : "m";
echo $child->hasMethod("HIDDENPARENT") ? "P" : "p";
echo $child->hasMethod("parentStatic") ? "S" : "s";
echo $child->hasMethod("missing") ? "X" : "x";
echo ":";
echo $child->hasProperty("childProp") ? "C" : "c";
echo $child->hasProperty("hiddenProp") ? "H" : "h";
echo $child->hasProperty("parentStaticProp") ? "T" : "t";
echo $child->hasProperty("childprop") ? "W" : "w";
echo $child->hasConstant("CHILD_CONST") ? "D" : "d";
echo $child->hasConstant("PARENT_CONST") ? "P" : "p";
echo $child->hasConstant("CLASS_LIMIT") ? "A" : "a";
echo $child->hasConstant("child_const") ? "Z" : "z";
echo ":";
$iface = new ReflectionClass("EvalMemberIface");
echo $iface->hasMethod("parentrequirement") ? "I" : "i";
echo $iface->hasMethod("childRequirement") ? "J" : "j";
echo $iface->hasProperty("hook") ? "K" : "k";
echo $iface->hasConstant("PARENT_LIMIT") ? "L" : "l";
echo $iface->hasConstant("CHILD_LIMIT") ? "C" : "c";
echo ":";
$trait = new ReflectionClass("EvalMemberTrait");
echo $trait->hasMethod("traithidden") ? "R" : "r";
echo $trait->hasProperty("traitProp") ? "U" : "u";
echo $trait->hasConstant("TRAIT_CONST") ? "K" : "k";
echo ":";
$pure = new ReflectionClass("EvalMemberPureEnum");
echo $pure->hasMethod("cases") ? "E" : "e";
echo $pure->hasMethod("label") ? "L" : "l";
echo $pure->hasProperty("name") ? "N" : "n";
echo $pure->hasProperty("value") ? "V" : "v";
echo $pure->hasConstant("Ready") ? "G" : "g";
echo $pure->hasConstant("LEVEL") ? "F" : "f";
echo $pure->hasConstant("ready") ? "R" : "r";
echo ":";
$backed = new ReflectionClass("EvalMemberBackedEnum");
echo $backed->hasMethod("tryfrom") ? "B" : "b";
echo $backed->hasProperty("value") ? "Y" : "y";
echo $backed->hasConstant("Ready") ? "Q" : "q";');
"#,
    );
    assert!(
        out.success,
        "program failed: stdout={:?} stderr={}",
        out.stdout, out.stderr
    );
    assert_eq!(out.stdout, "MPSx:ChTwDPAz:IJKLC:RUK:ELNvGFr:BYQ");
}

/// Verifies eval ReflectionClass returns constant values and enum cases through the bridge.
#[test]
fn test_eval_reflection_class_constant_values() {
    let out = compile_and_run_capture(
        r#"<?php
eval('class EvalReflectConstBase {
    public const BASE = 1;
}
interface EvalReflectConstIface {
    public const LIMIT = 2;
}
trait EvalReflectConstTrait {
    public const TRAIT_VALUE = 3;
}
class EvalReflectConstChild extends EvalReflectConstBase implements EvalReflectConstIface {
    private const SECRET = 9;
    public const OWN = "own";
    public const SUM = 5;
}
enum EvalReflectConstEnum {
    case Ready;
    public const LEVEL = 40;
}
$ref = new ReflectionClass("EvalReflectConstChild");
$all = $ref->getConstants();
$public = $ref->getConstants(ReflectionClassConstant::IS_PUBLIC);
$private = $ref->getConstants(filter: ReflectionClassConstant::IS_PRIVATE);
$none = $ref->getConstants(0);
$null = $ref->getConstants(null);
echo $ref->getConstant("OWN") . ":";
echo $ref->getConstant("BASE") . ":";
echo $ref->getConstant("LIMIT") . ":";
echo $ref->getConstant("SECRET") . ":";
echo $ref->getConstant("SUM") . ":";
echo $ref->getConstant("own") ? "bad" : "missing";
echo ":" . count($all) . ":" . $all["OWN"] . ":" . $all["BASE"] . ":" . $all["LIMIT"];
echo ":" . count($public) . ":" . $public["OWN"] . ":" . $public["BASE"];
echo ":" . count($private) . ":" . $private["SECRET"];
echo ":" . count($none) . ":" . count($null);
$trait = new ReflectionClass("EvalReflectConstTrait");
$traitAll = $trait->getConstants();
echo ":" . $trait->getConstant("TRAIT_VALUE") . ":" . count($traitAll) . ":" . $traitAll["TRAIT_VALUE"];
$enum = new ReflectionClass("EvalReflectConstEnum");
$case = $enum->getConstant("Ready");
$enumAll = $enum->getConstants();
echo ":" . $case->name;
echo ":" . $enum->getConstant("LEVEL") . ":" . $enumAll["LEVEL"] . ":" . count($enumAll);');
"#,
    );
    assert!(
        out.success,
        "program failed: stdout={:?} stderr={}",
        out.stdout, out.stderr
    );
    assert_eq!(
        out.stdout,
        "own:1:2:9:5:missing:5:own:1:2:4:own:1:1:9:0:5:3:1:3:Ready:40:40:2"
    );
}

/// Verifies eval ReflectionClass returns class-constant reflector objects through the bridge.
#[test]
fn test_eval_reflection_class_constant_reflector_objects() {
    let out = compile_and_run_capture(
        r#"<?php
eval('class EvalReflectConstMarker {
    public $label;
    public function __construct($label) {
        $this->label = $label;
    }
    public function label() {
        return $this->label;
    }
}
class EvalReflectConstObjectTarget {
    #[EvalReflectConstMarker("const")]
    final public const ANSWER = 42;
}
enum EvalReflectConstObjectEnum {
    #[EvalReflectConstMarker("case")]
    case Ready;
    final public const LEVEL = 7;
}
$ref = new ReflectionClass("EvalReflectConstObjectTarget");
$single = $ref->getReflectionConstant("ANSWER");
$all = $ref->getReflectionConstants();
$public = $ref->getReflectionConstants(ReflectionClassConstant::IS_PUBLIC);
$final = $ref->getReflectionConstants(filter: ReflectionClassConstant::IS_FINAL);
echo $single->getName() . ":";
echo ($single->isFinal() ? "F" : "f") . ":";
echo count($all) . ":" . $all[0]->getName() . ":";
echo $single->getAttributes()[0]->newInstance()->label() . ":";
echo $ref->getReflectionConstant("answer") ? "bad" : "missing";
echo ":" . count($public) . ":" . $public[0]->getName();
echo ":" . count($final) . ":" . $final[0]->getName();
$enum = new ReflectionClass("EvalReflectConstObjectEnum");
$enumAll = $enum->getReflectionConstants();
$enumFinal = $enum->getReflectionConstants(ReflectionClassConstant::IS_FINAL);
$case = $enum->getReflectionConstant("Ready");
$level = $enum->getReflectionConstant("LEVEL");
echo ":" . count($enumAll) . ":" . $enumAll[0]->getName() . ":" . $enumAll[1]->getName();
echo ":" . $case->getAttributes()[0]->newInstance()->label() . ":";
echo count($level->getAttributes()) . ":";
echo $level->isFinal() ? "F" : "f";
echo ":" . count($enumFinal) . ":" . $enumFinal[0]->getName();');
"#,
    );
    assert!(
        out.success,
        "program failed: stdout={:?} stderr={}",
        out.stdout, out.stderr
    );
    assert_eq!(
        out.stdout,
        "ANSWER:F:1:ANSWER:const:missing:1:ANSWER:1:ANSWER:2:Ready:LEVEL:case:0:F:1:LEVEL"
    );
}

/// Verifies eval ReflectionMethod and ReflectionProperty expose member predicates through the bridge.
#[test]
fn test_eval_reflection_member_predicates() {
    let out = compile_and_run_capture(
        r#"<?php
eval('abstract class EvalReflectMemberBase {
    protected static function baseStatic() {}
    abstract protected function mustImplement();
    final public function locked() {}
}
readonly class EvalReflectReadonlyClass {
    public int $classReadonly;
}
abstract class EvalReflectAbstractProperty {
    abstract public int $mustRead { get; }
}
class EvalReflectMemberChild extends EvalReflectMemberBase {
    public function mustImplement() {}
    private static $token;
    final public static $staticSeal;
    protected $visible;
    public readonly int $locked;
    final public int $sealed;
}
$baseStatic = new ReflectionMethod("EvalReflectMemberChild", "baseStatic");
echo $baseStatic->isStatic() ? "S" : "s";
echo $baseStatic->isProtected() ? "P" : "p";
echo $baseStatic->isPublic() ? "U" : "u";
echo $baseStatic->isPrivate() ? "R" : "r";
echo $baseStatic->isFinal() ? "F" : "f";
echo $baseStatic->isAbstract() ? "A" : "a";
echo ":";
$abstractMethod = new ReflectionMethod("EvalReflectMemberBase", "mustImplement");
echo $abstractMethod->isAbstract() ? "A" : "a";
echo $abstractMethod->isProtected() ? "P" : "p";
echo $abstractMethod->isStatic() ? "S" : "s";
echo ":";
$finalMethod = new ReflectionMethod("EvalReflectMemberChild", "locked");
echo $finalMethod->isFinal() ? "F" : "f";
echo $finalMethod->isPublic() ? "U" : "u";
echo $finalMethod->isStatic() ? "S" : "s";
echo ":";
$staticProp = new ReflectionProperty("EvalReflectMemberChild", "token");
echo $staticProp->isStatic() ? "S" : "s";
echo $staticProp->isPrivate() ? "R" : "r";
echo $staticProp->isProtected() ? "P" : "p";
echo $staticProp->isFinal() ? "F" : "f";
echo $staticProp->isAbstract() ? "A" : "a";
echo $staticProp->isReadOnly() ? "R" : "r";
echo $staticProp->isProtectedSet() ? "T" : "t";
echo $staticProp->isPrivateSet() ? "D" : "d";
echo $staticProp->getModifiers();
echo ":";
$visibleProp = new ReflectionProperty("EvalReflectMemberChild", "visible");
echo $visibleProp->isStatic() ? "S" : "s";
echo $visibleProp->isProtected() ? "P" : "p";
echo $visibleProp->isPublic() ? "U" : "u";
echo $visibleProp->isFinal() ? "F" : "f";
echo $visibleProp->isAbstract() ? "A" : "a";
echo $visibleProp->isReadOnly() ? "R" : "r";
echo $visibleProp->isProtectedSet() ? "T" : "t";
echo $visibleProp->isPrivateSet() ? "D" : "d";
echo $visibleProp->getModifiers();
echo ":";
$readonlyProp = new ReflectionProperty("EvalReflectMemberChild", "locked");
echo $readonlyProp->isReadOnly() ? "R" : "r";
echo $readonlyProp->isPublic() ? "U" : "u";
echo $readonlyProp->isProtectedSet() ? "T" : "t";
echo $readonlyProp->isPrivateSet() ? "D" : "d";
echo $readonlyProp->getModifiers();
echo ":";
$sealedProp = new ReflectionProperty("EvalReflectMemberChild", "sealed");
echo $sealedProp->isFinal() ? "F" : "f";
echo $sealedProp->isPublic() ? "U" : "u";
echo $sealedProp->getModifiers();
echo ":";
$staticFinalProp = new ReflectionProperty("EvalReflectMemberChild", "staticSeal");
echo $staticFinalProp->isFinal() ? "F" : "f";
echo $staticFinalProp->isStatic() ? "S" : "s";
echo $staticFinalProp->getModifiers();
echo ":";
$abstractProp = new ReflectionProperty("EvalReflectAbstractProperty", "mustRead");
echo $abstractProp->isAbstract() ? "A" : "a";
echo $abstractProp->isFinal() ? "F" : "f";
echo $abstractProp->getModifiers();
echo ":";
$classReadonlyProp = new ReflectionProperty("EvalReflectReadonlyClass", "classReadonly");
echo $classReadonlyProp->isReadOnly() ? "C" : "c";
echo $classReadonlyProp->isProtectedSet() ? "T" : "t";
echo $classReadonlyProp->isPrivateSet() ? "D" : "d";
echo $classReadonlyProp->getModifiers();
echo ":";
echo $visibleProp->isDynamic() ? "D" : "d";');
"#,
    );
    assert!(
        out.success,
        "program failed: stdout={:?} stderr={}",
        out.stdout, out.stderr
    );
    assert_eq!(
        out.stdout,
        "SPurfa:APs:FUs:SRpfartd20:sPufartd2:RUTd2177:FU33:FS49:Af577:CTd2177:d"
    );
}

/// Verifies eval ReflectionProperty reports generated asymmetric set-visibility predicates.
#[test]
fn test_eval_reflection_property_set_visibility_predicates_for_aot_class() {
    let out = compile_and_run(
        r#"<?php
class EvalAotReflectSetVisibility {
    public private(set) int $privateSet = 1;
    public protected(set) int $protectedSet = 2;
}
eval('$private = new ReflectionProperty("EvalAotReflectSetVisibility", "privateSet");
echo $private->isPrivateSet() ? "P" : "p";
echo $private->isProtectedSet() ? "T" : "t";
echo $private->getModifiers(); echo ":";
$protected = new ReflectionProperty("EvalAotReflectSetVisibility", "protectedSet");
echo $protected->isPrivateSet() ? "P" : "p";
echo $protected->isProtectedSet() ? "T" : "t";
echo $protected->getModifiers();');
"#,
    );
    assert_eq!(out, "Pt4129:pT2049");
}

/// Verifies eval can observe AOT constructor-promotion metadata through
/// `ReflectionProperty::isPromoted()`.
#[test]
fn test_eval_reflection_property_is_promoted_for_aot_class() {
    let out = compile_and_run_capture(
        r#"<?php
class EvalAotPromotedBase {
    public function __construct(public int $id, protected string $name = "Ada") {}
}
class EvalAotPromotedChild extends EvalAotPromotedBase {}
class EvalAotPromotedPlain {
    public int $id = 0;
    public static int $count = 0;
}
eval('$id = new ReflectionProperty("EvalAotPromotedBase", "id");
echo $id->isPromoted() ? "I" : "i";
$root = new ReflectionProperty("\EvalAotPromotedBase", "id");
echo $root->isPromoted() ? "I" : "i";
$name = new ReflectionProperty("EvalAotPromotedBase", "name");
echo $name->isPromoted() ? "N" : "n";
$child = new ReflectionProperty("EvalAotPromotedChild", "id");
echo $child->isPromoted() ? "C" : "c";
$plain = new ReflectionProperty("EvalAotPromotedPlain", "id");
echo $plain->isPromoted() ? "P" : "p";
$static = new ReflectionProperty("EvalAotPromotedPlain", "count");
echo $static->isPromoted() ? "S" : "s";
$class = new ReflectionClass("EvalAotPromotedBase");
echo $class->hasProperty("id") ? "H" : "h";
echo $class->hasProperty("missing") ? "M" : "m";
$listed = $class->getProperty("id");
echo $listed->isPromoted() ? "G" : "g";
echo $listed->getDeclaringClass()->getName();
$rootClass = new ReflectionClass("\EvalAotPromotedBase");
echo $rootClass->hasProperty("name") ? "N" : "n";
$properties = $class->getProperties();
echo ":" . count($properties);
$listedId = false;
$listedName = false;
foreach ($properties as $property) {
    if ($property->getName() === "id") {
        $listedId = $property->isPromoted();
    }
    if ($property->getName() === "name") {
        $listedName = $property->isPromoted();
    }
}
echo $listedId ? "I" : "i";
echo $listedName ? "N" : "n";
$publicProperties = $class->getProperties(ReflectionProperty::IS_PUBLIC);
$protectedProperties = $class->getProperties(filter: ReflectionProperty::IS_PROTECTED);
echo ":" . count($publicProperties) . $publicProperties[0]->getName();
echo ":" . count($protectedProperties) . $protectedProperties[0]->getName();
echo ":" . count($class->getProperties(0));
try {
    $class->getProperty("missing");
    echo "bad";
} catch (ReflectionException $e) {
    echo ":" . $e->getMessage();
}');
"#,
    );
    assert!(
        out.success,
        "program failed: stdout={:?} stderr={}",
        out.stdout, out.stderr
    );
    assert_eq!(
        out.stdout,
        "IINCpsHmGEvalAotPromotedBaseN:2IN:1id:1name:0:Property EvalAotPromotedBase::$missing does not exist"
    );
}

/// Verifies eval reports declaring classes for inherited generated/AOT properties.
#[test]
fn test_eval_reflection_property_declaring_class_for_inherited_aot_members() {
    let out = compile_and_run_capture(
        r#"<?php
class EvalAotReflectPropertyDeclaringBase {
    public int $base = 1;
    protected static string $baseStatic = "s";
}
class EvalAotReflectPropertyDeclaringChild extends EvalAotReflectPropertyDeclaringBase {
    public int $own = 2;
}
echo eval('$class = new ReflectionClass("EvalAotReflectPropertyDeclaringChild");
$base = $class->getProperty("base");
echo $base->getDeclaringClass()->getName() . ":";
$static = $class->getProperty("baseStatic");
echo $static->getDeclaringClass()->getName() . ":";
$own = $class->getProperty("own");
echo $own->getDeclaringClass()->getName() . ":";
$listed = null;
foreach ($class->getProperties() as $property) {
    if ($property->getName() === "base") {
        $listed = $property;
    }
}
echo $listed->getDeclaringClass()->getName();
');
"#,
    );
    assert!(
        out.success,
        "program failed: stdout={:?} stderr={}",
        out.stdout, out.stderr
    );
    assert_eq!(
        out.stdout,
        "EvalAotReflectPropertyDeclaringBase:EvalAotReflectPropertyDeclaringBase:EvalAotReflectPropertyDeclaringChild:EvalAotReflectPropertyDeclaringBase"
    );
}

/// Verifies eval exposes declared generated/AOT property types through
/// `ReflectionProperty::hasType()` and `getType()`.
#[test]
fn test_eval_reflection_property_exposes_aot_type_metadata() {
    let out = compile_and_run_capture(
        r#"<?php
class EvalAotReflectPropertyTypeDep {}
class EvalAotReflectPropertyTypeBase {
    protected ?string $baseName = null;
}
class EvalAotReflectPropertyTypeTarget extends EvalAotReflectPropertyTypeBase {
    public int|string $id = 0;
    public ?EvalAotReflectPropertyTypeDep $dep = null;
    public static ?int $count = null;
    public $untyped = 1;
}
echo eval('$id = new ReflectionProperty("EvalAotReflectPropertyTypeTarget", "id");
echo $id->hasType() ? "H:" : "h:";
$type = $id->getType();
$parts = $type->getTypes();
echo $parts[0]->getName() . ($parts[0]->isBuiltin() ? "B" : "C");
echo "," . $parts[1]->getName() . ($parts[1]->isBuiltin() ? "B" : "C");
echo $type->allowsNull() ? ":N" : ":n";
$dep = new ReflectionProperty("EvalAotReflectPropertyTypeTarget", "dep");
$depType = $dep->getType();
echo ":" . ($dep->hasType() ? "D" : "d");
echo $depType->allowsNull() ? "?" : "!";
echo $depType->getName() . ($depType->isBuiltin() ? "B" : "C");
$static = new ReflectionProperty("EvalAotReflectPropertyTypeTarget", "count");
$staticType = $static->getType();
echo ":" . ($static->hasType() ? "S" : "s");
echo $staticType->allowsNull() ? "?" : "!";
echo $staticType->getName() . ($staticType->isBuiltin() ? "B" : "C");
$base = new ReflectionProperty("EvalAotReflectPropertyTypeTarget", "baseName");
$baseType = $base->getType();
echo ":" . ($base->hasType() ? "B" : "b");
echo $baseType->allowsNull() ? "?" : "!";
echo $baseType->getName() . ($baseType->isBuiltin() ? "B" : "C");
$untyped = new ReflectionProperty("EvalAotReflectPropertyTypeTarget", "untyped");
echo ":" . ($untyped->hasType() ? "U" : "u");
echo $untyped->getType() === null ? "N" : "n";
');
"#,
    );
    assert!(
        out.success,
        "program failed: stdout={:?} stderr={}",
        out.stdout, out.stderr
    );
    assert_eq!(
        out.stdout,
        "H:intB,stringB:n:D?EvalAotReflectPropertyTypeDepC:S?intB:B?stringB:uN"
    );
}

/// Verifies eval exposes supported generated/AOT property defaults through
/// `ReflectionProperty::hasDefaultValue()` and `getDefaultValue()`.
#[test]
fn test_eval_reflection_property_exposes_aot_default_metadata() {
    let out = compile_and_run_capture(
        r#"<?php
class EvalAotReflectPropertyDefaultBase {
    public $implicit;
    protected int $base = 3;
}
class EvalAotReflectPropertyDefaultTarget extends EvalAotReflectPropertyDefaultBase {
    public int $count = 7;
    public static string $label = "ok";
    public ?string $nullable = null;
    public bool $flag = true;
    public float $neg = -1.5;
    public int $typed;
}
echo eval('foreach (["count", "label", "nullable", "implicit", "typed", "base", "flag", "neg"] as $name) {
    $property = new ReflectionProperty("EvalAotReflectPropertyDefaultTarget", $name);
    echo $name . ":";
    echo $property->hasDefaultValue() ? "D:" : "d:";
    $value = $property->getDefaultValue();
    echo $value === null ? "null" : $value;
    echo "|";
}
$listed = null;
foreach ((new ReflectionClass("EvalAotReflectPropertyDefaultTarget"))->getProperties() as $property) {
    if ($property->getName() === "count") {
        $listed = $property;
    }
}
echo "listed:";
echo $listed->hasDefaultValue() ? "D:" : "d:";
echo $listed->getDefaultValue();
');
"#,
    );
    assert!(
        out.success,
        "program failed: stdout={:?} stderr={}",
        out.stdout, out.stderr
    );
    assert_eq!(
        out.stdout,
        "count:D:7|label:D:ok|nullable:D:null|implicit:D:null|typed:d:null|base:D:3|flag:D:1|neg:D:-1.5|listed:D:7"
    );
}

/// Verifies eval exposes generated/AOT method and property attributes through
/// `ReflectionMethod::getAttributes()` and `ReflectionProperty::getAttributes()`.
#[test]
fn test_eval_reflection_member_exposes_aot_attributes() {
    let out = compile_and_run_capture(
        r#"<?php
class EvalAotMemberAttr {
    public function __construct($first = null, $second = null, $third = null, $fourth = null) {}
}
class EvalAotReflectAttrBase {
    #[EvalAotMemberAttr("base", 7, true, null)]
    public function baseRun() {}
    #[EvalAotMemberAttr("baseProp")]
    protected int $baseId = 1;
}
class EvalAotReflectAttrTarget extends EvalAotReflectAttrBase {
    #[EvalAotMemberAttr("method")]
    public function run() {}
    #[EvalAotMemberAttr("property", -3)]
    public int $id = 2;
}
echo eval('$methodAttrs = (new ReflectionMethod("EvalAotReflectAttrTarget", "run"))->getAttributes();
echo "M" . count($methodAttrs) . ":";
echo $methodAttrs[0]->getName() . ":" . $methodAttrs[0]->getArguments()[0] . ":";
$propertyAttrs = (new ReflectionProperty("EvalAotReflectAttrTarget", "id"))->getAttributes();
echo "P" . count($propertyAttrs) . ":";
echo $propertyAttrs[0]->getName() . ":";
$propertyArgs = $propertyAttrs[0]->getArguments();
echo $propertyArgs[0] . ":" . $propertyArgs[1] . ":";
$baseMethodAttrs = (new ReflectionMethod("EvalAotReflectAttrTarget", "baseRun"))->getAttributes();
echo "BM" . count($baseMethodAttrs) . ":";
$args = $baseMethodAttrs[0]->getArguments();
echo $args[0] . ":" . $args[1] . ":" . ($args[2] ? "T" : "F") . ":" . ($args[3] === null ? "N" : "n") . ":";
$basePropertyAttrs = (new ReflectionProperty("EvalAotReflectAttrTarget", "baseId"))->getAttributes();
echo "BP" . count($basePropertyAttrs) . ":" . $basePropertyAttrs[0]->getArguments()[0] . ":";
$listedMethod = (new ReflectionClass("EvalAotReflectAttrTarget"))->getMethod("run");
echo count($listedMethod->getAttributes()) . ":";
$listedProperty = (new ReflectionClass("EvalAotReflectAttrTarget"))->getProperty("id");
echo count($listedProperty->getAttributes());
');
"#,
    );
    assert!(
        out.success,
        "program failed: stdout={:?} stderr={}",
        out.stdout, out.stderr
    );
    assert_eq!(
        out.stdout,
        "M1:EvalAotMemberAttr:method:P1:EvalAotMemberAttr:property:-3:BM1:base:7:T:N:BP1:baseProp:1:1"
    );
}

/// Verifies eval can probe generated/AOT method predicate metadata through
/// `ReflectionClass::hasMethod()`, `getMethod()`, and direct `ReflectionMethod`.
#[test]
fn test_eval_reflection_method_for_aot_class() {
    let out = compile_and_run_capture(
        r#"<?php
class EvalAotReflectMethodBase {
    protected static function baseStatic() {}
    final public function locked() {}
}
class EvalAotReflectMethodChild extends EvalAotReflectMethodBase {
    public function run() {}
    private function hidden() {}
}
eval('$class = new ReflectionClass("EvalAotReflectMethodChild");
echo $class->hasMethod("RUN") ? "R" : "r";
echo $class->hasMethod("BASESTATIC") ? "B" : "b";
echo $class->hasMethod("missing") ? "M" : "m";
$run = $class->getMethod("RUN");
echo ":" . $run->getName();
echo $run->isPublic() ? "U" : "u";
echo $run->isStatic() ? "S" : "s";
echo $run->getDeclaringClass()->getName();
$base = $class->getMethod("baseStatic");
echo ":" . ($base->isStatic() ? "S" : "s");
echo $base->isProtected() ? "P" : "p";
$locked = new ReflectionMethod("EvalAotReflectMethodBase", "LOCKED");
echo ":" . $locked->getName();
echo $locked->isFinal() ? "F" : "f";
echo $locked->isPublic() ? "U" : "u";
echo $locked->getDeclaringClass()->getName();
$methods = $class->getMethods();
$seenRun = false;
$seenBase = false;
$seenLocked = false;
foreach ($methods as $method) {
    if (strtolower($method->getName()) === "run") {
        $seenRun = $method->isPublic();
    }
    if (strtolower($method->getName()) === "basestatic") {
        $seenBase = $method->isStatic();
    }
    if (strtolower($method->getName()) === "locked") {
        $seenLocked = $method->isFinal();
    }
}
echo ":" . count($methods);
echo $seenRun ? "R" : "r";
echo $seenBase ? "B" : "b";
echo $seenLocked ? "L" : "l";
$staticMethods = $class->getMethods(ReflectionMethod::IS_STATIC);
$privateMethods = $class->getMethods(filter: ReflectionMethod::IS_PRIVATE);
$seenStatic = false;
$seenHidden = false;
foreach ($staticMethods as $method) {
    if (strtolower($method->getName()) === "basestatic") {
        $seenStatic = $method->isProtected();
    }
}
foreach ($privateMethods as $method) {
    if (strtolower($method->getName()) === "hidden") {
        $seenHidden = $method->isPrivate();
    }
}
echo ":" . count($staticMethods) . ($seenStatic ? "S" : "s");
echo ":" . count($privateMethods) . ($seenHidden ? "H" : "h");
echo ":" . count($class->getMethods(0));
try {
    $class->getMethod("missing");
    echo "bad";
} catch (ReflectionException $e) {
    echo ":" . $e->getMessage();
}');
"#,
    );
    assert!(
        out.success,
        "program failed: stdout={:?} stderr={}",
        out.stdout, out.stderr
    );
    assert_eq!(
        out.stdout,
        "RBm:runUsEvalAotReflectMethodChild:SP:lockedFUEvalAotReflectMethodBase:4RBL:1S:1H:0:Method EvalAotReflectMethodChild::missing() does not exist"
    );
}

/// Verifies eval reports declaring classes for inherited generated/AOT methods and constructors.
#[test]
fn test_eval_reflection_method_declaring_class_for_inherited_aot_members() {
    let out = compile_and_run_capture(
        r#"<?php
class EvalAotReflectDeclaringBase {
    public function __construct(string $name = "base") {}

    public function inherited(): string {
        return "base";
    }

    protected static function baseStatic(): string {
        return "static";
    }
}
class EvalAotReflectDeclaringChild extends EvalAotReflectDeclaringBase {
    public function own(): string {
        return "child";
    }
}
echo eval('$class = new ReflectionClass("EvalAotReflectDeclaringChild");
$inherited = $class->getMethod("inherited");
echo $inherited->getDeclaringClass()->getName() . ":";
$static = $class->getMethod("baseStatic");
echo $static->getDeclaringClass()->getName() . ":";
$own = $class->getMethod("own");
echo $own->getDeclaringClass()->getName() . ":";
$ctor = $class->getConstructor();
echo $ctor->getDeclaringClass()->getName() . "/" . $ctor->getNumberOfParameters() . ":";
$listed = null;
foreach ($class->getMethods() as $method) {
    if ($method->getName() === "inherited") {
        $listed = $method;
    }
}
echo $listed->getDeclaringClass()->getName();
');
"#,
    );
    assert!(
        out.success,
        "program failed: stdout={:?} stderr={}",
        out.stdout, out.stderr
    );
    assert_eq!(
        out.stdout,
        "EvalAotReflectDeclaringBase:EvalAotReflectDeclaringBase:EvalAotReflectDeclaringChild:EvalAotReflectDeclaringBase/1:EvalAotReflectDeclaringBase"
    );
}

/// Verifies eval ReflectionMethod::invoke can dispatch public generated/AOT methods.
#[test]
fn test_eval_reflection_method_invoke_calls_aot_method() {
    let out = compile_and_run_capture(
        r#"<?php
class EvalAotReflectInvokeBase {
    public function who(): string {
        return static::class;
    }

    public static function make(string $left, string $right = "S"): string {
        return static::class . ":" . $left . $right;
    }
}
class EvalAotReflectInvokeChild extends EvalAotReflectInvokeBase {
    public function join(string $a, string $b = "B"): string {
        return $a . $b;
    }
}
echo eval('$object = new EvalAotReflectInvokeChild();
$who = (new ReflectionClass("EvalAotReflectInvokeChild"))->getMethod("who");
echo $who->invoke($object) . ":";
$static = new ReflectionMethod("EvalAotReflectInvokeBase", "make");
echo $static->invoke(null, right: "Y", left: "X") . ":";
echo $static->invoke($object, "A") . ":";
$join = new ReflectionMethod("EvalAotReflectInvokeChild", "join");
echo $join->invoke($object, "Q") . ":";
return $join->invokeArgs($object, ["b" => "2", "a" => "1"]);');
"#,
    );
    assert!(
        out.success,
        "program failed: stdout={:?} stderr={}",
        out.stdout, out.stderr
    );
    assert_eq!(
        out.stdout,
        "EvalAotReflectInvokeChild:EvalAotReflectInvokeBase:XY:EvalAotReflectInvokeBase:AS:QB:12"
    );
}

/// Verifies eval ReflectionMethod exposes registered generated/AOT parameter metadata.
#[test]
fn test_eval_reflection_method_exposes_aot_parameter_metadata() {
    let out = compile_and_run_capture(
        r#"<?php
class EvalAotReflectParamTarget {
    public function join(string $left, string $right = "B", ?int $count = null): string {
        return $left . $right . ($count ?? 0);
    }

    public static function sum(int $first, int $second = 2): int {
        return $first + $second;
    }
}
echo eval('$method = new ReflectionMethod("EvalAotReflectParamTarget", "join");
echo $method->getNumberOfParameters() . "/" . $method->getNumberOfRequiredParameters() . ":";
foreach ($method->getParameters() as $param) {
    echo $param->getName();
    echo $param->isOptional() ? "O" : "r";
    echo $param->isDefaultValueAvailable() ? "=" : "-";
    if ($param->isDefaultValueAvailable()) {
        $default = $param->getDefaultValue();
        echo is_null($default) ? "null" : $default;
    }
    echo ";";
}
$static = new ReflectionMethod("EvalAotReflectParamTarget", "sum");
echo ":" . $static->getNumberOfParameters() . "/" . $static->getNumberOfRequiredParameters() . ":";
foreach ($static->getParameters() as $param) {
    echo $param->getName();
    echo $param->isOptional() ? "O" : "r";
    echo $param->isDefaultValueAvailable() ? "=" : "-";
    if ($param->isDefaultValueAvailable()) {
        echo $param->getDefaultValue();
    }
    echo ";";
}
$listed = null;
foreach ((new ReflectionClass("EvalAotReflectParamTarget"))->getMethods() as $candidate) {
    if ($candidate->getName() === "join") {
        $listed = $candidate;
    }
}
echo ":" . $listed->getNumberOfParameters() . "/" . $listed->getParameters()[2]->getName();
');
"#,
    );
    assert!(
        out.success,
        "program failed: stdout={:?} stderr={}",
        out.stdout, out.stderr
    );
    assert_eq!(
        out.stdout,
        "3/1:leftr-;rightO=B;countO=null;:2/1:firstr-;secondO=2;:3/count"
    );
}

/// Verifies eval ReflectionMethod exposes generated/AOT declared type metadata.
#[test]
fn test_eval_reflection_method_exposes_aot_type_metadata() {
    let out = compile_and_run_capture(
        r#"<?php
interface EvalAotReflectTypeLeft {}
interface EvalAotReflectTypeRight {}
class EvalAotReflectTypeBoth implements EvalAotReflectTypeLeft, EvalAotReflectTypeRight {}
class EvalAotReflectTypeDep {}
class EvalAotReflectTypeTarget {
    public function describe(int|string $id, ?EvalAotReflectTypeDep $dep): ?string {
        return null;
    }

    public static function factory(EvalAotReflectTypeDep $dep): EvalAotReflectTypeDep {
        return $dep;
    }

    public function both(EvalAotReflectTypeLeft&EvalAotReflectTypeRight $value): void {}
}
echo eval('$method = new ReflectionMethod("EvalAotReflectTypeTarget", "describe");
$params = $method->getParameters();
$union = $params[0]->getType();
echo "U" . count($union->getTypes());
foreach ($union->getTypes() as $type) {
    echo ":" . $type->getName() . ($type->isBuiltin() ? "B" : "C");
}
$dep = $params[1]->getType();
echo ":D" . ($dep->allowsNull() ? "?" : "!") . ":" . $dep->getName() . ($dep->isBuiltin() ? "B" : "C");
$return = $method->getReturnType();
echo ":R" . ($return->allowsNull() ? "?" : "!") . ":" . $return->getName() . ($return->isBuiltin() ? "B" : "C");
$static = (new ReflectionMethod("EvalAotReflectTypeTarget", "factory"))->getReturnType();
echo ":S" . ($static->allowsNull() ? "?" : "!") . ":" . $static->getName() . ($static->isBuiltin() ? "B" : "C");
$intersection = (new ReflectionMethod("EvalAotReflectTypeTarget", "both"))->getParameters()[0]->getType();
echo ":I" . count($intersection->getTypes());
foreach ($intersection->getTypes() as $type) {
    echo ":" . $type->getName() . ($type->isBuiltin() ? "B" : "C");
}');
"#,
    );
    assert!(
        out.success,
        "program failed: stdout={:?} stderr={}",
        out.stdout, out.stderr
    );
    assert_eq!(
        out.stdout,
        "U2:intB:stringB:D?:EvalAotReflectTypeDepC:R?:stringB:S!:EvalAotReflectTypeDepC:I2:EvalAotReflectTypeLeftC:EvalAotReflectTypeRightC"
    );
}

/// Verifies eval ReflectionClass::getConstructor exposes generated/AOT constructor metadata.
#[test]
fn test_eval_reflection_class_get_constructor_for_aot_class() {
    let out = compile_and_run_capture(
        r#"<?php
class EvalAotReflectCtorParamTarget {
    public string $label = "";

    public function __construct(string $left, string $right = "B", ?int $count = null) {
        $this->label = $left . $right . ($count ?? 0);
    }
}
class EvalAotReflectCtorPlain {}
echo eval('$ctor = (new ReflectionClass("EvalAotReflectCtorParamTarget"))->getConstructor();
echo ($ctor instanceof ReflectionMethod) ? "M:" : "m:";
echo $ctor->getName() . "/" . $ctor->getDeclaringClass()->getName() . ":";
echo $ctor->getNumberOfParameters() . "/" . $ctor->getNumberOfRequiredParameters() . ":";
foreach ($ctor->getParameters() as $param) {
    echo $param->getName();
    echo $param->isOptional() ? "O" : "r";
    echo $param->isDefaultValueAvailable() ? "=" : "-";
    if ($param->isDefaultValueAvailable()) {
        $default = $param->getDefaultValue();
        echo is_null($default) ? "null" : $default;
    }
    $type = $param->getType();
    echo ":";
    echo $type === null ? "none" : $type->getName() . ($type->allowsNull() ? "?" : "!");
    echo ";";
}
$listed = null;
foreach ((new ReflectionClass("EvalAotReflectCtorParamTarget"))->getMethods() as $candidate) {
    if ($candidate->getName() === "__construct") {
        $listed = $candidate;
    }
}
echo ":" . $listed->getNumberOfParameters() . "/" . $listed->getParameters()[0]->getName();
$plain = (new ReflectionClass("EvalAotReflectCtorPlain"))->getConstructor();
echo ":" . ($plain === null ? "null" : "bad");
');
"#,
    );
    assert!(
        out.success,
        "program failed: stdout={:?} stderr={}",
        out.stdout, out.stderr
    );
    assert_eq!(
        out.stdout,
        "M:__construct/EvalAotReflectCtorParamTarget:3/1:leftr-:string!;rightO=B:string!;countO=null:int?;:3/left:null"
    );
}

/// Verifies eval ReflectionMethod constructor/destructor predicates through the bridge.
#[test]
fn test_eval_reflection_method_reports_constructor_and_destructor() {
    let out = compile_and_run_capture(
        r#"<?php
eval('class EvalReflectLifecycle {
    public function __construct() {}
    public function __destruct() {}
    public function run() {}
}
$ctor = new ReflectionMethod("EvalReflectLifecycle", "__CONSTRUCT");
echo $ctor->isConstructor() ? "C" : "c";
echo $ctor->isDestructor() ? "D" : "d";
echo ":";
$dtor = new ReflectionMethod("EvalReflectLifecycle", "__destruct");
echo $dtor->isConstructor() ? "C" : "c";
echo $dtor->isDestructor() ? "D" : "d";
echo ":";
$run = new ReflectionMethod("EvalReflectLifecycle", "run");
echo $run->isConstructor() ? "C" : "c";
echo $run->isDestructor() ? "D" : "d";
echo ":";
$listed = (new ReflectionClass("EvalReflectLifecycle"))->getConstructor();
echo $listed->isConstructor() ? "C" : "c";
echo $listed->isDestructor() ? "D" : "d";');
"#,
    );
    assert!(
        out.success,
        "program failed: stdout={:?} stderr={}",
        out.stdout, out.stderr
    );
    assert_eq!(out.stdout, "Cd:cD:cd:Cd");
}

/// Verifies eval ReflectionMethod keeps declared name case after case-insensitive lookup.
#[test]
fn test_eval_reflection_method_preserves_declared_name_case() {
    let out = compile_and_run_capture(
        r#"<?php
eval('class EvalReflectMethodCaseBase {
    public function MiXeDCase() { return "base"; }
}
class EvalReflectMethodCaseChild extends EvalReflectMethodCaseBase {
    public function childCase() { return "child"; }
}
$object = new EvalReflectMethodCaseChild();
$direct = new ReflectionMethod("EvalReflectMethodCaseChild", "mixedcase");
echo $direct->getName() . ":";
echo $direct->getShortName() . ":";
echo $direct->invoke($object) . ":";
$listed = (new ReflectionClass("EvalReflectMethodCaseChild"))->getMethod("CHILDCASE");
echo $listed->getName() . ":";
echo $listed->invoke($object);');
"#,
    );
    assert!(
        out.success,
        "program failed: stdout={:?} stderr={}",
        out.stdout, out.stderr
    );
    assert_eq!(out.stdout, "MiXeDCase:MiXeDCase:base:childCase:child");
}

/// Verifies eval ReflectionMethod accepts object targets through the bridge.
#[test]
fn test_eval_reflection_method_accepts_object_targets() {
    let out = compile_and_run_capture(
        r#"<?php
class EvalAotReflectMethodObjectBase {
    public function aotBase() { return "aot-base"; }
}
class EvalAotReflectMethodObjectChild extends EvalAotReflectMethodObjectBase {
    public function aotChild() { return "aot-child"; }
}
eval('class EvalReflectMethodObjectBase {
    public function MiXeDCase() { return "base"; }
}
class EvalReflectMethodObjectChild extends EvalReflectMethodObjectBase {
    public function childCase() { return "child"; }
}
$object = new EvalReflectMethodObjectChild();
$inherited = new ReflectionMethod($object, "mixedcase");
echo $inherited->getName() . ":";
echo $inherited->getDeclaringClass()->getName() . ":";
echo $inherited->invoke($object) . ":";
$own = new ReflectionMethod($object, "CHILDCASE");
echo $own->getName() . ":";
echo $own->getDeclaringClass()->getName() . ":";
echo $own->invoke($object) . "|";
$aot = new EvalAotReflectMethodObjectChild();
$aotInherited = new ReflectionMethod($aot, "aotbase");
echo $aotInherited->getName() . ":";
echo $aotInherited->getDeclaringClass()->getName() . ":";
$aotOwn = new ReflectionMethod($aot, "aotchild");
echo $aotOwn->getName() . ":";
echo $aotOwn->getDeclaringClass()->getName();');
"#,
    );
    assert!(
        out.success,
        "program failed: stdout={:?} stderr={}",
        out.stdout, out.stderr
    );
    assert_eq!(
        out.stdout,
        "MiXeDCase:EvalReflectMethodObjectBase:base:childCase:EvalReflectMethodObjectChild:child|aotbase:EvalAotReflectMethodObjectBase:aotchild:EvalAotReflectMethodObjectChild"
    );
}

/// Verifies eval-declared final properties cannot be redeclared by subclasses.
#[test]
fn test_eval_declared_final_property_override_fails() {
    let err = compile_and_run_expect_failure(
        r#"<?php
eval('class EvalFinalPropertyBase {
    final public $value = 1;
}
class EvalFinalPropertyChild extends EvalFinalPropertyBase {
    public $value = 2;
}');
"#,
    );
    assert!(
        err.contains("Fatal error: eval() runtime failed"),
        "stderr did not contain eval runtime fatal diagnostic: {err}"
    );
}

/// Verifies eval reflectors expose their declaring class through the bridge.
#[test]
fn test_eval_reflection_members_report_declaring_class() {
    let out = compile_and_run_capture(
        r#"<?php
eval('class EvalDeclaringBase {
    public $baseProp = 1;
    public function inherited() { return "base"; }
    public const BASE_CONST = 10;
}
class EvalDeclaringChild extends EvalDeclaringBase {
    public $childProp = 2;
    public function own() { return "child"; }
    public const CHILD_CONST = 20;
}
enum EvalDeclaringEnum: string {
    case Ready = "ready";
    public const LEVEL = 3;
}
echo (new ReflectionMethod("EvalDeclaringChild", "inherited"))->getDeclaringClass()->getName() . ":";
echo (new ReflectionClass("EvalDeclaringChild"))->getMethod("own")->getDeclaringClass()->getName() . ":";
echo (new ReflectionProperty("EvalDeclaringChild", "baseProp"))->getDeclaringClass()->getName() . ":";
echo (new ReflectionClass("EvalDeclaringChild"))->getProperty("childProp")->getDeclaringClass()->getName() . ":";
echo (new ReflectionClass("EvalDeclaringChild"))->getReflectionConstant("BASE_CONST")->getDeclaringClass()->getName() . ":";
echo (new ReflectionClassConstant("EvalDeclaringChild", "BASE_CONST"))->getDeclaringClass()->getName() . ":";
echo (new ReflectionClass("EvalDeclaringEnum"))->getReflectionConstant("Ready")->getDeclaringClass()->getName() . ":";
echo (new ReflectionEnumBackedCase("EvalDeclaringEnum", "Ready"))->getDeclaringClass()->getName();');
"#,
    );
    assert!(
        out.success,
        "program failed: stdout={:?} stderr={}",
        out.stdout, out.stderr
    );
    assert_eq!(
        out.stdout,
        "EvalDeclaringBase:EvalDeclaringChild:EvalDeclaringBase:EvalDeclaringChild:EvalDeclaringBase:EvalDeclaringBase:EvalDeclaringEnum:EvalDeclaringEnum"
    );
}

/// Verifies eval ReflectionClass getMethods/getProperties return member objects through the bridge.
#[test]
fn test_eval_reflection_class_lists_member_objects() {
    let out = compile_and_run_capture(
        r#"<?php
eval('#[Attribute]
class EvalListMarker {}
class EvalReflectListTarget {
    #[EvalListMarker]
    public function first() {}
    private static function helper() {}
    #[EvalListMarker]
    protected $visible;
    private static $token;
}
$ref = new ReflectionClass("EvalReflectListTarget");
$methods = $ref->getMethods();
$properties = $ref->getProperties();
$staticMethods = $ref->getMethods(ReflectionMethod::IS_STATIC);
$privateMethods = $ref->getMethods(filter: ReflectionMethod::IS_PRIVATE);
$noMethods = $ref->getMethods(0);
$nullMethods = $ref->getMethods(null);
$staticProperties = $ref->getProperties(ReflectionProperty::IS_STATIC);
$protectedProperties = $ref->getProperties(filter: ReflectionProperty::IS_PROTECTED);
$noProperties = $ref->getProperties(0);
echo count($methods) . ":" . count($properties) . ":";
echo ReflectionMethod::IS_STATIC . ":" . ReflectionMethod::IS_PRIVATE . ":";
$direct = new ReflectionMethod("EvalReflectListTarget", "helper");
echo "D" . $direct->getModifiers() . ":";
foreach ($methods as $method) {
    if ($method->getName() === "first") {
        echo "F" . count($method->getAttributes());
        echo "M" . $method->getModifiers();
    }
    if ($method->getName() === "helper") {
        echo $method->isStatic() ? "S" : "s";
        echo $method->isPrivate() ? "R" : "r";
        echo "M" . $method->getModifiers();
    }
}
echo ":";
foreach ($properties as $property) {
    if ($property->getName() === "visible") {
        echo "V" . count($property->getAttributes());
        echo $property->isProtected() ? "P" : "p";
        echo "M" . $property->getModifiers();
    }
    if ($property->getName() === "token") {
        echo $property->isStatic() ? "T" : "t";
        echo $property->isPrivate() ? "R" : "r";
        echo "M" . $property->getModifiers();
    }
}
echo ":";
echo count($staticMethods) . $staticMethods[0]->getName() . ":";
echo count($privateMethods) . $privateMethods[0]->getName() . ":";
echo count($noMethods) . ":" . count($nullMethods) . ":";
echo count($staticProperties) . $staticProperties[0]->getName() . ":";
echo count($protectedProperties) . $protectedProperties[0]->getName() . ":";
echo count($noProperties);');
"#,
    );
    assert!(
        out.success,
        "program failed: stdout={:?} stderr={}",
        out.stdout, out.stderr
    );
    assert_eq!(
        out.stdout,
        "2:2:16:4:D20:F1M1SRM20:V1PM2TRM20:1helper:1helper:0:2:1token:1visible:0"
    );
}

/// Verifies eval ReflectionClass getMethod/getProperty return single member objects.
#[test]
fn test_eval_reflection_class_get_method_and_property_lookup_members() {
    let out = compile_and_run_capture(
        r#"<?php
eval('class EvalReflectLookupTarget {
    public function first() {}
    private static function helper() {}
    protected $visible;
    private static $token;
}
$ref = new ReflectionClass("EvalReflectLookupTarget");
$method = $ref->getMethod("FIRST");
echo $method->getName() . ":";
echo $method->isPublic() ? "U" : "u";
echo ":";
$helper = $ref->getMethod("helper");
echo $helper->isPrivate() ? "P" : "p";
echo $helper->isStatic() ? "S" : "s";
echo ":";
$property = $ref->getProperty("visible");
echo $property->getName() . ":";
echo $property->isProtected() ? "R" : "r";
echo ":";
try {
    $ref->getProperty("Visible");
    echo "bad";
} catch (ReflectionException $e) {
    echo $e->getMessage();
}
echo ":";
try {
    $ref->getMethod("missing");
    echo "bad";
} catch (ReflectionException $e) {
    echo $e->getMessage();
}');
"#,
    );
    assert!(
        out.success,
        "program failed: stdout={:?} stderr={}",
        out.stdout, out.stderr
    );
    assert_eq!(
        out.stdout,
        "first:U:PS:visible:R:Property EvalReflectLookupTarget::$Visible does not exist:Method EvalReflectLookupTarget::missing() does not exist"
    );
}

/// Verifies eval ReflectionMethod materializes ReflectionParameter objects through the bridge.
#[test]
fn test_eval_reflection_method_lists_parameters() {
    let out = compile_and_run_capture(
        r#"<?php
eval('interface EvalReflectLeft {}
interface EvalReflectRight {}
class EvalReflectParamTarget {
    public function run(#[EvalParamTag("first")] int &$first, int|string $union, #[EvalParamTag("both")] EvalReflectLeft&EvalReflectRight $both, ?array $items = null, ?callable $callback = null, \App\Name|null $second = null, &...$rest) {}
}
$method = new ReflectionMethod("EvalReflectParamTarget", "run");
echo $method->getNumberOfParameters() . "/";
echo $method->getNumberOfRequiredParameters() . ":";
$params = $method->getParameters();
foreach ($params as $param) {
    echo $param->getName() . "@" . $param->getPosition();
    echo $param->isOptional() ? "O" : "r";
    echo $param->isVariadic() ? "V" : "v";
    echo $param->isPassedByReference() ? "R" : "b";
    echo $param->canBePassedByValue() ? "Y" : "N";
    echo $param->hasType() ? "T" : "t";
    echo $param->allowsNull() ? "N" : "n";
    echo $param->isArray() ? "A" : "a";
    echo $param->isCallable() ? "C" : "c";
    $type = $param->getType();
    if ($param->getName() == "union") {
        echo ":union";
        echo $type->allowsNull() ? "?" : "!";
        foreach ($type->getTypes() as $memberType) {
            echo ":" . $memberType->getName();
            echo $memberType->isBuiltin() ? "B" : "C";
        }
    } elseif ($param->getName() == "both") {
        echo ":intersection";
        echo $type->allowsNull() ? "?" : "!";
        foreach ($type->getTypes() as $memberType) {
            echo ":" . $memberType->getName();
            echo $memberType->isBuiltin() ? "B" : "C";
        }
    } elseif ($type) {
        echo ":" . $type->getName();
        echo $type->allowsNull() ? "?" : "!";
        echo $type->isBuiltin() ? "B" : "C";
    } else {
        echo ":null";
    }
    $attrs = $param->getAttributes();
    echo ":A" . count($attrs);
    if (count($attrs) > 0) {
        echo ":" . $attrs[0]->getName();
        echo ":" . $attrs[0]->getArguments()[0];
    }
    echo $param->isDefaultValueAvailable() ? ":D" : ":d";
    if ($param->isDefaultValueAvailable()) {
        echo "=";
        echo $param->getDefaultValue() === null ? "null" : $param->getDefaultValue();
    }
    echo "|";
}');
"#,
    );
    assert!(
        out.success,
        "program failed: stdout={:?} stderr={}",
        out.stdout, out.stderr
    );
    assert_eq!(
        out.stdout,
        "7/3:first@0rvRNTnac:int!B:A1:EvalParamTag:first:d|union@1rvbYTnac:union!:intB:stringB:A0:d|both@2rvbYTnac:intersection!:EvalReflectLeftC:EvalReflectRightC:A1:EvalParamTag:both:d|items@3OvbYTNAc:array?B:A0:D=null|callback@4OvbYTNaC:callable?B:A0:D=null|second@5OvbYTNac:App\\Name?C:A0:D=null|rest@6OVRNtNac:null:A0:d|"
    );
}

/// Verifies eval ReflectionParameter exposes PHP constant-default metadata.
#[test]
fn test_eval_reflection_parameter_reports_default_constant_metadata() {
    let out = compile_and_run_capture(
        r#"<?php
eval('define("EVAL_REFLECT_PARAM_DEFAULT_GLOBAL", "G");
class EvalReflectParamDefaultBase {
    const BASE = "B";
}
class EvalReflectParamDefaultTarget extends EvalReflectParamDefaultBase {
    const LABEL = "L";
    public function run($required, $global = EVAL_REFLECT_PARAM_DEFAULT_GLOBAL, $self = self::LABEL, $parent = parent::BASE, $literal = 7) {}
}
$params = (new ReflectionMethod("EvalReflectParamDefaultTarget", "run"))->getParameters();
foreach ($params as $param) {
    echo $param->getName() . ":";
    echo $param->isDefaultValueAvailable() ? "D:" : "d:";
    if ($param->isDefaultValueAvailable()) {
        if ($param->isDefaultValueConstant()) {
            echo "C:";
            echo $param->getDefaultValueConstantName();
            echo ":";
        } else {
            echo "c:null:";
        }
        echo $param->getDefaultValue();
    }
    echo "|";
}');
"#,
    );
    assert!(
        out.success,
        "program failed: stdout={:?} stderr={}",
        out.stdout, out.stderr
    );
    assert_eq!(
        out.stdout,
        "required:d:|global:D:C:EVAL_REFLECT_PARAM_DEFAULT_GLOBAL:G|self:D:C:self::LABEL:L|parent:D:C:parent::BASE:B|literal:D:c:null:7|"
    );
}

/// Verifies eval ReflectionMethod exposes eval-declared return type metadata.
#[test]
fn test_eval_reflection_method_reports_return_type_metadata() {
    let out = compile_and_run_capture(
        r#"<?php
eval('interface EvalReflectReturnIface {
    public function read(): string;
}
class EvalReflectReturnTarget implements EvalReflectReturnIface {
    public function read(): string { return "ok"; }
    public function selfReturn(): static { return $this; }
    public function done(): void {}
}
$iface = new ReflectionMethod("EvalReflectReturnIface", "read");
$ifaceType = $iface->getReturnType();
echo ($iface->hasReturnType() ? "I" : "i") . ":";
echo $ifaceType->getName() . ":";
echo ($ifaceType->isBuiltin() ? "B" : "b") . ":";
$self = (new ReflectionMethod("EvalReflectReturnTarget", "selfReturn"))->getReturnType();
echo $self->getName() . ":";
echo ($self->isBuiltin() ? "B" : "b") . ":";
$void = (new ReflectionMethod("EvalReflectReturnTarget", "done"))->getReturnType();
echo $void->getName() . ":";
echo ($void->allowsNull() ? "N" : "n") . ":";
echo $void->isBuiltin() ? "B" : "b";');
"#,
    );
    assert!(
        out.success,
        "program failed: stdout={:?} stderr={}",
        out.stdout, out.stderr
    );
    assert_eq!(out.stdout, "I:string:B:static:b:void:n:B");
}

/// Verifies eval ReflectionProperty materializes property get/set type metadata through the bridge.
#[test]
fn test_eval_reflection_property_get_type_metadata() {
    let out = compile_and_run_capture(
        r#"<?php
eval('class EvalReflectPropertyTypeDep {}
class EvalReflectPropertyTypeTarget {
    public int $id;
    public ?string $name;
    public EvalReflectPropertyTypeDep $dep;
    public $plain;
    public int|string $union;
}
$properties = (new ReflectionClass("EvalReflectPropertyTypeTarget"))->getProperties();
foreach ($properties as $property) {
    echo $property->getName() . ":";
    echo $property->hasType() ? "T:" : "t:";
    $type = $property->getType();
    if ($property->getName() == "union") {
        echo "union";
        echo $type->allowsNull() ? "?" : "!";
        foreach ($type->getTypes() as $memberType) {
            echo ":" . $memberType->getName();
            echo $memberType->isBuiltin() ? "B" : "C";
        }
    } elseif ($type) {
        echo $type->getName();
        echo $type->allowsNull() ? "?" : "!";
        echo $type->isBuiltin() ? "B" : "C";
    } else {
        echo "null";
    }
    echo "|";
}
$direct = new ReflectionProperty("EvalReflectPropertyTypeTarget", "dep");
$directType = $direct->getType();
echo "direct:";
echo $direct->hasType() ? "T:" : "t:";
echo $directType->getName();
$directSettableType = $direct->getSettableType();
echo ":set:" . $directSettableType->getName();
$plain = new ReflectionProperty("EvalReflectPropertyTypeTarget", "plain");
echo ":plainSet:" . ($plain->getSettableType() === null ? "N" : "n");
$directUnion = new ReflectionProperty("EvalReflectPropertyTypeTarget", "union");
echo ":unionSet:" . count($directUnion->getSettableType()->getTypes());');
"#,
    );
    assert!(
        out.success,
        "program failed: stdout={:?} stderr={}",
        out.stdout, out.stderr
    );
    assert_eq!(
        out.stdout,
        "id:T:int!B|name:T:string?B|dep:T:EvalReflectPropertyTypeDep!C|plain:t:null|union:T:union!:intB:stringB|direct:T:EvalReflectPropertyTypeDep:set:EvalReflectPropertyTypeDep:plainSet:N:unionSet:2"
    );
}

/// Verifies eval ReflectionProperty materializes property default metadata through the bridge.
#[test]
fn test_eval_reflection_property_get_default_value_metadata() {
    let out = compile_and_run_capture(
        r#"<?php
eval('class EvalReflectPropertyDefaultTarget {
    public $implicit;
    public int $typed;
    public ?string $nullableTyped;
    public $explicitNull = null;
    public int $count = 7;
    public static string $label = "ok";
}
foreach (["implicit", "typed", "nullableTyped", "explicitNull", "count", "label"] as $name) {
    $property = new ReflectionProperty("EvalReflectPropertyDefaultTarget", $name);
    echo $property->getName() . ":";
    echo $property->isDefault() ? "Y:" : "N:";
    echo $property->hasDefaultValue() ? "D:" : "d:";
    $value = $property->getDefaultValue();
    echo $value === null ? "null" : $value;
    echo "|";
}
$listed = (new ReflectionClass("EvalReflectPropertyDefaultTarget"))->getProperty("implicit");
echo "listed:";
echo $listed->isDefault() ? "Y:" : "N:";
echo $listed->hasDefaultValue() ? "D:" : "d:";
echo $listed->getDefaultValue() === null ? "null" : "bad";');
"#,
    );
    assert!(
        out.success,
        "program failed: stdout={:?} stderr={}",
        out.stdout, out.stderr
    );
    assert_eq!(
        out.stdout,
        "implicit:Y:D:null|typed:Y:d:null|nullableTyped:Y:d:null|explicitNull:Y:D:null|count:Y:D:7|label:Y:D:ok|listed:Y:D:null"
    );
}

/// Verifies eval ReflectionProperty materializes dynamic object properties through the bridge.
#[test]
fn test_eval_reflection_property_supports_dynamic_properties() {
    let out = compile_and_run_capture(
        r#"<?php
eval('class EvalReflectDynamicBridgeBase {}
class EvalReflectDynamicBridgeChild extends EvalReflectDynamicBridgeBase {}
$object = new EvalReflectDynamicBridgeBase();
$object->dynamic = "first";
$child = new EvalReflectDynamicBridgeChild();
$child->dynamic = "child";
$empty = new EvalReflectDynamicBridgeChild();
$property = new ReflectionProperty($object, "dynamic");
echo $property->getName(); echo ":";
echo $property->isDynamic() ? "D" : "d"; echo ":";
echo $property->isDefault() ? "Y" : "N"; echo ":";
echo $property->getModifiers(); echo ":";
echo $property->hasDefaultValue() ? "H" : "h"; echo ":";
echo is_null($property->getType()) ? "T" : "t"; echo ":";
echo $property->isInitialized($object) ? "I" : "i"; echo ":";
echo $property->getValue($object); echo ":";
echo $property->getValue($child); echo ":";
echo $property->isInitialized($empty) ? "E" : "e"; echo ":";
$property->setValue($empty, "filled");
echo $property->getValue($empty); echo ":";
$property->setRawValue($object, "raw");
echo $property->getRawValue($object); echo ":";
echo str_replace("\n", "\\n", $property->__toString());');
"#,
    );
    assert!(
        out.success,
        "program failed: stdout={:?} stderr={}",
        out.stdout, out.stderr
    );
    assert_eq!(
        out.stdout,
        "dynamic:D:N:1:h:T:I:first:child:e:filled:raw:Property [ <dynamic> public $dynamic ]\n"
    );
}

/// Verifies eval ReflectionProperty formats retained property metadata through `__toString()`.
#[test]
fn test_eval_reflection_property_to_string() {
    let out = compile_and_run_capture(
        r#"<?php
eval('class EvalReflectPropertyStringTarget {
    public int $id = 7;
    protected static string $label = "ok";
    private $implicit;
    public $virtual {
        get => 1;
    }
}
foreach (["id", "label", "implicit", "virtual"] as $name) {
    echo (new ReflectionProperty("EvalReflectPropertyStringTarget", $name))->__toString();
    echo "|";
}');
"#,
    );
    assert!(
        out.success,
        "program failed: stdout={:?} stderr={}",
        out.stdout, out.stderr
    );
    assert_eq!(
        out.stdout,
        "Property [ public int $id = 7 ]|Property [ protected static string $label = 'ok' ]|Property [ private $implicit = NULL ]|Property [ public $virtual ]|"
    );
}

/// Verifies eval ReflectionClass materializes property default metadata through the bridge.
#[test]
fn test_eval_reflection_class_get_default_properties_metadata() {
    let out = compile_and_run_capture(
        r#"<?php
	eval('class EvalReflectClassDefaultBase {
    public int $base = 1;
    protected string $prot = "p";
    private int $shadow = 3;
    public $implicit;
    public int $typed;
    public static string $baseStatic = "bs";
}
class EvalReflectClassDefaultChild extends EvalReflectClassDefaultBase {
    public int $child = 5;
    private int $shadow = 9;
    public static int $childStatic = 7;
    public ?int $nullable = null;
}
$defaults = (new ReflectionClass("EvalReflectClassDefaultChild"))->getDefaultProperties();
echo $defaults["childStatic"] . ":";
echo $defaults["baseStatic"] . ":";
echo $defaults["child"] . ":";
echo $defaults["shadow"] . ":";
echo $defaults["base"] . ":";
echo $defaults["prot"] . ":";
echo array_key_exists("implicit", $defaults) && $defaults["implicit"] === null ? "I:" : "i:";
echo array_key_exists("nullable", $defaults) && $defaults["nullable"] === null ? "N:" : "n:";
echo array_key_exists("typed", $defaults) ? "T" : "t";');
"#,
    );
    assert!(
        out.success,
        "program failed: stdout={:?} stderr={}",
        out.stdout, out.stderr
    );
    assert_eq!(out.stdout, "7:bs:5:9:1:p:I:N:t");
}

/// Verifies eval ReflectionProperty value APIs use current runtime object values.
#[test]
fn test_eval_reflection_property_gets_and_sets_values() {
    let out = compile_and_run_capture(
        r#"<?php
eval('class EvalReflectValueBase {
    private $secret = "base";
    public static $count = 1;
}
class EvalReflectValueChild extends EvalReflectValueBase {
    protected $name = "Ada";
}
class EvalReflectValueHook {
    public $raw = 2;
    public $doubled {
        get => $this->raw * 2;
        set { $this->raw = $value + 1; }
    }
    public $backed {
        get { return $this->backed * 2; }
        set { $this->backed = $value; }
    }
    public $virtual {
        get => $this->raw + 100;
    }
    public function __construct() {
        $this->backed = 2;
    }
}
$child = new EvalReflectValueChild();
$secret = new ReflectionProperty("EvalReflectValueBase", "secret");
echo $secret->getValue($child) . ":";
$secret->setValue($child, "changed");
echo $secret->getValue(object: $child) . ":";
$name = new ReflectionProperty("EvalReflectValueChild", "name");
echo $name->getValue($child) . ":";
$name->setValue(objectOrValue: $child, value: "Grace");
echo $name->getValue($child) . ":";
$count = new ReflectionProperty("EvalReflectValueBase", "count");
echo $count->getValue() . ":";
$count->setValue(5);
echo EvalReflectValueChild::$count . ":";
$count->setValue(null, 6);
echo $count->getValue($child) . ":";
$hook = new EvalReflectValueHook();
$doubled = new ReflectionProperty("EvalReflectValueHook", "doubled");
echo $doubled->getValue($hook) . ":";
$doubled->setValue($hook, 4);
echo $hook->raw . ":";
echo $doubled->getValue($hook) . ":";
$backed = new ReflectionProperty("EvalReflectValueHook", "backed");
echo $backed->getRawValue($hook) . ":";
echo $backed->getValue($hook) . ":";
$backed->setValue($hook, 4);
echo $backed->getRawValue(object: $hook) . ":";
echo $backed->getValue($hook) . ":";
$backed->setRawValue(object: $hook, value: 7);
echo $backed->getRawValue($hook) . ":";
echo $backed->getValue($hook) . ":";
echo $backed->isLazy($hook) ? "L:" : "l:";
$backed->skipLazyInitialization(object: $hook);
$backed->setRawValueWithoutLazyInitialization(object: $hook, value: 8);
echo $backed->getRawValue($hook) . ":";
echo $backed->getValue($hook) . ":";
echo $backed->getModifiers() . ":";
echo $backed->isVirtual() ? "V:" : "b:";
echo (new ReflectionProperty("EvalReflectValueHook", "virtual"))->isVirtual() ? "V:" : "b:";
echo (new ReflectionProperty("EvalReflectValueHook", "virtual"))->getModifiers();');
"#,
    );
    assert!(
        out.success,
        "program failed: stdout={:?} stderr={}",
        out.stdout, out.stderr
    );
    assert_eq!(
        out.stdout,
        "base:changed:Ada:Grace:1:5:6:4:5:10:2:4:4:8:7:14:l:8:16:1:b:V:513"
    );
}

/// Verifies eval ReflectionProperty raw APIs reject virtual property hooks.
#[test]
fn test_eval_reflection_property_virtual_raw_value_fails() {
    let err = compile_and_run_expect_failure(
        r#"<?php
eval('class EvalReflectVirtualRawHook {
    public $raw = 2;
    public $virtual {
        get => $this->raw * 2;
    }
}
$object = new EvalReflectVirtualRawHook();
$property = new ReflectionProperty("EvalReflectVirtualRawHook", "virtual");
$property->getRawValue($object);');
"#,
    );
    assert!(
        err.contains("Fatal error: eval() runtime failed"),
        "stderr did not contain eval runtime fatal diagnostic: {err}"
    );
}

/// Verifies eval ReflectionProperty reports instance and static initialization state.
#[test]
fn test_eval_reflection_property_is_initialized() {
    let out = compile_and_run_capture(
        r#"<?php
eval('class EvalReflectInitializedTarget {
    public int $typed;
    public ?int $nullable;
    public $plain;
    public static int $staticTyped;
    public static $staticPlain;
    public $virtual {
        get => 42;
    }
}
$object = new EvalReflectInitializedTarget();
$typed = new ReflectionProperty("EvalReflectInitializedTarget", "typed");
$nullable = new ReflectionProperty("EvalReflectInitializedTarget", "nullable");
$plain = new ReflectionProperty("EvalReflectInitializedTarget", "plain");
$staticTyped = new ReflectionProperty("EvalReflectInitializedTarget", "staticTyped");
$staticPlain = new ReflectionProperty("EvalReflectInitializedTarget", "staticPlain");
$virtual = new ReflectionProperty("EvalReflectInitializedTarget", "virtual");
echo $typed->isInitialized($object) ? "T:" : "t:";
echo $plain->isInitialized(object: $object) ? "P:" : "p:";
echo $staticTyped->isInitialized() ? "S:" : "s:";
echo $staticPlain->isInitialized() ? "N:" : "n:";
EvalReflectInitializedTarget::$staticTyped = 3;
echo $staticTyped->isInitialized() ? "S:" : "s:";
$object->typed = 5;
echo $typed->isInitialized($object) ? "T:" : "t:";
unset($object->typed);
echo $typed->isInitialized($object) ? "T:" : "t:";
$typed->setRawValue(object: $object, value: 9);
echo $typed->isInitialized($object) ? "T:" : "t:";
echo $nullable->isInitialized($object) ? "Y:" : "y:";
$nullable->setValue($object, null);
echo $nullable->isInitialized($object) ? "Y:" : "y:";
echo $virtual->isInitialized($object) ? "V" : "v";');
"#,
    );
    assert!(
        out.success,
        "program failed: stdout={:?} stderr={}",
        out.stdout, out.stderr
    );
    assert_eq!(out.stdout, "t:P:s:N:S:T:t:T:y:Y:V");
}

/// Verifies eval ReflectionProperty exposes property hook metadata and hook methods.
#[test]
fn test_eval_reflection_property_hook_metadata() {
    let out = compile_and_run_capture(
        r#"<?php
eval('class EvalReflectHookedProperty {
    public int $raw = 2;
    public int $doubled {
        get { return $this->raw * 2; }
        set { $this->raw = $value; }
    }
    public int $readonlyHook {
        get => $this->raw + 1;
    }
    public int $plain = 5;
}
abstract class EvalReflectAbstractHookProperty {
    abstract public int $contract { get; set; }
}
interface EvalReflectInterfaceHookProperty {
    public int $iface { get; }
}
$hooked = new ReflectionProperty("EvalReflectHookedProperty", "doubled");
$plain = new ReflectionProperty("EvalReflectHookedProperty", "plain");
$readonly = new ReflectionProperty("EvalReflectHookedProperty", "readonlyHook");
$abstract = new ReflectionProperty("EvalReflectAbstractHookProperty", "contract");
$iface = new ReflectionProperty("EvalReflectInterfaceHookProperty", "iface");
$getCase = PropertyHookType::Get;
$setCase = PropertyHookType::Set;
echo $getCase->name . ":" . $getCase->value . ":";
$caseList = PropertyHookType::cases();
echo count($caseList) . ":" . $caseList[0]->name . ":" . $caseList[1]->value . ":";
echo PropertyHookType::from("set")->name . ":";
echo (PropertyHookType::tryFrom("missing") === null ? "T" : "t") . ":";
echo ($hooked->hasHooks() ? "H" : "h") . ":";
echo ($hooked->hasHook($getCase) ? "G" : "g") . ":";
echo ($hooked->hasHook(type: $setCase) ? "S" : "s") . ":";
$hooks = $hooked->getHooks();
echo count($hooks) . ":" . $hooks["get"]->getName() . ":" . $hooks["set"]->getName() . ":";
$get = $hooked->getHook($getCase);
$set = $hooked->getHook(type: $setCase);
echo $get->getDeclaringClass()->getName() . ":" . $get->getNumberOfParameters() . ":";
echo $set->getNumberOfParameters() . ":" . $set->getParameters()[0]->getName() . ":";
$box = new EvalReflectHookedProperty();
echo $get->invoke($box) . ":";
$set->invoke($box, 7);
echo $box->raw . ":";
echo ($readonly->hasHook($getCase) ? "R" : "r") . ":";
echo ($readonly->hasHook($setCase) ? "w" : "W") . ":";
echo ($readonly->getHook($setCase) === null ? "N" : "n") . ":";
echo ($plain->hasHooks() ? "bad" : "plain") . ":";
echo count($plain->getHooks()) . ":";
$abstractHooks = $abstract->getHooks();
echo count($abstractHooks) . ":";
echo ($abstract->hasHook($getCase) ? "AG" : "ag") . ":";
echo ($abstract->hasHook($setCase) ? "AS" : "as") . ":";
echo $abstractHooks["get"]->getName() . ":" . ($abstractHooks["get"]->isAbstract() ? "A" : "a") . ":";
echo $abstractHooks["set"]->getName() . ":" . ($abstractHooks["set"]->isAbstract() ? "A" : "a") . ":";
$ifaceHook = $iface->getHook($getCase);
echo count($iface->getHooks()) . ":";
echo ($iface->hasHook($getCase) ? "IG" : "ig") . ":";
echo ($iface->hasHook($setCase) ? "bad" : "is") . ":";
echo $ifaceHook->isAbstract() ? "IA" : "ia";');
"#,
    );
    assert!(
        out.success,
        "program failed: stdout={:?} stderr={}",
        out.stdout, out.stderr
    );
    assert_eq!(
        out.stdout,
        "Get:get:2:Get:set:Set:T:H:G:S:2:$doubled::get:$doubled::set:EvalReflectHookedProperty:0:1:value:4:7:R:W:N:plain:0:2:AG:AS:$contract::get:A:$contract::set:A:1:IG:is:IA"
    );
}

/// Verifies eval ReflectionClass static-property APIs use current runtime values.
#[test]
fn test_eval_reflection_class_static_property_values() {
    let out = compile_and_run_capture(
        r#"<?php
	eval('class EvalReflectStaticBase {
    public static $base = "b";
    protected static $prot = "p";
    private static $shadow = "base-hidden";
    public $instance = "i";
}
class EvalReflectStaticChild extends EvalReflectStaticBase {
    public static $child = "c";
    private static $shadow = "child-hidden";
    public static int $count = 1;
}
EvalReflectStaticChild::$child = "mut";
$ref = new ReflectionClass("EvalReflectStaticChild");
$statics = $ref->getStaticProperties();
echo count($statics) . ":";
echo $statics["child"] . ":";
echo $statics["base"] . ":";
echo $statics["prot"] . ":";
echo $statics["shadow"] . ":";
echo $ref->getStaticPropertyValue("count") . ":";
$ref->setStaticPropertyValue("shadow", "changed");
echo $ref->getStaticPropertyValue("shadow") . ":";
$ref->setStaticPropertyValue(name: "count", value: 5);
echo EvalReflectStaticChild::$count . ":";
echo $ref->getStaticPropertyValue("instance", "fallback") . ":";
echo $ref->getStaticPropertyValue("missing", "fallback") . ":";
try {
    $ref->getStaticPropertyValue("missing");
    echo "bad";
} catch (ReflectionException $e) {
    echo "E";
}
echo ":";
try {
    $ref->setStaticPropertyValue("instance", "bad");
    echo "bad";
} catch (ReflectionException $e) {
    echo "S";
}');
"#,
    );
    assert!(
        out.success,
        "program failed: stdout={:?} stderr={}",
        out.stdout, out.stderr
    );
    assert_eq!(
        out.stdout,
        "5:mut:b:p:child-hidden:1:changed:5:fallback:fallback:E:S"
    );
}

/// Verifies eval ReflectionParameter exposes the declaring class for method parameters.
#[test]
fn test_eval_reflection_parameter_reports_declaring_class() {
    let out = compile_and_run_capture(
        r#"<?php
eval('class EvalDeclaringParamBase {
    public function inherited($base) {}
}
class EvalDeclaringParamChild extends EvalDeclaringParamBase {
    public function own($child) {}
}
$inherited = (new ReflectionMethod("EvalDeclaringParamChild", "inherited"))->getParameters()[0];
echo $inherited->getDeclaringClass()->getName() . ":";
echo $inherited->getDeclaringFunction()->getName() . ":";
echo $inherited->getDeclaringFunction()->getDeclaringClass()->getName() . ":";
$listed = (new ReflectionMethod("EvalDeclaringParamChild", "own"))->getParameters()[0];
echo $listed->getDeclaringClass()->getName() . ":";
echo $listed->getDeclaringFunction()->getName();');
"#,
    );
    assert!(
        out.success,
        "program failed: stdout={:?} stderr={}",
        out.stdout, out.stderr
    );
    assert_eq!(
        out.stdout,
        "EvalDeclaringParamBase:inherited:EvalDeclaringParamBase:EvalDeclaringParamChild:own"
    );
}

/// Verifies eval ReflectionFunction materializes eval-declared function parameters.
#[test]
fn test_eval_reflection_function_reports_eval_function_parameters() {
    let out = compile_and_run_capture(
        r#"<?php
eval('function eval_reflect_free($left, $right) {
    return $left;
}
$ref = new ReflectionFunction("eval_reflect_free");
$params = $ref->getParameters();
echo $ref->getName() . ":";
echo $ref->getNumberOfParameters() . ":";
echo $ref->getNumberOfRequiredParameters() . ":";
echo count($params) . ":";
echo $params[0]->getName() . ":";
echo $params[1]->getPosition() . ":";
$declaring = $params[0]->getDeclaringFunction();
echo get_class($declaring) . ":";
echo $declaring->getName();');
"#,
    );
    assert!(
        out.success,
        "program failed: stdout={:?} stderr={}",
        out.stdout, out.stderr
    );
    assert_eq!(
        out.stdout,
        "eval_reflect_free:2:2:2:left:1:ReflectionFunction:eval_reflect_free"
    );
}

/// Verifies eval ReflectionFunction preserves rich eval-declared function signatures.
#[test]
fn test_eval_reflection_function_reports_signature_metadata() {
    let out = compile_and_run_capture(
        r#"<?php
eval('class EvalFuncAttr {
    public $label;
    public function __construct($label) { $this->label = $label; }
    public function label() { return $this->label; }
}
#[EvalFuncAttr("free")]
function eval_reflect_rich(#[EvalFuncAttr("first")] string $name, int $count = 3, &...$items) {
    return $count;
}
$ref = new ReflectionFunction("eval_reflect_rich");
$attrs = $ref->getAttributes();
$params = $ref->getParameters();
echo count($attrs) . ":";
echo $attrs[0]->getName() . ":";
echo $attrs[0]->newInstance()->label() . ":";
echo $ref->getNumberOfParameters() . ":";
echo $ref->getNumberOfRequiredParameters() . ":";
echo ($params[0]->hasType() ? "T" : "t") . ":";
echo $params[0]->getType()->getName() . ":";
$paramAttrs = $params[0]->getAttributes();
echo count($paramAttrs) . ":";
echo $paramAttrs[0]->newInstance()->label() . ":";
echo ($params[1]->isOptional() ? "O" : "o") . ":";
echo $params[1]->getDefaultValue() . ":";
echo ($params[2]->isVariadic() ? "V" : "v") . ":";
echo $params[2]->isPassedByReference() ? "R" : "r";');
"#,
    );
    assert!(
        out.success,
        "program failed: stdout={:?} stderr={}",
        out.stdout, out.stderr
    );
    assert_eq!(
        out.stdout,
        "1:EvalFuncAttr:free:3:1:T:string:1:first:O:3:V:R"
    );
}

/// Verifies eval ReflectionFunction exposes eval-declared return type metadata.
#[test]
fn test_eval_reflection_function_reports_return_type_metadata() {
    let out = compile_and_run_capture(
        r#"<?php
eval('function eval_reflect_return_named(): ?int { return 1; }
function eval_reflect_return_union(): int|string { return 1; }
function eval_reflect_return_never(): never { throw new Exception("stop"); }
function eval_reflect_return_plain() {}
$namedRef = new ReflectionFunction("eval_reflect_return_named");
$named = $namedRef->getReturnType();
echo ($namedRef->hasReturnType() ? "T" : "t") . ":";
echo $named->getName() . ":";
echo ($named->allowsNull() ? "N" : "n") . ":";
echo ($named->isBuiltin() ? "B" : "b") . ":";
$union = (new ReflectionFunction("eval_reflect_return_union"))->getReturnType();
echo count($union->getTypes()) . ":";
foreach ($union->getTypes() as $type) {
    echo $type->getName();
    echo $type->isBuiltin() ? "B" : "b";
}
echo ":";
$never = (new ReflectionFunction("eval_reflect_return_never"))->getReturnType();
echo $never->getName() . ":";
echo ($never->allowsNull() ? "N" : "n") . ":";
echo ($never->isBuiltin() ? "B" : "b") . ":";
$plain = new ReflectionFunction("eval_reflect_return_plain");
echo ($plain->hasReturnType() ? "P" : "p") . ":";
echo $plain->getReturnType() === null ? "Q" : "q";');
"#,
    );
    assert!(
        out.success,
        "program failed: stdout={:?} stderr={}",
        out.stdout, out.stderr
    );
    assert_eq!(out.stdout, "T:int:N:B:2:intBstringB:never:n:B:p:Q");
}

/// Verifies eval Reflection origin metadata APIs are present on supported owners.
#[test]
fn test_eval_reflection_origin_metadata_defaults() {
    let out = compile_and_run_capture(
        r#"<?php
eval('class EvalReflectOriginTarget {
    public $id;
    public const ANSWER = 42;
    public function run() {}
}
function eval_reflect_origin_function() {}
enum EvalReflectOriginCase: string {
    case Ready = "ready";
}
$class = new ReflectionClass("EvalReflectOriginTarget");
$function = new ReflectionFunction("eval_reflect_origin_function");
$method = new ReflectionMethod("EvalReflectOriginTarget", "run");
$property = new ReflectionProperty("EvalReflectOriginTarget", "id");
$constant = new ReflectionClassConstant("EvalReflectOriginTarget", "ANSWER");
$unit = new ReflectionEnumUnitCase("EvalReflectOriginCase", "Ready");
$backed = new ReflectionEnumBackedCase("EvalReflectOriginCase", "Ready");
echo ($class->getDocComment() === false) ? "C" : "c"; echo ":";
echo ($function->getDocComment() === false) ? "F" : "f"; echo ":";
echo ($method->getDocComment() === false) ? "M" : "m"; echo ":";
echo ($property->getDocComment() === false) ? "P" : "p"; echo ":";
echo ($constant->getDocComment() === false) ? "K" : "k"; echo ":";
echo ($unit->getDocComment() === false) ? "U" : "u"; echo ":";
echo ($backed->getDocComment() === false) ? "B" : "b"; echo ":";
echo ($class->getExtensionName() === false) ? "E" : "e"; echo ":";
echo ($function->getExtensionName() === false) ? "N" : "n"; echo ":";
echo ($method->getExtensionName() === false) ? "O" : "o"; echo ":";
echo ($class->getExtension() === null) ? "X" : "x"; echo ":";
echo ($function->getExtension() === null) ? "Y" : "y"; echo ":";
echo ($method->getExtension() === null) ? "Z" : "z";');
"#,
    );
    assert!(
        out.success,
        "program failed: stdout={:?} stderr={}",
        out.stdout, out.stderr
    );
    assert_eq!(out.stdout, "C:F:M:P:K:U:B:E:N:O:X:Y:Z");
}

/// Verifies eval ReflectionFunction/Method expose name and origin predicate metadata.
#[test]
fn test_eval_reflection_function_and_method_name_origin_predicates() {
    let out = compile_and_run_capture(
        r#"<?php
eval('namespace EvalReflectNameNs;
function sample(...$items) {}
class Target {
    public function run(...$items) {}
}
$fn = new \ReflectionFunction("EvalReflectNameNs\\\\sample");
$method = new \ReflectionMethod(Target::class, "run");
echo $fn->getShortName() . ":";
echo $fn->getNamespaceName() . ":";
echo ($fn->inNamespace() ? "Y" : "N") . ":";
echo ($fn->isInternal() ? "I" : "i");
echo ($fn->isUserDefined() ? "U" : "u") . ":";
echo ($fn->isClosure() ? "C" : "c") . ":";
echo ($fn->isDeprecated() ? "D" : "d") . ":";
echo ($fn->returnsReference() ? "R" : "r") . ":";
echo ($fn->hasReturnType() ? "T" : "t") . ":";
echo ($fn->getReturnType() === null ? "N" : "n") . ":";
echo ($fn->isGenerator() ? "G" : "g") . ":";
echo ($fn->isVariadic() ? "V" : "v") . ":";
echo ($fn->hasTentativeReturnType() ? "H" : "h") . ":";
echo ($fn->getTentativeReturnType() === null ? "Q" : "q") . ":";
echo ($fn->isDisabled() ? "X" : "x") . "|";
echo $method->getShortName() . ":";
echo $method->getNamespaceName() . ":";
echo ($method->inNamespace() ? "Y" : "N") . ":";
echo ($method->isInternal() ? "I" : "i");
echo ($method->isUserDefined() ? "U" : "u") . ":";
echo ($method->isClosure() ? "C" : "c") . ":";
echo ($method->isDeprecated() ? "D" : "d") . ":";
echo ($method->returnsReference() ? "R" : "r") . ":";
echo ($method->hasReturnType() ? "T" : "t") . ":";
echo ($method->getReturnType() === null ? "N" : "n") . ":";
echo ($method->isGenerator() ? "G" : "g") . ":";
echo ($method->isVariadic() ? "V" : "v") . ":";
echo ($method->hasTentativeReturnType() ? "H" : "h") . ":";
echo $method->getTentativeReturnType() === null ? "Q" : "q";');
"#,
    );
    assert!(
        out.success,
        "program failed: stdout={:?} stderr={}",
        out.stdout, out.stderr
    );
    assert_eq!(
        out.stdout,
        "sample:EvalReflectNameNs:Y:iU:c:d:r:t:N:g:V:h:Q:x|run::N:iU:c:d:r:t:N:g:V:h:Q"
    );
}

/// Verifies eval ReflectionMethod hasPrototype/getPrototype follow PHP inheritance rules.
#[test]
fn test_eval_reflection_method_reports_eval_prototypes() {
    let out = compile_and_run_capture(
        r#"<?php
eval('interface EvalProtoParentIface {
    public function parented();
}
interface EvalProtoChildIface extends EvalProtoParentIface {}
interface EvalProtoIface {
    public function iface();
}
class EvalProtoBase {
    public function run() {}
    public function inherited() {}
}
class EvalProtoChild extends EvalProtoBase implements EvalProtoIface, EvalProtoChildIface {
    public function run() {}
    public function iface() {}
    public function parented() {}
    public function own() {}
}
$override = new ReflectionMethod("EvalProtoChild", "run");
$overrideProto = $override->getPrototype();
echo ($override->hasPrototype() ? "Y" : "N") . ":";
echo $overrideProto->getDeclaringClass()->getName() . "::";
echo $overrideProto->getName() . ":";
$iface = new ReflectionMethod("EvalProtoChild", "iface");
$ifaceProto = $iface->getPrototype();
echo ($iface->hasPrototype() ? "Y" : "N") . ":";
echo $ifaceProto->getDeclaringClass()->getName() . "::";
echo $ifaceProto->getName() . ":";
$parentIface = new ReflectionMethod("EvalProtoChild", "parented");
$parentIfaceProto = $parentIface->getPrototype();
echo $parentIfaceProto->getDeclaringClass()->getName() . "::";
echo $parentIfaceProto->getName() . ":";
$own = new ReflectionMethod("EvalProtoChild", "own");
echo ($own->hasPrototype() ? "Y" : "N") . ":";
try {
    $own->getPrototype();
} catch (ReflectionException $e) {
    echo "E";
}
echo ":";
$inherited = new ReflectionMethod("EvalProtoChild", "inherited");
echo $inherited->hasPrototype() ? "Y" : "N";');
"#,
    );
    assert!(
        out.success,
        "program failed: stdout={:?} stderr={}",
        out.stdout, out.stderr
    );
    assert_eq!(
        out.stdout,
        "Y:EvalProtoBase::run:Y:EvalProtoIface::iface:EvalProtoParentIface::parented:N:E:N"
    );
}

/// Verifies eval-declared functions share method-style named/default/ref/variadic binding.
#[test]
fn test_eval_declared_function_rich_argument_binding() {
    let out = compile_and_run_capture(
        r#"<?php
eval('function eval_signature_call(string $name, &$value, int $count = 2, ...$rest) {
    $value = $value + $count;
    echo $name . ":";
    echo $count . ":";
    echo count($rest) . ":";
}
function eval_signature_array(string $name, int $count = 2, ...$rest) {
    echo $name . ":";
    echo $count . ":";
    echo count($rest) . ":";
    echo $rest["extra"];
}
$seed = 4;
eval_signature_call(name: "ok", value: $seed, extra: "z");
echo $seed . ":";
call_user_func_array("eval_signature_array", ["extra" => "z", "name" => "cb"]);');
"#,
    );
    assert!(
        out.success,
        "program failed: stdout={:?} stderr={}",
        out.stdout, out.stderr
    );
    assert_eq!(out.stdout, "ok:2:1:6:cb:2:1:z");
}

/// Verifies eval ReflectionFunction::invoke and invokeArgs call eval-declared functions.
#[test]
fn test_eval_reflection_function_invoke_calls_eval_function() {
    let out = compile_and_run_capture(
        r#"<?php
eval('function eval_reflect_invoke($left = "A", $right = "B", ...$rest) {
    return $left . $right . count($rest) . $rest["extra"];
}
function eval_reflect_no_writeback(&$value) {
    $value = $value . "!";
    return $value;
}
$ref = new ReflectionFunction("eval_reflect_invoke");
echo $ref->invoke(right: "2", left: "1", extra: "X") . ":";
echo $ref->invokeArgs(["extra" => "Y", "left" => "3", "right" => "4"]) . ":";
$value = "Q";
$mutate = new ReflectionFunction("eval_reflect_no_writeback");
echo $mutate->invoke($value) . ":" . $value;');
"#,
    );
    assert!(
        out.success,
        "program failed: stdout={:?} stderr={}",
        out.stdout, out.stderr
    );
    assert_eq!(out.stdout, "121X:341Y:Q!:Q");
}

/// Verifies eval ReflectionClass::isCloneable uses eval class metadata through the bridge.
#[test]
fn test_eval_reflection_class_cloneable_predicate() {
    let out = compile_and_run(
        r#"<?php
eval('abstract class EvalCloneAbstract {}
class EvalClonePlain {}
final class EvalCloneFinal {}
class EvalClonePrivate { private function __clone() {} }
class EvalCloneProtected { protected function __clone() {} }
class EvalClonePublic { public function __clone() {} }
interface EvalCloneIface {}
trait EvalCloneTrait {}
enum EvalCloneEnum { case Ready; }
echo (new ReflectionClass("EvalCloneAbstract"))->isCloneable() ? "A" : "a";
echo (new ReflectionClass("EvalClonePlain"))->isCloneable() ? "P" : "p";
echo (new ReflectionClass("EvalCloneFinal"))->isCloneable() ? "F" : "f";
echo (new ReflectionClass("EvalClonePrivate"))->isCloneable() ? "V" : "v";
echo (new ReflectionClass("EvalCloneProtected"))->isCloneable() ? "R" : "r";
echo (new ReflectionClass("EvalClonePublic"))->isCloneable() ? "U" : "u";
echo (new ReflectionClass("EvalCloneIface"))->isCloneable() ? "I" : "i";
echo (new ReflectionClass("EvalCloneTrait"))->isCloneable() ? "T" : "t";
echo (new ReflectionClass("EvalCloneEnum"))->isCloneable() ? "E" : "e";');
"#,
    );
    assert_eq!(out, "aPFvrUite");
}

/// Verifies eval `clone` shallow-copies eval-declared objects and runs `__clone()`.
#[test]
fn test_eval_clone_object_expression_runtime_and_hook() {
    let out = compile_and_run(
        r#"<?php
eval('class EvalCloneRuntimeBox {
    public string $name;
    public function __construct($name) { $this->name = $name; }
    public function __clone() { $this->name = $this->name . ":clone"; }
}
$first = new EvalCloneRuntimeBox("A");
$second = clone $first;
echo $first->name; echo ":";
echo $second->name; echo ":";
$second->name = "B";
echo $first->name; echo ":";
echo $second->name;');
"#,
    );
    assert_eq!(out, "A:A:clone:A:B");
}

/// Verifies eval `clone` shallow-copies ordinary emitted AOT objects.
#[test]
fn test_eval_clone_aot_object_expression() {
    let out = compile_and_run(
        r#"<?php
class EvalCloneAotBox {
    public string $name;
    public int $count;

    public function __construct(string $name, int $count) {
        $this->name = $name;
        $this->count = $count;
    }

    public function run(): void {
        eval('$copy = clone $this;
$copy->name = $copy->name . ":copy";
$copy->count = $copy->count + 10;
echo $this->name; echo ":";
echo $this->count; echo ":";
echo $copy->name; echo ":";
echo $copy->count; echo ":";
$plain = new stdClass();
$plain->name = "S";
$plainCopy = clone $plain;
$plainCopy->name = "S:copy";
echo $plain->name; echo ":";
echo $plainCopy->name;');
    }
}

(new EvalCloneAotBox("A", 2))->run();
"#,
    );
    assert_eq!(out, "A:2:A:copy:12:S:S:copy");
}

/// Verifies eval `clone` invokes public AOT `__clone()` hooks after storage copying.
#[test]
fn test_eval_clone_aot_object_runs_clone_hook() {
    let out = compile_and_run(
        r#"<?php
class EvalCloneAotHookBox {
    public string $name;

    public function __construct(string $name) {
        $this->name = $name;
    }

    public function __clone(): void {
        $this->name = $this->name . ":hook";
    }

    public function run(): void {
        eval('$copy = clone $this;
echo $this->name; echo ":";
echo $copy->name;');
    }
}

(new EvalCloneAotHookBox("A"))->run();
"#,
    );
    assert_eq!(out, "A:A:hook");
}

/// Verifies eval ReflectionClass::isIterable reports eval and builtin class metadata.
#[test]
fn test_eval_reflection_class_iterable_predicate() {
    let out = compile_and_run(
        r#"<?php
eval('class EvalIterablePlain {}
abstract class EvalIterableAbstract implements Iterator {}
interface EvalIterableIface extends Iterator {}
trait EvalIterableTrait {}
enum EvalIterableEnum { case Ready; }
class EvalIterableIterator implements Iterator {
    public function current() { return null; }
    public function key() { return null; }
    public function next() {}
    public function valid() { return false; }
    public function rewind() {}
}
class EvalIterableAggregate implements IteratorAggregate {
    public function getIterator() { return $this; }
}
echo (new ReflectionClass("EvalIterablePlain"))->isIterable() ? "P" : "p";
$iter = new ReflectionClass("EvalIterableIterator");
echo $iter->isIterable() ? "I" : "i";
echo $iter->isIterateable() ? "A" : "a";
echo (new ReflectionClass("EvalIterableAggregate"))->isIterable() ? "G" : "g";
echo (new ReflectionClass("EvalIterableAbstract"))->isIterable() ? "B" : "b";
echo (new ReflectionClass("EvalIterableIface"))->isIterable() ? "F" : "f";
echo (new ReflectionClass("Iterator"))->isIterable() ? "T" : "t";
echo (new ReflectionClass("ArrayIterator"))->isIterable() ? "R" : "r";
echo (new ReflectionClass("stdClass"))->isIterable() ? "S" : "s";
echo (new ReflectionClass("EvalIterableEnum"))->isIterable() ? "E" : "e";
echo (new ReflectionClass("EvalIterableTrait"))->isIterable() ? "H" : "h";');
"#,
    );
    assert_eq!(out, "pIAGbftRseh");
}

/// Verifies eval ReflectionClass origin predicates distinguish eval symbols from built-ins.
#[test]
fn test_eval_reflection_class_internal_user_defined_predicates() {
    let out = compile_and_run(
        r#"<?php
eval('class EvalOriginClass {}
interface EvalOriginIface {}
trait EvalOriginTrait {}
enum EvalOriginEnum { case Ready; }
function eval_reflect_origin($name) {
    $r = new ReflectionClass($name);
    echo $r->isInternal() ? "I" : "i";
    echo $r->isUserDefined() ? "U" : "u";
    echo ":";
}
eval_reflect_origin("EvalOriginClass");
eval_reflect_origin("EvalOriginIface");
eval_reflect_origin("EvalOriginTrait");
eval_reflect_origin("EvalOriginEnum");
eval_reflect_origin("stdClass");
eval_reflect_origin("ReflectionClass");
eval_reflect_origin("Iterator");');
"#,
    );
    assert_eq!(out, "iU:iU:iU:iU:Iu:Iu:Iu:");
}

/// Verifies eval ReflectionClass::newInstance constructs eval-declared classes.
#[test]
fn test_eval_reflection_class_new_instance_constructs_eval_class() {
    let out = compile_and_run(
        r#"<?php
eval('class EvalReflectNewTarget {
    public $label = "";
    public function __construct($left, $right) {
        $this->label = $left . $right;
    }
    public function label() {
        return $this->label;
    }
}
$ref = new ReflectionClass("EvalReflectNewTarget");
$first = $ref->newInstance("E", "F");
echo $first->label() . ":";
$second = $ref->newInstance(...["G", "H"]);
echo $second->label() . ":";
$third = $ref->newInstanceArgs(["right" => "J", "left" => "I"]);
echo $third->label() . ":";
$fourth = $ref->newInstanceArgs(["K", "L"]);
echo $fourth->label();');
"#,
    );
    assert_eq!(out, "EF:GH:IJ:KL");
}

/// Verifies eval ReflectionMethod::invoke and invokeArgs call eval-declared methods.
#[test]
fn test_eval_reflection_method_invoke_calls_eval_method() {
    let out = compile_and_run(
        r#"<?php
eval('class EvalReflectInvokeBase {
    private function hidden($label = "H") {
        return "hidden:" . $label;
    }
    public function who() {
        return static::class;
    }
    public static function make($left, $right = "S") {
        return static::class . ":" . $left . $right;
    }
}
class EvalReflectInvokeChild extends EvalReflectInvokeBase {
    public function join($a, $b = "B") {
        return $a . $b;
    }
    public function mutate(&$value) {
        $value = $value . "!";
        return $value;
    }
}
$object = new EvalReflectInvokeChild();
$hidden = new ReflectionMethod("EvalReflectInvokeBase", "hidden");
echo $hidden->invoke($object, "X") . ":";
$who = (new ReflectionClass("EvalReflectInvokeChild"))->getMethod("who");
echo $who->invoke($object) . ":";
$static = new ReflectionMethod("EvalReflectInvokeBase", "make");
echo $static->invoke(null, right: "Y", left: "X") . ":";
echo $static->invoke($object, "A") . ":";
$join = null;
foreach ((new ReflectionClass("EvalReflectInvokeChild"))->getMethods() as $method) {
    if ($method->getName() === "join") {
        $join = $method;
    }
}
$value = "Q";
$mutate = new ReflectionMethod("EvalReflectInvokeChild", "mutate");
echo $join->invokeArgs($object, ["b" => "2", "a" => "1"]) . ":";
echo $mutate->invoke($object, $value) . ":" . $value;');
"#,
    );
    assert_eq!(
        out,
        "hidden:X:EvalReflectInvokeChild:EvalReflectInvokeBase:XY:EvalReflectInvokeBase:AS:12:Q!:Q"
    );
}

/// Verifies eval ReflectionMethod::invoke throws on incompatible receivers.
#[test]
fn test_eval_reflection_method_invoke_rejects_wrong_object() {
    let out = compile_and_run(
        r#"<?php
eval('class EvalReflectInvokeOwner {
    public function run() {
        return "owner";
    }
}
class EvalReflectInvokeOther {}
try {
    (new ReflectionMethod("EvalReflectInvokeOwner", "run"))->invoke(new EvalReflectInvokeOther());
    echo "bad";
} catch (ReflectionException $e) {
    echo "caught";
}');
"#,
    );
    assert_eq!(out, "caught");
}

/// Verifies eval ReflectionMethod/Property::setAccessible are PHP-compatible no-ops.
#[test]
fn test_eval_reflection_set_accessible_is_noop() {
    let out = compile_and_run(
        r#"<?php
eval('class EvalReflectAccessTarget {
    private $secret = "s";
    private function hidden() {
        return $this->secret;
    }
}
$object = new EvalReflectAccessTarget();
$method = new ReflectionMethod("EvalReflectAccessTarget", "hidden");
echo is_null($method->setAccessible(false)) ? "M" : "m"; echo ":";
echo $method->invoke($object); echo ":";
$property = new ReflectionProperty("EvalReflectAccessTarget", "secret");
echo is_null($property->setAccessible(accessible: true)) ? "P" : "p"; echo ":";
echo $property->getValue($object);');
"#,
    );
    assert_eq!(out, "M:s:P:s");
}

/// Verifies eval ReflectionClass::newInstanceWithoutConstructor allocates without constructors.
#[test]
fn test_eval_reflection_class_new_instance_without_constructor_allocates_eval_class() {
    let out = compile_and_run(
        r#"<?php
eval('class EvalReflectNoCtorTarget {
    public $label = "default";
    private $secret = "hidden";
    public function __construct() {
        $this->label = "ctor";
    }
    public function label() {
        return $this->label;
    }
    public function secret() {
        return $this->secret;
    }
}
$ref = new ReflectionClass("EvalReflectNoCtorTarget");
$without = $ref->newInstanceWithoutConstructor();
echo $without->label() . ":";
echo $without->secret() . ":";
$with = $ref->newInstance();
echo $with->label();');
"#,
    );
    assert_eq!(out, "default:hidden:ctor");
}

/// Verifies eval ReflectionClassConstant/EnumCase expose eval-declared attributes.
#[test]
fn test_eval_reflection_constant_and_enum_case_attributes() {
    let out = compile_and_run_capture(
        r#"<?php
eval('class EvalConstMarker {
    public $name;
    public function __construct($name) {
        $this->name = $name;
    }
    public function label() {
        return $this->name;
    }
}
class EvalConstReflectTarget {
    #[EvalConstMarker("const")]
    public const ANSWER = 42;
}
enum EvalCaseReflectTarget: string {
    #[EvalConstMarker("case")]
    case Ready = "ready";
}
$constAttrs = (new ReflectionClassConstant("EvalConstReflectTarget", "ANSWER"))->getAttributes();
echo count($constAttrs) . ":" . (new ReflectionClassConstant("EvalConstReflectTarget", "ANSWER"))->getName() . ":";
echo (new ReflectionClassConstant("EvalConstReflectTarget", "ANSWER"))->isEnumCase() ? "enum" : "plain"; echo ":";
echo $constAttrs[0]->getName() . ":" . $constAttrs[0]->getArguments()[0] . ":";
echo $constAttrs[0]->newInstance()->label() . ":";
$caseAttrs = (new ReflectionClassConstant("EvalCaseReflectTarget", "Ready"))->getAttributes();
echo count($caseAttrs) . ":" . (new ReflectionClassConstant("EvalCaseReflectTarget", "Ready"))->getName() . ":";
echo (new ReflectionClassConstant("EvalCaseReflectTarget", "Ready"))->isEnumCase() ? "enum" : "plain"; echo ":";
echo $caseAttrs[0]->getName() . ":" . $caseAttrs[0]->getArguments()[0] . ":";
$unitAttrs = (new ReflectionEnumUnitCase("EvalCaseReflectTarget", "Ready"))->getAttributes();
echo (new ReflectionEnumUnitCase("EvalCaseReflectTarget", "Ready"))->getName() . ":";
echo ((new ReflectionEnumUnitCase("EvalCaseReflectTarget", "Ready"))->getValue() === EvalCaseReflectTarget::Ready) ? "unit" : "bad"; echo ":";
echo $unitAttrs[0]->newInstance()->label() . ":";
$backedAttrs = (new ReflectionEnumBackedCase("EvalCaseReflectTarget", "Ready"))->getAttributes();
echo (new ReflectionEnumBackedCase("EvalCaseReflectTarget", "Ready"))->getName() . ":";
echo ((new ReflectionEnumBackedCase("EvalCaseReflectTarget", "Ready"))->getValue() === EvalCaseReflectTarget::Ready) ? "backed" : "bad"; echo ":";
echo (new ReflectionEnumBackedCase("EvalCaseReflectTarget", "Ready"))->getBackingValue() . ":";
echo $backedAttrs[0]->newInstance()->label();');
"#,
    );
    assert!(
        out.success,
        "program failed: stdout={:?} stderr={}",
        out.stdout, out.stderr
    );
    assert_eq!(
        out.stdout,
        "1:ANSWER:plain:EvalConstMarker:const:const:1:Ready:enum:EvalConstMarker:case:Ready:unit:case:Ready:backed:ready:case"
    );
}

/// Verifies eval ReflectionClassConstant exposes visibility predicates and modifiers.
#[test]
fn test_eval_reflection_class_constant_visibility_and_modifiers() {
    let out = compile_and_run_capture(
        r#"<?php
eval('class EvalConstVisibilityTarget {
    private const SECRET = 1;
    protected const LIMIT = 2;
    final public const ANSWER = 3;
}
enum EvalConstVisibilityEnum {
    case Ready;
}
$secret = new ReflectionClassConstant("EvalConstVisibilityTarget", "SECRET");
echo "SECRET:";
echo $secret->isPrivate() ? "R" : "r";
echo $secret->isProtected() ? "P" : "p";
echo $secret->isPublic() ? "U" : "u";
echo $secret->isFinal() ? "F" : "f";
echo ":" . $secret->getModifiers() . "\n";
$limit = new ReflectionClassConstant("EvalConstVisibilityTarget", "LIMIT");
echo "LIMIT:";
echo $limit->isPrivate() ? "R" : "r";
echo $limit->isProtected() ? "P" : "p";
echo $limit->isPublic() ? "U" : "u";
echo $limit->isFinal() ? "F" : "f";
echo ":" . $limit->getModifiers() . "\n";
$answer = new ReflectionClassConstant("EvalConstVisibilityTarget", "ANSWER");
echo "ANSWER:";
echo $answer->isPrivate() ? "R" : "r";
echo $answer->isProtected() ? "P" : "p";
echo $answer->isPublic() ? "U" : "u";
echo $answer->isFinal() ? "F" : "f";
echo ":" . $answer->getModifiers() . "\n";
$case = new ReflectionClassConstant("EvalConstVisibilityEnum", "Ready");
echo "Ready:";
echo $case->isPrivate() ? "R" : "r";
echo $case->isProtected() ? "P" : "p";
echo $case->isPublic() ? "U" : "u";
echo $case->isFinal() ? "F" : "f";
echo ":" . $case->getModifiers() . "\n";
echo "VALUES:" . $secret->getValue() . ":" . $limit->getValue() . ":" . $answer->getValue() . ":";
echo $case->getValue() === EvalConstVisibilityEnum::Ready ? "E" : "e";
echo "\n";
foreach ((new ReflectionClass("EvalConstVisibilityTarget"))->getReflectionConstants() as $constant) {
    if ($constant->getName() === "ANSWER") {
        echo "LIST:" . $constant->getValue() . "\n";
    }
}');
echo ReflectionClassConstant::IS_PUBLIC . ":";
echo ReflectionClassConstant::IS_PROTECTED . ":";
echo ReflectionClassConstant::IS_PRIVATE . ":";
echo ReflectionClassConstant::IS_FINAL;
"#,
    );
    assert!(
        out.success,
        "program failed: stdout={:?} stderr={}",
        out.stdout, out.stderr
    );
    assert_eq!(
        out.stdout,
        "SECRET:Rpuf:4\nLIMIT:rPuf:2\nANSWER:rpUF:33\nReady:rpUf:1\nVALUES:1:2:3:E\nLIST:3\n1:2:4:32"
    );
}

/// Verifies eval interface and trait constants work through the bridge.
#[test]
fn test_eval_declared_interface_and_trait_constants() {
    let out = compile_and_run_capture(
        r#"<?php
eval('interface EvalConstParentIface {
    public const BASE = 2;
}
interface EvalConstChildIface extends EvalConstParentIface {
    public const LOCAL = 3;
}
trait EvalConstReusableTrait {
    public const SEED = 6;
    public static function readTraitSeed() {
        return self::SEED;
    }
}
class EvalConstIfaceTraitBox implements EvalConstChildIface {
    use EvalConstReusableTrait;
}
echo EvalConstParentIface::BASE . ":";
echo EvalConstChildIface::BASE . ":";
echo EvalConstIfaceTraitBox::BASE . ":";
echo EvalConstIfaceTraitBox::LOCAL . ":";
echo EvalConstReusableTrait::SEED . ":";
echo EvalConstIfaceTraitBox::SEED . ":";
echo EvalConstIfaceTraitBox::readTraitSeed();');
"#,
    );
    assert!(
        out.success,
        "program failed: stdout={:?} stderr={}",
        out.stdout, out.stderr
    );
    assert_eq!(out.stdout, "2:2:2:3:6:6:6");
}

/// Verifies eval rejects private member access from outside the declaring class.
#[test]
fn test_eval_declared_private_member_access_fails() {
    let err = compile_and_run_expect_failure(
        r#"<?php
eval('class EvalPrivateAccessBox {
    private int $secret = 4;
}
$box = new EvalPrivateAccessBox();
echo $box->secret;');
"#,
    );
    assert!(
        err.contains("Fatal error: eval() runtime failed"),
        "stderr did not contain eval runtime fatal diagnostic: {err}"
    );
}

/// Verifies eval rejects protected class constant access from outside the declaring class.
#[test]
fn test_eval_declared_protected_class_constant_access_fails() {
    let err = compile_and_run_expect_failure(
        r#"<?php
eval('class EvalProtectedConstAccessBox {
    protected const SECRET = 4;
}
echo EvalProtectedConstAccessBox::SECRET;');
"#,
    );
    assert!(
        err.contains("Fatal error: eval() runtime failed"),
        "stderr did not contain eval runtime fatal diagnostic: {err}"
    );
}

/// Verifies eval rejects private static member access from outside the declaring class.
#[test]
fn test_eval_declared_private_static_member_access_fails() {
    let err = compile_and_run_expect_failure(
        r#"<?php
eval('class EvalPrivateStaticAccessBox {
    private static int $secret = 4;
}
echo EvalPrivateStaticAccessBox::$secret;');
"#,
    );
    assert!(
        err.contains("Fatal error: eval() runtime failed"),
        "stderr did not contain eval runtime fatal diagnostic: {err}"
    );
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

/// Verifies eval-declared classes support public properties, constructors, and methods.
#[test]
fn test_eval_declared_class_constructs_object_with_method() {
    let out = compile_and_run(
        r#"<?php
eval('class DynEvalSupported {
    public int $x = 1;
    public function __construct($x) { $this->x = $x; }
    public function bump($n) { $this->x = $this->x + $n; return $this->x; }
}');
echo eval('$box = new DynEvalSupported(5);
echo get_class($box) . ":";
echo $box->bump(4) . ":";
echo is_a($box, "DynEvalSupported") ? "Y" : "N";
$call = [$box, "bump"];
echo call_user_func($call, 1) . ":";
echo call_user_func_array($call, [2]) . ":";
return $box->x;');
"#,
    );
    assert_eq!(out, "DynEvalSupported:9:Y10:12:12");
}

/// Verifies eval-declared by-reference promoted properties remain aliased after construction.
#[test]
fn test_eval_declared_class_aliases_by_reference_promoted_property() {
    let out = compile_and_run(
        r#"<?php
eval('class DynEvalPromotedRefSupported {
    public function __construct(public &$value) {}
}');
echo eval('$value = 1;
$box = new DynEvalPromotedRefSupported($value);
$box->value = 5;
echo $value . ":";
$value = 7;
return $box->value;');
"#,
    );
    assert_eq!(out, "5:7");
}

/// Verifies eval `class_alias()` supports class-like interface, trait, enum, and class targets.
#[test]
fn test_eval_class_alias_supports_class_like_targets() {
    let out = compile_and_run(
        r#"<?php
echo eval('interface EvalAliasIface {}
trait EvalAliasTrait {}
enum EvalAliasEnum: string { case Ready = "ready"; }
class EvalAliasClass {}
echo class_alias("EvalAliasIface", "EvalAliasIfaceCopy") ? "I" : "i"; echo ":";
echo interface_exists("EvalAliasIfaceCopy") ? "IE" : "ie"; echo ":";
echo class_exists("EvalAliasIfaceCopy") ? "bad" : "IC"; echo ":";
echo is_a("EvalAliasIfaceCopy", "EvalAliasIface", true) ? "II" : "ii"; echo ":";
echo (new ReflectionClass("EvalAliasIfaceCopy"))->isInterface() ? "IR" : "ir"; echo ":";
echo class_alias("EvalAliasTrait", "EvalAliasTraitCopy") ? "T" : "t"; echo ":";
echo trait_exists("EvalAliasTraitCopy") ? "TE" : "te"; echo ":";
echo class_exists("EvalAliasTraitCopy") ? "bad" : "TC"; echo ":";
echo is_a("EvalAliasTraitCopy", "EvalAliasTrait", true) ? "TI" : "ti"; echo ":";
echo class_alias("EvalAliasEnum", "EvalAliasEnumCopy") ? "E" : "e"; echo ":";
echo enum_exists("EvalAliasEnumCopy") ? "EE" : "ee"; echo ":";
echo class_exists("EvalAliasEnumCopy") ? "EC" : "bad"; echo ":";
echo (new ReflectionClass("EvalAliasEnumCopy"))->getName(); echo ":";
echo EvalAliasEnumCopy::Ready->value; echo ":";
echo class_alias("EvalAliasClass", "EvalAliasClassCopy") ? "C" : "c"; echo ":";
echo class_exists("EvalAliasClassCopy") ? "CE" : "ce"; echo ":";
echo count(get_declared_classes()); echo ":";
echo count(get_declared_interfaces()); echo ":";
return count(get_declared_traits());');
"#,
    );
    assert_eq!(
        out,
        "I:IE:IC:II:IR:T:TE:TC:TI:E:EE:EC:EvalAliasEnum:ready:C:CE:2:1:1"
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

/// Verifies eval object construction fills registered AOT constructor defaults.
#[test]
fn test_eval_dynamic_new_runs_constructor_with_default_arg() {
    let out = compile_and_run(
        r#"<?php
class EvalDynamicNewDefaultCtor {
    public string $label = "";
    public function __construct(string $left, string $right = "B") {
        $this->label = $left . $right;
    }
}
echo eval('$first = new EvalDynamicNewDefaultCtor("A");
echo $first->label . ":";
$second = new EvalDynamicNewDefaultCtor(right: "Y", left: "X");
return $second->label;');
"#,
    );
    assert_eq!(out, "AB:XY");
}

/// Verifies eval object construction passes more than two arguments to an AOT constructor.
#[test]
fn test_eval_dynamic_new_runs_constructor_with_many_args() {
    let out = compile_and_run(
        r#"<?php
class EvalDynamicNewManyArgCtor {
    public string $label = "";
    public function __construct(int $a, int $b, int $c, string $suffix) {
        $this->label = ($a + $b + $c) . $suffix;
    }
}
echo eval('$box = new EvalDynamicNewManyArgCtor(1, 2, 3, "!"); return $box->label;');
"#,
    );
    assert_eq!(out, "6!");
}

/// Verifies inherited AOT methods returning eval results keep the boxed Mixed return ABI.
#[test]
fn test_eval_fragment_in_inherited_aot_method_returns_late_static_scope() {
    let out = compile_and_run(
        r#"<?php
class EvalInheritedAotScopeReturnBase {
    public function run() {
        return eval('return static::class;');
    }
}
class EvalInheritedAotScopeReturnChild extends EvalInheritedAotScopeReturnBase {}
echo (new EvalInheritedAotScopeReturnChild())->run();
"#,
    );
    assert_eq!(out, "EvalInheritedAotScopeReturnChild");
}

/// Verifies eval ReflectionClass::newInstanceArgs forwards named args to AOT constructors.
#[test]
fn test_eval_reflection_class_new_instance_args_constructs_aot_class() {
    let out = compile_and_run(
        r#"<?php
class EvalReflectNewArgsAotTarget {
    public string $label = "";
    public function __construct(string $left, string $right = "B") {
        $this->label = $left . $right;
    }
}
echo eval('$ref = new ReflectionClass("EvalReflectNewArgsAotTarget");
$first = $ref->newInstanceArgs(["right" => "Y", "left" => "X"]);
echo $first->label . ":";
$second = $ref->newInstanceArgs(["Q", "R"]);
return $second->label;');
"#,
    );
    assert_eq!(out, "XY:QR");
}

/// Verifies eval object construction passes AOT constructor arguments on the caller stack.
#[test]
fn test_eval_dynamic_new_runs_constructor_with_stack_string_arg() {
    let out = compile_and_run(
        r#"<?php
class EvalDynamicNewStackStringCtor {
    public string $label = "";
    public function __construct(string $a, string $b, string $c, string $d) {
        $this->label = $a . $b . $c . $d;
    }
}
echo eval('$box = new EvalDynamicNewStackStringCtor("Q", "R", "S", "T"); return $box->label;');
"#,
    );
    assert_eq!(out, "QRST");
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
    let err =
        compile_and_run_expect_failure("<?php eval('require \"missing-eval-require.php\";');");
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

/// Verifies eval-internal catch type narrowing uses the thrown object's class.
#[test]
fn test_eval_try_catch_matches_specific_exception_inside_eval() {
    let out = compile_and_run(
        r#"<?php
echo eval('try {
    throw new Exception("eval boom");
} catch (RuntimeException $wrong) {
    return "bad";
} catch (Exception $caught) {
    return is_a($caught, "Exception") ? "caught" : "bad-type";
}
return "miss";');
"#,
    );
    assert_eq!(out, "caught");
}

/// Verifies eval-internal union catch clauses match any listed class.
#[test]
fn test_eval_try_catch_matches_union_type_inside_eval() {
    let out = compile_and_run(
        r#"<?php
echo eval('try {
    throw new RuntimeException("eval boom");
} catch (LogicException|RuntimeException $caught) {
    return is_a($caught, "RuntimeException") ? "union" : "bad-type";
} catch (Exception $fallback) {
    return "fallback";
}
return "miss";');
"#,
    );
    assert_eq!(out, "union");
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
