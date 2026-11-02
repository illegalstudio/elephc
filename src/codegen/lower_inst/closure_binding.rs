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
    } else {
        // A compile-time scope that no variant serves is stamped like a runtime one below.
        abi::emit_load_int_immediate(ctx.emitter, result, scope.map(i64::from).unwrap_or(-1));
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
    // -- a closure no variant serves (a static one, one with only `use` captures, or one whose
    // scope rides in the hidden scope capture) is bound plainly and stamped at run time below --
    let plain = ctx.next_label("closure_scope_plain_bind");
    let finish = ctx.next_label("closure_scope_bound");
    abi::emit_jump(ctx.emitter, &plain);
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
    {
        // A selected variant already has its compile-time lexical class; a closure that ALSO
        // carries the hidden scope capture (`self::class`, `static::class`, `parent::`, runtime
        // member names read it) needs the stamp as well, and the plain path needs nothing else.
        let stamp = ctx.next_label("closure_scope_stamp");
        abi::emit_jump(ctx.emitter, &stamp);
        ctx.emitter.label(&plain);
        abi::emit_load_temporary_stack_slot(ctx.emitter, abi::int_arg_reg_name(ctx.emitter.target, 0), 0);
        abi::emit_load_temporary_stack_slot(ctx.emitter, abi::int_arg_reg_name(ctx.emitter.target, 1), 8);
        abi::emit_call_label(ctx.emitter, "__rt_closure_bind");
        ctx.emitter.label(&stamp);
        abi::emit_store_to_sp(ctx.emitter, result, 40);
        abi::emit_reg_move(ctx.emitter, abi::int_arg_reg_name(ctx.emitter.target, 0), result);
        abi::emit_load_temporary_stack_slot(ctx.emitter, abi::int_arg_reg_name(ctx.emitter.target, 1), 24);
        match ctx.emitter.target.arch {
            Arch::AArch64 => ctx.emitter.instruction("add x1, x1, #1"),         // encode the class id as id + 1 so a miss (-1) stays "unchanged" (0)
            Arch::X86_64 => ctx.emitter.instruction("add rsi, 1"),              // encode the class id as id + 1 so a miss (-1) stays "unchanged" (0)
        }
        abi::emit_call_label(ctx.emitter, "__rt_closure_set_scope");
        abi::emit_load_temporary_stack_slot(ctx.emitter, result, 40);
        ctx.emitter.label(&finish);
        if let Some(object_scope) = object_scope {
            emit_interpreted_scope_stamp(ctx, object_scope)?;
        }
    }
    abi::emit_release_temporary_stack(ctx.emitter, 64);
    store_if_result(ctx, inst)
}

/// Stamps a bound copy with an interpreter-declared scope class (the "negative stamp").
///
/// The variants only know compiled classes: a runtime scope naming a class that only the
/// interpreter declared resolves to id -1 (or to stdClass, which carries eval-declared objects),
/// so the copy kept global scope. When the program links the interpreter, ask it for the scope's
/// interned name record and stamp it, with bit 63 set, into the copy's hidden scope capture
/// (`__rt_closure_set_scope`; a no-op for a closure without one). The eval bridges read it back in
/// `scope_context::emit_effective_scope_name`. The bound descriptor is in the result register on
/// entry and exit; the scope operand is the boxed value saved in slot 16 and its class id in 24.
fn emit_interpreted_scope_stamp(ctx: &mut FunctionContext<'_>, scope: ValueId) -> Result<()> {
    if ctx.value_php_type(scope)?.codegen_repr() != PhpType::Mixed
        || !crate::codegen::eval_callable_helpers::module_needs_eval_callable_descriptor_support(ctx.module)
    {
        return Ok(());
    }
    let result = abi::int_result_reg(ctx.emitter);
    let scratch = abi::secondary_scratch_reg(ctx.emitter);
    let ask = ctx.next_label("closure_scope_ask_interpreter");
    let done = ctx.next_label("closure_scope_interpreted_done");
    abi::emit_store_to_sp(ctx.emitter, result, 40);
    abi::emit_load_temporary_stack_slot(ctx.emitter, result, 24);
    abi::emit_load_int_immediate(ctx.emitter, scratch, -1);
    ctx.emitter.instruction(&format!("cmp {result}, {scratch}"));               // a name no compiled class has?
    abi::emit_branch_if_equal_wide(ctx.emitter, &ask);
    abi::emit_load_symbol_to_reg(ctx.emitter, scratch, "_stdclass_class_id", 0);
    ctx.emitter.instruction(&format!("cmp {result}, {scratch}"));               // stdClass also carries eval-declared objects
    abi::emit_branch_if_equal_wide(ctx.emitter, &ask);
    abi::emit_jump(ctx.emitter, &done);
    ctx.emitter.label(&ask);
    abi::emit_load_temporary_stack_slot(ctx.emitter, result, 16);
    abi::emit_branch_if_int_result_zero(ctx.emitter, &done);                    // a null scope names nothing
    abi::emit_call_label(ctx.emitter, "__rt_mixed_unbox");
    if ctx.emitter.target.arch == Arch::X86_64 {
        ctx.emitter.instruction("mov rsi, rdi");                                // payload low word becomes the second argument
        ctx.emitter.instruction("mov rdi, rax");                                // runtime tag becomes the first argument
    }
    let symbol = ctx.emitter.target.extern_symbol("__elephc_eval_scope_name_token");
    abi::emit_call_label(ctx.emitter, &symbol);
    abi::emit_branch_if_int_result_zero(ctx.emitter, &done);                    // the interpreter does not know it: keep the compiled scope
    match ctx.emitter.target.arch {
        Arch::AArch64 => {
            ctx.emitter.instruction("orr x1, x0, #0x8000000000000000");         // bit 63 marks a name record, not a class id
            abi::emit_load_temporary_stack_slot(ctx.emitter, "x0", 40);
        }
        Arch::X86_64 => {
            ctx.emitter.instruction("mov rsi, rax");                            // the interned name record
            ctx.emitter.instruction("bts rsi, 63");                             // bit 63 marks a name record, not a class id
            abi::emit_load_temporary_stack_slot(ctx.emitter, "rdi", 40);
        }
    }
    abi::emit_call_label(ctx.emitter, "__rt_closure_set_scope");
    ctx.emitter.label(&done);
    abi::emit_load_temporary_stack_slot(ctx.emitter, result, 40);
    Ok(())
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
        // php accepts any string naming a class at run time as the scope; a name no compiled
        // class has resolves to -1, which selects the global scope.
        abi::emit_load_int_immediate(ctx.emitter, abi::tertiary_scratch_reg(ctx.emitter), 1);
        ctx.emitter.instruction(&format!("cmp {scratch}, {}", abi::tertiary_scratch_reg(ctx.emitter))); // a boxed string names the scope class
        let string = ctx.next_label("closure_scope_string");
        abi::emit_branch_if_equal_wide(ctx.emitter, &string);
        abi::emit_jump(ctx.emitter, &invalid);
        ctx.emitter.label(&string);
        match ctx.emitter.target.arch {
            Arch::AArch64 => {
                ctx.emitter.instruction(&format!("ldr x1, [{result}, #16]"));  // string length becomes helper arg1
                ctx.emitter.instruction(&format!("ldr x0, [{result}, #8]"));   // string pointer becomes helper arg0
            }
            Arch::X86_64 => {
                ctx.emitter.instruction(&format!("mov rsi, QWORD PTR [{result} + 16]")); // string length becomes helper arg1
                ctx.emitter.instruction(&format!("mov rdi, QWORD PTR [{result} + 8]"));  // string pointer becomes helper arg0
            }
        }
        abi::emit_call_label(ctx.emitter, "__rt_class_id_by_name");
        abi::emit_jump(ctx.emitter, &done);
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
    exceptions::emit_error(ctx, "Native closure binding needs an object, a class name or null for this runtime scope");
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
