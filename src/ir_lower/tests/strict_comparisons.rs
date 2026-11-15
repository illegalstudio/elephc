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
