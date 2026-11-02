//! Purpose:
//! Emits the class ladder of an untyped-receiver method call (`$mixed->foo($a, $b)`) ONCE per
//! module per key and calls it from every site, instead of re-emitting a class-id search and one
//! argument-materializing call arm per class that declares the method at each site.
//!
//! Called from:
//! - `lower_mixed_method_call()` in `method_dispatch`, once the candidate classes are known.
//! - `crate::codegen::lower_inst::objects` through `emit_pending_dynamic_property_helpers()`,
//!   which writes the helper bodies right behind the function that first asked for them.
//!
//! Key details:
//! - Measured on the compiled Laravel `--web` app, 25 956 `method_call` sites averaged 985
//!   instructions (25.6 M in all) and the average grew as more classes compiled: the cost of the
//!   inline ladder is sites x classes.
//! - The helper is an ordinary synthetic EIR function `(receiver, arg0..argN) -> Mixed` holding
//!   the ONE `MethodCall` instruction, so the existing lowering -- class ladder, per-candidate
//!   argument coercion and TypeErrors, defaults, variadic tails, missing-method and null-receiver
//!   errors, exception propagation -- runs unchanged inside it. Inside the helper `lower_method_call`
//!   skips the eval-owner probe (see `inside_mixed_method_call_helper`).
//! - Key: the method name as written at the site (it appears in error messages), the strict-types
//!   profile of the site, and each operand's IR and PHP type plus the result type. The candidate
//!   set is a function of the module, the name and the operand count, and it does not consult the
//!   lexical scope (visibility is the checker's job), so no scope enters the key.
//! - Calling convention: the same as a direct call to a user function with those parameter types;
//!   the site materializes the operands with the direct-call machinery, so ownership is the same
//!   as for any call -- arguments are borrowed by the callee, a coercion temporary the site makes is
//!   released by the site after the call, and the owned `Mixed` result is stored as the call's value.
//! - Stays inline: fewer than three class arms, a site in a body with an eval context (its native
//!   dispatch may fall back to the interpreter), any candidate that takes a by-reference
//!   parameter or returns by reference (the helper cannot reach the caller's variable), results
//!   that are neither an owned boxed `Mixed` nor a plain int, bool or float (those enter the key),
//!   and more than eight operands.
//! - `ELEPHC_NO_OUTLINED_METHOD_CALLS=1` (or `ELEPHC_NO_OUTLINED_DYNAMIC_PROPS=1`) keeps the inline
//!   ladders, for size and behaviour comparisons.

use super::*;
use crate::codegen::data_section::DataSection;
use crate::codegen::shared_state::{
    DynamicPropHelperKind, MethodCallHelperTemplate, PendingDynamicPropHelper, SharedCodegenState,
};
use crate::codegen_support::emit::Emitter;
use crate::ir::{Effects, IrHeapKind, Module};

/// Readable part of the helper's entry symbol.
const METHOD_CALL_PREFIX: &str = "mixed_method_call";

/// Smallest ladder, in class arms, worth a call.
const MIN_ARMS: usize = 3;

/// Most operands (receiver included) an outlined site passes.
const MAX_OPERANDS: usize = 9;

/// Environment switch that keeps the inline ladders, for size comparisons.
fn outlining_disabled() -> bool {
    std::env::var_os("ELEPHC_NO_OUTLINED_METHOD_CALLS").is_some()
        || std::env::var_os("ELEPHC_NO_OUTLINED_DYNAMIC_PROPS").is_some()
}

/// Returns whether the body being lowered IS a shared method-call helper.
pub(super) fn inside_mixed_method_call_helper(ctx: &FunctionContext<'_>) -> bool {
    ctx.function.flags.is_synthetic
        && ctx
            .function
            .name
            .contains(&format!("_{}_", METHOD_CALL_PREFIX))
}

/// Returns whether the helper can hand the call's result back like a user function would: an
/// owned boxed `Mixed`, or a plain integer, boolean or float (the `bool` of an `isset($mixed[$k])`
/// that dispatches `offsetExists()`, the `int` of a `count()`), which no ownership follows.
fn outlinable_result(inst: &Instruction) -> bool {
    match (inst.result_type, inst.result_ownership) {
        (IrType::Heap(IrHeapKind::Mixed), Ownership::Owned) => {
            inst.result_php_type.codegen_repr() == PhpType::Mixed
        }
        (IrType::I64, Ownership::NonHeap) => matches!(
            inst.result_php_type.codegen_repr(),
            PhpType::Int | PhpType::Bool
        ),
        (IrType::F64, Ownership::NonHeap) => inst.result_php_type.codegen_repr() == PhpType::Float,
        _ => false,
    }
}

/// Lowers an untyped-receiver method call through the shared helper.
///
/// Returns `false` without emitting anything when this site must keep the inline ladder.
pub(super) fn try_lower_outlined_mixed_method_call(
    ctx: &mut FunctionContext<'_>,
    inst: &Instruction,
    candidates: &[MixedMethodCandidate],
) -> Result<bool> {
    if outlining_disabled()
        || inst.op != Op::MethodCall
        || candidates.len() < MIN_ARMS
        || inst.operands.len() > MAX_OPERANDS
        || inside_mixed_method_call_helper(ctx)
        || builtins::has_eval_context(ctx)
        || !outlinable_result(inst)
        || !matches!(
            inst.immediate,
            Some(Immediate::Data(_)) | Some(Immediate::ProfiledData { .. })
        )
    {
        return Ok(false);
    }
    let Some(result) = inst.result else {
        return Ok(false);
    };
    let argument_count = inst.operands.len().saturating_sub(1);
    if candidates.iter().any(|candidate| {
        candidate.target.by_ref_return
            || candidate
                .target
                .ref_params
                .iter()
                .take(argument_count)
                .any(|by_ref| *by_ref)
    }) {
        return Ok(false);
    }
    let mut operands = Vec::with_capacity(inst.operands.len());
    for operand in &inst.operands {
        let php_type = ctx.raw_value_php_type(*operand)?;
        if matches!(php_type.codegen_repr(), PhpType::Void) {
            return Ok(false);
        }
        operands.push((ctx.value_ir_type(*operand)?, php_type));
    }
    let method_name = method_name_data(ctx, inst)?.to_string();
    let strict_php = instruction_strict_php_profile(inst);
    let mut key = format!("{}:{}:{}", METHOD_CALL_PREFIX, method_name, strict_php);
    for (ir_type, php_type) in &operands {
        key.push_str(&format!(":{:?}/{:?}", ir_type, php_type));
    }
    if inst.result_type != IrType::Heap(IrHeapKind::Mixed) {
        key.push_str(&format!("->{:?}/{:?}", inst.result_type, inst.result_php_type));
    }
    let label = ctx.keyed_global_label(&key, METHOD_CALL_PREFIX);
    ctx.shared.request_dynamic_prop_helper(
        &key,
        &label,
        DynamicPropHelperKind::MixedMethodCall,
        None,
    );
    ctx.shared
        .method_call_templates
        .entry(key)
        .or_insert_with(|| MethodCallHelperTemplate {
            op: inst.op,
            immediate: inst.immediate.clone(),
            operands: operands.clone(),
            result_type: inst.result_type.clone(),
            result_php_type: inst.result_php_type.clone(),
            result_ownership: inst.result_ownership,
        });

    let param_types = operands
        .iter()
        .map(|(_, php_type)| php_type.codegen_repr())
        .collect::<Vec<_>>();
    let ref_params = vec![false; param_types.len()];
    let call_args = materialize_direct_call_args_with_refs_and_options(
        ctx,
        &inst.operands,
        &param_types,
        &ref_params,
        true,
        RefArgCellLifetime::CallOnly,
    )?;
    let caller_stack_pad_bytes = direct_call_stack_pad_bytes(ctx, call_args.overflow_bytes);
    abi::emit_reserve_temporary_stack(ctx.emitter, caller_stack_pad_bytes);
    abi::emit_call_label(ctx.emitter, &label);
    abi::emit_release_temporary_stack(ctx.emitter, caller_stack_pad_bytes);
    abi::emit_release_temporary_stack(ctx.emitter, call_args.overflow_bytes);
    ctx.store_result_value(result)?;
    emit_call_arg_temp_cleanups(ctx, &call_args, None)?;
    emit_ref_arg_writebacks(ctx, &call_args)?;
    Ok(true)
}

/// Emits a method-call helper as a synthetic EIR function holding the one call instruction.
pub(in crate::codegen) fn emit_mixed_method_call_helper(
    module: &Module,
    emitter: &mut Emitter,
    data: &mut DataSection,
    shared: &mut SharedCodegenState,
    pending: &PendingDynamicPropHelper,
) -> Result<()> {
    use crate::ir::{Function, FunctionParam, LocalKind as Kind, Terminator};

    let Some(template) = shared.method_call_templates.get(&pending.key).cloned() else {
        return Err(CodegenIrError::invalid_module(format!(
            "method call helper {} has no template",
            pending.key
        )));
    };
    let mut function = Function::new(
        pending.label.clone(),
        template.result_type.clone(),
        template.result_php_type.clone(),
    );
    function.flags.is_synthetic = true;
    // A clone helper keyed by its site's scope resolves `__clone()` visibility against it.
    function.lexical_class = pending.scope.clone();
    for (index, (ir_type, php_type)) in template.operands.iter().enumerate() {
        let name = if index == 0 {
            "receiver".to_string()
        } else {
            format!("arg{}", index - 1)
        };
        function.params.push(FunctionParam {
            name: name.clone(),
            ir_type: ir_type.clone(),
            php_type: php_type.clone(),
            by_ref: false,
            variadic: false,
        });
        function.add_local(Some(name), ir_type.clone(), php_type.clone(), Kind::PhpLocal);
    }
    {
        let mut builder = Builder::new(&mut function);
        let entry = builder.create_named_block("entry", Vec::new());
        builder.set_entry(entry);
        builder.position_at_end(entry);
        let operands = template
            .operands
            .iter()
            .enumerate()
            .map(|(index, (ir_type, php_type))| {
                builder.emit_load_local(
                    LocalSlotId::from_raw(index as u32),
                    ir_type.clone(),
                    php_type.clone(),
                )
            })
            .collect::<Vec<_>>();
        let value = builder.emit_with_effects(
            template.op,
            operands,
            template.immediate.clone(),
            template.result_type.clone(),
            template.result_php_type.clone(),
            template.result_ownership,
            // The site already published the file and line of the PHP statement. This
            // instruction has no span, so letting it warn-publish would overwrite them with zero.
            template.op.default_effects() - Effects::MAY_WARN,
            None,
        );
        builder.terminate(Terminator::Return { value });
    }
    crate::codegen::block_emit::emit_synthetic_function_with_label(
        module,
        &function,
        &pending.label,
        emitter,
        data,
        shared,
        false,
    )
}
