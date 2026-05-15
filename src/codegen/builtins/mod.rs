//! Purpose:
//! Routes normalized PHP builtin calls from expression codegen into category dispatchers.
//! Owns shared named/spread argument lowering before builtin-specific emitters see arguments.
//!
//! Called from:
//! - `crate::codegen::expr::emit_expr()`.
//!
//! Key details:
//! - Builtin names arrive after type/catalog resolution, including PHP case-insensitive and namespace fallback behavior.

mod arrays;
mod io;
mod math;
mod pointers;
mod spl;
mod strings;
mod system;
mod types;

use super::context::Context;
use super::data_section::DataSection;
use super::emit::Emitter;
use crate::parser::ast::Expr;
use crate::span::Span;
use crate::types::PhpType;

/// Emit code for a built-in function call.
/// Returns Some(return_type) if the function is a known built-in, None otherwise.
pub fn emit_builtin_call(
    name: &str,
    args: &[Expr],
    call_span: Span,
    emitter: &mut Emitter,
    ctx: &mut Context,
    data: &mut DataSection,
) -> Option<PhpType> {
    let normalized_args;
    let args = if let Some(sig) = crate::types::builtin_call_sig(name) {
        let regular_param_count =
            crate::codegen::expr::calls::args::regular_param_count(Some(&sig), args.len());
        let normalized = if crate::codegen::expr::calls::args::has_named_args(args) {
            crate::codegen::expr::calls::args::preevaluate_named_call_args_to_temps(
                &sig,
                args,
                call_span,
                regular_param_count,
                true,
                emitter,
                ctx,
                data,
            )
        } else {
            crate::codegen::expr::calls::args::normalize_builtin_call_args_with_checks(&sig, args)
        };
        crate::codegen::expr::calls::args::emit_spread_length_checks(
            &normalized.spread_length_checks,
            emitter,
            ctx,
            data,
        );
        normalized_args = normalized.args;
        normalized_args.as_slice()
    } else {
        args
    };

    system::emit(name, args, emitter, ctx, data)
        .or_else(|| strings::emit(name, args, emitter, ctx, data))
        .or_else(|| arrays::emit(name, args, emitter, ctx, data))
        .or_else(|| math::emit(name, args, emitter, ctx, data))
        .or_else(|| types::emit(name, args, emitter, ctx, data))
        .or_else(|| io::emit(name, args, emitter, ctx, data))
        .or_else(|| pointers::emit(name, args, emitter, ctx, data))
        .or_else(|| spl::emit(name, args, emitter, ctx, data))
}
