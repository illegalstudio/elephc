//! Purpose:
//! Verifies integer division retains typed catchable failure handling across all targets.
//!
//! Called from:
//! - Focused staged numeric lowering tests.
//!
//! Key details:
//! - PHP already rejects INT_MIN divided by minus one; strict overflow shares that kernel.
//! - Both source and generated callable paths use the common inline exception machinery.

use crate::source::SourceMode;
use elephc_semantics::{SemanticProfile, StrictFeature};

/// Integer division preserves its integer ABI and target-aware shared exception emission.
#[test]
fn strict_numeric_builtins_intdiv_shared_failure_emits_on_all_targets() {
    let source = "<?php function integer_division_probe(int $left, int $right): int { return intdiv($left, $right); } $divide = intdiv(...); $name = $argc > 0 ? 'intdiv' : 'fdiv'; try { echo integer_division_probe($argc, -1); echo $name($argc, -1); echo $divide($argc, 0); } catch (ArithmeticError $error) { echo 'caught'; }";
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
                    let asm = crate::codegen::generate_user_asm_from_ir(&module, false, false).unwrap();
                    assert!(asm.contains("_spl_arithmetic_error_class_id"), "{name} {mode:?} {optimize}");
                    assert!(asm.contains("__rt_throw_current"), "{name}");
                }
            }
        }
    }
}
