//! Purpose:
//! Checks property and static array parents before key errors and warning callbacks.
//!
//! Called from:
//! - Focused staged walker and VM semantic profile regressions.
//!
//! Key details:
//! - The shared PHP fixture checks callback-visible publication and owner retirement.

use super::super::*;
use super::support::*;
use super::strict_semantics::staged_numeric_execution;
use elephc_semantics::{SemanticProfile, StrictFeature};

/// Both eval engines preserve deferred write completion, original key kinds and owner balance.
#[test]
fn false_parent_completion_walker_vm_key_priority_and_owners() {
    let source = include_str!("../../../../../tests/fixtures/strict/false_parent_completion.php").strip_prefix("<?php\n").unwrap();
    for (exact, coordinated) in [(false, false), (false, true), (true, false), (true, true)] {
        for vm in [false, true] {
            let profile = SemanticProfile::PERMISSIVE.with(StrictFeature::ArrayKeys, exact).with(StrictFeature::Casts, coordinated);
            let mut context = ElephcEvalContext::new(); context.install_semantic_profile(profile).unwrap();
            let mut scope = ElephcEvalScope::new(); let mut values = FakeOps::default(); values.fixed_profile = Some(profile);
            assert!(context.set_global_scope(&mut scope));
            staged_numeric_execution(source, false, &mut context, &mut scope, &mut values).unwrap();
            let mut expected = String::new();
            for place in 0..4 {
                for (key, kind, text, invalid, completes) in [
                    ("0", "integer", "0", false, true), ("'a'", "string", "a", false, true),
                    ("'0'", if exact { "string" } else { "integer" }, "0", false, true),
                    ("true", "integer", "1", true, true), ("null", "", "", true, false),
                    ("1.0", "", "", true, false), ("[]", "", "", true, true),
                    ("new PendingParent()", "", "", true, true),
                ] {
                    staged_numeric_execution(&format!("$pendingInput = {key};"), false,
                        &mut context, &mut scope, &mut values).unwrap();
                    let mut call = format!("pendingCase({place}, $pendingInput, 0);");
                    if vm { call = format!("if(false){{function pendingCase($place,$key,$operation){{}}}} {call}"); }
                    staged_numeric_execution(&call, vm, &mut context, &mut scope, &mut values).unwrap();
                    expected.push_str("K|R|W|");
                    let type_error = completes && invalid && (exact || kind.is_empty());
                    expected.push_str(if type_error { "TypeError|new|handler:same|older:same|" } else { "Exception|same|older:same|" });
                    if completes && !type_error { expected.push_str(&format!("array:1|{kind}:{text}:9|")); }
                    else { expected.push_str("array:0|"); }
                }
                let mut call = format!("pendingCase({place}, null, 1); pendingCase({place}, 0, 2);");
                if vm { call = format!("if(false){{function pendingCase($place,$key,$operation){{}}}} {call}"); }
                staged_numeric_execution(&call, vm, &mut context, &mut scope, &mut values).unwrap();
                expected.push_str("R|W|Exception|same|older:same|array:1|integer:0:9|");
                expected.push_str("K|R|W|Exception|same|older:same|array:0|");
            }
            assert_eq!(values.output, expected, "exact={exact} casts={coordinated} vm={vm}");
            assert!(values.warnings.is_empty(), "{:?}", values.warnings);
            for value in scope.drain_owned_cells() { values.release(value).unwrap(); }
            for value in context.take_persistent_cell_owners() { values.release(value).unwrap(); }
            for (id, owners) in &values.cell_owners { assert_eq!(*owners, 0, "exact={exact} casts={coordinated} vm={vm}: {:?}", values.values[id]); }
        }
    }
}
