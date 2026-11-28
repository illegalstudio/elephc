//! Purpose:
//! Shares Fiber-local magic accessor guards and unwind boundaries.
//!
//! Called from:
//! - Property setter lowering and initialized getter calls.
//!
//! Key details:
//! - Stack nodes are removed before normal return or rethrow.
//! - Aligned native receiver identities reserve bit zero for getter guards.

use super::*;
use crate::codegen_support::try_handlers::{
    TRY_HANDLER_DIAG_DEPTH_OFFSET, TRY_HANDLER_JMP_BUF_OFFSET, TRY_HANDLER_SLOT_SIZE,
};

pub(super) const MAGIC_SET_GUARD_FRAME_BYTES: usize = TRY_HANDLER_SLOT_SIZE + 64;
pub(super) const MAGIC_SET_GUARD_NODE_OFFSET: usize = 0;
const MAGIC_SET_GUARD_HANDLER_OFFSET: usize = 48;

/// Adds the caller-owned stack node unless the active chain already contains this pair.
pub(super) fn emit_magic_set_guard_push(
    ctx: &mut FunctionContext<'_>,
    receiver_offset: usize,
    name_offset: usize,
    node_offset: usize,
) {
    let target = ctx.emitter.target;
    abi::emit_load_temporary_stack_slot(
        ctx.emitter,
        abi::int_arg_reg_name(target, 0),
        receiver_offset,
    );
    abi::emit_load_temporary_stack_slot(
        ctx.emitter,
        abi::int_arg_reg_name(target, 1),
        name_offset,
    );
    abi::emit_load_temporary_stack_slot(
        ctx.emitter,
        abi::int_arg_reg_name(target, 2),
        name_offset + 8,
    );
    abi::emit_temporary_stack_address(
        ctx.emitter,
        abi::int_arg_reg_name(target, 3),
        node_offset,
    );
    abi::emit_call_label(ctx.emitter, "__rt_magic_set_guard_push");
}

/// Removes the exact stack node this invocation published.
pub(super) fn emit_magic_set_guard_pop(ctx: &mut FunctionContext<'_>, node_offset: usize) {
    abi::emit_temporary_stack_address(
        ctx.emitter,
        abi::int_arg_reg_name(ctx.emitter.target, 0),
        node_offset,
    );
    abi::emit_call_label(ctx.emitter, "__rt_magic_set_guard_pop");
}

/// Installs a local handler so an escaping `__set` cannot strand a stack guard node.
pub(super) fn emit_magic_set_exception_handler(ctx: &mut FunctionContext<'_>, caught_label: &str) {
    let scratch = abi::temp_int_reg(ctx.emitter.target);
    for (symbol, offset) in [
        ("_exc_handler_top", MAGIC_SET_GUARD_HANDLER_OFFSET),
        ("_exc_call_frame_top", MAGIC_SET_GUARD_HANDLER_OFFSET + 8),
        (
            "_rt_diag_suppression",
            MAGIC_SET_GUARD_HANDLER_OFFSET + TRY_HANDLER_DIAG_DEPTH_OFFSET,
        ),
    ] {
        abi::emit_load_symbol_to_reg(ctx.emitter, scratch, symbol, 0);
        abi::emit_store_to_sp(ctx.emitter, scratch, offset);
    }
    abi::emit_temporary_stack_address(ctx.emitter, scratch, MAGIC_SET_GUARD_HANDLER_OFFSET);
    abi::emit_store_reg_to_symbol(ctx.emitter, scratch, "_exc_handler_top", 0);
    abi::emit_temporary_stack_address(
        ctx.emitter,
        abi::int_arg_reg_name(ctx.emitter.target, 0),
        MAGIC_SET_GUARD_HANDLER_OFFSET + TRY_HANDLER_JMP_BUF_OFFSET,
    );
    ctx.emitter.save_exception_handler();
    abi::emit_branch_if_int_result_nonzero(ctx.emitter, caught_label);
}

/// Restores the handler and diagnostic state saved by the local `__set` boundary.
pub(super) fn emit_magic_set_exception_handler_pop(ctx: &mut FunctionContext<'_>) {
    let scratch = abi::temp_int_reg(ctx.emitter.target);
    for (symbol, offset) in [
        ("_exc_handler_top", MAGIC_SET_GUARD_HANDLER_OFFSET),
        (
            "_rt_diag_suppression",
            MAGIC_SET_GUARD_HANDLER_OFFSET + TRY_HANDLER_DIAG_DEPTH_OFFSET,
        ),
    ] {
        abi::emit_load_temporary_stack_slot(ctx.emitter, scratch, offset);
        abi::emit_store_reg_to_symbol(ctx.emitter, scratch, symbol, 0);
    }
}


/// Calls an EIR-selected getter with pair-specific recursion and balanced unwind state.
pub(super) fn lower_initialized_getter(ctx: &mut FunctionContext<'_>, inst: &Instruction,
    method: crate::ir::DataId, property: crate::ir::DataId,
) -> Result<()> {
    let object = expect_operand(inst, 0)?;
    let reg = abi::int_result_reg(ctx.emitter);
    ctx.load_value_to_reg(object, reg)?;
    abi::emit_push_reg(ctx.emitter, reg);
    let (label, length) = ctx.intern_string_data(property)?;
    let (ptr, len) = abi::string_result_regs(ctx.emitter);
    abi::emit_symbol_address(ctx.emitter, ptr, &label);
    abi::emit_load_int_immediate(ctx.emitter, len, length as i64);
    abi::emit_push_reg_pair(ctx.emitter, ptr, len);
    abi::emit_reserve_temporary_stack(ctx.emitter, MAGIC_SET_GUARD_FRAME_BYTES);
    let target = ctx.emitter.target;
    abi::emit_load_temporary_stack_slot(ctx.emitter, abi::int_arg_reg_name(target, 0), MAGIC_SET_GUARD_FRAME_BYTES + 16);
    abi::emit_load_temporary_stack_slot(ctx.emitter, abi::int_arg_reg_name(target, 1), MAGIC_SET_GUARD_FRAME_BYTES);
    abi::emit_load_temporary_stack_slot(ctx.emitter, abi::int_arg_reg_name(target, 2), MAGIC_SET_GUARD_FRAME_BYTES + 8);
    abi::emit_temporary_stack_address(ctx.emitter, abi::int_arg_reg_name(target, 3), MAGIC_SET_GUARD_NODE_OFFSET);
    abi::emit_call_label(ctx.emitter, "__rt_magic_get_guard_push");
    let recursive = ctx.next_label("initialized_getter_recursive");
    let caught = ctx.next_label("initialized_getter_caught");
    let done = ctx.next_label("initialized_getter_done");
    abi::emit_branch_if_int_result_zero(ctx.emitter, &recursive);
    emit_magic_set_exception_handler(ctx, &caught);
    let mut physical = inst.clone();
    physical.immediate = Some(Immediate::Data(method));
    super::method_dispatch::lower_method_call(ctx, &physical)?;
    if let Some(result) = inst.result {
        ctx.load_value_to_result(result)?;
        stash_result(ctx, &inst.result_php_type, false);
    }
    emit_magic_set_exception_handler_pop(ctx);
    emit_magic_set_guard_pop(ctx, MAGIC_SET_GUARD_NODE_OFFSET);
    if inst.result.is_some() { stash_result(ctx, &inst.result_php_type, true); }
    abi::emit_release_temporary_stack(ctx.emitter, MAGIC_SET_GUARD_FRAME_BYTES + 32);
    store_if_result(ctx, inst)?;
    abi::emit_jump(ctx.emitter, &done);
    ctx.emitter.label(&caught);
    emit_magic_set_exception_handler_pop(ctx);
    emit_magic_set_guard_pop(ctx, MAGIC_SET_GUARD_NODE_OFFSET);
    abi::emit_release_temporary_stack(ctx.emitter, MAGIC_SET_GUARD_FRAME_BYTES + 32);
    abi::emit_jump(ctx.emitter, "__rt_throw_current");
    ctx.emitter.label(&recursive);
    abi::emit_release_temporary_stack(ctx.emitter, MAGIC_SET_GUARD_FRAME_BYTES + 32);
    let name = ctx.module.data.strings.get(property.as_raw() as usize)
        .ok_or_else(|| CodegenIrError::missing_entry("property name", property.as_raw()))?;
    let message = elephc_semantics::initialization::property_message(name);
    super::exceptions::emit_error(ctx, &message);
    ctx.emitter.label(&done);
    Ok(())
}

/// Preserves all PHP getter return representations across nonallocating guard removal.
fn stash_result(ctx: &mut FunctionContext<'_>, ty: &PhpType, restore: bool) {
    let registers = match ty.codegen_repr() {
        PhpType::Str => {
            let (ptr, len) = abi::string_result_regs(ctx.emitter);
            vec![(ptr, 32), (len, 40)]
        }
        PhpType::Float => vec![(abi::float_result_reg(ctx.emitter), 32)],
        _ => vec![(abi::int_result_reg(ctx.emitter), 32)],
    };
    for (register, offset) in registers {
        if restore { abi::emit_load_temporary_stack_slot(ctx.emitter, register, offset); }
        else { abi::emit_store_to_sp(ctx.emitter, register, offset); }
    }
}
