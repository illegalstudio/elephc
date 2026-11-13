//! Purpose:
//! Checks eval-owned abstract method targets before native argument evaluation.
//!
//! Called from:
//! - The EIR MethodLookup native-factory guard.
//!
//! Key details:
//! - Object identity finds its owning context without creating a local eval frame.
//! - Temporary metadata boxes retire before the ordinary throwable status path.

use super::*;

/// Uses the existing boxed-cell and throwable result ABI without invoking the target method.
pub(in crate::codegen::lower_inst::builtins) fn lower_eval_abstract_method_guard(
    ctx: &mut FunctionContext<'_>, _inst: &Instruction, object: ValueId, method: Option<&str>,
) -> Result<()> {
    abi::emit_reserve_temporary_stack(ctx.emitter, EVAL_STACK_BYTES);
    let ty = ctx.load_value_to_result(object)?.codegen_repr();
    if !matches!(ty, PhpType::Mixed | PhpType::Union(_)) {
        emit_box_current_value_as_mixed(ctx.emitter, &ty);
    }
    let result = abi::int_result_reg(ctx.emitter);
    abi::emit_store_to_sp(ctx.emitter, result, EVAL_TEMP_CELL_OFFSET);
    abi::emit_load_temporary_stack_slot(ctx.emitter, abi::int_arg_reg_name(ctx.emitter.target, 0), EVAL_TEMP_CELL_OFFSET);
    if let Some(method) = method {
        let (label, len) = ctx.data.add_string(method.as_bytes());
        abi::emit_symbol_address(ctx.emitter, abi::int_arg_reg_name(ctx.emitter.target, 1), &label);
        abi::emit_load_int_immediate(ctx.emitter, abi::int_arg_reg_name(ctx.emitter.target, 2), len as i64);
    } else {
        abi::emit_load_temporary_stack_slot(ctx.emitter, abi::int_arg_reg_name(ctx.emitter.target, 1), EVAL_STACK_BYTES);
        abi::emit_load_temporary_stack_slot(ctx.emitter, abi::int_arg_reg_name(ctx.emitter.target, 2), EVAL_STACK_BYTES + 8);
    }
    abi::emit_temporary_stack_address(ctx.emitter, abi::int_arg_reg_name(ctx.emitter.target, 3), 0);
    let scope = ctx.defined_class_scope().unwrap_or("").to_string();
    let (scope_label, scope_len) = ctx.data.add_string(scope.as_bytes());
    abi::emit_symbol_address(ctx.emitter, abi::int_arg_reg_name(ctx.emitter.target, 4), &scope_label);
    abi::emit_load_int_immediate(ctx.emitter, abi::int_arg_reg_name(ctx.emitter.target, 5), scope_len as i64);
    let symbol = ctx.emitter.target.extern_symbol("__elephc_eval_method_lookup_v1");
    abi::emit_call_label(ctx.emitter, &symbol);
    retire_eval_metadata_operand_boxes(ctx, &[(object, EVAL_TEMP_CELL_OFFSET)])?;
    emit_eval_status_check(ctx);
    abi::emit_release_temporary_stack(ctx.emitter, EVAL_STACK_BYTES);
    Ok(())
}
