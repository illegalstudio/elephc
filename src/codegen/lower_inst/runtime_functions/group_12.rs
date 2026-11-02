//! Purpose:
//! Dispatches one bounded group of typed builtin runtime targets.
//!
//! Called from:
//! - `super::lower()` while lowering typed EIR runtime calls.
//!
//! Key details:
//! - Dispatch is by enum identity, never by PHP function-name strings.
//! - Extracted bodies remain thin calls into target-aware backend emitters.

use crate::codegen::context::FunctionContext;
use crate::codegen::Result;
use crate::ir::{RuntimeFnId, Instruction};

/// Lowers a target owned by bounded dispatch group 12, or returns `None`.
pub(super) fn lower(
    ctx: &mut FunctionContext<'_>,
    inst: &Instruction,
    target: RuntimeFnId,
) -> Option<Result<()>> {
    match target {
        RuntimeFnId::MixedSpreadArray => Some({
            crate::codegen::lower_inst::mixed_array_runtime::lower_mixed_spread_array(ctx, inst)
        }),
        RuntimeFnId::ArrayPtrSeek => Some({
            crate::codegen::lower_inst::builtins::arrays::lower_array_ptr_seek(ctx, inst)
        }),
        RuntimeFnId::ArrayPtrKey => Some({
            crate::codegen::lower_inst::builtins::arrays::lower_array_ptr_key(ctx, inst)
        }),
        RuntimeFnId::ArrayPtrValue => Some({
            crate::codegen::lower_inst::builtins::arrays::lower_array_ptr_value(ctx, inst)
        }),
        RuntimeFnId::BaseConvert => Some({
            crate::codegen::lower_inst::builtins::strings::lower_base_convert(ctx, inst)
        }),
        RuntimeFnId::ChunkSplit => Some({
            crate::codegen::lower_inst::builtins::strings::lower_chunk_split(ctx, inst)
        }),
        RuntimeFnId::StrWordCount => Some({
            crate::codegen::lower_inst::builtins::strings::lower_str_word_count(ctx, inst)
        }),
        RuntimeFnId::CountChars => Some({
            crate::codegen::lower_inst::builtins::strings::lower_count_chars(ctx, inst)
        }),
        RuntimeFnId::Strtr => Some({
            crate::codegen::lower_inst::builtins::strings::lower_strtr(ctx, inst)
        }),
        RuntimeFnId::MethodExists => Some({
            crate::codegen::lower_inst::builtins::lower_member_exists(
                ctx,
                inst,
                "method_exists",
            )
        }),
        RuntimeFnId::PropertyExists => Some({
            crate::codegen::lower_inst::builtins::lower_member_exists(
                ctx,
                inst,
                "property_exists",
            )
        }),
        RuntimeFnId::System => Some({
            crate::codegen::lower_inst::builtins::system::lower_system(ctx, inst)
        }),
        RuntimeFnId::Time => Some({
            crate::codegen::lower_inst::builtins::system::lower_time(ctx, inst)
        }),
        RuntimeFnId::Unpack => Some({
            crate::codegen::lower_inst::builtins::unpack::lower_unpack(ctx, inst)
        }),
        RuntimeFnId::Unserialize => Some({
            crate::codegen::lower_inst::builtins::serialize::lower_unserialize(ctx, inst)
        }),
        RuntimeFnId::Usleep => Some({
            crate::codegen::lower_inst::builtins::system::lower_usleep(ctx, inst)
        }),
        RuntimeFnId::GetResourceId => Some({
            crate::codegen::lower_inst::builtins::types::lower_get_resource_id(ctx, inst)
        }),
        RuntimeFnId::GetResourceType => Some({
            crate::codegen::lower_inst::builtins::types::lower_get_resource_type(ctx, inst)
        }),
        RuntimeFnId::Gettype => Some({
            crate::codegen::lower_inst::builtins::lower_gettype(ctx, inst)
        }),
        RuntimeFnId::IntvalBase => Some({
            crate::codegen::lower_inst::builtins::types::lower_intval_base(ctx, inst)
        }),
        RuntimeFnId::IsCallable => Some({
            crate::codegen::lower_inst::builtins::lower_is_callable(ctx, inst)
        }),
        RuntimeFnId::IsFinite => Some({
            crate::codegen::lower_inst::builtins::math::lower_is_finite(ctx, inst)
        }),
        RuntimeFnId::IsInfinite => Some({
            crate::codegen::lower_inst::builtins::math::lower_is_infinite(ctx, inst)
        }),
        RuntimeFnId::IsNan => Some({
            crate::codegen::lower_inst::builtins::math::lower_is_nan(ctx, inst)
        }),
        RuntimeFnId::IsNumeric => Some({
            crate::codegen::lower_inst::builtins::is_numeric::lower_is_numeric(ctx, inst)
        }),
        RuntimeFnId::Settype => Some({
            crate::codegen::lower_inst::builtins::types::lower_settype(ctx, inst)
        }),
        _ => None,
    }
}
