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
    let bound = BoundNativeFunctionArgs { values: Vec::new(), ref_slots: Vec::new() };
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
