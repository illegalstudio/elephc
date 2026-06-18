//! Purpose:
//! Interpreter tests for branching, loops, comparisons, match, logical operators, and foreach.
//!
//! Called from:
//! - `cargo test -p elephc-eval` through Rust's test harness.
//!
//! Key details:
//! - These cases exercise control-flow outcomes without leaving the fake runtime.

use super::super::*;
use super::support::*;

/// Verifies if/else executes only the PHP-truthy branch.
#[test]
fn execute_program_if_else_uses_php_truthiness() {
    let program = parse_fragment(br#"if ($flag) { $x = "then"; } else { $x = "else"; }"#)
        .expect("parse eval fragment");
    let mut scope = ElephcEvalScope::new();
    let mut values = FakeOps::default();
    let flag = values.int(0).expect("create fake int");
    scope.set("flag", flag, ScopeCellOwnership::Owned);

    let _ = execute_program(&program, &mut scope, &mut values).expect("execute eval ir");
    let x = scope.visible_cell("x").expect("scope should contain x");

    assert_eq!(values.get(x), FakeValue::String("else".to_string()));
}
/// Verifies elseif chains execute the first truthy branch and skip later branches.
#[test]
fn execute_program_elseif_uses_first_truthy_branch() {
    let program =
        parse_fragment(br#"if ($a) { $x = "a"; } elseif ($b) { $x = "b"; } else { $x = "c"; }"#)
            .expect("parse eval fragment");
    let mut scope = ElephcEvalScope::new();
    let mut values = FakeOps::default();
    let a = values.bool_value(false).expect("create fake bool");
    let b = values.bool_value(true).expect("create fake bool");
    scope.set("a", a, ScopeCellOwnership::Owned);
    scope.set("b", b, ScopeCellOwnership::Owned);

    let _ = execute_program(&program, &mut scope, &mut values).expect("execute eval ir");
    let x = scope.visible_cell("x").expect("scope should contain x");

    assert_eq!(values.get(x), FakeValue::String("b".to_string()));
}
/// Verifies while repeats while the condition remains truthy and propagates writes.
#[test]
fn execute_program_while_uses_php_truthiness() {
    let program = parse_fragment(br#"while ($flag) { echo $flag; $flag = false; }"#)
        .expect("parse eval fragment");
    let mut scope = ElephcEvalScope::new();
    let mut values = FakeOps::default();
    let flag = values.int(2).expect("create fake int");
    scope.set("flag", flag, ScopeCellOwnership::Owned);

    let _ = execute_program(&program, &mut scope, &mut values).expect("execute eval ir");
    let flag = scope
        .visible_cell("flag")
        .expect("scope should contain flag");

    assert_eq!(values.output, "2");
    assert_eq!(values.get(flag), FakeValue::Bool(false));
}
/// Verifies do/while runs the body before testing the condition.
#[test]
fn execute_program_do_while_runs_body_before_condition() {
    let program = parse_fragment(br#"do { echo $i; $i = $i + 1; } while (false);"#)
        .expect("parse eval fragment");
    let mut scope = ElephcEvalScope::new();
    let mut values = FakeOps::default();
    let i = values.int(0).expect("create fake int");
    scope.set("i", i, ScopeCellOwnership::Owned);

    let _ = execute_program(&program, &mut scope, &mut values).expect("execute eval ir");
    let i = scope.visible_cell("i").expect("scope should contain i");

    assert_eq!(values.output, "0");
    assert_eq!(values.get(i), FakeValue::Int(1));
}
/// Verifies switch uses loose matching and falls through after the matching case.
#[test]
fn execute_program_switch_matches_and_falls_through() {
    let program =
            parse_fragment(br#"switch ($x) { case 1: echo "one"; break; case 2: echo "two"; default: echo "default"; }"#)
                .expect("parse eval fragment");
    let mut scope = ElephcEvalScope::new();
    let mut values = FakeOps::default();
    let x = values.int(2).expect("create fake int");
    scope.set("x", x, ScopeCellOwnership::Owned);

    let _ = execute_program(&program, &mut scope, &mut values).expect("execute eval ir");

    assert_eq!(values.output, "twodefault");
}
/// Verifies for loops run init, condition, update, and body in PHP order.
#[test]
fn execute_program_for_loop_updates_after_body() {
    let program = parse_fragment(br#"for ($i = 3; $i; $i = $i - 1) { echo $i; }"#)
        .expect("parse eval fragment");
    let mut scope = ElephcEvalScope::new();
    let mut values = FakeOps::default();

    let _ = execute_program(&program, &mut scope, &mut values).expect("execute eval ir");
    let i = scope.visible_cell("i").expect("scope should contain i");

    assert_eq!(values.output, "321");
    assert_eq!(values.get(i), FakeValue::Int(0));
}
/// Verifies `continue` in a for loop still runs the update clause.
#[test]
fn execute_program_for_continue_runs_update_clause() {
    let program = parse_fragment(
        br#"for ($i = 3; $i; $i = $i - 1) { if ($i - 1) { continue; } echo "done"; }"#,
    )
    .expect("parse eval fragment");
    let mut scope = ElephcEvalScope::new();
    let mut values = FakeOps::default();

    let _ = execute_program(&program, &mut scope, &mut values).expect("execute eval ir");
    let i = scope.visible_cell("i").expect("scope should contain i");

    assert_eq!(values.output, "done");
    assert_eq!(values.get(i), FakeValue::Int(0));
}
/// Verifies comparison operators return boolean cells usable by echo and branches.
#[test]
fn execute_program_comparisons_return_bool_cells() {
    let program = parse_fragment(
            br#"echo 2 < 3; echo 3 <= 3; echo 4 > 3; echo 4 >= 4; if ("10" == 10) { echo "n"; } if ("a" != "b") { echo "s"; }"#,
        )
        .expect("parse eval fragment");
    let mut scope = ElephcEvalScope::new();
    let mut values = FakeOps::default();

    let _ = execute_program(&program, &mut scope, &mut values).expect("execute eval ir");

    assert_eq!(values.output, "1111ns");
}
/// Verifies spaceship comparisons return PHP -1/0/1 integer cells.
#[test]
fn execute_program_spaceship_returns_int_cells() {
    let program =
        parse_fragment(br#"echo 1 <=> 2; echo ":"; echo 2 <=> 2; echo ":"; echo 3 <=> 2;"#)
            .expect("parse eval fragment");
    let mut scope = ElephcEvalScope::new();
    let mut values = FakeOps::default();

    let _ = execute_program(&program, &mut scope, &mut values).expect("execute eval ir");

    assert_eq!(values.output, "-1:0:1");
}
/// Verifies strict equality keeps PHP type identity distinct from loose equality.
#[test]
fn execute_program_strict_equality_uses_type_identity() {
    let program = parse_fragment(
        br#"echo "10" == 10; echo "10" === 10; echo "10" === "10"; echo "10" !== 10;"#,
    )
    .expect("parse eval fragment");
    let mut scope = ElephcEvalScope::new();
    let mut values = FakeOps::default();

    let _ = execute_program(&program, &mut scope, &mut values).expect("execute eval ir");

    assert_eq!(values.output, "111");
}
/// Verifies logical AND skips an unsupported right-hand expression after a false left side.
#[test]
fn execute_program_short_circuits_logical_and() {
    let program = parse_fragment(br#"return false && missing();"#).expect("parse eval fragment");
    let mut scope = ElephcEvalScope::new();
    let mut values = FakeOps::default();

    let result = execute_program(&program, &mut scope, &mut values).expect("execute eval ir");

    assert_eq!(values.get(result), FakeValue::Bool(false));
}
/// Verifies logical OR skips an unsupported right-hand expression after a true left side.
#[test]
fn execute_program_short_circuits_logical_or() {
    let program = parse_fragment(br#"return true || missing();"#).expect("parse eval fragment");
    let mut scope = ElephcEvalScope::new();
    let mut values = FakeOps::default();

    let result = execute_program(&program, &mut scope, &mut values).expect("execute eval ir");

    assert_eq!(values.get(result), FakeValue::Bool(true));
}
/// Verifies match expressions use strict comparison across comma-separated patterns.
#[test]
fn execute_program_match_uses_strict_pattern_comparison() {
    let program =
        parse_fragment(br#"return match ($x) { 1, "1" => "string", default => "other" };"#)
            .expect("parse eval fragment");
    let mut scope = ElephcEvalScope::new();
    let mut values = FakeOps::default();
    let x = values.string("1").expect("create fake string");
    scope.set("x", x, ScopeCellOwnership::Owned);

    let result = execute_program(&program, &mut scope, &mut values).expect("execute eval ir");

    assert_eq!(values.get(result), FakeValue::String("string".to_string()));
}
/// Verifies match expressions evaluate only the selected arm result.
#[test]
fn execute_program_match_skips_unselected_results() {
    let program = parse_fragment(
        br#"return match (2) { 1 => missing(), 2 => "two", default => missing() };"#,
    )
    .expect("parse eval fragment");
    let mut scope = ElephcEvalScope::new();
    let mut values = FakeOps::default();

    let result = execute_program(&program, &mut scope, &mut values).expect("execute eval ir");

    assert_eq!(values.get(result), FakeValue::String("two".to_string()));
}
/// Verifies match expressions without a matching arm or default fail at runtime.
#[test]
fn execute_program_match_without_default_fails_on_miss() {
    let program = parse_fragment(br#"return match (3) { 1 => "one", 2 => "two" };"#)
        .expect("parse eval fragment");
    let mut scope = ElephcEvalScope::new();
    let mut values = FakeOps::default();

    let result = execute_program(&program, &mut scope, &mut values);

    assert_eq!(result, Err(EvalStatus::RuntimeFatal));
}
/// Verifies PHP keyword logical operators use PHP precedence and short-circuiting.
#[test]
fn execute_program_evaluates_keyword_logical_operators() {
    let program =
        parse_fragment(br#"echo (false || true and false) ? "T" : "F"; return true or missing();"#)
            .expect("parse eval fragment");
    let mut scope = ElephcEvalScope::new();
    let mut values = FakeOps::default();

    let result = execute_program(&program, &mut scope, &mut values).expect("execute eval ir");

    assert_eq!(values.output, "F");
    assert_eq!(values.get(result), FakeValue::Bool(true));
}
/// Verifies PHP keyword `xor` evaluates both operands and returns a boolean cell.
#[test]
fn execute_program_evaluates_keyword_xor() {
    let program =
        parse_fragment(br#"echo (true xor false) ? "T" : "F"; echo (true xor true) ? "T" : "F";"#)
            .expect("parse eval fragment");
    let mut scope = ElephcEvalScope::new();
    let mut values = FakeOps::default();

    let _ = execute_program(&program, &mut scope, &mut values).expect("execute eval ir");

    assert_eq!(values.output, "TF");
}
/// Verifies ternary expressions evaluate only the selected branch.
#[test]
fn execute_program_ternary_short_circuits_unselected_branch() {
    let program =
        parse_fragment(br#"echo true ? "yes" : missing(); echo false ? missing() : "no";"#)
            .expect("parse eval fragment");
    let mut scope = ElephcEvalScope::new();
    let mut values = FakeOps::default();

    let _ = execute_program(&program, &mut scope, &mut values).expect("execute eval ir");

    assert_eq!(values.output, "yesno");
}
/// Verifies the short ternary form returns the condition value when it is truthy.
#[test]
fn execute_program_short_ternary_reuses_truthy_condition() {
    let program = parse_fragment(br#"echo "x" ?: "fallback"; echo false ?: "fallback";"#)
        .expect("parse eval fragment");
    let mut scope = ElephcEvalScope::new();
    let mut values = FakeOps::default();

    let _ = execute_program(&program, &mut scope, &mut values).expect("execute eval ir");

    assert_eq!(values.output, "xfallback");
}
/// Verifies null coalescing uses the default for missing or null values.
#[test]
fn execute_program_null_coalesce_uses_default_for_missing_or_null() {
    let program = parse_fragment(br#"echo $missing ?? "fallback"; echo $x ?? "null-fallback";"#)
        .expect("parse eval fragment");
    let mut scope = ElephcEvalScope::new();
    let mut values = FakeOps::default();
    let x = values.null().expect("create fake null");
    scope.set("x", x, ScopeCellOwnership::Owned);

    let _ = execute_program(&program, &mut scope, &mut values).expect("execute eval ir");

    assert_eq!(values.output, "fallbacknull-fallback");
}
/// Verifies null coalescing skips the default expression for non-null values.
#[test]
fn execute_program_null_coalesce_short_circuits_non_null_value() {
    let program = parse_fragment(br#"echo "set" ?? missing();"#).expect("parse eval fragment");
    let mut scope = ElephcEvalScope::new();
    let mut values = FakeOps::default();

    let _ = execute_program(&program, &mut scope, &mut values).expect("execute eval ir");

    assert_eq!(values.output, "set");
}
/// Verifies logical negation returns boolean cells using PHP truthiness.
#[test]
fn execute_program_evaluates_logical_not() {
    let program = parse_fragment(br#"echo !false; echo !"x";"#).expect("parse eval fragment");
    let mut scope = ElephcEvalScope::new();
    let mut values = FakeOps::default();

    let _ = execute_program(&program, &mut scope, &mut values).expect("execute eval ir");

    assert_eq!(values.output, "1");
}
/// Verifies unary numeric operators delegate to PHP numeric runtime operations.
#[test]
fn execute_program_evaluates_unary_numeric_ops() {
    let program = parse_fragment(br#"return -$x + +2;"#).expect("parse eval fragment");
    let mut scope = ElephcEvalScope::new();
    let mut values = FakeOps::default();
    let x = values.int(5).expect("create fake int");
    scope.set("x", x, ScopeCellOwnership::Owned);

    let result = execute_program(&program, &mut scope, &mut values).expect("execute eval ir");

    assert_eq!(values.get(result), FakeValue::Int(-3));
}
/// Verifies foreach assigns each indexed element to the value variable.
#[test]
fn execute_program_foreach_iterates_indexed_values() {
    let program = parse_fragment(br#"foreach (["a", "b"] as $item) { echo $item; }"#)
        .expect("parse eval fragment");
    let mut scope = ElephcEvalScope::new();
    let mut values = FakeOps::default();

    let _ = execute_program(&program, &mut scope, &mut values).expect("execute eval ir");
    let item = scope
        .visible_cell("item")
        .expect("scope should contain last foreach item");

    assert_eq!(values.output, "ab");
    assert_eq!(values.get(item), FakeValue::String("b".to_string()));
}
/// Verifies foreach key-value targets receive indexed integer keys and values.
#[test]
fn execute_program_foreach_assigns_indexed_keys() {
    let program =
        parse_fragment(br#"foreach (["a", "b"] as $key => $item) { echo $key . $item; }"#)
            .expect("parse eval fragment");
    let mut scope = ElephcEvalScope::new();
    let mut values = FakeOps::default();

    let _ = execute_program(&program, &mut scope, &mut values).expect("execute eval ir");
    let key = scope.visible_cell("key").expect("scope should contain key");
    let item = scope
        .visible_cell("item")
        .expect("scope should contain last foreach item");

    assert_eq!(values.output, "0a1b");
    assert_eq!(values.get(key), FakeValue::Int(1));
    assert_eq!(values.get(item), FakeValue::String("b".to_string()));
}
/// Verifies foreach over associative arrays preserves insertion-order keys and values.
#[test]
fn execute_program_foreach_iterates_assoc_keys_and_values() {
    let program = parse_fragment(
        br#"foreach (["a" => 1, "b" => 2] as $key => $item) { echo $key . ":" . $item . ";"; }"#,
    )
    .expect("parse eval fragment");
    let mut scope = ElephcEvalScope::new();
    let mut values = FakeOps::default();

    let _ = execute_program(&program, &mut scope, &mut values).expect("execute eval ir");

    assert_eq!(values.output, "a:1;b:2;");
}
/// Verifies value-only foreach over associative arrays still yields values in insertion order.
#[test]
fn execute_program_foreach_iterates_assoc_values_only() {
    let program = parse_fragment(br#"foreach (["a" => 1, "b" => 2] as $item) { echo $item; }"#)
        .expect("parse eval fragment");
    let mut scope = ElephcEvalScope::new();
    let mut values = FakeOps::default();

    let _ = execute_program(&program, &mut scope, &mut values).expect("execute eval ir");

    assert_eq!(values.output, "12");
}
/// Verifies break and continue control foreach execution inside eval.
#[test]
fn execute_program_foreach_honors_break_and_continue() {
    let program = parse_fragment(
        br#"foreach ([1, 2, 3] as $item) { if ($item == 1) { continue; } echo $item; break; }"#,
    )
    .expect("parse eval fragment");
    let mut scope = ElephcEvalScope::new();
    let mut values = FakeOps::default();

    let _ = execute_program(&program, &mut scope, &mut values).expect("execute eval ir");

    assert_eq!(values.output, "2");
}
