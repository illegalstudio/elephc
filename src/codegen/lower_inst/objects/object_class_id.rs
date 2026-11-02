//! Purpose:
//! Lowers runtime object-class identity reads from typed EIR object values.
//!
//! Called from:
//! - `crate::codegen::lower_inst::lower_instruction()`.
//!
//! Key details:
//! - Every supported target stores the runtime class id at object payload offset zero.

use super::*;

/// Loads an object's runtime class id from its payload header into the EIR result.
pub(in crate::codegen::lower_inst) fn lower_object_class_id(
    ctx: &mut FunctionContext<'_>,
    inst: &Instruction,
) -> Result<()> {
    let object = expect_operand(inst, 0)?;
    let object_ty = ctx.value_php_type(object)?.codegen_repr();
    if !matches!(object_ty, PhpType::Object(_)) {
        return Err(CodegenIrError::invalid_module(format!(
            "object_class_id operand has PHP type {:?}",
            object_ty
        )));
    }
    ctx.load_value_to_result(object)?;
    let result = abi::int_result_reg(ctx.emitter);
    abi::emit_load_from_address(ctx.emitter, result, result, 0);
    store_if_result(ctx, inst)
}
