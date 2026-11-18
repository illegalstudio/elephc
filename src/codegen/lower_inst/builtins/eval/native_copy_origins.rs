//! Purpose:
//! Copies producer metadata without sharing the source value or its binding lifetime.
//!
//! Called from:
//! - EvalScopeCopyBindingType EIR after a fresh native destination is prepared.
//!
//! Key details:
//! - Native predecessors select checked descriptors; eval predecessors query live origins.
//! - Pure scope descriptors and interpreter types share the existing native signature grammar.

use super::*;

const SCOPE: usize = 0;
const DESCRIPTOR: usize = 8;
const METADATA: usize = 24;
const NATIVE_ORIGIN: usize = 80;

/// Reads immutable producer metadata without retaining the source value or its activation.
pub(in crate::codegen::lower_inst::builtins) fn lower_eval_scope_binding_type(
    ctx: &mut FunctionContext<'_>, inst: &Instruction,
) -> Result<()> {
    let name = super::scope_access::eval_scope_instruction_name(ctx, inst)?;
    let (label, len) = ctx.data.add_string(name.as_bytes());
    abi::emit_reserve_temporary_stack(ctx.emitter, EVAL_STACK_BYTES);
    ensure_eval_scope(ctx)?;
    load_eval_scope_to_arg(ctx, 0);
    abi::emit_symbol_address(ctx.emitter, abi::int_arg_reg_name(ctx.emitter.target, 1), &label);
    abi::emit_load_int_immediate(ctx.emitter, abi::int_arg_reg_name(ctx.emitter.target, 2), len as i64);
    let entry = ctx.emitter.target.extern_symbol("__elephc_reference_scope_type_v1");
    abi::emit_call_label(ctx.emitter, &entry);
    abi::emit_release_temporary_stack(ctx.emitter, EVAL_STACK_BYTES);
    store_if_result(ctx, inst)
}

/// Attaches one independent origin through shared target ABI and canonical reference helpers.
pub(in crate::codegen::lower_inst::builtins) fn lower_eval_scope_copy_binding_type(
    ctx: &mut FunctionContext<'_>, inst: &Instruction,
) -> Result<()> {
    let Some(Immediate::EvalBindingCopy { source, destination, owner }) = inst.immediate else {
        return Err(CodegenIrError::invalid_module("binding copy requires source and owned destination metadata"));
    };
    let source_name = ctx.module.data.global_names.get(source.as_raw() as usize)
        .ok_or_else(|| CodegenIrError::invalid_module("binding copy source name is missing"))?.clone();
    let target_name = ctx.function.locals[destination.as_raw() as usize].name.clone()
        .ok_or_else(|| CodegenIrError::invalid_module("binding copy destination name is missing"))?;
    let target_name = target_name.strip_suffix("#cow").unwrap_or(&target_name);
    let source = ctx.data.add_string(source_name.as_bytes());
    let target = ctx.data.add_string(target_name.as_bytes());
    let offset = ctx.local_offset(owner)?;
    abi::emit_reserve_temporary_stack(ctx.emitter, EVAL_STACK_BYTES);
    ctx.load_value_to_result(expect_operand(inst, 0)?)?;
    abi::emit_store_to_sp(ctx.emitter, abi::int_result_reg(ctx.emitter), NATIVE_ORIGIN);
    ensure_eval_scope(ctx)?;
    load_eval_scope_to_arg(ctx, 0);
    abi::emit_store_to_sp(ctx.emitter, abi::int_arg_reg_name(ctx.emitter.target, 0), SCOPE);

    let selected = ctx.next_label("copy_origin_selected");
    if let Some(descriptor) = inst.operands.get(1) {
        ctx.load_value_to_result(*descriptor)?;
    } else {
        let query = ctx.next_label("copy_eval_origin");
        if crate::semantics::conversions::inferred_binding_contract(&inst.result_php_type).is_some() {
            abi::emit_load_temporary_stack_slot(ctx.emitter, abi::int_result_reg(ctx.emitter), NATIVE_ORIGIN);
            abi::emit_branch_if_int_result_zero(ctx.emitter, &query);
            let descriptor = crate::codegen::runtime_metadata::local_reference_descriptor(ctx.module, &inst.result_php_type)?;
            abi::emit_symbol_address(ctx.emitter, abi::int_result_reg(ctx.emitter), &descriptor);
            abi::emit_jump(ctx.emitter, &selected);
        }
        ctx.emitter.label(&query);
        load_scope(ctx);
        load_name(ctx, 1, &source);
        let entry = ctx.emitter.target.extern_symbol("__elephc_reference_scope_type_v1");
        abi::emit_call_label(ctx.emitter, &entry);
    }
    ctx.emitter.label(&selected);
    abi::emit_store_to_sp(ctx.emitter, abi::int_result_reg(ctx.emitter), DESCRIPTOR);
    store_copy_metadata(ctx, &source, &target);

    let open = ctx.next_label("copy_origin_open");
    abi::emit_branch_if_int_result_zero(ctx.emitter, &open);
    abi::emit_reg_move(ctx.emitter, abi::int_arg_reg_name(ctx.emitter.target, 1), abi::int_result_reg(ctx.emitter));
    abi::emit_frame_slot_address(ctx.emitter, abi::int_arg_reg_name(ctx.emitter.target, 0), offset);
    abi::emit_call_label(ctx.emitter, "__rt_reference_sources_bind_local");
    ctx.emitter.label(&open);
    load_scope(ctx);
    load_name(ctx, 1, &target);
    abi::emit_load_temporary_stack_slot(ctx.emitter, abi::int_arg_reg_name(ctx.emitter.target, 3), DESCRIPTOR);
    let entry = ctx.emitter.target.extern_symbol("__elephc_reference_scope_binding_v1");
    abi::emit_call_label(ctx.emitter, &entry);

    abi::load_at_offset(ctx.emitter, abi::int_result_reg(ctx.emitter), offset);
    abi::emit_call_label(ctx.emitter, "__rt_reference_cell_promote_mixed");
    let result = abi::int_result_reg(ctx.emitter);
    abi::emit_load_from_address(ctx.emitter, result, result, 0);
    abi::emit_store_to_sp(ctx.emitter, result, DESCRIPTOR);
    load_scope(ctx);
    abi::emit_temporary_stack_address(ctx.emitter, abi::int_arg_reg_name(ctx.emitter.target, 1), METADATA);
    abi::emit_load_temporary_stack_slot(ctx.emitter, abi::int_arg_reg_name(ctx.emitter.target, 2), DESCRIPTOR);
    let scratch = abi::tertiary_scratch_reg(ctx.emitter);
    abi::emit_load_int_immediate(ctx.emitter, scratch, 0);
    abi::emit_store_to_sp(ctx.emitter, scratch, EVAL_RESULT_ERROR_OFFSET);
    let entry = ctx.emitter.target.extern_symbol("__elephc_eval_scope_copy_binding_type_v1");
    abi::emit_call_label(ctx.emitter, &entry);
    emit_eval_status_check(ctx);
    crate::codegen::local_binding_state::mark(ctx, destination, true);
    abi::emit_release_temporary_stack(ctx.emitter, EVAL_STACK_BYTES);
    Ok(())
}

/// Builds one dependency-neutral versioned record in the caller's reserved scratch frame.
fn store_copy_metadata(ctx: &mut FunctionContext<'_>, source: &(String, usize), target: &(String, usize)) {
    use elephc_semantics::native_local_contract::{BindingCopyV1, ABI_VERSION};
    assert_eq!(std::mem::size_of::<BindingCopyV1>(), 48);
    let result = abi::int_result_reg(ctx.emitter);
    for (offset, value) in [(METADATA, ABI_VERSION), (METADATA + 16, source.1), (METADATA + 32, target.1)] {
        abi::emit_load_int_immediate(ctx.emitter, result, value as i64);
        abi::emit_store_to_sp(ctx.emitter, result, offset);
    }
    for (offset, label) in [(METADATA + 8, &source.0), (METADATA + 24, &target.0)] {
        abi::emit_symbol_address(ctx.emitter, result, label);
        abi::emit_store_to_sp(ctx.emitter, result, offset);
    }
    abi::emit_load_temporary_stack_slot(ctx.emitter, result, DESCRIPTOR);
    abi::emit_store_to_sp(ctx.emitter, result, METADATA + 40);
}

/// Reloads the retained scope pointer after metadata operations clobber argument registers.
fn load_scope(ctx: &mut FunctionContext<'_>) {
    abi::emit_load_temporary_stack_slot(ctx.emitter, abi::int_arg_reg_name(ctx.emitter.target, 0), SCOPE);
}

/// Materializes one static byte range in adjacent argument registers on every target.
fn load_name(ctx: &mut FunctionContext<'_>, index: usize, name: &(String, usize)) {
    abi::emit_symbol_address(ctx.emitter, abi::int_arg_reg_name(ctx.emitter.target, index), &name.0);
    abi::emit_load_int_immediate(ctx.emitter, abi::int_arg_reg_name(ctx.emitter.target, index + 1), name.1 as i64);
}
