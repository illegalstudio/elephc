//! Purpose:
//! Adapts original descriptor argument values to reusable EIR value guards.
//!
//! Called from:
//! - Invoker argument materialization before any PHP scalar conversion.
//! - Module finalization after native and eval descriptors have registered their contracts.
//!
//! Key details:
//! - Public callable boundaries retain the module policy independently of implementation syntax.
//! - Guards borrow existing owners; temporary boxes are visible to native exception cleanup.

use super::{abi, DataSection, Emitter, InvokerEmitContext, PhpType};
use elephc_semantics::conversions::ConversionPlan;

/// Checks original kinds while preserving the source's complete result-register representation.
pub(super) fn check(emitter: &mut Emitter, ctx: &mut InvokerEmitContext,
    data: &mut DataSection, source: &PhpType) {
    if ctx.materializing_default { return; }
    let Some(expected) = ctx.parameter_contract.as_ref() else { return; };
    if crate::semantics::conversions::implicit_contract(expected).is_none() { return; }
    let plan = crate::semantics::conversions::implicit(ctx.semantic_profile,
        crate::source::SourceMode::Php, expected, source);
    if matches!(plan, ConversionPlan::ExistingBehavior | ConversionPlan::Identity) { return; }
    let label = data.parameter_guard(expected);
    let boxed = source.codegen_repr() != PhpType::Mixed;
    if boxed {
        super::emit_box_current_value_as_mixed(emitter, source);
        ctx.argument_owners.record_coercion_source(emitter);
    }
    abi::emit_push_reg(emitter, abi::int_result_reg(emitter));
    abi::emit_reg_move(emitter, abi::int_arg_reg_name(emitter.target, 0), abi::int_result_reg(emitter));
    abi::emit_call_label(emitter, &label);
    abi::emit_pop_reg(emitter, abi::int_result_reg(emitter));
    if boxed {
        abi::emit_push_reg(emitter, abi::int_result_reg(emitter));
        super::coerce_result_to_type(emitter, ctx, data, &PhpType::Mixed, source);
        ctx.argument_owners.clear_coercion_source(emitter);
        super::release_preserved_mixed_after_arg_coercion(emitter, source);
    }
}

/// Emits each requested scalar or nominal guard through ordinary EIR exactly once.
pub(in crate::codegen) fn emit_requested(module: &crate::ir::Module, emitter: &mut Emitter,
    data: &mut DataSection, shared: &mut crate::codegen::shared_state::SharedCodegenState)
    -> crate::codegen::Result<()> {
    for (label, expected) in data.requested_parameter_guards() {
        let mut wrapper_module = shared.take_wrapper_module(module);
        let guard = crate::ir_lower::lower_callable_parameter_guard(&mut wrapper_module, &label, &expected);
        crate::ir::validate_function(&guard).expect("parameter guard must be valid EIR");
        let scope = emitter.begin_out_of_line();
        let result = crate::codegen::block_emit::emit_synthetic_function_with_label(
            &wrapper_module, &guard, &label, emitter, data, shared, false,
        );
        emitter.end_out_of_line(scope);
        shared.recycle_wrapper_module(wrapper_module);
        result?;
    }
    Ok(())
}
