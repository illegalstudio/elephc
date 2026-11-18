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

/// Both execution entry points preserve new null sources, old aliases and balanced owners.
#[test]
fn fresh_reference_sources_eval_values_and_owners() {
    let source = include_str!("../../../../../tests/fixtures/strict/fresh_reference_sources.php")
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
            let expected = if locals { "NULL|old|NULL|old:new|Tstring|integer|old:4|NULL|alias|NULL:alias|9|sorted|NULL|self|NULL:NULL|seen|TNULL|" }
                else { "NULL|old|NULL|old:new|integer|integer|3:4|NULL|alias|NULL:alias|9|sorted|NULL|self|NULL:NULL|seen|Wstring|" };
            assert_eq!(values.output, expected, "locals={locals} dispatch={dispatch}");
            for value in scope.drain_owned_cells() { values.release(value).unwrap(); }
            for (id, owners) in &values.cell_owners { assert_eq!(*owners, 0, "locals={locals} dispatch={dispatch}: {:?}", values.values[id]); }
        }
    }
}

