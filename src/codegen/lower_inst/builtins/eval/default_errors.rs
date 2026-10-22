//! Purpose:
//! Publishes deferred native callable defaults to the eval signature ABI.
//!
//! Called from:
//! - Method and constructor metadata registration before dynamic eval executes.
//!
//! Key details:
//! - Deferred Errors are data, never callbacks that unwind through Rust frames.
//! - All supported targets use the shared six-argument registration ABI helpers.

use super::*;

/// Registers deferred Errors or runtime array spreads as owned expression metadata.
pub(super) fn register_eval_native_param_default_error(
    ctx: &mut FunctionContext<'_>, context_offset: usize, key: (&str, usize),
    index: usize, default: Option<&Expr>, default_context: &EvalNativeDefaultContext<'_>, symbol: &str,
) -> bool {
    let Some(default) = default else { return false };
    let (payload, symbol) = if let Some(message) =
        crate::types::deferred_defaults::deferred_error_message(default)
    {
        (message.to_string(), symbol.to_string())
    } else if crate::types::deferred_defaults::contains_deferred_error(default)
        || crate::types::deferred_defaults::contains_array_spread(default) {
        let Some(source) = super::default_source::default_expression_source(default, default_context)
            else { return false; };
        (source, format!("{}_expression", symbol.strip_suffix("_error").unwrap_or(symbol)))
    } else { return false; };
    let (message_label, message_len) = ctx.data.add_string(payload.as_bytes());
    load_eval_context_local_to_arg(ctx, context_offset, 0);
    for (index, label) in [(1, key.0), (4, message_label.as_str())] {
        abi::emit_symbol_address(ctx.emitter, abi::int_arg_reg_name(ctx.emitter.target, index), label);
    }
    for (argument, value) in [(2, key.1), (3, index), (5, message_len)] {
        abi::emit_load_int_immediate(ctx.emitter, abi::int_arg_reg_name(ctx.emitter.target, argument), value as i64);
    }
    let symbol = ctx.emitter.target.extern_symbol(&symbol);
    abi::emit_call_label(ctx.emitter, &symbol);
    let registered = ctx.next_label("eval_native_default_registered");
    abi::emit_branch_if_int_result_nonzero(ctx.emitter, &registered);
    super::status::emit_eval_fatal_message(ctx, "Fatal error: eval() native default registration failed\n");
    ctx.emitter.label(&registered);
    if !matches!(default.kind, ExprKind::Throw(_)) {
        register_native_reflection_factory(ctx, context_offset, key, index, default,
            default_context, if symbol.contains("_constructor_") { 2 }
                else if symbol.contains("_static_method_") { 1 }
                else if symbol.contains("_function_") { 3 } else { 0 });
    }
    true
}

/// Registers a capture-free native factory only for protected Reflection getter invocation.
/// Default argument binding still interprets the owned expression data and never calls it.
fn register_native_reflection_factory(
    ctx: &mut FunctionContext<'_>, context_offset: usize, key: (&str, usize), index: usize,
    default: &Expr, context: &EvalNativeDefaultContext<'_>, kind: i64,
) {
    let Some(helper) = ctx.module.functions.iter().find(|function|
        function.lexical_class.as_deref() == context.current_class
            && function.callable_default.as_ref().is_some_and(|value|
                value == default && value.span == default.span)) else { return; };
    let Some(signature) = helper.signature.clone() else { return; };
    let entry = crate::names::function_symbol(&helper.name);
    let invoker = crate::codegen::lower_inst::emit_runtime_callable_invoker_with_string_owner(
        ctx, &signature, &[], false, true,
    );
    let descriptor = crate::codegen_support::callable_descriptor::static_descriptor_with_optional_invoker_meta(
        ctx.data, &entry, None,
        crate::codegen_support::callable_descriptor::CALLABLE_DESC_KIND_FUNCTION,
        Some(&signature), &[], &[],
        crate::codegen_support::callable_descriptor::CallableDescriptorInvocation::new(
            crate::codegen_support::callable_descriptor::CallableDescriptorShape::Function,
        ), Some(&invoker),
    );
    load_eval_context_local_to_arg(ctx, context_offset, 0);
    for (argument, label) in [(1, key.0), (5, descriptor.as_str())] {
        abi::emit_symbol_address(ctx.emitter, abi::int_arg_reg_name(ctx.emitter.target, argument), label);
    }
    for (argument, value) in [(2, key.1 as i64), (3, index as i64), (4, kind)] {
        abi::emit_load_int_immediate(ctx.emitter, abi::int_arg_reg_name(ctx.emitter.target, argument), value);
    }
    let symbol = ctx.emitter.target.extern_symbol("__elephc_eval_register_native_param_default_factory");
    abi::emit_call_label(ctx.emitter, &symbol);
}
