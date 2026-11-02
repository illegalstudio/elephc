//! Purpose:
//! Lowers eval-created functions, objects, methods, and late-static members.
//!
//! Called from:
//! - The eval lowering facade and sibling eval support modules.
//!
//! Key details:
//! - All dynamic dispatch paths keep the existing scratch-frame contract.

use super::*;

/// Lowers a native positional call to a function declared by a prior `eval()` call.
pub(in crate::codegen::lower_inst::builtins) fn lower_eval_function_call(
    ctx: &mut FunctionContext<'_>,
    inst: &Instruction,
) -> Result<()> {
    let function_name = ctx.function_name_data(expect_data(inst)?)?.to_string();
    lower_eval_function_call_named(ctx, inst, &function_name)
}

/// Calls PHP function `function_name` through the eval bridge with `inst`'s operands, storing
/// the returned Mixed cell as `inst`'s result.
///
/// The named form lets a natively lowered builtin hand a case it cannot answer (a class only
/// the interpreter declared) to the bridge exactly as an `eval_function_call` would.
pub(in crate::codegen::lower_inst::builtins) fn lower_eval_function_call_named(
    ctx: &mut FunctionContext<'_>,
    inst: &Instruction,
    function_name: &str,
) -> Result<()> {
    if !has_eval_context(ctx) {
        return lower_eval_owned_function_call(ctx, inst, function_name);
    }
    let function_name = function_name.to_string();
    let args_offset = EVAL_STACK_BYTES;
    let stack_bytes = eval_function_call_stack_bytes(inst.operands.len());
    abi::emit_reserve_temporary_stack(ctx.emitter, stack_bytes);
    ensure_eval_context(ctx)?;
    let boxed = store_eval_function_call_args(ctx, inst, args_offset)?;
    load_eval_context_to_arg(ctx, 0);
    let (name_label, name_len) = ctx.data.add_string(function_name.as_bytes());
    let name_arg = abi::int_arg_reg_name(ctx.emitter.target, 1);
    abi::emit_symbol_address(ctx.emitter, name_arg, &name_label);
    abi::emit_load_int_immediate(
        ctx.emitter,
        abi::int_arg_reg_name(ctx.emitter.target, 2),
        name_len as i64,
    );
    let args_arg = abi::int_arg_reg_name(ctx.emitter.target, 3);
    if inst.operands.is_empty() {
        abi::emit_load_int_immediate(ctx.emitter, args_arg, 0);
    } else {
        abi::emit_temporary_stack_address(ctx.emitter, args_arg, args_offset);
    }
    abi::emit_load_int_immediate(
        ctx.emitter,
        abi::int_arg_reg_name(ctx.emitter.target, 4),
        inst.operands.len() as i64,
    );
    let out_arg = abi::int_arg_reg_name(ctx.emitter.target, 5);
    abi::emit_temporary_stack_address(ctx.emitter, out_arg, 0);
    let symbol = ctx
        .emitter
        .target
        .extern_symbol("__elephc_eval_call_function");
    abi::emit_call_label(ctx.emitter, &symbol);
    emit_eval_status_check_releasing(ctx, &boxed);
    emit_release_eval_boxed_operands_keeping_result(ctx, &boxed);
    let result_reg = abi::int_result_reg(ctx.emitter);
    abi::emit_load_temporary_stack_slot(ctx.emitter, result_reg, EVAL_RESULT_VALUE_CELL_OFFSET);
    abi::emit_release_temporary_stack(ctx.emitter, stack_bytes);
    store_if_result(ctx, inst)
}

/// Calls a function declared by a dynamically included file from an AOT frame without eval locals.
///
/// Runtime include declarations are request-global in PHP, while an unrelated compiled method has
/// no frame-local eval context to consult. Resolve the function's owning context by its canonical
/// PHP name, then use the existing Magician function-call ABI with the normal staged arguments.
fn lower_eval_owned_function_call(
    ctx: &mut FunctionContext<'_>,
    inst: &Instruction,
    function_name: &str,
) -> Result<()> {
    let function_name = function_name.to_string();
    let args_offset = EVAL_STACK_BYTES;
    let stack_bytes = eval_function_call_stack_bytes(inst.operands.len());
    let missing_label = ctx.next_label("eval_owned_function_missing");
    let done_label = ctx.next_label("eval_owned_function_done");
    let result_reg = abi::int_result_reg(ctx.emitter).to_string();

    abi::emit_reserve_temporary_stack(ctx.emitter, stack_bytes);
    let boxed = store_eval_function_call_args(ctx, inst, args_offset)?;
    let (name_label, name_len) = ctx.data.add_string(function_name.as_bytes());
    let name_ptr_arg = abi::int_arg_reg_name(ctx.emitter.target, 0);
    abi::emit_symbol_address(ctx.emitter, name_ptr_arg, &name_label);
    abi::emit_load_int_immediate(
        ctx.emitter,
        abi::int_arg_reg_name(ctx.emitter.target, 1),
        name_len as i64,
    );
    let owner_symbol = ctx
        .emitter
        .target
        .extern_symbol("__elephc_eval_function_owner_context");
    abi::emit_call_label(ctx.emitter, &owner_symbol);
    let bridge_serves_label = ctx.next_label("eval_owned_function_bridge_serves");
    let owner_found_label = ctx.next_label("eval_owned_function_owner_found");
    abi::emit_branch_if_int_result_zero(ctx.emitter, &bridge_serves_label);
    abi::emit_store_to_sp(ctx.emitter, &result_reg, EVAL_CONTEXT_HANDLE_OFFSET);
    abi::emit_jump(ctx.emitter, &owner_found_label);

    // "Which context DECLARED this function" is not the same question as "can this call be
    // answered". The bridge answers for names no context declares: the two backtrace functions,
    // the OPcache family, the procedural date aliases, every builtin the interpreter implements.
    // An unqualified `debug_backtrace()` written inside a namespace arrives here as the namespaced
    // candidate, no context declares it, and ending on the declaration question turned it into
    // `Call to undefined function <namespace>\\debug_backtrace()` where PHP returns the frames.
    // A null context handle is what the bridge takes for "resolve this yourself".
    ctx.emitter.label(&bridge_serves_label);
    abi::emit_symbol_address(
        ctx.emitter,
        abi::int_arg_reg_name(ctx.emitter.target, 0),
        &name_label,
    );
    abi::emit_load_int_immediate(
        ctx.emitter,
        abi::int_arg_reg_name(ctx.emitter.target, 1),
        name_len as i64,
    );
    let can_call_symbol = ctx
        .emitter
        .target
        .extern_symbol("__elephc_eval_bridge_can_call_function");
    abi::emit_call_label(ctx.emitter, &can_call_symbol);
    abi::emit_branch_if_int_result_zero(ctx.emitter, &missing_label);
    abi::emit_load_int_immediate(ctx.emitter, &result_reg, 0);
    abi::emit_store_to_sp(ctx.emitter, &result_reg, EVAL_CONTEXT_HANDLE_OFFSET);
    ctx.emitter.label(&owner_found_label);

    let context_arg = abi::int_arg_reg_name(ctx.emitter.target, 0);
    abi::emit_load_temporary_stack_slot(ctx.emitter, context_arg, EVAL_CONTEXT_HANDLE_OFFSET);
    let name_ptr_arg = abi::int_arg_reg_name(ctx.emitter.target, 1);
    abi::emit_symbol_address(ctx.emitter, name_ptr_arg, &name_label);
    abi::emit_load_int_immediate(
        ctx.emitter,
        abi::int_arg_reg_name(ctx.emitter.target, 2),
        name_len as i64,
    );
    let args_arg = abi::int_arg_reg_name(ctx.emitter.target, 3);
    if inst.operands.is_empty() {
        abi::emit_load_int_immediate(ctx.emitter, args_arg, 0);
    } else {
        abi::emit_temporary_stack_address(ctx.emitter, args_arg, args_offset);
    }
    abi::emit_load_int_immediate(
        ctx.emitter,
        abi::int_arg_reg_name(ctx.emitter.target, 4),
        inst.operands.len() as i64,
    );
    let out_arg = abi::int_arg_reg_name(ctx.emitter.target, 5);
    abi::emit_temporary_stack_address(ctx.emitter, out_arg, 0);
    let symbol = ctx
        .emitter
        .target
        .extern_symbol("__elephc_eval_call_function");
    abi::emit_call_label(ctx.emitter, &symbol);
    emit_eval_status_check_releasing(ctx, &boxed);
    emit_release_eval_boxed_operands_keeping_result(ctx, &boxed);
    abi::emit_load_temporary_stack_slot(ctx.emitter, &result_reg, EVAL_RESULT_VALUE_CELL_OFFSET);
    abi::emit_release_temporary_stack(ctx.emitter, stack_bytes);
    abi::emit_jump(ctx.emitter, &done_label);

    // The missing-function path ends the process, so its boxed operands outlive nothing.
    ctx.emitter.label(&missing_label);
    abi::emit_release_temporary_stack(ctx.emitter, stack_bytes);
    emit_eval_owned_function_missing_fatal(ctx, &function_name);
    ctx.emitter.label(&done_label);
    store_if_result(ctx, inst)
}

/// Emits the PHP undefined-function fatal when no dynamic declaration owner exists.
fn emit_eval_owned_function_missing_fatal(ctx: &mut FunctionContext<'_>, function_name: &str) {
    let message = format!("Fatal error: Call to undefined function {function_name}()\n");
    let (message_label, message_len) = ctx.data.add_string(message.as_bytes());
    match ctx.emitter.target.arch {
        Arch::AArch64 => {
            ctx.emitter.instruction("mov x0, #2");
            ctx.emitter.adrp("x1", &message_label);
            ctx.emitter.add_lo12("x1", "x1", &message_label);
            ctx.emitter.instruction(&format!("mov x2, #{}", message_len));
            ctx.emitter.syscall(4);
            abi::emit_exit(ctx.emitter, 1);
        }
        Arch::X86_64 => {
            ctx.emitter.instruction("mov edi, 2");
            abi::emit_symbol_address(ctx.emitter, "rsi", &message_label);
            ctx.emitter.instruction(&format!("mov edx, {}", message_len));
            ctx.emitter.instruction("mov eax, 1");
            ctx.emitter.instruction("syscall");
            abi::emit_exit(ctx.emitter, 1);
        }
    }
}

/// Lowers a native call to a prior eval-declared function using an argument array/hash.
///
/// The bridge borrows the container cell rather than taking it, so the box this frame made is
/// still this frame's to release. Boxing an array retains only the container, and releasing the
/// cell decrefs that same container through `__rt_mixed_free_deep`, so the release is neutral for
/// the elements the container spreads.
pub(in crate::codegen::lower_inst::builtins) fn lower_eval_function_call_array(
    ctx: &mut FunctionContext<'_>,
    inst: &Instruction,
) -> Result<()> {
    if !has_eval_context(ctx) {
        return lower_eval_owned_function_call_array(ctx, inst);
    }
    super::super::ensure_arg_count(inst, "eval function call array", 1)?;
    let function_name = ctx.function_name_data(expect_data(inst)?)?.to_string();
    let arg_array = expect_operand(inst, 0)?;
    abi::emit_reserve_temporary_stack(ctx.emitter, EVAL_STACK_BYTES);
    ensure_eval_context(ctx)?;
    let mut boxed = EvalBoxedOperands::new();
    boxed.extend(store_eval_mixed_operand_at(
        ctx,
        arg_array,
        EVAL_TEMP_CELL_OFFSET,
    )?);
    load_eval_context_to_arg(ctx, 0);
    let (name_label, name_len) = ctx.data.add_string(function_name.as_bytes());
    let name_arg = abi::int_arg_reg_name(ctx.emitter.target, 1);
    abi::emit_symbol_address(ctx.emitter, name_arg, &name_label);
    abi::emit_load_int_immediate(
        ctx.emitter,
        abi::int_arg_reg_name(ctx.emitter.target, 2),
        name_len as i64,
    );
    let args_arg = abi::int_arg_reg_name(ctx.emitter.target, 3);
    abi::emit_load_temporary_stack_slot(ctx.emitter, args_arg, EVAL_TEMP_CELL_OFFSET);
    let out_arg = abi::int_arg_reg_name(ctx.emitter.target, 4);
    abi::emit_temporary_stack_address(ctx.emitter, out_arg, 0);
    let symbol = ctx
        .emitter
        .target
        .extern_symbol("__elephc_eval_call_function_array");
    abi::emit_call_label(ctx.emitter, &symbol);
    emit_eval_status_check_releasing(ctx, &boxed);
    emit_release_eval_boxed_operands_keeping_result(ctx, &boxed);
    let result_reg = abi::int_result_reg(ctx.emitter);
    abi::emit_load_temporary_stack_slot(ctx.emitter, result_reg, EVAL_RESULT_VALUE_CELL_OFFSET);
    abi::emit_release_temporary_stack(ctx.emitter, EVAL_STACK_BYTES);
    store_if_result(ctx, inst)
}

/// Calls a runtime-declared function with one spread container from an AOT frame without eval locals.
///
/// The bridge borrows the container cell, so the box is released on the serviced exit; the
/// missing-function path ends the process, so its boxed container outlives nothing.
fn lower_eval_owned_function_call_array(
    ctx: &mut FunctionContext<'_>,
    inst: &Instruction,
) -> Result<()> {
    super::super::ensure_arg_count(inst, "eval function call array", 1)?;
    let function_name = ctx.function_name_data(expect_data(inst)?)?.to_string();
    let arg_array = expect_operand(inst, 0)?;
    let missing_label = ctx.next_label("eval_owned_function_array_missing");
    let done_label = ctx.next_label("eval_owned_function_array_done");
    let result_reg = abi::int_result_reg(ctx.emitter).to_string();

    abi::emit_reserve_temporary_stack(ctx.emitter, EVAL_STACK_BYTES);
    let mut boxed = EvalBoxedOperands::new();
    boxed.extend(store_eval_mixed_operand_at(
        ctx,
        arg_array,
        EVAL_TEMP_CELL_OFFSET,
    )?);
    let (name_label, name_len) = ctx.data.add_string(function_name.as_bytes());
    let name_ptr_arg = abi::int_arg_reg_name(ctx.emitter.target, 0);
    abi::emit_symbol_address(ctx.emitter, name_ptr_arg, &name_label);
    abi::emit_load_int_immediate(
        ctx.emitter,
        abi::int_arg_reg_name(ctx.emitter.target, 1),
        name_len as i64,
    );
    let owner_symbol = ctx
        .emitter
        .target
        .extern_symbol("__elephc_eval_function_owner_context");
    abi::emit_call_label(ctx.emitter, &owner_symbol);
    let bridge_serves_label = ctx.next_label("eval_owned_function_bridge_serves");
    let owner_found_label = ctx.next_label("eval_owned_function_owner_found");
    abi::emit_branch_if_int_result_zero(ctx.emitter, &bridge_serves_label);
    abi::emit_store_to_sp(ctx.emitter, &result_reg, EVAL_CONTEXT_HANDLE_OFFSET);
    abi::emit_jump(ctx.emitter, &owner_found_label);

    // Same as the positional call above: a name no context declares can still be one the bridge
    // answers itself, and a null context handle is how it is asked to.
    ctx.emitter.label(&bridge_serves_label);
    abi::emit_symbol_address(
        ctx.emitter,
        abi::int_arg_reg_name(ctx.emitter.target, 0),
        &name_label,
    );
    abi::emit_load_int_immediate(
        ctx.emitter,
        abi::int_arg_reg_name(ctx.emitter.target, 1),
        name_len as i64,
    );
    let can_call_symbol = ctx
        .emitter
        .target
        .extern_symbol("__elephc_eval_bridge_can_call_function");
    abi::emit_call_label(ctx.emitter, &can_call_symbol);
    abi::emit_branch_if_int_result_zero(ctx.emitter, &missing_label);
    abi::emit_load_int_immediate(ctx.emitter, &result_reg, 0);
    abi::emit_store_to_sp(ctx.emitter, &result_reg, EVAL_CONTEXT_HANDLE_OFFSET);
    ctx.emitter.label(&owner_found_label);

    let context_arg = abi::int_arg_reg_name(ctx.emitter.target, 0);
    abi::emit_load_temporary_stack_slot(ctx.emitter, context_arg, EVAL_CONTEXT_HANDLE_OFFSET);
    let name_ptr_arg = abi::int_arg_reg_name(ctx.emitter.target, 1);
    abi::emit_symbol_address(ctx.emitter, name_ptr_arg, &name_label);
    abi::emit_load_int_immediate(
        ctx.emitter,
        abi::int_arg_reg_name(ctx.emitter.target, 2),
        name_len as i64,
    );
    let args_arg = abi::int_arg_reg_name(ctx.emitter.target, 3);
    abi::emit_load_temporary_stack_slot(ctx.emitter, args_arg, EVAL_TEMP_CELL_OFFSET);
    let out_arg = abi::int_arg_reg_name(ctx.emitter.target, 4);
    abi::emit_temporary_stack_address(ctx.emitter, out_arg, 0);
    let symbol = ctx
        .emitter
        .target
        .extern_symbol("__elephc_eval_call_function_array");
    abi::emit_call_label(ctx.emitter, &symbol);
    emit_eval_status_check_releasing(ctx, &boxed);
    emit_release_eval_boxed_operands_keeping_result(ctx, &boxed);
    abi::emit_load_temporary_stack_slot(ctx.emitter, &result_reg, EVAL_RESULT_VALUE_CELL_OFFSET);
    abi::emit_release_temporary_stack(ctx.emitter, EVAL_STACK_BYTES);
    abi::emit_jump(ctx.emitter, &done_label);

    ctx.emitter.label(&missing_label);
    abi::emit_release_temporary_stack(ctx.emitter, EVAL_STACK_BYTES);
    emit_eval_owned_function_missing_fatal(ctx, &function_name);
    ctx.emitter.label(&done_label);
    store_if_result(ctx, inst)
}

/// Lowers native construction of a class declared by a prior eval fragment.
pub(in crate::codegen::lower_inst::builtins) fn lower_eval_object_new(
    ctx: &mut FunctionContext<'_>,
    inst: &Instruction,
) -> Result<()> {
    let (name_label, name_len) = ctx.intern_class_name_data(expect_data(inst)?)?;
    let args_offset = EVAL_STACK_BYTES;
    let stack_bytes = eval_function_call_stack_bytes(inst.operands.len());
    abi::emit_reserve_temporary_stack(ctx.emitter, stack_bytes);
    ensure_eval_context(ctx)?;
    // The construction is written inside this frame's class body, and that lexical class is the
    // calling scope php judges the constructor's visibility from: `BigNumber::of()` building a
    // `BigInteger` with a PROTECTED constructor was refused "from global scope".
    let pushed_class_scope = if current_eval_method_class(ctx).is_some() {
        push_eval_context_class_scope(ctx)?
    } else {
        false
    };
    let boxed = store_eval_function_call_args(ctx, inst, args_offset)?;
    load_eval_context_to_arg(ctx, 0);
    let name_arg = abi::int_arg_reg_name(ctx.emitter.target, 1);
    abi::emit_symbol_address(ctx.emitter, name_arg, &name_label);
    abi::emit_load_int_immediate(
        ctx.emitter,
        abi::int_arg_reg_name(ctx.emitter.target, 2),
        name_len as i64,
    );
    let args_arg = abi::int_arg_reg_name(ctx.emitter.target, 3);
    if inst.operands.is_empty() {
        abi::emit_load_int_immediate(ctx.emitter, args_arg, 0);
    } else {
        abi::emit_temporary_stack_address(ctx.emitter, args_arg, args_offset);
    }
    abi::emit_load_int_immediate(
        ctx.emitter,
        abi::int_arg_reg_name(ctx.emitter.target, 4),
        inst.operands.len() as i64,
    );
    let out_arg = abi::int_arg_reg_name(ctx.emitter.target, 5);
    abi::emit_temporary_stack_address(ctx.emitter, out_arg, 0);
    let symbol = ctx.emitter.target.extern_symbol("__elephc_eval_new_object");
    abi::emit_call_label(ctx.emitter, &symbol);
    pop_eval_context_class_scope(ctx, pushed_class_scope);
    emit_eval_status_check_releasing(ctx, &boxed);
    emit_release_eval_boxed_operands_keeping_result(ctx, &boxed);
    let result_reg = abi::int_result_reg(ctx.emitter);
    abi::emit_load_temporary_stack_slot(ctx.emitter, result_reg, EVAL_RESULT_VALUE_CELL_OFFSET);
    abi::emit_release_temporary_stack(ctx.emitter, stack_bytes);
    store_if_result(ctx, inst)
}

/// Lowers the eval fallback of `ReflectionClass::newInstanceWithoutConstructor()`.
///
/// The class-name string sits in the 16-byte temporary slot the caller pushed, exactly as for
/// `lower_eval_object_new_dynamic_fallback`. A miss leaves that slot for the caller's fatal.
pub(in crate::codegen::lower_inst::builtins) fn lower_eval_object_new_without_constructor_fallback(
    ctx: &mut FunctionContext<'_>,
    inst: &Instruction,
    miss_label: &str,
) -> Result<()> {
    let stack_bytes = eval_function_call_stack_bytes(2);
    let eval_miss_label = ctx.next_label("eval_dynamic_new_no_ctor_missing_class");
    let done_label = ctx.next_label("eval_dynamic_new_no_ctor_done");
    let name_ptr_reg = abi::int_arg_reg_name(ctx.emitter.target, 1);
    let name_len_reg = abi::int_arg_reg_name(ctx.emitter.target, 2);
    abi::emit_load_temporary_stack_slot(ctx.emitter, name_ptr_reg, 0);
    abi::emit_load_temporary_stack_slot(ctx.emitter, name_len_reg, 8);
    abi::emit_reserve_temporary_stack(ctx.emitter, stack_bytes);
    abi::emit_store_to_sp(ctx.emitter, name_ptr_reg, EVAL_CODE_PTR_OFFSET);
    abi::emit_store_to_sp(ctx.emitter, name_len_reg, EVAL_CODE_LEN_OFFSET);
    load_eval_context_or_null(ctx)?;
    load_eval_context_to_arg(ctx, 0);
    let name_ptr_arg = abi::int_arg_reg_name(ctx.emitter.target, 1);
    abi::emit_load_temporary_stack_slot(ctx.emitter, name_ptr_arg, EVAL_CODE_PTR_OFFSET);
    let name_len_arg = abi::int_arg_reg_name(ctx.emitter.target, 2);
    abi::emit_load_temporary_stack_slot(ctx.emitter, name_len_arg, EVAL_CODE_LEN_OFFSET);
    let out_arg = abi::int_arg_reg_name(ctx.emitter.target, 3);
    abi::emit_temporary_stack_address(ctx.emitter, out_arg, 0);
    let symbol = ctx
        .emitter
        .target
        .extern_symbol("__elephc_eval_try_new_object_without_constructor");
    abi::emit_call_label(ctx.emitter, &symbol);
    emit_branch_if_eval_c_int_negative(ctx, &eval_miss_label);
    emit_eval_status_check(ctx);
    let result_reg = abi::int_result_reg(ctx.emitter);
    abi::emit_load_temporary_stack_slot(ctx.emitter, result_reg, EVAL_RESULT_VALUE_CELL_OFFSET);
    emit_eval_result_as_type(ctx, &inst.result_php_type)?;
    abi::emit_release_temporary_stack(ctx.emitter, stack_bytes);
    abi::emit_release_temporary_stack(ctx.emitter, 16);
    abi::emit_jump(ctx.emitter, &done_label);

    ctx.emitter.label(&eval_miss_label);
    abi::emit_release_temporary_stack(ctx.emitter, stack_bytes);
    abi::emit_jump(ctx.emitter, miss_label);
    ctx.emitter.label(&done_label);
    Ok(())
}

/// Lowers fallback `new $class` construction through eval dynamic metadata.
pub(in crate::codegen::lower_inst::builtins) fn lower_eval_object_new_dynamic_fallback(
    ctx: &mut FunctionContext<'_>,
    inst: &Instruction,
    miss_label: &str,
) -> Result<()> {
    let constructor_args = inst.operands.get(1..).ok_or_else(|| {
        CodegenIrError::invalid_module("eval dynamic object new missing class operand")
    })?;
    let args_offset = EVAL_STACK_BYTES;
    let site_offset = args_offset + constructor_args.len() * 8;
    let stack_bytes = eval_function_call_stack_bytes(constructor_args.len() + 5);
    // `DynamicObjectNewMixed` flags a runtime `...$args` container with `Bool(true)`: its one
    // operand is that array, which only the bridge -- knowing the class -- can bind by name.
    let spread_container = matches!(inst.immediate, Some(Immediate::Bool(true)));
    let eval_miss_label = ctx.next_label("eval_dynamic_new_missing_class");
    let done_label = ctx.next_label("eval_dynamic_new_done");
    let name_ptr_reg = abi::int_arg_reg_name(ctx.emitter.target, 1);
    let name_len_reg = abi::int_arg_reg_name(ctx.emitter.target, 2);
    abi::emit_load_temporary_stack_slot(ctx.emitter, name_ptr_reg, 0);
    abi::emit_load_temporary_stack_slot(ctx.emitter, name_len_reg, 8);
    abi::emit_reserve_temporary_stack(ctx.emitter, stack_bytes);
    abi::emit_store_to_sp(ctx.emitter, name_ptr_reg, EVAL_CODE_PTR_OFFSET);
    abi::emit_store_to_sp(ctx.emitter, name_len_reg, EVAL_CODE_LEN_OFFSET);
    load_eval_context_or_null(ctx)?;
    let boxed = store_eval_function_call_operands(ctx, constructor_args, args_offset)?;
    emit_eval_construction_site(ctx, inst, constructor_args.len(), site_offset, spread_container);
    load_eval_context_to_arg(ctx, 0);
    let name_ptr_arg = abi::int_arg_reg_name(ctx.emitter.target, 1);
    abi::emit_load_temporary_stack_slot(ctx.emitter, name_ptr_arg, EVAL_CODE_PTR_OFFSET);
    let name_len_arg = abi::int_arg_reg_name(ctx.emitter.target, 2);
    abi::emit_load_temporary_stack_slot(ctx.emitter, name_len_arg, EVAL_CODE_LEN_OFFSET);
    let args_arg = abi::int_arg_reg_name(ctx.emitter.target, 3);
    if constructor_args.is_empty() {
        abi::emit_load_int_immediate(ctx.emitter, args_arg, 0);
    } else {
        abi::emit_temporary_stack_address(ctx.emitter, args_arg, args_offset);
    }
    abi::emit_temporary_stack_address(
        ctx.emitter,
        abi::int_arg_reg_name(ctx.emitter.target, 4),
        site_offset,
    );
    let out_arg = abi::int_arg_reg_name(ctx.emitter.target, 5);
    abi::emit_temporary_stack_address(ctx.emitter, out_arg, 0);
    let symbol = ctx
        .emitter
        .target
        .extern_symbol("__elephc_eval_try_new_object_at");
    abi::emit_call_label(ctx.emitter, &symbol);
    emit_branch_if_eval_c_int_negative(ctx, &eval_miss_label);
    emit_eval_status_check_releasing(ctx, &boxed);
    emit_release_eval_boxed_operands_keeping_result(ctx, &boxed);
    let result_reg = abi::int_result_reg(ctx.emitter);
    abi::emit_load_temporary_stack_slot(ctx.emitter, result_reg, EVAL_RESULT_VALUE_CELL_OFFSET);
    // The mixed factory keeps the cell; a constrained `new static` expects the object pointer.
    emit_eval_result_as_type(ctx, &inst.result_php_type)?;
    abi::emit_release_temporary_stack(ctx.emitter, stack_bytes);
    abi::emit_release_temporary_stack(ctx.emitter, 16);
    abi::emit_jump(ctx.emitter, &done_label);

    ctx.emitter.label(&eval_miss_label);
    emit_release_eval_boxed_operands(ctx, &boxed);
    abi::emit_release_temporary_stack(ctx.emitter, stack_bytes);
    abi::emit_jump(ctx.emitter, miss_label);
    ctx.emitter.label(&done_label);
    Ok(())
}

/// Lowers a method call that may dispatch to an eval-created dynamic object.
pub(in crate::codegen::lower_inst::builtins) fn lower_eval_method_call(
    ctx: &mut FunctionContext<'_>,
    inst: &Instruction,
    object: ValueId,
    method_name: &str,
) -> Result<()> {
    let arg_count = inst.operands.len().saturating_sub(1);
    let args_offset = EVAL_STACK_BYTES;
    let stack_bytes = eval_method_call_stack_bytes(arg_count);
    abi::emit_reserve_temporary_stack(ctx.emitter, stack_bytes);
    ensure_eval_context(ctx)?;
    let pushed_class_scope = push_eval_context_class_scope(ctx)?;
    let object_ty = ctx.load_value_to_result(object)?.codegen_repr();
    let mut boxed = EvalBoxedOperands::new();
    if !matches!(object_ty, PhpType::Mixed | PhpType::Union(_)) {
        emit_box_current_value_as_mixed(ctx.emitter, &object_ty);
        boxed.push(EVAL_TEMP_CELL_OFFSET);
    }
    let result_reg = abi::int_result_reg(ctx.emitter);
    abi::emit_store_to_sp(ctx.emitter, result_reg, EVAL_TEMP_CELL_OFFSET);
    let collector_flags = eval_method_call_collector_flags(ctx, inst, object, method_name)?;
    boxed.extend(store_eval_method_call_arg_pack(ctx, inst, args_offset, collector_flags)?);
    load_eval_context_to_arg(ctx, 0);
    let object_arg = abi::int_arg_reg_name(ctx.emitter.target, 1);
    abi::emit_load_temporary_stack_slot(ctx.emitter, object_arg, EVAL_TEMP_CELL_OFFSET);
    let (method_label, method_len) = ctx.data.add_string(method_name.as_bytes());
    let method_ptr_arg = abi::int_arg_reg_name(ctx.emitter.target, 2);
    abi::emit_symbol_address(ctx.emitter, method_ptr_arg, &method_label);
    abi::emit_load_int_immediate(
        ctx.emitter,
        abi::int_arg_reg_name(ctx.emitter.target, 3),
        method_len as i64,
    );
    let pack_arg = abi::int_arg_reg_name(ctx.emitter.target, 4);
    abi::emit_temporary_stack_address(ctx.emitter, pack_arg, args_offset);
    let out_arg = abi::int_arg_reg_name(ctx.emitter.target, 5);
    abi::emit_temporary_stack_address(ctx.emitter, out_arg, 0);
    let symbol = ctx
        .emitter
        .target
        .extern_symbol("__elephc_eval_method_call");
    abi::emit_call_label(ctx.emitter, &symbol);
    pop_eval_context_class_scope(ctx, pushed_class_scope);
    emit_eval_status_check_releasing(ctx, &boxed);
    emit_release_eval_boxed_operands_keeping_result(ctx, &boxed);
    let result_reg = abi::int_result_reg(ctx.emitter);
    abi::emit_load_temporary_stack_slot(ctx.emitter, result_reg, EVAL_RESULT_VALUE_CELL_OFFSET);
    emit_eval_owned_result_as_type(ctx, &inst.result_php_type)?;
    abi::emit_release_temporary_stack(ctx.emitter, stack_bytes);
    store_if_result(ctx, inst)
}

/// Reads a property through the active eval context after boxing the receiver.
///
/// The bridge borrows the receiver cell rather than taking it, so the box this frame made is
/// still this frame's to release; a receiver already represented as Mixed is never boxed and
/// so has nothing to release.
pub(in crate::codegen::lower_inst::builtins) fn lower_eval_property_get(
    ctx: &mut FunctionContext<'_>,
    inst: &Instruction,
    object: ValueId,
    property: &str,
) -> Result<()> {
    abi::emit_reserve_temporary_stack(ctx.emitter, EVAL_STACK_BYTES);
    ensure_eval_context(ctx)?;
    let pushed_class_scope = push_eval_context_class_scope(ctx)?;
    let object_ty = ctx.load_value_to_result(object)?.codegen_repr();
    let mut boxed = EvalBoxedOperands::new();
    if !matches!(object_ty, PhpType::Mixed | PhpType::Union(_)) {
        emit_box_current_value_as_mixed(ctx.emitter, &object_ty);
        boxed.push(EVAL_TEMP_CELL_OFFSET);
    }
    let result_reg = abi::int_result_reg(ctx.emitter);
    abi::emit_store_to_sp(ctx.emitter, result_reg, EVAL_TEMP_CELL_OFFSET);
    load_eval_context_to_arg(ctx, 0);
    let object_arg = abi::int_arg_reg_name(ctx.emitter.target, 1);
    abi::emit_load_temporary_stack_slot(ctx.emitter, object_arg, EVAL_TEMP_CELL_OFFSET);
    let (property_label, property_len) = ctx.data.add_string(property.as_bytes());
    let property_ptr_arg = abi::int_arg_reg_name(ctx.emitter.target, 2);
    abi::emit_symbol_address(ctx.emitter, property_ptr_arg, &property_label);
    abi::emit_load_int_immediate(
        ctx.emitter,
        abi::int_arg_reg_name(ctx.emitter.target, 3),
        property_len as i64,
    );
    let out_arg = abi::int_arg_reg_name(ctx.emitter.target, 4);
    abi::emit_temporary_stack_address(ctx.emitter, out_arg, 0);
    let symbol = ctx
        .emitter
        .target
        .extern_symbol("__elephc_eval_property_get");
    abi::emit_call_label(ctx.emitter, &symbol);
    pop_eval_context_class_scope(ctx, pushed_class_scope);
    emit_eval_status_check_releasing(ctx, &boxed);
    emit_release_eval_boxed_operands_keeping_result(ctx, &boxed);
    let result_reg = abi::int_result_reg(ctx.emitter);
    abi::emit_load_temporary_stack_slot(ctx.emitter, result_reg, EVAL_RESULT_VALUE_CELL_OFFSET);
    emit_eval_property_result_as_borrowed(ctx, &inst.result_php_type)?;
    abi::emit_release_temporary_stack(ctx.emitter, EVAL_STACK_BYTES);
    store_if_result(ctx, inst)
}

/// Converts the bridge's fresh Mixed property cell to the concrete type of a `prop_get`.
///
/// EIR treats a concrete-typed property read as BORROWED: its consumers retain with `acquire` and
/// balance with `release`. The bridge instead hands back an owned cell, so a plain conversion left
/// the cell, the string it persisted, and the converted string all unreleased on every read (three
/// blocks per `echo $p->name` after a reflection write). The cell is always released; a string
/// is re-homed in the concat buffer (a non-heap value the consumers' acquire/release leave alone);
/// an object drops the reference the unboxing took, the property slot keeping it alive exactly as
/// it does for a native read. A Mixed result stays owned, which is what EIR expects for it.
fn emit_eval_property_result_as_borrowed(
    ctx: &mut FunctionContext<'_>,
    result_ty: &PhpType,
) -> Result<()> {
    emit_eval_owned_result_as_type(ctx, result_ty)?;
    match result_ty.codegen_repr() {
        PhpType::Str => {
            abi::emit_push_result_value(ctx.emitter, &PhpType::Str);
            crate::codegen::lower_inst::runtime_class_messages::emit_concat_static_suffix(ctx, "");
            abi::emit_push_result_value(ctx.emitter, &PhpType::Str);
            let result_reg = abi::int_result_reg(ctx.emitter);
            abi::emit_load_temporary_stack_slot(ctx.emitter, result_reg, 16);
            abi::emit_call_label(ctx.emitter, "__rt_heap_free_safe");
            let (ptr_reg, len_reg) = abi::string_result_regs(ctx.emitter);
            abi::emit_pop_reg_pair(ctx.emitter, ptr_reg, len_reg);
            abi::emit_release_temporary_stack(ctx.emitter, 16);
        }
        PhpType::Object(_) => {
            let result_reg = abi::int_result_reg(ctx.emitter);
            abi::emit_push_reg(ctx.emitter, result_reg);
            abi::emit_decref_if_refcounted(ctx.emitter, &PhpType::Object(String::new()));
            abi::emit_pop_reg(ctx.emitter, result_reg);
        }
        _ => {}
    }
    Ok(())
}

/// Probes a statically typed receiver for eval ownership before reading its property.
///
/// Native and eval-created objects deliberately keep their property state in different
/// representations. A module may use the eval bridge for one unrelated expression, so the
/// presence of that bridge alone is not evidence that a particular native receiver must be
/// read through its eval overlay. Only an object registered to an eval context takes the
/// bridge; every other receiver continues through its native property slot.
pub(in crate::codegen::lower_inst) fn lower_eval_owned_property_get(
    ctx: &mut FunctionContext<'_>,
    inst: &Instruction,
    object: ValueId,
    property: &str,
    miss_label: &str,
    done_label: &str,
) -> Result<()> {
    let object_ty = ctx.load_value_to_result(object)?.codegen_repr();
    if matches!(object_ty, PhpType::Mixed | PhpType::Union(_)) {
        abi::emit_call_label(ctx.emitter, "__rt_mixed_unbox");
        match ctx.emitter.target.arch {
            Arch::AArch64 => {
                ctx.emitter.instruction("cmp x0, #6"); // only object payloads can have an eval owner
                ctx.emitter.instruction(&format!("b.ne {}", miss_label));
                ctx.emitter.instruction("mov x0, x1"); // pass the raw object identity to the owner probe
            }
            Arch::X86_64 => {
                ctx.emitter.instruction("cmp rax, 6"); // only object payloads can have an eval owner
                ctx.emitter.instruction(&format!("jne {}", miss_label));
                // The unboxed object identity already occupies the first C argument register.
            }
        }
    } else {
        abi::emit_reg_move(
            ctx.emitter,
            abi::int_arg_reg_name(ctx.emitter.target, 0),
            abi::int_result_reg(ctx.emitter),
        );
    }
    abi::emit_call_label(ctx.emitter, "__rt_eval_object_has_owner");
    abi::emit_branch_if_int_result_zero(ctx.emitter, miss_label);
    lower_eval_property_get(ctx, inst, object, property)?;
    abi::emit_jump(ctx.emitter, done_label);
    Ok(())
}

/// Reads a property of an eval-owned object reached through a `Mixed` receiver, in a frame that
/// has no eval context of its own, leaving the owned `Mixed` result in the result register.
///
/// A property an eval-declared class declares lives in its owning context, not in a native slot
/// nor in a stdClass hash, so the native reads answered null for it: compiled
/// `$provider->singletons` on an eval subclass of a compiled class read nothing. Objects no eval
/// context owns branch to `miss_label` untouched. The bridge finds the owning context from the
/// object itself, so this frame passes no context.
pub(in crate::codegen::lower_inst) fn lower_eval_owner_mixed_property_get(
    ctx: &mut FunctionContext<'_>,
    object: ValueId,
    property: &str,
    miss_label: &str,
    done_label: &str,
) -> Result<()> {
    lower_eval_owner_mixed_property_get_named(
        ctx,
        object,
        OwnerPropertyName::Literal(property),
        miss_label,
        done_label,
    )
}

/// Same read for a property named by a RUNTIME string (`$object->{$name}`).
///
/// Laravel's `DelegatesToResource::__get($key)` is `$this->resource->{$key}`; the runtime-name
/// read answered null for an eval-declared model, so every API resource attribute was empty.
pub(in crate::codegen::lower_inst) fn lower_eval_owner_mixed_dynamic_property_get(
    ctx: &mut FunctionContext<'_>,
    object: ValueId,
    property_value: ValueId,
    miss_label: &str,
    done_label: &str,
) -> Result<()> {
    lower_eval_owner_mixed_property_get_named(
        ctx,
        object,
        OwnerPropertyName::Runtime(property_value),
        miss_label,
        done_label,
    )
}

/// How the owner property read names its property.
#[derive(Clone, Copy)]
enum OwnerPropertyName<'a> {
    Literal(&'a str),
    Runtime(ValueId),
}

fn lower_eval_owner_mixed_property_get_named(
    ctx: &mut FunctionContext<'_>,
    object: ValueId,
    name: OwnerPropertyName<'_>,
    miss_label: &str,
    done_label: &str,
) -> Result<()> {
    ctx.load_value_to_result(object)?;
    abi::emit_call_label(ctx.emitter, "__rt_mixed_unbox");
    match ctx.emitter.target.arch {
        Arch::AArch64 => {
            ctx.emitter.instruction("cmp x0, #6"); // only object payloads can have an eval owner
            ctx.emitter.instruction(&format!("b.ne {}", miss_label));
            ctx.emitter.instruction("mov x0, x1"); // pass the raw object identity to the owner probe
        }
        Arch::X86_64 => {
            ctx.emitter.instruction("cmp rax, 6"); // only object payloads can have an eval owner
            ctx.emitter.instruction(&format!("jne {}", miss_label));
            // The unboxed object identity already occupies the first C argument register.
        }
    }
    let owner_probe = ctx
        .emitter
        .target
        .extern_symbol("__elephc_eval_object_has_dynamic_owner");
    abi::emit_call_label(ctx.emitter, &owner_probe);
    abi::emit_branch_if_int_result_zero(ctx.emitter, miss_label);

    abi::emit_reserve_temporary_stack(ctx.emitter, EVAL_STACK_BYTES);
    // The bridge gets a NULL caller context, so it reads the calling scope from the published
    // lexical class: `ReflectionProperty::getValue()` reads a private property this way, and
    // without a scope the interpreter refused it "Cannot access private property".
    let pushed_caller_class = push_native_caller_class(ctx)?;
    let object_arg = abi::int_arg_reg_name(ctx.emitter.target, 1);
    ctx.load_value_to_reg(object, object_arg)?;
    let property_ptr_arg = abi::int_arg_reg_name(ctx.emitter.target, 2);
    let property_len_arg = abi::int_arg_reg_name(ctx.emitter.target, 3);
    match name {
        OwnerPropertyName::Literal(property) => {
            let (property_label, property_len) = ctx.data.add_string(property.as_bytes());
            abi::emit_symbol_address(ctx.emitter, property_ptr_arg, &property_label);
            abi::emit_load_int_immediate(ctx.emitter, property_len_arg, property_len as i64);
        }
        OwnerPropertyName::Runtime(value) => {
            ctx.load_string_value_to_regs(value, property_ptr_arg, property_len_arg)?;
        }
    }
    let out_arg = abi::int_arg_reg_name(ctx.emitter.target, 4);
    abi::emit_temporary_stack_address(ctx.emitter, out_arg, 0);
    abi::emit_load_int_immediate(ctx.emitter, abi::int_arg_reg_name(ctx.emitter.target, 0), 0);
    let symbol = ctx
        .emitter
        .target
        .extern_symbol("__elephc_eval_property_get");
    abi::emit_call_label(ctx.emitter, &symbol);
    pop_native_caller_class(ctx, pushed_caller_class);
    emit_eval_status_check(ctx);
    let result_reg = abi::int_result_reg(ctx.emitter);
    abi::emit_load_temporary_stack_slot(ctx.emitter, result_reg, EVAL_RESULT_VALUE_CELL_OFFSET);
    abi::emit_release_temporary_stack(ctx.emitter, EVAL_STACK_BYTES);
    abi::emit_jump(ctx.emitter, done_label);
    Ok(())
}

/// Probes a statically typed method receiver for an eval-owned dynamic class.
///
/// The caller supplies native fallback and completion labels. A negative probe
/// status reaches `miss_label`; a dynamic-owner hit publishes the converted
/// result, stores it, and jumps to `done_label`.
pub(in crate::codegen::lower_inst) fn lower_eval_owned_method_call(
    ctx: &mut FunctionContext<'_>,
    inst: &Instruction,
    object: ValueId,
    method_name: &str,
    miss_label: &str,
    done_label: &str,
) -> Result<()> {
    let result_type = inst.result_php_type.clone();
    lower_eval_owned_method_call_with_result(
        ctx,
        inst,
        object,
        method_name,
        miss_label,
        done_label,
        &result_type,
        true,
    )
}

/// [`lower_eval_owned_method_call`] with the result type chosen by the caller and, optionally,
/// left in the integer result register instead of being stored into the instruction's value.
///
/// `ArrayAccess` probes need it: the `offsetExists()` answer is a bool the surrounding lowering
/// branches on, while the instruction's own value is the `offsetGet()` element.
pub(in crate::codegen::lower_inst) fn lower_eval_owned_method_call_with_result(
    ctx: &mut FunctionContext<'_>,
    inst: &Instruction,
    object: ValueId,
    method_name: &str,
    miss_label: &str,
    done_label: &str,
    result_type: &PhpType,
    store_result: bool,
) -> Result<()> {
    // Native receivers need neither boxed arguments nor reference markers. Look up
    // dynamic ownership before preparing that pack, including on the fallback path.
    let object_ty = ctx.load_value_to_result(object)?.codegen_repr();
    if matches!(object_ty, PhpType::Mixed | PhpType::Union(_)) {
        abi::emit_call_label(ctx.emitter, "__rt_mixed_unbox");
        match ctx.emitter.target.arch {
            Arch::AArch64 => {
                ctx.emitter.instruction("cmp x0, #6"); // only object payloads can have a dynamic owner
                ctx.emitter.instruction(&format!("b.ne {}", miss_label)); // preserve native diagnostics for other tags
                ctx.emitter.instruction("mov x0, x1"); // raw identity becomes the C ABI argument
            }
            Arch::X86_64 => {
                ctx.emitter.instruction("cmp rax, 6"); // only object payloads can have a dynamic owner
                ctx.emitter.instruction(&format!("jne {}", miss_label)); // preserve native diagnostics for other tags
                // The unboxed object identity is already in the first C argument register, rdi.
            }
        }
    } else {
        abi::emit_reg_move(ctx.emitter,
            abi::int_arg_reg_name(ctx.emitter.target, 0),
            abi::int_result_reg(ctx.emitter));
    }
    abi::emit_call_label(ctx.emitter, "__rt_eval_object_has_owner");
    abi::emit_branch_if_int_result_zero(ctx.emitter, miss_label);
    if super::outlined_owner_call::try_lower_outlined_owner_call(
        ctx,
        inst,
        object,
        method_name,
        miss_label,
        done_label,
        result_type,
        store_result,
    )? {
        return Ok(());
    }
    emit_owned_method_slow_path(
        ctx,
        inst,
        object,
        method_name,
        miss_label,
        done_label,
        result_type,
        store_result,
        false,
    )
}

/// The part of an owned-receiver method call that runs only for a class the interpreter declared:
/// scope publication, boxing the receiver and the arguments, the bridge call, the result
/// conversion and every release.
///
/// `in_shared_helper` is set when this body is the shared `eval_owner_call` helper: a hit then
/// clears `_eval_owner_miss` before jumping to `done_label`, and the caller's `miss_label` is
/// responsible for raising it.
pub(super) fn emit_owned_method_slow_path(
    ctx: &mut FunctionContext<'_>,
    inst: &Instruction,
    object: ValueId,
    method_name: &str,
    miss_label: &str,
    done_label: &str,
    result_type: &PhpType,
    store_result: bool,
    in_shared_helper: bool,
) -> Result<()> {
    let arg_count = inst.operands.len().saturating_sub(1);
    let args_offset = EVAL_STACK_BYTES;
    let stack_bytes = eval_method_call_stack_bytes(arg_count);
    let miss_stack_label = ctx.next_label("eval_owned_method_miss");
    abi::emit_reserve_temporary_stack(ctx.emitter, stack_bytes);
    // The owner context decides DISPATCH; this frame still decides SCOPE. Handing the bridge a
    // null caller context left `current_class_scope()` empty, so a protected method reached from
    // a compiled body -- `Twig\Template::yield()` calling `$this->doDisplay()` -- was refused
    // with "from global scope". `__elephc_eval_method_call` forwards a non-null caller's scopes
    // into the owner context, which is exactly what this path was missing.
    //
    // A body with no class of its own and no eval context local -- an injected prelude function,
    // say -- has no scope to forward and keeps the null caller it always passed.
    let forwards_scope = current_eval_method_class(ctx).is_some() && has_eval_context(ctx);
    let pushed_class_scope = if forwards_scope {
        ensure_eval_context(ctx)?;
        push_eval_context_class_scope(ctx)?
    } else {
        false
    };
    // No context to forward from, but this frame still has a lexical class, and that class is
    // the calling scope. Publish it directly.
    let pushed_caller_class = if forwards_scope {
        false
    } else if in_shared_helper {
        super::outlined_owner_call::push_site_caller_class(ctx)
    } else {
        push_native_caller_class(ctx)?
    };
    let object_ty = ctx.load_value_to_result(object)?.codegen_repr();
    let mut boxed = EvalBoxedOperands::new();
    if !matches!(object_ty, PhpType::Mixed | PhpType::Union(_)) {
        emit_box_current_value_as_mixed(ctx.emitter, &object_ty);
        boxed.push(EVAL_TEMP_CELL_OFFSET);
    }
    let result_reg = abi::int_result_reg(ctx.emitter);
    abi::emit_store_to_sp(ctx.emitter, result_reg, EVAL_TEMP_CELL_OFFSET);
    let collector_flags = if in_shared_helper {
        ctx.shared
            .owner_call_site_facts
            .get(&ctx.function.name)
            .map_or(0, |(flags, _)| *flags)
    } else {
        eval_method_call_collector_flags(ctx, inst, object, method_name)?
    };
    boxed.extend(store_eval_method_call_arg_pack(ctx, inst, args_offset, collector_flags)?);
    if forwards_scope {
        load_eval_context_to_arg(ctx, 0);
    } else {
        abi::emit_load_int_immediate(
            ctx.emitter,
            abi::int_arg_reg_name(ctx.emitter.target, 0),
            0,
        );
    }
    let object_arg = abi::int_arg_reg_name(ctx.emitter.target, 1);
    abi::emit_load_temporary_stack_slot(ctx.emitter, object_arg, EVAL_TEMP_CELL_OFFSET);
    let (method_label, method_len) = ctx.data.add_string(method_name.as_bytes());
    let method_ptr_arg = abi::int_arg_reg_name(ctx.emitter.target, 2);
    abi::emit_symbol_address(ctx.emitter, method_ptr_arg, &method_label);
    abi::emit_load_int_immediate(
        ctx.emitter,
        abi::int_arg_reg_name(ctx.emitter.target, 3),
        method_len as i64,
    );
    let pack_arg = abi::int_arg_reg_name(ctx.emitter.target, 4);
    abi::emit_temporary_stack_address(ctx.emitter, pack_arg, args_offset);
    let out_arg = abi::int_arg_reg_name(ctx.emitter.target, 5);
    abi::emit_temporary_stack_address(ctx.emitter, out_arg, 0);
    let symbol = ctx
        .emitter
        .target
        .extern_symbol("__elephc_eval_method_call");
    abi::emit_call_label(ctx.emitter, &symbol);
    pop_eval_context_class_scope(ctx, pushed_class_scope);
    pop_native_caller_class(ctx, pushed_caller_class);
    emit_branch_if_eval_c_int_negative(ctx, &miss_stack_label);
    emit_eval_status_check_releasing(ctx, &boxed);
    emit_release_eval_boxed_operands_keeping_result(ctx, &boxed);
    let result_reg = abi::int_result_reg(ctx.emitter);
    abi::emit_load_temporary_stack_slot(ctx.emitter, result_reg, EVAL_RESULT_VALUE_CELL_OFFSET);
    emit_eval_owned_result_as_type(ctx, result_type)?;
    abi::emit_release_temporary_stack(ctx.emitter, stack_bytes);
    if store_result {
        store_if_result(ctx, inst)?;
    }
    if in_shared_helper {
        super::outlined_owner_call::emit_set_owner_miss_flag(ctx, 0);
    }
    abi::emit_jump(ctx.emitter, done_label);

    ctx.emitter.label(&miss_stack_label);
    emit_release_eval_boxed_operands(ctx, &boxed);
    abi::emit_release_temporary_stack(ctx.emitter, stack_bytes);
    abi::emit_jump(ctx.emitter, miss_label);
    Ok(())
}

/// Lowers a native static-method call to an eval-declared dynamic class.
pub(in crate::codegen::lower_inst::builtins) fn lower_eval_static_method_call(
    ctx: &mut FunctionContext<'_>,
    inst: &Instruction,
    class_name: &str,
    method_name: &str,
) -> Result<()> {
    let args_offset = EVAL_STACK_BYTES;
    let stack_bytes = eval_static_method_call_stack_bytes(inst.operands.len());
    abi::emit_reserve_temporary_stack(ctx.emitter, stack_bytes);
    ensure_eval_context(ctx)?;
    let pushed_class_scope = push_eval_context_class_scope(ctx)?;
    let collector_flags =
        eval_static_method_call_collector_flags(ctx, inst, class_name, method_name);
    let boxed = store_eval_static_method_call_arg_pack(ctx, inst, args_offset, collector_flags)?;
    load_eval_context_to_arg(ctx, 0);
    let target = format!("{}::{}", class_name, method_name);
    let (target_label, target_len) = ctx.data.add_string(target.as_bytes());
    let target_arg = abi::int_arg_reg_name(ctx.emitter.target, 1);
    abi::emit_symbol_address(ctx.emitter, target_arg, &target_label);
    abi::emit_load_int_immediate(
        ctx.emitter,
        abi::int_arg_reg_name(ctx.emitter.target, 2),
        target_len as i64,
    );
    let pack_arg = abi::int_arg_reg_name(ctx.emitter.target, 3);
    abi::emit_temporary_stack_address(ctx.emitter, pack_arg, args_offset);
    let out_arg = abi::int_arg_reg_name(ctx.emitter.target, 4);
    abi::emit_temporary_stack_address(ctx.emitter, out_arg, 0);
    let symbol = ctx
        .emitter
        .target
        .extern_symbol("__elephc_eval_static_method_call");
    abi::emit_call_label(ctx.emitter, &symbol);
    pop_eval_context_class_scope(ctx, pushed_class_scope);
    emit_eval_status_check_releasing(ctx, &boxed);
    emit_release_eval_boxed_operands_keeping_result(ctx, &boxed);
    let result_reg = abi::int_result_reg(ctx.emitter);
    abi::emit_load_temporary_stack_slot(ctx.emitter, result_reg, EVAL_RESULT_VALUE_CELL_OFFSET);
    emit_eval_owned_result_as_type(ctx, &inst.result_php_type)?;
    abi::emit_release_temporary_stack(ctx.emitter, stack_bytes);
    store_if_result(ctx, inst)
}

/// Lowers a late-static AOT-frame static method call through an active eval override.
pub(in crate::codegen::lower_inst::builtins) fn lower_eval_native_frame_static_method_call(
    ctx: &mut FunctionContext<'_>,
    inst: &Instruction,
    frame_class: &str,
    method_name: &str,
    no_override_label: &str,
    done_label: &str,
) -> Result<()> {
    let args_offset = EVAL_STACK_BYTES;
    let stack_bytes = eval_static_method_call_stack_bytes(inst.operands.len());
    // The override probe answers before any operand is boxed, so it needs an exit of its own:
    // releasing cells at the shared miss label would decref whatever the uninitialized scratch
    // slots happened to hold.
    let no_probe_label = ctx.next_label("eval_native_frame_static_method_no_probe");
    let miss_stack_label = ctx.next_label("eval_native_frame_static_method_miss");
    abi::emit_reserve_temporary_stack(ctx.emitter, stack_bytes);
    emit_eval_native_frame_override_probe(ctx, frame_class, &no_probe_label);
    let collector_flags =
        eval_static_method_call_collector_flags(ctx, inst, frame_class, method_name);
    let boxed = store_eval_static_method_call_arg_pack(ctx, inst, args_offset, collector_flags)?;
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
    let (method_label, method_len) = ctx.data.add_string(method_name.as_bytes());
    abi::emit_symbol_address(
        ctx.emitter,
        abi::int_arg_reg_name(ctx.emitter.target, 2),
        &method_label,
    );
    abi::emit_load_int_immediate(
        ctx.emitter,
        abi::int_arg_reg_name(ctx.emitter.target, 3),
        method_len as i64,
    );
    let pack_arg = abi::int_arg_reg_name(ctx.emitter.target, 4);
    abi::emit_temporary_stack_address(ctx.emitter, pack_arg, args_offset);
    let out_arg = abi::int_arg_reg_name(ctx.emitter.target, 5);
    abi::emit_temporary_stack_address(ctx.emitter, out_arg, 0);
    let symbol = ctx
        .emitter
        .target
        .extern_symbol("__elephc_eval_native_frame_static_method_call");
    abi::emit_call_label(ctx.emitter, &symbol);
    emit_branch_if_eval_c_int_negative(ctx, &miss_stack_label);
    emit_eval_status_check_releasing(ctx, &boxed);
    emit_release_eval_boxed_operands_keeping_result(ctx, &boxed);
    let result_reg = abi::int_result_reg(ctx.emitter);
    abi::emit_load_temporary_stack_slot(ctx.emitter, result_reg, EVAL_RESULT_VALUE_CELL_OFFSET);
    emit_eval_owned_result_as_type(ctx, &inst.result_php_type)?;
    abi::emit_release_temporary_stack(ctx.emitter, stack_bytes);
    store_if_result(ctx, inst)?;
    abi::emit_jump(ctx.emitter, done_label);

    ctx.emitter.label(&miss_stack_label);
    emit_release_eval_boxed_operands(ctx, &boxed);
    ctx.emitter.label(&no_probe_label);
    abi::emit_release_temporary_stack(ctx.emitter, stack_bytes);
    abi::emit_jump(ctx.emitter, no_override_label);
    Ok(())
}

/// Lowers a late-static AOT-frame static-property read through an active eval override.
pub(in crate::codegen::lower_inst::builtins) fn lower_eval_native_frame_static_property_get(
    ctx: &mut FunctionContext<'_>,
    inst: &Instruction,
    frame_class: &str,
    property_name: &str,
    no_override_label: &str,
    done_label: &str,
) -> Result<()> {
    let miss_stack_label = ctx.next_label("eval_native_frame_static_prop_get_miss");
    abi::emit_reserve_temporary_stack(ctx.emitter, EVAL_STACK_BYTES);
    emit_eval_native_frame_override_probe(ctx, frame_class, &miss_stack_label);
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
    let (property_label, property_len) = ctx.data.add_string(property_name.as_bytes());
    abi::emit_symbol_address(
        ctx.emitter,
        abi::int_arg_reg_name(ctx.emitter.target, 2),
        &property_label,
    );
    abi::emit_load_int_immediate(
        ctx.emitter,
        abi::int_arg_reg_name(ctx.emitter.target, 3),
        property_len as i64,
    );
    let out_arg = abi::int_arg_reg_name(ctx.emitter.target, 4);
    abi::emit_temporary_stack_address(ctx.emitter, out_arg, 0);
    let symbol = ctx
        .emitter
        .target
        .extern_symbol("__elephc_eval_native_frame_static_property_get");
    abi::emit_call_label(ctx.emitter, &symbol);
    emit_branch_if_eval_c_int_negative(ctx, &miss_stack_label);
    emit_eval_status_check(ctx);
    let result_reg = abi::int_result_reg(ctx.emitter);
    abi::emit_load_temporary_stack_slot(ctx.emitter, result_reg, EVAL_RESULT_VALUE_CELL_OFFSET);
    emit_eval_result_as_type(ctx, &inst.result_php_type)?;
    abi::emit_release_temporary_stack(ctx.emitter, EVAL_STACK_BYTES);
    store_if_result(ctx, inst)?;
    abi::emit_jump(ctx.emitter, done_label);

    ctx.emitter.label(&miss_stack_label);
    abi::emit_release_temporary_stack(ctx.emitter, EVAL_STACK_BYTES);
    abi::emit_jump(ctx.emitter, no_override_label);
    Ok(())
}

/// Lowers a late-static AOT-frame static-property write through an active eval override.
pub(in crate::codegen::lower_inst::builtins) fn lower_eval_native_frame_static_property_set(
    ctx: &mut FunctionContext<'_>,
    _inst: &Instruction,
    value: ValueId,
    frame_class: &str,
    property_name: &str,
    no_override_label: &str,
    done_label: &str,
) -> Result<()> {
    // A property write hands the boxed value to the store, which keeps it; unlike a call's
    // borrowed arguments it is not this frame's to release afterwards.
    let miss_stack_label = ctx.next_label("eval_native_frame_static_prop_set_miss");
    abi::emit_reserve_temporary_stack(ctx.emitter, EVAL_STACK_BYTES);
    emit_eval_native_frame_override_probe(ctx, frame_class, &miss_stack_label);
    store_eval_mixed_operand_at(ctx, value, EVAL_TEMP_CELL_OFFSET)?;
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
    let (property_label, property_len) = ctx.data.add_string(property_name.as_bytes());
    abi::emit_symbol_address(
        ctx.emitter,
        abi::int_arg_reg_name(ctx.emitter.target, 2),
        &property_label,
    );
    abi::emit_load_int_immediate(
        ctx.emitter,
        abi::int_arg_reg_name(ctx.emitter.target, 3),
        property_len as i64,
    );
    let value_arg = abi::int_arg_reg_name(ctx.emitter.target, 4);
    abi::emit_load_temporary_stack_slot(ctx.emitter, value_arg, EVAL_TEMP_CELL_OFFSET);
    let out_arg = abi::int_arg_reg_name(ctx.emitter.target, 5);
    abi::emit_temporary_stack_address(ctx.emitter, out_arg, 0);
    let symbol = ctx
        .emitter
        .target
        .extern_symbol("__elephc_eval_native_frame_static_property_set");
    abi::emit_call_label(ctx.emitter, &symbol);
    emit_branch_if_eval_c_int_negative(ctx, &miss_stack_label);
    emit_eval_status_check(ctx);
    abi::emit_release_temporary_stack(ctx.emitter, EVAL_STACK_BYTES);
    abi::emit_jump(ctx.emitter, done_label);

    ctx.emitter.label(&miss_stack_label);
    abi::emit_release_temporary_stack(ctx.emitter, EVAL_STACK_BYTES);
    abi::emit_jump(ctx.emitter, no_override_label);
    Ok(())
}
