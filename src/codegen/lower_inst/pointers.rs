//! Purpose:
//! Lowers pointer-specific EIR opcodes that are not PHP builtin calls.
//! Currently covers type-only pointer casts.
//!
//! Called from:
//! - `crate::codegen::lower_inst::lower_instruction()`.
//!
//! Key details:
//! - `PtrCast` changes the pointee type metadata but must preserve the raw address payload exactly.

use crate::codegen::{CodegenIrError, Result};
use crate::ir::Instruction;
use crate::types::PhpType;

use super::super::context::FunctionContext;
use super::{expect_operand, store_if_result};

/// Lowers `PtrCast` by forwarding the raw pointer address into the cast result slot.
pub(super) fn lower_ptr_cast(ctx: &mut FunctionContext<'_>, inst: &Instruction) -> Result<()> {
    let pointer = expect_operand(inst, 0)?;
    match ctx.load_value_to_result(pointer)?.codegen_repr() {
        PhpType::Pointer(_) => store_if_result(ctx, inst),
        other => Err(CodegenIrError::unsupported(format!(
            "ptr_cast operand PHP type {:?}",
            other
        ))),
    }
}
