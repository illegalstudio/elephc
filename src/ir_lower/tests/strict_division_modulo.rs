//! Purpose:
//! Verifies explicit checked division and remainder contracts through EIR and target lowering.
//!
//! Called from:
//! - Focused compiler library tests.
//!
//! Key details:
//! - Trusted implementation operations keep their existing PHP entries.
//! - Policy survives optimization and malformed unprofiled division fails validation.

use crate::codegen_support::platform::Target;
use crate::ir::{Immediate, MixedNumericOp, Op};
use crate::source::SourceMode;
use elephc_semantics::{SemanticProfile, StrictFeature};

/// Checked dynamic arithmetic keeps scalar result normalization and profile identity on every target.
#[test]
fn strict_aot_division_modulo_policy_survives_targets_optimization_and_internal_source() {
    let source = "<?php function strict_div_probe(mixed $left, mixed $right): float { return $left / $right; } function strict_mod_probe(mixed $left, mixed $right): int { return $left % $right; } function strict_float_mod_probe(float $left, float $right): int { return $left % $right; }";
    for name in ["macos-aarch64", "ios-arm64", "ios-sim-arm64", "linux-aarch64", "linux-x86_64"] {
        for mode in [SourceMode::Php, SourceMode::Lfc, SourceMode::Internal] {
            let target = Target::parse(name).unwrap();
            let program = crate::parser::parse_with_mode(&crate::lexer::tokenize(source).unwrap(), mode).unwrap();
            let mut checked = crate::types::check_with_target(&program, target).unwrap();
            checked.semantic_profile = SemanticProfile::PERMISSIVE.with(StrictFeature::Overflow, true);
            let module = crate::ir_lower::lower_program(&program, &checked, target, false).unwrap();
            for optimize in [false, true] {
                let mut module = module.clone();
                if optimize { crate::ir_passes::optimize_module(&mut module); }
                crate::ir::validate_module(&module).unwrap();
                let operations = module.functions.iter().flat_map(|function| &function.instructions)
                    .filter(|inst| matches!(inst.immediate, Some(Immediate::NumericOperation {
                        op: MixedNumericOp::Div | MixedNumericOp::Mod, .. }))).count();
                let user = mode != SourceMode::Internal;
                assert_eq!(operations, if user { 3 } else { 0 }, "{name}, {mode:?}, opt={optimize}");
                let asm = crate::codegen::generate_user_asm_from_ir(&module, false, false).unwrap();
                for helper in ["__rt_strict_mixed_numeric_div", "__rt_strict_mixed_numeric_mod"] {
                    assert_eq!(asm.contains(helper), user, "{name}, {mode:?}, opt={optimize}");
                }
                if !user { continue; }
                let mut invalid = module.clone();
                let instruction = invalid.functions.iter_mut().flat_map(|function| &mut function.instructions)
                    .find(|inst| matches!(inst.immediate, Some(Immediate::NumericOperation { op: MixedNumericOp::Mod, .. }))).unwrap();
                assert_eq!(instruction.op, Op::MixedNumericBinop);
                instruction.immediate = Some(Immediate::MixedNumericOp(MixedNumericOp::Mod));
                assert!(crate::ir::validate_module(&invalid).is_err(), "{name}: unprofiled remainder must fail closed");
                module.semantic_profile = SemanticProfile::PERMISSIVE;
                assert!(crate::ir::validate_module(&module).is_err(), "{name}: required profile was lost");
            }
        }
    }
}


/// Boxed division/remainder cannot enter the old promoting integer-chain instruction format.
#[test]
fn strict_aot_division_modulo_rejects_unsupported_fused_chains() {
    use crate::ir::{Builder, CheckedNumericChainImmediate, Function, IrType, Ownership, Terminator};
    use crate::types::PhpType;
    for operation in [MixedNumericOp::Div, MixedNumericOp::Mod] {
        let mut function = Function::new("invalid_division_chain".into(), IrType::I64, PhpType::Int);
        let mut builder = Builder::new(&mut function);
        let entry = builder.create_named_block("entry", vec![]);
        builder.set_entry(entry);
        builder.position_at_end(entry);
        let left = builder.emit_const_i64(7);
        let right = builder.emit_const_i64(2);
        let result = builder.emit(Op::ICheckedNumericChainToInt, vec![left, right],
            Some(Immediate::CheckedNumericChain(Box::new(CheckedNumericChainImmediate::new(vec![operation])))),
            IrType::I64, PhpType::Int, Ownership::NonHeap).unwrap();
        builder.terminate(Terminator::Return { value: Some(result) });
        assert!(crate::ir::validate_function(&function).is_err(), "{operation:?}");
    }
}
