//! Purpose:
//! Removes offsets from boxed PHP arrays without renumbering surviving keys.
//!
//! Called from:
//! - `crate::codegen::lower_inst::lower_instruction` for `Op::OffsetUnset`.
//!
//! Key details:
//! - EIR publishes a detached cell before this lowering promotes and publishes its unique hash.
//! - Destructors can throw or replace the array; no receiver writeback follows value release.

use crate::codegen::{abi, CodegenIrError, Result};
use crate::codegen::context::FunctionContext;
use crate::codegen::platform::Arch;
use crate::ir::Instruction;
use crate::types::PhpType;

use super::{expect_operand, hashes};

/// Promotes a rooted array cell to sparse storage, then removes the key from its installed hash.
pub(super) fn lower_offset_unset(ctx: &mut FunctionContext<'_>, inst: &Instruction) -> Result<()> {
    let cell = expect_operand(inst, 0)?;
    let key = expect_operand(inst, 1)?;
    if ctx.value_php_type(cell)?.codegen_repr() != PhpType::Mixed {
        return Err(CodegenIrError::invalid_module("offset_unset expects a boxed PHP array"));
    }
    ctx.load_value_to_reg(cell, abi::int_arg_reg_name(ctx.emitter.target, 0))?;
    abi::emit_call_label(ctx.emitter, "__rt_mixed_cell_promote_to_hash");
    let valid = ctx.next_label("offset_unset_array");
    match ctx.emitter.target.arch {
        Arch::AArch64 => {
            ctx.emitter.instruction(&format!("cbnz x0, {valid}"));              // a nonzero result is the cell's installed unique hash
        }
        Arch::X86_64 => {
            ctx.emitter.instruction("test rax, rax");                           // reject invalid boxed receivers before dereferencing storage
            ctx.emitter.instruction(&format!("jnz {valid}"));                   // a nonzero result is the cell's installed unique hash
        }
    }
    super::exceptions::emit_type_error(ctx, "Cannot unset offset in a non-array variable");
    ctx.emitter.label(&valid);
    abi::emit_push_reg(ctx.emitter, abi::int_result_reg(ctx.emitter));
    match ctx.emitter.target.arch {
        Arch::AArch64 => {
            hashes::materialize_hash_key_aarch64(ctx, key)?;
            abi::emit_pop_reg(ctx.emitter, "x0");
        }
        Arch::X86_64 => {
            hashes::materialize_hash_key_x86_64(ctx, key)?;
            abi::emit_pop_reg(ctx.emitter, "rdi");
        }
    }
    abi::emit_call_label(ctx.emitter, "__rt_hash_unset");
    Ok(())
}
