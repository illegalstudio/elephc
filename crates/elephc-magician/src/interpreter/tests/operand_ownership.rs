//! Purpose:
//! Verifies normal-call owner ledgers preserve values, references, and array side metadata.
//!
//! Called from:
//! - The Magician interpreter unit-test harness.
//!
//! Key details:
//! - Named and spread arguments retain caller reference targets through dispatch.
//! - Native by-reference entry coercions reach caller storage before invocation.
//! - Borrowed array metadata survives successful, identity, and failing consumers.

use super::super::*;
use super::support::*;

/// Reflection storage retains a borrowed argument and balances replacements and identical writes.
#[test]
fn static_property_storage_owns_reflection_arguments() {
    let mut values = FakeOps::default();
    let mut context = ElephcEvalContext::new();
    let first = values.string("first").unwrap();
    store_borrowed_static_property(
        "Stored",
        "value",
        first.borrowed(),
        &mut context,
        &mut values,
    )
    .unwrap();
    values.release(first).unwrap();
    assert_eq!(context.static_property("Stored", "value"), Some(first));
    assert_eq!(values.cell_owners[&(first.as_ptr() as usize)], 1);

    store_borrowed_static_property(
        "Stored",
        "value",
        first.borrowed(),
        &mut context,
        &mut values,
    )
    .unwrap();
    assert_eq!(values.cell_owners[&(first.as_ptr() as usize)], 1);

    let second = values.int(5).unwrap();
    store_borrowed_static_property(
        "Stored",
        "value",
        second.borrowed(),
        &mut context,
        &mut values,
    )
    .unwrap();
    values.release(second).unwrap();
    assert_eq!(values.cell_owners[&(first.as_ptr() as usize)], 0);
    assert_eq!(values.cell_owners[&(second.as_ptr() as usize)], 1);
    assert_eq!(values.retains, vec![first, second]);
}

/// An owned constant fetch detaches array storage and preserves side metadata for its copy.
#[test]
fn owned_constant_array_fetch_is_independent_from_context_storage() {
    let mut values = FakeOps::default();
    let mut context = ElephcEvalContext::new();
    let mut scope = ElephcEvalScope::new();
    let array = values.array_new(1).unwrap();
    let zero = values.int(0).unwrap();
    let original = values.int(1).unwrap();
    values.array_set(array, zero, original).unwrap();
    context.set_array_cursor(array, EvalArrayCursor::Position(0));
    assert!(context.define_constant("VALUES", array));

    let owned = eval_owned_expr(
        &EvalExpr::ConstFetch("VALUES".into()),
        &mut context,
        &mut scope,
        &mut values,
    )
    .unwrap();

    assert_ne!(owned.as_ptr(), array.as_ptr());
    assert_eq!(context.array_cursor(owned), EvalArrayCursor::Position(0));
    let replacement = values.int(2).unwrap();
    values.array_set(owned, zero, replacement).unwrap();
    let stored = values.array_get(array, zero).unwrap();
    assert_eq!(values.get(stored), FakeValue::Int(1));
    values.release(stored).unwrap();
    values.release(zero).unwrap();
    context.clear_array_metadata(owned);
    values.release(owned).unwrap();
}

/// A later argument may replace the first source variable without invalidating its captured value.
#[test]
fn call_arguments_retain_source_before_later_global_replacement() {
    let mut values = FakeOps::default();
    let mut context = ElephcEvalContext::new();
    let mut scope = ElephcEvalScope::new();
    let source = values.string("original").unwrap();
    scope.set("source", source, ScopeCellOwnership::Owned);
    context.set_global_scope(&mut scope);
    let declaration = parse_fragment(
        b"function replaceArgument() { global $source; $source = 9; return 0; }",
    )
    .unwrap();
    execute_program_outcome_with_context(&mut context, &declaration, &mut scope, &mut values)
        .unwrap();
    let args = [
        EvalCallArg::positional(EvalExpr::LoadVar("source".into())),
        EvalCallArg::positional(EvalExpr::Call {
            name: "replaceargument".into(),
            args: vec![],
        }),
    ];

    let result = with_eval_call_arguments(
        &args,
        &mut context,
        &mut scope,
        &mut values,
        |arguments, _, scope, values| {
            assert_ne!(scope.visible_cell("source"), Some(source));
            assert_eq!(values.cell_owners[&(source.as_ptr() as usize)], 1);
            assert_eq!(arguments[0].value, source);
            assert!(arguments[0].value.is_borrowed());
            Ok(arguments[0].value)
        },
    )
    .unwrap();

    assert_eq!(result, source);
    assert!(!result.is_borrowed());
    assert_eq!(values.cell_owners[&(source.as_ptr() as usize)], 1);
}

/// Eval-declared calls preserve named and unpacked by-reference writeback targets.
#[test]
fn normal_eval_calls_preserve_named_and_spread_reference_targets() {
    let program = parse_fragment(
        br#"function replace(&$value, $next) { $value = $next; }
$named = "before";
replace(value: $named, next: "named");
$spread = "before";
$arguments = [&$spread, "spread"];
replace(...$arguments);
return $named . ":" . $spread;"#,
    )
    .unwrap();
    let mut scope = ElephcEvalScope::new();
    let mut values = FakeOps::default();

    let result = execute_program(&program, &mut scope, &mut values).unwrap();

    assert_eq!(values.get(result), FakeValue::String("named:spread".into()));
}

/// Registered native calls preserve named and unpacked by-reference coercion writeback.
#[test]
fn normal_native_calls_preserve_named_and_spread_reference_targets() {
    let program = parse_fragment(
        br#"$named = "3";
native_named(left: $named, right: 2);
$spread = "4";
$arguments = [&$spread, 2];
native_spread(...$arguments);
return $named;"#,
    )
    .unwrap();
    let mut context = ElephcEvalContext::new();
    let mut scope = ElephcEvalScope::new();
    let mut values = FakeOps::default();
    for (name, expected) in [
        ("native_named", values.int(41).unwrap()),
        ("native_spread", values.int(42).unwrap()),
    ] {
        let mut native = NativeFunction::new(
            expected.as_ptr().cast(),
            fake_native_return_descriptor,
            2,
        );
        assert!(native.set_param_name(0, "left"));
        assert!(native.set_param_name(1, "right"));
        assert!(native.set_param_by_ref(0, true));
        assert!(native.set_param_type(
            0,
            EvalParameterType::new(vec![EvalParameterTypeVariant::Int], false),
        ));
        assert!(context.define_native_function(name, native).is_ok());
    }

    let result = execute_program_with_context(
        &mut context,
        &program,
        &mut scope,
        &mut values,
    )
    .unwrap();

    assert_eq!(values.get(result), FakeValue::Int(3));
    let spread = eval_owned_expr(
        &EvalExpr::LoadVar("spread".into()),
        &mut context,
        &mut scope,
        &mut values,
    )
    .unwrap();
    assert_eq!(values.get(spread), FakeValue::Int(4));
    values.release(spread).unwrap();
}

/// Nullable native by-reference coercion is visible before invocation without stale writeback.
#[test]
fn native_by_ref_coercion_precedes_invocation_and_unchanged_slots_do_not_overwrite() {
    let mut context = ElephcEvalContext::new();
    let mut scope = ElephcEvalScope::new();
    let mut values = FakeOps::default();
    let source = values.string("5").unwrap();
    scope.set("value", source, ScopeCellOwnership::Owned);
    let expected = values.int(42).unwrap();
    let mut native = NativeFunction::new(
        expected.as_ptr().cast(),
        fake_native_return_descriptor,
        1,
    );
    assert!(native.set_param_by_ref(0, true));
    assert!(native.set_param_type(
        0,
        EvalParameterType::new(vec![EvalParameterTypeVariant::Int], true),
    ));
    let target = EvalReferenceTarget::Variable {
        scope: &mut scope,
        name: "value".into(),
    };
    let bound = bind_evaluated_native_function_args(
        &native,
        vec![EvaluatedCallArg {
            name: None,
            value: source.borrowed(),
            ref_target: Some(target),
        }],
        &mut context,
        &mut values,
    )
    .unwrap();

    let coerced = scope.visible_cell("value").unwrap();
    assert_eq!(values.get(coerced), FakeValue::Int(5));

    let intervening = values.int(99).unwrap();
    if let Some(replaced) = scope.set("value", intervening, ScopeCellOwnership::Owned) {
        values.release(replaced).unwrap();
    }
    let result = eval_native_function_with_values(native, bound, &mut context, &mut values).unwrap();
    values.release(result).unwrap();

    let value = scope.visible_cell("value").unwrap();
    assert_eq!(values.get(value), FakeValue::Int(99));
}

/// Retained caller arrays keep aliases and cursors across every call completion mode.
#[test]
fn call_argument_cleanup_preserves_borrowed_array_metadata() {
    for completion in ["value", "identity", "failure"] {
        let mut context = ElephcEvalContext::new();
        let mut scope = ElephcEvalScope::new();
        let mut values = FakeOps::default();
        let array = values.array_new(1).unwrap();
        let key = EvalArrayReferenceKey::Int(0);
        let aliased = values.string("aliased").unwrap();
        scope.set("array", array, ScopeCellOwnership::Owned);
        scope.set("aliased", aliased, ScopeCellOwnership::Owned);
        context.bind_array_element_alias(
            array,
            key.clone(),
            EvalReferenceTarget::Variable {
                scope: &mut scope,
                name: "aliased".into(),
            },
        );
        context.set_array_cursor(array, EvalArrayCursor::Position(0));
        let args = [EvalCallArg::positional(EvalExpr::LoadVar("array".into()))];

        let result = with_eval_call_arguments(
            &args,
            &mut context,
            &mut scope,
            &mut values,
            |arguments, _, _, values| match completion {
                "identity" => Ok(arguments[0].value),
                "failure" => Err(EvalStatus::RuntimeFatal),
                _ => values.null(),
            },
        );

        assert_eq!(result.is_err(), completion == "failure");
        assert!(context.array_element_alias(array, &key).is_some());
        assert_eq!(context.array_cursor(array), EvalArrayCursor::Position(0));
        if let Ok(result) = result {
            values.release(result).unwrap();
        }
    }
}

/// An index side effect may replace the receiver variable without invalidating the old array.
#[test]
fn array_get_retains_receiver_before_index_side_effects() {
    let declaration = parse_fragment(
        br#"function replaceReceiver() { global $source; $source = ["new"]; return 0; }"#,
    ).unwrap();
    let mut context = ElephcEvalContext::new();
    let mut scope = ElephcEvalScope::new();
    let mut values = FakeOps::default();
    let _ = execute_program_with_context(
        &mut context,
        &declaration,
        &mut scope,
        &mut values,
    ).unwrap();
    let old_array = values.array_new(1).unwrap();
    let zero = values.int(0).unwrap();
    let old = values.string("old").unwrap();
    values.array_set(old_array, zero, old).unwrap();
    values.release(zero).unwrap();
    scope.set("source", old_array, ScopeCellOwnership::Owned);
    context.set_global_scope(&mut scope);
    let expression = EvalExpr::ArrayGet {
        array: Box::new(EvalExpr::LoadVar("source".into())),
        index: Box::new(EvalExpr::Call {
            name: "replacereceiver".into(),
            args: Vec::new(),
        }),
    };

    let result = eval_expr(
        &expression,
        &mut context,
        &mut scope,
        &mut values,
    ).unwrap();

    assert_eq!(values.get(result), FakeValue::String("old".into()));
    assert_eq!(values.retains.iter().filter(|value| {
        value.as_ptr() == old_array.as_ptr()
    }).count(), 1);
    assert_eq!(values.releases.iter().filter(|value| {
        value.as_ptr() == old_array.as_ptr()
    }).count(), 2);
    assert_eq!(values.cell_owners.get(&(old_array.as_ptr() as usize)), Some(&0));
}

/// A borrowed callback-array boundary promotes an aliased return and keeps caller metadata.
#[test]
fn borrowed_call_array_promotes_alias_return_and_preserves_source_metadata() {
    let mut context = ElephcEvalContext::new();
    let mut scope = ElephcEvalScope::new();
    let mut values = FakeOps::default();
    let callback = values.string("min").unwrap();
    let aliased = values.int(1).unwrap();
    scope.set("aliased", aliased, ScopeCellOwnership::Owned);
    context.set_global_scope(&mut scope);
    let arguments = values.array_new(2).unwrap();
    let zero = values.int(0).unwrap();
    values.array_set(arguments, zero, aliased).unwrap();
    values.release(zero).unwrap();
    let one = values.int(1).unwrap();
    let other = values.int(2).unwrap();
    values.array_set(arguments, one, other).unwrap();
    values.release(one).unwrap();
    let key = EvalArrayReferenceKey::Int(0);
    context.bind_array_element_alias(
        arguments,
        key.clone(),
        EvalReferenceTarget::Variable {
            scope: &mut scope,
            name: "aliased".into(),
        },
    );
    context.set_array_cursor(arguments, EvalArrayCursor::Position(0));

    let outcome = execute_context_callable_call_array_outcome(
        &mut context,
        callback.borrowed(),
        arguments.borrowed(),
        &mut values,
    )
    .unwrap();
    let EvalOutcome::Value(result) = outcome else {
        panic!("borrowed call-array boundary returned a Throwable");
    };

    assert_eq!(result.as_ptr(), aliased.as_ptr());
    assert!(!result.is_borrowed());
    assert!(context.array_element_alias(arguments, &key).is_some());
    assert_eq!(context.array_cursor(arguments), EvalArrayCursor::Position(0));
    assert_eq!(values.cell_owners.get(&(callback.as_ptr() as usize)), Some(&1));
    assert_eq!(values.cell_owners.get(&(arguments.as_ptr() as usize)), Some(&1));
    values.release(result).unwrap();
}
