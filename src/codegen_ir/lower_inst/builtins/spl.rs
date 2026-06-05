//! Purpose:
//! Lowers SPL object-introspection builtins for the EIR backend.
//! Handles stable object ids and object hashes using the concrete heap pointer.
//!
//! Called from:
//! - `crate::codegen_ir::lower_inst::builtins::lower_builtin_call()`.
//!
//! Key details:
//! - The legacy backend exposes the object pointer as a process-stable identity.
//!   `spl_object_hash()` stringifies that same identity with the shared itoa helper.

use crate::codegen::abi;
use crate::codegen_ir::{CodegenIrError, Result};
use crate::ir::Instruction;
use crate::types::PhpType;

use super::super::super::context::FunctionContext;
use super::{expect_operand, store_if_result};

/// Lowers `spl_object_id(object)` by returning the loaded object pointer as an integer.
pub(super) fn lower_spl_object_id(
    ctx: &mut FunctionContext<'_>,
    inst: &Instruction,
) -> Result<()> {
    super::ensure_arg_count(inst, "spl_object_id", 1)?;
    load_object_operand(ctx, inst, "spl_object_id")?;
    store_if_result(ctx, inst)
}

/// Lowers `spl_object_hash(object)` by formatting the loaded object pointer as a string.
pub(super) fn lower_spl_object_hash(
    ctx: &mut FunctionContext<'_>,
    inst: &Instruction,
) -> Result<()> {
    super::ensure_arg_count(inst, "spl_object_hash", 1)?;
    load_object_operand(ctx, inst, "spl_object_hash")?;
    abi::emit_call_label(ctx.emitter, "__rt_itoa");
    store_if_result(ctx, inst)
}

/// Loads the single object operand into the canonical integer result register.
fn load_object_operand(
    ctx: &mut FunctionContext<'_>,
    inst: &Instruction,
    name: &str,
) -> Result<()> {
    let value = expect_operand(inst, 0)?;
    let ty = ctx.load_value_to_result(value)?;
    match ty {
        PhpType::Object(_) => Ok(()),
        other => Err(CodegenIrError::unsupported(format!(
            "{} for PHP type {:?}",
            name,
            other
        ))),
    }
}
