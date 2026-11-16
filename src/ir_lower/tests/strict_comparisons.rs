//! Purpose:
//! Verifies strict comparison source evidence, bytewise folding and all-target EIR.
//!
//! Called from:
//! - Focused selected-profile compiler library tests.
//!
//! Key details:
//! - Staged checking exercises the actual frontend without opening public feature admission.

use crate::{source::SourceMode, codegen_support::platform::Target};
use elephc_semantics::{SemanticProfile, StrictFeature};

/// Automatic builtin comparisons cannot disappear before checking, including named and spread modes.
#[test]
fn strict_comparison_builtin_source_modes_and_defaults() {
    let profile = SemanticProfile::PERMISSIVE.with(StrictFeature::Comparisons, true);
    for source in [
        "<?php if (false) { in_array(1, [1]); }",
        "<?php in_array(1, [1], false);", "<?php in_array(1, [1], 0);",
        "<?php in_array(needle: 1, haystack: [1]);",
        "<?php in_array(...['strict' => false, 'haystack' => [1], 'needle' => 1]);",
        "<?php array_search(...[1, [1], false]);", "<?php array_search(1, [1]);",
        "<?php $values = [1]; sort($values);", "<?php array_unique([1]);",
        "<?php max(1, 2);", "<?php min(1, 2);", "<?php array_diff([1], ['1']);",
    ] {
        let program = crate::parser::parse(&crate::lexer::tokenize(source).unwrap()).unwrap();
        let error = crate::semantics::validate_source(&program, profile).unwrap_err();
        assert!(error.message.contains("strict-comparisons"), "{source}: {error:?}");
        assert!(crate::semantics::validate_source(&program, SemanticProfile::PERMISSIVE).is_ok());
    }
    for source in [
        "<?php in_array(1, [1], true);",
        "<?php in_array(strict: true, needle: 1, haystack: [1]);",
        "<?php array_search(...['strict' => true, 'haystack' => [1], 'needle' => 1]);",
        "<?php function search(bool $mode) { return in_array(1, [1], $mode); }",
        "<?php function search(array $args) { return in_array(...$args); }",
        "<?php array_keys([1]);",
    ] {
        let program = crate::parser::parse(&crate::lexer::tokenize(source).unwrap()).unwrap();
        crate::semantics::validate_source(&program, profile).unwrap_or_else(|error| panic!("{source}: {error:?}"));
    }
}

/// A prepared builtin call keeps its operation policy even when its physical consumer is internal.
#[test]
fn strict_comparison_builtin_prepared_source_modes() {
    let profile = SemanticProfile::PERMISSIVE.with(StrictFeature::Comparisons, true);
    for (physical, producer, rejected) in [
        (SourceMode::Internal, SourceMode::Php, true),
        (SourceMode::Php, SourceMode::Internal, false),
    ] {
        let mut program = crate::parser::parse(&crate::lexer::tokenize("<?php echo in_array(1, [1]);").unwrap()).unwrap();
        program[0].source_mode = physical;
        let crate::parser::ast::StmtKind::Echo(value) = &mut program[0].kind else { panic!("builtin fixture"); };
        value.producer_binding = Some(crate::parser::ast::ProducerBinding::Identity);
        value.source_profile.mode = producer;
        assert_eq!(crate::semantics::validate_source(&program, profile).is_err(), rejected,
            "physical={physical:?} producer={producer:?}");
    }
}

/// Evaluated builtin mode guards use ordinary EIR on all targets and retain trusted controls.
#[test]
fn strict_comparison_builtin_all_target_guards() {
    let profile = SemanticProfile::PERMISSIVE.with(StrictFeature::Comparisons, true);
    let source = "<?php function choose(mixed $needle, array $items, bool $mode): bool { return in_array($needle, $items, $mode); }";
    for mode in [SourceMode::Php, SourceMode::Internal] {
        let program = crate::parser::parse_with_mode(&crate::lexer::tokenize(source).unwrap(), mode).unwrap();
        for name in ["macos-aarch64", "ios-arm64", "ios-sim-arm64", "linux-aarch64", "linux-x86_64"] {
            let target = Target::parse(name).unwrap();
            let checked = crate::types::checker::check_staged_semantics(&program, target, profile).unwrap();
            let module = crate::ir_lower::lower_program(&program, &checked, target, false).unwrap();
            for optimize in [false, true] {
                let mut module = module.clone();
                if optimize { crate::ir_passes::optimize_module(&mut module); }
                let function = module.functions.iter().find(|function| function.name == "choose").unwrap();
                let mode_guard = module.data.strings.iter().any(|message| message.contains("--strict-comparisons: in_array()"))
                    && function.instructions.iter().any(|instruction| instruction.op == crate::ir::Op::ThrowTypeError);
                assert_eq!(mode_guard,
                    mode != SourceMode::Internal, "{name} {mode:?} opt={optimize}");
                crate::ir::validate_module(&module).unwrap();
                crate::codegen::generate_user_asm_from_ir(&module, false, false).unwrap();
            }
        }
    }
}

/// Prepared producer copies retain the operation policy of their source across physical scopes.
#[test]
fn strict_comparison_prepared_producer_source_policy() {
    let profile = SemanticProfile::PERMISSIVE.with(StrictFeature::Comparisons, true);
    for source in ["<?php echo 1 == 1;", "<?php echo (1 == 1) ? 1 : 0;"] {
        for (statement_mode, producer_mode, rejected) in [
            (SourceMode::Internal, SourceMode::Php, true),
            (SourceMode::Php, SourceMode::Internal, false),
        ] {
            let mut program = crate::parser::parse_with_mode(&crate::lexer::tokenize(source).unwrap(), SourceMode::Php).unwrap();
            program[0].source_mode = statement_mode;
            let crate::parser::ast::StmtKind::Echo(value) = &mut program[0].kind else { panic!("echo fixture"); };
            value.producer_binding = Some(crate::parser::ast::ProducerBinding::Identity);
            value.source_profile.mode = producer_mode;
            let result = crate::semantics::validate_source(&program, profile);
            assert_eq!(result.is_err(), rejected, "{source}: {statement_mode:?} {producer_mode:?}: {result:?}");
        }
    }
}

/// Prepared policies restore at expression exit and do not exempt nested user statements.
#[test]
fn strict_comparison_prepared_producers_restore_nested_source() {
    let profile = SemanticProfile::PERMISSIVE.with(StrictFeature::Comparisons, true);
    let source = "<?php echo 1 == 1;\necho 2 == 2;";
    let mut program = crate::parser::parse_with_mode(&crate::lexer::tokenize(source).unwrap(), SourceMode::Php).unwrap();
    let crate::parser::ast::StmtKind::Echo(value) = &mut program[0].kind else { panic!("echo fixture"); };
    value.producer_binding = Some(crate::parser::ast::ProducerBinding::Identity);
    value.source_profile.mode = SourceMode::Internal;
    let error = crate::semantics::validate_source(&program, profile).unwrap_err();
    assert_eq!(error.span.line, 2);
    let source = "<?php $closure = function () { echo 1 == 1; };";
    let mut program = crate::parser::parse_with_mode(&crate::lexer::tokenize(source).unwrap(), SourceMode::Php).unwrap();
    program[0].source_mode = SourceMode::Internal;
    let crate::parser::ast::StmtKind::Assign { value, .. } = &mut program[0].kind else { panic!("closure fixture"); };
    value.producer_binding = Some(crate::parser::ast::ProducerBinding::Identity);
    value.source_profile.mode = SourceMode::Internal;
    assert!(crate::semantics::validate_source(&program, profile).unwrap_err().message.contains("strict-comparisons"));
}

/// Known switch labels must share a possible exact kind or nominal instance with the live subject.
#[test]
fn strict_comparison_typed_switch_label_rejections() {
    let profile = SemanticProfile::PERMISSIVE.with(StrictFeature::Comparisons, true);
    for source in [
        "<?php function choose(int $value): void { switch ($value) { case '1': break; } }",
        "<?php function choose(float $value): void { switch ($value) { case 1: break; } }",
        "<?php function choose(array $value): void { switch ($value) { case false: break; } }",
        "<?php function choose(int|string $value): void { switch ($value) { case false: break; } }",
        "<?php class SwitchA {} class SwitchB {} function choose(SwitchA $value, SwitchB $label): void { switch ($value) { case $label: break; } }",
    ] {
        let program = crate::parser::parse_with_mode(&crate::lexer::tokenize(source).unwrap(), SourceMode::Php).unwrap();
        let error = crate::types::checker::check_staged_semantics(&program, Target::detect_host(), profile)
            .err().unwrap_or_else(|| panic!("{source}"));
        assert!(error.message.contains("strict-comparisons"), "{source}: {error:?}");
    }
}

/// Union members, nominal inheritance, interfaces and evaluated label effects keep valid exact cases.
#[test]
fn strict_comparison_typed_switch_label_controls() {
    let profile = SemanticProfile::PERMISSIVE.with(StrictFeature::Comparisons, true);
    for source in [
        "<?php function choose(int|string $value): void { switch ($value) { case 1: break; case '1': break; } }",
        "<?php function choose(array $value, array $label): void { switch ($value) { case $label: break; } }",
        "<?php class SwitchParent {} class SwitchChild extends SwitchParent {} function choose(SwitchParent $value, SwitchChild $label): void { switch ($value) { case $label: break; } }",
        "<?php interface SwitchInterface {} class SwitchConcrete implements SwitchInterface {} function choose(SwitchInterface $value, SwitchConcrete $label): void { switch ($value) { case $label: break; } }",
        "<?php function initial(): mixed { return 1; } $value = initial(); switch ($value) { case ($value = '2'): break; }",
    ] {
        let program = crate::parser::parse_with_mode(&crate::lexer::tokenize(source).unwrap(), SourceMode::Php).unwrap();
        crate::types::checker::check_staged_semantics(&program, Target::detect_host(), profile)
            .unwrap_or_else(|error| panic!("{source}: {error:?}"));
    }
    let source = "<?php function choose(int $value): void { switch ($value) { case '1': break; } }";
    let program = crate::parser::parse_with_mode(&crate::lexer::tokenize(source).unwrap(), SourceMode::Internal).unwrap();
    crate::types::checker::check_staged_semantics(&program, Target::detect_host(), profile).unwrap();
}

/// Loose syntax and known original-kind mismatches survive pruning and nested literal eval.
#[test]
fn strict_comparison_source_and_typed_rejections() {
    let profile = SemanticProfile::PERMISSIVE.with(StrictFeature::Comparisons, true);
    for source in [
        "<?php if (false) { echo 1 == 1; }", "<?php echo 1 != 2;", "<?php echo 1 <> 2;",
        "<?php echo 1 < '2';", "<?php echo 1 <=> 2.0;",
        "<?php function order(int $left, float $right): int { return $left <=> $right; }",
        "<?php function order(int $left): bool { return $left < '2'; }",
        "<?php switch (1) { case '1': echo 'bad'; }",
        "<?php eval('if (false) { echo 1 == 1; }');",
    ] {
        for mode in [SourceMode::Php, SourceMode::Lfc] {
            let program = crate::parser::parse_with_mode(&crate::lexer::tokenize(source).unwrap(), mode).unwrap();
            let error = crate::types::checker::check_staged_semantics(&program, Target::detect_host(), profile)
                .err().unwrap_or_else(|| panic!("{source}"));
            assert!(error.message.contains("strict-comparisons"), "{source}: {error:?}");
        }
    }
}

/// Guarded unknown values and bytewise string order lower through existing operations on every target.
#[test]
fn strict_comparison_all_target_scalar_guards_and_internal_controls() {
    let source = "<?php function dynamicOrder(mixed $left, mixed $right): int { return $left <=> $right; } function stringOrder(string $left, string $right): bool { return $left < $right; }";
    let profile = SemanticProfile::PERMISSIVE.with(StrictFeature::Comparisons, true);
    for mode in [SourceMode::Php, SourceMode::Lfc, SourceMode::Internal] {
        let program = crate::parser::parse_with_mode(&crate::lexer::tokenize(source).unwrap(), mode).unwrap();
        for name in ["macos-aarch64", "ios-arm64", "ios-sim-arm64", "linux-aarch64", "linux-x86_64"] {
            let target = Target::parse(name).unwrap();
            let checked = crate::types::checker::check_staged_semantics(&program, target, profile).unwrap();
            let module = crate::ir_lower::lower_program(&program, &checked, target, false).unwrap();
            for optimize in [false, true] {
                let mut module = module.clone();
                if optimize { crate::ir_passes::optimize_module(&mut module); }
                let dynamic = module.functions.iter().find(|function| function.name == "dynamicOrder").unwrap();
                let string = module.functions.iter().find(|function| function.name == "stringOrder").unwrap();
                assert_eq!(dynamic.instructions.iter().any(|instruction| instruction.op == crate::ir::Op::ThrowTypeError),
                    mode != SourceMode::Internal, "{name} {mode:?}");
                assert_eq!(string.instructions.iter().any(|instruction| instruction.op == crate::ir::Op::StrCmp),
                    mode != SourceMode::Internal, "{name} {mode:?}");
                crate::ir::validate_module(&module).unwrap();
                crate::codegen::generate_user_asm_from_ir(&module, false, false).unwrap();
            }
        }
    }
}

/// Initial folding follows byte order while preserving invalid conversions and exact switch matching.
#[test]
fn strict_comparison_folding_preserves_kinds_and_numeric_string_order() {
    for (profile, expected) in [(SemanticProfile::PERMISSIVE, false),
        (SemanticProfile::PERMISSIVE.with(StrictFeature::Comparisons, true), true)] {
        let _profile = crate::optimize::scoped_semantic_profile(profile);
        let program = crate::parser::parse(&crate::lexer::tokenize("<?php echo '10' < '9';").unwrap()).unwrap();
        let folded = crate::optimize::fold_constants(program);
        assert!(matches!(&folded[0].kind, crate::parser::ast::StmtKind::Echo(expr)
            if matches!(expr.kind, crate::parser::ast::ExprKind::BoolLiteral(value) if value == expected)));
    }
    let _profile = crate::optimize::scoped_semantic_profile(SemanticProfile::PERMISSIVE.with(StrictFeature::Comparisons, true));
    let program = crate::parser::parse(&crate::lexer::tokenize("<?php echo 1 < 2.0;").unwrap()).unwrap();
    let folded = crate::optimize::fold_constants(program);
    assert!(matches!(&folded[0].kind, crate::parser::ast::StmtKind::Echo(expr)
        if matches!(expr.kind, crate::parser::ast::ExprKind::BinaryOp { .. })));
}
