//! Purpose:
//! Lowers PHP array filtering through the boxed value/key predicate runtime.
//!
//! Called from:
//! - The typed ArrayFilter runtime-function dispatcher.
//!
//! Key details:
//! - Borrowed source/callback triples preserve EIR ownership and support null callbacks.
//! - PHP 8.6 validates mode values; earlier profiles use value-only mode for other integers.

use super::*;

const STACK_BYTES: usize = 96;
const MODE_OFFSET: usize = 64;
const DESCRIPTOR_OFFSET: usize = 80;

/// Validates the source, acquires an optional descriptor and transfers it into the filtering scan.
pub(crate) fn lower_array_filter(ctx: &mut FunctionContext<'_>, inst: &Instruction) -> Result<()> {
    super::super::ensure_arg_count(inst, "array_filter", 3)?;
    let array = expect_operand(inst, 0)?;
    let callback = expect_operand(inst, 1)?;
    let mode = expect_operand(inst, 2)?;
    abi::emit_reserve_temporary_stack(ctx.emitter, STACK_BYTES);
    super::boxed_membership::store_borrowed_cell(ctx, array, 0)?;
    super::boxed_membership::store_borrowed_cell(ctx, callback, 32)?;
    super::boxed_predicates::validate_source(ctx, "array_filter", STACK_BYTES);
    let result = abi::int_result_reg(ctx.emitter);
    ctx.load_value_to_result(mode)?;
    abi::emit_store_to_sp(ctx.emitter, result, MODE_OFFSET);
    let null_callback = ctx.next_label("array_filter_null_callback");
    let ready = ctx.next_label("array_filter_descriptor_ready");
    abi::emit_temporary_stack_address(ctx.emitter, result, 32);
    abi::emit_call_label(ctx.emitter, "__rt_mixed_unbox");
    match ctx.emitter.target.arch {
        Arch::AArch64 => {
            ctx.emitter.instruction("cmp x0, #8");                              // null callbacks select PHP's empty-value filtering
            ctx.emitter.instruction(&format!("b.eq {null_callback}"));          // omit callback normalization only for null
        }
        Arch::X86_64 => {
            ctx.emitter.instruction("cmp rax, 8");                              // null callbacks select PHP's empty-value filtering
            ctx.emitter.instruction(&format!("je {null_callback}"));            // omit callback normalization only for null
        }
    }
    super::boxed_predicates::acquire_callback_descriptor(
        ctx, inst, callback, "array_filter",
        "array_filter(): Argument #2 ($callback) must be a valid callback or null",
    )?;
    abi::emit_jump(ctx.emitter, &ready);
    ctx.emitter.label(&null_callback);
    abi::emit_load_int_immediate(ctx.emitter, result, 0);
    ctx.emitter.label(&ready);
    abi::emit_store_to_sp(ctx.emitter, result, DESCRIPTOR_OFFSET);
    normalize_filter_mode(ctx);
    abi::emit_load_temporary_stack_slot(ctx.emitter, abi::int_arg_reg_name(ctx.emitter.target, 0), DESCRIPTOR_OFFSET);
    abi::emit_temporary_stack_address(ctx.emitter, abi::int_arg_reg_name(ctx.emitter.target, 1), 0);
    abi::emit_load_temporary_stack_slot(ctx.emitter, abi::int_arg_reg_name(ctx.emitter.target, 2), MODE_OFFSET);
    abi::emit_call_label(ctx.emitter, "__rt_array_predicate_boxed");
    abi::emit_release_temporary_stack(ctx.emitter, STACK_BYTES);
    store_if_result(ctx, inst)
}

/// Maps PHP modes to scan modes 3/4/5, retiring the descriptor before a PHP 8.6 mode error.
fn normalize_filter_mode(ctx: &mut FunctionContext<'_>) {
    let result = abi::int_result_reg(ctx.emitter);
    let valid = ctx.next_label("array_filter_valid_mode");
    abi::emit_load_temporary_stack_slot(ctx.emitter, result, MODE_OFFSET);
    match ctx.emitter.target.arch {
        Arch::AArch64 => {
            ctx.emitter.instruction("cmp x0, #2");                              // an unsigned range check excludes negative and excessive modes
            ctx.emitter.instruction(&format!("b.ls {valid}"));                  // retain value, both and key mode selection
        }
        Arch::X86_64 => {
            ctx.emitter.instruction("cmp rax, 2");                              // an unsigned range check excludes negative and excessive modes
            ctx.emitter.instruction(&format!("jbe {valid}"));                   // retain value, both and key mode selection
        }
    }
    if crate::codegen::compile_php_version().version_id() >= 80600 {
        abi::emit_load_temporary_stack_slot(ctx.emitter, result, DESCRIPTOR_OFFSET);
        abi::emit_call_label(ctx.emitter, "__rt_callable_descriptor_release");
        abi::emit_release_temporary_stack(ctx.emitter, STACK_BYTES);
        crate::codegen::lower_inst::exceptions::emit_value_error(
            ctx, "array_filter(): Argument #3 ($mode) must be one of ARRAY_FILTER_USE_VALUE, ARRAY_FILTER_USE_KEY, or ARRAY_FILTER_USE_BOTH",
        );
    } else {
        abi::emit_load_int_immediate(ctx.emitter, result, 0);
    }
    ctx.emitter.label(&valid);
    match ctx.emitter.target.arch {
        Arch::AArch64 => ctx.emitter.instruction("add x0, x0, #3"),             // select the predicate runtime's filtering mode family
        Arch::X86_64 => ctx.emitter.instruction("add rax, 3"),                  // select the predicate runtime's filtering mode family
    }
    abi::emit_store_to_sp(ctx.emitter, result, MODE_OFFSET);
}
