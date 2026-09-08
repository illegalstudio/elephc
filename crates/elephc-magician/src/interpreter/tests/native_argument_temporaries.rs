//! Purpose:
//! Verifies ownership of argument arrays and evaluated scalar argument temporaries.
//!
//! Called from:
//! - Magician's interpreter unit-test harness.
//!
//! Key details:
//! - Index cells are fresh owners; argument cells remain borrowed by the builder.

use super::super::*;
use super::support::*;

/// Callback cleanup releases acquired receivers, leaves borrows intact and transfers returned owners.
#[test]
fn callable_receiver_temporaries_cleanup_respects_ownership_and_result_aliases() {
    for owns_receiver in [false, true] {
        for returns_receiver in [false, true] {
            let mut values = FakeOps::default();
            let receiver = values.new_object("KnownClass").unwrap();
            let result = if returns_receiver { receiver } else { values.int(7).unwrap() };
            let callback = EvaluatedCallable::ObjectMethod {
                object: receiver,
                owns_receiver,
                method: "answer".into(),
                called_class: None,
                native_class: None,
                bridge_scope: None,
            };
            assert_eq!(finish_evaluated_callable(callback, Ok(result), &ElephcEvalContext::new(), &mut values).unwrap(), result);
            assert_eq!(values.releases.iter().filter(|handle| **handle == receiver).count(),
                usize::from(owns_receiver && !returns_receiver));
        }
    }
}

/// A callback throwing its receiver transfers the normalization owner to the pending exception.
#[test]
fn callable_receiver_temporaries_transfer_to_pending_throwable() {
    let mut context = ElephcEvalContext::new();
    let mut values = FakeOps::default();
    let receiver = values.new_object("Exception").unwrap();
    context.set_pending_throw(receiver);
    let callback = EvaluatedCallable::ObjectMethod {
        object: receiver,
        owns_receiver: true,
        method: "raise".into(),
        called_class: None,
        native_class: None,
        bridge_scope: None,
    };
    assert_eq!(finish_evaluated_callable(callback, Err(EvalStatus::UncaughtThrowable),
        &context, &mut values), Err(EvalStatus::UncaughtThrowable));
    assert_eq!(context.take_pending_throw(), Some(receiver));
    assert!(!values.releases.contains(&receiver));
}

/// Callback normalization copies method names to Rust without retaining their fetched cells.
#[test]
fn callable_name_temporaries_release_method_cell() {
    let context = ElephcEvalContext::new();
    let mut values = FakeOps::default();
    let receiver = values.new_object("KnownClass").unwrap();
    let method = values.string("answer").unwrap();
    let callback = values.argument_array(&[receiver, method]).unwrap();
    let normalized = eval_array_callable(callback, &context, None, &mut values).unwrap();
    assert!(matches!(normalized, EvaluatedCallable::ObjectMethod { object, method, .. }
        if object == receiver && method == "answer"));
    assert_eq!(values.releases.iter().filter(|handle| **handle == method).count(), 1);
    assert!(!values.releases.contains(&receiver));
    assert!(!values.releases.contains(&callback));
}

/// Callback validation errors release the fetched receiver without consuming the caller's array.
#[test]
fn callable_receiver_temporaries_release_after_validation_error() {
    let mut context = ElephcEvalContext::new();
    let mut values = FakeOps::default();
    let receiver = values.new_object("MissingCallbackClass").unwrap();
    let method = values.string("missingMethod").unwrap();
    let callback = values.argument_array(&[receiver, method]).unwrap();
    let result = eval_call_user_func_with_values_from_scope(
        vec![callback], None, &mut context, &mut values,
    );
    assert!(result.is_err(), "unknown method must reject the callback");
    assert_eq!(values.releases.iter().filter(|handle| **handle == receiver).count(), 1,
        "validation failure must release the receiver fetched during normalization");
    assert!(!values.releases.contains(&callback), "caller still owns the callback array");
}

/// A non-invoking callable probe releases the receiver reference acquired by array normalization.
#[test]
fn callable_receiver_temporaries_release_after_probe() {
    let context = ElephcEvalContext::new();
    let mut values = FakeOps::default();
    let receiver = values.new_object("MissingCallbackClass").unwrap();
    let method = values.string("missingMethod").unwrap();
    let callback = values.argument_array(&[receiver, method]).unwrap();
    assert!(!eval_is_callable_value(callback, None, &context, &mut values).unwrap());
    assert_eq!(values.releases.iter().filter(|handle| **handle == receiver).count(), 1,
        "is_callable must not retain the receiver after probing");
    assert!(!values.releases.contains(&callback), "caller still owns the callback array");
}

/// Static callable normalization releases both fetched name cells after decoding their Rust strings.
#[test]
fn callable_name_temporaries_release_static_receiver_cell() {
    let context = ElephcEvalContext::new();
    let mut values = FakeOps::default();
    let receiver = values.string("KnownClass").unwrap();
    let method = values.string("answer").unwrap();
    let callback = values.argument_array(&[receiver, method]).unwrap();
    let normalized = eval_array_callable(callback, &context, None, &mut values).unwrap();
    assert!(matches!(normalized, EvaluatedCallable::StaticMethod { class_name, method, .. }
        if class_name == "KnownClass" && method == "answer"));
    assert_eq!(values.releases.iter().filter(|handle| **handle == method).count(), 1);
    assert_eq!(values.releases.iter().filter(|handle| **handle == receiver).count(), 1);
}

/// Native argument array construction releases indices without releasing borrowed arguments.
#[test]
fn native_argument_temporaries_release_indices() {
    let mut values = FakeOps::default();
    let first = values.int(17).unwrap();
    let second = values.int(23).unwrap();
    let bound = BoundNativeFunctionArgs { values: vec![first, second], ref_slots: vec![] };
    let array = build_native_function_arg_array(&bound, &mut values).unwrap();
    assert_eq!(values.releases.len(), 2);
    assert_eq!(values.get(values.releases[0]), FakeValue::Int(0));
    assert_eq!(values.get(values.releases[1]), FakeValue::Int(1));
    assert!(!values.releases.contains(&first));
    assert!(!values.releases.contains(&second));
    assert!(!values.releases.contains(&array));
}

/// Failed insertion releases the index and partial array, preserving the argument owner.
#[test]
fn native_argument_temporaries_release_partial_array_on_error() {
    let mut values = FakeOps::default();
    let argument = values.int(17).unwrap();
    values.fail_array_set_call(0);
    let bound = BoundNativeFunctionArgs { values: vec![argument], ref_slots: vec![] };
    assert!(build_native_function_arg_array(&bound, &mut values).is_err());
    assert_eq!(values.releases.len(), 2);
    assert_eq!(values.get(values.releases[0]), FakeValue::Int(0));
    assert!(!values.releases.contains(&argument));
}

/// Source owners survive invocation and are released once, independently of forwarded argument clones.
#[test]
fn method_argument_temporaries_preserve_borrowed_named_values() {
    let mut context = ElephcEvalContext::new();
    let mut scope = ElephcEvalScope::new();
    let mut values = FakeOps::default();
    let borrowed = values.int(23).unwrap();
    scope.set("borrowed", borrowed, ScopeCellOwnership::Owned);
    let args = [
        EvalCallArg::named("first", EvalExpr::Const(EvalConst::Int(17))),
        EvalCallArg::named("second", EvalExpr::LoadVar("borrowed".into())),
    ];
    let result = eval_with_method_call_args(&args, &mut context, &mut scope, &mut values,
        |args, _, _, values| {
            assert_eq!(args[0].name.as_deref(), Some("first"));
            assert_eq!(args[1].name.as_deref(), Some("second"));
            assert!(values.releases.is_empty());
            let forwarded = args.clone();
            assert_eq!(forwarded[1].value, borrowed);
            values.int(9)
        }).unwrap();
    assert_eq!(values.get(result), FakeValue::Int(9));
    assert_eq!(values.releases.len(), 1);
    assert_eq!(values.get(values.releases[0]), FakeValue::Int(17));
    assert!(!values.releases.contains(&borrowed));
}

/// A return alias receives the argument's source owner instead of being freed at the call boundary.
#[test]
fn method_argument_temporaries_transfer_return_alias() {
    let mut context = ElephcEvalContext::new();
    let mut scope = ElephcEvalScope::new();
    let mut values = FakeOps::default();
    let args = [EvalCallArg::positional(EvalExpr::Const(EvalConst::Int(17)))];
    let result = eval_with_method_call_args(&args, &mut context, &mut scope, &mut values,
        |args, _, _, _| Ok(args[0].value)).unwrap();
    assert_eq!(values.get(result), FakeValue::Int(17));
    assert!(values.releases.is_empty());
}

/// Evaluation failure cleans earlier argument owners before invocation can run.
#[test]
fn method_argument_temporaries_cleanup_evaluation_error() {
    let mut context = ElephcEvalContext::new();
    let mut scope = ElephcEvalScope::new();
    let mut values = FakeOps::default();
    let args = [
        EvalCallArg::positional(EvalExpr::Const(EvalConst::Int(999))),
        EvalCallArg::positional(EvalExpr::Call { name: "missing_function".into(), args: vec![] }),
    ];
    assert!(eval_with_method_call_args(&args, &mut context, &mut scope, &mut values,
        |_, _, _, _| panic!("must not invoke after argument evaluation fails")).is_err());
    assert_eq!(values.releases.iter()
        .filter(|handle| values.get(**handle) == FakeValue::Int(999)).count(), 1);
}

/// Invocation failure releases every scalar argument owner while preserving the original error.
#[test]
fn method_argument_temporaries_cleanup_invocation_error() {
    let mut context = ElephcEvalContext::new();
    let mut scope = ElephcEvalScope::new();
    let mut values = FakeOps::default();
    let args = [
        EvalCallArg::positional(EvalExpr::Const(EvalConst::Int(1))),
        EvalCallArg::positional(EvalExpr::Const(EvalConst::Int(2))),
    ];
    let result = eval_with_method_call_args(&args, &mut context, &mut scope, &mut values,
        |_, _, _, _| Err(EvalStatus::RuntimeFatal));
    assert_eq!(result, Err(EvalStatus::RuntimeFatal));
    assert_eq!(values.releases.len(), 2);
}
