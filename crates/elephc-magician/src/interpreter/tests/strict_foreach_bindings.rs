//! Purpose:
//! Checks foreach publication order and owners across callbacks and source retirement.
//!
//! Called from:
//! - Focused walker and VM-caller ownership regressions.
//!
//! Key details:
//! - Mixed producers keep these publication tests independent of stronger origin inference.

use super::super::*;
use super::{support::*, strict_semantics::staged_numeric_execution};
use elephc_semantics::{SemanticProfile, StrictFeature};

/// Foreach prepares callbacks once, publishes in PHP order and retires every owner.
#[test]
fn strict_foreach_eval_publication_callbacks_returns_and_owners() {
    let source = include_str!("../../../../../tests/fixtures/strict/foreach_binding_lifetimes.php")
        .strip_prefix("<?php\n").unwrap();
    let expected = "Kafter|A9:after|Vold|B9:after|Told|Xold:after|Uafter|Y9:after|C|Qbefore|P|Zold:before|C|N|P|S5|LJ|64|I|R|";
    for vm in [false, true] {
        for strict in [false, true] {
            let mut context = ElephcEvalContext::new();
            context.install_semantic_profile(SemanticProfile::PERMISSIVE.with(StrictFeature::Locals, strict)).unwrap();
            let mut scope = ElephcEvalScope::new(); let mut values = FakeOps::default();
            assert!(context.set_global_scope(&mut scope));
            let source = if vm {
                let wrapper = format!("function foreach_observations() {{global $foreachValue,$foreachKey;{source}}}");
                staged_numeric_execution(&wrapper, false, &mut context, &mut scope, &mut values).unwrap();
                "if(false){function foreach_observations(){}} foreach_observations();".to_string()
            } else { source.to_string() };
            let result = staged_numeric_execution(&source, vm, &mut context, &mut scope, &mut values);
            assert!(result.is_ok(), "vm={vm}, strict={strict}, output={}: {:?}", values.output, result.as_ref().err());
            assert_eq!(values.output, expected, "vm={vm}, strict={strict}");
            assert!(context.take_pending_throw().is_none());
            for value in scope.drain_owned_cells() { eval_release_value(&mut context, &mut values, value).unwrap(); }
            for value in context.take_persistent_cell_owners() { eval_release_value(&mut context, &mut values, value).unwrap(); }
            for (id, owners) in &values.cell_owners { assert_eq!(*owners, 0, "vm={vm}, strict={strict}: {:?}", values.values[id]); }
        }
    }
}
