//! Purpose:
//! Lowers generator yield operations and Generator runtime intrinsics.
//!
//! Called from:
//! - `crate::codegen::lower_inst::lower_instruction()` and sibling lowering helpers.
//!
//! Key details:
//! - Preserves EIR ownership, ABI ordering, runtime symbols, and target-aware lowering.

use super::*;

/// Lowers a `yield` / `yield <k> => <v>` suspension to the `__rt_gen_suspend`
/// coroutine primitive.
///
/// Operand layout from `ir_lower::lower_yield`: `[]` for `yield;`, `[value]`
/// for `yield $v`, `[key, value]` for `yield $k => $v`. The yielded value (and
/// explicit key, if any) are boxed into owned Mixed cells and passed as
/// `__rt_gen_suspend(key, value)`; a NULL key requests an auto-increment
/// integer key. The helper's result register holds the value delivered by the
/// next `send()`/`next()`, which becomes the SSA result of the yield.
///
/// An `Immediate::Bool(true)` marks a *delegated* yield emitted by the
/// `yield from <array>` desugaring. Those keys are forwarded verbatim, so the
/// call targets `__rt_gen_suspend_delegated`, which skips PHP's auto-key
/// bookkeeping exactly like `__rt_gen_delegate` does for inner generators.
pub(super) fn lower_generator_yield(ctx: &mut FunctionContext<'_>, inst: &Instruction) -> Result<()> {
    let target = ctx.emitter.target;
    let key_arg = abi::int_arg_reg_name(target, 0);
    let value_arg = abi::int_arg_reg_name(target, 1);
    let result_reg = abi::int_result_reg(ctx.emitter);

    let n = inst.operands.len();
    let value_operand = if n >= 1 {
        Some(inst.operands[n - 1])
    } else {
        None
    };
    let key_operand = if n >= 2 { Some(inst.operands[0]) } else { None };

    // -- materialize the yielded value as an owned Mixed cell and park it --
    match value_operand {
        Some(value) => emit_value_as_owned_mixed(ctx, value)?,
        None => emit_owned_null_mixed(ctx),
    }
    abi::emit_push_reg(ctx.emitter, result_reg);

    // -- materialize the key: explicit owned Mixed cell, or NULL for auto-key --
    match key_operand {
        Some(key) => {
            emit_value_as_owned_mixed(ctx, key)?;
            if key_arg != result_reg {
                ctx.emitter
                    .instruction(&format!("mov {}, {}", key_arg, result_reg)); // move the boxed key into the first argument register
            }
        }
        None => {
            abi::emit_load_int_immediate(ctx.emitter, key_arg, 0); // NULL key requests the auto-increment integer key path
        }
    }
    abi::emit_pop_reg(ctx.emitter, value_arg);

    let suspend_symbol = if matches!(inst.immediate, Some(Immediate::Bool(true))) {
        "__rt_gen_suspend_delegated"
    } else {
        "__rt_gen_suspend"
    };
    abi::emit_call_label(ctx.emitter, suspend_symbol);
    store_call_result(ctx, inst, &PhpType::Mixed)
}

/// Lowers `yield from <generator>` by delegating to the `__rt_gen_delegate`
/// runtime helper, which drives the inner generator on the current coroutine
/// stack and returns its `return` value (the value of the `yield from`
/// expression). `yield from <array>` is desugared into an iterator loop before
/// reaching the backend, so the operand here is always a Generator/Traversable.
pub(super) fn lower_generator_yield_from(ctx: &mut FunctionContext<'_>, inst: &Instruction) -> Result<()> {
    let operand = expect_operand(inst, 0)?;
    let target = ctx.emitter.target;
    let arg0 = abi::int_arg_reg_name(target, 0);
    let result_reg = abi::int_result_reg(ctx.emitter);
    ctx.load_value_to_result(operand)?; // inner generator pointer (borrowed)
    if arg0 != result_reg {
        ctx.emitter
            .instruction(&format!("mov {}, {}", arg0, result_reg)); // pass the inner generator as delegate argument 0
    }
    abi::emit_call_label(ctx.emitter, "__rt_gen_delegate");
    store_call_result(ctx, inst, &PhpType::Mixed)
}

/// Loads `value` and boxes it into an *owned* Mixed cell in the result register.
///
/// Scalars, strings, arrays, objects, and callables box into a freshly retained
/// Mixed cell. An already-`Mixed` operand is left borrowed by the boxer, so it
/// is increfed to give the callee its own reference (the generator stores the
/// cell into a persistent slot).
pub(super) fn emit_value_as_owned_mixed(ctx: &mut FunctionContext<'_>, value: ValueId) -> Result<()> {
    let ty = ctx.load_value_to_result(value)?;
    let repr = ty.codegen_repr();
    emit_box_current_value_as_mixed(ctx.emitter, &ty);
    if matches!(repr, PhpType::Mixed | PhpType::Union(_)) {
        abi::emit_call_label(ctx.emitter, "__rt_incref"); // own the borrowed Mixed cell handed to the generator
    }
    Ok(())
}

/// Boxes PHP null into an owned Mixed cell in the result register.
pub(super) fn emit_owned_null_mixed(ctx: &mut FunctionContext<'_>) {
    match ctx.emitter.target.arch {
        Arch::AArch64 => {
            ctx.emitter.instruction("mov x0, #8");                              // runtime tag 8 = PHP null
            ctx.emitter.instruction("mov x1, #0");                              // null has no low payload word
            ctx.emitter.instruction("mov x2, #0");                              // null has no high payload word
            ctx.emitter.instruction("bl __rt_mixed_from_value");                // allocate a boxed Mixed null cell
        }
        Arch::X86_64 => {
            ctx.emitter.instruction("mov rax, 8");                              // runtime tag 8 = PHP null
            ctx.emitter.instruction("xor edi, edi");                            // null has no low payload word
            ctx.emitter.instruction("xor esi, esi");                            // null has no high payload word
            ctx.emitter.instruction("call __rt_mixed_from_value");              // allocate a boxed Mixed null cell
        }
    }
}

/// Lowers built-in `Generator` methods to their runtime helpers.
pub(super) fn lower_generator_intrinsic(
    ctx: &mut FunctionContext<'_>,
    inst: &Instruction,
    intrinsic: IntrinsicCall,
) -> Result<()> {
    let param_types = generator_intrinsic_param_types(intrinsic);
    let ref_params = vec![false; param_types.len()];
    let call_args =
        materialize_direct_call_args_with_refs(ctx, &inst.operands, &param_types, &ref_params)?;
    let caller_stack_pad_bytes = direct_call_stack_pad_bytes(ctx, call_args.overflow_bytes);
    abi::emit_reserve_temporary_stack(ctx.emitter, caller_stack_pad_bytes);
    let helper = intrinsic.runtime_helper().ok_or_else(|| {
        CodegenIrError::invalid_module(format!(
            "Generator intrinsic {:?} has no runtime helper",
            intrinsic.kind()
        ))
    })?;
    abi::emit_call_label(ctx.emitter, helper);
    abi::emit_release_temporary_stack(ctx.emitter, caller_stack_pad_bytes);
    abi::emit_release_temporary_stack(ctx.emitter, call_args.overflow_bytes);
    store_call_result(ctx, inst, &generator_intrinsic_return_type(intrinsic))?;
    emit_ref_arg_writebacks(ctx, &call_args)
}

/// Returns ABI-visible parameter types for a `Generator` intrinsic call.
pub(super) fn generator_intrinsic_param_types(intrinsic: IntrinsicCall) -> Vec<PhpType> {
    let mut params = vec![PhpType::Object("Generator".to_string())];
    match intrinsic.kind() {
        IntrinsicCallKind::GeneratorSend => params.push(PhpType::Mixed),
        IntrinsicCallKind::GeneratorThrow => {
            params.push(PhpType::Object("Throwable".to_string()));
        }
        _ => {}
    }
    params
}

/// Returns the PHP result type produced by a `Generator` runtime helper.
pub(super) fn generator_intrinsic_return_type(intrinsic: IntrinsicCall) -> PhpType {
    match intrinsic.kind() {
        IntrinsicCallKind::GeneratorValid => PhpType::Bool,
        IntrinsicCallKind::GeneratorNext | IntrinsicCallKind::GeneratorRewind => PhpType::Void,
        _ => PhpType::Mixed,
    }
}

