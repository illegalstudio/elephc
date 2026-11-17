//! Purpose:
//! Lowers the eval-bridge half of `__elephc_eval_reflection_class_build`: hands the class name
//! an eval-built member reflector stored to libelephc-magician, which returns a fresh,
//! complete `ReflectionClass`.
//!
//! Called from:
//! - `crate::codegen::lower_inst::builtins::reflection_class_build`, only when the module
//!   links the eval bridge.
//!
//! Key details:
//! - The synthetic getters own no eval local, so the call passes no context handle; Magician
//!   resolves the owning context itself, or builds in a transient context attached to this
//!   module's native catalog token (see `__elephc_eval_reflection_class_build_v1`).
//! - The operand is the slot's boxed Mixed cell, which EIR releases after the call; the name
//!   is BORROWED from it through `__rt_mixed_unbox` (a cast would publish a string copy the
//!   getter has no release point for). Unboxing leaves the pair in argument registers 1 and
//!   2 on ARM64 and in `rdi`/`rdx` on x86_64, so the pointer moves before argument 0 is set.
//! - The bridge returns an OWNED boxed Mixed cell, stored as the call's Mixed result (EIR
//!   types the call `mixed`, since the contract can only say `object`). A thrown
//!   status goes through the shared eval status helper; a missing context yields `false`.

use super::*;

/// Calls Magician with the stored class name and keeps the reflector it builds.
pub(in crate::codegen::lower_inst::builtins) fn lower_eval_reflection_class_build_call(
    ctx: &mut FunctionContext<'_>,
    inst: &Instruction,
) -> Result<()> {
    if let Some(result) = inst.result {
        let result_ty = ctx.value_php_type(result)?.codegen_repr();
        if result_ty != PhpType::Mixed {
            return Err(CodegenIrError::unsupported(format!(
                "__elephc_eval_reflection_class_build result of PHP type {:?}",
                result_ty
            )));
        }
    }
    // -- borrow the class name straight from the boxed slot value --
    let operand = expect_operand(inst, 0)?;
    let operand_ty = ctx.value_php_type(operand)?.codegen_repr();
    if !matches!(operand_ty, PhpType::Mixed | PhpType::Union(_)) {
        return Err(CodegenIrError::unsupported(format!(
            "__elephc_eval_reflection_class_build for PHP type {:?}",
            operand_ty
        )));
    }
    ctx.load_value_to_result(operand)?;
    abi::emit_call_label(ctx.emitter, "__rt_mixed_unbox");
    let missing = ctx.next_label("eval_reflection_class_missing");
    let not_name = ctx.next_label("eval_reflection_class_not_name");
    let done = ctx.next_label("eval_reflection_class_done");
    let (name_ptr, name_len) = match ctx.emitter.target.arch {
        Arch::AArch64 => {
            ctx.emitter.instruction("cmp x0, #1");                              // runtime tag 1 means the slot holds a class name
            ctx.emitter.instruction(&format!("b.ne {}", not_name));             // any other slot value builds no reflector
            ("x1", "x2")
        }
        Arch::X86_64 => {
            ctx.emitter.instruction("cmp rax, 1");                              // runtime tag 1 means the slot holds a class name
            ctx.emitter.instruction(&format!("jne {}", not_name));              // any other slot value builds no reflector
            ("rdi", "rdx")
        }
    };
    abi::emit_reserve_temporary_stack(ctx.emitter, EVAL_STACK_BYTES);
    // -- pass the module catalog token, the class name and the outcome slot to the bridge --
    abi::emit_reg_move(ctx.emitter, abi::int_arg_reg_name(ctx.emitter.target, 1), name_ptr);
    abi::emit_reg_move(ctx.emitter, abi::int_arg_reg_name(ctx.emitter.target, 2), name_len);
    let catalog = ctx.data.add_private_identity("L__elephc_eval_native_catalog");
    abi::emit_symbol_address(ctx.emitter, abi::int_arg_reg_name(ctx.emitter.target, 0), &catalog);
    abi::emit_temporary_stack_address(ctx.emitter, abi::int_arg_reg_name(ctx.emitter.target, 3), 0);
    let symbol = ctx.emitter.target.extern_symbol("__elephc_eval_reflection_class_build_v1");
    abi::emit_call_label(ctx.emitter, &symbol);
    emit_branch_if_eval_c_int_negative(ctx, &missing);
    emit_eval_status_check(ctx);
    let result_reg = abi::int_result_reg(ctx.emitter);
    abi::emit_load_temporary_stack_slot(ctx.emitter, result_reg, EVAL_RESULT_VALUE_CELL_OFFSET);
    abi::emit_release_temporary_stack(ctx.emitter, EVAL_STACK_BYTES);
    abi::emit_jump(ctx.emitter, &done);
    ctx.emitter.label(&missing);
    abi::emit_release_temporary_stack(ctx.emitter, EVAL_STACK_BYTES);
    ctx.emitter.label(&not_name);
    abi::emit_load_int_immediate(ctx.emitter, result_reg, 0);
    emit_box_current_value_as_mixed(ctx.emitter, &PhpType::Bool);
    ctx.emitter.label(&done);
    store_if_result(ctx, inst)
}
