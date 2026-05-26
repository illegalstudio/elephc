//! Purpose:
//! Lowers final ABI argument pushes from named-source descriptors.
//! Works with the shared call-argument plan to preserve PHP named-argument semantics.
//!
//! Called from:
//! - `crate::codegen::expr::calls::args::named`
//!
//! Key details:
//! - Side effects occur in source order, while final argument materialization follows parameter and ABI order.

use crate::codegen::emit::Emitter;
use crate::codegen::{context::Context, data_section::DataSection};
use crate::types::{FunctionSig, PhpType};

use super::prefix::push_prefix_array_element_arg;
use super::temps::{push_saved_source_temp_arg, pushed_temp_bytes, temp_slot_size};
use super::variadic::emit_variadic_array_arg_from_sources;
use super::{FinalArgSource, PrefixVariadicTail, VariadicArgSource};
use super::super::{declared_target_ty, emit_empty_variadic_array_arg, push_expr_arg, EmittedCallArgs};

/// Materializes final ABI arguments from named-source descriptors in parameter order.
 ///
 /// Side effects from prefix-element evaluation and source-temp reads occur in source order
 /// (as planned by the shared call-argument planner), while the final ABI push order follows
 /// parameter/ABI order. This function consumes `slot_sources` and `variadic_sources`, emitting
 /// each argument via the helper chain: `push_saved_source_temp_arg`, `push_prefix_array_element_arg`,
 /// `push_expr_arg`, or `emit_variadic_array_arg_from_sources` depending on the source variant.
 ///
 /// # Parameters
 /// - `slot_sources`: per-slot sources for regular parameters (indexed by parameter position).
 /// - `variadic_sources`: sources for individual variadic arguments.
 /// - `prefix_variadic_tail`: optional prefix-array tail for variadic expansion.
 /// - `sig`: callee function signature used to resolve parameter names for named-key preference.
 /// - `regular_param_count`: number of caller-visible regular (non-variadic) parameters.
 /// - `source_temp_types`: PHP types of saved source temporaries for slot-size calculation.
 ///
 /// # Returns
 /// `EmittedCallArgs` containing the types of all arguments pushed and the total byte size of
 /// source temporaries (used by the caller for temp cleanup/frame accounting).
pub(super) fn push_final_call_args_from_sources(
    slot_sources: Vec<Option<FinalArgSource>>,
    variadic_sources: Vec<VariadicArgSource>,
    prefix_variadic_tail: Option<PrefixVariadicTail>,
    sig: &FunctionSig,
    regular_param_count: usize,
    source_temp_types: &[PhpType],
    emitter: &mut Emitter,
    ctx: &mut Context,
    data: &mut DataSection,
) -> EmittedCallArgs {
    let source_temp_bytes = pushed_temp_bytes(source_temp_types);
    let mut arg_types = Vec::new();
    let mut final_pushed_bytes = 0usize;

    for (idx, source) in slot_sources.into_iter().enumerate().take(regular_param_count) {
        let target_ty = declared_target_ty(Some(sig), idx);
        let pushed_ty = match source {
            Some(FinalArgSource::SourceTemp(temp_idx)) => {
                push_saved_source_temp_arg(
                    temp_idx,
                    source_temp_types,
                    final_pushed_bytes,
                    emitter,
                )
            }
            Some(FinalArgSource::PrefixElement {
                prefix_temp_idx,
                element_idx,
                prefer_named_key,
                default,
            }) => push_prefix_array_element_arg(
                prefix_temp_idx,
                element_idx,
                prefer_named_key
                    .then(|| sig.params.get(idx).map(|(name, _)| name.as_str()))
                    .flatten(),
                default.as_ref(),
                target_ty,
                source_temp_types,
                final_pushed_bytes,
                emitter,
                ctx,
                data,
            ),
            Some(FinalArgSource::Default(default)) => {
                push_expr_arg(&default, target_ty, emitter, ctx, data)
            }
            None => continue,
        };
        final_pushed_bytes += temp_slot_size(&pushed_ty);
        arg_types.push(pushed_ty);
    }

    if sig.variadic.is_some() {
        let variadic_ty = if variadic_sources.is_empty() && prefix_variadic_tail.is_none() {
            emit_empty_variadic_array_arg("empty variadic array", emitter)
        } else {
            emit_variadic_array_arg_from_sources(
                &variadic_sources,
                prefix_variadic_tail.as_ref(),
                source_temp_types,
                final_pushed_bytes,
                emitter,
                ctx,
                data,
            )
        };
        arg_types.push(variadic_ty);
    }

    EmittedCallArgs {
        arg_types,
        source_temp_bytes,
    }
}
