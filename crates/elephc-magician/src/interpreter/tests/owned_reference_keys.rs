//! Purpose:
//! Verifies owned reference keys through generic calls and escaped promoted property aliases.
//!
//! Called from:
//! - Focused staged walker and VM ownership regressions.
//!
//! Key details:
//! - Literal integer/string dimensions cannot borrow a caller variable's key owner.
//! - Numeric string kinds and embedded NUL bytes survive call-local metadata retirement.

use super::super::*;
use super::support::*;
use super::strict_semantics::staged_numeric_execution;
use elephc_semantics::{SemanticProfile, StrictFeature};

/// Both engines keep escaped key metadata alive and retire every original evaluated key owner.
#[test]
fn owned_eval_reference_keys_walker_vm_escape_and_retirement() {
    let source = include_str!("../../../../../tests/fixtures/strict/owned_reference_keys.php").strip_prefix("<?php\n").unwrap();
    for (exact, coordinated) in [(false, false), (false, true), (true, false), (true, true)] {
        for vm in [false, true] {
            let profile = SemanticProfile::PERMISSIVE.with(StrictFeature::ArrayKeys, exact).with(StrictFeature::Casts, coordinated);
            let mut context = ElephcEvalContext::new(); context.install_semantic_profile(profile).unwrap();
            let mut scope = ElephcEvalScope::new(); let mut values = FakeOps::default(); values.fixed_profile = Some(profile);
            assert!(context.set_global_scope(&mut scope));
            staged_numeric_execution(source, false, &mut context, &mut scope, &mut values).unwrap();
            let call = if vm { "if(false){function ownedKeyObservations(){}} ownedKeyObservations();" }
                else { "ownedKeyObservations();" };
            let result = staged_numeric_execution(call, vm, &mut context, &mut scope, &mut values);
            assert!(result.is_ok(), "exact={exact} casts={coordinated} vm={vm}: {:?}; {}", result.err(), values.output);
            let expected = if exact { "31/41|31/41|7|8|9/6|10|31/41|31/41|" }
                else { "41/41|41/41|7|8|9/9|10|31/41|31/41|" };
            assert_eq!(values.output, expected, "exact={exact} casts={coordinated} vm={vm}");
            for value in scope.drain_owned_cells() { values.release(value).unwrap(); }
            for value in context.take_persistent_cell_owners() { values.release(value).unwrap(); }
            for (id, owners) in &values.cell_owners { assert_eq!(*owners, 0, "exact={exact} casts={coordinated} vm={vm}: {:?}", values.values[id]); }
        }
    }
}

/// Argument binding failures retire original key operands even when the callee never executes.
#[test]
fn owned_eval_reference_key_argument_failure_retires_original_operands() {
    let source = "function ownedKeyRequired(mixed &$value, int $required): void {}
function ownedKeyFailure(): void {
$items = ['01' => 7];
try { ownedKeyRequired($items['01']); }
catch (ArgumentCountError $error) { echo 'caught|'; unset($error); }
echo $items['01'], '|';
}";
    for exact in [false, true] {
        for vm in [false, true] {
            let profile = SemanticProfile::PERMISSIVE.with(StrictFeature::ArrayKeys, exact);
            let mut context = ElephcEvalContext::new(); context.install_semantic_profile(profile).unwrap();
            let mut scope = ElephcEvalScope::new(); let mut values = FakeOps::default(); values.fixed_profile = Some(profile);
            assert!(context.set_global_scope(&mut scope));
            staged_numeric_execution(source, false, &mut context, &mut scope, &mut values).unwrap();
            let call = if vm { "if(false){function ownedKeyFailure(){}} ownedKeyFailure();" } else { "ownedKeyFailure();" };
            staged_numeric_execution(call, vm, &mut context, &mut scope, &mut values).unwrap();
            assert_eq!(values.output, "caught|7|", "exact={exact} vm={vm}");
            for value in scope.drain_owned_cells() { values.release(value).unwrap(); }
            for value in context.take_persistent_cell_owners() { values.release(value).unwrap(); }
            for (id, owners) in &values.cell_owners { assert_eq!(*owners, 0, "exact={exact} vm={vm}: {:?}", values.values[id]); }
        }
    }
}

/// Selected key snapshots survive source retirement and avoid a second float diagnostic.
#[test]
fn owned_eval_reference_key_snapshots_preserve_data_without_runtime_owners() {
    for exact in [false, true] {
        let profile = SemanticProfile::PERMISSIVE.with(StrictFeature::ArrayKeys, exact);
        let mut values = FakeOps::default();
        let mut cases = vec![
            (FakeValue::Int(7), EvalArrayReferenceKey::Int(7)),
            (FakeValue::String("0".into()), if exact { EvalArrayReferenceKey::String(b"0".to_vec()) } else { EvalArrayReferenceKey::Int(0) }),
            (FakeValue::String("01".into()), EvalArrayReferenceKey::String(b"01".to_vec())),
            (FakeValue::String("a\0b".into()), EvalArrayReferenceKey::String(b"a\0b".to_vec())),
        ];
        if !exact {
            cases.extend([
                (FakeValue::Null, EvalArrayReferenceKey::String(Vec::new())),
                (FakeValue::Bool(true), EvalArrayReferenceKey::Int(1)),
                (FakeValue::Float(1.5), EvalArrayReferenceKey::Int(1)),
                (FakeValue::Resource(0), EvalArrayReferenceKey::Int(1)),
            ]);
        }
        for (source, expected) in cases {
            let source = values.alloc(source);
            let key = reference_keys::snapshot(source, profile, &mut values).unwrap();
            values.release(source).unwrap();
            assert_eq!(key, expected);
            let detached = key.clone();
            drop(key);
            let materialized = reference_keys::materialize(&detached, &mut values).unwrap();
            match expected {
                EvalArrayReferenceKey::Int(integer) => assert_eq!(values.get(materialized), FakeValue::Int(integer)),
                EvalArrayReferenceKey::String(bytes) => {
                    assert_eq!(values.type_tag(materialized).unwrap(), EVAL_TAG_STRING);
                    assert_eq!(values.string_bytes(materialized).unwrap(), bytes);
                }
            }
            values.release(materialized).unwrap();
        }
        assert!(values.warnings.is_empty(), "{:?}", values.warnings);
        assert!(values.cell_owners.values().all(|owners| *owners == 0));
    }
}
