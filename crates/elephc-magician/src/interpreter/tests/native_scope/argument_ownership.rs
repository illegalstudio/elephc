//! Purpose:
//! Checks native eval argument owners through evaluation, binding, staging, and invocation failures.
//!
//! Called from:
//! - Magician's native-scope interpreter tests.
//!
//! Key details:
//! - Release records distinguish fresh argument owners from borrowed caller variables.
//! - Native executable tests separately validate actual heap reference counts.

use super::*;
use crate::value::RuntimeCell;

/// Releases each literal on success, later expression failure, and duplicate or unknown named binding.
#[test]
fn native_argument_ownership_literals() {
    for source in [
        "return native_answer(731);",
        "return native_answer(731, missing_argument());",
        "return native_answer(unknown: 731);",
        "return native_answer(731, value: 732);",
    ] {
        let mut context = ElephcEvalContext::new();
        let mut scope = ElephcEvalScope::new();
        let mut values = FakeOps::default();
        let expected = values.int(42).unwrap();
        let mut native = NativeFunction::new(expected.as_ptr().cast(), fake_native_return_descriptor, 1);
        assert!(native.set_param_name(0, "value"));
        assert!(context.define_native_function("native_answer", native).is_ok());
        let program = parse_fragment(source.as_bytes()).unwrap();
        let result = execute_program_with_context(&mut context, &program, &mut scope, &mut values);
        assert_eq!(result.is_ok(), source == "return native_answer(731);", "{source}");
        assert_released_once(&values, &FakeValue::Int(731));
        if source.contains("732") { assert_released_once(&values, &FakeValue::Int(732)); }
        assert!(!values.releases.contains(&expected));
    }
}

/// Releases defaults for fixed and variadic callees on success, type failure, and array insertion failure.
#[test]
fn native_argument_ownership_defaults() {
    for variadic in [false, true] {
        for failure in [0, 1, 2] {
            let mut context = ElephcEvalContext::new();
            let mut scope = ElephcEvalScope::new();
            let mut values = FakeOps::default();
            let expected = values.int(42).unwrap();
            let borrowed = values.int(997).unwrap();
            scope.set("value", borrowed, ScopeCellOwnership::Borrowed);
            let mut native = NativeFunction::new(expected.as_ptr().cast(), fake_native_return_descriptor,
                if variadic { 4 } else { 3 });
            if variadic { assert!(native.set_variadic_index(3)); }
            assert!(native.set_param_default(1, NativeCallableDefault::Int(731)));
            assert!(native.set_param_default(2, NativeCallableDefault::String("732".to_string())));
            if failure == 1 {
                assert!(native.set_param_type(2, EvalParameterType::new(
                    vec![EvalParameterTypeVariant::Class("ExpectedObject".to_string())], false,
                )));
            }
            if failure == 2 { values.fail_array_set_call(1); }
            assert!(context.define_native_function("native_answer", native).is_ok());
            let program = parse_fragment(b"return native_answer($value);").unwrap();
            let result = execute_program_with_context(&mut context, &program, &mut scope, &mut values);
            assert_eq!(result.is_ok(), failure == 0);
            assert_released_once(&values, &FakeValue::Int(731));
            assert_released_once(&values, &FakeValue::String("732".to_string()));
            assert_borrowed_lease_balanced(&values, borrowed);
        }
    }
}

/// Releases a scalar conversion while preserving the caller's unconverted argument cell.
#[test]
fn native_argument_ownership_coercions() {
    for fail in [false, true] {
        let mut context = ElephcEvalContext::new();
        let mut scope = ElephcEvalScope::new();
        let mut values = FakeOps::default();
        let expected = values.int(42).unwrap();
        let borrowed = values.string("731").unwrap();
        scope.set("value", borrowed, ScopeCellOwnership::Borrowed);
        let mut native = NativeFunction::new(expected.as_ptr().cast(), fake_native_return_descriptor, 1);
        assert!(native.set_param_type(0, EvalParameterType::new(vec![EvalParameterTypeVariant::Int], false)));
        if fail { values.fail_array_set_call(0); }
        assert!(context.define_native_function("native_answer", native).is_ok());
        let program = parse_fragment(b"return native_answer($value);").unwrap();
        let result = execute_program_with_context(&mut context, &program, &mut scope, &mut values);
        assert_eq!(result.is_ok(), !fail);
        assert_released_once(&values, &FakeValue::Int(731));
        assert_borrowed_lease_balanced(&values, borrowed);
    }
}

/// Releases reference markers and their retained payloads after normal invocation or argument-array failure.
#[test]
fn native_argument_ownership_reference_markers() {
    for fail in [false, true] {
        let mut context = ElephcEvalContext::new();
        let mut scope = ElephcEvalScope::new();
        let mut values = FakeOps::default();
        let expected = values.int(42).unwrap();
        let mut native = NativeFunction::new(expected.as_ptr().cast(), fake_native_return_descriptor, 2);
        for (index, name) in ["left", "right"].iter().enumerate() {
            let value = values.string(name).unwrap();
            scope.set(*name, value, ScopeCellOwnership::Borrowed);
            assert!(native.set_param_by_ref(index, true));
            assert!(native.set_param_type(index, EvalParameterType::new(vec![EvalParameterTypeVariant::String], false)));
        }
        if fail { values.fail_array_set_call(1); }
        assert!(context.define_native_function("native_answer", native).is_ok());
        let program = parse_fragment(b"return native_answer($left, $right);").unwrap();
        let result = execute_program_with_context(&mut context, &program, &mut scope, &mut values);
        assert_eq!(result.is_ok(), !fail);
        let markers: Vec<_> = values.values.values().filter(|value| matches!(value, FakeValue::InvokerRefCell(_))).collect();
        assert_eq!(markers.len(), 2);
        for marker in markers { assert_released_once(&values, marker); }
        for name in ["left", "right"] {
            let cells = values.values.iter().filter_map(|(id, value)| {
                (value == &FakeValue::String(name.to_string())).then_some(*id)
            }).collect::<Vec<_>>();
            assert_eq!(cells.len(), 1, "{name}");
            let value = RuntimeCellHandle::from_raw(cells[0] as *mut RuntimeCell);
            assert_borrowed_lease_balanced(&values, value);
        }
    }
}

/// Balances the sole live raw-string slot owner before and after native replacement.
#[test]
fn native_argument_ownership_raw_string_slot_ledger() {
    for changed in [false, true] {
        let mut values = FakeOps::default();
        let original = values.string("original").unwrap();
        let original_words = [
            values.raw_value_word(original).unwrap(),
            values.raw_value_high_word(original).unwrap(),
        ];
        let retained = values
            .retain_raw_string_words(original_words[0], original_words[1])
            .unwrap();
        let current = if changed {
            values
                .release_raw_string_words(retained.0, retained.1)
                .unwrap();
            let replacement = values.string("replacement").unwrap();
            [
                values.raw_value_word(replacement).unwrap(),
                values.raw_value_high_word(replacement).unwrap(),
            ]
        } else {
            [retained.0, retained.1]
        };
        let args = raw_string_ref_args([retained.0, retained.1], current);

        cleanup_native_function_ref_args_for_test(&args, &mut values).unwrap();

        assert_eq!(release_count(&values, original), 1);
        assert_eq!(values.cell_owners.get(&(original.as_ptr() as usize)), Some(&1));
        if changed {
            let replacement = RuntimeCellHandle::from_raw(current[0] as *mut RuntimeCell);
            assert_eq!(release_count(&values, replacement), 1);
            assert_eq!(values.cell_owners.get(&(replacement.as_ptr() as usize)), Some(&0));
        }
    }
}

/// Balances the sole live one-word heap slot owner before and after native replacement.
#[test]
fn native_argument_ownership_raw_heap_slot_ledger() {
    for changed in [false, true] {
        let mut values = FakeOps::default();
        let original = values.array_new(0).unwrap();
        let original_word = values.raw_value_word(original).unwrap();
        let retained = values.retain_raw_heap_word(original_word).unwrap();
        let current = if changed {
            values.release_raw_heap_word(retained).unwrap();
            let replacement = values.array_new(0).unwrap();
            values.raw_value_word(replacement).unwrap()
        } else {
            retained
        };
        let args = raw_heap_ref_args(original_word, current);

        cleanup_native_function_ref_args_for_test(&args, &mut values).unwrap();

        assert_eq!(release_count(&values, original), 1);
        assert_eq!(values.cell_owners.get(&(original.as_ptr() as usize)), Some(&1));
        if changed {
            let replacement = RuntimeCellHandle::from_raw(current as *mut RuntimeCell);
            assert_eq!(release_count(&values, replacement), 1);
            assert_eq!(values.cell_owners.get(&(replacement.as_ptr() as usize)), Some(&0));
        }
    }
}

/// Builds one staged raw-string argument without a caller writeback target.
fn raw_string_ref_args(original: [u64; 2], current: [u64; 2]) -> BoundNativeFunctionArgs {
    BoundNativeFunctionArgs {
        values: Vec::new(),
        ref_slots: vec![BoundNativeFunctionRefSlot::RawString {
            original,
            slot: Box::new(current),
            target: None,
        }],
        owners: Vec::new(),
    }
}

/// Builds one staged raw-heap argument without a caller writeback target.
fn raw_heap_ref_args(original: u64, current: u64) -> BoundNativeFunctionArgs {
    BoundNativeFunctionArgs {
        values: Vec::new(),
        ref_slots: vec![BoundNativeFunctionRefSlot::OwnedRawWord {
            original,
            slot: Box::new(current),
            target: None,
        }],
        owners: Vec::new(),
    }
}

/// Counts recorded releases for one fake runtime identity.
fn release_count(values: &FakeOps, value: RuntimeCellHandle) -> usize {
    values
        .releases
        .iter()
        .filter(|released| **released == value)
        .count()
}

/// Checks the single explicit owner of a value constructed by an argument fixture.
fn assert_released_once(values: &FakeOps, expected: &FakeValue) {
    let cells: Vec<_> = values.values.iter().filter_map(|(id, value)| (value == expected).then_some(*id)).collect();
    assert_eq!(cells.len(), 1, "{expected:?}");
    assert_eq!(values.releases.iter().filter(|value| value.as_ptr() as usize == cells[0]).count(), 1, "{expected:?}");
}

/// Confirms temporary retains are balanced without consuming the caller's durable owner.
fn assert_borrowed_lease_balanced(values: &FakeOps, value: RuntimeCellHandle) {
    let identity = value.as_ptr() as usize;
    let retains = values.retains.iter().filter(|retained| {
        retained.as_ptr() as usize == identity
    }).count();
    let releases = values.releases.iter().filter(|released| {
        released.as_ptr() as usize == identity
    }).count();
    assert!(retains > 0, "borrowed cell was not retained");
    assert_eq!(releases, retains, "borrowed lease retain/release imbalance");
    assert_eq!(values.cell_owners.get(&identity), Some(&1));
}
