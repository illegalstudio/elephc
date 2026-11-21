//! Purpose:
//! Checks exact eval key identities across array metadata and by-reference call writeback.
//!
//! Called from:
//! - Focused walker and bytecode semantic-profile regressions.
//!
//! Key details:
//! - The fake primitive adapter declares the same immutable profile as its context.
//! - Reference metadata and automatic indexes preserve integer/string distinctions.

use super::super::*;
use super::support::*;
use super::strict_semantics::staged_numeric_execution;
use elephc_semantics::{SemanticProfile, StrictFeature};

/// Metadata keys reuse exact bytes and integer identities without retaining temporary handles.
#[test]
fn exact_eval_reference_key_metadata_preserves_kinds_and_bytes() {
    for exact in [false, true] {
        let profile = SemanticProfile::PERMISSIVE.with(StrictFeature::ArrayKeys, exact);
        let mut values = FakeOps::default();
        for (bytes, integer) in [(b"0".as_slice(), Some(0)), (b"1".as_slice(), Some(1)),
            (b"01".as_slice(), None), (b"-1".as_slice(), Some(-1)),
            (b"a\0b".as_slice(), None), (b"9223372036854775808".as_slice(), None)] {
            let key = values.string_bytes_value(bytes).unwrap();
            let result = eval_array_reference_key(key, profile, &mut values).unwrap().unwrap();
            let expected = if let Some(integer) = integer.filter(|_| !exact) { EvalArrayReferenceKey::Int(integer) }
                else { EvalArrayReferenceKey::String(bytes.into()) };
            assert_eq!(result, expected, "exact={exact}, bytes={bytes:?}");
            values.release(key).unwrap();
        }
        assert!(values.cell_owners.values().all(|owners| *owners == 0));
    }
}

/// Walker and VM keep automatic keys, aliases, method ref arguments and property mutations coherent.
#[test]
fn exact_eval_key_metadata_walker_vm_aliases_indexes_and_owners() {
    let source = include_str!("../../../../../tests/fixtures/strict/eval_exact_key_metadata.php")
        .strip_prefix("<?php\n").unwrap();
    for exact in [false, true] {
        for vm in [false, true] {
            let profile = SemanticProfile::PERMISSIVE.with(StrictFeature::ArrayKeys, exact);
            let mut context = ElephcEvalContext::new(); context.install_semantic_profile(profile).unwrap();
            let mut scope = ElephcEvalScope::new(); let mut values = FakeOps::default();
            values.fixed_profile = Some(profile);
            assert!(context.set_global_scope(&mut scope));
            staged_numeric_execution(source, false, &mut context, &mut scope, &mut values).unwrap();
            let call = if vm { "if(false){function evalExactMetadataObservations(){}} evalExactMetadataObservations();" }
                else { "evalExactMetadataObservations();" };
            let result = staged_numeric_execution(call, vm, &mut context, &mut scope, &mut values);
            assert!(result.is_ok(), "exact={exact} vm={vm}: {:?}; {}", result.as_ref().err(), values.output);
            let expected = if exact {
                "string:0:10|integer:0:20|integer:1:30|12/20|31/41|31/20/31/41|string:0:9|string:01:3|3/2|"
            } else {
                "integer:0:10|integer:1:20|integer:2:30|12/20|41/41|41/20/41/41|string:01:3|3/3|"
            };
            assert_eq!(values.output, expected, "exact={exact} vm={vm}");
            for value in scope.drain_owned_cells() { values.release(value).unwrap(); }
            for value in context.take_persistent_cell_owners() { values.release(value).unwrap(); }
            for (id, owners) in &values.cell_owners {
                assert_eq!(*owners, 0, "exact={exact} vm={vm}: {:?}", values.values[id]);
            }
        }
    }
}
