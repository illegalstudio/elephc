//! Purpose:
//! Lowers scalar comparisons and local or ref-cell loads.
//!
//! Called from:
//! - `crate::codegen::lower_inst::lower_instruction()` and sibling lowering helpers.
//!
//! Key details:
//! - Preserves EIR ownership, ABI ordering, runtime symbols, and target-aware lowering.
//! - EIR construction and codegen share the local-storage compatibility predicate.

use super::*;

/// Lowers a signed integer comparison into a boolean result value.
pub(super) fn lower_int_compare(ctx: &mut FunctionContext<'_>, inst: &Instruction) -> Result<()> {
    let predicate = immediate_operands::emit_int_compare(ctx, inst)?;
    let result_reg = abi::int_result_reg(ctx.emitter);
    match ctx.emitter.target.arch {
        Arch::AArch64 => {
            ctx.emitter.instruction(&format!(                                   // materialize the predicate result as 0 or 1
                "cset {}, {}",
                result_reg,
                aarch64_condition(predicate)?
            ));
        }
        Arch::X86_64 => {
            ctx.emitter.instruction(&format!(                                   // materialize the predicate result in the low byte
                "set{} al", x86_64_condition(predicate)?
            ));
            ctx.emitter.instruction(&format!(                                   // widen the predicate byte into the integer result register
                "movzx {}, al", result_reg
            ));
        }
    }
    store_if_result(ctx, inst)
}

/// Lowers an addressable local load into the result register and SSA destination slot.
pub(super) fn lower_load_local(ctx: &mut FunctionContext<'_>, inst: &Instruction) -> Result<()> {
    let slot = expect_local_slot(inst)?;
    let result = inst
        .result
        .ok_or_else(|| CodegenIrError::invalid_module("load_local missing result value"))?;
    ctx.load_local_to_result(slot)?;
    // The slot's box type, so a `?ptr<T>` slot boxed for a `mixed` result reads null for 0.
    let source_ty = ctx.local_mixed_box_type(slot)?;
    let result_ty = ctx.value_php_type(result)?;
    // A by-value PARAMETER slot the prologue boxed (a store later widened it to `Mixed`) is read
    // here as a concrete heap value, and the generic coercion unboxes with a retained payload of
    // its own. Lowering treats a parameter read as a borrow -- parameters borrow their caller's
    // payload -- so it emits no release for that retain: `function expand(iterable $v) { $v =
    // new Bag($v); ... }` leaked the caller's array on every call. When nothing releases or takes
    // over the loaded value, borrow the box's payload instead, as the string case below does.
    if ctx.function.params.get(slot.as_raw() as usize).is_some_and(|param| !param.by_ref)
        && matches!(source_ty.codegen_repr(), PhpType::Mixed)
        && matches!(
            result_ty.codegen_repr(),
            PhpType::Array(_) | PhpType::AssocArray { .. } | PhpType::Object(_) | PhpType::Iterable
        )
        && !ctx.value_is_released_later(result)
        && !ctx.value_uses().is_consumed_beyond_box(result)
    {
        abi::emit_call_label(ctx.emitter, "__rt_mixed_unbox");
        move_reg_to_int_result(ctx, mixed_unbox_low_payload_reg(ctx));
        return ctx.store_result_value(result);
    }
    coerce_loaded_local_for_value(ctx, &source_ty, &result_ty, result)?;
    ctx.store_result_value(result)
}

/// Coerces a raw local load to its result type, borrowing a boxed string nothing releases.
fn coerce_loaded_local_for_value(
    ctx: &mut FunctionContext<'_>,
    source_ty: &PhpType,
    result_ty: &PhpType,
    result: ValueId,
) -> Result<()> {
    if matches!(source_ty.codegen_repr(), PhpType::Mixed)
        && matches!(result_ty.codegen_repr(), PhpType::Str)
        && !ctx.value_is_released_later(result)
        // A value a `return`, a branch argument or a store takes over must own its payload: the
        // epilogue frees the box it was borrowed from, and Laravel's
        // `ComponentTagCompiler::compileTags()` returned the freed bytes (`return $value;` over a
        // boxed `$value`), which the caller freed again -- every Blade page died.
        && !ctx.value_uses().is_consumed(result)
    {
        emit_mixed_local_to_borrowed_string(ctx);
        return Ok(());
    }
    coerce_loaded_local_to_result_type(ctx, source_ty, result_ty)
}

/// Reads a boxed local as a string BORROWED from its box, for a load nothing releases.
///
/// A `string` slot gets boxed `mixed` frame storage when a later statement can promote it to a
/// reference cell (`$ignore($key)` with an unknown callable): a cell holds one word, a string
/// two. A load lowered before that promotion was typed as a borrowed read of a plain string
/// slot, so the IR never releases it -- but `__rt_mixed_cast_string` hands back a fresh copy of
/// a string payload, and every such read leaked it (Laravel Prompts' key handler, once per
/// keystroke). The box owns its payload for as long as the slot keeps it, which is what a
/// borrowed read promises; any other tag still goes through the conversion, whose results are
/// scratch.
fn emit_mixed_local_to_borrowed_string(ctx: &mut FunctionContext<'_>) {
    let convert = ctx.next_label("mixed_local_string_convert");
    let done = ctx.next_label("mixed_local_string_done");
    let result_reg = abi::int_result_reg(ctx.emitter).to_string();
    abi::emit_push_reg(ctx.emitter, &result_reg);
    abi::emit_call_label(ctx.emitter, "__rt_mixed_unbox");
    match ctx.emitter.target.arch {
        Arch::AArch64 => {
            ctx.emitter.instruction("cmp x0, #1");                              // runtime tag 1 is a string payload owned by the box
            ctx.emitter.instruction(&format!("b.ne {}", convert));              // any other payload needs the string conversion
        }
        Arch::X86_64 => {
            ctx.emitter.instruction("cmp rax, 1");                              // runtime tag 1 is a string payload owned by the box
            ctx.emitter.instruction(&format!("jne {}", convert));               // any other payload needs the string conversion
            ctx.emitter.instruction("mov rax, rdi");                            // borrow the payload pointer; rdx already holds its length
        }
    }
    ctx.emit_branch(&done);
    ctx.emitter.label(&convert);
    abi::emit_load_temporary_stack_slot(ctx.emitter, &result_reg, 0);
    move_int_result_to_first_arg(ctx);
    abi::emit_call_label(ctx.emitter, "__rt_mixed_cast_string");
    ctx.emitter.label(&done);
    abi::emit_release_temporary_stack(ctx.emitter, 16);
}

/// Lowers an explicit local ref-cell load into the result register and SSA slot.
pub(super) fn lower_load_ref_cell(ctx: &mut FunctionContext<'_>, inst: &Instruction) -> Result<()> {
    let slot = expect_local_slot(inst)?;
    let result = inst
        .result
        .ok_or_else(|| CodegenIrError::invalid_module("load_ref_cell missing result value"))?;
    let result_ty = ctx.value_php_type(result)?;
    if ctx.local_ref_cell_representation_is_definite(slot) {
        load_ref_cell_content_as(ctx, slot, &result_ty)?;
        return ctx.store_result_value(result);
    }
    if !ctx.local_ref_cell_representation_is_dynamic(slot) {
        let source_ty = ctx.load_raw_local_to_result(slot)?;
        coerce_borrowed_ref_cell_local(ctx, &source_ty, &result_ty)?;
        return ctx.store_result_value(result);
    }
    let state_offset = ctx.ref_cell_state_offset(slot).ok_or_else(|| {
        CodegenIrError::invalid_module(format!(
            "dynamic ref-cell slot {} has no representation flag",
            slot.as_raw()
        ))
    })?;
    let ref_cell = ctx.next_label("dynamic_load_ref_cell");
    let done = ctx.next_label("dynamic_load_ref_cell_done");
    let state_reg = abi::secondary_scratch_reg(ctx.emitter);
    abi::load_at_offset(ctx.emitter, state_reg, state_offset);
    match ctx.emitter.target.arch {
        Arch::AArch64 => {
            ctx.emitter.instruction(                                            // select the alias representation after runtime promotion
                &format!("cbnz {}, {}", state_reg, ref_cell)
            );
        }
        Arch::X86_64 => {
            ctx.emitter.instruction(                                            // test the slot's runtime representation flag
                &format!("test {}, {}", state_reg, state_reg)
            );
            ctx.emitter.instruction(&format!("jne {}", ref_cell));              // select the alias representation after runtime promotion
        }
    }
    let source_ty = ctx.load_raw_local_to_result(slot)?;
    coerce_borrowed_ref_cell_local(ctx, &source_ty, &result_ty)?;
    ctx.emit_branch(&done);
    ctx.emitter.label(&ref_cell);
    load_ref_cell_content_as(ctx, slot, &result_ty)?;
    ctx.emitter.label(&done);
    ctx.store_result_value(result)
}

/// Keeps a ref-cell read borrowed on the raw side of a conditional promotion.
/// The cell path already borrows its payload; a flow-proven heap view from an unpromoted
/// Mixed box must borrow the payload directly instead of adding a lease or string copy.
pub(super) fn coerce_borrowed_ref_cell_local(
    ctx: &mut FunctionContext<'_>,
    source_ty: &PhpType,
    result_ty: &PhpType,
) -> Result<()> {
    if source_ty.codegen_repr() == PhpType::Mixed
        && matches!(result_ty.codegen_repr(), PhpType::Str | PhpType::Array(_)
            | PhpType::AssocArray { .. } | PhpType::Object(_) | PhpType::Callable
            | PhpType::Iterable)
        // With the eval bridge a boxed callable may be an interpreted closure object, which must
        // go through the bridge-descriptor conversion instead of being borrowed as a descriptor.
        && !(result_ty.codegen_repr() == PhpType::Callable
            && ctx.module.required_runtime_features.eval_bridge)
    {
        abi::emit_call_label(ctx.emitter, "__rt_mixed_unbox");
        move_reg_to_int_result(ctx, mixed_unbox_low_payload_reg(ctx));
        return Ok(());
    }
    coerce_loaded_local_to_result_type(ctx, source_ty, result_ty)
}

/// Loads a ref-cell local's content under the flow type `result_ty`.
///
/// A cell whose storage is a Mixed box can be read under a narrowed OBJECT flow type
/// (`function f(&$v) { if ($v instanceof \Stringable) { $v = (string) $v; } }`). The word in the
/// cell is then the box, and taking it for the object read the box's tag (6) as a class id:
/// `(string)` failed on "ArrayIterator" and Symfony's `UrlGenerator` `$caster` killed the demo's
/// login redirect. Borrow the box's object payload instead. A slot promoted at run time (an
/// element reference, `$v = &$a['k']`, then `$v instanceof C && $v->p`) takes the same path.
fn load_ref_cell_content_as(
    ctx: &mut FunctionContext<'_>,
    slot: LocalSlotId,
    result_ty: &PhpType,
) -> Result<()> {
    if matches!(result_ty.codegen_repr(), PhpType::Object(_))
        && matches!(
            ctx.local_php_type(slot)?.codegen_repr(),
            PhpType::Mixed | PhpType::Union(_)
        )
    {
        // The flow type can be stale: a by-reference callee may have stored a string over the
        // object the caller narrowed to (`relay(new Box(), $user)`), and only an object box is
        // unwrapped. Any other content keeps the box, which every gradual consumer reads.
        let keep_box = ctx.next_label("ref_cell_object_keep_box");
        let done = ctx.next_label("ref_cell_object_done");
        load_ref_cell_local_to_result_as(ctx, slot, &PhpType::Mixed)?;
        let result_reg = abi::int_result_reg(ctx.emitter);
        abi::emit_push_reg(ctx.emitter, result_reg);
        abi::emit_call_label(ctx.emitter, "__rt_mixed_unbox");
        match ctx.emitter.target.arch {
            Arch::AArch64 => {
                ctx.emitter.instruction("cmp x0, #6");                          // runtime tag 6 = object
                ctx.emitter.instruction(&format!("b.ne {}", keep_box));
                ctx.emitter.instruction("mov x0, x1");                          // borrow the boxed object payload the cell's box keeps owning
            }
            Arch::X86_64 => {
                ctx.emitter.instruction("cmp rax, 6");                          // runtime tag 6 = object
                ctx.emitter.instruction(&format!("jne {}", keep_box));
                ctx.emitter.instruction("mov rax, rdi");                        // borrow the boxed object payload the cell's box keeps owning
            }
        }
        abi::emit_release_temporary_stack(ctx.emitter, 16);
        abi::emit_jump(ctx.emitter, &done);
        ctx.emitter.label(&keep_box);
        abi::emit_pop_reg(ctx.emitter, result_reg);
        ctx.emitter.label(&done);
        return Ok(());
    }
    load_ref_cell_local_to_result_as(ctx, slot, result_ty)?;
    Ok(())
}

/// Loads the value pointed to by a local ref-cell slot using the supplied alias type.
pub(super) fn load_ref_cell_local_to_result_as(
    ctx: &mut FunctionContext<'_>,
    slot: LocalSlotId,
    ty: &PhpType,
) -> Result<PhpType> {
    let ty = ty.codegen_repr();
    reject_multiword_ref_param_local(&ty, "load")?;
    let offset = ctx.local_offset(slot)?;
    let pointer_reg = abi::symbol_scratch_reg(ctx.emitter);
    abi::load_at_offset(ctx.emitter, pointer_reg, offset);
    match ty {
        PhpType::Str => {
            let (ptr_reg, len_reg) = abi::string_result_regs(ctx.emitter);
            abi::emit_load_from_address(ctx.emitter, ptr_reg, pointer_reg, 0);
            abi::emit_load_from_address(ctx.emitter, len_reg, pointer_reg, 8);
        }
        PhpType::Float => {
            abi::emit_load_from_address(
                ctx.emitter,
                abi::float_result_reg(ctx.emitter),
                pointer_reg,
                0,
            );
        }
        PhpType::TaggedScalar => {
            abi::emit_load_from_address(
                ctx.emitter,
                abi::int_result_reg(ctx.emitter),
                pointer_reg,
                0,
            );
            abi::emit_load_from_address(
                ctx.emitter,
                crate::codegen::sentinels::tagged_scalar_tag_reg(ctx.emitter),
                pointer_reg,
                8,
            );
        }
        _ => {
            abi::emit_load_from_address(
                ctx.emitter,
                abi::int_result_reg(ctx.emitter),
                pointer_reg,
                0,
            );
        }
    }
    Ok(ty)
}

/// Reads the boxed Mixed in the integer result register as an OWNED callable descriptor.
///
/// A box holding a compiled closure (tag 10) yields its descriptor, retained, exactly as the
/// plain unbox does. Anything else is an interpreted callable: `$v instanceof Closure ? $v() : …`
/// narrowed a slot holding an eval closure OBJECT to `callable`, and the call then jumped through
/// the object as if it were a descriptor. The invoker helper builds a bridge descriptor for it,
/// already owned by the caller.
pub(super) fn emit_mixed_local_to_owned_callable_descriptor(ctx: &mut FunctionContext<'_>) {
    let convert = ctx.next_label("mixed_local_callable_convert");
    let done = ctx.next_label("mixed_local_callable_done");
    let result_reg = abi::int_result_reg(ctx.emitter).to_string();
    abi::emit_push_reg(ctx.emitter, &result_reg);
    abi::emit_call_label(ctx.emitter, "__rt_mixed_unbox");
    match ctx.emitter.target.arch {
        Arch::AArch64 => {
            ctx.emitter.instruction("cmp x0, #10");                             // runtime tag 10 already holds a compiled callable descriptor
            ctx.emitter.instruction(&format!("b.ne {}", convert));              // an interpreted callable needs a bridge descriptor
            ctx.emitter.instruction("mov x0, x1");                              // take the existing descriptor payload
        }
        Arch::X86_64 => {
            ctx.emitter.instruction("cmp rax, 10");                             // runtime tag 10 already holds a compiled callable descriptor
            ctx.emitter.instruction(&format!("jne {}", convert));               // an interpreted callable needs a bridge descriptor
            ctx.emitter.instruction("mov rax, rdi");                            // take the existing descriptor payload
        }
    }
    abi::emit_incref_if_refcounted(ctx.emitter, &PhpType::Callable);
    ctx.emit_branch(&done);
    ctx.emitter.label(&convert);
    abi::emit_load_temporary_stack_slot(ctx.emitter, &result_reg, 0);
    ctx.shared.require_callable_argument_normalizer();
    abi::emit_call_label(ctx.emitter, super::CALLABLE_ARGUMENT_NORMALIZER);
    ctx.emitter.label(&done);
    abi::emit_release_temporary_stack(ctx.emitter, 16);
}

/// Converts a loaded local slot value to the SSA result representation requested by EIR.
pub(in crate::codegen) fn coerce_loaded_local_to_result_type(
    ctx: &mut FunctionContext<'_>,
    source_ty: &PhpType,
    result_ty: &PhpType,
) -> Result<()> {
    let box_ty = crate::types::mixed_box_php_type(source_ty);
    let source_ty = source_ty.codegen_repr();
    let result_ty = result_ty.codegen_repr();
    if crate::ir::local_load_types_share_storage(&source_ty, &result_ty) {
        return Ok(());
    }
    match (&source_ty, &result_ty) {
        // A boxed pointer is an integer-like payload and a boxed null casts to 0, which is the
        // null of a `?ptr<T>` read (a `mixed` static local later holding a pointer).
        (PhpType::Mixed, PhpType::Int | PhpType::Pointer(_)) => {
            move_int_result_to_first_arg(ctx);
            abi::emit_call_label(ctx.emitter, "__rt_mixed_cast_int");
            Ok(())
        }
        (PhpType::Mixed, PhpType::Bool) => {
            move_int_result_to_first_arg(ctx);
            abi::emit_call_label(ctx.emitter, "__rt_mixed_cast_bool");
            Ok(())
        }
        (PhpType::Mixed, PhpType::Float) => {
            move_int_result_to_first_arg(ctx);
            abi::emit_call_label(ctx.emitter, "__rt_mixed_cast_float");
            Ok(())
        }
        (PhpType::Mixed, PhpType::Str) => {
            move_int_result_to_first_arg(ctx);
            abi::emit_call_label(ctx.emitter, "__rt_mixed_cast_string");
            Ok(())
        }
        (PhpType::Mixed, PhpType::Callable) if ctx.module.required_runtime_features.eval_bridge => {
            emit_mixed_local_to_owned_callable_descriptor(ctx);
            Ok(())
        }
        (PhpType::Mixed, PhpType::Array(_))
        | (PhpType::Mixed, PhpType::AssocArray { .. })
        | (PhpType::Mixed, PhpType::Callable)
        | (PhpType::Mixed, PhpType::Object(_)) => {
            emit_unbox_mixed_to_owned_refcounted_result(ctx, &result_ty);
            Ok(())
        }
        (PhpType::Mixed, PhpType::Iterable) => {
            emit_unbox_mixed_to_owned_refcounted_result(ctx, &result_ty);
            Ok(())
        }
        (PhpType::Callable, PhpType::Object(_)) => {
            let descriptor_reg = abi::int_result_reg(ctx.emitter).to_string();
            callable_descriptor::emit_load_runtime_capture_to_result(
                ctx.emitter,
                &descriptor_reg,
                0,
                &result_ty,
            );
            Ok(())
        }
        (PhpType::Mixed, PhpType::Void) => {
            abi::emit_load_int_immediate(
                ctx.emitter,
                abi::int_result_reg(ctx.emitter),
                0x7fff_ffff_ffff_fffe,
            );
            Ok(())
        }
        // A pointer-word local that lowering knows holds `null` here (address 0 in the slot)
        // reads as the ordinary null value every `null` consumer expects.
        (PhpType::Pointer(_), PhpType::Void) => {
            abi::emit_load_int_immediate(
                ctx.emitter,
                abi::int_result_reg(ctx.emitter),
                crate::codegen::sentinels::NULL_SENTINEL,
            );
            Ok(())
        }
        (PhpType::TaggedScalar, PhpType::Int) => {
            let null_label = ctx.next_label("narrowed_nullable_local_null");
            let accepted_label = ctx.next_label("narrowed_nullable_local_accepted");
            crate::codegen::sentinels::emit_branch_if_tagged_scalar_null(
                ctx.emitter,
                &null_label,
            );
            abi::emit_jump(ctx.emitter, &accepted_label);
            ctx.emitter.label(&null_label);
            exceptions::emit_type_error(ctx, "Value must be of type int, null given");
            ctx.emitter.label(&accepted_label);
            Ok(())
        }
        (_, PhpType::TaggedScalar) => {
            coerce_loaded_value_to_tagged_scalar(ctx, &source_ty)?;
            Ok(())
        }
        // A `?ptr<T>` source keeps its union, so address 0 is PHP null.
        (_, PhpType::Mixed) => {
            emit_box_current_value_as_mixed(ctx.emitter, &box_ty);
            Ok(())
        }
        _ => Err(CodegenIrError::unsupported(format!(
            "local load from PHP type {:?} as {:?}",
            source_ty, result_ty
        ))),
    }
}

