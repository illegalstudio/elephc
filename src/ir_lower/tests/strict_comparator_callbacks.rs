//! Purpose:
//! Verifies source-owned comparator return policy across EIR optimization and targets.
//!
//! Called from:
//! - Focused compiler library tests after comparator callback integration.
//!
//! Key details:
//! - A generated callback consumer keeps the original invocation's policy.
//! - Known integer callbacks remain eligible for their direct native ABI.

use crate::{codegen_support::platform::Target, ir::RuntimeFnId, source::SourceMode};
use elephc_semantics::{SemanticProfile, StrictFeature};

/// Static callbacks preserve checked return requirements independently of the physical emitter.
#[test]
fn strict_comparison_comparator_eir_source_policy_and_all_targets() {
    let profile = SemanticProfile::PERMISSIVE.with(StrictFeature::Comparisons, true);
    let source = "<?php function exactComparator(int $left, int $right): mixed { return $left <=> $right; } \
        $items = [2, 1]; usort($items, exactComparator(...));";
    for mode in [SourceMode::Php, SourceMode::Internal] {
        let program = crate::parser::parse_with_mode(&crate::lexer::tokenize(source).unwrap(), mode).unwrap();
        for name in ["macos-aarch64", "ios-arm64", "ios-sim-arm64", "linux-aarch64", "linux-x86_64"] {
            let target = Target::parse(name).unwrap();
            let checked = crate::types::checker::check_staged_semantics(&program, target, profile).unwrap();
            let module = crate::ir_lower::lower_program(&program, &checked, target, false).unwrap();
            for optimize in [false, true] {
                let mut module = module.clone();
                if optimize { crate::ir_passes::optimize_module(&mut module); }
                let call = module.functions.iter().flat_map(|function| &function.instructions)
                    .find(|instruction| instruction.runtime_function() == Some(RuntimeFnId::Usort))
                    .expect("user-sort invocation survives optimization");
                assert_eq!(call.strict_comparisons(), mode != SourceMode::Internal,
                    "{name} {mode:?} opt={optimize}");
                if call.strict_comparisons() {
                    assert!(call.effects.contains(crate::ir::Effects::MAY_THROW), "{name}: comparator guard can throw");
                }
                crate::ir::validate_module(&module).unwrap();
                let assembly = crate::codegen::generate_user_asm_from_ir(&module, false, false).unwrap();
                assert_eq!(assembly.contains("__rt_strict_comparator_owned"), mode != SourceMode::Internal,
                    "{name} {mode:?} opt={optimize}");
            }
        }
    }
}
