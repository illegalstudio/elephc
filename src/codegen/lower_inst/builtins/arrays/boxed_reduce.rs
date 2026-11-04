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
    if try_lower_direct_int_reduce(ctx, inst, source, callback, initial)? {
        return store_if_result(ctx, inst);
    }
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
    abi::emit_load_int_immediate(
        ctx.emitter,
        abi::int_arg_reg_name(ctx.emitter.target, 3),
        ctx.lexical_class_id(),
    );
    abi::emit_call_label(ctx.emitter, "__rt_array_reduce_boxed");
    abi::emit_release_temporary_stack(ctx.emitter, BORROWED_BYTES);
    store_if_result(ctx, inst)
}

/// Reduces a packed `array<int>` through a typed raw int reducer.
///
/// The generic path boxes the source and initial value, builds an argument array, and invokes a
/// callable descriptor per element. A capture-free closure `(int $carry, int $value): int` can
/// run directly with both operands in integer registers and a raw accumulator, avoiding every
/// per-element allocation. Only reducers that cannot mutate or warn are admitted; the helper owns
/// no heap values, so a typed arithmetic fatal has nothing to leak.
fn try_lower_direct_int_reduce(
    ctx: &mut FunctionContext<'_>,
    inst: &Instruction,
    source: ValueId,
    callback: ValueId,
    initial: ValueId,
) -> Result<bool> {
    if ctx.value_php_type(source)?.codegen_repr() != PhpType::Array(Box::new(PhpType::Int))
        || ctx.value_php_type(initial)?.codegen_repr() != PhpType::Int
    {
        return Ok(false);
    }
    let Some(closure) = super::map_dispatch::capture_free_closure(ctx, callback)? else {
        return Ok(false);
    };
    if closure.params.len() != 2
        || closure.params.iter().any(|param| {
            param.by_ref || param.variadic || param.php_type.codegen_repr() != PhpType::Int
        })
        || closure.return_php_type.codegen_repr() != PhpType::Int
        || closure.flags.is_generator
        || closure.flags.is_fiber_wrapper
        || !safe_reducer_body(closure)
    {
        return Ok(false);
    }
    let symbol = crate::names::function_symbol(&closure.name);
    let callback_arg = abi::int_arg_reg_name(ctx.emitter.target, 0);
    let source_arg = abi::int_arg_reg_name(ctx.emitter.target, 1);
    let initial_arg = abi::int_arg_reg_name(ctx.emitter.target, 2);
    let env_arg = abi::int_arg_reg_name(ctx.emitter.target, 3);
    ctx.load_value_to_result(initial)?;
    abi::emit_push_reg(ctx.emitter, abi::int_result_reg(ctx.emitter));
    abi::emit_symbol_address(ctx.emitter, callback_arg, &symbol);
    ctx.load_value_to_reg(source, source_arg)?;
    abi::emit_pop_reg(ctx.emitter, initial_arg);
    abi::emit_load_int_immediate(ctx.emitter, env_arg, 0);
    abi::emit_call_label(ctx.emitter, "__rt_array_reduce_int_direct");
    crate::codegen_support::emit_box_current_value_as_mixed(ctx.emitter, &PhpType::Int);
    store_if_result(ctx, inst)?;
    Ok(true)
}

/// Returns whether a typed reducer cannot mutate state, warn, or throw through user code.
fn safe_reducer_body(closure: &crate::ir::Function) -> bool {
    closure.instructions.iter().all(|inst| {
        inst.effects
            .intersection(
                crate::ir::Effects::MAY_WARN
                    | crate::ir::Effects::WRITES_GLOBAL
                    | crate::ir::Effects::WRITES_HEAP
                    | crate::ir::Effects::WRITES_FS
                    | crate::ir::Effects::WRITES_PROCESS
                    | crate::ir::Effects::OUTPUT
                    | crate::ir::Effects::REFCOUNT_OP
                    | crate::ir::Effects::BLOCKING_IO
                    | crate::ir::Effects::NETWORK_IO,
            )
            .is_empty()
    })
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
