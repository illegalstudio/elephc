//! Purpose:
//! Checks empty-parent publication before eval key errors and diagnostic callbacks.
//!
//! Called from:
//! - Focused staged walker and bytecode profile regressions.
//!
//! Key details:
//! - The shared PHP fixture observes original operand order and callback-visible storage.

use super::super::*;
use super::support::*;
use super::strict_semantics::staged_numeric_execution;
use elephc_semantics::{SemanticProfile, StrictFeature};

/// Both engines publish empty parents before key failures and retain handler replacements.
#[test]
fn exact_eval_parent_key_order_walker_vm_publication_and_owners() {
    let source = include_str!("../../../../../tests/fixtures/strict/array_parent_key_order.php")
        .strip_prefix("<?php\n").unwrap();
    for exact in [false, true] {
        for vm in [false, true] {
            let profile = SemanticProfile::PERMISSIVE.with(StrictFeature::ArrayKeys, exact);
            let mut context = ElephcEvalContext::new(); context.install_semantic_profile(profile).unwrap();
            let mut scope = ElephcEvalScope::new(); let mut values = FakeOps::default(); values.fixed_profile = Some(profile);
            assert!(context.set_global_scope(&mut scope));
            staged_numeric_execution(source, false, &mut context, &mut scope, &mut values).unwrap();
            let mut expected = String::new();
            for false_parent in [false, true] {
                for (key, invalid, legacy_invalid, kind, text) in [
                    ("true", true, false, "integer", "1"), ("0.5", true, false, "integer", "0"),
                    ("null", true, false, "string", ""), ("[]", true, true, "", ""),
                    ("new ParentKeyOperand()", true, true, "", ""), ("0", false, false, "integer", "0"),
                    ("'0'", false, false, if exact { "string" } else { "integer" }, "0"),
                    ("'01'", false, false, "string", "01"), ("''", false, false, "string", ""),
                ] {
                    staged_numeric_execution(&format!("$parentInputKey = {key};"), false,
                        &mut context, &mut scope, &mut values).unwrap();
                    let mut call = format!("parentKeyWrite($parentInputKey, {false_parent}, 0);");
                    if vm { call = format!("if(false){{function parentKeyWrite($key,$parent,$mutation){{}}}} {call}"); }
                    let result = staged_numeric_execution(&call, vm, &mut context, &mut scope, &mut values);
                    assert!(result.is_ok(), "exact={exact} vm={vm}: {key}: {:?}; {}", result.err(), values.output);
                    expected.push_str("K|R|");
                    if false_parent { expected.push_str("F:array:0|"); }
                    if legacy_invalid || exact && invalid { expected.push_str("T|array:0|"); }
                    else { expected.push_str(&format!("ok|array:1|{kind}:{text}:9|")); }
                }
            }
            for false_parent in [false, true] {
                for (key, invalid) in [("0", false), ("true", true)] {
                    let mut call = format!("parentKeyWrite({key}, {false_parent}, 3);");
                    if vm { call = format!("if(false){{function parentKeyWrite($key,$parent,$mutation){{}}}} {call}"); }
                    staged_numeric_execution(&call, vm, &mut context, &mut scope, &mut values).unwrap();
                    expected.push_str("K|R|");
                    if false_parent { expected.push_str("F:array:0|"); }
                    if exact && invalid { expected.push_str("T|array:0|"); }
                    else { expected.push_str(&format!("ok|array:1|integer:{}:9|", if invalid { 1 } else { 0 })); }
                }
            }
            for mutation in [1, 2] {
                for (key, invalid) in [("0", false), ("true", true)] {
                    let mut call = format!("parentKeyWrite({key}, true, {mutation});");
                    if vm { call = format!("if(false){{function parentKeyWrite($key,$parent,$mutation){{}}}} {call}"); }
                    let result = staged_numeric_execution(&call, vm, &mut context, &mut scope, &mut values);
                    assert!(result.is_ok(), "exact={exact} vm={vm}: mutation={mutation}: {:?}; {}", result.err(), values.output);
                    expected.push_str("K|R|F:array:0|");
                    expected.push_str(if exact && invalid { "T|" } else { "ok|" });
                    expected.push_str("array:1|string:h:7|");
                }
            }
            let mut copy_check = "parentKeyCopyCheck();".to_string();
            if vm { copy_check = format!("if(false){{function parentKeyCopyCheck(){{}}}} {copy_check}"); }
            staged_numeric_execution(&copy_check, vm, &mut context, &mut scope, &mut values).unwrap();
            expected.push_str("K|R|9:3|");
            assert_eq!(values.output, expected, "exact={exact} vm={vm}");
            // FakeOps's legacy metadata conversion reports fractional-key casts directly. This
            // regression checks parent diagnostics; native gates cover PHP key callbacks.
            let legacy_warnings = if exact { Vec::new() } else {
                vec!["Deprecated: Implicit conversion from float 0.5 to int loses precision\n".to_string(); 2]
            };
            assert_eq!(values.warnings, legacy_warnings);
            for value in scope.drain_owned_cells() { values.release(value).unwrap(); }
            for value in context.take_persistent_cell_owners() { values.release(value).unwrap(); }
            for (id, owners) in &values.cell_owners { assert_eq!(*owners, 0, "exact={exact} vm={vm}: {:?}", values.values[id]); }
        }
    }
}
