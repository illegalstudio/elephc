//! Purpose:
//! Emits the class-name ladder of `new $class(...)` / `newInstanceWithoutConstructor` over a
//! runtime class string ONCE per module per key and calls it from every site, instead of
//! re-emitting one class-name compare plus the whole allocation, property-default and
//! constructor arm per candidate class at each site.
//!
//! Called from:
//! - `lower_dynamic_object_new_without_constructor_mixed()` and
//!   `lower_dynamic_object_new_mixed()` in `objects::fiber_dynamic_entry`, before they build the
//!   inline ladder.
//! - `crate::codegen::lower_inst::objects` through `emit_pending_dynamic_property_helpers()`,
//!   which writes the helper bodies right behind the function that first asked for them (the
//!   body is the one the untyped method-call helpers use: `emit_mixed_method_call_helper`).
//!
//! Key details:
//! - Measured on the compiled Symfony demo console, 18 constructorless sites each carried ~3 340
//!   class arms and 78 constructor sites ~540 each: ~100 000 arms, about 6 % of all code. The cost
//!   of the inline ladder is sites x classes.
//! - The helper is an ordinary synthetic EIR function `(class name, arg0..argN) -> Mixed` holding
//!   the ONE dynamic-new instruction, so the existing lowering -- the name ladder, allocation,
//!   property defaults, constructor call, argument coercion, refusals, the registry fallback and
//!   the class-not-found fatal -- runs unchanged inside it. Inside the helper the lowering sees
//!   `inside_dynamic_new_helper()` and keeps its inline ladder.
//! - Key: the helper prefix, the strict-types profile of the site, the instruction's immediate and
//!   each operand's IR and PHP type plus the result type. The candidate set is a function of the
//!   module and the operand shapes only.
//! - Calling convention: the same as a direct call to a user function with those parameter types;
//!   the site materializes the operands with the direct-call machinery, so ownership is the same as
//!   for any call -- arguments are borrowed by the callee, a coercion temporary the site makes is
//!   released by the site after the call, and the owned `Mixed` result is stored as the value.
//! - Stays inline: a constructor site in a body with an eval context (its refusal arms are omitted
//!   there), a class operand that is neither a string nor boxed,
//!   results that are not an owned boxed `Mixed`, and more operands than the helper passes.
//! - `ELEPHC_NO_OUTLINED_DYNAMIC_NEW=1` keeps the inline ladders, for size and behaviour
//!   comparisons.

use super::*;
use crate::codegen::shared_state::{DynamicPropHelperKind, MethodCallHelperTemplate};
use crate::ir::IrHeapKind;

/// Entry-symbol fragment of the constructorless helper.
pub(in crate::codegen::lower_inst) const NOCTOR_PREFIX: &str = "dyn_new_noctor";

/// Entry-symbol fragment of the constructor helper.
pub(in crate::codegen::lower_inst) const CTOR_PREFIX: &str = "dyn_new_ctor";

/// Fewest helper-held candidate arms worth a shared helper.
pub(in crate::codegen::lower_inst) const MIN_CTOR_ARMS: usize = 3;

/// Most operands (class name included) an outlined site passes.
const MAX_OPERANDS: usize = 9;

/// Environment switch that keeps the inline ladders, for size comparisons.
fn outlining_disabled() -> bool {
    std::env::var_os("ELEPHC_NO_OUTLINED_DYNAMIC_NEW").is_some()
}

/// Returns whether the body being lowered IS a shared dynamic-new helper.
pub(in crate::codegen::lower_inst) fn inside_dynamic_new_helper(ctx: &FunctionContext<'_>) -> bool {
    ctx.function.flags.is_synthetic
        && [NOCTOR_PREFIX, CTOR_PREFIX]
            .iter()
            .any(|prefix| ctx.function.name.contains(&format!("_{}_", prefix)))
}

/// Returns whether the body being lowered IS the shared constructor helper.
pub(in crate::codegen::lower_inst) fn inside_dynamic_new_ctor_helper(
    ctx: &FunctionContext<'_>,
) -> bool {
    ctx.function.flags.is_synthetic
        && ctx.function.name.contains(&format!("_{}_", CTOR_PREFIX))
}

/// Publishes this site's source file and line for a Throwable the constructor helper builds
/// (`throwable_new::emit_throwable_creation_site`): `getFile()`/`getLine()` name the `new`
/// statement, which the shared helper cannot know. The helper reads them right after the
/// allocation, before any constructor can run another site. Emitted before the arguments are
/// loaded, so no argument register is clobbered.
fn publish_creation_site(ctx: &mut FunctionContext<'_>, inst: &Instruction) {
    use crate::codegen_support::throwable_layout::{DYNAMIC_NEW_SITE_BYTES, DYNAMIC_NEW_SITE_SYMBOL};
    ctx.data.add_comm(DYNAMIC_NEW_SITE_SYMBOL.to_string(), DYNAMIC_NEW_SITE_BYTES);
    let reg = abi::int_result_reg(ctx.emitter);
    let file = ctx
        .span_source_file(ctx.current_instruction_span())
        .map(str::to_string)
        .map(|path| ctx.data.add_string(path.as_bytes()));
    match file {
        Some((label, len)) => {
            abi::emit_symbol_address(ctx.emitter, reg, &label);
            abi::emit_store_reg_to_symbol(ctx.emitter, reg, DYNAMIC_NEW_SITE_SYMBOL, 0);
            abi::emit_load_int_immediate(ctx.emitter, reg, len as i64);
        }
        None => {
            abi::emit_symbol_address(ctx.emitter, reg, "_script_source_file");
            abi::emit_store_reg_to_symbol(ctx.emitter, reg, DYNAMIC_NEW_SITE_SYMBOL, 0);
            abi::emit_load_symbol_to_reg(ctx.emitter, reg, "_script_source_file_len", 0);
        }
    }
    abi::emit_store_reg_to_symbol(ctx.emitter, reg, DYNAMIC_NEW_SITE_SYMBOL, 8);
    let line = inst.span.map_or(0, |span| span.source_line());
    abi::emit_load_int_immediate(ctx.emitter, reg, i64::from(line));
    abi::emit_store_reg_to_symbol(ctx.emitter, reg, DYNAMIC_NEW_SITE_SYMBOL, 16);
}

/// Lowers a constructorless dynamic-new instruction through its shared helper.
///
/// Returns `false` without emitting anything when this site must keep the inline ladder.
pub(in crate::codegen::lower_inst) fn try_lower_outlined_dynamic_new(
    ctx: &mut FunctionContext<'_>,
    inst: &Instruction,
    prefix: &str,
) -> Result<bool> {
    emit_outlined_dynamic_new_call(ctx, inst, prefix)
}

/// Emits the call of a dynamic-new helper, stores the helper's boxed result into the instruction's
/// value and releases the site's argument temporaries.
///
/// The constructor helper answers `0` for a class it does not hold (a miss); the site then stores
/// that `0` and goes on with its own remaining arms, which overwrite it.
pub(in crate::codegen::lower_inst) fn emit_outlined_dynamic_new_call(
    ctx: &mut FunctionContext<'_>,
    inst: &Instruction,
    prefix: &str,
) -> Result<bool> {
    if outlining_disabled()
        || inside_dynamic_new_helper(ctx)
        // The ladder's refusal arms are omitted under an eval context, so a constructor site in such
        // a body would change meaning inside the context-free helper. The runtime-argument-container
        // form has no refusal arms at all, so it outlines either way. The constructorless form has
        // no refusals and reaches the interpreter through the null-or-own-context loader either way.
        || (prefix == CTOR_PREFIX
            && builtins::has_eval_context(ctx)
            && !matches!(inst.immediate, Some(Immediate::Bool(true))))
        || inst.operands.is_empty()
        || inst.operands.len() > MAX_OPERANDS
        || inst.result_ownership != Ownership::Owned
        || inst.result_type != IrType::Heap(IrHeapKind::Mixed)
        || inst.result_php_type.codegen_repr() != PhpType::Mixed
    {
        return Ok(false);
    }
    let Some(result) = inst.result else {
        return Ok(false);
    };
    // The inline lowering refuses a class operand that is not a string or boxed.
    if !matches!(
        ctx.value_php_type(inst.operands[0])?.codegen_repr(),
        PhpType::Str | PhpType::Mixed | PhpType::Union(_)
    ) {
        return Ok(false);
    }
    let mut operands = Vec::with_capacity(inst.operands.len());
    for operand in &inst.operands {
        let php_type = ctx.raw_value_php_type(*operand)?;
        if matches!(php_type.codegen_repr(), PhpType::Void) {
            return Ok(false);
        }
        // Every operand reaches the helper boxed: its ladder coerces a `Mixed` to each constructor's
        // parameter types, as for any gradual call, and reads a boxed class name as it reads a
        // string one. Keyed by their concrete types, every operand-type shape got a helper of its
        // own holding the WHOLE class ladder: 29 of them, 19 MB of the compiled Symfony console.
        // An operand that is ALREADY boxed is keyed as `Mixed` too: its PHP type (`?Foo`, a union)
        // changes nothing the helper does with the box, and keeping it split one arity into
        // identical copies -- three 0.8 MB four-argument helpers in the compiled Symfony app.
        operands.push((IrType::Heap(IrHeapKind::Mixed), PhpType::Mixed));
    }
    let strict_php = instruction_strict_php_profile(inst);
    let mut key = format!("{}:{}:{:?}", prefix, strict_php, inst.immediate);
    for (ir_type, php_type) in &operands {
        key.push_str(&format!(":{:?}/{:?}", ir_type, php_type));
    }
    let label = ctx.keyed_global_label(&key, prefix);
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
            result_ownership: Ownership::Owned,
        });

    if prefix == CTOR_PREFIX {
        publish_creation_site(ctx, inst);
    }
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
