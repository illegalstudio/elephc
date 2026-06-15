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

/// Verifies the linked eval bridge can execute scalar echo fragments.
#[test]
fn test_eval_scalar_echo_executes_through_bridge() {
    let out = compile_and_run("<?php eval('echo \"x\";');");
    assert_eq!(out, "x");
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

/// Verifies eval indexed-array literals and reads execute through Mixed array helpers.
#[test]
fn test_eval_indexed_array_literal_and_read() {
    let out = compile_and_run("<?php echo eval('return [1, 2, 3][1];');");
    assert_eq!(out, "2");
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
eval('echo STRLEN("abcd") . ":" . count([1, 2, 3]);');
"#,
    );
    assert_eq!(out, "4:3");
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
