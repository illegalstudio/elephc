//! Purpose:
//! Rebinds native closure descriptors and applies explicit lexical-scope specializations.
//!
//! Called from:
//! - The EIR instruction dispatcher for `Op::ClosureBind`.
//!
//! Key details:
//! - Selection uses the actual source entry, so callbacks may replace a global closure safely.
//! - The existing runtime helper owns capture retention; only the immutable header is replaced.
//! - Scope variants keep capture layout identical and provide their own uniform invoker metadata.

use super::*;

/// Binds the selected descriptor to a receiver, preserving or replacing its visibility scope.
pub(super) fn lower_closure_bind(ctx: &mut FunctionContext<'_>, inst: &Instruction) -> Result<()> {
    let descriptor = expect_operand(inst, 0)?;
    let new_this = expect_operand(inst, 1)?;
    if let Some(Immediate::ClosureBindScope(scope)) = inst.immediate {
        return lower_scoped_bind(ctx, inst, descriptor, new_this, scope, None);
    }
    if let Some(Immediate::ClosureBindObjectScope(_)) = inst.immediate {
        let object_scope = expect_operand(inst, 2)?;
        return lower_scoped_bind(ctx, inst, descriptor, new_this, None, Some(object_scope));
    }
    if ctx.value_php_type(new_this)?.codegen_repr() == PhpType::Mixed {
        abi::emit_reserve_temporary_stack(ctx.emitter, 32);
        ctx.load_value_to_result(descriptor)?;
        abi::emit_store_to_sp(ctx.emitter, abi::int_result_reg(ctx.emitter), 0);
        ctx.load_value_to_result(new_this)?;
        abi::emit_store_to_sp(ctx.emitter, abi::int_result_reg(ctx.emitter), 8);
        unbox_receiver_slot(ctx, 32);
        abi::emit_load_temporary_stack_slot(ctx.emitter, abi::int_arg_reg_name(ctx.emitter.target, 0), 0);
        abi::emit_load_temporary_stack_slot(ctx.emitter, abi::int_arg_reg_name(ctx.emitter.target, 1), 8);
        abi::emit_call_label(ctx.emitter, "__rt_closure_bind");
        abi::emit_release_temporary_stack(ctx.emitter, 32);
        return store_if_result(ctx, inst);
    }
    ctx.load_value_to_reg(descriptor, abi::int_arg_reg_name(ctx.emitter.target, 0))?;
    ctx.load_value_to_reg(new_this, abi::int_arg_reg_name(ctx.emitter.target, 1))?;
    abi::emit_call_label(ctx.emitter, "__rt_closure_bind");
    store_if_result(ctx, inst)
}

/// Selects a compatible scoped entry before the runtime binder allocates the result owner.
fn lower_scoped_bind(
    ctx: &mut FunctionContext<'_>,
    inst: &Instruction,
    descriptor: ValueId,
    new_this: ValueId,
    scope: Option<u32>,
    object_scope: Option<ValueId>,
) -> Result<()> {
    let variants = ctx.module.closure_scope_variants.iter()
        .filter(|variant| object_scope.is_some() || variant.scope_class_id == scope)
        .cloned().collect::<Vec<_>>();
    let mut headers = Vec::new();
    for variant in variants {
        let function = ctx.module.closures.iter()
            .find(|function| function.name == variant.function_name)
            .ok_or_else(|| CodegenIrError::invalid_module("missing scoped closure function"))?;
        let header = core_closures::emit_static_closure_descriptor(ctx, function, variant.capture_count);
        headers.push((variant.scope_class_id, variant.source_names, header, ctx.next_label("closure_scope_match")));
    }
    // -- save the selected PHP operands before dispatch overwrites result and scratch registers --
    abi::emit_reserve_temporary_stack(ctx.emitter, 64);
    let result = abi::int_result_reg(ctx.emitter);
    ctx.load_value_to_result(descriptor)?;
    abi::emit_store_to_sp(ctx.emitter, result, 0);
    ctx.load_value_to_result(new_this)?;
    abi::emit_store_to_sp(ctx.emitter, result, 8);
    if let Some(object_scope) = object_scope {
        ctx.load_value_to_result(object_scope)?;
        abi::emit_store_to_sp(ctx.emitter, result, 16);
    }
    if ctx.value_php_type(new_this)?.codegen_repr() == PhpType::Mixed {
        unbox_receiver_slot(ctx, 64);
    }
    if let Some(object_scope) = object_scope {
        emit_object_scope_class_id(ctx, object_scope)?;
        abi::emit_store_to_sp(ctx.emitter, result, 24);
    }
    let entry = abi::secondary_scratch_reg(ctx.emitter);
    let candidate = abi::tertiary_scratch_reg(ctx.emitter);
    let selected = ctx.next_label("closure_scope_selected");
    for (class, sources, _, matched) in &headers {
        let next_scope = ctx.next_label("closure_scope_next");
        if object_scope.is_some() {
            let matches_scope = ctx.next_label("closure_scope_class");
            abi::emit_load_temporary_stack_slot(ctx.emitter, result, 24);
            abi::emit_load_int_immediate(ctx.emitter, entry, class.map(i64::from).unwrap_or(-1));
            ctx.emitter.instruction(&format!("cmp {result}, {entry}"));         // object scopes use the actual runtime class rather than the declared base
            abi::emit_branch_if_equal_wide(ctx.emitter, &matches_scope);
            abi::emit_jump(ctx.emitter, &next_scope);
            ctx.emitter.label(&matches_scope);
        }
        abi::emit_load_temporary_stack_slot(ctx.emitter, candidate, 0);
        callable_descriptor::emit_load_entry_from_descriptor(ctx.emitter, entry, candidate);
        for source in sources {
            abi::emit_symbol_address(ctx.emitter, candidate, &function_symbol(source));
            ctx.emitter.instruction(&format!("cmp {entry}, {candidate}"));      // compare physical native entries without assuming callable identity
            abi::emit_branch_if_equal_wide(ctx.emitter, matched);
        }
        ctx.emitter.label(&next_scope);
    }
    // -- an incompatible capture layout must not silently keep the wrong lexical scope --
    abi::emit_release_temporary_stack(ctx.emitter, 64);
    exceptions::emit_error(ctx, "Native closure binding cannot apply the requested scope to this callable");
    for (_, _, header, matched) in &headers {
        ctx.emitter.label(matched);
        abi::emit_symbol_address(ctx.emitter, result, header);
        abi::emit_jump(ctx.emitter, &selected);
    }
    ctx.emitter.label(&selected);
    abi::emit_store_to_sp(ctx.emitter, result, 32);
    abi::emit_load_temporary_stack_slot(ctx.emitter, abi::int_arg_reg_name(ctx.emitter.target, 0), 0);
    abi::emit_load_temporary_stack_slot(ctx.emitter, abi::int_arg_reg_name(ctx.emitter.target, 1), 8);
    abi::emit_call_label(ctx.emitter, "__rt_closure_bind");
    // -- retain the runtime helper's capture owners and replace only immutable entry metadata --
    abi::emit_load_temporary_stack_slot(ctx.emitter, candidate, 32);
    for offset in (0..callable_descriptor::CALLABLE_DESC_STATIC_SIZE).step_by(8) {
        abi::emit_load_from_address(ctx.emitter, entry, candidate, offset);
        abi::emit_store_to_address(ctx.emitter, entry, result, offset);
    }
    abi::emit_release_temporary_stack(ctx.emitter, 64);
    store_if_result(ctx, inst)
}

/// Extracts the class of an explicit object scope, accepting boxed nullable object storage.
fn emit_object_scope_class_id(ctx: &mut FunctionContext<'_>, scope: ValueId) -> Result<()> {
    let result = abi::int_result_reg(ctx.emitter);
    abi::emit_load_temporary_stack_slot(ctx.emitter, result, 16);
    let scratch = abi::secondary_scratch_reg(ctx.emitter);
    let global = ctx.next_label("closure_scope_global");
    let done = ctx.next_label("closure_scope_id_done");
    let invalid = ctx.next_label("closure_scope_invalid");
    abi::emit_branch_if_int_result_zero(ctx.emitter, &global);
    if ctx.value_php_type(scope)?.codegen_repr() == PhpType::Mixed {
        abi::emit_load_from_address(ctx.emitter, scratch, result, 0);
        abi::emit_load_int_immediate(ctx.emitter, abi::tertiary_scratch_reg(ctx.emitter), 8);
        ctx.emitter.instruction(&format!("cmp {scratch}, {}", abi::tertiary_scratch_reg(ctx.emitter))); // a boxed null removes the explicit class scope
        abi::emit_branch_if_equal_wide(ctx.emitter, &global);
        abi::emit_load_int_immediate(ctx.emitter, abi::tertiary_scratch_reg(ctx.emitter), 6);
        ctx.emitter.instruction(&format!("cmp {scratch}, {}", abi::tertiary_scratch_reg(ctx.emitter))); // every supported non-null scope is an object
        let object = ctx.next_label("closure_scope_object");
        abi::emit_branch_if_equal_wide(ctx.emitter, &object);
        abi::emit_jump(ctx.emitter, &invalid);
        ctx.emitter.label(&object);
        abi::emit_load_from_address(ctx.emitter, result, result, 8);
    }
    abi::emit_branch_if_int_result_zero(ctx.emitter, &global);
    abi::emit_load_from_address(ctx.emitter, result, result, 0);
    abi::emit_jump(ctx.emitter, &done);
    ctx.emitter.label(&global);
    abi::emit_load_int_immediate(ctx.emitter, result, -1);
    abi::emit_jump(ctx.emitter, &done);
    ctx.emitter.label(&invalid);
    abi::emit_release_temporary_stack(ctx.emitter, 64);
    exceptions::emit_error(ctx, "Native closure binding needs an object or null for this runtime scope");
    ctx.emitter.label(&done);
    Ok(())
}

/// Extracts a boxed object/null receiver after all operands have been saved across helper calls.
fn unbox_receiver_slot(ctx: &mut FunctionContext<'_>, stack_bytes: usize) {
    abi::emit_load_temporary_stack_slot(ctx.emitter, abi::int_result_reg(ctx.emitter), 8);
    abi::emit_call_label(ctx.emitter, "__rt_mixed_unbox");
    let result = abi::int_result_reg(ctx.emitter);
    let scratch = abi::secondary_scratch_reg(ctx.emitter);
    let valid = ctx.next_label("closure_bind_receiver_valid");
    for tag in [6, 8] {
        abi::emit_load_int_immediate(ctx.emitter, scratch, tag);
        ctx.emitter.instruction(&format!("cmp {result}, {scratch}"));           // binding accepts an object payload or PHP null
        abi::emit_branch_if_equal_wide(ctx.emitter, &valid);
    }
    abi::emit_release_temporary_stack(ctx.emitter, stack_bytes);
    exceptions::emit_type_error(ctx, "Closure binding requires an object or null receiver");
    ctx.emitter.label(&valid);
    abi::emit_reg_move(ctx.emitter, result, crate::codegen_support::mixed_unbox_payload_reg(ctx.emitter.target));
    abi::emit_store_to_sp(ctx.emitter, result, 8);
}
