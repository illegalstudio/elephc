//! Purpose:
//! Materializes method and by-reference call arguments with writeback.
//!
//! Called from:
//! - `crate::codegen::lower_inst::lower_instruction()` and sibling lowering helpers.
//!
//! Key details:
//! - Preserves EIR ownership, ABI ordering, runtime symbols, and target-aware lowering.
//! - Method argument coercions use the shared cleanup plan; callers retire its block before
//!   processing reference writebacks, whose addresses include the intervening cleanup bytes.
//! - Reference arguments borrow caller locals or indexed element addresses. Managed hash-entry
//!   cells and omitted defaults use scoped owner records; scalar-to-Mixed uses writeback cells.
//! - Closures can retain default cells beyond the call. Normal return and exception unwinding
//!   retire only the caller's lease, leaving any captured lease intact.
//! - Constructor-promoted borrowed properties still use the separate persistent-cell fallback.

use super::*;

/// Loads lexical instance-call arguments using local `this`, tracking coercion owners for cleanup.
pub(super) fn materialize_method_call_args_with_receiver_local_and_refs(
    ctx: &mut FunctionContext<'_>,
    receiver_slot: LocalSlotId,
    receiver_ty: &PhpType,
    operands: &[ValueId],
    param_types: &[PhpType],
    ref_params: &[bool],
    lifetime: RefArgCellLifetime,
) -> Result<CallArgMaterialization> {
    if operands.len() + 1 != param_types.len() {
        return Err(CodegenIrError::invalid_module(format!(
            "lexical instance call materialization received {} operands for {} params",
            operands.len(),
            param_types.len()
        )));
    }
    if ref_params.len() != param_types.len() {
        return Err(CodegenIrError::invalid_module(format!(
            "lexical instance call materialization received {} ref flags for {} params",
            ref_params.len(),
            param_types.len()
        )));
    }
    let visible_param_types = &param_types[1..];
    let visible_ref_params = &ref_params[1..];
    let preleased_ref_cells =
        plan_preleased_ref_arg_cells(ctx, operands, visible_ref_params)?;
    let mut ref_writebacks =
        plan_ref_arg_writebacks(ctx, operands, visible_param_types, visible_ref_params)?;
    let mut ref_temp_cells = plan_ref_arg_temp_cells(
        ctx,
        operands,
        visible_param_types,
        visible_ref_params,
        &ref_writebacks,
        lifetime,
    )?;
    emit_ref_arg_cell_block(
        ctx,
        &mut ref_writebacks,
        &mut ref_temp_cells,
    )?;
    let cleanup_slots = plan_call_arg_temp_cleanups(
        ctx, operands, visible_param_types, visible_ref_params, &[],
    )?;
    let cleanup_bytes = cleanup_slots.len() * CALL_ARG_TEMP_CLEANUP_BYTES;
    abi::emit_reserve_temporary_stack(ctx.emitter, cleanup_bytes);
    let abi_param_types = abi_param_types_for_refs(param_types, ref_params);
    let assignments =
        abi::build_outgoing_arg_assignments_for_target(ctx.emitter.target, &abi_param_types, 0);
    ctx.load_local_to_result(receiver_slot)?;
    abi::emit_push_result_value(ctx.emitter, receiver_ty);
    let mut arg_temp_bytes = call_arg_temp_slot_size(&abi_param_types[0]);
    for (index, (value, param_ty)) in operands.iter().zip(visible_param_types.iter()).enumerate() {
        if visible_ref_params[index] {
            materialize_ref_arg_address(
                ctx,
                *value,
                index,
                param_ty,
                arg_temp_bytes,
                &ref_writebacks,
                &ref_temp_cells,
                cleanup_bytes,
            )?;
            abi::emit_push_result_value(ctx.emitter, &PhpType::Int);
        } else {
            let cleanup = cleanup_slots.iter().find(|cleanup| cleanup.param_index == index);
            let push_ty = materialize_plain_call_arg(ctx, *value, param_ty, cleanup, arg_temp_bytes)?;
            abi::emit_push_result_value(ctx.emitter, &push_ty);
        }
        arg_temp_bytes += call_arg_temp_slot_size(&abi_param_types[index + 1]);
    }
    Ok(CallArgMaterialization {
        overflow_bytes: abi::materialize_outgoing_args(ctx.emitter, &assignments),
        ref_writebacks,
        ref_temp_cells,
        preleased_ref_cells,
        cleanup_slots,
        cleanup_bytes,
        cleanup_guard_bytes: 0,
        borrowed_stack_arg_bytes: 0,
    })
}

/// Loads a register receiver and method arguments, tracking coercion owners and reference cells.
pub(super) fn materialize_method_call_args_with_receiver_reg_and_refs(
    ctx: &mut FunctionContext<'_>,
    receiver_reg: &str,
    receiver_ty: &PhpType,
    operands: &[ValueId],
    param_types: &[PhpType],
    ref_params: &[bool],
    lifetime: RefArgCellLifetime,
) -> Result<CallArgMaterialization> {
    materialize_method_call_args_with_receiver_reg_refs_defaults_and_tail(
        ctx,
        receiver_reg,
        receiver_ty,
        operands,
        param_types,
        ref_params,
        lifetime,
        &[],
        None,
        None,
    )
}

/// Whether a call supplies every parameter but a trailing hidden `func_get_args()` collector.
///
/// An internal dispatcher (`__toString` for a string conversion, `offsetGet` for `$o[$k]`) calls
/// with the source arity. The callee's physical signature may also carry the collector: the
/// `func_args` pass adds it to every frame when backtraces need argument snapshots, and
/// `func_args::method_families` spreads it over a whole interface family when one member reads
/// its surplus arguments. The collector is then staged empty, exactly what a source call with
/// no surplus arguments passes.
pub(super) fn omits_only_hidden_collector(operand_count: usize, param_types: &[PhpType]) -> bool {
    operand_count >= 1
        && operand_count + 1 == param_types.len()
        && param_types.last().map(PhpType::codegen_repr) == Some(PhpType::Array(Box::new(PhpType::Mixed)))
}

/// Whether an omitted argument's declared default can be staged without evaluating PHP code.
///
/// A runtime-dispatched call (a `Mixed` or interface receiver) picks its method only once the
/// class is known, so it cannot run the checker's default planning. A literal default is the
/// value itself, and is all this path needs: `RecursiveIteratorIterator` calls
/// `$it->hasChildren()` with no argument, and Symfony Finder's override declares
/// `hasChildren(bool $allowLinks = false)`. Refusing that candidate left the iterator with
/// no implementation to call.
pub(super) fn omitted_default_is_materializable(
    default: Option<&crate::parser::ast::Expr>,
    param_ty: &PhpType,
) -> bool {
    use crate::parser::ast::ExprKind;
    let Some(default) = default else {
        return false;
    };
    match (&default.kind, param_ty.codegen_repr()) {
        // Only the two null spellings the materializer below can stage: a boxed null and the
        // tagged scalar null. A `?array` slot is a raw container pointer and has neither.
        (ExprKind::Null, PhpType::Mixed | PhpType::TaggedScalar) => true,
        (ExprKind::BoolLiteral(_), PhpType::Bool) => true,
        (ExprKind::IntLiteral(_), PhpType::Int) => true,
        (ExprKind::StringLiteral(_), PhpType::Str) => true,
        // A parameter widened to `mixed` by its call sites (`basePath($path = '')` called with a
        // string and with an int elsewhere) still has the literal as its default; the call stages
        // it boxed. Without these arms the candidate was dropped, a call with the argument
        // omitted found no implementation, and php's `$this->basePath()` died as "Call to a
        // member function basePath() on null".
        (
            ExprKind::BoolLiteral(_) | ExprKind::IntLiteral(_) | ExprKind::StringLiteral(_),
            PhpType::Mixed,
        ) => true,
        // `withCommands(array $commands = [])` with its parameter widened to `mixed` by its call
        // sites: the empty array is staged boxed. Without this arm the candidate was dropped and
        // Laravel's `Application::configure()` died on "Call to a member function withCommands()
        // on null" -- every artisan command did.
        (ExprKind::ArrayLiteral(items), PhpType::Array(_) | PhpType::Mixed) => items.is_empty(),
        _ => false,
    }
}

/// [`materialize_method_call_args_with_receiver_reg_and_refs`], staging each omitted
/// argument from `defaults` (one entry per parameter after the receiver) when it is a literal.
///
/// `variadic_tail`, when present, says the LAST parameter is a variadic tail: the call passes a
/// fresh array holding those operands (boxed), empty when there are none, and releases it after.
/// `tail_count_prefix` opens that array with the actual argument count, which a hidden
/// `func_get_args()` collector behind optional parameters reads as its element 0.
/// An omitted by-reference parameter gets a managed cell seeded with null; its write is discarded.
#[allow(clippy::too_many_arguments)]
pub(super) fn materialize_method_call_args_with_receiver_reg_refs_defaults_and_tail(
    ctx: &mut FunctionContext<'_>,
    receiver_reg: &str,
    receiver_ty: &PhpType,
    operands: &[ValueId],
    param_types: &[PhpType],
    ref_params: &[bool],
    lifetime: RefArgCellLifetime,
    defaults: &[Option<crate::parser::ast::Expr>],
    variadic_tail: Option<&[ValueId]>,
    tail_count_prefix: Option<usize>,
) -> Result<CallArgMaterialization> {
    // The collector itself declares no default; earlier parameters may.
    let omitted_collector = variadic_tail.is_none()
        && omits_only_hidden_collector(operands.len(), param_types)
        && defaults
            .get(param_types.len().wrapping_sub(2))
            .and_then(Option::as_ref)
            .is_none();
    let tail_index = (variadic_tail.is_some() || omitted_collector)
        .then(|| param_types.len().saturating_sub(1));
    // `function f(array $items = [])` called without `$items`: the omitted default is a fresh
    // empty array, staged and released exactly as an empty variadic tail is.
    let empty_array_default = |param_index: usize| {
        Some(param_index) != tail_index
            && matches!(
                param_types[param_index].codegen_repr(),
                PhpType::Array(_) | PhpType::Mixed
            )
            && defaults
                .get(param_index.wrapping_sub(1))
                .and_then(Option::as_ref)
                .is_some_and(|default| {
                    matches!(&default.kind, crate::parser::ast::ExprKind::ArrayLiteral(items) if items.is_empty())
                })
    };
    if operands.len() > param_types.len() {
        return Err(CodegenIrError::invalid_module(format!(
            "method call materialization received {} operands for {} params",
            operands.len(),
            param_types.len()
        )));
    }
    if ref_params.len() != param_types.len() {
        return Err(CodegenIrError::invalid_module(format!(
            "method call materialization received {} ref flags for {} params",
            ref_params.len(),
            param_types.len()
        )));
    }
    let ref_writebacks = plan_ref_arg_writebacks(ctx, operands, param_types, ref_params)?;
    let preleased_ref_cells = plan_preleased_ref_arg_cells(ctx, operands, ref_params)?;
    if !ref_writebacks.is_empty() {
        return Err(CodegenIrError::unsupported(
            "receiver-register method call with scalar-to-mixed by-reference writebacks",
        ));
    }
    // Dynamic constructors use the same reference lifetime selection as fixed construction.
    // Only classes with borrowed reference properties keep the persistent fallback.
    let mut ref_temp_cells =
        plan_ref_arg_temp_cells(ctx, operands, param_types, ref_params, &ref_writebacks, lifetime)?;
    if lifetime == RefArgCellLifetime::CallOnly {
        for param_index in operands.len().max(1)..param_types.len() {
            if ref_params[param_index] {
                ref_temp_cells.push(RefArgTempCell {
                    param_index,
                    source_value: None,
                    cell_ty: param_types[param_index].codegen_repr(),
                    cell_offset: 0,
                    retain_existing: false,
                });
            }
        }
    }
    // THE RECEIVER IS ALREADY IN A REGISTER, AND THE CELL BLOCK IS ALLOWED TO DESTROY
    // CALLER-SAVED ONES: materializing a cell's value runs `load_value_to_result` and may
    // call runtime helpers, so anything the caller left in a caller-saved register, or in
    // the integer result register, is gone by the time the receiver is staged below.
    //
    // Every caller that can reach a non-empty block therefore hands the receiver over in the
    // reserved CALLEE-SAVED nested-call register (`abi::nested_call_reg`, x19/r12), which
    // `crate::codegen::frame` reserves a save slot for. This check makes that a hard
    // contract instead of a coincidence: a future caller that passes a scratch register and
    // a by-reference argument needing a cell gets a compile-time backend error rather than a
    // constructor entered with the cell's value as `$this`.
    if !ref_temp_cells.is_empty() && receiver_reg != abi::nested_call_reg(ctx.emitter) {
        return Err(CodegenIrError::unsupported(format!(
            "receiver-register method call staging a by-reference cell with the receiver in \
             {receiver_reg} (needs the callee-saved nested-call register)"
        )));
    }
    let mut no_writebacks: Vec<RefArgWriteback> = Vec::new();
    emit_ref_arg_cell_block(ctx, &mut no_writebacks, &mut ref_temp_cells)?;
    let mut cleanup_slots = plan_call_arg_temp_cleanups(ctx, operands, param_types, ref_params, &[])?;
    // An omitted array parameter with no literal default is a variadic tail (see
    // `mixed_method_candidates`): the call passes a fresh array and releases it afterwards.
    for param_index in operands.len().max(1)..param_types.len() {
        if !ref_params[param_index]
            && (Some(param_index) == tail_index || empty_array_default(param_index))
        {
            cleanup_slots.push(CallArgTempCleanup {
                param_index,
                offset: cleanup_slots.len() * CALL_ARG_TEMP_CLEANUP_BYTES,
                ty: param_types[param_index].codegen_repr(),
            });
        }
    }
    let cleanup_bytes = cleanup_slots.len() * CALL_ARG_TEMP_CLEANUP_BYTES;
    abi::emit_reserve_temporary_stack(ctx.emitter, cleanup_bytes);
    let abi_param_types = abi_param_types_for_refs(param_types, ref_params);
    let assignments =
        abi::build_outgoing_arg_assignments_for_target(ctx.emitter.target, &abi_param_types, 0);
    move_reg_to_int_result(ctx, receiver_reg);
    abi::emit_push_result_value(ctx.emitter, receiver_ty);
    let mut arg_temp_bytes = call_arg_temp_slot_size(&abi_param_types[0]);
    for (index, (value, param_ty)) in operands
        .iter()
        .skip(1)
        .zip(param_types.iter().skip(1))
        .enumerate()
    {
        let param_index = index + 1;
        if ref_params[param_index] {
            materialize_ref_arg_address(
                ctx,
                *value,
                param_index,
                &param_types[param_index],
                arg_temp_bytes,
                &ref_writebacks,
                &ref_temp_cells,
                cleanup_bytes,
            )?;
            abi::emit_push_result_value(ctx.emitter, &PhpType::Int);
        } else {
            let cleanup = cleanup_slots.iter().find(|cleanup| cleanup.param_index == param_index);
            let push_ty = materialize_plain_call_arg(ctx, *value, param_ty, cleanup, arg_temp_bytes)?;
            abi::emit_push_result_value(ctx.emitter, &push_ty);
        }
        arg_temp_bytes += call_arg_temp_slot_size(&abi_param_types[param_index]);
    }
    for param_index in operands.len()..param_types.len() {
        if param_index == 0 {
            return Err(CodegenIrError::unsupported(
                "receiver-register method call with no receiver operand",
            ));
        }
        if ref_params[param_index] {
            materialize_omitted_ref_arg_address(
                ctx,
                param_index,
                &param_types[param_index],
                arg_temp_bytes,
                &ref_temp_cells,
                cleanup_bytes,
            )?;
            abi::emit_push_result_value(ctx.emitter, &PhpType::Int);
            arg_temp_bytes += call_arg_temp_slot_size(&abi_param_types[param_index]);
            continue;
        }
        let param_ty = param_types[param_index].codegen_repr();
        let literal_default = defaults
            .get(param_index - 1)
            .and_then(|default| default.as_ref())
            .map(|default| &default.kind);
        match (literal_default, &param_ty) {
            (Some(crate::parser::ast::ExprKind::BoolLiteral(value)), PhpType::Bool) => {
                abi::emit_load_int_immediate(
                    ctx.emitter,
                    abi::int_result_reg(ctx.emitter),
                    i64::from(*value),
                );
                abi::emit_push_result_value(ctx.emitter, &PhpType::Bool);
                arg_temp_bytes += call_arg_temp_slot_size(&abi_param_types[param_index]);
                continue;
            }
            (Some(crate::parser::ast::ExprKind::IntLiteral(value)), PhpType::Int) => {
                abi::emit_load_int_immediate(ctx.emitter, abi::int_result_reg(ctx.emitter), *value);
                abi::emit_push_result_value(ctx.emitter, &PhpType::Int);
                arg_temp_bytes += call_arg_temp_slot_size(&abi_param_types[param_index]);
                continue;
            }
            (Some(crate::parser::ast::ExprKind::StringLiteral(value)), PhpType::Str) => {
                let (label, len) = ctx.data.add_string(value.as_bytes());
                let (ptr_reg, len_reg) = abi::string_result_regs(ctx.emitter);
                abi::emit_symbol_address(ctx.emitter, ptr_reg, &label);
                abi::emit_load_int_immediate(ctx.emitter, len_reg, len as i64);
                abi::emit_push_result_value(ctx.emitter, &PhpType::Str);
                arg_temp_bytes += call_arg_temp_slot_size(&abi_param_types[param_index]);
                continue;
            }
            _ => {}
        }
        let is_empty_default = empty_array_default(param_index);
        if Some(param_index) == tail_index || is_empty_default {
            let tail = if is_empty_default { &[][..] } else { variadic_tail.unwrap_or(&[]) };
            let capacity = tail.len().max(4) as i64;
            abi::emit_load_int_immediate(ctx.emitter, abi::int_arg_reg_name(ctx.emitter.target, 0), capacity);
            abi::emit_load_int_immediate(ctx.emitter, abi::int_arg_reg_name(ctx.emitter.target, 1), 8);
            abi::emit_call_label(ctx.emitter, "__rt_array_new");
            let result_reg = abi::int_result_reg(ctx.emitter).to_string();
            let element_ty = match &param_ty {
                PhpType::Array(element) if is_empty_default => element.codegen_repr(),
                _ => PhpType::Mixed,
            };
            crate::codegen::emit_array_value_type_stamp(ctx.emitter, &result_reg, &element_ty);
            if let Some(count) = tail_count_prefix.filter(|_| !is_empty_default) {
                emit_push_int_onto_array(ctx, count as i64, &result_reg);
            }
            for value in tail {
                emit_push_boxed_onto_array(ctx, *value, &result_reg)?;
            }
            if is_empty_default && param_ty == PhpType::Mixed {
                // The box takes the fresh array's reference; the cleanup slot releases the box.
                crate::codegen::emit_box_current_owned_value_as_mixed(
                    ctx.emitter,
                    &PhpType::Array(Box::new(PhpType::Mixed)),
                );
            }
            if let Some(cleanup) = cleanup_slots
                .iter()
                .find(|cleanup| cleanup.param_index == param_index)
            {
                save_call_arg_temp_cleanup(ctx, cleanup, arg_temp_bytes);
            }
            abi::emit_push_result_value(ctx.emitter, &param_ty);
            arg_temp_bytes += call_arg_temp_slot_size(&abi_param_types[param_index]);
            continue;
        }
        match param_ty {
            PhpType::Mixed => {
                // A literal default is staged as itself, boxed; only an omitted parameter with
                // no literal default (or `null`) is the boxed null.
                match literal_default {
                    Some(crate::parser::ast::ExprKind::BoolLiteral(value)) => {
                        abi::emit_load_int_immediate(
                            ctx.emitter,
                            abi::int_result_reg(ctx.emitter),
                            i64::from(*value),
                        );
                        crate::codegen::emit_box_current_value_as_mixed(ctx.emitter, &PhpType::Bool);
                    }
                    Some(crate::parser::ast::ExprKind::IntLiteral(value)) => {
                        abi::emit_load_int_immediate(ctx.emitter, abi::int_result_reg(ctx.emitter), *value);
                        crate::codegen::emit_box_current_value_as_mixed(ctx.emitter, &PhpType::Int);
                    }
                    Some(crate::parser::ast::ExprKind::StringLiteral(value)) => {
                        let (label, len) = ctx.data.add_string(value.as_bytes());
                        let (ptr_reg, len_reg) = abi::string_result_regs(ctx.emitter);
                        abi::emit_symbol_address(ctx.emitter, ptr_reg, &label);
                        abi::emit_load_int_immediate(ctx.emitter, len_reg, len as i64);
                        crate::codegen::emit_box_current_value_as_mixed(ctx.emitter, &PhpType::Str);
                    }
                    _ => objects::emit_boxed_null(ctx),
                }
                abi::emit_push_result_value(ctx.emitter, &PhpType::Mixed);
            }
            PhpType::TaggedScalar => {
                crate::codegen::sentinels::emit_tagged_scalar_null(ctx.emitter);
                abi::emit_push_result_value(ctx.emitter, &PhpType::TaggedScalar);
            }
            _ => {
                return Err(CodegenIrError::unsupported(format!(
                    "receiver-register method call missing default for ABI type {:?} (parameter {} of {} in {})",
                    param_ty,
                    param_index,
                    param_types.len(),
                    ctx.function.name,
                )));
            }
        }
        arg_temp_bytes += call_arg_temp_slot_size(&abi_param_types[param_index]);
    }
    Ok(CallArgMaterialization {
        overflow_bytes: abi::materialize_outgoing_args(ctx.emitter, &assignments),
        ref_writebacks,
        ref_temp_cells,
        preleased_ref_cells,
        cleanup_slots,
        cleanup_bytes,
        cleanup_guard_bytes: 0,
        borrowed_stack_arg_bytes: 0,
    })
}

/// Appends `value`, boxed as Mixed, to the array in `array_reg` (which the append may move).
///
/// The array pointer is parked on the stack while the value is loaded and boxed. A value that is
/// already a Mixed cell is borrowed, and `__rt_array_push_refcounted` takes its own reference; a
/// cell boxed here is released once the array holds it.
fn emit_push_boxed_onto_array(
    ctx: &mut FunctionContext<'_>,
    value: ValueId,
    array_reg: &str,
) -> Result<()> {
    abi::emit_push_reg(ctx.emitter, array_reg);
    let ty = ctx.load_value_to_result(value)?.codegen_repr();
    emit_push_current_value_onto_array(ctx, &ty, array_reg);
    Ok(())
}

/// Appends a boxed integer literal to the array in `array_reg`.
fn emit_push_int_onto_array(ctx: &mut FunctionContext<'_>, value: i64, array_reg: &str) {
    abi::emit_push_reg(ctx.emitter, array_reg);
    abi::emit_load_int_immediate(ctx.emitter, abi::int_result_reg(ctx.emitter), value);
    emit_push_current_value_onto_array(ctx, &PhpType::Int, array_reg);
}

/// Appends the value in the result registers, of type `ty`, to the array saved on the stack by
/// the caller (`array_reg` pushed), leaving the possibly grown array in `array_reg`.
pub(super) fn emit_push_current_value_onto_array(
    ctx: &mut FunctionContext<'_>,
    ty: &PhpType,
    array_reg: &str,
) {
    let fresh_box = !matches!(ty, PhpType::Mixed | PhpType::Union(_));
    if fresh_box {
        crate::codegen::emit_box_current_value_as_mixed(ctx.emitter, ty);
    }
    let result_reg = abi::int_result_reg(ctx.emitter).to_string();
    let value_arg = abi::int_arg_reg_name(ctx.emitter.target, 1);
    let array_arg = abi::int_arg_reg_name(ctx.emitter.target, 0);
    abi::emit_reg_move(ctx.emitter, value_arg, &result_reg);
    abi::emit_pop_reg(ctx.emitter, array_arg);
    if fresh_box {
        abi::emit_push_reg(ctx.emitter, value_arg);
    }
    abi::emit_call_label(ctx.emitter, "__rt_array_push_refcounted");
    if fresh_box {
        // The array retained the fresh box; drop the reference this helper made.
        abi::emit_reg_move(ctx.emitter, array_arg, &result_reg);
        abi::emit_pop_reg(ctx.emitter, value_arg);
        abi::emit_push_reg(ctx.emitter, array_arg);
        abi::emit_reg_move(ctx.emitter, &result_reg, value_arg);
        abi::emit_decref_if_refcounted(ctx.emitter, &PhpType::Mixed);
        abi::emit_pop_reg(ctx.emitter, &result_reg);
    }
    if result_reg != array_reg {
        abi::emit_reg_move(ctx.emitter, array_reg, &result_reg);
    }
}

/// Converts declared parameter types to the ABI-visible shape for by-reference args.
pub(super) fn abi_param_types_for_refs(param_types: &[PhpType], ref_params: &[bool]) -> Vec<PhpType> {
    param_types
        .iter()
        .zip(ref_params.iter())
        .map(|(ty, is_ref)| {
            if *is_ref {
                PhpType::Int
            } else {
                ty.codegen_repr()
            }
        })
        .collect()
}

/// Returns the temporary stack slot size used by outgoing-argument staging.
pub(super) fn call_arg_temp_slot_size(ty: &PhpType) -> usize {
    if matches!(ty.codegen_repr(), PhpType::Void | PhpType::Never) {
        0
    } else {
        16
    }
}

/// Plans caller-side Mixed cells needed for scalar locals passed to by-reference Mixed params.
pub(super) fn plan_ref_arg_writebacks(
    ctx: &FunctionContext<'_>,
    args: &[ValueId],
    param_types: &[PhpType],
    ref_params: &[bool],
) -> Result<Vec<RefArgWriteback>> {
    let mut writebacks = Vec::new();
    for (param_index, value) in args.iter().enumerate() {
        if !ref_params[param_index] || param_types[param_index].codegen_repr() != PhpType::Mixed {
            continue;
        }
        // An acquired hash-entry cell already is the caller's writable Mixed storage. Looking
        // through it to the entry's earlier concrete payload type would incorrectly schedule a
        // scalar writeback against a value that has no local source slot.
        if value_is_acquired_array_element_ref_cell(ctx, *value)? {
            continue;
        }
        let source_ty = if let Some(array) = array_element_address_source(ctx, *value)? {
            match ctx.value_php_type(array)?.codegen_repr() {
                PhpType::Array(element) => element.codegen_repr(),
                PhpType::AssocArray { value, .. } => value.codegen_repr(),
                _ => {
                    return Err(CodegenIrError::invalid_module(
                        "array element address requires an array receiver",
                    ));
                }
            }
        } else {
            ctx.raw_value_php_type(*value)?.codegen_repr()
        };
        if matches!(source_ty, PhpType::Mixed | PhpType::Union(_)) {
            continue;
        }
        if local_ref_arg_source(ctx, *value).ok()
            .and_then(|source| ctx.local_php_type(source.slot).ok())
            .is_some_and(|storage| storage.codegen_repr() == PhpType::Mixed)
        {
            continue;
        }
        if array_element_address_source(ctx, *value)?.is_none()
            && local_ref_arg_source(ctx, *value).is_err()
        {
            // Omitted defaults have no caller location. The temporary-cell planner owns
            // their conversion and disposal, so there is no scalar/container writeback.
            continue;
        }
        reject_unsupported_mixed_ref_writeback_source(&source_ty)?;
        let source = local_ref_arg_source(ctx, *value)?;
        writebacks.push(RefArgWriteback {
            param_index,
            source_value: *value,
            source_slot: source.slot,
            source_ty,
            cell_offset: 0,
        });
    }
    Ok(writebacks)
}

/// Rejects scalar-to-Mixed temporary ref cells whose writeback shape is not supported yet.
pub(super) fn reject_unsupported_mixed_ref_writeback_source(source_ty: &PhpType) -> Result<()> {
    if matches!(
        source_ty.codegen_repr(),
        PhpType::Int
            | PhpType::Bool
            | PhpType::Array(_)
            | PhpType::AssocArray { .. }
            | PhpType::Object(_)
    ) {
        return Ok(());
    }
    Err(CodegenIrError::unsupported(format!(
        "by-reference Mixed parameter writeback to PHP type {:?}",
        source_ty
    )))
}

/// Plans managed reference-cell leases and default cells required by call arguments.
/// Writebacks, locals, and borrowed indexed element addresses need no additional cell.
pub(super) fn plan_ref_arg_temp_cells(
    ctx: &FunctionContext<'_>,
    args: &[ValueId],
    param_types: &[PhpType],
    ref_params: &[bool],
    writebacks: &[RefArgWriteback],
    lifetime: RefArgCellLifetime,
) -> Result<Vec<RefArgTempCell>> {
    let mut cells = Vec::new();
    // Constructor-promoted properties borrow without retaining. Until their ownership
    // model adopts managed cells, they must keep the separate persistent fallback.
    if lifetime == RefArgCellLifetime::MayOutliveCall {
        return Ok(cells);
    }
    for (param_index, value) in args.iter().enumerate() {
        if !ref_params[param_index] {
            continue;
        }
        if writebacks
            .iter()
            .any(|writeback| writeback.param_index == param_index)
        {
            continue;
        }
        if local_ref_arg_source(ctx, *value).is_ok() {
            continue;
        }
        if value_is_acquired_array_element_ref_cell(ctx, *value)? {
            continue;
        }
        if value_is_managed_array_element_ref_cell(ctx, *value)? {
            cells.push(RefArgTempCell {
                param_index,
                source_value: Some(*value),
                cell_ty: param_types[param_index].codegen_repr(),
                cell_offset: 0,
                retain_existing: true,
            });
            continue;
        }
        if value_is_array_element_address(ctx, *value)? {
            continue;
        }
        cells.push(RefArgTempCell {
            param_index,
            source_value: Some(*value),
            cell_ty: param_types[param_index].codegen_repr(),
            cell_offset: 0,
            retain_existing: false,
        });
    }
    Ok(cells)
}

/// Returns whether EIR already acquired the managed array-element cell used by this operand.
/// `Borrow` marks ledger cleanup, `Move` is a neutral forward and `RefCellMixedPayload` steps into
/// the cell's box, but all three still name the same acquired writable cell and must bypass scalar
/// writeback and backend fallback-cell planning.
fn value_is_acquired_array_element_ref_cell(
    ctx: &FunctionContext<'_>,
    mut value: ValueId,
) -> Result<bool> {
    loop {
        let Some(value_ref) = ctx.function.value(value) else {
            return Err(CodegenIrError::missing_entry("value", value.as_raw()));
        };
        let ValueDef::Instruction { inst, .. } = value_ref.def else {
            return Ok(false);
        };
        let inst_ref = ctx
            .function
            .instruction(inst)
            .ok_or_else(|| CodegenIrError::missing_entry("instruction", inst.as_raw()))?;
        match inst_ref.op {
            Op::AcquireRefCell => return Ok(true),
            Op::Borrow | Op::Move | Op::RefCellMixedPayload => {
                value = expect_operand(inst_ref, 0)?
            }
            _ => return Ok(false),
        }
    }
}

/// Finds managed cell owners acquired during EIR argument evaluation without an EIR ledger.
///
/// A ledger-protected operand carries an outer `Borrow`, while a bare `AcquireRefCell` delegates
/// normal-path retirement to the shared ABI call cleanup. The IR peephole preserves that marker:
/// searching the whole function for a later release would conflate mutually exclusive paths and
/// owner-slot lifetimes, and could suppress the only normal-path cleanup for this call.
pub(super) fn plan_preleased_ref_arg_cells(
    ctx: &FunctionContext<'_>,
    args: &[ValueId],
    ref_params: &[bool],
) -> Result<Vec<PreleasedRefArgCell>> {
    let mut cells = Vec::new();
    for (value, is_ref) in args.iter().zip(ref_params.iter()) {
        if !is_ref {
            continue;
        }
        let Some((owner_slot, publication_order)) = preleased_ref_cell_owner(ctx, *value)? else {
            continue;
        };
        if !cells
            .iter()
            .any(|(_, cell): &(u32, PreleasedRefArgCell)| cell.owner_slot == owner_slot)
        {
            cells.push((
                publication_order,
                PreleasedRefArgCell {
                    owner_slot,
                    cell_ty: PhpType::Mixed,
                },
            ));
        }
    }
    // Named arguments are stored in ABI parameter order, which can be the reverse of source
    // evaluation and cleanup-record publication. Instruction order is the EIR source order and
    // therefore the only valid basis for the later LIFO pop.
    cells.sort_by_key(|(publication_order, _)| *publication_order);
    Ok(cells.into_iter().map(|(_, cell)| cell).collect())
}

/// Traces neutral forwarding around a bare EIR-managed argument lease.
/// `Borrow` is a deliberate stop marker: its enclosing EIR ledger owns cleanup.
fn preleased_ref_cell_owner(
    ctx: &FunctionContext<'_>,
    value: ValueId,
) -> Result<Option<(LocalSlotId, u32)>> {
    let Some(value_ref) = ctx.function.value(value) else {
        return Err(CodegenIrError::missing_entry("value", value.as_raw()));
    };
    let ValueDef::Instruction { inst, .. } = value_ref.def else {
        return Ok(None);
    };
    let inst_ref = ctx
        .function
        .instruction(inst)
        .ok_or_else(|| CodegenIrError::missing_entry("instruction", inst.as_raw()))?;
    if inst_ref.op == Op::Borrow {
        return Ok(None);
    }
    if matches!(inst_ref.op, Op::Move | Op::RefCellMixedPayload) {
        return preleased_ref_cell_owner(ctx, expect_operand(inst_ref, 0)?);
    }
    if inst_ref.op != Op::AcquireRefCell {
        return Ok(None);
    }
    let Some(Immediate::LocalSlot(owner_slot)) = inst_ref.immediate else {
        return Err(CodegenIrError::invalid_module(
            "preleased reference-cell argument has no owner slot",
        ));
    };
    Ok(Some((owner_slot, inst.as_raw())))
}

/// Keeps the persistent fallback only when the constructed hierarchy can borrow a property cell.
/// Ordinary constructors use managed call leases, which escaping closures can retain themselves.
pub(super) fn constructor_ref_cell_lifetime(
    ctx: &FunctionContext<'_>,
    class_name: &str,
) -> RefArgCellLifetime {
    let mut current = Some(class_name);
    let mut seen = crate::fast_hash::FastSet::default();
    while let Some(name) = current {
        if !seen.insert(name) { return RefArgCellLifetime::MayOutliveCall; }
        let Some(info) = ctx.module.class_infos.get(name) else {
            return RefArgCellLifetime::MayOutliveCall;
        };
        if info.reference_properties.iter().any(|property| {
            !info.owned_reference_properties.contains(property)
        }) {
            return RefArgCellLifetime::MayOutliveCall;
        }
        // Inspect ancestors as well, since private reference properties may be shadowed.
        current = info.parent.as_deref();
    }
    RefArgCellLifetime::CallOnly
}

/// Reserves writeback slots and publishes a scoped managed owner for each default cell.
/// Each managed owner has an adjacent unwind record, linked in argument order.
pub(super) fn emit_ref_arg_cell_block(
    ctx: &mut FunctionContext<'_>,
    writebacks: &mut [RefArgWriteback],
    temp_cells: &mut [RefArgTempCell],
) -> Result<()> {
    let writeback_bytes = writebacks.len() * 16;
    let total_bytes = writeback_bytes + temp_cells.len() * CALL_ARG_TEMP_CLEANUP_BYTES;
    abi::emit_reserve_temporary_stack(ctx.emitter, total_bytes);
    for (index, writeback) in writebacks.iter_mut().enumerate() {
        ctx.load_value_to_result(writeback.source_value)?;
        emit_box_current_value_as_mixed(ctx.emitter, &writeback.source_ty);
        writeback.cell_offset = index * 16;
        abi::emit_store_to_sp(ctx.emitter, abi::int_result_reg(ctx.emitter), writeback.cell_offset);
    }
    for (index, cell) in temp_cells.iter_mut().enumerate() {
        cell.cell_offset = writeback_bytes + index * CALL_ARG_TEMP_CLEANUP_BYTES;
        if cell.retain_existing {
            let source = cell.source_value.ok_or_else(|| {
                CodegenIrError::invalid_module("retained reference cell without an operand")
            })?;
            ctx.load_value_to_reg(source, abi::int_result_reg(ctx.emitter))?;
            abi::emit_call_label(ctx.emitter, "__rt_incref");
            let cell_reg = abi::symbol_scratch_reg(ctx.emitter);
            abi::emit_push_reg(ctx.emitter, abi::int_result_reg(ctx.emitter));
            abi::emit_pop_reg(ctx.emitter, cell_reg);
            abi::emit_store_to_sp(ctx.emitter, cell_reg, cell.cell_offset);
            let owner_address = abi::tertiary_scratch_reg(ctx.emitter);
            abi::emit_temporary_stack_address(ctx.emitter, owner_address, cell.cell_offset);
            abi::emit_link_call_operand_owner_at_stack(
                ctx.emitter,
                owner_address,
                false,
                cell.cell_offset + 16,
            );
            continue;
        }
        let Some(source) = cell.source_value else {
            // An OMITTED optional by-reference argument: no operand, a null of the cell's type.
            emit_omitted_ref_cell_seed(ctx, &cell.cell_ty)?;
            emit_new_ref_cell_from_pushed_seed(ctx, cell)?;
            continue;
        };
        let source_ty = ctx.load_value_to_result(source)?;
        coerce_ref_cell_store_value(ctx, source, &source_ty, &cell.cell_ty)?;
        // A container stored under another container shape is still the source itself: the
        // hash that named arguments build for an `array<mixed>` by-reference variadic went into
        // the cell unretained, so releasing the cell and then the operand freed it twice.
        let container_kept_as_is = matches!(
            (source_ty.codegen_repr(), cell.cell_ty.codegen_repr()),
            (
                PhpType::Array(_) | PhpType::AssocArray { .. },
                PhpType::Array(_) | PhpType::AssocArray { .. }
            )
        );
        if source_ty.codegen_repr() == cell.cell_ty.codegen_repr() || container_kept_as_is {
            // EIR still owns the default operand. The mutable cell needs its own retain
            // because the callee can replace it before EIR retires that original owner.
            if cell.cell_ty.codegen_repr() == PhpType::Str {
                abi::emit_call_label(ctx.emitter, "__rt_str_persist");
            } else {
                abi::emit_incref_if_refcounted(ctx.emitter, &cell.cell_ty);
            }
        }
        abi::emit_push_result_value(ctx.emitter, &cell.cell_ty);
        abi::emit_load_int_immediate(
            ctx.emitter,
            abi::int_arg_reg_name(ctx.emitter.target, 0),
            crate::codegen_support::runtime::reference_cells::payload_tag(&cell.cell_ty),
        );
        abi::emit_call_label(ctx.emitter, "__rt_reference_cell_new");
        let cell_reg = abi::symbol_scratch_reg(ctx.emitter);
        abi::emit_push_reg(ctx.emitter, abi::int_result_reg(ctx.emitter));
        abi::emit_pop_reg(ctx.emitter, cell_reg);
        store_pushed_value_to_ref_cell(ctx, cell_reg, &cell.cell_ty);
        abi::emit_store_to_sp(ctx.emitter, cell_reg, cell.cell_offset);
        let owner_address = abi::tertiary_scratch_reg(ctx.emitter);
        abi::emit_temporary_stack_address(ctx.emitter, owner_address, cell.cell_offset);
        abi::emit_link_call_operand_owner_at_stack(
            ctx.emitter, owner_address, false, cell.cell_offset + 16,
        );
    }
    Ok(())
}

/// Loads the address that should be passed for a by-reference argument.
pub(super) fn materialize_ref_arg_address(
    ctx: &mut FunctionContext<'_>,
    value: ValueId,
    param_index: usize,
    param_ty: &PhpType,
    arg_temp_bytes: usize,
    writebacks: &[RefArgWriteback],
    temp_cells: &[RefArgTempCell],
    ref_cell_base_offset: usize,
) -> Result<()> {
    if let Some(writeback) = writebacks
        .iter()
        .find(|writeback| writeback.param_index == param_index)
    {
        let cell_offset = arg_temp_bytes + ref_cell_base_offset + writeback.cell_offset;
        abi::emit_temporary_stack_address(
            ctx.emitter,
            abi::int_result_reg(ctx.emitter),
            cell_offset,
        );
        return Ok(());
    }
    if local_ref_arg_source(ctx, value).is_ok() {
        return materialize_local_ref_arg_address(ctx, value);
    }
    if let Some(cell) = temp_cells
        .iter()
        .find(|cell| cell.param_index == param_index)
    {
        let cell_offset = arg_temp_bytes + ref_cell_base_offset + cell.cell_offset;
        abi::emit_load_temporary_stack_slot(
            ctx.emitter,
            abi::int_result_reg(ctx.emitter),
            cell_offset,
        );
        return Ok(());
    }
    if value_is_array_element_address(ctx, value)? {
        ctx.load_value_to_reg(value, abi::int_result_reg(ctx.emitter))?;
        return Ok(());
    }
    materialize_temporary_ref_arg_cell(ctx, value, param_ty)
}

/// Allocates the persistent fallback cell required by borrowed constructor properties.
/// Ordinary calls and non-promoting constructors use managed temporary cells instead.
/// This legacy allocation must remain live until promoted properties retain their own cell
/// owners; replacing it with stack storage or releasing it on return would dangle properties.
pub(super) fn materialize_temporary_ref_arg_cell(
    ctx: &mut FunctionContext<'_>,
    value: ValueId,
    param_ty: &PhpType,
) -> Result<()> {
    let source_ty = ctx.load_value_to_result(value)?;
    let target_ty = param_ty.codegen_repr();
    coerce_ref_cell_store_value(ctx, value, &source_ty, &target_ty)?;
    abi::emit_push_result_value(ctx.emitter, &target_ty);
    abi::emit_load_int_immediate(ctx.emitter, abi::int_result_reg(ctx.emitter), 16);
    abi::emit_call_label(ctx.emitter, "__rt_heap_alloc");
    let cell_reg = abi::symbol_scratch_reg(ctx.emitter);
    abi::emit_push_reg(ctx.emitter, abi::int_result_reg(ctx.emitter));
    abi::emit_pop_reg(ctx.emitter, cell_reg);
    store_pushed_value_to_ref_cell(ctx, cell_reg, &target_ty);
    move_reg_to_int_result(ctx, cell_reg);
    Ok(())
}

/// Stores the pushed argument value into a freshly allocated by-reference cell.
pub(super) fn store_pushed_value_to_ref_cell(ctx: &mut FunctionContext<'_>, cell_reg: &str, val_ty: &PhpType) {
    let temp_reg = if cell_reg == abi::temp_int_reg(ctx.emitter.target) {
        abi::symbol_scratch_reg(ctx.emitter)
    } else {
        abi::temp_int_reg(ctx.emitter.target)
    };
    match val_ty.codegen_repr() {
        PhpType::Str => {
            let (ptr_reg, len_reg) = abi::string_result_regs(ctx.emitter);
            abi::emit_pop_reg_pair(ctx.emitter, ptr_reg, len_reg);
            abi::emit_store_to_address(ctx.emitter, ptr_reg, cell_reg, 0);
            abi::emit_store_to_address(ctx.emitter, len_reg, cell_reg, 8);
        }
        PhpType::TaggedScalar => {
            let tag_reg = crate::codegen::sentinels::tagged_scalar_tag_reg(ctx.emitter);
            abi::emit_pop_reg_pair(ctx.emitter, abi::int_result_reg(ctx.emitter), tag_reg);
            abi::emit_store_to_address(ctx.emitter, abi::int_result_reg(ctx.emitter), cell_reg, 0);
            abi::emit_store_to_address(ctx.emitter, tag_reg, cell_reg, 8);
        }
        PhpType::Float => {
            abi::emit_pop_float_reg(ctx.emitter, abi::float_result_reg(ctx.emitter));
            abi::emit_store_to_address(
                ctx.emitter,
                abi::float_result_reg(ctx.emitter),
                cell_reg,
                0,
            );
        }
        _ => {
            abi::emit_pop_reg(ctx.emitter, temp_reg);
            abi::emit_store_to_address(ctx.emitter, temp_reg, cell_reg, 0);
            abi::emit_store_zero_to_address(ctx.emitter, cell_reg, 8);
        }
    }
}

/// Publishes scalar writebacks, retires default-cell leases in unwind order, and frees staging.
/// Captured cells survive because closure creation retains a separate managed lease.
pub(super) fn emit_ref_arg_writebacks(
    ctx: &mut FunctionContext<'_>,
    call_args: &CallArgMaterialization,
) -> Result<()> {
    for writeback in &call_args.ref_writebacks {
        abi::emit_load_temporary_stack_slot(
            ctx.emitter,
            abi::int_result_reg(ctx.emitter),
            writeback.cell_offset,
        );
        match writeback.source_ty.codegen_repr() {
            PhpType::AssocArray { .. } => {
                emit_hash_ref_writeback_from_mixed_cell(ctx, writeback)?;
                continue;
            }
            container @ (PhpType::Array(_) | PhpType::Object(_)) => {
                emit_owned_payload_ref_writeback_from_mixed_cell(ctx, writeback, &container)?;
                continue;
            }
            _ => {}
        }
        abi::emit_push_reg(ctx.emitter, abi::int_result_reg(ctx.emitter));
        abi::emit_call_label(ctx.emitter, "__rt_mixed_unbox");
        move_reg_to_int_result(ctx, mixed_unbox_low_payload_reg(ctx));
        store_current_scalar_result_to_ref_source(ctx, writeback)?;
        abi::emit_pop_reg(ctx.emitter, abi::int_result_reg(ctx.emitter));
        abi::emit_call_label(ctx.emitter, "__rt_decref_mixed");
    }
    for cell in call_args.ref_temp_cells.iter().rev() {
        abi::emit_unlink_call_operand_owner_at_stack(ctx.emitter, cell.cell_offset + 16);
        abi::emit_load_temporary_stack_slot(
            ctx.emitter,
            abi::int_result_reg(ctx.emitter),
            cell.cell_offset,
        );
        abi::emit_call_label(ctx.emitter, "__rt_reference_cell_release");
    }
    let block_bytes = call_args.ref_writebacks.len() * 16
        + call_args.ref_temp_cells.len() * CALL_ARG_TEMP_CLEANUP_BYTES;
    abi::emit_release_temporary_stack(ctx.emitter, block_bytes);
    for cell in call_args.preleased_ref_cells.iter().rev() {
        abi::emit_pop_call_operand_owner(ctx.emitter);
        super::local_stores::release_local_ref_cell_owner(
            ctx,
            cell.owner_slot,
            &cell.cell_ty,
        )?;
    }
    Ok(())
}

/// Returns the low payload register produced by `__rt_mixed_unbox` on the active target.
pub(super) fn mixed_unbox_low_payload_reg(ctx: &FunctionContext<'_>) -> &'static str {
    match ctx.emitter.target.arch {
        Arch::AArch64 => "x1",
        Arch::X86_64 => "rdi",
    }
}

/// Unboxes a boxed Mixed/Union payload and retains it for an owned concrete heap result.
pub(super) fn emit_unbox_mixed_to_owned_refcounted_result(ctx: &mut FunctionContext<'_>, result_ty: &PhpType) {
    abi::emit_call_label(ctx.emitter, "__rt_mixed_unbox");
    move_reg_to_int_result(ctx, mixed_unbox_low_payload_reg(ctx));
    abi::emit_incref_if_refcounted(ctx.emitter, result_ty);
}

/// Unboxes a guarded Mixed value into an owned concrete heap representation.
///
/// Flow-sensitive checking proves the value has the requested type before this op is emitted;
/// the runtime helper extracts its payload and this result takes its own reference so retaining
/// stores and later cleanup have a balanced ownership ledger.
pub(super) fn lower_mixed_unbox(ctx: &mut FunctionContext<'_>, inst: &Instruction) -> Result<()> {
    let value = expect_operand(inst, 0)?;
    load_value_to_first_int_arg(ctx, value)?;
    let result_ty = inst.result_php_type.codegen_repr();
    // The immediate asks for a runtime type guard on the way out of the box: `clone` of a gradual
    // value (Bool), a nominal class or interface (Data), a callable (I64 10), a spread source
    // (I64 4), or any `iterable` result. Without it a wrong runtime value was handed on unboxed
    // instead of raising php's TypeError.
    match (&result_ty, &inst.immediate) {
        (PhpType::Object(_), Some(Immediate::Bool(true))) => {
            emit_mixed_tag_unbox_guard(ctx, 6, &|given| {
                format!(
                    "clone(): Argument #1 ($object) must be of type object, {} given",
                    given
                )
            });
        }
        (PhpType::Object(_), Some(Immediate::Data(_))) => {
            emit_mixed_nominal_object_unbox_guard(ctx, inst)?;
        }
        // A guarded callable unbox in an eval program must still accept a Closure the interpreter
        // created (an object, converted to a bridge descriptor below), so it checks for the two
        // closure representations rather than for the descriptor tag alone.
        (PhpType::Callable, Some(Immediate::I64(10)))
            if ctx.module.required_runtime_features.eval_bridge =>
        {
            emit_mixed_closure_unbox_guard(ctx);
            return store_if_result(ctx, inst);
        }
        (PhpType::Callable, Some(Immediate::I64(10))) => {
            emit_mixed_tag_unbox_guard(ctx, 10, &|given| {
                format!("Value must be of type callable, {} given", given)
            });
        }
        (PhpType::Array(_), Some(Immediate::I64(4))) => {
            return lower_mixed_spread_source_unbox(ctx, inst, &result_ty);
        }
        (PhpType::Iterable, _) => emit_mixed_iterable_unbox_guard(ctx),
        // A `callable` parameter fed from a `mixed` slot (`function keep($cb) { return new
        // CallbackFilterIterator($it, $cb); }`) may hold a Closure the interpreter created: an
        // object, not a descriptor, which the callee jumped through (Twig's `|filter` arrow
        // functions). The local-load conversion builds a bridge descriptor for it; so does this.
        (PhpType::Callable, None) if ctx.module.required_runtime_features.eval_bridge => {
            super::local_loads::emit_mixed_local_to_owned_callable_descriptor(ctx);
            return store_if_result(ctx, inst);
        }
        _ => {
            emit_unbox_mixed_to_owned_refcounted_result(ctx, &result_ty);
            return store_if_result(ctx, inst);
        }
    }
    move_reg_to_int_result(ctx, mixed_unbox_low_payload_reg(ctx));
    if matches!(
        &result_ty,
        PhpType::Array(element) if element.codegen_repr() == PhpType::Mixed
    ) {
        let result_reg = abi::int_result_reg(ctx.emitter).to_string();
        callable_invoker_args::emit_clone_indexed_array_for_invoker_with_runtime_tag(
            &result_reg,
            ctx.emitter,
        );
    } else {
        abi::emit_incref_if_refcounted(ctx.emitter, &result_ty);
    }
    store_if_result(ctx, inst)
}

/// Publishes a by-reference `mixed` cell back into a caller slot held as a hash.
///
/// The callee may leave a packed list in the cell: `sort()` on an array of mixed values runs
/// the gradual `__elephc_sort_mixed(mixed &$array)`, which assigns `array_values()` order back.
/// Unboxing that payload stored an indexed array where the caller reads a hash, and the next
/// hash walk faulted. `__rt_mixed_to_owned_hash` hands back an owned hash for either array shape.
fn emit_hash_ref_writeback_from_mixed_cell(
    ctx: &mut FunctionContext<'_>,
    writeback: &RefArgWriteback,
) -> Result<()> {
    let result_reg = abi::int_result_reg(ctx.emitter);
    let arg_reg = abi::int_arg_reg_name(ctx.emitter.target, 0);
    abi::emit_push_reg(ctx.emitter, result_reg);
    if arg_reg != result_reg {
        abi::emit_reg_move(ctx.emitter, arg_reg, result_reg);
    }
    abi::emit_call_label(ctx.emitter, "__rt_mixed_to_owned_hash");
    store_owned_result_over_ref_source(ctx, writeback)?;
    abi::emit_pop_reg(ctx.emitter, result_reg);
    abi::emit_call_label(ctx.emitter, "__rt_decref_mixed");
    Ok(())
}

/// Publishes a by-reference `mixed` cell back into a caller slot holding an array or object.
///
/// The cell's payload becomes the slot's new owner, so it is retained before the cell (which
/// still holds its own reference) is released, and the slot's previous owner is retired.
fn emit_owned_payload_ref_writeback_from_mixed_cell(
    ctx: &mut FunctionContext<'_>,
    writeback: &RefArgWriteback,
    payload_ty: &PhpType,
) -> Result<()> {
    let result_reg = abi::int_result_reg(ctx.emitter);
    abi::emit_push_reg(ctx.emitter, result_reg);
    abi::emit_call_label(ctx.emitter, "__rt_mixed_unbox");
    move_reg_to_int_result(ctx, mixed_unbox_low_payload_reg(ctx));
    abi::emit_incref_if_refcounted(ctx.emitter, payload_ty);
    store_owned_result_over_ref_source(ctx, writeback)?;
    abi::emit_pop_reg(ctx.emitter, result_reg);
    abi::emit_call_label(ctx.emitter, "__rt_decref_mixed");
    Ok(())
}

/// Stores the owned value in the result register into the caller slot, retiring its old owner.
fn store_owned_result_over_ref_source(
    ctx: &mut FunctionContext<'_>,
    writeback: &RefArgWriteback,
) -> Result<()> {
    let result_reg = abi::int_result_reg(ctx.emitter);
    let slot_ty = ctx.local_php_type(writeback.source_slot)?.codegen_repr();
    let offset = ctx.local_offset(writeback.source_slot)?;
    abi::emit_push_reg(ctx.emitter, result_reg);
    super::super::frame::emit_owned_local_cleanup(ctx, writeback.source_slot, offset, &slot_ty);
    abi::emit_pop_reg(ctx.emitter, result_reg);
    ctx.store_current_result_to_local(writeback.source_slot)
}

/// Stores an unboxed scalar Mixed payload back through the original by-reference source.
pub(super) fn store_current_scalar_result_to_ref_source(
    ctx: &mut FunctionContext<'_>,
    writeback: &RefArgWriteback,
) -> Result<()> {
    ctx.store_current_result_to_local(writeback.source_slot)
}

/// Loads a local variable's address for a by-reference method-call argument.
pub(super) fn materialize_local_ref_arg_address(ctx: &mut FunctionContext<'_>, value: ValueId) -> Result<()> {
    let source = local_ref_arg_source(ctx, value)?;
    ctx.materialize_local_storage_address(source.slot, abi::int_result_reg(ctx.emitter))
}

/// Returns true when a value already holds a direct pointer to array element reference storage.
pub(super) fn value_is_array_element_address(ctx: &FunctionContext<'_>, value: ValueId) -> Result<bool> {
    Ok(array_element_address_source(ctx, value)?.is_some())
}

/// Resolves an element-slot pointer to the array whose element type describes its pointee storage.
fn array_element_address_source(ctx: &FunctionContext<'_>, value: ValueId) -> Result<Option<ValueId>> {
    let Some(value_ref) = ctx.function.value(value) else {
        return Err(CodegenIrError::missing_entry("value", value.as_raw()));
    };
    let ValueDef::Instruction { inst, .. } = value_ref.def else {
        return Ok(None);
    };
    let inst_ref = ctx
        .function
        .instruction(inst)
        .ok_or_else(|| CodegenIrError::missing_entry("instruction", inst.as_raw()))?;
    if matches!(
        inst_ref.op,
        Op::AcquireRefCell | Op::Borrow | Op::Move | Op::RefCellMixedPayload
    ) {
        return array_element_address_source(ctx, expect_operand(inst_ref, 0)?);
    }
    if !matches!(inst_ref.op, Op::ArrayElemAddr | Op::LoadArrayElemRefCell) {
        return Ok(None);
    }
    Ok(Some(expect_operand(inst_ref, 0)?))
}

/// Returns whether an element-address operand names a managed hash reference cell.
///
/// `ArrayElemAddr` and a concrete indexed-array ref-cell load are borrowed interior addresses.
/// Associative, boxed, and runtime-shaped `Array(Mixed)` sources normalize to hash storage, so
/// their `LoadArrayElemRefCell` result needs a caller lease while the callee can replace its entry
/// owner.
fn value_is_managed_array_element_ref_cell(
    ctx: &FunctionContext<'_>,
    value: ValueId,
) -> Result<bool> {
    let Some(value_ref) = ctx.function.value(value) else {
        return Err(CodegenIrError::missing_entry("value", value.as_raw()));
    };
    let ValueDef::Instruction { inst, .. } = value_ref.def else {
        return Ok(false);
    };
    let inst_ref = ctx
        .function
        .instruction(inst)
        .ok_or_else(|| CodegenIrError::missing_entry("instruction", inst.as_raw()))?;
    if inst_ref.op != Op::LoadArrayElemRefCell {
        return Ok(false);
    }
    let source = expect_operand(inst_ref, 0)?;
    Ok(match ctx.value_php_type(source)?.codegen_repr() {
        PhpType::Array(element) => element.codegen_repr() == PhpType::Mixed,
        PhpType::AssocArray { .. } | PhpType::Mixed | PhpType::Union(_) => true,
        _ => false,
    })
}

/// Describes a local operand used as a by-reference call argument.
struct LocalRefArgSource {
    slot: LocalSlotId,
}

/// Resolves an EIR value back to a local slot and whether it already stores a ref-cell pointer.
fn local_ref_arg_source(ctx: &FunctionContext<'_>, value: ValueId) -> Result<LocalRefArgSource> {
    let Some(value_ref) = ctx.function.value(value) else {
        return Err(CodegenIrError::missing_entry("value", value.as_raw()));
    };
    let ValueDef::Instruction { inst, .. } = value_ref.def else {
        return Err(CodegenIrError::unsupported(
            "by-reference method call argument from non-local value",
        ));
    };
    let inst_ref = ctx
        .function
        .instruction(inst)
        .ok_or_else(|| CodegenIrError::missing_entry("instruction", inst.as_raw()))?;
    match inst_ref.op {
        Op::LoadLocal | Op::LoadRefCell => {}
        _ => {
            return Err(CodegenIrError::unsupported(format!(
                "by-reference method call argument from opcode {}",
                inst_ref.op.name()
            )))
        }
    };
    let Some(Immediate::LocalSlot(slot)) = inst_ref.immediate else {
        return Err(CodegenIrError::invalid_module(
            "by-reference load argument has no local slot",
        ));
    };
    Ok(LocalRefArgSource { slot })
}

/// Resolves an EIR value back to a `load_local` source slot for by-reference calls.
pub(super) fn local_slot_for_loaded_value(ctx: &FunctionContext<'_>, value: ValueId) -> Result<LocalSlotId> {
    local_ref_arg_source(ctx, value).map(|source| source.slot)
}

/// Returns true when a local slot stores a ref-cell pointer instead of a raw value.
pub(super) fn local_slot_stores_ref_cell_pointer(ctx: &FunctionContext<'_>, slot: LocalSlotId) -> bool {
    ctx.local_stores_ref_cell_pointer(slot)
}

/// Moves a scratch integer register into the canonical integer result register.
pub(super) fn move_reg_to_int_result(ctx: &mut FunctionContext<'_>, source_reg: &str) {
    let result_reg = abi::int_result_reg(ctx.emitter);
    if source_reg == result_reg {
        return;
    }
    match ctx.emitter.target.arch {
        Arch::AArch64 => {
            ctx.emitter
                .instruction(&format!("mov {}, {}", result_reg, source_reg)); // move the unboxed receiver pointer into the normal argument staging register
        }
        Arch::X86_64 => {
            ctx.emitter
                .instruction(&format!("mov {}, {}", result_reg, source_reg)); // move the unboxed receiver pointer into the normal argument staging register
        }
    }
}

/// Seeds a discarded cell that stands in for an OMITTED by-reference argument.
///
/// PHP's default for such a parameter is `null` in every case elephc can reach here: a
/// by-reference parameter may not carry a non-null constant default and still be omitted by a
/// caller that means to read the write back. The refusal is kept for representations with no
/// null this runtime can spell, rather than pushing a zero that
/// [`emit_ref_arg_writebacks`] would then hand to a refcount helper.
fn emit_omitted_ref_cell_seed(ctx: &mut FunctionContext<'_>, cell_ty: &PhpType) -> Result<()> {
    match cell_ty.codegen_repr() {
        PhpType::Mixed => {
            objects::emit_boxed_null(ctx);
            Ok(())
        }
        PhpType::TaggedScalar => {
            crate::codegen::sentinels::emit_tagged_scalar_null(ctx.emitter);
            Ok(())
        }
        PhpType::Int | PhpType::Bool | PhpType::False => {
            abi::emit_load_int_immediate(ctx.emitter, abi::int_result_reg(ctx.emitter), 0);
            Ok(())
        }
        other => Err(CodegenIrError::unsupported(format!(
            "omitted by-reference parameter cell for ABI type {other:?}"
        ))),
    }
}

/// Wraps the null seed already in the result registers into a managed reference cell.
///
/// The same allocation and unwind-owner linkage as a cell built from an operand; a null seed
/// carries no reference, so there is nothing to retain first.
fn emit_new_ref_cell_from_pushed_seed(ctx: &mut FunctionContext<'_>, cell: &RefArgTempCell) -> Result<()> {
    abi::emit_push_result_value(ctx.emitter, &cell.cell_ty);
    abi::emit_load_int_immediate(
        ctx.emitter,
        abi::int_arg_reg_name(ctx.emitter.target, 0),
        crate::codegen_support::runtime::reference_cells::payload_tag(&cell.cell_ty),
    );
    abi::emit_call_label(ctx.emitter, "__rt_reference_cell_new");
    let cell_reg = abi::symbol_scratch_reg(ctx.emitter);
    abi::emit_push_reg(ctx.emitter, abi::int_result_reg(ctx.emitter));
    abi::emit_pop_reg(ctx.emitter, cell_reg);
    store_pushed_value_to_ref_cell(ctx, cell_reg, &cell.cell_ty);
    abi::emit_store_to_sp(ctx.emitter, cell_reg, cell.cell_offset);
    let owner_address = abi::tertiary_scratch_reg(ctx.emitter);
    abi::emit_temporary_stack_address(ctx.emitter, owner_address, cell.cell_offset);
    abi::emit_link_call_operand_owner_at_stack(
        ctx.emitter, owner_address, false, cell.cell_offset + 16,
    );
    Ok(())
}

/// Loads the cell standing in for an OMITTED by-reference argument.
///
/// `plan_ref_arg_temp_cells` plans no cells for `MayOutliveCall`, where the callee may keep the
/// reference (a promoted constructor property), so that position gets the persistent fallback
/// cell seeded the way the managed one would have been.
fn materialize_omitted_ref_arg_address(
    ctx: &mut FunctionContext<'_>,
    param_index: usize,
    param_ty: &PhpType,
    arg_temp_bytes: usize,
    temp_cells: &[RefArgTempCell],
    ref_cell_base_offset: usize,
) -> Result<()> {
    let Some(cell) = temp_cells
        .iter()
        .find(|cell| cell.param_index == param_index)
    else {
        let target_ty = param_ty.codegen_repr();
        emit_omitted_ref_cell_seed(ctx, &target_ty)?;
        abi::emit_push_result_value(ctx.emitter, &target_ty);
        abi::emit_load_int_immediate(ctx.emitter, abi::int_result_reg(ctx.emitter), 16);
        abi::emit_call_label(ctx.emitter, "__rt_heap_alloc");
        let cell_reg = abi::symbol_scratch_reg(ctx.emitter);
        abi::emit_push_reg(ctx.emitter, abi::int_result_reg(ctx.emitter));
        abi::emit_pop_reg(ctx.emitter, cell_reg);
        store_pushed_value_to_ref_cell(ctx, cell_reg, &target_ty);
        move_reg_to_int_result(ctx, cell_reg);
        return Ok(());
    };
    abi::emit_load_temporary_stack_slot(
        ctx.emitter,
        abi::int_result_reg(ctx.emitter),
        arg_temp_bytes + ref_cell_base_offset + cell.cell_offset,
    );
    Ok(())
}

/// Resolves the PHP value type stored behind an indexed-array element address.
pub(super) fn array_element_address_value_type(
    ctx: &FunctionContext<'_>,
    value: ValueId,
) -> Result<Option<PhpType>> {
    let Some(value_ref) = ctx.function.value(value) else {
        return Err(CodegenIrError::missing_entry("value", value.as_raw()));
    };
    let ValueDef::Instruction { inst, .. } = value_ref.def else {
        return Ok(None);
    };
    let inst_ref = ctx
        .function
        .instruction(inst)
        .ok_or_else(|| CodegenIrError::missing_entry("instruction", inst.as_raw()))?;
    if inst_ref.op != Op::ArrayElemAddr {
        return Ok(None);
    }
    let array = inst_ref
        .operands
        .first()
        .copied()
        .ok_or_else(|| CodegenIrError::invalid_module("array_elem_addr missing array operand"))?;
    let PhpType::Array(element) = ctx.value_php_type(array)?.codegen_repr() else {
        return Err(CodegenIrError::invalid_module(
            "array_elem_addr source does not use indexed-array storage",
        ));
    };
    Ok(Some(element.codegen_repr()))
}

/// Unboxes the gradual source of a `...$spread` into array storage.
///
/// A list (tag 4) is cloned into Mixed slots for the invoker, as every gradual unbox to
/// `array<mixed>` is. A keyed array (tag 5) is unpackable too -- its string keys name parameters --
/// and it used to meet the list-only guard, which threw "Only arrays and Traversables can be
/// unpacked, array given". Hash storage already tags each entry and every element read of an
/// `array<mixed>` value dispatches on the heap kind, so it is retained as it is.
fn lower_mixed_spread_source_unbox(
    ctx: &mut FunctionContext<'_>,
    inst: &Instruction,
    result_ty: &PhpType,
) -> Result<()> {
    abi::emit_call_label(ctx.emitter, "__rt_mixed_unbox");
    let list_label = ctx.next_label("mixed_spread_list");
    let hash_label = ctx.next_label("mixed_spread_hash");
    let wrong_label = ctx.next_label("mixed_spread_wrong_tag");
    let done_label = ctx.next_label("mixed_spread_done");
    match ctx.emitter.target.arch {
        Arch::AArch64 => {
            ctx.emitter.instruction("cmp x0, #4");                              // runtime tag 4 is a list source
            ctx.emitter.instruction(&format!("b.eq {}", list_label));
            ctx.emitter.instruction("cmp x0, #5");                              // runtime tag 5 is a keyed source
            ctx.emitter.instruction(&format!("b.eq {}", hash_label));
            ctx.emitter.instruction(&format!("b {}", wrong_label));
        }
        Arch::X86_64 => {
            ctx.emitter.instruction("cmp rax, 4");                              // runtime tag 4 is a list source
            ctx.emitter.instruction(&format!("je {}", list_label));
            ctx.emitter.instruction("cmp rax, 5");                              // runtime tag 5 is a keyed source
            ctx.emitter.instruction(&format!("je {}", hash_label));
            ctx.emitter.instruction(&format!("jmp {}", wrong_label));
        }
    }
    builtins::arrays::union_type_guard::emit_mixed_wrong_tag_type_error_dispatch(
        ctx,
        &wrong_label,
        &|given| format!("Only arrays and Traversables can be unpacked, {} given", given),
    );

    ctx.emitter.label(&hash_label);
    move_reg_to_int_result(ctx, mixed_unbox_low_payload_reg(ctx));
    abi::emit_incref_if_refcounted(
        ctx.emitter,
        &PhpType::AssocArray {
            key: Box::new(PhpType::Mixed),
            value: Box::new(PhpType::Mixed),
        },
    );
    abi::emit_jump(ctx.emitter, &done_label);

    ctx.emitter.label(&list_label);
    move_reg_to_int_result(ctx, mixed_unbox_low_payload_reg(ctx));
    if matches!(
        result_ty,
        PhpType::Array(element) if element.codegen_repr() == PhpType::Mixed
    ) {
        let result_reg = abi::int_result_reg(ctx.emitter).to_string();
        callable_invoker_args::emit_clone_indexed_array_for_invoker_with_runtime_tag(
            &result_reg,
            ctx.emitter,
        );
    } else {
        abi::emit_incref_if_refcounted(ctx.emitter, result_ty);
    }

    ctx.emitter.label(&done_label);
    store_if_result(ctx, inst)
}

/// Unboxes a gradual iterable after validating arrays or Traversable object payloads.
pub(super) fn emit_mixed_iterable_unbox_guard(ctx: &mut FunctionContext<'_>) {
    abi::emit_call_label(ctx.emitter, "__rt_mixed_unbox");
    let accepted_label = ctx.next_label("mixed_iterable_accepted");
    let object_label = ctx.next_label("mixed_iterable_object");
    let object_accepted_label = ctx.next_label("mixed_iterable_object_accepted");
    let wrong_label = ctx.next_label("mixed_iterable_wrong_type");
    match ctx.emitter.target.arch {
        Arch::AArch64 => {
            ctx.emitter.instruction("cmp x0, #4");                              // runtime tag 4 identifies an indexed array iterable
            ctx.emitter.instruction(&format!("b.eq {}", accepted_label));       // accept indexed array payloads without object checks
            ctx.emitter.instruction("cmp x0, #5");                              // runtime tag 5 identifies an associative array iterable
            ctx.emitter.instruction(&format!("b.eq {}", accepted_label));       // accept associative array payloads without object checks
            ctx.emitter.instruction("cmp x0, #6");                              // runtime tag 6 identifies an object candidate
            ctx.emitter.instruction(&format!("b.eq {}", object_label));         // validate object candidates against Traversable contracts
            ctx.emitter.instruction(&format!("b {}", wrong_label));             // reject scalar and resource payloads at the boundary
        }
        Arch::X86_64 => {
            ctx.emitter.instruction("cmp rax, 4");                              // runtime tag 4 identifies an indexed array iterable
            ctx.emitter.instruction(&format!("je {}", accepted_label));         // accept indexed array payloads without object checks
            ctx.emitter.instruction("cmp rax, 5");                              // runtime tag 5 identifies an associative array iterable
            ctx.emitter.instruction(&format!("je {}", accepted_label));         // accept associative array payloads without object checks
            ctx.emitter.instruction("cmp rax, 6");                              // runtime tag 6 identifies an object candidate
            ctx.emitter.instruction(&format!("je {}", object_label));           // validate object candidates against Traversable contracts
            ctx.emitter.instruction(&format!("jmp {}", wrong_label));           // reject scalar and resource payloads at the boundary
        }
    }

    ctx.emitter.label(&object_label);
    let interface_ids = builtins::type_predicates::traversable_interface_ids(ctx);
    if interface_ids.is_empty() {
        abi::emit_jump(ctx.emitter, &wrong_label);
    } else {
        match ctx.emitter.target.arch {
            Arch::AArch64 => abi::emit_push_reg(ctx.emitter, "x1"),
            Arch::X86_64 => abi::emit_push_reg(ctx.emitter, "rdi"),
        }
        for interface_id in interface_ids {
            builtins::type_predicates::emit_saved_object_interface_check(
                ctx,
                interface_id,
                &object_accepted_label,
            );
        }
        match ctx.emitter.target.arch {
            Arch::AArch64 => {
                abi::emit_pop_reg(ctx.emitter, "x1");
                abi::emit_load_int_immediate(ctx.emitter, "x0", 6);
            }
            Arch::X86_64 => {
                abi::emit_pop_reg(ctx.emitter, "rdi");
                abi::emit_load_int_immediate(ctx.emitter, "rax", 6);
            }
        }
        abi::emit_jump(ctx.emitter, &wrong_label);
        ctx.emitter.label(&object_accepted_label);
        match ctx.emitter.target.arch {
            Arch::AArch64 => abi::emit_pop_reg(ctx.emitter, "x1"),
            Arch::X86_64 => abi::emit_pop_reg(ctx.emitter, "rdi"),
        }
        abi::emit_jump(ctx.emitter, &accepted_label);
    }

    builtins::arrays::union_type_guard::emit_mixed_wrong_tag_type_error_dispatch(
        ctx,
        &wrong_label,
        &|given| format!("Value must be of type iterable, {} given", given),
    );
    ctx.emitter.label(&accepted_label);
}

/// Unboxes a dynamic value and raises a catchable type error unless its tag matches.
fn emit_mixed_tag_unbox_guard(
    ctx: &mut FunctionContext<'_>,
    expected_tag: i64,
    message_for: &impl Fn(&str) -> String,
) {
    abi::emit_call_label(ctx.emitter, "__rt_mixed_unbox");
    let accepted_label = ctx.next_label("mixed_unbox_tag_accepted");
    let wrong_label = ctx.next_label("mixed_unbox_wrong_tag");
    match ctx.emitter.target.arch {
        Arch::AArch64 => {
            ctx.emitter.instruction(&format!("cmp x0, #{}", expected_tag));     // compare the boxed value's runtime tag with the boundary contract
            ctx.emitter.instruction(&format!("b.eq {}", accepted_label));       // continue only when the runtime representation matches
            ctx.emitter.instruction(&format!("b {}", wrong_label));             // report the dynamic value's concrete runtime type
        }
        Arch::X86_64 => {
            ctx.emitter.instruction(&format!("cmp rax, {}", expected_tag));     // compare the boxed value's runtime tag with the boundary contract
            ctx.emitter.instruction(&format!("je {}", accepted_label));         // continue only when the runtime representation matches
            ctx.emitter.instruction(&format!("jmp {}", wrong_label));           // report the dynamic value's concrete runtime type
        }
    }
    builtins::arrays::union_type_guard::emit_mixed_wrong_tag_type_error_dispatch(
        ctx,
        &wrong_label,
        message_for,
    );
    ctx.emitter.label(&accepted_label);
}

/// Converts a boxed Closure to an owned descriptor, raising a TypeError for any other value.
///
/// Tag 10 is a compiled descriptor and tag 6 an object (an interpreter Closure); both go through
/// the same conversion an unguarded `Mixed` callable load uses. The box stays in the result
/// register for that conversion, so it is saved across the tag probe.
fn emit_mixed_closure_unbox_guard(ctx: &mut FunctionContext<'_>) {
    let accepted_label = ctx.next_label("mixed_closure_unbox_accepted");
    let wrong_label = ctx.next_label("mixed_closure_unbox_wrong");
    let result_reg = abi::int_result_reg(ctx.emitter).to_string();
    abi::emit_push_reg(ctx.emitter, &result_reg);
    abi::emit_call_label(ctx.emitter, "__rt_mixed_unbox");
    match ctx.emitter.target.arch {
        Arch::AArch64 => {
            ctx.emitter.instruction("cmp x0, #10");                             // a compiled callable descriptor is a Closure
            ctx.emitter.instruction(&format!("b.eq {}", accepted_label));       // accept it
            ctx.emitter.instruction("cmp x0, #6");                              // an interpreter Closure arrives as an object
            ctx.emitter.instruction(&format!("b.eq {}", accepted_label));       // accept it for bridge conversion
            ctx.emitter.instruction(&format!("b {}", wrong_label));             // every other value is a TypeError
        }
        Arch::X86_64 => {
            ctx.emitter.instruction("cmp rax, 10");                             // a compiled callable descriptor is a Closure
            ctx.emitter.instruction(&format!("je {}", accepted_label));         // accept it
            ctx.emitter.instruction("cmp rax, 6");                              // an interpreter Closure arrives as an object
            ctx.emitter.instruction(&format!("je {}", accepted_label));         // accept it for bridge conversion
            ctx.emitter.instruction(&format!("jmp {}", wrong_label));           // every other value is a TypeError
        }
    }
    builtins::arrays::union_type_guard::emit_mixed_wrong_tag_type_error_dispatch(
        ctx,
        &wrong_label,
        &|given| format!("Value must be of type Closure, {} given", given),
    );
    ctx.emitter.label(&accepted_label);
    abi::emit_pop_reg(ctx.emitter, &result_reg);
    super::local_loads::emit_mixed_local_to_owned_callable_descriptor(ctx);
}

/// Validates a boxed object against a named class or interface before unboxing it.
fn emit_mixed_nominal_object_unbox_guard(
    ctx: &mut FunctionContext<'_>,
    inst: &Instruction,
) -> Result<()> {
    let expected = objects::class_name_immediate(ctx, inst)?.to_string();
    if expected.is_empty() || php_symbol_key(&expected) == "object" {
        emit_mixed_tag_unbox_guard(ctx, 6, &|given| {
            format!("Value must be of type object, {} given", given)
        });
        return Ok(());
    }
    let Some((target_id, target_kind)) = objects::classify_named_target(ctx, &expected) else {
        // A class-like the compiled program does not declare (its file is left to the runtime
        // autoloader, or it is never loaded at all) has no class id to match against, and no
        // compiled object can be an instance of it. Refusing the whole function sent every class
        // naming one to the interpreter -- `SymfonyRuntime::resolveType()` names
        // `Console\Application`, which a web build never compiles. The boundary still requires an
        // object; the nominal half is left to the object's own class, which only the runtime has.
        emit_mixed_tag_unbox_guard(ctx, 6, &|given| {
            format!("Value must be of type {}, {} given", expected, given)
        });
        return Ok(());
    };
    let source_reg = abi::int_arg_reg_name(ctx.emitter.target, 0);
    abi::emit_push_reg(ctx.emitter, source_reg);
    objects::emit_match_call(ctx, target_id, target_kind, "__rt_mixed_instanceof");
    let accepted_label = ctx.next_label("mixed_unbox_nominal_accepted");
    let wrong_label = ctx.next_label("mixed_unbox_nominal_wrong");
    match ctx.emitter.target.arch {
        Arch::AArch64 => {
            ctx.emitter.instruction(&format!("cbnz x0, {}", accepted_label));   // a true matcher result satisfies the named boundary
            ctx.emitter.instruction(&format!("b {}", wrong_label));             // reject a scalar or unrelated object payload
        }
        Arch::X86_64 => {
            ctx.emitter.instruction("test rax, rax");                           // did the boxed object satisfy the named boundary?
            ctx.emitter.instruction(&format!("jne {}", accepted_label));        // continue when class or interface matching succeeded
            ctx.emitter.instruction(&format!("jmp {}", wrong_label));           // reject a scalar or unrelated object payload
        }
    }
    ctx.emitter.label(&wrong_label);
    // An object of a class the INTERPRETER declared has no compiled class id, so the matcher above
    // says no for it whatever it implements. Ask the bridge before raising, as the gradual and
    // generic boundaries already do: symfony/http-client's interpreted `NativeHttpClient` handed to
    // a compiled `?HttpClientInterface $client` parameter was "Value must be of type
    // HttpClientInterface, object given", and every importmap command died on it. The boxed cell is
    // still on the stack, which is what the accepted path pops.
    if ctx.module.required_runtime_features.eval_bridge {
        let native_fallback_label = ctx.next_label("mixed_unbox_nominal_eval_native");
        let result_reg = abi::int_result_reg(ctx.emitter);
        match ctx.emitter.target.arch {
            Arch::AArch64 => ctx.emitter.instruction(&format!("ldr {}, [sp]", result_reg)), // reload the boxed source cell for the bridge probe
            Arch::X86_64 => ctx.emitter.instruction(&format!("mov {}, QWORD PTR [rsp]", result_reg)), // reload the boxed source cell for the bridge probe
        }
        builtins::emit_eval_object_is_a_named_fallback_from_result(
            ctx,
            &expected,
            &accepted_label,
            &native_fallback_label,
            &native_fallback_label,
        )?;
        ctx.emitter.label(&native_fallback_label);
    }
    abi::emit_pop_reg(ctx.emitter, source_reg);
    abi::emit_call_label(ctx.emitter, "__rt_mixed_unbox");
    let type_error_label = ctx.next_label("mixed_unbox_nominal_type_error");
    builtins::arrays::union_type_guard::emit_mixed_wrong_tag_type_error_dispatch(
        ctx,
        &type_error_label,
        &|given| format!("Value must be of type {}, {} given", expected, given),
    );
    ctx.emitter.label(&accepted_label);
    abi::emit_pop_reg(ctx.emitter, source_reg);
    abi::emit_call_label(ctx.emitter, "__rt_mixed_unbox");
    Ok(())
}
