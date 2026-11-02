//! Purpose:
//! Normalizes descriptor-invoker callable arguments through the existing callable selectors.
//!
//! Called from:
//! - `crate::codegen::finalize_user_asm()` after all descriptor invokers are known.
//!
//! Key details:
//! - Accepts a borrowed Mixed cell in the integer result register and returns an owned descriptor.
//! - The source has a stable frame home, while its invoker owns exception cleanup.
//! - Invalid callable shapes raise TypeError instead of reaching a native descriptor dereference.

use crate::codegen::shared_helper::{helper_function, helper_value};
use crate::codegen::shared_state::SharedCodegenState;
use crate::codegen::{abi, context::FunctionContext, data_section::DataSection, emit::Emitter, frame};
use crate::ir::Module;
use crate::types::PhpType;

use super::Result;

/// Shared entry using the same result-register convention as runtime value coercions.
pub(in crate::codegen) const CALLABLE_ARGUMENT_NORMALIZER: &str = "_eir_callable_argument_normalizer";

/// Emits one normalizer with a stored source and a preserved nested-call register on every target.
pub(in crate::codegen) fn emit_callable_argument_normalizer(
    module: &Module,
    emitter: &mut Emitter,
    data: &mut DataSection,
    shared: &mut SharedCodegenState,
) -> Result<()> {
    let mut function = helper_function(CALLABLE_ARGUMENT_NORMALIZER, PhpType::Callable);
    let callback_context = crate::codegen::eval_callable_helpers::module_needs_eval_callable_descriptor_support(module)
        .then(|| function.add_local(None, crate::ir::IrType::I64, PhpType::Int, crate::ir::LocalKind::EvalContext));
    let layout = frame::layout_for_function(&function, emitter.target, false, false, false);
    let saved_nested_offset = layout.frame_size - 8;
    let frame_size = layout.frame_size + 16;
    let mut ctx = FunctionContext::new(
        module, &function, emitter, data, shared, layout, false, false, false, None,
    );
    ctx.emitter.blank();
    ctx.emitter.comment("shared callable argument normalization");
    ctx.emitter.label_global(CALLABLE_ARGUMENT_NORMALIZER);
    abi::emit_frame_prologue(ctx.emitter, frame_size);
    let nested = abi::nested_call_reg(ctx.emitter);
    abi::store_at_offset(ctx.emitter, nested, saved_nested_offset);
    ctx.store_result_value(helper_value())?;
    if let Some(slot) = callback_context {
        // The helper has no source eval frame. The retained callback identifies its owner.
        abi::emit_load_int_immediate(ctx.emitter, abi::int_arg_reg_name(ctx.emitter.target, 0), 0);
        ctx.load_value_to_reg(helper_value(), abi::int_arg_reg_name(ctx.emitter.target, 1))?;
        let symbol = ctx.emitter.target.extern_symbol("__elephc_eval_object_context");
        abi::emit_call_label(ctx.emitter, &symbol);
        let offset = ctx.local_offset(slot)?;
        abi::store_at_offset(ctx.emitter, abi::int_result_reg(ctx.emitter), offset);
    }
    super::callables::emit_runtime_mixed_callable_descriptor_value_with_type_error(
        &mut ctx, helper_value(), "callable argument",
        "Argument must be a valid callback",
    )?;
    abi::load_at_offset(ctx.emitter, nested, saved_nested_offset);
    abi::emit_frame_restore(ctx.emitter, frame_size);
    abi::emit_return(ctx.emitter);
    Ok(())
}

#[cfg(test)]
mod tests {
    use crate::codegen_support::platform::Target;

    /// Shared argument normalization discovers an eval closure owner without a source eval local.
    #[test]
    fn callable_argument_normalizer_discovers_eval_callback_owner_on_every_target() {
        let source = "<?php function acceptOwnedEvalCallback(callable $callback): mixed { return $callback(); } \
            class NativeNormalizerInvoke { public function __invoke(): int { return 2; } } \
            acceptOwnedEvalCallback(new NativeNormalizerInvoke()); \
            $code = $argc > 0 ? 'acceptOwnedEvalCallback(function (): int { return 1; });' : ''; eval($code);";
        for name in ["macos-aarch64", "ios-arm64", "ios-sim-arm64", "linux-aarch64", "linux-x86_64"] {
            let target = Target::parse(name).unwrap();
            let module = crate::ir_lower::tests::lower_source_at_for_target(
                source, std::path::Path::new("main.php"), std::path::Path::new("."), target,
            );
            let assembly = crate::codegen::generate_user_asm_from_ir(&module, false, false).unwrap();
            let helper = assembly.split_once("_eir_callable_argument_normalizer:\n").unwrap().1;
            let owner = helper.find("__elephc_eval_object_context").unwrap();
            let probe = helper.find("__elephc_eval_is_callable").unwrap();
            let wrapper = helper.find("__elephc_eval_wrap_callback").unwrap();
            assert!(owner < probe && probe < wrapper,
                "{name}: native shape misses must discover and probe the callback's real owner");
        }
    }
}
