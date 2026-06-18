//! Purpose:
//! Interpreter tests for native function dispatch, scope array mutation, ownership, break, and continue.
//!
//! Called from:
//! - `cargo test -p elephc-eval` through Rust's test harness.
//!
//! Key details:
//! - These cases cover integration edges between scope cells and fake runtime hooks.

use super::super::*;
use super::support::*;

/// Verifies eval fragments can dispatch registered native AOT functions.
#[test]
fn execute_program_calls_registered_native_function() {
    let program = parse_fragment(br#"return native_answer();"#).expect("parse eval fragment");
    let mut context = ElephcEvalContext::new();
    let mut scope = ElephcEvalScope::new();
    let mut values = FakeOps::default();
    let expected = values.int(42).expect("allocate fake result");
    let native = NativeFunction::new(expected.as_ptr().cast(), fake_native_return_descriptor, 0);
    assert!(context
        .define_native_function("native_answer", native)
        .is_ok());

    let result = execute_program_with_context(&mut context, &program, &mut scope, &mut values)
        .expect("execute eval ir");

    assert_eq!(result, expected);
}
/// Verifies direct eval calls can bind registered native parameters by name.
#[test]
fn execute_program_calls_registered_native_function_with_named_args() {
    let program = parse_fragment(br#"return native_answer(right: 2, left: 1);"#)
        .expect("parse eval fragment");
    let mut context = ElephcEvalContext::new();
    let mut scope = ElephcEvalScope::new();
    let mut values = FakeOps::default();
    let expected = values.int(42).expect("allocate fake result");
    let mut native =
        NativeFunction::new(expected.as_ptr().cast(), fake_native_return_descriptor, 2);
    assert!(native.set_param_name(0, "left"));
    assert!(native.set_param_name(1, "right"));
    assert!(context
        .define_native_function("native_answer", native)
        .is_ok());

    let result = execute_program_with_context(&mut context, &program, &mut scope, &mut values)
        .expect("execute eval ir");

    assert_eq!(result, expected);
}
/// Verifies direct eval calls can unpack arrays into registered native parameters.
#[test]
fn execute_program_calls_registered_native_function_with_spread_args() {
    let program =
        parse_fragment(br#"return native_answer(...[1, 2]);"#).expect("parse eval fragment");
    let mut context = ElephcEvalContext::new();
    let mut scope = ElephcEvalScope::new();
    let mut values = FakeOps::default();
    let expected = values.int(42).expect("allocate fake result");
    let mut native =
        NativeFunction::new(expected.as_ptr().cast(), fake_native_return_descriptor, 2);
    assert!(native.set_param_name(0, "left"));
    assert!(native.set_param_name(1, "right"));
    assert!(context
        .define_native_function("native_answer", native)
        .is_ok());

    let result = execute_program_with_context(&mut context, &program, &mut scope, &mut values)
        .expect("execute eval ir");

    assert_eq!(result, expected);
}
/// Verifies indexed array writes mutate an existing scope array.
#[test]
fn execute_program_writes_indexed_scope_array() {
    let program = parse_fragment(br#"$items = ["a"]; $items[1] = "b"; return $items[1];"#)
        .expect("parse eval fragment");
    let mut scope = ElephcEvalScope::new();
    let mut values = FakeOps::default();

    let result = execute_program(&program, &mut scope, &mut values).expect("execute eval ir");

    assert_eq!(values.get(result), FakeValue::String("b".to_string()));
}
/// Verifies indexed array append writes use the next visible index.
#[test]
fn execute_program_appends_indexed_scope_array() {
    let program = parse_fragment(br#"$items = ["a"]; $items[] = "b"; return $items[1];"#)
        .expect("parse eval fragment");
    let mut scope = ElephcEvalScope::new();
    let mut values = FakeOps::default();

    let result = execute_program(&program, &mut scope, &mut values).expect("execute eval ir");

    assert_eq!(values.get(result), FakeValue::String("b".to_string()));
}
/// Verifies associative append starts at key zero when only string keys exist.
#[test]
fn execute_program_appends_assoc_scope_array_with_string_keys() {
    let program =
        parse_fragment(br#"$items = ["name" => "Ada"]; $items[] = "Grace"; return $items[0];"#)
            .expect("parse eval fragment");
    let mut scope = ElephcEvalScope::new();
    let mut values = FakeOps::default();

    let result = execute_program(&program, &mut scope, &mut values).expect("execute eval ir");

    assert_eq!(values.get(result), FakeValue::String("Grace".to_string()));
}
/// Verifies associative append uses one plus the largest existing integer key.
#[test]
fn execute_program_appends_assoc_scope_array_after_positive_int_key() {
    let program = parse_fragment(
        br#"$items = [2 => "two", "name" => "Ada"]; $items[] = "tail"; return $items[3];"#,
    )
    .expect("parse eval fragment");
    let mut scope = ElephcEvalScope::new();
    let mut values = FakeOps::default();

    let result = execute_program(&program, &mut scope, &mut values).expect("execute eval ir");

    assert_eq!(values.get(result), FakeValue::String("tail".to_string()));
}
/// Verifies associative append preserves PHP's largest-negative-key behavior.
#[test]
fn execute_program_appends_assoc_scope_array_after_negative_int_key() {
    let program =
        parse_fragment(br#"$items = [-2 => "minus"]; $items[] = "tail"; return $items[-1];"#)
            .expect("parse eval fragment");
    let mut scope = ElephcEvalScope::new();
    let mut values = FakeOps::default();

    let result = execute_program(&program, &mut scope, &mut values).expect("execute eval ir");

    assert_eq!(values.get(result), FakeValue::String("tail".to_string()));
}
/// Verifies mutating a borrowed scope array does not make the eval scope own it.
#[test]
fn execute_program_preserves_borrowed_array_ownership() {
    let program = parse_fragment(br#"$items[0] = "b";"#).expect("parse eval fragment");
    let mut scope = ElephcEvalScope::new();
    let mut values = FakeOps::default();
    let array = values.array_new(1).expect("create fake array");
    scope.set("items", array, ScopeCellOwnership::Borrowed);

    let _ = execute_program(&program, &mut scope, &mut values).expect("execute eval ir");
    let entry = scope.entry("items").expect("scope should contain items");

    assert_eq!(entry.cell(), array);
    assert_eq!(entry.flags().ownership, ScopeCellOwnership::Borrowed);
    assert!(values.releases.is_empty());
}
/// Verifies replacing an eval-owned scope value releases the old cell.
#[test]
fn execute_program_releases_replaced_scope_value() {
    let program = parse_fragment(br#"$x = "old"; $x = "new";"#).expect("parse eval fragment");
    let mut scope = ElephcEvalScope::new();
    let mut values = FakeOps::default();

    let _ = execute_program(&program, &mut scope, &mut values).expect("execute eval ir");

    assert_eq!(values.releases.len(), 1);
    assert_eq!(
        values.get(values.releases[0]),
        FakeValue::String("old".to_string())
    );
}
/// Verifies unsetting an eval-owned scope value releases the old cell.
#[test]
fn execute_program_releases_unset_scope_value() {
    let program = parse_fragment(br#"$x = "old"; unset($x);"#).expect("parse eval fragment");
    let mut scope = ElephcEvalScope::new();
    let mut values = FakeOps::default();

    let _ = execute_program(&program, &mut scope, &mut values).expect("execute eval ir");

    assert_eq!(values.releases.len(), 1);
    assert_eq!(
        values.get(values.releases[0]),
        FakeValue::String("old".to_string())
    );
}
/// Verifies break exits a runtime eval loop before later statements run.
#[test]
fn execute_program_break_exits_loop() {
    let program = parse_fragment(br#"while ($flag) { echo "a"; break; echo "b"; }"#)
        .expect("parse eval fragment");
    let mut scope = ElephcEvalScope::new();
    let mut values = FakeOps::default();
    let flag = values.bool_value(true).expect("create fake bool");
    scope.set("flag", flag, ScopeCellOwnership::Owned);

    let _ = execute_program(&program, &mut scope, &mut values).expect("execute eval ir");

    assert_eq!(values.output, "a");
}
/// Verifies continue restarts a runtime eval loop and observes later scope updates.
#[test]
fn execute_program_continue_restarts_loop() {
    let program = parse_fragment(
        br#"while ($flag) { $flag = false; continue; echo "unreachable"; } echo "done";"#,
    )
    .expect("parse eval fragment");
    let mut scope = ElephcEvalScope::new();
    let mut values = FakeOps::default();
    let flag = values.bool_value(true).expect("create fake bool");
    scope.set("flag", flag, ScopeCellOwnership::Owned);

    let _ = execute_program(&program, &mut scope, &mut values).expect("execute eval ir");

    assert_eq!(values.output, "done");
}
