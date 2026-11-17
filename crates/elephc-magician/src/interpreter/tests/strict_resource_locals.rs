//! Purpose:
//! Verifies resource producer contracts through walker and VM stores and reference aliases.
//!
//! Called from:
//! - Focused interpreter strict local origin regressions.
//!
//! Key details:
//! - False remains a valid producer alternative; rejected values and every cell owner are preserved.
//! - VM alias cases enter an admitted value wrapper; referenced writes use the shared callee.

use super::super::*;
use super::{support::*, strict_semantics::staged_numeric_execution};
use elephc_semantics::{SemanticProfile, StrictFeature};

/// Both engines enforce resource-or-false origins before direct and reference publication.
#[test]
fn strict_resource_locals_eval_producer_aliases_and_owners() {
    for vm in [false, true] {
        for strict in [false, true] {
            for source in ["$value=1;", "$value=false;", "$value=true;",
                "replaceResourceCandidate($value,1);", "replaceResourceCandidate($value,false);",
                "replaceResourceCandidate($value,true);"] {
                let mut context = ElephcEvalContext::new();
                context.install_semantic_profile(SemanticProfile::PERMISSIVE.with(StrictFeature::Locals, strict)).unwrap();
                let mut scope = ElephcEvalScope::new(); let mut values = FakeOps::default();
                assert!(context.set_global_scope(&mut scope));
                staged_numeric_execution("function replaceResourceCandidate(mixed &$target,mixed $next): void {$target=$next;} \
                    function resourceAliasDriver(mixed $next): void {global $value;replaceResourceCandidate($value,$next);} \
                    $value=fopen('php://temp','w+');$owner=$value;", false, &mut context, &mut scope, &mut values).unwrap();
                let before = values.get(scope.visible_cell("value").unwrap());
                let execution_source = if vm && source.starts_with("replaceResourceCandidate") {
                    let call = source.replace("replaceResourceCandidate($value,", "resourceAliasDriver(");
                    format!("if(false) {{function resourceAliasDriver($next) {{}}}} {call}")
                } else { source.to_string() };
                let result = staged_numeric_execution(&execution_source, vm, &mut context, &mut scope, &mut values);
                if strict && !source.contains("false") {
                    assert!(matches!(result, Err(EvalStatus::UncaughtThrowable)), "{source}, vm={vm}: {:?}", result.as_ref().err());
                    assert_eq!(values.get(scope.visible_cell("value").unwrap()), before, "{source}, vm={vm}");
                    let error = context.take_pending_throw().expect("resource TypeError owner");
                    assert!(values.object_is_a(error, "TypeError", false).unwrap()); values.release(error).unwrap();
                } else { result.unwrap_or_else(|status| panic!("{source}, vm={vm}: {status:?}")); }
                staged_numeric_execution("fclose($owner);", false, &mut context, &mut scope, &mut values).unwrap();
                for value in scope.drain_owned_cells() { values.release(value).unwrap(); }
                for value in context.take_persistent_cell_owners() { values.release(value).unwrap(); }
                for (id, owners) in &values.cell_owners { assert_eq!(*owners, 0, "{source}, vm={vm}: {:?}", values.values[id]); }
            }
        }
    }
}
