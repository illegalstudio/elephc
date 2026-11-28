//! Purpose:
//! Checks property reference sources after their creating call scopes retire.
//!
//! Called from:
//! - Focused walker and VM regressions for persistent property aliases.
//!
//! Key details:
//! - Scalar and nested array sources share actual cells across multiple escaped owners.

use super::super::*;
use super::{support::*, strict_semantics::staged_numeric_execution};
use elephc_semantics::{SemanticProfile, StrictFeature};

/// Escaped property aliases keep their source cells alive independently of conversion policies.
#[test]
fn escaped_property_sources_walker_vm_profiles_and_owners() {
    let source = include_str!("../../../../../tests/fixtures/strict/escaped_promoted_aliases.php")
        .strip_prefix("<?php\n").unwrap();
    for exact in [false, true] {
        for casts in [false, true] {
            for coercions in [false, true] {
                for vm in [false, true] {
                    let profile = SemanticProfile::PERMISSIVE.with(StrictFeature::ArrayKeys, exact)
                        .with(StrictFeature::Casts, casts).with(StrictFeature::Coercions, coercions);
                    let mut context = ElephcEvalContext::new(); context.install_semantic_profile(profile).unwrap();
                    let mut scope = ElephcEvalScope::new(); let mut values = FakeOps::default();
                    values.fixed_profile = Some(profile); assert!(context.set_global_scope(&mut scope));
                    staged_numeric_execution(source, false, &mut context, &mut scope, &mut values).unwrap();
                    let call = if vm { "if(false){function escapedPromotedObservations(){}} escapedPromotedObservations();" }
                        else { "escapedPromotedObservations();" };
                    let result = staged_numeric_execution(call, vm, &mut context, &mut scope, &mut values);
                    assert!(result.is_ok(), "exact={exact} casts={casts} coercions={coercions} vm={vm}: {:?}; {}", result.err(), values.output);
                    assert_eq!(values.output, "3/7/11|4/8/12|5/9/13|");
                    for value in scope.drain_owned_cells() { values.release(value).unwrap(); }
                    for value in context.take_persistent_cell_owners() { values.release(value).unwrap(); }
                    for (id, owners) in &values.cell_owners { assert_eq!(*owners, 0, "{:?}", values.values[id]); }
                }
            }
        }
    }
}
