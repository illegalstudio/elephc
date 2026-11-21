//! Purpose:
//! Keeps native reference adoption attached directly to its owning producer.
//!
//! Called from:
//! - Function validation after structural, dominance, operand and effect checks.
//!
//! Key details:
//! - Codegen uses these typed consumers to distinguish raw-cell calls from value calls.
//! - Optimizers must remap the call and adopter together, without an intervening copy or phi.

use super::*;

/// Rejects reference adopters whose operand no longer names a direct owning producer.
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
                    | Op::ClosureCall | Op::ExprCall | Op::PipeCall | Op::LoadGlobalRefCell),
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

    /// Global leases transfer ownership directly; borrowed addresses and copied leases cannot.
    #[test]
    fn global_reference_adoption_requires_an_owned_direct_producer() {
        for (op, copied, accepts) in [(Op::LoadGlobalRefCell, false, true),
            (Op::LoadGlobalRefCell, true, false), (Op::LocalPlace, false, false)] {
            let mut function = Function::new("global_transfer".into(), IrType::Void, PhpType::Void);
            let local = function.add_local(Some("alias".into()), IrType::from_php(&PhpType::Mixed), PhpType::Mixed, LocalKind::PhpLocal);
            let owner = function.add_local(Some("owner".into()), IrType::from_php(&PhpType::Mixed), PhpType::Mixed, LocalKind::RefCell);
            {
                let mut builder = crate::ir::Builder::new(&mut function);
                let entry = builder.create_named_block("entry", vec![]); builder.position_at_end(entry);
                let immediate = if op == Op::LoadGlobalRefCell { Immediate::GlobalName(crate::ir::DataId::from_raw(0)) }
                    else { Immediate::LocalSlot(local) };
                let value = builder.emit(op, vec![], Some(immediate), IrType::I64,
                    PhpType::Pointer(None), Ownership::NonHeap).unwrap();
                let value = if copied { builder.emit(Op::Borrow, vec![value], None, IrType::I64,
                    PhpType::Pointer(None), Ownership::NonHeap).unwrap() } else { value };
                builder.emit(Op::AdoptRefCellPtr, vec![value],
                    Some(Immediate::LocalSlotPair { first: local, second: owner }),
                    IrType::Void, PhpType::Mixed, Ownership::NonHeap);
                builder.terminate(Terminator::Return { value: None });
            }
            assert_eq!(validate_transfers(&function).is_ok(), accepts, "{op:?}, copied={copied}");
        }
    }

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
