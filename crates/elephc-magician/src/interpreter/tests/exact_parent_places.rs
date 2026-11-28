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

/// Both engines preserve publication and callback writes through declared property slots.
#[test]
fn exact_eval_parent_places_walker_vm_publication_and_owners() {
    let source = include_str!("../../../../../tests/fixtures/strict/array_parent_places.php")
        .strip_prefix("<?php\n").unwrap();
    for (exact, coordinated) in [(false, false), (false, true), (true, false), (true, true)] {
        for vm in [false, true] {
            let profile = SemanticProfile::PERMISSIVE.with(StrictFeature::ArrayKeys, exact).with(StrictFeature::Casts, coordinated);
            let mut context = ElephcEvalContext::new(); context.install_semantic_profile(profile).unwrap();
            let mut scope = ElephcEvalScope::new(); let mut values = FakeOps::default(); values.fixed_profile = Some(profile);
            assert!(context.set_global_scope(&mut scope));
            staged_numeric_execution(source, false, &mut context, &mut scope, &mut values).unwrap();
            let mut expected = String::new();
            for static_parent in [false, true] {
                for false_parent in [false, true] {
                    for (key, invalid, legacy_invalid, text) in [
                        ("0", false, false, "0"), ("true", true, false, "1"),
                        ("[]", true, true, ""), ("new ParentPlace()", true, true, ""),
                    ] {
                        staged_numeric_execution(&format!("$placeInput = {key};"), false,
                            &mut context, &mut scope, &mut values).unwrap();
                        let mut call = format!("placeWrite({static_parent}, {false_parent}, $placeInput, 0);");
                        if vm { call = format!("if(false){{function placeWrite($static,$falseParent,$key,$mutation){{}}}} {call}"); }
                        staged_numeric_execution(&call, vm, &mut context, &mut scope, &mut values).unwrap();
                        expected.push_str("K|R|");
                        if false_parent { expected.push_str("F|array:0|"); }
                        if legacy_invalid || exact && invalid { expected.push_str("T|array:0|"); }
                        else { expected.push_str(&format!("ok|array:1|integer:{text}:9|")); }
                    }
                }
                let mut call = format!("placeWrite({static_parent}, true, 0, 3);");
                if vm { call = format!("if(false){{function placeWrite($static,$falseParent,$key,$mutation){{}}}} {call}"); }
                staged_numeric_execution(&call, vm, &mut context, &mut scope, &mut values).unwrap();
                expected.push_str("K|R|F|array:0|ok|array:1|integer:0:9|");
                expected.push_str("array:1|integer:0:9|");
                for mutation in [1, 2, 4, 5] {
                    let mut call = format!("placeWrite({static_parent}, true, 0, {mutation});");
                    if vm { call = format!("if(false){{function placeWrite($static,$falseParent,$key,$mutation){{}}}} {call}"); }
                    staged_numeric_execution(&call, vm, &mut context, &mut scope, &mut values).unwrap();
                    expected.push_str("K|R|F|array:0|ok|array:1|string:h:7|");
                    if mutation >= 4 { expected.push_str("array:1|string:h:7|"); }
                }
            }
            for mutation in [6, 7] {
                for (key, text) in [("0", "0"), ("true", "1")] {
                    let mut call = format!("placeWrite(false, true, {key}, {mutation});");
                    if vm { call = format!("if(false){{function placeWrite($static,$falseParent,$key,$mutation){{}}}} {call}"); }
                    staged_numeric_execution(&call, vm, &mut context, &mut scope, &mut values).unwrap();
                    let rejected = exact && key == "true";
                    expected.push_str("K|R|F|array:0|");
                    expected.push_str(if rejected { "T|absent|" } else { "ok|absent|" });
                    if mutation == 6 {
                        if rejected { expected.push_str("array:0|"); }
                        else { expected.push_str(&format!("array:1|integer:{text}:9|")); }
                    }
                }
            }
            assert_eq!(values.output, expected, "exact={exact} casts={coordinated} vm={vm}");
            assert!(values.warnings.is_empty(), "{:?}", values.warnings);
            for value in scope.drain_owned_cells() { values.release(value).unwrap(); }
            for value in context.take_persistent_cell_owners() { values.release(value).unwrap(); }
            for (id, owners) in &values.cell_owners { assert_eq!(*owners, 0, "exact={exact} casts={coordinated} vm={vm}: {:?}", values.values[id]); }
        }
    }
}
