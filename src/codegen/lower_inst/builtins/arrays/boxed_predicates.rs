//! Purpose:
//! Lowers keyed array predicates through one storage-neutral callback runtime.
//!
//! Called from:
//! - The ArrayFind, ArrayAny and ArrayAll runtime-function dispatchers.
//!
//! Key details:
//! - Borrowed stack triples do not acquire EIR operand ownership.
//! - Validate the array before acquiring the descriptor consumed by the runtime.

use super::*;
use crate::codegen::lower_inst::{callables, callable_argument_normalizer};
use crate::codegen_support::{callable_descriptor, sentinels};

const BORROWED_BYTES: usize = 64;

/// Returns the first matching value, retaining its PHP type and independent result owner.
pub(crate) fn lower_array_find(ctx: &mut FunctionContext<'_>, inst: &Instruction) -> Result<()> {
    lower_predicate(ctx, inst, "array_find", 0)
}

/// Returns whether any callback invocation is truthy, stopping at the first match.
pub(crate) fn lower_array_any(ctx: &mut FunctionContext<'_>, inst: &Instruction) -> Result<()> {
    lower_predicate(ctx, inst, "array_any", 1)
}

/// Returns whether every callback invocation is truthy, including true for an empty array.
pub(crate) fn lower_array_all(ctx: &mut FunctionContext<'_>, inst: &Instruction) -> Result<()> {
    lower_predicate(ctx, inst, "array_all", 2)
}

/// Normalizes the callback once and adapts the owned Mixed answer to the builtin result ABI.
fn lower_predicate(ctx: &mut FunctionContext<'_>, inst: &Instruction, name: &str, mode: i64) -> Result<()> {
    super::super::ensure_arg_count(inst, name, 2)?;
    let source = expect_operand(inst, 0)?;
    let callback = expect_operand(inst, 1)?;
    abi::emit_reserve_temporary_stack(ctx.emitter, BORROWED_BYTES);
    super::boxed_membership::store_borrowed_cell(ctx, source, 0)?;
    super::boxed_membership::store_borrowed_cell(ctx, callback, 32)?;
    validate_source(ctx, name, BORROWED_BYTES);
    let result = abi::int_result_reg(ctx.emitter);
    let callback_error = match mode {
        0 => "array_find(): Argument #2 ($callback) must be a valid callback",
        1 => "array_any(): Argument #2 ($callback) must be a valid callback",
        _ => "array_all(): Argument #2 ($callback) must be a valid callback",
    };
    acquire_callback_descriptor(ctx, inst, callback, name, callback_error)?;
    abi::emit_reg_move(ctx.emitter, abi::int_arg_reg_name(ctx.emitter.target, 0), result);
    abi::emit_temporary_stack_address(ctx.emitter, abi::int_arg_reg_name(ctx.emitter.target, 1), 0);
    abi::emit_load_int_immediate(ctx.emitter, abi::int_arg_reg_name(ctx.emitter.target, 2), mode);
    abi::emit_call_label(ctx.emitter, "__rt_array_predicate_boxed");
    if mode != 0 {
        abi::emit_store_to_sp(ctx.emitter, result, 0);
        abi::emit_call_label(ctx.emitter, "__rt_mixed_cast_bool");
        abi::emit_store_to_sp(ctx.emitter, result, 32);
        abi::emit_load_temporary_stack_slot(ctx.emitter, result, 0);
        abi::emit_call_label(ctx.emitter, "__rt_decref_mixed");
        abi::emit_load_temporary_stack_slot(ctx.emitter, result, 32);
    }
    abi::emit_release_temporary_stack(ctx.emitter, BORROWED_BYTES);
    store_if_result(ctx, inst)
}

/// Acquires the descriptor consumed by a keyed array scan; callback triples reside at sp+32.
pub(super) fn acquire_callback_descriptor(
    ctx: &mut FunctionContext<'_>, inst: &Instruction, callback: ValueId,
    name: &str, callback_error: &'static str,
) -> Result<()> {
    let result = abi::int_result_reg(ctx.emitter);
    match ctx.value_php_type(callback)?.codegen_repr() {
        PhpType::Callable => {
            ctx.load_value_to_result(callback)?;
            callable_descriptor::emit_retain_current_descriptor(ctx.emitter);
        }
        PhpType::Mixed => {
            callables::emit_runtime_mixed_callable_descriptor_value_with_type_error(
                ctx, callback, name, callback_error,
            )?;
        }
        PhpType::Str => {
            callables::emit_runtime_string_descriptor_value_with_type_error(
                ctx, callback, result, name,
                super::super::instruction_strict_php_profile(inst), callback_error,
            )?;
            callable_descriptor::emit_retain_current_descriptor(ctx.emitter);
        }
        _ => {
            ctx.shared.callable_argument_normalizer = true;
            abi::emit_temporary_stack_address(ctx.emitter, result, 32);
            abi::emit_call_label(ctx.emitter, callable_argument_normalizer::CALLABLE_ARGUMENT_NORMALIZER);
        }
    }
    Ok(())
}

/// Rejects scalar or sentinel array operands before descriptor ownership is acquired.
pub(super) fn validate_source(ctx: &mut FunctionContext<'_>, name: &str, stack_bytes: usize) {
    let invalid = ctx.next_label("array_predicate_invalid_source");
    let valid = ctx.next_label("array_predicate_valid_source");
    abi::emit_temporary_stack_address(ctx.emitter, abi::int_result_reg(ctx.emitter), 0);
    abi::emit_call_label(ctx.emitter, "__rt_mixed_unbox");
    match ctx.emitter.target.arch {
        Arch::AArch64 => {
            ctx.emitter.instruction("sub x9, x0, #4");                          // packed and hash tags map to zero and one
            ctx.emitter.instruction("cmp x9, #1");                              // other PHP values are not arrays
            ctx.emitter.instruction(&format!("b.hi {invalid}"));                // validate before inspecting a container header
            sentinels::emit_branch_if_null_container(ctx.emitter, "x1", "x9", &invalid);
        }
        Arch::X86_64 => {
            ctx.emitter.instruction("lea r10, [rax - 4]");                      // packed and hash tags map to zero and one
            ctx.emitter.instruction("cmp r10, 1");                              // other PHP values are not arrays
            ctx.emitter.instruction(&format!("ja {invalid}"));                  // validate before inspecting a container header
            sentinels::emit_branch_if_null_container(ctx.emitter, "rdi", "r10", &invalid);
        }
    }
    abi::emit_jump(ctx.emitter, &valid);
    ctx.emitter.label(&invalid);
    abi::emit_release_temporary_stack(ctx.emitter, stack_bytes);
    crate::codegen::lower_inst::exceptions::emit_type_error(
        ctx, &format!("{name}(): Argument #1 ($array) must be of type array"),
    );
    ctx.emitter.label(&valid);
}
