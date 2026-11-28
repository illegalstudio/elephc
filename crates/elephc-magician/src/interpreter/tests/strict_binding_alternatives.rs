//! Purpose:
//! Checks local binding alternatives and independent copies in both eval engines.
//!
//! Called from:
//! - Focused interpreter semantic-profile tests.
//!
//! Key details:
//! - Unset ends a binding, while an imported or explicitly mixed producer remains open.

use super::super::*;
use super::{support::*, strict_semantics::staged_numeric_execution};
use elephc_semantics::{SemanticProfile, StrictFeature};

/// Walker and normal execution preserve first-path contracts, detached copies and owner cleanup.
#[test]
fn strict_binding_alternatives_walker_dispatch_values_and_owners() {
    let source = include_str!("../../../../../tests/fixtures/strict/local_binding_alternatives.php")
        .strip_prefix("<?php").unwrap();
    for dispatch in [false, true] {
        for locals in [false, true] {
            let mut context = ElephcEvalContext::new();
            context.install_semantic_profile(SemanticProfile::PERMISSIVE.with(StrictFeature::Locals, locals)).unwrap();
            let mut scope = ElephcEvalScope::new(); let mut values = FakeOps::default();
            if dispatch {
                let program = crate::parser::parse_fragment_with_semantics(source.as_bytes(), context.semantic_profile()).unwrap();
                let result = execute_program_with_context(&mut context, &program, &mut scope, &mut values).unwrap();
                values.release(result).unwrap();
            } else {
                staged_numeric_execution(source, false, &mut context, &mut scope, &mut values).unwrap();
            }
            let expected = if locals { "integer|string|string|integer|Winteger|Tstring|Tinteger|Wstring|Winteger|Tstring|string|string|string|integer|Winteger|Tdouble|TNULL|Wstring|Tstring|Winteger|Tstring|Tstring|" }
                else { "integer|string|string|integer|Winteger|Winteger|Wstring|Wstring|Winteger|Winteger|string|string|string|integer|Winteger|Winteger|Wstring|Wstring|Winteger|Winteger|Winteger|Winteger|" };
            assert_eq!(values.output, expected, "locals={locals} dispatch={dispatch}");
            for value in scope.drain_owned_cells() { values.release(value).unwrap(); }
            for (id, owners) in &values.cell_owners { assert_eq!(*owners, 0, "locals={locals} dispatch={dispatch}: {:?}", values.values[id]); }
        }
    }
}

/// The actual bytecode slice enforces source-path stores and independent copied contracts.
#[test]
fn strict_binding_alternatives_vm_branch_stores_and_owners() {
    for (source, target, rejected_pick, success, previous) in [
        ("if($pick) {$value=1;} else {$value='s';} $value=2;", "value", Some(false), FakeValue::Int(2), FakeValue::String("s".into())),
        ("if($pick) {$value=1;} $value='s';", "value", Some(true), FakeValue::String("s".into()), FakeValue::Int(1)),
        ("if($pick) {$value=1;} else {$value='s';} $copy=$value; $copy=2;", "copy", Some(false), FakeValue::Int(2), FakeValue::String("s".into())),
        ("$value=$pick ? 1 : 's'; $value=2;", "value", None, FakeValue::Int(2), FakeValue::Null),
        ("$value=$input; if($pick) {$value='open';}", "value", None, FakeValue::Int(1), FakeValue::Null),
        ("if($pick) {$value=1;} else {$value=2.5;} $value=2;", "value", Some(false), FakeValue::Int(2), FakeValue::Float(2.5)),
        ("if($pick) {$value=null;} else {$value='s';} $value='new';", "value", Some(true), FakeValue::String("new".into()), FakeValue::Null),
    ] {
        for vm in [false, true] {
            for locals in [false, true] {
                for pick in [false, true] {
                    let mut context = ElephcEvalContext::new();
                    context.install_semantic_profile(SemanticProfile::PERMISSIVE.with(StrictFeature::Locals, locals)).unwrap();
                    let mut scope = ElephcEvalScope::new(); let mut values = FakeOps::default();
                    let flag = values.bool_value(pick).unwrap();
                    scope.set("pick", flag, ScopeCellOwnership::Owned);
                    let input = values.int(1).unwrap();
                    scope.set("input", input, ScopeCellOwnership::Owned);
                    let rejects = locals && rejected_pick == Some(pick);
                    let result = staged_numeric_execution(source, vm, &mut context, &mut scope, &mut values);
                    if rejects {
                        assert!(matches!(result, Err(EvalStatus::UncaughtThrowable)), "{source}, vm={vm} pick={pick}");
                        let thrown = context.take_pending_throw().unwrap();
                        assert!(values.object_is_a(thrown, "TypeError", false).unwrap());
                        values.release(thrown).unwrap();
                    } else { result.unwrap(); }
                    let actual = values.get(scope.visible_cell(target).unwrap());
                    let expected = if rejects { previous.clone() }
                        else if source.contains("$input") && pick { FakeValue::String("open".into()) }
                        else { success.clone() };
                    assert_eq!(actual, expected, "{source}, vm={vm} locals={locals} pick={pick}");
                    for value in scope.drain_owned_cells() { values.release(value).unwrap(); }
                    for (id, owners) in &values.cell_owners { assert_eq!(*owners, 0, "{source}, vm={vm}: {:?}", values.values[id]); }
                }
            }
        }
    }
}
