//! Purpose:
//! Defines the consumer contract for replacing boxed numeric results with scalar values.
//!
//! Called from:
//! - Integer range specialization and EIR constant folding.
//!
//! Key details:
//! - A numerical proof alone does not prove storage or conversion compatibility.
//! - Unknown consumers and module-dependent stores retain their boxed inputs.
//! - Legacy scalar null sentinels must not alias an ordinary boxed integer payload.

use std::collections::HashSet;

use crate::ir::{Function, Immediate, Instruction, IrType, Op, Terminator, ValueId};
use crate::types::PhpType;

/// Checks that every boxed integer payload keeps its meaning in the active scalar representation.
pub(super) fn integer_range_can_narrow(lo: i64, hi: i64) -> bool {
    let sentinel = crate::codegen_support::sentinels::NULL_SENTINEL;
    crate::codegen_support::sentinels::null_repr_is_tagged()
        || hi < sentinel || lo > sentinel
}

/// Collects values with any use that cannot consume the requested scalar representation.
pub(super) fn blocked_results(function: &Function, scalar: IrType) -> HashSet<ValueId> {
    let mut blocked = HashSet::new();
    for block in &function.blocks {
        if let Some(term) = &block.terminator {
            if matches!(term, Terminator::Return { .. })
                && function.return_type == scalar
                && matches!((&function.return_php_type, scalar),
                    (PhpType::Int, IrType::I64) | (PhpType::Float, IrType::F64))
            {
                continue;
            }
            blocked.extend(super::liveness::terminator_uses(term));
        }
    }
    for user in &function.instructions {
        if !accepts_scalar(function, user, scalar) {
            blocked.extend(user.operands.iter().copied());
        }
    }
    blocked
}

/// Checks the actual conversion contract, not just whether an opcode accepts one operand.
fn accepts_scalar(function: &Function, user: &Instruction, scalar: IrType) -> bool {
    match user.op {
        Op::Cast => matches!(user.immediate,
            Some(Immediate::CastTarget(IrType::I64 | IrType::F64 | IrType::Str))),
        Op::StoreLocal | Op::InitStaticLocal | Op::StoreRefCell => {
            let Some(Immediate::LocalSlot(slot)) = user.immediate else { return false; };
            let Some(local) = function.locals.get(slot.as_raw() as usize) else { return false; };
            accepts_scalar_store(&local.php_type, scalar)
                && (user.op != Op::StoreRefCell || accepts_scalar_store(&user.result_php_type, scalar))
        }
        // Scalar spaceship uses numeric ordering, unlike boxed bool/null truthiness ordering.
        Op::Spaceship => user.operands.iter().all(|value| function.value(*value)
            .is_some_and(|value| matches!(value.php_type.codegen_repr(),
                PhpType::Int | PhpType::Float | PhpType::Mixed | PhpType::TaggedScalar))),
        // Scalar casts and observations dispatch on the operand's current PHP type.
        Op::Acquire | Op::Release | Op::MixedBox
        | Op::EchoValue | Op::PrintValue | Op::WriteStdout | Op::VarDump | Op::PrintR
        | Op::StrictEq | Op::StrictNotEq | Op::LooseEq | Op::LooseNotEq
        | Op::IsNull | Op::IsTruthy
        | Op::TypePredicate | Op::IsEmpty => true,
        // Static properties require module metadata and may need a Mixed-to-scalar cast.
        // Static-local assignments do not rebox. Web superglobals can use raw storage.
        // Extern stores select their ABI from the source rather than the declared target.
        // PhpRelCmp requires at least one runtime-tagged operand even after batch narrowing.
        _ => false,
    }
}

/// Allows only stores whose backend can preserve the scalar payload and target representation.
fn accepts_scalar_store(target: &PhpType, scalar: IrType) -> bool {
    match target.codegen_repr() {
        PhpType::Mixed => true,
        PhpType::Int | PhpType::TaggedScalar => scalar == IrType::I64,
        _ => false,
    }
}
