//! Purpose:
//! Lowers PHP `extract()` through the materialized eval activation scope.
//!
//! Called from:
//! - The typed runtime-function dispatcher for `RuntimeFnId::Extract`.
//!
//! Key details:
//! - The source array is boxed for the bridge without transferring caller ownership.
//! - Scope locals are flushed before extraction and reloaded afterwards so runtime-named keys
//!   become visible to subsequent AOT and eval operations.

use super::*;

/// Extracts array keys into the current activation scope and returns the number written.
pub(in crate::codegen::lower_inst::builtins) fn lower_extract(
    ctx: &mut FunctionContext<'_>,
    inst: &Instruction,
) -> Result<()> {
    super::super::ensure_arg_count_between(inst, "extract", 1, 3)?;
    abi::emit_reserve_temporary_stack(ctx.emitter, EVAL_STACK_BYTES);

    ensure_eval_scope(ctx)?;
    let sync_locals = eval_sync_locals(ctx);
    flush_eval_scope_locals(ctx, &sync_locals)?;

    let array = expect_operand(inst, 0)?;
    let array_ty = ctx.load_value_to_result(array)?.codegen_repr();
    let boxed_source = !matches!(array_ty, PhpType::Mixed | PhpType::Union(_));
    if boxed_source {
        let boxing_ty = if matches!(array_ty, PhpType::Array(_)) {
            PhpType::Iterable
        } else {
            array_ty.clone()
        };
        emit_box_current_value_as_mixed(ctx.emitter, &boxing_ty);
    }
    let result_reg = abi::int_result_reg(ctx.emitter);
    abi::emit_store_to_sp(ctx.emitter, result_reg, EVAL_TEMP_CELL_OFFSET);

    if let Some(flags) = inst.operands.get(1).copied() {
        let flags_ty = ctx.load_value_to_result(flags)?.codegen_repr();
        if flags_ty != PhpType::Int {
            return Err(CodegenIrError::unsupported(format!(
                "extract() flags lowering for PHP type {:?}",
                flags_ty
            )));
        }
    } else {
        abi::emit_load_int_immediate(ctx.emitter, result_reg, 0);
    }
    abi::emit_store_to_sp(ctx.emitter, result_reg, EVAL_CODE_PTR_OFFSET);

    if let Some(prefix) = inst.operands.get(2).copied() {
        let prefix_ty = ctx.load_value_to_result(prefix)?.codegen_repr();
        if prefix_ty != PhpType::Str {
            return Err(CodegenIrError::unsupported(format!(
                "extract() prefix lowering for PHP type {:?}",
                prefix_ty
            )));
        }
        let (prefix_ptr, prefix_len) = abi::string_result_regs(ctx.emitter);
        abi::emit_store_to_sp(ctx.emitter, prefix_ptr, EVAL_CODE_LEN_OFFSET);
        abi::emit_store_to_sp(ctx.emitter, prefix_len, EVAL_GLOBAL_SCOPE_HANDLE_OFFSET);
    } else {
        abi::emit_load_int_immediate(ctx.emitter, result_reg, 0);
        abi::emit_store_to_sp(ctx.emitter, result_reg, EVAL_CODE_LEN_OFFSET);
        abi::emit_store_to_sp(ctx.emitter, result_reg, EVAL_GLOBAL_SCOPE_HANDLE_OFFSET);
    }

    load_eval_scope_to_arg(ctx, 0);
    abi::emit_load_temporary_stack_slot(
        ctx.emitter,
        abi::int_arg_reg_name(ctx.emitter.target, 1),
        EVAL_TEMP_CELL_OFFSET,
    );
    abi::emit_load_temporary_stack_slot(
        ctx.emitter,
        abi::int_arg_reg_name(ctx.emitter.target, 2),
        EVAL_CODE_PTR_OFFSET,
    );
    abi::emit_load_temporary_stack_slot(
        ctx.emitter,
        abi::int_arg_reg_name(ctx.emitter.target, 3),
        EVAL_CODE_LEN_OFFSET,
    );
    abi::emit_load_temporary_stack_slot(
        ctx.emitter,
        abi::int_arg_reg_name(ctx.emitter.target, 4),
        EVAL_GLOBAL_SCOPE_HANDLE_OFFSET,
    );
    let symbol = ctx.emitter.target.extern_symbol("__elephc_eval_extract");
    abi::emit_call_label(ctx.emitter, &symbol);
    emit_extract_status_check(ctx);
    abi::emit_store_to_sp(ctx.emitter, result_reg, EVAL_CALLED_CLASS_PTR_OFFSET);

    if boxed_source {
        abi::emit_load_temporary_stack_slot(ctx.emitter, result_reg, EVAL_TEMP_CELL_OFFSET);
        abi::emit_decref_if_refcounted(ctx.emitter, &PhpType::Mixed);
    }
    reload_eval_scope_locals(ctx, &sync_locals, None)?;
    abi::emit_load_temporary_stack_slot(ctx.emitter, result_reg, EVAL_CALLED_CLASS_PTR_OFFSET);
    abi::emit_release_temporary_stack(ctx.emitter, EVAL_STACK_BYTES);
    store_if_result(ctx, inst)
}

/// Converts a negative bridge result back into an eval status and raises it.
fn emit_extract_status_check(ctx: &mut FunctionContext<'_>) {
    let ok = ctx.next_label("extract_status_ok");
    let result_reg = abi::int_result_reg(ctx.emitter);
    match ctx.emitter.target.arch {
        Arch::AArch64 => {
            ctx.emitter.instruction(&format!("cmp {}, #0", result_reg));
            ctx.emitter.instruction(&format!("b.ge {}", ok));
            ctx.emitter
                .instruction(&format!("neg {}, {}", result_reg, result_reg));
        }
        Arch::X86_64 => {
            ctx.emitter
                .instruction(&format!("test {}, {}", result_reg, result_reg));
            ctx.emitter.instruction(&format!("jns {}", ok));
            ctx.emitter.instruction(&format!("neg {}", result_reg));
        }
    }
    emit_eval_status_check(ctx);
    ctx.emitter.label(&ok);
}
