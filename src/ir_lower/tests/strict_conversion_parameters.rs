//! Purpose:
//! Verifies original scalar contracts before parameter storage normalization.
//!
//! Called from:
//! - Focused selected-profile frontend and all-target EIR tests.
//!
//! Key details:
//! - Source validation and EIR guards are tested separately from public feature admission.

use elephc_semantics::{SemanticProfile, StrictFeature};
use crate::{source::SourceMode, codegen_support::platform::Target};

/// Known implicit changes fail before permissive assignability can accept them.
#[test]
fn strict_conversion_parameters_reject_known_kinds_after_initial_folding() {
    let profile = SemanticProfile::PERMISSIVE.with(StrictFeature::Coercions, true);
    for source in [
        "<?php declare(strict_types=0); function f(float $v): void {} f(1);",
        "<?php function f(string $v): void {} f(2);",
        "<?php function f(bool $v): void {} f(1);",
        "<?php function f(int $v): void {} f('2');",
        "<?php function f(float $v): void {} f(v: 1);",
        "<?php function f(float $v): void {} f(...[1]);",
        "<?php function f(float ...$v): void {} f(1);",
        "<?php function f(float $v): void {} $call = f(...); $call(1);",
        "<?php function f(bool $v): void { if (false) { pow($v, 2); } }",
        "<?php function f(float $v): void { if (false) { intdiv($v, 2); } }",
    ] {
        for mode in [SourceMode::Php, SourceMode::Lfc] {
            let program = crate::parser::parse_with_mode(&crate::lexer::tokenize(source).unwrap(), mode).unwrap();
            let _policy = crate::optimize::scoped_semantic_profile(profile);
            let program = crate::optimize::fold_constants_for_target(program, Target::detect_host());
            let error = crate::types::checker::check_staged_semantics(&program, Target::detect_host(), profile)
                .err().unwrap_or_else(|| panic!("expected strict conversion failure: {source}"));
            assert!(error.message.contains("strict-coercions"), "{mode:?}: {source}: {error:?}");
        }
    }
}

/// Dynamic scalar arguments retain guards and correct ABI storage on every supported target.
#[test]
fn strict_conversion_parameters_original_values_and_all_target_storage() {
    let source = "<?php function f(float $v): void { echo $v; } function g(mixed $v): void { f($v); } function members(int|false|null $v): void {} function memberCaller(mixed $v): void { members($v); } g(2.0); memberCaller(false);";
    for mode in [SourceMode::Php, SourceMode::Lfc, SourceMode::Internal] {
        let program = crate::parser::parse_with_mode(&crate::lexer::tokenize(source).unwrap(), mode).unwrap();
        for target in ["macos-aarch64", "ios-arm64", "ios-sim-arm64", "linux-aarch64", "linux-x86_64"] {
            let target = Target::parse(target).unwrap();
            let profile = SemanticProfile::PERMISSIVE.with(StrictFeature::Coercions, true);
            let checked = crate::types::checker::check_staged_semantics(&program, target, profile).unwrap();
            let module = crate::ir_lower::lower_program(&program, &checked, target, false).unwrap();
            for optimize in [false, true] {
                let mut module = module.clone(); if optimize { crate::ir_passes::optimize_module(&mut module); }
                for function in ["g", "memberCaller"] {
                    let function = module.functions.iter().find(|f| f.name == function).unwrap();
                    assert_eq!(function.instructions.iter().any(|i| i.op == crate::ir::Op::ThrowTypeError),
                        mode != SourceMode::Internal, "{target:?} {mode:?}");
                }
                crate::ir::validate_module(&module).unwrap();
                crate::codegen::generate_user_asm_from_ir(&module, false, false).unwrap();
            }
        }
    }
}
