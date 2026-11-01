//! Purpose:
//! Materializes eval call arguments and converts bridge results.
//!
//! Called from:
//! - The eval lowering facade and sibling eval support modules.
//!
//! Key details:
//! - Scratch offsets, Mixed ownership, and target register ordering are unchanged.

use super::*;

/// Returns the aligned scratch size for an eval-declared function call.
pub(super) fn eval_function_call_stack_bytes(arg_count: usize) -> usize {
    let bytes = EVAL_STACK_BYTES + arg_count * 8;
    (bytes + 15) & !15
}

/// Returns the aligned scratch size for an eval dynamic method-call argument pack.
pub(super) fn eval_method_call_stack_bytes(arg_count: usize) -> usize {
    let bytes = EVAL_STACK_BYTES + 8 + arg_count * 8;
    (bytes + 15) & !15
}

/// Returns the aligned scratch size for an eval dynamic static-method call.
pub(super) fn eval_static_method_call_stack_bytes(arg_count: usize) -> usize {
    let bytes = EVAL_STACK_BYTES + 8 + arg_count * 8;
    (bytes + 15) & !15
}

/// Scratch offsets holding Mixed cells an eval-bridge call boxed and still owns.
///
/// `__elephc_eval_*` borrows the cells it is handed; it never takes ownership of them. Anything
/// `emit_box_current_value_as_mixed` created for one of these calls therefore stays this frame's
/// to release, and every path that leaves the scratch frame owes that release.
pub(super) type EvalBoxedOperands = Vec<usize>;

/// Stores positional operands as boxed Mixed cells for the eval function-call ABI.
pub(super) fn store_eval_function_call_args(
    ctx: &mut FunctionContext<'_>,
    inst: &Instruction,
    args_offset: usize,
) -> Result<EvalBoxedOperands> {
    store_eval_function_call_operands(ctx, &inst.operands, args_offset)
}

/// Stores one operand slice as boxed Mixed cells for eval positional-call ABIs.
pub(super) fn store_eval_function_call_operands(
    ctx: &mut FunctionContext<'_>,
    operands: &[ValueId],
    args_offset: usize,
) -> Result<EvalBoxedOperands> {
    let mut boxed = EvalBoxedOperands::new();
    for (index, operand) in operands.iter().enumerate() {
        let ty = ctx.load_value_to_result(*operand)?.codegen_repr();
        let offset = args_offset + index * 8;
        if !matches!(ty, PhpType::Mixed | PhpType::Union(_)) {
            emit_box_current_value_as_mixed(ctx.emitter, &ty);
            boxed.push(offset);
        }
        let result_reg = abi::int_result_reg(ctx.emitter);
        abi::emit_store_to_sp(ctx.emitter, result_reg, offset);
    }
    Ok(boxed)
}

/// Count-word flag: the pack's last argument is the hidden `func_get_args()` collector, whose
/// elements the bridge splices in as the surplus arguments. Mirrors
/// `elephc_magician::abi::EVAL_ARG_PACK_SURPLUS_COLLECTOR`.
const EVAL_ARG_PACK_SURPLUS_COLLECTOR: i64 = 1 << 62;
/// Count-word flag: the collector opens with the actual PHP argument count. Mirrors
/// `elephc_magician::abi::EVAL_ARG_PACK_COLLECTOR_COUNTS`.
const EVAL_ARG_PACK_COLLECTOR_COUNTS: i64 = 1 << 61;

/// Stores a count-prefixed positional argument pack for the eval method-call ABI.
///
/// `collector_flags` comes from [`eval_method_call_collector_flags`] and rides in the count
/// word's high bits.
pub(super) fn store_eval_method_call_arg_pack(
    ctx: &mut FunctionContext<'_>,
    inst: &Instruction,
    args_offset: usize,
    collector_flags: i64,
) -> Result<EvalBoxedOperands> {
    let arg_count = inst.operands.len().saturating_sub(1);
    let result_reg = abi::int_result_reg(ctx.emitter);
    abi::emit_load_int_immediate(ctx.emitter, result_reg, arg_count as i64 | collector_flags);
    abi::emit_store_to_sp(ctx.emitter, result_reg, args_offset);
    let mut boxed = EvalBoxedOperands::new();
    for (index, operand) in inst.operands.iter().skip(1).enumerate() {
        boxed.extend(store_eval_native_method_argument(
            ctx,
            *operand,
            args_offset + 8 + index * 8,
        )?);
    }
    Ok(boxed)
}

/// Returns the count-word flags telling the bridge that the last argument is a hidden collector.
///
/// The call was planned against the receiver class's COMPILED signature, which carries the
/// generated `func_get_args()` collector as its last parameter, so the operands end with that
/// collector. An interpreted override declares only its source parameters and bound the
/// collector as a real argument: Symfony's compiled `Container::get()` calling `$this->load($file)`
/// on the dumped container gave `load($file, $lazyLoad = true)` the collector array as
/// `$lazyLoad`, and the entity manager's lazy-ghost factory then called `__construct` on an array.
/// The collector holds the surplus arguments (after the actual count, when it carries one), so
/// the bridge splices it rather than dropping it. A source variadic owns its tail differently
/// and is left alone.
pub(super) fn eval_method_call_collector_flags(
    ctx: &FunctionContext<'_>,
    inst: &Instruction,
    object: ValueId,
    method_name: &str,
) -> Result<i64> {
    let PhpType::Object(class_name) = ctx.value_php_type(object)?.codegen_repr() else {
        return Ok(0);
    };
    let class_name = class_name.trim_start_matches('\\');
    let key = php_symbol_key(method_name);
    // An INTERFACE-typed receiver was planned against the interface's signature, which carries
    // the collector the same way: Symfony's `CheckCredentialsListener` calls
    // `PasswordHasherInterface::verify($hash, $plain, $salt)` -- two declared parameters, `$salt`
    // packed as surplus -- on an interpreted `MigratingPasswordHasher`, which then received the
    // collector array as `$salt` and every demo login died on a TypeError.
    let signature = ctx
        .module
        .class_infos
        .get(class_name)
        .and_then(|info| info.methods.get(&key))
        .or_else(|| {
            ctx.module
                .interface_infos
                .get(class_name)
                .and_then(|info| info.methods.get(&key))
        });
    Ok(collector_flags_for_signature(
        signature,
        inst.operands.len().saturating_sub(1),
    ))
}

/// Static-method twin of [`eval_method_call_collector_flags`]: `static::load($file)` in a
/// compiled body was planned against `class_name`'s compiled signature too.
pub(super) fn eval_static_method_call_collector_flags(
    ctx: &FunctionContext<'_>,
    inst: &Instruction,
    class_name: &str,
    method_name: &str,
) -> i64 {
    let signature = ctx
        .module
        .class_infos
        .get(class_name.trim_start_matches('\\'))
        .and_then(|info| info.static_methods.get(&php_symbol_key(method_name)));
    collector_flags_for_signature(signature, inst.operands.len())
}

/// Returns the collector flags when `passed` operands fill `signature` up to its trailing
/// hidden collector.
fn collector_flags_for_signature(
    signature: Option<&crate::types::FunctionSig>,
    passed: usize,
) -> i64 {
    let Some(signature) = signature else {
        return 0;
    };
    let trailing_collector = crate::func_args::sig_collects_surplus_args(signature)
        && !crate::func_args::sig_has_hidden_argc_param(signature)
        && signature
            .params
            .last()
            .is_some_and(|(name, _)| name == crate::func_args::HIDDEN_ARGS_PARAM);
    if !trailing_collector || passed != signature.params.len() {
        return 0;
    }
    let mut flags = EVAL_ARG_PACK_SURPLUS_COLLECTOR;
    if crate::func_args::sig_collects_optional_arg_count(signature) {
        flags |= EVAL_ARG_PACK_COLLECTOR_COUNTS;
    }
    flags
}

/// Stores all positional operands as a count-prefixed static-method argument pack.
///
/// `collector_flags` comes from [`eval_static_method_call_collector_flags`].
pub(super) fn store_eval_static_method_call_arg_pack(
    ctx: &mut FunctionContext<'_>,
    inst: &Instruction,
    args_offset: usize,
    collector_flags: i64,
) -> Result<EvalBoxedOperands> {
    let result_reg = abi::int_result_reg(ctx.emitter);
    abi::emit_load_int_immediate(
        ctx.emitter,
        result_reg,
        inst.operands.len() as i64 | collector_flags,
    );
    abi::emit_store_to_sp(ctx.emitter, result_reg, args_offset);
    let mut boxed = EvalBoxedOperands::new();
    for (index, operand) in inst.operands.iter().enumerate() {
        boxed.extend(store_eval_native_method_argument(
            ctx,
            *operand,
            args_offset + 8 + index * 8,
        )?);
    }
    Ok(boxed)
}

/// Stores one dynamic method argument, retaining writable local storage for PHP references.
///
/// The reference-marker form boxes a frame address under an invoker marker tag rather than a
/// PHP value, so it is deliberately not reported as a releasable operand: `__rt_decref_mixed`
/// would deep-free a payload that never was a heap value.
fn store_eval_native_method_argument(
    ctx: &mut FunctionContext<'_>,
    value: ValueId,
    offset: usize,
) -> Result<Option<usize>> {
    let local_slot = crate::codegen::lower_inst::reference_arguments::local_slot_for_loaded_value(ctx, value);
    // The shared owner-call helper only ever sees its own parameters: a reference marker would
    // point at the helper's copy, so the site proved the argument is passed by value instead.
    if super::outlined_owner_call::inside_owner_call_helper(ctx) {
        return store_eval_native_method_argument_by_value(ctx, value, offset);
    }
    if let Ok(slot) = local_slot {
        if !eval_method_argument_has_stable_ref_place(ctx, slot) {
            return store_eval_native_method_argument_by_value(ctx, value, offset);
        }
        let source_ty = ctx.local_php_type(slot)?.codegen_repr();
        if matches!(source_ty, PhpType::TaggedScalar) {
            return store_eval_native_method_argument_by_value(ctx, value, offset);
        }
        let ref_cell_reg = abi::secondary_scratch_reg(ctx.emitter);
        let marker_tag_reg = abi::tertiary_scratch_reg(ctx.emitter);
        let source_tag_reg = abi::symbol_scratch_reg(ctx.emitter);
        crate::codegen::lower_inst::globals_constants::emit_marker_source_tag(
            ctx,
            slot,
            &source_ty,
            source_tag_reg,
        )?;
        ctx.materialize_local_storage_address(slot, ref_cell_reg)?;
        abi::emit_load_int_immediate(
            ctx.emitter,
            marker_tag_reg,
            crate::codegen::callable_invoker_args::INVOKER_ARG_REF_CELL_TAG,
        );
        ctx.emitter.comment("eval_method_ref_arg");
        crate::codegen::emit_box_runtime_payload_as_mixed(
            ctx.emitter,
            marker_tag_reg,
            ref_cell_reg,
            source_tag_reg,
        );
    } else {
        return store_eval_native_method_argument_by_value(ctx, value, offset);
    }
    let result_reg = abi::int_result_reg(ctx.emitter);
    abi::emit_store_to_sp(ctx.emitter, result_reg, offset);
    Ok(None)
}

/// Returns whether a local remains an addressable PHP reference place throughout an eval call.
///
/// One-shot EIR merge temporaries are moved out and cleared before their `LoadLocal` is consumed.
/// Passing their frame address to the eval bridge would therefore expose an empty cell instead of
/// the loaded value. Ordinary PHP and function-static locals retain stable storage and may carry
/// a reference marker when the runtime-resolved method requires one.
pub(super) fn eval_method_argument_has_stable_ref_place(ctx: &FunctionContext<'_>, slot: LocalSlotId) -> bool {
    matches!(
        ctx.local_kind(slot),
        Ok(LocalKind::PhpLocal | LocalKind::StaticLocal)
    )
}

/// Stores a dynamic method argument by value when no stable local reference slot is available.
fn store_eval_native_method_argument_by_value(
    ctx: &mut FunctionContext<'_>,
    value: ValueId,
    offset: usize,
) -> Result<Option<usize>> {
    let ty = ctx.load_value_to_result(value)?.codegen_repr();
    let boxed = if matches!(ty, PhpType::Mixed | PhpType::Union(_)) {
        None
    } else {
        emit_box_current_value_as_mixed(ctx.emitter, &ty);
        Some(offset)
    };
    let result_reg = abi::int_result_reg(ctx.emitter);
    abi::emit_store_to_sp(ctx.emitter, result_reg, offset);
    Ok(boxed)
}

/// Stores an object operand as a boxed Mixed cell in eval scratch storage.
pub(super) fn store_eval_object_operand(
    ctx: &mut FunctionContext<'_>,
    object: ValueId,
) -> Result<Option<usize>> {
    store_eval_mixed_operand_at(ctx, object, EVAL_TEMP_CELL_OFFSET)
}

/// Stores one operand as a boxed Mixed cell; native representations create an owner to retire.
pub(super) fn store_eval_mixed_operand_at(
    ctx: &mut FunctionContext<'_>,
    value: ValueId,
    offset: usize,
) -> Result<Option<usize>> {
    let value_ty = ctx.load_value_to_result(value)?.codegen_repr();
    let boxed = if matches!(value_ty, PhpType::Mixed | PhpType::Union(_)) {
        None
    } else {
        emit_box_current_value_as_mixed(ctx.emitter, &value_ty);
        Some(offset)
    };
    let result_reg = abi::int_result_reg(ctx.emitter);
    abi::emit_store_to_sp(ctx.emitter, result_reg, offset);
    Ok(boxed)
}

/// Releases the Mixed cells an eval-bridge call boxed from statically typed operands.
///
/// Use this on a path that leaves the scratch frame without a published result cell: a probe
/// that missed and falls back to native dispatch strands exactly as many references as a
/// serviced call would, and a stranded receiver reference is a destructor that never runs.
///
/// Must be emitted while the scratch frame is still reserved, and while no value the caller
/// still needs is live in the ABI result register.
pub(super) fn emit_release_eval_boxed_operands(ctx: &mut FunctionContext<'_>, boxed: &[usize]) {
    for offset in boxed {
        abi::emit_load_temporary_stack_slot(ctx.emitter, abi::int_result_reg(ctx.emitter), *offset);
        abi::emit_call_label(ctx.emitter, "__rt_decref_mixed"); // release the Mixed cell this call boxed for the eval bridge
    }
}

/// Releases boxed operand cells on a probe ABI that answers with a plain integer.
///
/// These bridges hand their answer back in the register `emit_release_eval_boxed_operands` reuses
/// and never fill the scratch result slot, so the answer is parked in that unused slot across the
/// decref and taken back afterwards.
pub(super) fn emit_release_eval_boxed_operands_keeping_int_answer(
    ctx: &mut FunctionContext<'_>,
    boxed: &[usize],
) {
    if boxed.is_empty() {
        return;
    }
    abi::emit_store_to_sp(
        ctx.emitter,
        abi::int_result_reg(ctx.emitter),
        EVAL_RESULT_VALUE_CELL_OFFSET,
    );
    emit_release_eval_boxed_operands(ctx, boxed);
    abi::emit_load_temporary_stack_slot(
        ctx.emitter,
        abi::int_result_reg(ctx.emitter),
        EVAL_RESULT_VALUE_CELL_OFFSET,
    );
}

/// Releases boxed operand cells on a path where the bridge published a result cell.
///
/// This used to SKIP the release when the published result cell was the very cell an operand was
/// boxed into (`function f($a) { return $a; }`), on the reasoning that the operand's reference had
/// moved to the result. That reasoning held only while the interpreter returned a BORROWED cell in
/// that case. It no longer does: `statements::dispatch` now retains a returned value whose
/// activation scope does not own one -- a by-value parameter is bound `Borrowed` on purpose, and
/// without the retain a discarded `f($x);` released the CALLER's `$x`.
///
/// So the interpreter now guarantees every result carries a reference of its own, and this frame's
/// box is always still this frame's to drop. Skipping it left the object with one reference too
/// many: `$back = echo_back($local);` never ran `$local`'s destructor
/// (`codegen::destructors::test_eval_call_returning_its_argument_keeps_the_returned_cell`).
///
/// The two compensations are the same pair the `$this` exclusion in `statements::dispatch` already
/// avoids -- retaining on the return side AND holding on the call side double-counts. Exactly one
/// side compensates, and it is the interpreter.
pub(super) fn emit_release_eval_boxed_operands_keeping_result(
    ctx: &mut FunctionContext<'_>,
    boxed: &[usize],
) {
    emit_release_eval_boxed_operands(ctx, boxed);
}

/// Retires metadata-query adapter boxes while preserving the C status or scalar predicate result.
/// Existing Mixed operands remain borrowed; native payloads retain their original EIR owner.
pub(super) fn retire_eval_metadata_operand_boxes(
    ctx: &mut FunctionContext<'_>,
    operands: &[(ValueId, usize)],
) -> Result<()> {
    let result = abi::int_result_reg(ctx.emitter);
    for &(value, offset) in operands {
        if matches!(ctx.value_php_type(value)?.codegen_repr(), PhpType::Mixed | PhpType::Union(_)) {
            continue;
        }
        ctx.emitter.comment("retire temporary eval metadata operand box");
        abi::emit_push_reg(ctx.emitter, result);
        abi::emit_load_temporary_stack_slot(ctx.emitter, result, offset + 16);
        abi::emit_call_label(ctx.emitter, "__rt_decref_mixed");
        abi::emit_pop_reg(ctx.emitter, result);
    }
    Ok(())
}

/// Probes whether eval has a late-static called-class override for an AOT frame.
pub(super) fn emit_eval_native_frame_override_probe(
    ctx: &mut FunctionContext<'_>,
    frame_class: &str,
    no_override_label: &str,
) {
    let (frame_label, frame_len) = ctx.data.add_string(frame_class.as_bytes());
    abi::emit_symbol_address(
        ctx.emitter,
        abi::int_arg_reg_name(ctx.emitter.target, 0),
        &frame_label,
    );
    abi::emit_load_int_immediate(
        ctx.emitter,
        abi::int_arg_reg_name(ctx.emitter.target, 1),
        frame_len as i64,
    );
    let out_ptr_arg = abi::int_arg_reg_name(ctx.emitter.target, 2);
    abi::emit_temporary_stack_address(ctx.emitter, out_ptr_arg, EVAL_CALLED_CLASS_PTR_OFFSET);
    let out_len_arg = abi::int_arg_reg_name(ctx.emitter.target, 3);
    abi::emit_temporary_stack_address(ctx.emitter, out_len_arg, EVAL_CALLED_CLASS_LEN_OFFSET);
    let symbol = ctx
        .emitter
        .target
        .extern_symbol("__elephc_eval_native_frame_called_class_override");
    abi::emit_call_label(ctx.emitter, &symbol);
    abi::emit_branch_if_int_result_zero(ctx.emitter, no_override_label);
}

/// Converts an eval Mixed result cell to the concrete EIR type expected here.
pub(super) fn emit_eval_result_as_type(ctx: &mut FunctionContext<'_>, result_ty: &PhpType) -> Result<()> {
    match result_ty.codegen_repr() {
        PhpType::Mixed | PhpType::Union(_) => Ok(()),
        PhpType::Str => {
            abi::emit_call_label(ctx.emitter, "__rt_mixed_cast_string");
            Ok(())
        }
        PhpType::Float => {
            abi::emit_call_label(ctx.emitter, "__rt_mixed_cast_float");
            Ok(())
        }
        PhpType::Int => {
            abi::emit_call_label(ctx.emitter, "__rt_mixed_cast_int");
            Ok(())
        }
        PhpType::Bool | PhpType::False => {
            abi::emit_call_label(ctx.emitter, "__rt_mixed_cast_bool");
            Ok(())
        }
        PhpType::TaggedScalar => {
            emit_eval_mixed_result_as_tagged_scalar(ctx);
            Ok(())
        }
        PhpType::Void | PhpType::Never => {
            abi::emit_load_int_immediate(
                ctx.emitter,
                abi::int_result_reg(ctx.emitter),
                0x7fff_ffff_ffff_fffe,
            );
            Ok(())
        }
        PhpType::Array(element) if matches!(element.codegen_repr(), PhpType::Object(_)) => {
            emit_eval_mixed_array_as_owned_object_array(ctx);
            Ok(())
        }
        PhpType::AssocArray { value, .. }
            if matches!(value.codegen_repr(), PhpType::Object(_)) =>
        {
            super::associative_results::emit_eval_owned_object_hash(ctx);
            Ok(())
        }
        PhpType::Array(_) | PhpType::AssocArray { .. }
        | PhpType::Iterable
        | PhpType::Object(_)
        | PhpType::Buffer(_)
        | PhpType::Callable
        | PhpType::Packed(_)
        | PhpType::Pointer(_)
        | PhpType::Narrow(_)
        | PhpType::Resource(_) => {
            emit_eval_unbox_mixed_to_owned_result(ctx, &result_ty.codegen_repr());
            Ok(())
        }
    }
}

/// Converts an eval call's OWNED Mixed result to its static type, releasing the box whenever the
/// conversion leaves something other than the box itself.
///
/// The bridge hands the call's result back as a fresh Mixed cell. A Mixed result type keeps it.
/// Every other arm only READS it: the scalar casts return detached values, and the unboxing and
/// cloning arms take their own reference to the payload. The cell was never released, so every
/// eval-dispatched call typed `string` leaked the box and its string, and one typed with a class
/// kept the object alive through the leaked box -- `ReflectionClass::getMethods()` on a
/// dynamically named class leaked every method object and all it holds, which exhausted Symfony's
/// heap in the opcache preloader.
pub(super) fn emit_eval_owned_result_as_type(
    ctx: &mut FunctionContext<'_>,
    result_ty: &PhpType,
) -> Result<()> {
    let repr = result_ty.codegen_repr();
    if !matches!(
        repr,
        PhpType::Str
            | PhpType::Int
            | PhpType::Float
            | PhpType::Bool
            | PhpType::False
            | PhpType::Array(_)
            | PhpType::AssocArray { .. }
            | PhpType::Iterable
            | PhpType::Object(_)
            | PhpType::Buffer(_)
            | PhpType::Callable
            | PhpType::Packed(_)
            | PhpType::Pointer(_)
            | PhpType::Resource(_)
    ) {
        return emit_eval_result_as_type(ctx, result_ty);
    }
    let result_reg = abi::int_result_reg(ctx.emitter);
    abi::emit_push_reg(ctx.emitter, result_reg);
    emit_eval_result_as_type(ctx, result_ty)?;
    abi::emit_push_result_value(ctx.emitter, &repr);
    abi::emit_load_temporary_stack_slot(ctx.emitter, result_reg, 16);
    abi::emit_decref_if_refcounted(ctx.emitter, &PhpType::Mixed);
    match repr {
        PhpType::Str => {
            let (ptr_reg, len_reg) = abi::string_result_regs(ctx.emitter);
            abi::emit_pop_reg_pair(ctx.emitter, ptr_reg, len_reg);
        }
        PhpType::Float => abi::emit_pop_float_reg(ctx.emitter, abi::float_result_reg(ctx.emitter)),
        _ => abi::emit_pop_reg(ctx.emitter, result_reg),
    }
    abi::emit_release_temporary_stack(ctx.emitter, 16);
    Ok(())
}

/// Clones an eval `array<mixed>` result and rewrites its boxed object slots to raw object pointers.
fn emit_eval_mixed_array_as_owned_object_array(ctx: &mut FunctionContext<'_>) {
    match ctx.emitter.target.arch {
        Arch::AArch64 => emit_eval_mixed_array_as_owned_object_array_aarch64(ctx),
        Arch::X86_64 => emit_eval_mixed_array_as_owned_object_array_x86_64(ctx),
    }
}

/// Emits the ARM64 eval Mixed-array to object-array conversion.
fn emit_eval_mixed_array_as_owned_object_array_aarch64(ctx: &mut FunctionContext<'_>) {
    abi::emit_call_label(ctx.emitter, "__rt_mixed_unbox");
    ctx.emitter.instruction("mov x0, x1");                                      // pass the borrowed eval array payload to the clone helper
    abi::emit_call_label(ctx.emitter, "__rt_array_clone_shallow");
    ctx.emitter.instruction("sub sp, sp, #32");                                 // reserve array, index, old-cell, and scratch slots
    ctx.emitter.instruction("str x0, [sp]");                                    // retain the owned cloned array across slot helper calls
    ctx.emitter.instruction("str xzr, [sp, #8]");                               // start conversion at slot zero
    let loop_label = ctx.next_label("eval_object_array_loop");
    let done_label = ctx.next_label("eval_object_array_done");
    ctx.emitter.instruction("ldr x9, [sp]");                                    // load the cloned indexed array before inspecting its physical slots
    ctx.emitter.instruction("ldr x10, [x9, #-8]");                              // load the cloned array's packed value-type metadata
    ctx.emitter.instruction("lsr x10, x10, #8");                                // move the runtime value-type tag into the low bits
    ctx.emitter.instruction("and x10, x10, #0x7f");                             // discard the persistent array flag before comparing the slot layout
    ctx.emitter.instruction("cmp x10, #6");                                     // do the cloned slots already contain raw object pointers?
    ctx.emitter.instruction(&format!("b.eq {}", done_label));                   // preserve object slots instead of unboxing object fields as Mixed cells
    ctx.emitter.label(&loop_label);
    ctx.emitter.instruction("ldr x9, [sp]");                                    // reload the cloned indexed array
    ctx.emitter.instruction("ldr x10, [x9]");                                   // load the logical element count
    ctx.emitter.instruction("ldr x11, [sp, #8]");                               // load the current slot index
    ctx.emitter.instruction("cmp x11, x10");                                    // has every Mixed slot been converted?
    ctx.emitter.instruction(&format!("b.hs {}", done_label));                   // finish after the final live slot
    ctx.emitter.instruction("add x12, x9, #24");                                // address the indexed-array payload
    ctx.emitter.instruction("ldr x0, [x12, x11, lsl #3]");                      // load the boxed Mixed object cell
    ctx.emitter.instruction("str x0, [sp, #16]");                               // preserve the old cell for balanced release
    abi::emit_call_label(ctx.emitter, "__rt_mixed_unbox");
    ctx.emitter.instruction("mov x0, x1");                                      // move the raw object payload into the retain ABI
    abi::emit_call_label(ctx.emitter, "__rt_incref");
    ctx.emitter.instruction("ldr x9, [sp]");                                    // reload the cloned array after the retain call
    ctx.emitter.instruction("ldr x11, [sp, #8]");                               // reload the current slot index
    ctx.emitter.instruction("add x12, x9, #24");                                // recover the destination payload base
    ctx.emitter.instruction("str x0, [x12, x11, lsl #3]");                      // replace the Mixed cell with the retained object pointer
    ctx.emitter.instruction("ldr x0, [sp, #16]");                               // release the cloned array's old Mixed-cell owner
    abi::emit_call_label(ctx.emitter, "__rt_decref_mixed");
    ctx.emitter.instruction("ldr x11, [sp, #8]");                               // advance to the next slot
    ctx.emitter.instruction("add x11, x11, #1");
    ctx.emitter.instruction("str x11, [sp, #8]");
    ctx.emitter.instruction(&format!("b {}", loop_label));                      // continue converting live object slots
    ctx.emitter.label(&done_label);
    ctx.emitter.instruction("ldr x0, [sp]");                                    // return the normalized owned array
    ctx.emitter.instruction("ldr x9, [x0, #-8]");                               // load packed array metadata
    ctx.emitter.instruction("mov x10, #0x7f00");                                // mask the previous runtime value-type byte
    ctx.emitter.instruction("bic x9, x9, x10");
    ctx.emitter.instruction("orr x9, x9, #0x600");                              // stamp runtime object tag 6 into the value-type byte
    ctx.emitter.instruction("str x9, [x0, #-8]");                               // publish the concrete object-slot representation
    ctx.emitter.instruction("add sp, sp, #32");                                 // release conversion scratch storage
}

/// Emits the x86_64 eval Mixed-array to object-array conversion.
fn emit_eval_mixed_array_as_owned_object_array_x86_64(ctx: &mut FunctionContext<'_>) {
    abi::emit_call_label(ctx.emitter, "__rt_mixed_unbox");
    ctx.emitter.instruction("mov rax, rdi");                                    // move the borrowed eval array into the standard result register
    ctx.emitter.instruction("mov rdi, rax");                                    // pass the array payload to the clone helper
    abi::emit_call_label(ctx.emitter, "__rt_array_clone_shallow");
    ctx.emitter.instruction("sub rsp, 32");                                     // reserve array, index, old-cell, and scratch slots
    ctx.emitter.instruction("mov QWORD PTR [rsp], rax");                        // retain the owned cloned array across helper calls
    ctx.emitter.instruction("mov QWORD PTR [rsp + 8], 0");                      // start conversion at slot zero
    let loop_label = ctx.next_label("eval_object_array_loop");
    let done_label = ctx.next_label("eval_object_array_done");
    ctx.emitter.instruction("mov r9, QWORD PTR [rsp]");                         // load the cloned indexed array before inspecting its physical slots
    ctx.emitter.instruction("mov r10, QWORD PTR [r9 - 8]");                     // load the cloned array's packed value-type metadata
    ctx.emitter.instruction("shr r10, 8");                                      // move the runtime value-type tag into the low bits
    ctx.emitter.instruction("and r10, 0x7f");                                   // discard the persistent array flag before comparing the slot layout
    ctx.emitter.instruction("cmp r10, 6");                                      // do the cloned slots already contain raw object pointers?
    ctx.emitter.instruction(&format!("je {}", done_label));                     // preserve object slots instead of unboxing object fields as Mixed cells
    ctx.emitter.label(&loop_label);
    ctx.emitter.instruction("mov r9, QWORD PTR [rsp]");                         // reload the cloned indexed array
    ctx.emitter.instruction("mov r10, QWORD PTR [r9]");                         // load the logical element count
    ctx.emitter.instruction("mov r11, QWORD PTR [rsp + 8]");                    // load the current slot index
    ctx.emitter.instruction("cmp r11, r10");                                    // has every Mixed slot been converted?
    ctx.emitter.instruction(&format!("jae {}", done_label));                    // finish after the final live slot
    ctx.emitter.instruction("mov rax, QWORD PTR [r9 + r11*8 + 24]");            // load the boxed Mixed object cell
    ctx.emitter.instruction("mov QWORD PTR [rsp + 16], rax");                   // preserve the old cell for balanced release
    abi::emit_call_label(ctx.emitter, "__rt_mixed_unbox");
    ctx.emitter.instruction("mov rax, rdi");                                    // move the raw object payload into the retain ABI
    abi::emit_call_label(ctx.emitter, "__rt_incref");
    ctx.emitter.instruction("mov r9, QWORD PTR [rsp]");                         // reload the cloned array after the retain call
    ctx.emitter.instruction("mov r11, QWORD PTR [rsp + 8]");                    // reload the current slot index
    ctx.emitter.instruction("mov QWORD PTR [r9 + r11*8 + 24], rax");            // replace the Mixed cell with the retained object pointer
    ctx.emitter.instruction("mov rax, QWORD PTR [rsp + 16]");                   // release the cloned array's old Mixed-cell owner
    abi::emit_call_label(ctx.emitter, "__rt_decref_mixed");
    ctx.emitter.instruction("inc QWORD PTR [rsp + 8]");                         // advance to the next slot
    ctx.emitter.instruction(&format!("jmp {}", loop_label));                    // continue converting live object slots
    ctx.emitter.label(&done_label);
    ctx.emitter.instruction("mov rax, QWORD PTR [rsp]");                        // return the normalized owned array
    ctx.emitter.instruction("mov r9, QWORD PTR [rax - 8]");                     // load packed array metadata
    ctx.emitter.instruction("and r9, -32513");                                  // clear the previous runtime value-type byte
    ctx.emitter.instruction("or r9, 1536");                                     // stamp runtime object tag 6 into the value-type byte
    ctx.emitter.instruction("mov QWORD PTR [rax - 8], r9");                     // publish the concrete object-slot representation
    ctx.emitter.instruction("add rsp, 32");                                     // release conversion scratch storage
}

/// Reorders an eval Mixed result cell into inline tagged-scalar result registers.
pub(super) fn emit_eval_mixed_result_as_tagged_scalar(ctx: &mut FunctionContext<'_>) {
    abi::emit_call_label(ctx.emitter, "__rt_mixed_unbox");
    match ctx.emitter.target.arch {
        Arch::AArch64 => {
            ctx.emitter.instruction("mov x9, x0");                              // preserve the unboxed eval result tag before moving the payload
            ctx.emitter.instruction("mov x0, x1");                              // place the unboxed eval payload into the tagged-scalar payload register
            ctx.emitter.instruction("mov x1, x9");                              // place the unboxed eval tag into the tagged-scalar tag register
        }
        Arch::X86_64 => {
            ctx.emitter.instruction("mov r10, rax");                            // preserve the unboxed eval result tag before moving the payload
            ctx.emitter.instruction("mov rax, rdi");                            // place the unboxed eval payload into the tagged-scalar payload register
            ctx.emitter.instruction("mov rdx, r10");                            // place the unboxed eval tag into the tagged-scalar tag register
        }
    }
}

/// Unboxes an eval Mixed result cell and retains concrete refcounted payloads.
pub(super) fn emit_eval_unbox_mixed_to_owned_result(ctx: &mut FunctionContext<'_>, result_ty: &PhpType) {
    abi::emit_call_label(ctx.emitter, "__rt_mixed_unbox");
    emit_eval_move_unboxed_low_payload_to_result(ctx);
    abi::emit_incref_if_refcounted(ctx.emitter, result_ty);
}

/// Moves the low payload from `__rt_mixed_unbox` into the integer result register.
pub(super) fn emit_eval_move_unboxed_low_payload_to_result(ctx: &mut FunctionContext<'_>) {
    match ctx.emitter.target.arch {
        Arch::AArch64 => {
            ctx.emitter.instruction("mov x0, x1");                              // return the unboxed eval low payload as the concrete result
        }
        Arch::X86_64 => {
            ctx.emitter.instruction("mov rax, rdi");                            // return the unboxed eval low payload as the concrete result
        }
    }
}

/// Boxes a raw eval predicate result when the enclosing IR value expects Mixed storage.
pub(super) fn box_eval_bool_result_if_mixed(ctx: &mut FunctionContext<'_>, inst: &Instruction) {
    if inst.result.is_some() && inst.result_php_type.codegen_repr() == PhpType::Mixed {
        emit_box_current_value_as_mixed(ctx.emitter, &PhpType::Bool);
    }
}

/// Returns the eval ABI discriminator for a class-name builtin.
pub(super) fn eval_class_lookup_kind(name: &str) -> Result<i64> {
    match name {
        "get_class" => Ok(EVAL_CLASS_LOOKUP_GET_CLASS),
        "get_parent_class" => Ok(EVAL_CLASS_LOOKUP_GET_PARENT_CLASS),
        _ => Err(CodegenIrError::unsupported(format!(
            "eval object class-name lookup {}",
            name
        ))),
    }
}

/// Returns the eval ABI discriminator for member-existence builtins.
pub(super) fn eval_member_lookup_kind(name: &str) -> Result<i64> {
    match name {
        "method_exists" => Ok(EVAL_MEMBER_LOOKUP_METHOD_EXISTS),
        "property_exists" => Ok(EVAL_MEMBER_LOOKUP_PROPERTY_EXISTS),
        _ => Err(CodegenIrError::unsupported(format!(
            "eval member-exists lookup {}",
            name
        ))),
    }
}

/// Returns the eval ABI discriminator for class/interface/trait relation builtins.
pub(super) fn eval_class_relation_kind(name: &str) -> Result<i64> {
    match name {
        "class_implements" => Ok(EVAL_CLASS_RELATION_IMPLEMENTS),
        "class_parents" => Ok(EVAL_CLASS_RELATION_PARENTS),
        "class_uses" => Ok(EVAL_CLASS_RELATION_USES),
        _ => Err(CodegenIrError::unsupported(format!(
            "eval class-relation lookup {}",
            name
        ))),
    }
}

/// Branches when `__rt_mixed_unbox` did not expose an object payload.
pub(super) fn emit_branch_if_eval_unboxed_not_object(ctx: &mut FunctionContext<'_>, label: &str) {
    match ctx.emitter.target.arch {
        Arch::AArch64 => {
            ctx.emitter.instruction("cmp x0, #6");                              // runtime tag 6 means the Mixed value contains an object
            ctx.emitter.instruction(&format!("b.ne {}", label));                // non-object values use the native false/empty fallback
        }
        Arch::X86_64 => {
            ctx.emitter.instruction("cmp rax, 6");                              // runtime tag 6 means the Mixed value contains an object
            ctx.emitter.instruction(&format!("jne {}", label));                 // non-object values use the native false/empty fallback
        }
    }
}

/// Branches to the invalid-target fatal unless an eval dynamic target is string or object.
pub(super) fn emit_validate_eval_dynamic_instanceof_target(ctx: &mut FunctionContext<'_>, label: &str) {
    let ok_label = ctx.next_label("eval_object_is_a_dynamic_target_ok");
    match ctx.emitter.target.arch {
        Arch::AArch64 => {
            ctx.emitter.instruction("cmp x0, #1");                              // runtime tag 1 means the dynamic target is a string
            ctx.emitter.instruction(&format!("b.eq {}", ok_label));             // accept string targets for dynamic instanceof
            ctx.emitter.instruction("cmp x0, #6");                              // runtime tag 6 means the dynamic target is an object
            ctx.emitter.instruction(&format!("b.eq {}", ok_label));             // accept object targets for dynamic instanceof
            ctx.emitter.instruction(&format!("b {}", label));                   // reject every other dynamic instanceof target kind
        }
        Arch::X86_64 => {
            ctx.emitter.instruction("cmp rax, 1");                              // runtime tag 1 means the dynamic target is a string
            ctx.emitter.instruction(&format!("je {}", ok_label));               // accept string targets for dynamic instanceof
            ctx.emitter.instruction("cmp rax, 6");                              // runtime tag 6 means the dynamic target is an object
            ctx.emitter.instruction(&format!("je {}", ok_label));               // accept object targets for dynamic instanceof
            ctx.emitter.instruction(&format!("jmp {}", label));                 // reject every other dynamic instanceof target kind
        }
    }
    ctx.emitter.label(&ok_label);
}

/// Branches when an eval C-ABI call returned a negative `int` sentinel.
pub(super) fn emit_branch_if_eval_c_int_negative(ctx: &mut FunctionContext<'_>, label: &str) {
    match ctx.emitter.target.arch {
        Arch::AArch64 => {
            let branch = format!("tbnz w0, #31, {}", label);
            ctx.emitter.instruction(&branch);                                   // branch when the C int result is the invalid-target sentinel
        }
        Arch::X86_64 => {
            ctx.emitter.instruction("test eax, eax");                           // set flags from the C int result
            ctx.emitter.instruction(&format!("js {}", label));                  // branch when the C int result is the invalid-target sentinel
        }
    }
}

/// Reorders an unboxed eval string cell into the target string result registers.
pub(super) fn emit_eval_unboxed_string_result(ctx: &mut FunctionContext<'_>) {
    if ctx.emitter.target.arch == Arch::X86_64 {
        ctx.emitter.instruction("mov rax, rdi");                                // move the unboxed string pointer into the x86_64 string-result register
    }
}

/// Emits a borrowed string literal as the current native string result.
pub(super) fn emit_eval_string_result(ctx: &mut FunctionContext<'_>, bytes: &[u8]) {
    let (label, len) = ctx.data.add_string(bytes);
    let (ptr_reg, len_reg) = abi::string_result_regs(ctx.emitter);
    abi::emit_symbol_address(ctx.emitter, ptr_reg, &label);
    abi::emit_load_int_immediate(ctx.emitter, len_reg, len as i64);
}

/// Saves the loaded eval source string while scope setup calls use argument registers.
pub(super) fn save_eval_code_string(ctx: &mut FunctionContext<'_>) {
    let (ptr_reg, len_reg) = abi::string_result_regs(ctx.emitter);
    abi::emit_store_to_sp(ctx.emitter, ptr_reg, EVAL_CODE_PTR_OFFSET);
    abi::emit_store_to_sp(ctx.emitter, len_reg, EVAL_CODE_LEN_OFFSET);
}
