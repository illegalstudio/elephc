//! Purpose:
//! Lowers Fiber instance methods, argument staging, and state predicates.
//!
//! Called from:
//! - `crate::codegen::lower_inst::lower_instruction()` and sibling lowering helpers.
//!
//! Key details:
//! - Preserves EIR ownership, ABI ordering, runtime symbols, and target-aware lowering.

use super::*;

/// Lowers `Fiber::start(...)` by copying boxed start arguments into the Fiber object.
pub(super) fn lower_fiber_start(
    ctx: &mut FunctionContext<'_>,
    inst: &Instruction,
    object: ValueId,
) -> Result<()> {
    let args = fiber_start_visible_args(ctx, inst)?;
    if args.len() > runtime::FIBER_START_ARGS_MAX as usize {
        return Err(CodegenIrError::unsupported(
            "Fiber::start with more than seven EIR arguments",
        ));
    }
    let param_types = vec![PhpType::Mixed; args.len()];
    let assignments =
        abi::build_outgoing_arg_assignments_for_target(ctx.emitter.target, &param_types, 1);
    for value in &args {
        ctx.load_value_to_result(*value)?;
        let source_ty = ctx.raw_value_php_type(*value)?;
        let push_ty = materialize_direct_call_arg_for_param(ctx, &source_ty, &PhpType::Mixed)?;
        abi::emit_push_result_value(ctx.emitter, &push_ty);
    }
    let overflow_bytes = abi::materialize_outgoing_args(ctx.emitter, &assignments);
    let receiver_arg = abi::int_arg_reg_name(ctx.emitter.target, 0);
    ctx.load_value_to_reg(object, receiver_arg)?;
    emit_store_fiber_start_args(ctx, &assignments, args.len())?;
    abi::emit_call_label(ctx.emitter, "__rt_fiber_start");
    abi::emit_release_temporary_stack(ctx.emitter, overflow_bytes);
    store_if_result(ctx, inst)
}

/// Lowers `Fiber::resume($value = null)` through the shared runtime helper.
pub(super) fn lower_fiber_resume(
    ctx: &mut FunctionContext<'_>,
    inst: &Instruction,
    object: ValueId,
) -> Result<()> {
    let value =
        fiber_single_optional_arg(ctx, inst.operands.get(1..).unwrap_or(&[]), "Fiber::resume")?;
    emit_optional_mixed_arg(ctx, value)?;
    abi::emit_push_reg(ctx.emitter, abi::int_result_reg(ctx.emitter)); // preserve the boxed resume value while loading the receiver
    let receiver_arg = abi::int_arg_reg_name(ctx.emitter.target, 0);
    ctx.load_value_to_reg(object, receiver_arg)?;
    abi::emit_pop_reg(ctx.emitter, abi::int_arg_reg_name(ctx.emitter.target, 1)); // pass the boxed resume value as runtime helper argument 2
    abi::emit_call_label(ctx.emitter, "__rt_fiber_resume");
    store_if_result(ctx, inst)
}

/// Lowers `Fiber::throw(Throwable $exception)` through the shared runtime helper.
pub(super) fn lower_fiber_throw(
    ctx: &mut FunctionContext<'_>,
    inst: &Instruction,
    object: ValueId,
) -> Result<()> {
    let args = fiber_visible_args(ctx, inst.operands.get(1..).unwrap_or(&[]), "Fiber::throw")?;
    if args.len() != 1 {
        return Err(CodegenIrError::unsupported(
            "Fiber::throw without exactly one EIR argument",
        ));
    }
    let thrown = args[0];
    let thrown_ty = ctx.load_value_to_result(thrown)?;
    if !matches!(thrown_ty.codegen_repr(), PhpType::Object(_)) {
        return Err(CodegenIrError::unsupported(format!(
            "Fiber::throw argument PHP type {:?}",
            thrown_ty
        )));
    }
    abi::emit_push_reg(ctx.emitter, abi::int_result_reg(ctx.emitter)); // preserve the Throwable while loading the Fiber receiver
    ctx.load_value_to_reg(object, abi::int_arg_reg_name(ctx.emitter.target, 0))?;
    abi::emit_pop_reg(ctx.emitter, abi::int_arg_reg_name(ctx.emitter.target, 1)); // pass the Throwable object as runtime helper argument 2
    abi::emit_call_label(ctx.emitter, "__rt_fiber_throw");
    store_if_result(ctx, inst)
}

/// Copies materialized `Fiber::start` arguments into the runtime Fiber start-arg buffer.
pub(super) fn emit_store_fiber_start_args(
    ctx: &mut FunctionContext<'_>,
    assignments: &[abi::OutgoingArgAssignment],
    supplied_arg_count: usize,
) -> Result<()> {
    match ctx.emitter.target.arch {
        Arch::AArch64 => emit_store_fiber_start_args_aarch64(ctx, assignments, supplied_arg_count),
        Arch::X86_64 => {
            emit_store_fiber_start_args_x86_64(ctx, assignments, supplied_arg_count);
            Ok(())
        }
    }
}

/// Copies register-passed ARM64 start arguments into `Fiber::start_args`.
pub(super) fn emit_store_fiber_start_args_aarch64(
    ctx: &mut FunctionContext<'_>,
    assignments: &[abi::OutgoingArgAssignment],
    supplied_arg_count: usize,
) -> Result<()> {
    let skip_label = ctx.next_label("fiber_start_args_done");
    ctx.emitter.instruction(&format!(
        "ldr x9, [x0, #{}]",
        runtime::FIBER_USER_ARG_MAX_OFFSET
    ));                                                                         // x9 = writable Fiber start_args slot count
    for (idx, assignment) in assignments.iter().take(supplied_arg_count).enumerate() {
        if !assignment.in_register() {
            return Err(CodegenIrError::unsupported(
                "Fiber::start ARM64 stack-passed EIR arguments",
            ));
        }
        let source_reg = abi::int_arg_reg_name(ctx.emitter.target, assignment.start_reg);
        let offset = runtime::FIBER_START_ARGS_OFFSET + (idx as i32) * 8;
        ctx.emitter.instruction(&format!("cmp x9, #{}", idx + 1));              // is this start() slot allowed for user arguments?
        ctx.emitter.instruction(&format!("b.lt {}", skip_label));               // stop once wrapper-reserved slots would be overwritten
        ctx.emitter
            .instruction(&format!("str {}, [x0, #{}]", source_reg, offset)); // store the boxed Mixed start() argument
    }
    ctx.emitter.label(&skip_label);
    ctx.emitter
        .instruction(&format!("mov x9, #{}", supplied_arg_count)); // materialize the visible start() argument count
    ctx.emitter.instruction(&format!(
        "str x9, [x0, #{}]",
        runtime::FIBER_START_ARG_COUNT_OFFSET
    ));                                                                         // publish start() arity for Fiber wrappers
    Ok(())
}

/// Copies SysV x86_64 register and stack-passed start arguments into `Fiber::start_args`.
pub(super) fn emit_store_fiber_start_args_x86_64(
    ctx: &mut FunctionContext<'_>,
    assignments: &[abi::OutgoingArgAssignment],
    supplied_arg_count: usize,
) {
    let skip_label = ctx.next_label("fiber_start_args_done");
    ctx.emitter.instruction(&format!(
        "mov r11, QWORD PTR [rdi + {}]",
        runtime::FIBER_USER_ARG_MAX_OFFSET
    ));                                                                         // r11 = writable Fiber start_args slot count
    let mut overflow_slot = 0usize;
    for (idx, assignment) in assignments.iter().take(supplied_arg_count).enumerate() {
        let offset = runtime::FIBER_START_ARGS_OFFSET + (idx as i32) * 8;
        ctx.emitter.instruction(&format!("cmp r11, {}", idx + 1));              // is this start() slot allowed for user arguments?
        ctx.emitter.instruction(&format!("jl {}", skip_label));                 // stop once wrapper-reserved slots would be overwritten
        if assignment.in_register() {
            let source_reg = abi::int_arg_reg_name(ctx.emitter.target, assignment.start_reg);
            ctx.emitter
                .instruction(&format!("mov QWORD PTR [rdi + {}], {}", offset, source_reg));
        // store the boxed Mixed register argument
        } else {
            let stack_offset = overflow_slot * 16;
            if stack_offset == 0 {
                ctx.emitter.instruction("mov r10, QWORD PTR [rsp]");            // load the first stack-passed boxed Mixed start() argument
            } else {
                ctx.emitter
                    .instruction(&format!("mov r10, QWORD PTR [rsp + {}]", stack_offset));
                // load this stack-passed boxed Mixed start() argument
            }
            ctx.emitter
                .instruction(&format!("mov QWORD PTR [rdi + {}], r10", offset)); // store the boxed Mixed stack argument
            overflow_slot += 1;
        }
    }
    ctx.emitter.label(&skip_label);
    ctx.emitter.instruction(&format!(
        "mov QWORD PTR [rdi + {}], {}",
        runtime::FIBER_START_ARG_COUNT_OFFSET,
        supplied_arg_count
    ));                                                                         // publish start() arity for Fiber wrappers
}

/// Lowers no-argument Fiber instance methods that delegate to one runtime helper.
pub(super) fn lower_fiber_noarg_runtime_method(
    ctx: &mut FunctionContext<'_>,
    inst: &Instruction,
    object: ValueId,
    helper: &str,
) -> Result<()> {
    if inst.operands.len() != 1 {
        return Err(CodegenIrError::unsupported(format!(
            "Fiber runtime method {} with EIR arguments",
            helper
        )));
    }
    let receiver_arg = abi::int_arg_reg_name(ctx.emitter.target, 0);
    ctx.load_value_to_reg(object, receiver_arg)?;
    abi::emit_call_label(ctx.emitter, helper);
    store_if_result(ctx, inst)
}

/// Returns the visible `Fiber::start(...)` operands before synthetic default padding.
pub(super) fn fiber_start_visible_args(ctx: &FunctionContext<'_>, inst: &Instruction) -> Result<Vec<ValueId>> {
    fiber_visible_args(ctx, inst.operands.get(1..).unwrap_or(&[]), "Fiber::start")
}

/// Returns at most one visible Fiber runtime argument after default padding.
pub(super) fn fiber_single_optional_arg(
    ctx: &FunctionContext<'_>,
    operands: &[ValueId],
    context: &str,
) -> Result<Option<ValueId>> {
    let args = fiber_visible_args(ctx, operands, context)?;
    if args.len() > 1 {
        return Err(CodegenIrError::unsupported(format!(
            "{} with more than one EIR argument",
            context
        )));
    }
    Ok(args.first().copied())
}

/// Returns visible Fiber operands before synthetic default padding.
pub(super) fn fiber_visible_args(
    ctx: &FunctionContext<'_>,
    operands: &[ValueId],
    context: &str,
) -> Result<Vec<ValueId>> {
    let mut args = Vec::new();
    let mut saw_default_padding = false;
    for operand in operands {
        if is_synthetic_null_value(ctx, *operand)? {
            saw_default_padding = true;
            continue;
        }
        if saw_default_padding {
            return Err(CodegenIrError::unsupported(format!(
                "{} with non-trailing EIR default arguments",
                context
            )));
        }
        args.push(*operand);
    }
    Ok(args)
}

/// Leaves a boxed Mixed value in the integer result register, using null when omitted.
pub(super) fn emit_optional_mixed_arg(ctx: &mut FunctionContext<'_>, value: Option<ValueId>) -> Result<()> {
    if let Some(value) = value {
        ctx.load_value_to_result(value)?;
        let source_ty = ctx.raw_value_php_type(value)?;
        materialize_direct_call_arg_for_param(ctx, &source_ty, &PhpType::Mixed)?;
        return Ok(());
    }
    abi::emit_load_int_immediate(ctx.emitter, abi::int_result_reg(ctx.emitter), 0);
    emit_box_current_value_as_mixed(ctx.emitter, &PhpType::Void);
    Ok(())
}

/// Returns true when a value is an omitted optional-argument placeholder.
pub(super) fn is_synthetic_null_value(ctx: &FunctionContext<'_>, value: ValueId) -> Result<bool> {
    if ctx.value_php_type(value)? != PhpType::Void {
        return Ok(false);
    }
    let Some(value) = ctx.function.value(value) else {
        return Err(CodegenIrError::missing_entry("value", value.as_raw()));
    };
    let crate::ir::ValueDef::Instruction { inst, .. } = value.def else {
        return Ok(false);
    };
    let Some(inst) = ctx.function.instruction(inst) else {
        return Err(CodegenIrError::missing_entry("instruction", inst.as_raw()));
    };
    Ok(matches!(inst.op, Op::ConstNull)
        && inst
            .span
            .is_some_and(|span| span.line == 0 && span.col == 0))
}

/// Lowers Fiber state predicates directly to the shared runtime helper.
pub(super) fn lower_fiber_state_predicate(
    ctx: &mut FunctionContext<'_>,
    inst: &Instruction,
    object: ValueId,
    state: FiberStatePredicate,
) -> Result<()> {
    let receiver_arg = abi::int_arg_reg_name(ctx.emitter.target, 0);
    ctx.load_value_to_reg(object, receiver_arg)?;
    emit_fiber_state_predicate_call(ctx, inst, state)
}

/// Lowers Fiber state predicates when the receiver is boxed as `Mixed`.
pub(super) fn lower_mixed_fiber_state_predicate(
    ctx: &mut FunctionContext<'_>,
    inst: &Instruction,
    object: ValueId,
    method_name: &str,
    state: FiberStatePredicate,
) -> Result<()> {
    if inst.operands.len() != 1 {
        return Err(CodegenIrError::unsupported(format!(
            "Fiber mixed state predicate {} with EIR arguments",
            method_name
        )));
    }
    emit_mixed_fiber_receiver_to_arg(ctx, object, method_name)?;
    emit_fiber_state_predicate_call(ctx, inst, state)
}

/// Unboxes a `Mixed` receiver and leaves a verified `Fiber*` in argument register 0.
pub(super) fn emit_mixed_fiber_receiver_to_arg(
    ctx: &mut FunctionContext<'_>,
    object: ValueId,
    method_name: &str,
) -> Result<()> {
    let object_label = ctx.next_label("mixed_fiber_state_object");
    let fiber_label = ctx.next_label("mixed_fiber_state_fiber");
    let class_id = ctx
        .module
        .class_infos
        .get("Fiber")
        .map(|class| class.class_id)
        .ok_or_else(|| {
            CodegenIrError::unsupported("mixed Fiber predicate without Fiber metadata")
        })?;
    let receiver_arg = abi::int_arg_reg_name(ctx.emitter.target, 0);
    ctx.load_value_to_reg(object, abi::int_result_reg(ctx.emitter))?;
    abi::emit_call_label(ctx.emitter, "__rt_mixed_unbox");
    match ctx.emitter.target.arch {
        Arch::AArch64 => {
            ctx.emitter.instruction("cmp x0, #6");                              // continue only when the Mixed receiver holds an object
            ctx.emitter.instruction(&format!("b.eq {}", object_label));         // inspect the object class before calling the Fiber predicate
            emit_method_call_on_null_fatal(ctx, method_name);
            ctx.emitter.label(&object_label);
            ctx.emitter.instruction("ldr x9, [x1]");                            // load the receiver object's runtime class id
            ctx.emitter.instruction(&format!("cmp x9, #{}", class_id));         // verify the boxed object is a Fiber instance
            ctx.emitter.instruction(&format!("b.eq {}", fiber_label));          // call the Fiber predicate only for real Fiber receivers
            emit_method_call_on_null_fatal(ctx, method_name);
            ctx.emitter.label(&fiber_label);
            ctx.emitter
                .instruction(&format!("mov {}, x1", receiver_arg)); // pass the unboxed Fiber object to the runtime predicate
        }
        Arch::X86_64 => {
            ctx.emitter.instruction("cmp rax, 6");                              // continue only when the Mixed receiver holds an object
            ctx.emitter.instruction(&format!("je {}", object_label));           // inspect the object class before calling the Fiber predicate
            emit_method_call_on_null_fatal(ctx, method_name);
            ctx.emitter.label(&object_label);
            ctx.emitter.instruction("mov r10, QWORD PTR [rdi]");                // load the receiver object's runtime class id
            ctx.emitter.instruction(&format!("cmp r10, {}", class_id));         // verify the boxed object is a Fiber instance
            ctx.emitter.instruction(&format!("je {}", fiber_label));            // call the Fiber predicate only for real Fiber receivers
            emit_method_call_on_null_fatal(ctx, method_name);
            ctx.emitter.label(&fiber_label);
        }
    }
    Ok(())
}

/// Calls the shared runtime state predicate helper for a receiver already in arg0.
pub(super) fn emit_fiber_state_predicate_call(
    ctx: &mut FunctionContext<'_>,
    inst: &Instruction,
    state: FiberStatePredicate,
) -> Result<()> {
    abi::emit_load_int_immediate(
        ctx.emitter,
        abi::int_arg_reg_name(ctx.emitter.target, 1),
        state.expected_state() as i64,
    );
    abi::emit_call_label(ctx.emitter, "__rt_fiber_state_eq");
    if matches!(state, FiberStatePredicate::Started) {
        match ctx.emitter.target.arch {
            Arch::AArch64 => {
                ctx.emitter.instruction("eor x0, x0, #1");                      // invert not-started into PHP's isStarted predicate
            }
            Arch::X86_64 => {
                ctx.emitter.instruction("xor rax, 1");                          // invert not-started into PHP's isStarted predicate
            }
        }
    }
    store_if_result(ctx, inst)
}

/// Fiber state-query method selected by a direct method call.
pub(super) enum FiberStatePredicate {
    Started,
    Running,
    Suspended,
    Terminated,
}

impl FiberStatePredicate {
    /// Returns the runtime state value compared by `__rt_fiber_state_eq`.
    fn expected_state(&self) -> i32 {
        match self {
            Self::Started => crate::codegen::runtime::FIBER_STATE_NOT_STARTED,
            Self::Running => crate::codegen::runtime::FIBER_STATE_RUNNING,
            Self::Suspended => crate::codegen::runtime::FIBER_STATE_SUSPENDED,
            Self::Terminated => crate::codegen::runtime::FIBER_STATE_TERMINATED,
        }
    }
}

/// Returns true when a direct method call targets PHP's built-in `Fiber::start`.
pub(super) fn is_fiber_start_call(class_name: &str, method_name: &str) -> bool {
    php_symbol_key(class_name.trim_start_matches('\\')) == "fiber"
        && php_symbol_key(method_name) == "start"
}

/// Returns true when a direct method call targets PHP's built-in `Fiber::resume`.
pub(super) fn is_fiber_resume_call(class_name: &str, method_name: &str) -> bool {
    php_symbol_key(class_name.trim_start_matches('\\')) == "fiber"
        && php_symbol_key(method_name) == "resume"
}

/// Returns true when a direct method call targets PHP's built-in `Fiber::throw`.
pub(super) fn is_fiber_throw_call(class_name: &str, method_name: &str) -> bool {
    php_symbol_key(class_name.trim_start_matches('\\')) == "fiber"
        && php_symbol_key(method_name) == "throw"
}

/// Returns true when a direct method call targets PHP's built-in `Fiber::getReturn`.
pub(super) fn is_fiber_get_return_call(class_name: &str, method_name: &str) -> bool {
    php_symbol_key(class_name.trim_start_matches('\\')) == "fiber"
        && php_symbol_key(method_name) == "getreturn"
}

/// Resolves a Fiber state predicate method name, if the receiver is `Fiber`.
pub(super) fn fiber_state_predicate(class_name: &str, method_name: &str) -> Option<FiberStatePredicate> {
    if php_symbol_key(class_name.trim_start_matches('\\')) != "fiber" {
        return None;
    }
    fiber_state_predicate_method(method_name)
}

/// Resolves a Fiber state predicate solely from the method name.
pub(super) fn fiber_state_predicate_method(method_name: &str) -> Option<FiberStatePredicate> {
    match php_symbol_key(method_name).as_str() {
        "isstarted" => Some(FiberStatePredicate::Started),
        "isrunning" => Some(FiberStatePredicate::Running),
        "issuspended" => Some(FiberStatePredicate::Suspended),
        "isterminated" => Some(FiberStatePredicate::Terminated),
        _ => None,
    }
}

/// Lowers a nullsafe method call by short-circuiting boxed-null receivers.
pub(super) fn lower_nullsafe_method_call(ctx: &mut FunctionContext<'_>, inst: &Instruction) -> Result<()> {
    let object = expect_operand(inst, 0)?;
    let method_name = method_name_data(ctx, inst)?.to_string();
    let Some((class_name, nullable)) = objects::nullable_object_receiver_class(ctx, object)? else {
        return Err(CodegenIrError::unsupported(format!(
            "{} for receiver PHP type {:?}",
            inst.op.name(),
            objects::raw_value_php_type(ctx, object)?
        )));
    };
    if !nullable {
        return lower_method_call(ctx, inst);
    }
    let target = resolve_method_call_target(ctx, &class_name, &method_name, inst.operands.len())?;
    let null_label = ctx.next_label("nullsafe_method_null");
    let done_label = ctx.next_label("nullsafe_method_done");
    // THE RESERVED CALLEE-SAVED NESTED-CALL REGISTER (x19/r12), not a scratch one: the
    // receiver has to survive argument materialization, which runs runtime helpers and — for
    // an omitted optional by-reference argument — stages a caller-side cell. A caller-saved
    // scratch register is destroyed by both, and the method would then be entered with
    // whatever the last helper left there as `$this`. Every other receiver-register dispatch
    // (`lower_mixed_method_call`, callable dispatch, the array-access and intrinsic paths)
    // already uses this register for the same reason, and `crate::codegen::frame` reserves
    // its save slot for exactly the `NullsafeMethodCall` receivers this lowering handles.
    let object_reg = abi::nested_call_reg(ctx.emitter);
    objects::emit_nullable_receiver_object_payload(ctx, object, &null_label, object_reg)?;
    let receiver_ty = PhpType::Object(class_name);
    let mut param_types = Vec::with_capacity(target.params.len() + 1);
    param_types.push(receiver_ty.clone());
    param_types.extend(target.params.iter().map(|param| param.codegen_repr()));
    let mut ref_params = Vec::with_capacity(target.ref_params.len() + 1);
    ref_params.push(false);
    ref_params.extend(target.ref_params.iter().copied());
    let call_args = materialize_method_call_args_with_receiver_reg_and_refs(
        ctx,
        object_reg,
        &receiver_ty,
        &inst.operands,
        &param_types,
        &ref_params,
        crate::codegen::lower_inst::RefArgCellLifetime::CallOnly,
    )?;
    let caller_stack_pad_bytes = direct_call_stack_pad_bytes(ctx, call_args.overflow_bytes);
    abi::emit_reserve_temporary_stack(ctx.emitter, caller_stack_pad_bytes);
    abi::emit_call_label(
        ctx.emitter,
        &method_symbol(&target.impl_class, &target.method_key),
    );
    abi::emit_release_temporary_stack(ctx.emitter, caller_stack_pad_bytes);
    abi::emit_release_temporary_stack(ctx.emitter, call_args.overflow_bytes);
    if inst.result_php_type.codegen_repr() == PhpType::Mixed
        && target.return_ty.codegen_repr() != PhpType::Mixed
    {
        emit_box_current_value_as_mixed(ctx.emitter, &target.return_ty.codegen_repr());
    }
    abi::emit_jump(ctx.emitter, &done_label);
    ctx.emitter.label(&null_label);
    objects::emit_boxed_null(ctx);
    ctx.emitter.label(&done_label);
    store_if_result(ctx, inst)?;
    emit_ref_arg_writebacks(ctx, &call_args)
}
