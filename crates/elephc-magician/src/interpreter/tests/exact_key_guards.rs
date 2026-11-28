//! Purpose:
//! Checks catchable original key membership at eval array boundaries.
//!
//! Called from:
//! - Focused staged walker and bytecode semantic-profile regressions.
//!
//! Key details:
//! - ArrayAccess receives its original kinds and every exceptional operand owner retires.

use super::super::*;
use super::support::*;
use super::strict_semantics::staged_numeric_execution;
use elephc_semantics::{SemanticProfile, StrictFeature};

/// Both eval engines reject original non-key kinds while keeping array and object receivers distinct.
#[test]
fn exact_eval_array_key_guards_walker_vm_types_and_owners() {
    let source = include_str!("../../../../../tests/fixtures/strict/eval_array_key_guards.php")
        .strip_prefix("<?php\n").unwrap();
    for exact in [false, true] {
        for vm in [false, true] {
            let profile = SemanticProfile::PERMISSIVE.with(StrictFeature::ArrayKeys, exact);
            let mut context = ElephcEvalContext::new(); context.install_semantic_profile(profile).unwrap();
            let mut scope = ElephcEvalScope::new(); let mut values = FakeOps::default();
            values.fixed_profile = Some(profile); assert!(context.set_global_scope(&mut scope));
            staged_numeric_execution(source, false, &mut context, &mut scope, &mut values).unwrap();
            let mut expected = String::new();
            for (argument, kind, read) in [("true", "boolean", "20"), ("0.0", "double", "10"),
                ("null", "NULL", "30"), ("0", "integer", "10"), ("''", "string", "30")]
            {
                let call = if vm { format!("if(false){{function evalArrayKeyGuardObservation($key){{}}}} evalArrayKeyGuardObservation({argument});") }
                    else { format!("evalArrayKeyGuardObservation({argument});") };
                let result = staged_numeric_execution(&call, vm, &mut context, &mut scope, &mut values);
                assert!(result.is_ok(), "exact={exact} vm={vm} {argument}: {:?}; {}", result.as_ref().err(), values.output);
                let rejected = exact && !matches!(kind, "integer" | "string");
                if rejected { expected.push_str("R|W|L|U|P|Q|S|T|"); }
                else { expected.push_str(&format!("{read}|w|l|u|p|q|s|t|")); }
                expected.push_str(&format!("G:{kind}|1|O:{kind}|V:{kind}|"));
            }
            if exact {
                for (argument, kind) in [("[]", "array"), ("new EvalGuardOwnedKey()", "object")] {
                    // Array/object construction can take the VM's normal walker fallback;
                    // load the resulting original value through a VM-supported argument node.
                    staged_numeric_execution(&format!("$guardKey = {argument};"), false,
                        &mut context, &mut scope, &mut values).unwrap();
                    let call = if vm { "if(false){function evalArrayKeyGuardObservation($key){}} evalArrayKeyGuardObservation($guardKey);" }
                        else { "evalArrayKeyGuardObservation($guardKey);" };
                    let result = staged_numeric_execution(call, vm, &mut context, &mut scope, &mut values);
                    assert!(result.is_ok(), "vm={vm} {argument}: {:?}; {}", result.as_ref().err(), values.output);
                    staged_numeric_execution("unset($guardKey);", false, &mut context, &mut scope, &mut values).unwrap();
                    expected.push_str(&format!("R|W|L|U|P|Q|S|T|G:{kind}|1|O:{kind}|V:{kind}|"));
                    if kind == "object" { expected.push_str("D|"); }
                }
            }
            assert_eq!(values.output, expected, "exact={exact} vm={vm}");
            for value in scope.drain_owned_cells() { values.release(value).unwrap(); }
            for value in context.take_persistent_cell_owners() { values.release(value).unwrap(); }
            for (id, owners) in &values.cell_owners {
                assert_eq!(*owners, 0, "exact={exact} vm={vm}: {:?}", values.values[id]);
            }
        }
    }
}
