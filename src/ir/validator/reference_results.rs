//! Purpose:
//! Keeps native reference-return adoption attached directly to its producing call.
//!
//! Called from:
//! - Function validation after structural, dominance, operand and effect checks.
//!
//! Key details:
//! - Codegen uses these typed consumers to distinguish raw-cell calls from value calls.
//! - Optimizers must remap the call and adopter together, without an intervening copy or phi.

use super::*;

/// Rejects reference adopters whose operand no longer names the direct producing call.
pub(super) fn validate_transfers(function: &Function) -> Result<(), ValidationError> {
    for block in &function.blocks {
        for id in &block.instructions {
            let inst = &function.instructions[id.as_raw() as usize];
            if inst.op != Op::AdoptRefCellPtr { continue; }
            let operand = inst.operands[0];
            let valid = match function.values[operand.as_raw() as usize].def {
                ValueDef::Instruction { inst, .. } => matches!(function.instructions[inst.as_raw() as usize].op,
                    Op::Call | Op::FunctionVariantCall | Op::MethodCall | Op::NullsafeMethodCall
                    | Op::StaticMethodCall | Op::EvalStaticMethodCall | Op::EvalFunctionCall
                    | Op::ClosureCall | Op::ExprCall | Op::PipeCall),
                ValueDef::BlockParam { .. } => false,
            };
            if !valid {
                return Err(ValidationError::InvalidReferenceCallResult { inst: *id, operand });
            }
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A copy between the call and adopter must fail instead of changing the backend ABI silently.
    #[test]
    fn reference_adoption_requires_the_direct_call_result() {
        for copied in [false, true] {
            let mut function = Function::new("adopt_call_result".into(), IrType::Void, PhpType::Void);
            let local = function.add_local(Some("alias".into()), IrType::I64, PhpType::Int, LocalKind::PhpLocal);
            let owner = function.add_local(Some("owner".into()), IrType::I64, PhpType::Int, LocalKind::RefCell);
            let adopted;
            {
                let mut builder = crate::ir::Builder::new(&mut function);
                let entry = builder.create_named_block("entry", vec![]);
                builder.position_at_end(entry);
                let call = builder.emit_with_effects(Op::Call, vec![], None, IrType::I64, PhpType::Int,
                    Ownership::NonHeap, Op::Call.default_effects(), None).unwrap();
                adopted = if copied {
                    builder.emit_with_effects(Op::Borrow, vec![call], None, IrType::I64, PhpType::Int,
                        Ownership::NonHeap, Op::Borrow.default_effects(), None).unwrap()
                } else { call };
                builder.emit_with_effects(Op::AdoptRefCellPtr, vec![adopted],
                    Some(Immediate::LocalSlotPair { first: local, second: owner }),
                    IrType::Void, PhpType::Int, Ownership::NonHeap,
                    Op::AdoptRefCellPtr.default_effects(), None);
                builder.terminate(Terminator::Return { value: None });
            }
            if copied {
                assert!(matches!(super::super::validate_function(&function),
                    Err(ValidationError::InvalidReferenceCallResult { operand, .. }) if operand == adopted));
            } else {
                assert_eq!(super::super::validate_function(&function), Ok(()));
            }
        }
    }
}
