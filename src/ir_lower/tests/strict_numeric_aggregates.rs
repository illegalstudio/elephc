//! Purpose:
//! Verifies explicit checked aggregate contracts and numeric fusion boundaries.
//!
//! Called from:
//! - Focused AST-to-EIR regression tests.
//!
//! Key details:
//! - All target ABIs and source modes retain separate PHP and checked paths.
//! - Legacy typed sum and fused-map kernels cannot erase a checked operation policy.

use crate::ir::{Immediate, RuntimeCallTarget, RuntimeFnId};
use crate::source::SourceMode;
use elephc_semantics::{SemanticProfile, StrictFeature};

/// Checked direct/callable reductions survive both optimizers without PHP-promoting shortcuts.
#[test]
fn strict_numeric_aggregates_retain_policies_and_disable_promoting_fusions_on_all_targets() {
    let source = "<?php function strict_sum_probe(mixed $items): mixed { return array_sum($items); } function strict_product_probe(mixed $items): mixed { return array_product($items); } $sum = array_sum(...); $product = array_product(...); $ints = [$argc, 1]; echo strict_sum_probe($ints); echo strict_product_probe($ints); echo $sum($ints); echo $product($ints); echo (int) array_sum($ints); echo array_reduce($ints, fn($carry, $item) => $carry + $item, 0); echo array_sum(array_map(fn($item) => $item + 1, $ints)); echo array_sum(array_map(fn(int $item): int => $item + 1, $ints));";
    let tokens = crate::lexer::tokenize(source).unwrap();
    for mode in [SourceMode::Php, SourceMode::Lfc, SourceMode::Internal] {
        let program = crate::parser::parse_with_mode(&tokens, mode).unwrap();
        for checked in [false, true] {
            for name in ["macos-aarch64", "ios-arm64", "ios-sim-arm64", "linux-aarch64", "linux-x86_64"] {
                let target = crate::codegen::platform::Target::parse(name).unwrap();
                let mut types = crate::types::check_with_target(&program, target).unwrap();
                types.semantic_profile = SemanticProfile::PERMISSIVE.with(StrictFeature::Overflow, checked);
                for optimize in [false, true] {
                    let mut module = crate::ir_lower::lower_program(&program, &types, target, false).unwrap();
                    if optimize { crate::ir_passes::optimize_module(&mut module); }
                    crate::ir::validate_module(&module).unwrap();
                    for (probe, operation) in [("strict_sum_probe", RuntimeFnId::ArraySum), ("strict_product_probe", RuntimeFnId::ArrayProduct)] {
                        let function = module.functions.iter().find(|function| function.name == probe).unwrap();
                        assert_eq!(function.instructions.iter().any(|instruction| matches!(instruction.immediate,
                            Some(Immediate::RuntimeCall(RuntimeCallTarget::NumericFunction { target, .. })) if target == operation)),
                            checked && mode != SourceMode::Internal, "{name} {mode:?} {optimize} {probe}");
                    }
                    if checked && mode != SourceMode::Internal {
                        assert!(!module.functions.iter().flat_map(|function| &function.instructions).any(|instruction|
                            matches!(instruction.runtime_function(), Some(RuntimeFnId::ArraySumInt | RuntimeFnId::ArrayMapSumAdd))),
                            "{name} {optimize}: an unchecked kernel replaced checked arithmetic");
                        let mut mismatched = module.clone();
                        mismatched.semantic_profile = SemanticProfile::PERMISSIVE;
                        assert!(crate::ir::validate_module(&mismatched).is_err(), "{name}");
                    }
                    crate::codegen::generate_user_asm_from_ir(&module, false, false).unwrap();
                }
            }
        }
    }
}
