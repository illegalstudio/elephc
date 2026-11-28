//! Purpose:
//! Verifies exact checked numeric conversions and ownership through both eval engines.
//!
//! Called from:
//! - Focused Magician interpreter tests using the real shared scalar cast contract.
//!
//! Key details:
//! - Staged execution leaves public admission closed while testing pending guarantees.

use super::super::*;
use super::{support::*, strict_semantics::staged_numeric_execution};
use elephc_semantics::{SemanticProfile, StrictFeature};

/// Retires all visible owners and proves every created eval cell has been released.
fn clean(scope: &mut ElephcEvalScope, values: &mut FakeOps) {
    for value in scope.drain_owned_cells() { values.release(value).unwrap(); }
    for (id, owners) in &values.cell_owners { assert_eq!(*owners, 0, "{:?}", values.values[id]); }
    assert!(values.warnings.is_empty());
}

/// Walker and VM retain endpoint integers and explicit scalar conversions under independent flags.
#[test]
fn strict_checked_casts_eval_walker_vm_exact_results_and_owners() {
    for vm in [false, true] {
        for extra in [None, Some(StrictFeature::Coercions), Some(StrictFeature::Arithmetic), Some(StrictFeature::Overflow)] {
            for (source, expected) in [
                ("return (int)'9223372036854775807.9';", FakeValue::Int(i64::MAX)),
                ("return (int)'-9223372036854775808.9';", FakeValue::Int(i64::MIN)),
                ("return (int)'92233720368547758070e-1';", FakeValue::Int(i64::MAX)),
                ("return (int)'1e-99999999999999999999';", FakeValue::Int(0)),
                ("return (int)true;", FakeValue::Int(1)), ("return (int)null;", FakeValue::Int(0)),
                ("return (int)-7.9;", FakeValue::Int(-7)), ("return (float)1;", FakeValue::Float(1.0)),
                ("return (float)'1e999';", FakeValue::Float(f64::INFINITY)),
            ] {
                let mut profile = SemanticProfile::PERMISSIVE.with(StrictFeature::Casts, true);
                if let Some(feature) = extra { profile = profile.with(feature, true); }
                let mut context = ElephcEvalContext::new(); context.install_semantic_profile(profile).unwrap();
                let mut scope = ElephcEvalScope::new(); let mut values = FakeOps::default();
                let result = staged_numeric_execution(source, vm, &mut context, &mut scope, &mut values);
                let Ok(EvalControl::Return(value)) = result else { panic!("unexpected result: {source} vm={vm} {profile:?}"); };
                assert_eq!(values.get(value), expected, "{source} vm={vm}");
                values.release(value).unwrap(); clean(&mut scope, &mut values);
            }
        }
    }
}

/// Both engines publish ValueError without replacing the failed conversion's borrowed source.
#[test]
fn strict_checked_casts_eval_failures_preserve_borrowed_sources_and_throwable_owners() {
    for vm in [false, true] {
        for source in ["$saved = '12tail'; return (int)$saved;", "$saved = '9223372036854775808'; return (int)$saved;",
            "$saved = 1e999; return (int)$saved;", "$saved = 'junk'; return (float)$saved;",
            "$saved = \"1\\0\"; return (int)$saved;"] {
            let mut context = ElephcEvalContext::new();
            context.install_semantic_profile(SemanticProfile::PERMISSIVE.with(StrictFeature::Casts, true)).unwrap();
            let mut scope = ElephcEvalScope::new(); let mut values = FakeOps::default();
            assert!(matches!(staged_numeric_execution(source, vm, &mut context, &mut scope, &mut values), Err(EvalStatus::UncaughtThrowable)), "{source} vm={vm}");
            let error = context.take_pending_throw().expect("one checked cast Throwable owner");
            assert!(values.object_is_a(error, "ValueError", false).unwrap());
            let saved = scope.visible_cell("saved").unwrap();
            assert_eq!(values.cell_owners[&(saved.as_ptr() as usize)], 1);
            values.release(error).unwrap(); clean(&mut scope, &mut values);
        }
    }
}

/// Opaque scalar payloads keep IEEE bits, while unsupported aggregate/resource domains fail safely.
#[test]
fn strict_checked_casts_eval_ieee_bits_and_original_non_scalar_domains() {
    for vm in [false, true] {
        for bits in [0, 1 << 63, 0x7ff8_1234_5678_9abc, f64::INFINITY.to_bits()] {
            let mut context = ElephcEvalContext::new();
            context.install_semantic_profile(SemanticProfile::PERMISSIVE.with(StrictFeature::Casts, true)).unwrap();
            let mut scope = ElephcEvalScope::new(); let mut values = FakeOps::default();
            let original = values.alloc(FakeValue::Float(f64::from_bits(bits))); scope.set("original", original, ScopeCellOwnership::Owned);
            let Ok(EvalControl::Return(result)) = staged_numeric_execution("return (float)$original;", vm, &mut context, &mut scope, &mut values) else { panic!("float identity"); };
            assert_eq!(values.raw_value_word(result).unwrap(), bits); assert_ne!(result, original);
            values.release(result).unwrap(); clean(&mut scope, &mut values);
        }
        for (input, class) in [(FakeValue::Array(Vec::new()), "TypeError"),
            (FakeValue::Object(Vec::new()), "TypeError"), (FakeValue::Resource(7), "TypeError"),
            (FakeValue::Bytes(vec![255]), "ValueError")] {
            let mut context = ElephcEvalContext::new();
            context.install_semantic_profile(SemanticProfile::PERMISSIVE.with(StrictFeature::Casts, true)).unwrap();
            let mut scope = ElephcEvalScope::new(); let mut values = FakeOps::default();
            let original = values.alloc(input.clone()); scope.set("original", original, ScopeCellOwnership::Owned);
            assert!(matches!(staged_numeric_execution("return (int)$original;", vm, &mut context, &mut scope, &mut values), Err(EvalStatus::UncaughtThrowable)));
            assert_eq!(values.get(original), input);
            let error = context.take_pending_throw().unwrap(); assert!(values.object_is_a(error, class, false).unwrap());
            values.release(error).unwrap(); clean(&mut scope, &mut values);
        }
    }
}

/// Walker catch/finally and direct VM failures preserve the permissive scalar control.
#[test]
fn strict_checked_casts_eval_catch_finally_and_permissive_controls() {
    for vm in [false, true] {
        for checked in [false, true] {
            let mut context = ElephcEvalContext::new();
            context.install_semantic_profile(SemanticProfile::PERMISSIVE.with(StrictFeature::Casts, checked)).unwrap();
            let mut scope = ElephcEvalScope::new(); let mut values = FakeOps::default();
            let source = if vm { "echo (int)'junk';" } else {
                "try { echo (int)'junk'; } catch (ValueError $e) { echo 'V'; } finally { echo 'F'; }"
            };
            let result = staged_numeric_execution(source, vm, &mut context, &mut scope, &mut values);
            if vm && checked {
                assert!(matches!(result, Err(EvalStatus::UncaughtThrowable)));
                let error = context.take_pending_throw().unwrap(); values.release(error).unwrap();
            } else { assert!(result.is_ok()); }
            let expected = match (vm, checked) { (false, true) => "VF", (false, false) => "0F",
                (true, true) => "", (true, false) => "0" };
            assert_eq!(values.output, expected); clean(&mut scope, &mut values);
        }
    }
}

/// Protected native failures transfer one owner; success and unsupported status do not consume it.
#[test]
fn strict_checked_casts_eval_native_status_transfers_preserve_owner_contracts() {
    let mut context = ElephcEvalContext::new(); let mut scope = ElephcEvalScope::new(); let mut values = FakeOps::default();
    let pending = values.new_object("ValueError").unwrap(); values.pending_runtime_throwable = Some(pending);
    let value = values.int(7).unwrap();
    assert_eq!(native_results::finish(Ok(value), &mut context, &mut values), Ok(value));
    assert_eq!(values.pending_runtime_throwable, Some(pending));
    assert_eq!(native_results::finish::<RuntimeCellHandle>(Err(EvalStatus::UnsupportedConstruct), &mut context, &mut values), Err(EvalStatus::UnsupportedConstruct));
    assert_eq!(values.pending_runtime_throwable, Some(pending));
    assert_eq!(native_results::finish::<RuntimeCellHandle>(Err(EvalStatus::UncaughtThrowable), &mut context, &mut values), Err(EvalStatus::UncaughtThrowable));
    assert!(values.pending_runtime_throwable.is_none()); assert_eq!(context.take_pending_throw(), Some(pending));
    values.release(value).unwrap(); values.release(pending).unwrap(); clean(&mut scope, &mut values);
}
