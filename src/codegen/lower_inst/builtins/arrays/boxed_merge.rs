//! Purpose:
//! Lowers merges involving boxed PHP array declarations without copying the input containers.
//!
//! Called from:
//! - `super::reduce_sets::lower_array_merge()`.
//!
//! Key details:
//! - Borrowed payload/tag pairs describe both packed and associative inputs.
//! - The caller's original operands root the inputs throughout the runtime traversal.
//! - The helper returns an owned hash, which is transferred into the boxed result.

use super::*;

/// Borrows both sources, merges their entries, and transfers the fresh hash owner into Mixed storage.
pub(super) fn lower_boxed_array_merge(
    ctx: &mut FunctionContext<'_>,
    inst: &Instruction,
    first: ValueId,
    second: ValueId,
) -> Result<()> {
    if inst.result_php_type.codegen_repr() != PhpType::Mixed {
        return Err(CodegenIrError::unsupported(
            "boxed array_merge requires a boxed array result".to_string(),
        ));
    }
    load_borrowed_array_pair(ctx, first)?;
    match ctx.emitter.target.arch {
        Arch::AArch64 => abi::emit_push_reg_pair(ctx.emitter, "x1", "x0"),
        Arch::X86_64 => abi::emit_push_reg_pair(ctx.emitter, "rdi", "rax"),
    }
    load_borrowed_array_pair(ctx, second)?;
    match ctx.emitter.target.arch {
        Arch::AArch64 => {
            ctx.emitter.instruction("mov x2, x1");                              // preserve the second source payload before restoring the first
            ctx.emitter.instruction("mov x3, x0");                              // pass its actual packed or associative tag
            abi::emit_pop_reg_pair(ctx.emitter, "x0", "x1");
        }
        Arch::X86_64 => {
            ctx.emitter.instruction("mov rdx, rdi");                            // preserve the second borrowed payload
            ctx.emitter.instruction("mov rcx, rax");                            // preserve its actual layout discriminator
            abi::emit_pop_reg_pair(ctx.emitter, "rdi", "rsi");
        }
    }
    abi::emit_call_label(ctx.emitter, "__rt_array_merge_boxed");
    let valid = ctx.next_label("array_merge_boxed_valid");
    let bad_second = ctx.next_label("array_merge_boxed_bad_second");
    match ctx.emitter.target.arch {
        Arch::AArch64 => {
            ctx.emitter.instruction(&format!("cbnz x0, {valid}"));              // successful merges return an owned hash even when empty
            ctx.emitter.instruction("cmp x1, #2");                              // rejected inputs identify the offending argument without allocating
            ctx.emitter.instruction(&format!("b.eq {bad_second}"));             // select the second argument diagnostic
        }
        Arch::X86_64 => {
            ctx.emitter.instruction("test rax, rax");                           // inspect the helper result before boxing its owned hash
            ctx.emitter.instruction(&format!("jnz {valid}"));                   // only invalid inputs return zero
            ctx.emitter.instruction("cmp rdx, 2");                              // invalid input returns its one-based argument index
            ctx.emitter.instruction(&format!("je {bad_second}"));               // select the second argument diagnostic
        }
    }
    crate::codegen::lower_inst::exceptions::emit_type_error(
        ctx, "array_merge(): Argument #1 must be of type array",
    );
    ctx.emitter.label(&bad_second);
    crate::codegen::lower_inst::exceptions::emit_type_error(
        ctx, "array_merge(): Argument #2 must be of type array",
    );
    ctx.emitter.label(&valid);
    box_hash_result_for_mixed_builtin(ctx, inst, &PhpType::Mixed);
    store_if_result(ctx, inst)
}

/// Loads the mixed-unbox ABI pair without allocating a wrapper for a concrete array operand.
fn load_borrowed_array_pair(ctx: &mut FunctionContext<'_>, value: ValueId) -> Result<()> {
    let source_type = ctx.value_php_type(value)?.codegen_repr();
    ctx.load_value_to_result(value)?;
    let tag = match source_type {
        PhpType::Mixed => {
            abi::emit_call_label(ctx.emitter, "__rt_mixed_unbox");
            return Ok(());
        }
        PhpType::Array(_) => 4,
        PhpType::AssocArray { .. } => 5,
        _ => return Err(CodegenIrError::unsupported(
            "array_merge operand must use packed, associative, or boxed array storage".to_string(),
        )),
    };
    match ctx.emitter.target.arch {
        Arch::AArch64 => {
            ctx.emitter.instruction("mov x1, x0");                              // borrow the concrete payload without acquiring a temporary owner
            abi::emit_load_int_immediate(ctx.emitter, "x0", tag);
        }
        Arch::X86_64 => {
            ctx.emitter.instruction("mov rdi, rax");                            // use the mixed-unbox payload register for the concrete array
            abi::emit_load_int_immediate(ctx.emitter, "rax", tag);
        }
    }
    Ok(())
}
