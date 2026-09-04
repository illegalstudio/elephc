//! Purpose:
//! Resolves method targets and stores direct or dynamic call results.
//!
//! Called from:
//! - `crate::codegen::lower_inst::lower_instruction()` and sibling lowering helpers.
//!
//! Key details:
//! - Preserves EIR ownership, ABI ordering, runtime symbols, and target-aware lowering.

use super::*;

/// Resolves method implementation class, canonical key, return type, and ABI arity.
pub(super) fn resolve_method_call_target(
    ctx: &FunctionContext<'_>,
    class_name: &str,
    method_name: &str,
    operand_count: usize,
) -> Result<MethodCallTarget> {
    let normalized = class_name.trim_start_matches('\\');
    let class_info = ctx.module.class_infos.get(normalized).ok_or_else(|| {
        CodegenIrError::unsupported(format!("method call on unknown class {}", normalized))
    })?;
    let method_key = php_symbol_key(method_name);
    let callee_sig = class_info.methods.get(&method_key).ok_or_else(|| {
        CodegenIrError::unsupported(format!(
            "method call to unknown method {}::{}",
            normalized, method_name
        ))
    })?;
    let expected_args = callee_sig.params.len() + 1;
    if operand_count != expected_args {
        return Err(CodegenIrError::unsupported(format!(
            "method call to {}::{} with {} operands for {} ABI params",
            normalized, method_name, operand_count, expected_args
        )));
    }
    let impl_class = class_info
        .method_impl_classes
        .get(&method_key)
        .cloned()
        .unwrap_or_else(|| normalized.to_string());
    let dynamic_slot = class_info.vtable_slots.get(&method_key).copied();
    let has_direct_body = class_method_already_emitted(ctx, &impl_class, &method_key, false);
    if !has_direct_body && dynamic_slot.is_none() {
        return Err(CodegenIrError::unsupported(format!(
            "method call to {}::{} without an emitted EIR method body",
            impl_class, method_name
        )));
    }
    let dynamic_slot = if class_info.final_methods.contains(&method_key) {
        None
    } else {
        dynamic_slot
    };
    Ok(MethodCallTarget {
        impl_class,
        method_key,
        dynamic_slot,
        params: callee_sig
            .params
            .iter()
            .map(|(_, ty)| ty.codegen_repr())
            .collect(),
        ref_params: callee_sig.ref_params.clone(),
        return_ty: callee_sig.return_type.clone(),
        by_ref_return: callee_sig.by_ref_return,
    })
}

/// Emits a runtime vtable dispatch for an instance method whose concrete override is late-bound.
pub(super) fn emit_dynamic_instance_method_call(ctx: &mut FunctionContext<'_>, slot: usize) {
    let class_id_reg = abi::temp_int_reg(ctx.emitter.target);
    let dispatch_reg = abi::symbol_scratch_reg(ctx.emitter);
    abi::emit_load_from_address(
        ctx.emitter,
        class_id_reg,
        abi::int_arg_reg_name(ctx.emitter.target, 0),
        0,
    );
    abi::emit_symbol_address(ctx.emitter, dispatch_reg, "_class_vtable_ptrs");
    match ctx.emitter.target.arch {
        Arch::AArch64 => {
            ctx.emitter.instruction(&format!(
                "ldr {}, [{}, {}, lsl #3]",
                dispatch_reg, dispatch_reg, class_id_reg
            ));                                                                 // load the class-specific instance-vtable pointer
        }
        Arch::X86_64 => {
            ctx.emitter.instruction(&format!(
                "mov {}, QWORD PTR [{} + {} * 8]",
                dispatch_reg, dispatch_reg, class_id_reg
            ));                                                                 // load the class-specific instance-vtable pointer
        }
    }
    abi::emit_load_from_address(ctx.emitter, dispatch_reg, dispatch_reg, slot * 8);
    abi::emit_call_reg(ctx.emitter, dispatch_reg);
}

/// Publishes the line of a call INTO a synthesized builtin class, for the exceptions it raises.
///
/// php reports the CALL SITE for an exception an internal method throws, because the `new` lives
/// in php-src and has no php line of its own. The synthesized bodies here are in exactly that
/// position: their `new` carries no span, so `getLine()` answered 0 and the uncaught report
/// dropped both its ` in FILE:LINE` suffix and its `thrown in` tail — MEASURED against
/// `(new SplFileInfo("nope"))->getSize()`, which php reports at the line of the call.
///
/// Only calls into a class the PROGRAM DID NOT DECLARE publish anything, so an ordinary method
/// call costs nothing. The predicate is the declaration's own span: a compiler-injected class
/// carries `Span::dummy()`. Reading whether the emitted BODIES were synthetic answered the same
/// question for the classes built from a synthesized AST and the wrong one for the rest —
/// `SplFixedArray` has no PHP body at all, its `new` and its throws are runtime helpers, and it
/// reported `line=0` where php reports the line of the `new`.
///
/// NOT the primary scratch: `emit_store_reg_to_symbol` resolves the symbol's own address through
/// it, which would overwrite the value before the store.
pub(super) fn publish_internal_call_line(
    ctx: &mut FunctionContext<'_>,
    inst: &crate::ir::Instruction,
    class_name: &str,
) {
    let Some(span) = inst.span else {
        return;
    };
    if span.line == 0 || !class_is_compiler_injected(ctx, class_name) {
        return;
    }
    publish_internal_call_trace_frame(ctx, inst, class_name);
    let reg = abi::secondary_scratch_reg(ctx.emitter);
    abi::emit_load_int_immediate(ctx.emitter, reg, i64::from(span.line));
    abi::emit_store_reg_to_symbol(ctx.emitter, reg, "_rt_internal_call_line", 0);
    // The same line, in the form a WARNING wants. `publish_diagnostic_location` only fires for an
    // instruction whose effects admit `MAY_WARN`, and a call into one of these classes may not:
    // the refinement pass unions the summaries of every class the receiver could hold and drops
    // the whole answer when one of them has none, leaving the lowering's own effects — which do
    // not mention warnings. MEASURED, `(new ArrayIterator([1]))->offsetGet(7)` raised
    // `Undefined array key 7` with NO ` in FILE on line N` at all, while the `ArrayObject`
    // spelling of the same call, whose summary did resolve, named its line correctly.
    crate::codegen::lower_inst::publish_diagnostic_line(ctx, span.line);
}

/// Records php's frame for this call, and publishes whether the chain it belongs to is COMPLETE.
///
/// An exception raised inside a builtin class is reported with the CALL as its frame `#0` —
/// `#0 p.php(13): SplFileInfo->getSize()` — and the arguments are the ones passed HERE, not the
/// synthesized body's own parameters, so the frame has to be recorded at the call.
///
/// `main` is the one caller this lowering can prove nothing hides. Anywhere else the chain would
/// need the enclosing function's frame and its callers, so the site publishes zero and the report
/// stays silent rather than printing a trace that is short — `#0 {main}` where php names a
/// function is a wrong answer, not a missing one.
///
/// The recording is skipped entirely off `main`, which is also what keeps it off hot paths: a
/// method called in a loop inside a user function pays two stores, not a frame render.
fn publish_internal_call_trace_frame(
    ctx: &mut FunctionContext<'_>,
    inst: &crate::ir::Instruction,
    class_name: &str,
) {
    let reg = abi::secondary_scratch_reg(ctx.emitter);
    abi::emit_load_int_immediate(ctx.emitter, reg, i64::from(ctx.is_main));
    abi::emit_store_reg_to_symbol(ctx.emitter, reg, "_rt_trace_site_exact", 0);
    if !ctx.is_main {
        return;
    }
    let Some((frame_name, arguments)) = internal_call_frame_shape(ctx, inst, class_name) else {
        return;
    };
    super::exceptions::emit_builtin_class_call_trace_frame(ctx, inst, &frame_name, &arguments);
}

/// Names the frame php would print for this call, and the operands it renders as its arguments.
///
/// A `new` is php's `Class->__construct(…)` — measured, `new DirectoryIterator("dtest")` reports
/// `DirectoryIterator->__construct('dtest')`. An instance call is `Class->method(…)`, and its
/// first operand is the RECEIVER rather than an argument.
fn internal_call_frame_shape(
    ctx: &FunctionContext<'_>,
    inst: &crate::ir::Instruction,
    class_name: &str,
) -> Option<(String, Vec<crate::ir::ValueId>)> {
    match inst.op {
        crate::ir::Op::ObjectNew => Some((
            format!("{class_name}->__construct"),
            inst.operands.clone(),
        )),
        crate::ir::Op::MethodCall | crate::ir::Op::NullsafeMethodCall => {
            let method = method_name_data(ctx, inst).ok()?;
            // ASKING an exception about itself is not a frame in anyone's trace. The buffer holds
            // one trace at a time, so recording here replaced the frames `getTraceAsString()` was
            // being asked to render with a frame for the question — and left them replaced for
            // whatever was reported next.
            if super::throwable_methods::is_throwable_standard_method_key(&php_symbol_key(method)) {
                return None;
            }
            Some((
                format!("{}->{method}", frame_class_for_method(ctx, class_name, method)),
                inst.operands.iter().skip(1).copied().collect(),
            ))
        }
        _ => None,
    }
}

/// Names the class php puts in a trace frame: the one that DECLARES the method.
///
/// php prints the declaring class, not the receiver's. MEASURED on `php -n` 8.5.6 with a plain
/// user hierarchy — `(new Derived())->boom()` reports `#0 …: Base->boom()` — and the SPL surface
/// is the same rule: `(new SplTempFileObject())->getSize()` reports `SplFileInfo->getSize()`,
/// because `getSize()` is declared there and neither `SplFileObject` nor `SplTempFileObject`
/// overrides it. elephc named the RECEIVER, which is right only when the two coincide.
///
/// `method_declaring_classes` is the checker's own answer and already accounts for overrides;
/// falling back to the receiver keeps a class with no schema entry naming something rather than
/// nothing.
fn frame_class_for_method(ctx: &FunctionContext<'_>, class_name: &str, method: &str) -> String {
    ctx.module
        .class_infos
        .get(class_name)
        .and_then(|class| class.method_declaring_classes.get(&php_symbol_key(method)))
        .cloned()
        .unwrap_or_else(|| class_name.to_string())
}

/// Reports whether this class came from the compiler rather than from the program's source.
fn class_is_compiler_injected(ctx: &FunctionContext<'_>, class_name: &str) -> bool {
    ctx.module
        .class_infos
        .get(class_name)
        .is_some_and(|class_info| class_info.declaration_span.line == 0)
}

/// Returns true when the current EIR module includes the target class method body.
pub(super) fn class_method_already_emitted(
    ctx: &FunctionContext<'_>,
    class_name: &str,
    method_key: &str,
    is_static: bool,
) -> bool {
    ctx.module.class_methods.iter().any(|function| {
        function.flags.is_static == is_static
            && function
                .name
                .rsplit_once("::")
                .is_some_and(|(candidate_class, candidate_method)| {
                    candidate_class == class_name && php_symbol_key(candidate_method) == method_key
                })
    })
}

/// Stores a call result, boxing concrete returns for generic EIR result slots.
pub(super) fn store_call_result(
    ctx: &mut FunctionContext<'_>,
    inst: &Instruction,
    return_ty: &PhpType,
) -> Result<()> {
    if let Some(result) = inst.result {
        let result_ty = ctx.value_php_type(result)?;
        let return_ty = return_ty.codegen_repr();
        if return_ty == PhpType::Void || result_ty == PhpType::Void {
            abi::emit_load_int_immediate(
                ctx.emitter,
                abi::int_result_reg(ctx.emitter),
                0x7fff_ffff_ffff_fffe,
            );
            if matches!(result_ty, PhpType::Mixed | PhpType::Union(_)) {
                emit_box_current_value_as_mixed(ctx.emitter, &PhpType::Void);
            }
            ctx.store_result_value(result)?;
            return Ok(());
        }
        if matches!(result_ty, PhpType::Mixed | PhpType::Union(_)) && return_ty != PhpType::Mixed {
            emit_box_current_value_as_mixed(ctx.emitter, &return_ty);
        }
        ctx.store_result_value(result)?;
    }
    Ok(())
}

/// Stores a resolved method call's result, honoring by-reference returns.
///
/// A by-reference-returning method hands back a single-word reference-cell pointer in the
/// integer result register (the method body's `Terminator::Return` placed it there), so the
/// result is stored single-word rather than split by the declared `Str`/`Float` return type.
pub(super) fn store_method_call_result(
    ctx: &mut FunctionContext<'_>,
    inst: &Instruction,
    target: &MethodCallTarget,
) -> Result<()> {
    if target.by_ref_return {
        if let Some(result) = inst.result {
            ctx.store_int_result_value(result)?;
        }
        return Ok(());
    }
    store_call_result(ctx, inst, &target.return_ty)
}

/// Resolves an instruction data immediate as a method name.
pub(super) fn method_name_data<'a>(ctx: &'a FunctionContext<'_>, inst: &Instruction) -> Result<&'a str> {
    let data = expect_data(inst)?;
    ctx.module
        .data
        .strings
        .get(data.as_raw() as usize)
        .map(String::as_str)
        .ok_or_else(|| CodegenIrError::missing_entry("data string", data.as_raw()))
}

