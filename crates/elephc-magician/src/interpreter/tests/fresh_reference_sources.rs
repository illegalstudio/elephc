//! Purpose:
//! Checks fresh reference bindings and displaced aliases in the eval engines.
//!
//! Called from:
//! - Focused strict-local source-creation parity tests.
//!
//! Key details:
//! - Forming a reference initializes a missing name without closing its producer contract.

use super::super::*;
use super::{support::*, strict_semantics::staged_numeric_execution};
use elephc_semantics::{SemanticProfile, StrictFeature};

/// Checks both eval entry points, profiles and complete scope-owner retirement.
fn assert_reference_fixture(source: &str, checked: &str, permissive: &str) {
    let source = source.strip_prefix("<?php").unwrap();
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
            assert_eq!(values.output, if locals { checked } else { permissive }, "locals={locals} dispatch={dispatch}");
            for value in scope.drain_owned_cells() { values.release(value).unwrap(); }
            for (id, owners) in &values.cell_owners { assert_eq!(*owners, 0, "locals={locals} dispatch={dispatch}: {:?}", values.values[id]); }
        }
    }
}

/// Both execution entry points preserve new null sources, old aliases and balanced owners.
#[test]
fn fresh_reference_sources_eval_values_and_owners() {
    assert_reference_fixture(include_str!("../../../../../tests/fixtures/strict/fresh_reference_sources.php"),
        "NULL|old|NULL|old:new|Tstring|integer|old:4|NULL|alias|NULL:alias|9|sorted|NULL|self|NULL:NULL|seen|TNULL|",
        "NULL|old|NULL|old:new|integer|integer|3:4|NULL|alias|NULL:alias|9|sorted|NULL|self|NULL:NULL|seen|Wstring|");
}

/// Known reference arguments publish their source before subsequent operands in both engines.
#[test]
fn fresh_reference_arguments_eval_values_and_owners() {
    let prefix = "bound|bound|bound|closure|NULL:NULL|observed|present:NULL|observed|formed|closure-formed|later:string|observed|positional:named|NULL|private|NULL|";
    assert_reference_fixture(include_str!("../../../../../tests/fixtures/strict/fresh_reference_arguments.php"),
        &format!("{prefix}Tinteger|"), &format!("{prefix}Wstring|"));
}

/// Member, constructor and expression calls preserve skipped scopes and reference identities.
#[test]
fn fresh_member_reference_arguments_eval_values_and_owners() {
    let prefix = "method|static|present:NULL|seen|constructor|immediate|constructor|method|method|absent|present|first:last|formed|changed:string|seen|null:absent|null:present|";
    assert_reference_fixture(include_str!("../../../../../tests/fixtures/strict/fresh_member_reference_arguments.php"),
        &format!("{prefix}Tinteger|"), &format!("{prefix}Wstring|"));
}

/// Reference exposure keeps exclusive first producers, including open and numeric alternatives.
#[test]
fn reference_binding_alternatives_eval_values_and_owners() {
    assert_reference_fixture(include_str!("../../../../../tests/fixtures/strict/reference_binding_alternatives.php"),
        "Tinteger:1|Tstring:before|Winteger:2|Wstring:after|Tinteger:1|Tstring:before|Tinteger:1|Tstring:before|Tinteger:1|Tstring:before|Tinteger:1|Tdouble:2.5|Wdouble:3.5|Wstring:after|Tinteger:1|Tinteger:1|Tstring:before|Winteger:2|",
        "Wstring:after|Winteger:2|Winteger:2|Wstring:after|Wstring:after|Winteger:2|Wstring:after|Winteger:2|Wstring:after|Winteger:2|Wdouble:2.5|Winteger:1|Wdouble:3.5|Wstring:after|Wstring:after|Wstring:after|Winteger:2|Winteger:2|");
}

/// Incoming parameter contracts end only on the executed unset path in either eval entry point.
#[test]
fn conditional_parameter_origins_eval_values_and_owners() {
    assert_reference_fixture(include_str!("../../../../../tests/fixtures/strict/conditional_parameter_origins.php"),
        "Tstring:after|Tinteger:1|Wstring:ready|Winteger:3|Winteger|TNULL|Tstring|Wdouble|Tstring|Wstring|Tstring|Wobject|Tobject|Tstring|Warray|Tstring|1|Winteger|1|Tstring|closed|Tstring|Tinteger|Tstring|Tinteger|outer|present|absent|Warray|Tstring|",
        "Winteger:2|Wstring:bad|Wstring:ready|Winteger:3|Winteger|Wdouble|Winteger|Wdouble|Wdouble|Wstring|Winteger|Wobject|Wobject|Wobject|Warray|Warray|1|Winteger|1|Winteger|closed|Winteger|Wstring|Winteger|Wstring|outer|present|absent|Warray|Warray|");
}

/// Eval-visible binding ends keep escaped old cells separate from newly created names.
#[test]
fn eval_binding_ends_eval_values_and_owners() {
    assert_reference_fixture(include_str!("../../../../../tests/fixtures/strict/eval_binding_ends.php"),
        "1:string:after|Tinteger:1|Wstring:1|NULL:new|NULL:new|",
        "1:string:after|Wstring:after|Wstring:1|NULL:new|NULL:new|");
}
