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

/// Flow-known builtin inputs fail with strict diagnostics while erased values keep runtime guards.
#[test]
fn strict_conversion_parameters_builtin_typed_inputs_and_all_targets() {
    let profile = SemanticProfile::PERMISSIVE.with(StrictFeature::Coercions, true);
    for mode in [SourceMode::Php, SourceMode::Lfc] {
        for source in [
            "<?php function checkedLength(int $value): int { return strlen($value); }",
            "<?php function checkedRepeat(float $value): string { return str_repeat('a', $value); }",
            "<?php function checkedJson(int $value): mixed { return json_decode($value); }",
        ] {
            let program = crate::parser::parse_with_mode(&crate::lexer::tokenize(source).unwrap(), mode).unwrap();
            let error = crate::types::checker::check_staged_semantics(&program, Target::detect_host(), profile)
                .err().unwrap_or_else(|| panic!("{source}"));
            assert!(error.message.contains("strict-coercions"), "{source}: {error:?}");
        }
    }
    let source = "<?php function builtinDynamic(mixed $value): int { return strlen($value); } function builtinJson(mixed $value): mixed { return json_decode($value); } function builtinLength(mixed $length): string { return substr('abc', 0, $length); }";
    for mode in [SourceMode::Php, SourceMode::Lfc, SourceMode::Internal] {
        let program = crate::parser::parse_with_mode(&crate::lexer::tokenize(source).unwrap(), mode).unwrap();
        for target in ["macos-aarch64", "ios-arm64", "ios-sim-arm64", "linux-aarch64", "linux-x86_64"] {
            let target = Target::parse(target).unwrap();
            let checked = crate::types::checker::check_staged_semantics(&program, target, profile).unwrap();
            let module = crate::ir_lower::lower_program(&program, &checked, target, false).unwrap();
            for optimize in [false, true] {
                let mut module = module.clone();
                if optimize { crate::ir_passes::optimize_module(&mut module); }
                for name in ["builtinDynamic", "builtinJson", "builtinLength"] {
                    let function = module.functions.iter().find(|function| function.name == name).unwrap();
                    assert_eq!(function.instructions.iter().any(|instruction| instruction.op == crate::ir::Op::ThrowTypeError),
                        mode != SourceMode::Internal, "{target:?} {mode:?}: {name}");
                }
                crate::ir::validate_module(&module).unwrap();
                crate::codegen::generate_user_asm_from_ir(&module, false, false).unwrap();
            }
        }
    }
}

/// Original builtin inputs remain subject to exact contracts before constant calls disappear.
#[test]
fn strict_conversion_parameters_builtin_inputs_survive_source_folding() {
    let profile = SemanticProfile::PERMISSIVE.with(StrictFeature::Coercions, true);
    for source in [
        "<?php echo strlen(7);",
        "<?php if (false) { strtoupper(7); }",
        "<?php echo strlen(string: true);",
        "<?php echo strlen(...[7]);",
        "<?php echo substr('abc', 1.0);",
        "<?php echo str_repeat('a', 2.0);",
        "<?php $array = [2, 1]; ksort($array, '1');",
        "<?php $value = 1.5; settype($value, 7);",
    ] {
        for mode in [SourceMode::Php, SourceMode::Lfc] {
            let program = crate::parser::parse_with_mode(&crate::lexer::tokenize(source).unwrap(), mode).unwrap();
            let error = crate::semantics::validate_source(&program, profile)
                .expect_err(source);
            assert!(error.message.contains("strict-coercions"), "{mode:?}: {source}: {error:?}");
            let _policy = crate::optimize::scoped_semantic_profile(profile);
            let folded = crate::optimize::fold_constants_for_target(program, Target::detect_host());
            let error = crate::types::checker::check_staged_semantics(&folded, Target::detect_host(), profile)
                .err().unwrap_or_else(|| panic!("folded source escaped: {source}"));
            assert!(error.message.contains("strict-coercions"), "{mode:?}: {source}: {error:?}");
        }
    }
}

/// Exact values, explicit conversion APIs and trusted implementation calls retain source admission.
#[test]
fn strict_conversion_parameters_builtin_source_identity_controls() {
    let profile = SemanticProfile::PERMISSIVE.with(StrictFeature::Coercions, true);
    for source in [
        "<?php echo strlen('7');", "<?php echo strlen(strval(7));",
        "<?php echo intval('7');", "<?php echo substr('abc', 1, null);",
        "<?php $array = [2, 1]; ksort($array, SORT_STRING);",
        "<?php $value = 1.5; settype($value, 'int');",
    ] {
        let program = crate::parser::parse_with_mode(&crate::lexer::tokenize(source).unwrap(), SourceMode::Php).unwrap();
        crate::semantics::validate_source(&program, profile).unwrap_or_else(|error| panic!("{source}: {error:?}"));
    }
    for mode in [SourceMode::Php, SourceMode::Internal] {
        let program = crate::parser::parse_with_mode(&crate::lexer::tokenize("<?php echo strlen(7);").unwrap(), mode).unwrap();
        crate::semantics::validate_source(&program, SemanticProfile::PERMISSIVE).unwrap();
        if mode == SourceMode::Internal { crate::semantics::validate_source(&program, profile).unwrap(); }
    }
}

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

/// Public library callers admit Coercions and retain original kind guards on every target.
#[test]
fn strict_conversion_parameters_public_profile_on_all_targets() {
    let profile = SemanticProfile::PERMISSIVE.with(StrictFeature::Coercions, true);
    let source = "<?php function exactPublicArgument(float $value): float { return $value; } function exactPublicReturn(mixed $value): float { return exactPublicArgument($value); } function exactPublicString(string $value): string { return $value; } function exactPublicStringRead(int $index): string { $parts = [\"a\"]; return exactPublicString($parts[$index]); }";
    let program = crate::parser::parse(&crate::lexer::tokenize(source).unwrap()).unwrap();
    for name in ["macos-aarch64", "ios-arm64", "ios-sim-arm64", "linux-aarch64", "linux-x86_64"] {
        let target = Target::parse(name).unwrap();
        let checked = crate::types::check_with_target_and_options(&program, target,
            crate::types::CheckOptions { semantic_profile: profile, ..Default::default() },
            &crate::generics::GenericContext::default()).unwrap();
        let module = crate::ir_lower::lower_program(&program, &checked, target, false).unwrap();
        assert_eq!(module.semantic_profile, profile);
        let read = module.functions.iter().find(|function| function.name == "exactPublicStringRead").unwrap();
        assert!(read.instructions.iter().any(|inst| inst.op == crate::ir::Op::TypePredicate
            && inst.immediate == Some(crate::ir::Immediate::TypePredicate(crate::ir::PhpTypePredicate::String))),
            "{name}: concrete string storage retains logical kind validation");
        assert!(module.functions.iter().any(|function| function.instructions.iter()
            .any(|inst| inst.op == crate::ir::Op::ThrowTypeError)), "{name}: original input guard");
        crate::ir::validate_module(&module).unwrap();
        crate::codegen::generate_user_asm_from_ir(&module, false, false).unwrap();
    }
}
