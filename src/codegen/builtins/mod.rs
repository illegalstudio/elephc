//! Purpose:
//! Routes normalized PHP builtin calls from expression codegen into category dispatchers.
//! Owns shared named/spread argument lowering before builtin-specific emitters see arguments.
//!
//! Called from:
//! - `crate::codegen::expr::emit_expr()`.
//!
//! Key details:
//! - Builtin names arrive after type/catalog resolution, including PHP case-insensitive and namespace fallback behavior.

pub(crate) mod arrays;
/// Resolves string-literal function names used by callable/introspection builtins.
/// Shares PHP case-insensitive lookup between string-callback and introspection builtins.
/// Include variants, externs, builtins, and user functions stay distinguishable so callers
/// can choose the right lowering path.
pub(crate) mod callable_lookup;
mod io;
mod math;
mod pointers;
mod spl;
mod strings;
mod system;
mod types;

pub(crate) use io::publish_tls_function_pointers;
pub(crate) use strings::hash_crypto;

use super::context::Context;
use super::data_section::DataSection;
use super::emit::Emitter;
use crate::parser::ast::Expr;
use crate::span::Span;
use crate::types::PhpType;

/// Routes a normalized PHP builtin call to its category dispatcher and emits the call.
 ///
 /// Handles named-argument reordering, spread unpacking, and call argument preevaluation
 /// before delegating to the first category `emit()` that recognizes the builtin name.
 ///
 /// Returns `Some(return_type)` if a dispatcher handled the call, or `None` if no
 /// category recognized the name (caller should treat it as a user function call).
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
