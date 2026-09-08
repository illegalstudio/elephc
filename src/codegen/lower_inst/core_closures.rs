//! Purpose:
//! Lowers closure construction, capture promotion, and EIR callable signatures.
//!
//! Called from:
//! - `crate::codegen::lower_inst::lower_instruction()` and sibling lowering helpers.
//!
//! Key details:
//! - Preserves EIR ownership, ABI ordering, runtime symbols, and target-aware lowering.

use super::*;

/// Materializes an EIR closure literal as a callable descriptor pointer.
pub(super) fn lower_closure_new(ctx: &mut FunctionContext<'_>, inst: &Instruction) -> Result<()> {
    let closure_name = callable_target_data(ctx, inst)?.to_string();
    let closure = ctx
        .module
        .closures
        .iter()
        .find(|function| function.name == closure_name)
        .ok_or_else(|| CodegenIrError::missing_entry("closure", 0))?;
    if inst.operands.len() > closure.params.len() {
        return Err(CodegenIrError::invalid_module(format!(
            "closure_new for {} has {} captures but only {} params",
            closure.name,
            inst.operands.len(),
            closure.params.len()
        )));
    }
    let visible_param_count = closure.params.len() - inst.operands.len();
    let signature = function_signature_from_eir_with_param_count(closure, visible_param_count);
    let captures = closure_capture_params_from_eir(closure, inst.operands.len());
    let static_bindings = static_debug_bindings(closure);
    let owned_object_return = super::object_return_ownership::object_return_ownership(closure)
        == super::object_return_ownership::ObjectReturnOwnership::Owned;
    let invoker_label = super::runtime_wrappers::emit_runtime_callable_invoker_with_ownership(
        ctx, &signature, &captures, owned_object_return,
    );
    let debug_source_path = ctx.module.source_path.clone();
    let debug_source_line = inst.span.map_or(0, |span| span.line);
    let debug_primary_name = format!(
        "{{closure:{}:{}}}",
        debug_source_path.as_deref().unwrap_or("<unknown>"),
        debug_source_line
    );
    let descriptor_label = callable_descriptor::static_closure_descriptor_with_debug_meta(
        ctx.data,
        &function_symbol(&closure.name),
        &closure.name,
        &signature,
        &captures,
        &captures,
        callable_descriptor::CallableDescriptorInvocation::new(
            callable_descriptor::CallableDescriptorShape::Closure,
        ),
        Some(&invoker_label),
        callable_descriptor::CallableDebugMetadata {
            flags: callable_descriptor::CALLABLE_DEBUG_FLAG_CLOSURE,
            primary_name: &debug_primary_name,
            source_path: debug_source_path.as_deref(),
            source_line: debug_source_line,
            bindings: &captures,
            static_bindings: &static_bindings,
        },
    );
    // Every closure gets HEAP storage, capture-free ones included. In PHP a Closure
    // is an object and consumes an object handle from the same pool `new` draws
    // from — `$f = function () {}; var_dump(new P());` prints `object(P)#2` — so a
    // capture-free closure that collapsed to a static `.data` descriptor address
    // would have no allocation to bind a handle to and no lifetime to release one
    // at, and every `#N` after it would be off by one. Routing it through the same
    // runtime descriptor the capturing case already uses gives the closure real
    // storage, so `__rt_object_handle_acquire` binds a handle at creation and
    // `__rt_callable_descriptor_release` → `__rt_heap_free` hands it back exactly
    // when PHP destroys the Closure.
    emit_runtime_closure_descriptor_with_captures(
        ctx,
        &descriptor_label,
        &captures,
        &inst.operands,
    )?;
    store_if_result(ctx, inst)
}

/// Collects symbol-backed function `static $local` slots for php-src Closure debug metadata.
pub(super) fn static_debug_bindings(
    function: &crate::ir::Function,
) -> Vec<callable_descriptor::CallableDebugStaticBinding> {
    function
        .locals
        .iter()
        .filter(|local| local.kind == crate::ir::LocalKind::StaticLocal)
        .filter_map(|local| {
            let name = local.name.clone()?;
            Some(callable_descriptor::CallableDebugStaticBinding {
                value_symbol: crate::names::static_local_symbol(&function.name, &name),
                name,
                php_type: local.php_type.codegen_repr(),
            })
        })
        .collect()
}

/// Returns the hidden closure capture params from the tail of the EIR closure ABI.
pub(super) fn closure_capture_params_from_eir(
    closure: &crate::ir::Function,
    capture_count: usize,
) -> Vec<(String, PhpType, bool)> {
    closure
        .params
        .iter()
        .rev()
        .take(capture_count)
        .collect::<Vec<_>>()
        .into_iter()
        .rev()
        .map(|param| (param.name.clone(), param.php_type.clone(), param.by_ref))
        .collect()
}

/// Allocates a runtime Closure descriptor, assigns its object handle, and stores captures.
pub(super) fn emit_runtime_closure_descriptor_with_captures(
    ctx: &mut FunctionContext<'_>,
    descriptor_label: &str,
    captures: &[(String, PhpType, bool)],
    operands: &[ValueId],
) -> Result<()> {
    let result_reg = abi::int_result_reg(ctx.emitter);
    let descriptor_reg = abi::nested_call_reg(ctx.emitter);
    let total_bytes =
        callable_descriptor::CALLABLE_DESC_RUNTIME_CAPTURE_OFFSET + captures.len() * 16;
    abi::emit_load_int_immediate(ctx.emitter, result_reg, total_bytes as i64);
    abi::emit_call_label(ctx.emitter, "__rt_heap_alloc");
    crate::codegen_support::runtime::emit_acquire_object_handle(ctx.emitter); // every runtime Closure draws one handle that descriptor release returns exactly once
    ctx.emitter
        .instruction(&format!("mov {}, {}", descriptor_reg, result_reg)); // keep the runtime closure descriptor while storing captures
    callable_descriptor::emit_copy_static_descriptor_to_runtime(
        ctx.emitter,
        descriptor_reg,
        descriptor_label,
    );
    for (idx, ((_, capture_ty, by_ref), operand)) in
        captures.iter().zip(operands.iter()).enumerate()
    {
        if *by_ref {
            let slot = local_slot_for_loaded_value(ctx, *operand)?;
            let release_replaced_value = promoted_ref_capture_replaces_owned_value(ctx, *operand)?;
            promote_local_slot_for_ref_capture(
                ctx,
                slot,
                None,
                capture_ty,
                release_replaced_value,
            )?;
            materialize_local_ref_arg_address(ctx, *operand)?;
            callable_descriptor::emit_store_current_result_to_runtime_capture(
                ctx.emitter,
                descriptor_reg,
                idx,
                &PhpType::Int,
            );
            continue;
        }
        ctx.load_value_to_result(*operand)?;
        if !ctx.value_can_transfer_ownership_to_consumer(*operand)? {
            if capture_ty.codegen_repr() == PhpType::Str {
                abi::emit_call_label(ctx.emitter, "__rt_str_persist");
            } else {
                abi::emit_incref_if_refcounted(ctx.emitter, capture_ty);
            }
        }
        callable_descriptor::emit_store_current_result_to_runtime_capture(
            ctx.emitter,
            descriptor_reg,
            idx,
            capture_ty,
        );
    }
    if descriptor_reg != result_reg {
        ctx.emitter
            .instruction(&format!("mov {}, {}", result_reg, descriptor_reg)); // return the runtime closure descriptor pointer
    }
    Ok(())
}

/// Returns whether a by-reference closure capture replaces a caller-owned local value.
pub(super) fn promoted_ref_capture_replaces_owned_value(
    ctx: &FunctionContext<'_>,
    value: ValueId,
) -> Result<bool> {
    Ok(matches!(
        ctx.value_ownership(value)?,
        Ownership::Owned | Ownership::MaybeOwned
    ))
}

/// Promotes a normal local slot to a heap ref-cell for an escaping by-reference capture.
pub(super) fn promote_local_slot_for_ref_capture(
    ctx: &mut FunctionContext<'_>,
    slot: LocalSlotId,
    owner_slot: Option<LocalSlotId>,
    capture_ty: &PhpType,
    release_replaced_value: bool,
) -> Result<()> {
    if local_slot_stores_ref_cell_pointer(ctx, slot) {
        let Some(state_offset) = ctx.ref_cell_state_offset(slot) else {
            return Ok(());
        };
        let promote = ctx.next_label("promote_local_ref_cell");
        let done = ctx.next_label("promote_local_ref_cell_done");
        let state_reg = abi::int_result_reg(ctx.emitter);
        abi::load_at_offset(ctx.emitter, state_reg, state_offset);
        match ctx.emitter.target.arch {
            Arch::AArch64 => {
                ctx.emitter.instruction(
                    &format!("cbz {}, {}", state_reg, promote)
                );                                                              // create the fallback cell only on the first runtime promotion
                ctx.emitter
                    .instruction(&format!("b {}", done));                         // reuse the existing cell on later loop iterations
            }
            Arch::X86_64 => {
                ctx.emitter.instruction(
                    &format!("test {}, {}", state_reg, state_reg)
                );                                                              // test whether this slot already stores a fallback cell
                ctx.emitter
                    .instruction(&format!("je {}", promote));                       // create the fallback cell only on the first runtime promotion
                ctx.emitter
                    .instruction(&format!("jmp {}", done));                         // reuse the existing cell on later loop iterations
            }
        }
        ctx.emitter.label(&promote);
        promote_local_slot_for_ref_capture_unchecked(
            ctx,
            slot,
            owner_slot,
            capture_ty,
            release_replaced_value,
        )?;
        ctx.emitter.label(&done);
        return Ok(());
    }
    promote_local_slot_for_ref_capture_unchecked(
        ctx,
        slot,
        owner_slot,
        capture_ty,
        release_replaced_value,
    )
}

/// Allocates and installs a ref-cell after the caller has ruled out an existing cell.
pub(super) fn promote_local_slot_for_ref_capture_unchecked(
    ctx: &mut FunctionContext<'_>,
    slot: LocalSlotId,
    owner_slot: Option<LocalSlotId>,
    capture_ty: &PhpType,
    release_replaced_value: bool,
) -> Result<()> {
    reject_multiword_ref_param_local(capture_ty, "capture")?;
    let local_ty = ctx.local_php_type(slot)?;
    let offset = ctx.local_offset(slot)?;
    abi::emit_load(ctx.emitter, &local_ty, offset);
    retain_promoted_ref_cell_value(ctx, &local_ty);
    abi::emit_push_result_value(ctx.emitter, &local_ty);
    abi::emit_load_int_immediate(ctx.emitter, abi::int_result_reg(ctx.emitter), 16);
    abi::emit_call_label(ctx.emitter, "__rt_heap_alloc");
    let cell_reg = abi::symbol_scratch_reg(ctx.emitter);
    ctx.emitter.instruction(&format!(
        "mov {}, {}",
        cell_reg,
        abi::int_result_reg(ctx.emitter)
    ));                                                                         // keep the promoted closure capture cell while restoring its value
    pop_result_value(ctx, &local_ty);
    store_current_result_to_ref_cell(ctx, cell_reg, &local_ty);
    if release_replaced_value {
        release_replaced_promoted_local_value(ctx, &local_ty, offset, cell_reg);
    }
    abi::store_at_offset_scratch(
        ctx.emitter,
        cell_reg,
        offset,
        abi::tertiary_scratch_reg(ctx.emitter),
    );
    if let Some(owner_slot) = owner_slot {
        let owner_offset = ctx.local_offset(owner_slot)?;
        abi::store_at_offset_scratch(
            ctx.emitter,
            cell_reg,
            owner_offset,
            abi::tertiary_scratch_reg(ctx.emitter),
        );
    }
    ctx.mark_promoted_ref_cell(slot);
    Ok(())
}

/// Releases the old local owner after its retained value has been copied into a ref-cell.
pub(super) fn release_replaced_promoted_local_value(
    ctx: &mut FunctionContext<'_>,
    local_ty: &PhpType,
    offset: usize,
    cell_reg: &str,
) {
    let local_ty = local_ty.codegen_repr();
    if !matches!(local_ty, PhpType::Str | PhpType::Callable) && !local_ty.is_refcounted() {
        return;
    }
    abi::emit_push_reg(ctx.emitter, cell_reg);
    match local_ty {
        PhpType::Str => {
            abi::load_at_offset_scratch(
                ctx.emitter,
                abi::int_result_reg(ctx.emitter),
                offset,
                abi::secondary_scratch_reg(ctx.emitter),
            );
            abi::emit_call_label(ctx.emitter, "__rt_heap_free_safe");
        }
        PhpType::Callable => {
            abi::load_at_offset_scratch(
                ctx.emitter,
                abi::int_result_reg(ctx.emitter),
                offset,
                abi::secondary_scratch_reg(ctx.emitter),
            );
            callable_descriptor::emit_release_current_descriptor(ctx.emitter);
        }
        ty if ty.is_refcounted() => {
            abi::load_at_offset_scratch(
                ctx.emitter,
                abi::int_result_reg(ctx.emitter),
                offset,
                abi::secondary_scratch_reg(ctx.emitter),
            );
            abi::emit_decref_if_refcounted(ctx.emitter, &ty);
        }
        _ => {}
    }
    abi::emit_pop_reg(ctx.emitter, cell_reg);
}

/// Retains or persists a value before it is moved into a promoted ref-cell.
pub(super) fn retain_promoted_ref_cell_value(ctx: &mut FunctionContext<'_>, local_ty: &PhpType) {
    match local_ty.codegen_repr() {
        PhpType::Str => {
            abi::emit_call_label(ctx.emitter, "__rt_str_persist");
        }
        PhpType::Callable => {
            callable_descriptor::emit_retain_current_descriptor(ctx.emitter);
        }
        other if other.is_refcounted() => {
            abi::emit_incref_if_refcounted(ctx.emitter, &other);
        }
        _ => {}
    }
}

/// Pops a previously saved result value back into the target result registers.
pub(super) fn pop_result_value(ctx: &mut FunctionContext<'_>, local_ty: &PhpType) {
    match local_ty.codegen_repr() {
        PhpType::Float => {
            abi::emit_pop_float_reg(ctx.emitter, abi::float_result_reg(ctx.emitter));
        }
        PhpType::Str => {
            let (ptr_reg, len_reg) = abi::string_result_regs(ctx.emitter);
            abi::emit_pop_reg_pair(ctx.emitter, ptr_reg, len_reg);
        }
        PhpType::TaggedScalar => {
            abi::emit_pop_reg_pair(
                ctx.emitter,
                abi::int_result_reg(ctx.emitter),
                crate::codegen::sentinels::tagged_scalar_tag_reg(ctx.emitter),
            );
        }
        PhpType::Void | PhpType::Never => {}
        _ => {
            abi::emit_pop_reg(ctx.emitter, abi::int_result_reg(ctx.emitter));
        }
    }
}

/// Stores the current result registers into a two-word heap ref-cell.
pub(super) fn store_current_result_to_ref_cell(
    ctx: &mut FunctionContext<'_>,
    cell_reg: &str,
    local_ty: &PhpType,
) {
    match local_ty.codegen_repr() {
        PhpType::Float => {
            abi::emit_store_to_address(
                ctx.emitter,
                abi::float_result_reg(ctx.emitter),
                cell_reg,
                0,
            );
            abi::emit_store_zero_to_address(ctx.emitter, cell_reg, 8);
        }
        PhpType::Str => {
            let (ptr_reg, len_reg) = abi::string_result_regs(ctx.emitter);
            abi::emit_store_to_address(ctx.emitter, ptr_reg, cell_reg, 0);
            abi::emit_store_to_address(ctx.emitter, len_reg, cell_reg, 8);
        }
        PhpType::TaggedScalar => {
            abi::emit_store_to_address(ctx.emitter, abi::int_result_reg(ctx.emitter), cell_reg, 0);
            abi::emit_store_to_address(
                ctx.emitter,
                crate::codegen::sentinels::tagged_scalar_tag_reg(ctx.emitter),
                cell_reg,
                8,
            );
        }
        PhpType::Void | PhpType::Never => {
            abi::emit_store_zero_to_address(ctx.emitter, cell_reg, 0);
            abi::emit_store_zero_to_address(ctx.emitter, cell_reg, 8);
        }
        _ => {
            abi::emit_store_to_address(ctx.emitter, abi::int_result_reg(ctx.emitter), cell_reg, 0);
            abi::emit_store_zero_to_address(ctx.emitter, cell_reg, 8);
        }
    }
}

/// Reconstructs callable signature metadata from an emitted EIR function.
pub(in crate::codegen) fn function_signature_from_eir(function: &crate::ir::Function) -> FunctionSig {
    function_signature_from_eir_with_param_count(function, function.params.len())
}

/// Reconstructs signature metadata from the first `param_count` EIR params.
pub(super) fn function_signature_from_eir_with_param_count(
    function: &crate::ir::Function,
    param_count: usize,
) -> FunctionSig {
    if let Some(signature) = &function.signature {
        let mut signature = signature.clone();
        let original_param_count = signature.params.len();
        ensure_variadic_param_slot(&mut signature);
        if original_param_count == param_count {
            return signature.clone();
        }
    }

    FunctionSig {
        params: function
            .params
            .iter()
            .take(param_count)
            .map(|param| (param.name.clone(), param.php_type.clone()))
            .collect(),
        param_type_exprs: vec![None; param_count],
        param_attributes: Vec::new(),
        defaults: function
            .params
            .iter()
            .take(param_count)
            .map(|_| None)
            .collect(),
        return_type: function.return_php_type.clone(),
        declared_return: !matches!(function.return_php_type, PhpType::Mixed),
        by_ref_return: false,
        ref_params: function
            .params
            .iter()
            .take(param_count)
            .map(|param| param.by_ref)
            .collect(),
        declared_params: function
            .params
            .iter()
            .take(param_count)
            .map(|param| !matches!(param.php_type, PhpType::Mixed))
            .collect(),
        variadic: function
            .params
            .iter()
            .take(param_count)
            .find(|param| param.variadic)
            .map(|param| param.name.clone()),
        deprecation: None,
    }
}

/// Adds the virtual variadic array slot when the EIR ABI stores it outside `params`.
pub(super) fn ensure_variadic_param_slot(signature: &mut FunctionSig) {
    let Some(variadic) = signature.variadic.clone() else {
        return;
    };
    if signature.params.iter().any(|(name, _)| name == &variadic) {
        return;
    }
    let variadic_index = signature.params.len();
    let variadic_type_expr = if signature.param_type_exprs.len() > variadic_index {
        signature.param_type_exprs.remove(variadic_index)
    } else {
        None
    };
    let variadic_ref = if signature.ref_params.len() > variadic_index {
        signature.ref_params.remove(variadic_index)
    } else {
        false
    };
    let variadic_declared = if signature.declared_params.len() > variadic_index {
        signature.declared_params.remove(variadic_index)
    } else {
        false
    };
    signature
        .params
        .push((variadic, PhpType::Array(Box::new(PhpType::Mixed))));
    signature.defaults.push(None);
    signature.ref_params.push(variadic_ref);
    signature.declared_params.push(variadic_declared);
    signature.param_type_exprs.push(variadic_type_expr);
}
