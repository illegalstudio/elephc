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
    let function = helper_function(CALLABLE_ARGUMENT_NORMALIZER, PhpType::Callable);
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
    super::callables::emit_runtime_mixed_callable_descriptor_value_with_type_error(
        &mut ctx, helper_value(), "callable argument",
        "Argument must be a valid callback",
    )?;
    abi::load_at_offset(ctx.emitter, nested, saved_nested_offset);
    abi::emit_frame_restore(ctx.emitter, frame_size);
    abi::emit_return(ctx.emitter);
    Ok(())
}
