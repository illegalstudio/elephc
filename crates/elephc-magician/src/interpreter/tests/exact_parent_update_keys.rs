//! Purpose:
//! Checks exact compound keys before parent promotion and nested null publication.
//!
//! Called from:
//! - Focused staged walker and bytecode semantic-profile regressions.
//!
//! Key details:
//! - Invalid update keys leave null and false parents untouched without a deprecation.
//! - A valid outer key publishes a null child before an invalid leaf key is rejected.

use super::super::*;
use super::strict_semantics::staged_numeric_execution;
use super::support::*;
use elephc_semantics::{SemanticProfile, StrictFeature};

/// Both eval engines preserve compound parents, nested publication order and owner balance.
#[test]
fn exact_eval_parent_update_keys_walker_vm_order_and_owners() {
    let source = include_str!("../../../../../tests/fixtures/strict/eval_parent_update_key_guards.php")
        .strip_prefix("<?php\n").unwrap();
    for vm in [false, true] {
        let profile = SemanticProfile::PERMISSIVE.with(StrictFeature::ArrayKeys, true);
        let mut context = ElephcEvalContext::new();
        context.install_semantic_profile(profile).unwrap();
        let mut scope = ElephcEvalScope::new();
        let mut values = FakeOps::default();
        values.fixed_profile = Some(profile);
        assert!(context.set_global_scope(&mut scope));
        staged_numeric_execution(source, false, &mut context, &mut scope, &mut values).unwrap();
        let mut expected = String::new();
        for place in 0..4 {
            for (parent, kind) in [("null", "NULL"), ("false", "boolean")] {
                let mut call = format!("evalParentUpdateCase({place}, {parent}, false);");
                if vm {
                    call = format!(
                        "if(false){{function evalParentUpdateCase($place,$parent,$key){{}}}} {call}"
                    );
                }
                let result = staged_numeric_execution(
                    &call, vm, &mut context, &mut scope, &mut values,
                );
                assert!(result.is_ok(), "vm={vm} place={place} parent={parent}: {:?}; {}",
                    result.err(), values.output);
                expected.push_str(&format!("E|{kind}|"));
            }
        }
        assert!(values.warnings.is_empty(),
            "compound parent conversion ran before the key guard: vm={vm}: {:?}", values.warnings);
        for (child, outer, leaf, kind) in [
            ("null", "0", "false", "array"),
            ("false", "0", "false", "boolean"),
            ("null", "false", "0", "NULL"),
            ("false", "false", "0", "boolean"),
        ] {
            let mut call = format!("evalParentLeafCase({child}, {outer}, {leaf});");
            if vm {
                call = format!(
                    "if(false){{function evalParentLeafCase($child,$outerKey,$leafKey){{}}}} {call}"
                );
            }
            let result = staged_numeric_execution(
                &call, vm, &mut context, &mut scope, &mut values,
            );
            assert!(result.is_ok(), "vm={vm} child={child} outer={outer}: {:?}; {}",
                result.err(), values.output);
            expected.push_str(&format!("E|{kind}|"));
        }
        assert!(values.warnings.is_empty(),
            "invalid leaf conversion ran before the key guard: vm={vm}: {:?}", values.warnings);
        for child in ["null", "false"] {
            let mut valid_call = format!("evalParentValidLeafCase({child});");
            if vm {
                valid_call = format!(
                    "if(false){{function evalParentValidLeafCase($child){{}}}} {valid_call}"
                );
            }
            staged_numeric_execution(
                &valid_call, vm, &mut context, &mut scope, &mut values,
            ).unwrap();
            expected.push_str("array:7|");
        }
        assert_eq!(values.output, expected, "vm={vm}");
        assert_eq!(values.warnings, vec![
            "Deprecated: Automatic conversion of false to array is deprecated in  on line 0\n".to_string(),
        ], "vm={vm}");
        for value in scope.drain_owned_cells() { values.release(value).unwrap(); }
        for value in context.take_persistent_cell_owners() { values.release(value).unwrap(); }
        for (id, owners) in &values.cell_owners {
            assert_eq!(*owners, 0, "vm={vm}: {:?}", values.values[id]);
        }
    }
}
