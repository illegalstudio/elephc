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


/// Every independent element domain retains its explicit aggregate policy through all target emitters.
#[test]
fn strict_numeric_aggregate_domains_frontend_policies_on_all_targets() {
    use crate::ir::Op;
    let source = "<?php function domainSum(mixed $items): mixed { return array_sum($items); } function domainProduct(mixed $items): mixed { return array_product($items); } $sum = array_sum(...); echo $sum([$argc, 2]);";
    for feature in [StrictFeature::Arithmetic, StrictFeature::Coercions] {
        for overflow in [false, true] {
            let profile = SemanticProfile::PERMISSIVE.with(feature, true).with(StrictFeature::Overflow, overflow);
            for mode in [SourceMode::Php, SourceMode::Lfc, SourceMode::Internal] {
                let program = crate::parser::parse_with_mode(&crate::lexer::tokenize(source).unwrap(), mode).unwrap();
                for name in ["macos-aarch64", "ios-arm64", "ios-sim-arm64", "linux-aarch64", "linux-x86_64"] {
                    let target = crate::codegen::platform::Target::parse(name).unwrap();
                    let checked = crate::types::checker::check_staged_semantics(&program, target, profile).unwrap();
                    let module = crate::ir_lower::lower_program(&program, &checked, target, false).unwrap();
                    for optimize in [false, true] {
                        let mut module = module.clone(); if optimize { crate::ir_passes::optimize_module(&mut module); }
                        for name in ["domainSum", "domainProduct"] {
                            let function = module.functions.iter().find(|function| function.name == name).unwrap();
                            assert_eq!(function.instructions.iter().any(|inst| inst.op == Op::RuntimeCall && inst.arithmetic_policy().has_numeric_domain()),
                                mode != SourceMode::Internal, "{feature:?}, {mode:?}, overflow={overflow}");
                        }
                        crate::ir::validate_module(&module).unwrap();
                        crate::codegen::generate_user_asm_from_ir(&module, false, false).unwrap();
                    }
                }
            }
        }
    }
}

/// Known invalid final entries are diagnosed even beside unknown values, while overwrite proofs remain exact.
#[test]
fn strict_numeric_aggregate_domains_source_proofs_and_element_contracts() {
    let target = crate::codegen::platform::Target::detect_host();
    for feature in [StrictFeature::Arithmetic, StrictFeature::Coercions] {
        let profile = SemanticProfile::PERMISSIVE.with(feature, true);
        for source in ["<?php if (false) { array_sum(['2']); }", "<?php if (false) { array_product([$argc, '2']); }",
            "<?php array_sum([true]);", "<?php array_product([null]);", "<?php array_sum([[]]);",
            "<?php const ITEMS = ['1' => '2', 1 => '3']; array_sum(ITEMS);",
            "<?php function bad(array<string> $items): mixed { return array_sum($items); }",
            "<?php class Bad {} function bad(array<Bad> $items): mixed { return array_product($items); }"] {
            let program = crate::parser::parse(&crate::lexer::tokenize(source).unwrap()).unwrap();
            let error = crate::types::checker::check_staged_semantics(&program, target, profile).expect_err(source);
            assert!(error.message.contains(feature.flag()), "{source}: {error:?}");
        }
        let source = "<?php echo array_sum([]), array_product([]), array_sum([1, 2.5]); echo array_sum([0 => 'bad', 0 => $argc]); echo array_product([true => 'bad', 1 => 2]);";
        let program = crate::parser::parse(&crate::lexer::tokenize(source).unwrap()).unwrap();
        crate::types::checker::check_staged_semantics(&program, target, profile).unwrap();
    }
}

/// Native Number cast contracts precede scalar guards in every target's aggregate program.
#[test]
fn strict_numeric_aggregate_domains_native_number_contracts_on_all_targets() {
    let source = "<?php function typedGmpSum(array<GMP> $items): mixed { return array_sum($items); } echo typedGmpSum([gmp_init('2')]);";
    let program = crate::parser::parse(&crate::lexer::tokenize(source).unwrap()).unwrap();
    let mut inventory = crate::optimize::reachability::PreludeInventory::new();
    let program = crate::core_prelude::inject_if_used(program, &mut inventory);
    let program = crate::gmp_prelude::inject_if_used(program, false, &mut inventory);
    let program = crate::name_resolver::resolve(program).unwrap();
    let profile = SemanticProfile::PERMISSIVE.with(StrictFeature::Arithmetic, true)
        .with(StrictFeature::Coercions, true).with(StrictFeature::Overflow, true);
    for name in ["macos-aarch64", "ios-arm64", "ios-sim-arm64", "linux-aarch64", "linux-x86_64"] {
        let target = crate::codegen::platform::Target::parse(name).unwrap();
        let checked = crate::types::checker::check_staged_semantics(&program, target, profile).unwrap();
        let module = crate::ir_lower::lower_program(&program, &checked, target, false).unwrap();
        for optimize in [false, true] {
            let mut module = module.clone(); if optimize { crate::ir_passes::optimize_module(&mut module); }
            crate::ir::validate_module(&module).unwrap();
            crate::codegen::generate_user_asm_from_ir(&module, false, false).unwrap();
        }
    }
}
