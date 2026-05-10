//! Purpose:
//! Lowers direct user-defined and builtin function calls.
//! Resolves the callable shape, prepares arguments, and leaves the call result for expression consumers.
//!
//! Called from:
//! - `crate::codegen::expr::calls`
//!
//! Key details:
//! - Callable metadata and argument signatures must stay synchronized with type checking and runtime dispatch.

use crate::codegen::context::Context;
use crate::codegen::data_section::DataSection;
use crate::codegen::emit::Emitter;
use crate::names::function_symbol;
use crate::parser::ast::Expr;
use crate::types::PhpType;

use super::args;

pub(super) fn emit_function_call(
    name: &str,
    args_exprs: &[Expr],
    emitter: &mut Emitter,
    ctx: &mut Context,
    data: &mut DataSection,
) -> PhpType {
    emitter.comment(&format!("call {}()", name));

    let save_concat_before_args =
        emitter.target.arch == crate::codegen::platform::Arch::X86_64;
    if save_concat_before_args {
        super::super::save_concat_offset_before_nested_call(emitter, ctx);
    }

    let sig = ctx.functions.get(name).cloned();
    let emitted_args = args::emit_pushed_call_args(
        args_exprs,
        sig.as_ref(),
        args::regular_param_count(sig.as_ref(), args_exprs.len()),
        "ref arg",
        false,
        emitter,
        ctx,
        data,
    );
    let arg_types = emitted_args.arg_types;

    let assignments =
        crate::codegen::abi::build_outgoing_arg_assignments_for_target(emitter.target, &arg_types, 0);
    let overflow_bytes = crate::codegen::abi::materialize_outgoing_args(emitter, &assignments);

    let ret_ty = ctx
        .functions
        .get(name)
        .map(|sig| sig.return_type.clone())
        .unwrap_or(PhpType::Void);

    if !save_concat_before_args {
        super::super::save_concat_offset_before_nested_call(emitter, ctx);
    }
    crate::codegen::abi::emit_call_label(emitter, &function_symbol(name));
    if save_concat_before_args {
        crate::codegen::abi::emit_release_temporary_stack(emitter, overflow_bytes);
        crate::codegen::abi::emit_release_temporary_stack(emitter, emitted_args.source_temp_bytes);
        super::super::restore_concat_offset_after_nested_call(emitter, ctx, &ret_ty);
    } else {
        super::super::restore_concat_offset_after_nested_call(emitter, ctx, &ret_ty);
        crate::codegen::abi::emit_release_temporary_stack(emitter, overflow_bytes);
        crate::codegen::abi::emit_release_temporary_stack(emitter, emitted_args.source_temp_bytes);
    }

    ret_ty
}
