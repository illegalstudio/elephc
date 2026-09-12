//! Purpose:
//! Verifies native function result and reference-slot cleanup after interrupted writeback.
//!
//! Called from:
//! - The Magician interpreter unit-test harness.
//!
//! Key details:
//! - Injected release errors consume their owner, matching throwing native destructors.
//! - Staged replacement cells model owners already transferred by the native callee.

use super::super::*;
use super::support::*;

/// PHP array reference declarations stage the box itself, matching the native packed-or-hash ABI.
#[test]
fn native_array_reference_staging_owns_a_boxed_slot() {
    let mut values = FakeOps::default();
    let mut context = ElephcEvalContext::new();
    let mut scope = ElephcEvalScope::new();
    let array = values.array_new(0).unwrap();
    let mut function = NativeFunction::new(std::ptr::null_mut(), fake_native_return_descriptor, 1);
    assert!(function.set_param_type(0, EvalParameterType::new(vec![EvalParameterTypeVariant::Array], false)));
    assert!(function.set_param_by_ref(0, true));
    let bound = bind_evaluated_native_function_args(
        &function,
        vec![EvaluatedCallArg {
            name: None, value: array.borrowed(),
            ref_target: Some(EvalReferenceTarget::Variable { scope: &mut scope, name: "items".into() }),
        }],
        &mut context, &mut values,
    ).unwrap();
    assert_eq!(bound.ref_slots.len(), 1);
    let BoundNativeFunctionRefSlot::Mixed { original, slot, .. } = &bound.ref_slots[0] else {
        panic!("array references must stage a boxed value, not its raw payload");
    };
    assert_eq!(*original, array);
    assert_eq!(**slot, array.as_ptr());
    assert_eq!(values.cell_owners[&(array.as_ptr() as usize)], 2);
    write_back_native_function_ref_args(&bound, &mut context, &mut values).unwrap();
    for marker in bound.values { values.release(marker).unwrap(); }
    assert_eq!(values.cell_owners[&(array.as_ptr() as usize)], 1);
    assert_eq!(values.cell_owners.values().sum::<usize>(), 1);
}

/// Call completion and pre-invocation failures retire internal cells without releasing caller borrows.
#[test]
fn native_function_retires_bound_cell_owners_on_every_dispatch_exit() {
    for exit in ["return", "array failure", "unsupported", "arity"] {
        let mut values = FakeOps::default();
        let mut context = ElephcEvalContext::new();
        let result = values.int(42).unwrap();
        let owned = values.string("default").unwrap();
        let caller = values.string("caller").unwrap();
        let mut function = NativeFunction::new(
            result.as_ptr().cast(), fake_native_return_descriptor, if exit == "arity" { 3 } else { 2 },
        );
        if exit == "unsupported" { function.set_bridge_supported(false); }
        if exit == "array failure" { values.fail_array_set_call(0); }
        let bound = BoundNativeFunctionArgs {
            values: vec![owned, caller.borrowed()], ref_slots: Vec::new(),
            named_keys: vec![None, None],
        };
        let outcome = eval_native_function_with_values(function, bound, &mut context, &mut values);
        assert_eq!(outcome.is_ok(), exit == "return", "{exit}");
        assert_eq!(values.cell_owners[&(owned.as_ptr() as usize)], 0, "{exit}");
        assert_eq!(values.cell_owners[&(caller.as_ptr() as usize)], 1, "{exit}");
    }
}

/// Defaults and scalar coercions own their cells, including a default replaced during coercion.
#[test]
fn native_function_binding_releases_defaults_and_coercions_after_dispatch() {
    let mut values = FakeOps::default();
    let mut context = ElephcEvalContext::new();
    let result = values.int(42).unwrap();
    let caller = values.string("7").unwrap();
    let mut function = NativeFunction::new(result.as_ptr().cast(), fake_native_return_descriptor, 3);
    assert!(function.set_param_type(0, EvalParameterType::new(vec![EvalParameterTypeVariant::Int], false)));
    assert!(function.set_param_type(1, EvalParameterType::new(vec![EvalParameterTypeVariant::String], false)));
    assert!(function.set_param_default(1, NativeCallableDefault::Int(8)));
    assert!(function.set_param_default(2, NativeCallableDefault::String("default".into())));
    let bound = bind_evaluated_native_function_args(
        &function, vec![EvaluatedCallArg { name: None, value: caller, ref_target: None }],
        &mut context, &mut values,
    ).unwrap();
    assert!(bound.values.iter().all(|value| !value.is_borrowed()));
    let outcome = eval_native_function_with_values(function, bound, &mut context, &mut values).unwrap();
    assert_eq!(outcome, result);
    assert_eq!(values.cell_owners[&(caller.as_ptr() as usize)], 1);
    assert_eq!(values.cell_owners.values().sum::<usize>(), 2);
}

/// A later type rejection releases earlier coercions and defaults while caller operands remain live.
#[test]
fn native_function_binding_releases_partial_type_conversions() {
    let mut values = FakeOps::default();
    let mut context = ElephcEvalContext::new();
    let caller = values.string("7").unwrap();
    let invalid = values.string("not an array").unwrap();
    let mut function = NativeFunction::new(std::ptr::null_mut(), fake_native_return_descriptor, 2);
    assert!(function.set_param_type(0, EvalParameterType::new(vec![EvalParameterTypeVariant::Int], false)));
    assert!(function.set_param_type(1, EvalParameterType::new(vec![EvalParameterTypeVariant::Array], false)));
    let outcome = bind_evaluated_native_function_args(
        &function,
        vec![
            EvaluatedCallArg { name: None, value: caller, ref_target: None },
            EvaluatedCallArg { name: None, value: invalid, ref_target: None },
        ],
        &mut context, &mut values,
    );
    assert!(matches!(outcome, Err(EvalStatus::RuntimeFatal)));
    assert_eq!(values.cell_owners[&(caller.as_ptr() as usize)], 1);
    assert_eq!(values.cell_owners[&(invalid.as_ptr() as usize)], 1);
    assert_eq!(values.cell_owners.values().sum::<usize>(), 2);
}

/// A failing later default does not strand cells allocated for earlier omitted parameters.
#[test]
fn native_function_binding_releases_partial_default_materialization() {
    let mut values = FakeOps::default();
    let mut context = ElephcEvalContext::new();
    let supplied = values.int(3).unwrap();
    let mut function = NativeFunction::new(std::ptr::null_mut(), fake_native_return_descriptor, 3);
    assert!(function.set_param_name(0, "first"));
    assert!(function.set_param_name(1, "second"));
    assert!(function.set_param_name(2, "third"));
    assert!(function.set_param_default(0, NativeCallableDefault::String("first".into())));
    assert!(function.set_param_default(1, NativeCallableDefault::Array(vec![
        crate::context::NativeCallableArrayDefaultElement::positional(NativeCallableDefault::Int(1)),
    ])));
    assert!(function.set_param_default(2, NativeCallableDefault::Int(3)));
    values.fail_array_set_call(0);
    let outcome = bind_evaluated_native_function_args(
        &function,
        vec![EvaluatedCallArg {
            name: Some("third".into()),
            value: supplied,
            ref_target: None,
        }],
        &mut context,
        &mut values,
    );
    assert!(matches!(outcome, Err(EvalStatus::UnsupportedConstruct)));
    assert_eq!(values.cell_owners[&(supplied.as_ptr() as usize)], 1);
    assert_eq!(values.cell_owners.values().sum::<usize>(), 1);
}

/// A named-only call over an earlier missing required slot retires every binding it already made.
///
/// `f(b: 1)` against `f($a, $b)` reaches the required-slot count through the NAMED slot alone, so
/// the refusal happens after slot `$b` is already bound. Both the bound regular slots and the
/// surplus list must be reclaimed there; the caller operand itself is borrowed and stays live.
#[test]
fn native_function_binding_releases_named_args_when_an_earlier_required_slot_is_missing() {
    let mut values = FakeOps::default();
    let mut context = ElephcEvalContext::new();
    let caller = values.string("second").unwrap();
    let mut function = NativeFunction::new(std::ptr::null_mut(), fake_native_return_descriptor, 2);
    assert!(function.set_param_name(0, "a"));
    assert!(function.set_param_name(1, "b"));
    let outcome = bind_evaluated_native_function_args(
        &function,
        vec![EvaluatedCallArg { name: Some("b".into()), value: caller, ref_target: None }],
        &mut context, &mut values,
    );
    assert!(matches!(outcome, Err(EvalStatus::RuntimeFatal)));
    assert_eq!(values.cell_owners[&(caller.as_ptr() as usize)], 1);
    assert_eq!(values.cell_owners.values().sum::<usize>(), 1);
    assert!(values.releases.is_empty(), "a borrowed caller operand must not be released here");
}

#[test]
fn variadic_native_function_binding_releases_named_args_with_an_earlier_required_hole() {
    let mut values = FakeOps::default();
    let mut context = ElephcEvalContext::new();
    let caller = values.string("second").unwrap();
    let mut function = NativeFunction::new(std::ptr::null_mut(), fake_native_return_descriptor, 3);
    assert!(function.set_param_name(0, "a"));
    assert!(function.set_param_name(1, "b"));
    assert!(function.set_param_name(2, "rest"));
    assert!(function.set_variadic_index(2));
    let outcome = bind_evaluated_native_function_args(
        &function,
        vec![EvaluatedCallArg { name: Some("b".into()), value: caller, ref_target: None }],
        &mut context,
        &mut values,
    );
    assert!(matches!(outcome, Err(EvalStatus::RuntimeFatal)));
    assert_eq!(values.cell_owners[&(caller.as_ptr() as usize)], 1);
    assert_eq!(values.cell_owners.values().sum::<usize>(), 1);
    assert!(values.releases.is_empty(), "a borrowed caller operand must not be released here");
}

/// Failed marker allocation rolls back both earlier slots and the current raw or boxed lease.
#[test]
fn native_function_staging_releases_partial_markers_and_payloads() {
    for variant in [EvalParameterTypeVariant::String, EvalParameterTypeVariant::Array, EvalParameterTypeVariant::Mixed] {
        let mut values = FakeOps::default();
        let mut context = ElephcEvalContext::new();
        let mut scope = ElephcEvalScope::new();
        let mut function = NativeFunction::new(std::ptr::null_mut(), fake_native_return_descriptor, 2);
        let mut arguments = Vec::new();
        for position in 0..2 {
            let value = if matches!(variant, EvalParameterTypeVariant::Array) {
                values.array_new(0).unwrap()
            } else { values.string("keep").unwrap() };
            assert!(function.set_param_type(position, EvalParameterType::new(vec![variant.clone()], false)));
            assert!(function.set_param_by_ref(position, true));
            arguments.push(EvaluatedCallArg {
                name: None, value,
                ref_target: Some(EvalReferenceTarget::Variable { scope: &mut scope, name: position.to_string() }),
            });
        }
        let caller_values: Vec<_> = arguments.iter().map(|argument| argument.value).collect();
        values.fail_invoker_marker_call = Some(1);
        let outcome = bind_evaluated_native_function_args(&function, arguments, &mut context, &mut values);
        assert!(matches!(outcome, Err(EvalStatus::RuntimeFatal)));
        assert_eq!(values.invoker_marker_calls, 2);
        for caller in caller_values { assert_eq!(values.cell_owners[&(caller.as_ptr() as usize)], 1); }
        assert_eq!(values.cell_owners.values().sum::<usize>(), 2);
    }
}

/// A failing cleanup still retires all subsequent raw and boxed staging owners.
#[test]
fn native_ref_cleanup_consumes_every_slot_after_a_release_error() {
    for writeback in [false, true] {
        let mut values = FakeOps::default();
        let mut context = ElephcEvalContext::new();
        let string = values.string("staged").unwrap();
        let array = values.array_new(0).unwrap();
        let mixed = values.int(7).unwrap();
        let bound = BoundNativeFunctionArgs {
            values: Vec::new(),
            named_keys: Vec::new(),
            ref_slots: vec![
                BoundNativeFunctionRefSlot::RawString {
                    original: [string.as_ptr() as u64, 6],
                    slot: Box::new([string.as_ptr() as u64, 6]), target: None,
                },
                BoundNativeFunctionRefSlot::OwnedRawWord {
                    original: array.as_ptr() as u64,
                    slot: Box::new(array.as_ptr() as u64), target: None,
                },
                BoundNativeFunctionRefSlot::Mixed {
                    original: mixed, slot: Box::new(mixed.as_ptr()), target: None,
                },
            ],
        };
        values.fail_release_call = Some(0);
        let result = if writeback {
            write_back_native_function_ref_args(&bound, &mut context, &mut values)
        } else {
            cleanup_native_function_ref_args(&bound, &mut values)
        };
        assert_eq!(result, Err(EvalStatus::UncaughtThrowable));
        assert_eq!(values.releases, vec![string, array, mixed]);
        for value in [string, array, mixed] {
            assert_eq!(values.cell_owners[&(value.as_ptr() as usize)], 0);
        }
    }
}

/// An error releasing displaced storage must not release the value already adopted by its scope.
#[test]
fn native_ref_writeback_preserves_published_values_and_finishes_later_slots() {
    let mut values = FakeOps::default();
    let mut context = ElephcEvalContext::new();
    let mut scope = ElephcEvalScope::new();
    let old_first = values.int(1).unwrap();
    let old_second = values.int(2).unwrap();
    let first = values.string("first").unwrap();
    let second = values.string("second").unwrap();
    scope.set("first", old_first, ScopeCellOwnership::Owned);
    scope.set("second", old_second, ScopeCellOwnership::Owned);
    let bound = BoundNativeFunctionArgs {
        values: Vec::new(),
        named_keys: Vec::new(),
        ref_slots: vec![
            BoundNativeFunctionRefSlot::Mixed {
                original: old_first, slot: Box::new(first.as_ptr()),
                target: Some(EvalReferenceTarget::Variable { scope: &mut scope, name: "first".into() }),
            },
            BoundNativeFunctionRefSlot::Mixed {
                original: old_second, slot: Box::new(second.as_ptr()),
                target: Some(EvalReferenceTarget::Variable { scope: &mut scope, name: "second".into() }),
            },
        ],
    };
    values.fail_release_call = Some(0);
    let result = write_back_native_function_ref_args(&bound, &mut context, &mut values);
    assert_eq!(result, Err(EvalStatus::UncaughtThrowable));
    assert_eq!(scope.entry("first").unwrap().cell(), first);
    assert_eq!(scope.entry("second").unwrap().cell(), second);
    assert_eq!(values.releases, vec![old_first, old_second]);
    assert_eq!(values.cell_owners[&(first.as_ptr() as usize)], 1);
    assert_eq!(values.cell_owners[&(second.as_ptr() as usize)], 1);
}

/// A failed argument-array destructor discards a successful native result exactly once.
#[test]
fn native_function_discards_result_when_argument_cleanup_throws() {
    let mut values = FakeOps::default();
    let mut context = ElephcEvalContext::new();
    let value = values.string("result").unwrap();
    let function = NativeFunction::new(value.as_ptr().cast(), fake_native_return_descriptor, 0);
    let bound = BoundNativeFunctionArgs::default();
    values.fail_release_call = Some(0);
    let result = eval_native_function_with_values(function, bound, &mut context, &mut values);
    assert_eq!(result, Err(EvalStatus::UncaughtThrowable));
    assert_eq!(values.releases.len(), 2);
    assert_eq!(values.releases[1], value);
    assert_eq!(values.cell_owners[&(value.as_ptr() as usize)], 0);
}

/// Invalid caller storage releases the unpublished slot, later slots, the result, and arguments.
#[test]
fn native_function_discards_result_and_all_slots_when_writeback_fails() {
    let mut values = FakeOps::default();
    let mut context = ElephcEvalContext::new();
    let value = values.string("result").unwrap();
    let original = values.int(1).unwrap();
    let first = values.string("first").unwrap();
    let later = values.string("later").unwrap();
    let function = NativeFunction::new(value.as_ptr().cast(), fake_native_return_descriptor, 0);
    let bound = BoundNativeFunctionArgs {
        values: Vec::new(),
        named_keys: Vec::new(),
        ref_slots: vec![
            BoundNativeFunctionRefSlot::Mixed {
                original, slot: Box::new(first.as_ptr()),
                target: Some(EvalReferenceTarget::Variable { scope: std::ptr::null_mut(), name: "missing".into() }),
            },
            BoundNativeFunctionRefSlot::Mixed {
                original: later, slot: Box::new(later.as_ptr()), target: None,
            },
        ],
    };
    let result = eval_native_function_with_values(function, bound, &mut context, &mut values);
    assert_eq!(result, Err(EvalStatus::RuntimeFatal));
    assert_eq!(&values.releases[..3], &[first, later, value]);
    assert_eq!(values.releases.len(), 4);
    for released in [first, later, value] {
        assert_eq!(values.cell_owners[&(released.as_ptr() as usize)], 0);
    }
    assert_eq!(values.cell_owners[&(original.as_ptr() as usize)], 1);
}

/// A retaining invoker-slot setter receives a borrow and does not keep the source staging owner.
#[test]
fn native_ref_writeback_retires_its_owner_after_a_retaining_setter() {
    let mut values = FakeOps::default();
    let mut context = ElephcEvalContext::new();
    let old = values.int(1).unwrap();
    let value = values.string("new").unwrap();
    let mut caller_slot = old.as_ptr();
    let bound = BoundNativeFunctionArgs {
        values: Vec::new(),
        named_keys: Vec::new(),
        ref_slots: vec![BoundNativeFunctionRefSlot::Mixed {
            original: old, slot: Box::new(value.as_ptr()),
            target: Some(EvalReferenceTarget::InvokerSlot {
                slot: (&mut caller_slot as *mut *mut crate::value::RuntimeCell) as usize,
                source_tag: EVAL_TAG_MIXED,
            }),
        }],
    };
    write_back_native_function_ref_args(&bound, &mut context, &mut values).unwrap();
    assert_eq!(caller_slot, value.as_ptr());
    assert_eq!(values.retains, vec![value.borrowed()]);
    assert_eq!(values.releases, vec![old, value]);
    assert_eq!(values.cell_owners[&(value.as_ptr() as usize)], 1);
}
