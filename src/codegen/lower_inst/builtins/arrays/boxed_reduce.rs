//! Purpose:
//! Lowers array reduction with arbitrary PHP source elements and carry values.
//!
//! Called from:
//! - The typed ArrayReduce runtime-function dispatcher.
//!
//! Key details:
//! - Stack cells borrow EIR arguments; the runtime acquires its own payload snapshots.
//! - Source validation precedes callback normalization, including for empty arrays.
//! - The runtime consumes the resolved descriptor and returns an owned Mixed cell.

use super::*;
use crate::codegen::lower_inst::{callables, callable_argument_normalizer};
use crate::codegen_support::{callable_descriptor, sentinels};

const BORROWED_BYTES: usize = 96;

/// Converts borrowed operands to the uniform reduction ABI without narrowing the carry type.
pub(crate) fn lower_array_reduce(ctx: &mut FunctionContext<'_>, inst: &Instruction) -> Result<()> {
    super::super::ensure_arg_count(inst, "array_reduce", 3)?;
    let source = expect_operand(inst, 0)?;
    let callback = expect_operand(inst, 1)?;
    let initial = expect_operand(inst, 2)?;
    abi::emit_reserve_temporary_stack(ctx.emitter, BORROWED_BYTES);
    for (value, offset) in [(source, 0), (initial, 32), (callback, 64)] {
        super::boxed_membership::store_borrowed_cell(ctx, value, offset)?;
    }
    validate_source(ctx);
    let result = abi::int_result_reg(ctx.emitter);
    match ctx.value_php_type(callback)?.codegen_repr() {
        PhpType::Callable => {
            ctx.load_value_to_result(callback)?;
            callable_descriptor::emit_retain_current_descriptor(ctx.emitter);
        }
        PhpType::Mixed => {
            callables::emit_runtime_mixed_callable_descriptor_value_with_type_error(
                ctx, callback, "array_reduce", "array_reduce(): Argument #2 ($callback) must be a valid callback",
            )?;
        }
        PhpType::Str => {
            callables::emit_runtime_string_descriptor_value_with_type_error(
                ctx, callback, result, "array_reduce",
                super::super::instruction_strict_php_profile(inst),
                "array_reduce(): Argument #2 ($callback) must be a valid callback",
            )?;
            callable_descriptor::emit_retain_current_descriptor(ctx.emitter);
        }
        _ => {
            ctx.shared.callable_argument_normalizer = true;
            abi::emit_temporary_stack_address(ctx.emitter, result, 64);
            abi::emit_call_label(ctx.emitter, callable_argument_normalizer::CALLABLE_ARGUMENT_NORMALIZER);
        }
    }
    abi::emit_reg_move(ctx.emitter, abi::int_arg_reg_name(ctx.emitter.target, 0), result);
    for (index, offset) in [(1, 0), (2, 32)] {
        abi::emit_temporary_stack_address(ctx.emitter, abi::int_arg_reg_name(ctx.emitter.target, index), offset);
    }
    abi::emit_call_label(ctx.emitter, "__rt_array_reduce_boxed");
    abi::emit_release_temporary_stack(ctx.emitter, BORROWED_BYTES);
    store_if_result(ctx, inst)
}

/// Rejects non-array or sentinel payloads before any helper-owned descriptor is acquired.
fn validate_source(ctx: &mut FunctionContext<'_>) {
    let invalid = ctx.next_label("array_reduce_invalid_source");
    let valid = ctx.next_label("array_reduce_valid_source");
    abi::emit_temporary_stack_address(ctx.emitter, abi::int_result_reg(ctx.emitter), 0);
    abi::emit_call_label(ctx.emitter, "__rt_mixed_unbox");
    match ctx.emitter.target.arch {
        Arch::AArch64 => {
            ctx.emitter.instruction("sub x9, x0, #4");                          // packed and hash tags map to zero and one
            ctx.emitter.instruction("cmp x9, #1");                              // every other PHP type is invalid
            ctx.emitter.instruction(&format!("b.hi {invalid}"));                // validate before reading a container header
            sentinels::emit_branch_if_null_container(ctx.emitter, "x1", "x9", &invalid);
        }
        Arch::X86_64 => {
            ctx.emitter.instruction("lea r10, [rax - 4]");                      // packed and hash tags map to zero and one
            ctx.emitter.instruction("cmp r10, 1");                              // every other PHP type is invalid
            ctx.emitter.instruction(&format!("ja {invalid}"));                  // validate before reading a container header
            sentinels::emit_branch_if_null_container(ctx.emitter, "rdi", "r10", &invalid);
        }
    }
    abi::emit_jump(ctx.emitter, &valid);
    ctx.emitter.label(&invalid);
    abi::emit_release_temporary_stack(ctx.emitter, BORROWED_BYTES);
    crate::codegen::lower_inst::exceptions::emit_type_error(
        ctx, "array_reduce(): Argument #1 ($array) must be of type array",
    );
    ctx.emitter.label(&valid);
}
