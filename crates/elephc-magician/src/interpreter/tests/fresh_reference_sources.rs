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
                let result = execute_program_with_context(&mut context, &program, &mut scope, &mut values)
                    .unwrap_or_else(|error| panic!("locals={locals} dispatch={dispatch}: {error:?}; output={}; pending={:?}",
                        values.output, values.pending_runtime_throwable.map(|cell| values.get(cell))));
                values.release(result).unwrap();
            } else {
                staged_numeric_execution(source, false, &mut context, &mut scope, &mut values)
                    .unwrap_or_else(|error| panic!("locals={locals} dispatch={dispatch}: {error:?}; output={}; pending={:?}",
                        values.output, values.pending_runtime_throwable.map(|cell| values.get(cell))));
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

/// Owned incoming parameter bindings can end without changing escaped cells or caller values.
#[test]
fn eval_parameter_origins_eval_values_and_owners() {
    assert_reference_fixture(include_str!("../../../../../tests/fixtures/strict/eval_parameter_origins.php"),
        "Tinteger:1|Tstring:1|1:new|Winteger|Tstring|integer|string|integer|string|new|1|",
        "Wstring:bad|Winteger:1|1:new|Winteger|Winteger|integer|string|integer|string|new|1|");
}

/// A conditional reference detach preserves the caller's cell and the replacement local origin.
#[test]
fn conditional_reference_origins_eval_values_and_owners() {
    assert_reference_fixture(include_str!("../../../../../tests/fixtures/strict/conditional_reference_origins.php"),
        "Tinteger:2:2|2|Tstring:new:2|2|Winteger|3|Tstring|1|Tstring|string|Tstring|early|Winteger|integer|Tstring|string|",
        "Wstring:bad:bad|bad|Winteger:3:2|2|Winteger|3|Winteger|1|Winteger|integer|Winteger|early|Winteger|integer|Winteger|string|");
}

/// Unannotated inputs remain open while declared inputs and caller-owned aliases retain contracts.
#[test]
fn open_parameter_contracts_eval_values_and_owners() {
    assert_reference_fixture(include_str!("../../../../../tests/fixtures/strict/open_parameter_contracts.php"),
        "integer:2|string:ready|double:2.5|double:3.5|integer:2|Tinteger:1|Tinteger:1|string:new|Tinteger:1|method|static|closure|local:1|stream|1|closed|merged|merged|",
        "integer:2|string:ready|double:2.5|double:3.5|integer:2|Wstring:bad|Wstring:bad|string:new|Wstring:new|method|static|closure|local:1|stream|1|closed|merged|merged|");
}

/// Captured values and references keep the incoming origin until the executed binding end.
#[test]
fn captured_binding_origins_eval_values_and_owners() {
    let prefix = "string:new|integer:1|1|string:new|integer:1|1|string:new|string:new|1|string:new|string:new|1|";
    assert_reference_fixture(include_str!("../../../../../tests/fixtures/strict/captured_binding_origins.php"),
        &format!("{prefix}Winteger|Tstring|W2:integer:2|2|T2:string:new|2|"),
        &format!("{prefix}Winteger|Winteger|W2:integer:2|2|W2:integer:3|2|"));
}

/// Reference captures retain whichever exclusive producer executed before capture formation.
#[test]
fn captured_reference_producers_eval_values_and_owners() {
    assert_reference_fixture(include_str!("../../../../../tests/fixtures/strict/captured_reference_producers.php"),
        "Tinteger:1|integer:1|Tstring:old|string:old|Winteger:3|integer:3|Wstring:ready|string:ready|1:new|old:new|1|",
        "Wstring:new|string:new|Winteger:2|integer:2|Winteger:3|integer:3|Wstring:ready|string:ready|1:new|old:new|1|");
}
