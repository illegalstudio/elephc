//! Purpose:
//! Answers native interface dispatch for objects an eval context created.
//!
//! Called from:
//! - `crate::codegen::lower_inst::iterators::emit_interface_dispatch_call_with_miss`, when the
//!   interface-table scan finds no entry for the receiver.
//! - The SPL `iterator_*` lowering, when neither native `Iterator` nor `IteratorAggregate`
//!   matches the receiver it saved.
//!
//! Key details:
//! - An object the interpreter built carries the generic `stdClass` header whatever class
//!   declared it, so the interface-table scan and `__rt_exception_matches` never see the
//!   interfaces it really implements. `foreach` then iterated it as empty and
//!   `iterator_to_array()` fataled with "unsupported kind".
//! - Both helpers ask the bridge about ownership first and only then box the receiver, so an
//!   AOT object pays one probe call on these (already failing) paths and nothing else.
//! - The bridge borrows the boxed receiver; this frame releases it on every path.

use super::*;

/// Calls a zero-argument interface method on an eval-owned receiver whose interface scan missed.
///
/// Precondition: the receiver object pointer is in the first integer argument register, as the
/// interface-table scan leaves it. When the receiver is not eval-owned, or its class does not
/// implement `interface_name`, control reaches `native_miss_label` with the stack balanced and
/// nothing released or retained. Otherwise the method's result is left in the result registers
/// in the representation `return_ty` describes, and control falls through.
pub(in crate::codegen::lower_inst) fn emit_eval_interface_method_on_miss(
    ctx: &mut FunctionContext<'_>,
    interface_name: &str,
    method_name: &str,
    return_ty: &PhpType,
    native_miss_label: &str,
) -> Result<()> {
    let receiver_arg = abi::int_arg_reg_name(ctx.emitter.target, 0);
    let result_reg = abi::int_result_reg(ctx.emitter);
    let not_owned = ctx.next_label("eval_interface_not_owned");
    let not_implemented = ctx.next_label("eval_interface_not_implemented");
    let done = ctx.next_label("eval_interface_done");
    abi::emit_push_reg(ctx.emitter, receiver_arg);
    let owner_probe = ctx
        .emitter
        .target
        .extern_symbol("__elephc_eval_object_has_dynamic_owner");
    abi::emit_call_label(ctx.emitter, &owner_probe);
    abi::emit_branch_if_int_result_zero(ctx.emitter, &not_owned);

    let stack_bytes = eval_method_call_stack_bytes(0);
    abi::emit_reserve_temporary_stack(ctx.emitter, stack_bytes);
    abi::emit_load_temporary_stack_slot(ctx.emitter, result_reg, stack_bytes);
    emit_box_current_value_as_mixed(ctx.emitter, &PhpType::Object(interface_name.to_string()));
    abi::emit_store_to_sp(ctx.emitter, result_reg, EVAL_TEMP_CELL_OFFSET);
    let boxed: EvalBoxedOperands = vec![EVAL_TEMP_CELL_OFFSET];

    emit_eval_boxed_temp_is_a_call(ctx, interface_name);
    emit_branch_unless_is_a_match(ctx, &not_implemented);

    abi::emit_load_int_immediate(ctx.emitter, result_reg, 0);
    abi::emit_store_to_sp(ctx.emitter, result_reg, EVAL_STACK_BYTES); // an empty argument pack
    abi::emit_load_int_immediate(ctx.emitter, abi::int_arg_reg_name(ctx.emitter.target, 0), 0);
    abi::emit_load_temporary_stack_slot(
        ctx.emitter,
        abi::int_arg_reg_name(ctx.emitter.target, 1),
        EVAL_TEMP_CELL_OFFSET,
    );
    let (method_label, method_len) = ctx.data.add_string(method_name.as_bytes());
    abi::emit_symbol_address(ctx.emitter, abi::int_arg_reg_name(ctx.emitter.target, 2), &method_label);
    abi::emit_load_int_immediate(
        ctx.emitter,
        abi::int_arg_reg_name(ctx.emitter.target, 3),
        method_len as i64,
    );
    abi::emit_temporary_stack_address(
        ctx.emitter,
        abi::int_arg_reg_name(ctx.emitter.target, 4),
        EVAL_STACK_BYTES,
    );
    abi::emit_temporary_stack_address(ctx.emitter, abi::int_arg_reg_name(ctx.emitter.target, 5), 0);
    let method_call = ctx.emitter.target.extern_symbol("__elephc_eval_method_call");
    abi::emit_call_label(ctx.emitter, &method_call);
    emit_branch_if_eval_c_int_negative(ctx, &not_implemented);
    emit_eval_status_check_releasing(ctx, &boxed);
    emit_release_eval_boxed_operands_keeping_result(ctx, &boxed);
    abi::emit_load_temporary_stack_slot(ctx.emitter, result_reg, EVAL_RESULT_VALUE_CELL_OFFSET);
    if matches!(return_ty.codegen_repr(), PhpType::Void | PhpType::Never) {
        abi::emit_decref_if_refcounted(ctx.emitter, &PhpType::Mixed); // a void method's result cell is nobody's
        abi::emit_load_int_immediate(ctx.emitter, result_reg, 0);
    } else {
        emit_eval_owned_result_as_type(ctx, return_ty)?;
    }
    abi::emit_release_temporary_stack(ctx.emitter, stack_bytes);
    abi::emit_release_temporary_stack(ctx.emitter, 16); // drop the saved receiver
    abi::emit_jump(ctx.emitter, &done);

    ctx.emitter.label(&not_implemented);
    emit_release_eval_boxed_operands(ctx, &boxed);
    abi::emit_release_temporary_stack(ctx.emitter, stack_bytes);
    ctx.emitter.label(&not_owned);
    abi::emit_pop_reg(ctx.emitter, receiver_arg);
    abi::emit_jump(ctx.emitter, native_miss_label);

    ctx.emitter.label(&done);
    Ok(())
}

/// Branches to `matched_label` when the object saved at the top of the temporary stack is an
/// eval-owned instance of `interface_name`.
///
/// Leaves the stack and the saved object untouched on both paths; the result registers are
/// clobbered.
pub(in crate::codegen::lower_inst) fn emit_branch_if_eval_saved_object_is_a(
    ctx: &mut FunctionContext<'_>,
    interface_name: &str,
    matched_label: &str,
) {
    let result_reg = abi::int_result_reg(ctx.emitter);
    let not_owned = ctx.next_label("eval_saved_object_not_owned");
    abi::emit_load_temporary_stack_slot(
        ctx.emitter,
        abi::int_arg_reg_name(ctx.emitter.target, 0),
        0,
    );
    let owner_probe = ctx
        .emitter
        .target
        .extern_symbol("__elephc_eval_object_has_dynamic_owner");
    abi::emit_call_label(ctx.emitter, &owner_probe);
    abi::emit_branch_if_int_result_zero(ctx.emitter, &not_owned);
    abi::emit_reserve_temporary_stack(ctx.emitter, EVAL_STACK_BYTES);
    abi::emit_load_temporary_stack_slot(ctx.emitter, result_reg, EVAL_STACK_BYTES);
    emit_box_current_value_as_mixed(ctx.emitter, &PhpType::Object(interface_name.to_string()));
    abi::emit_store_to_sp(ctx.emitter, result_reg, EVAL_TEMP_CELL_OFFSET);
    emit_eval_boxed_temp_is_a_call(ctx, interface_name);
    emit_release_eval_boxed_operands_keeping_int_answer(ctx, &[EVAL_TEMP_CELL_OFFSET]);
    abi::emit_release_temporary_stack(ctx.emitter, EVAL_STACK_BYTES);
    let not_matched = ctx.next_label("eval_saved_object_not_matched");
    emit_branch_unless_is_a_match(ctx, &not_matched);
    abi::emit_jump(ctx.emitter, matched_label);
    ctx.emitter.label(&not_matched);
    ctx.emitter.label(&not_owned);
}

/// Branches to `label` unless the `__elephc_eval_object_is_a` answer in the result register is 1.
///
/// A call made with no context gets 2 for an interpreted object that is definitely NOT an
/// instance. Testing the answer for zero read that 2 as a match: every interpreted
/// IteratorAggregate took the Iterator path, and `iterator_to_array()` called `rewind()` on it
/// (Laravel's dotenv `Some` option, on every request), while a method-on-miss dispatch called an
/// interface method the object does not implement.
fn emit_branch_unless_is_a_match(ctx: &mut FunctionContext<'_>, label: &str) {
    match ctx.emitter.target.arch {
        Arch::AArch64 => {
            ctx.emitter.instruction("cmp x0, #1");                              // 1 = the object is an instance of the interface
            ctx.emitter.instruction(&format!("b.ne {}", label));                // 0 and 2 are both "not an instance"
        }
        Arch::X86_64 => {
            ctx.emitter.instruction("cmp rax, 1");                              // 1 = the object is an instance of the interface
            ctx.emitter.instruction(&format!("jne {}", label));                 // 0 and 2 are both "not an instance"
        }
    }
}

/// Asks the bridge whether the receiver boxed at `EVAL_TEMP_CELL_OFFSET` is a `target_class`.
///
/// Passes a null context so the bridge answers from the object's own owning context.
fn emit_eval_boxed_temp_is_a_call(ctx: &mut FunctionContext<'_>, target_class: &str) {
    abi::emit_load_int_immediate(ctx.emitter, abi::int_arg_reg_name(ctx.emitter.target, 0), 0);
    abi::emit_load_temporary_stack_slot(
        ctx.emitter,
        abi::int_arg_reg_name(ctx.emitter.target, 1),
        EVAL_TEMP_CELL_OFFSET,
    );
    let (target_label, target_len) = ctx.data.add_string(target_class.as_bytes());
    abi::emit_symbol_address(ctx.emitter, abi::int_arg_reg_name(ctx.emitter.target, 2), &target_label);
    abi::emit_load_int_immediate(
        ctx.emitter,
        abi::int_arg_reg_name(ctx.emitter.target, 3),
        target_len as i64,
    );
    abi::emit_load_int_immediate(ctx.emitter, abi::int_arg_reg_name(ctx.emitter.target, 4), 0);
    let symbol = ctx.emitter.target.extern_symbol("__elephc_eval_object_is_a");
    abi::emit_call_label(ctx.emitter, &symbol);
}
