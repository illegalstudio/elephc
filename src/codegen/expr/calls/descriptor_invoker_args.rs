//! Purpose:
//! Builds runtime callable-descriptor invoker argument containers for direct expression calls.
//! Keeps synthetic indexed/associative arrays out of the indirect-call dispatcher.
//!
//! Called from:
//! - `crate::codegen::expr::calls::indirect`
//!
//! Key details:
//! - Descriptor invokers consume caller-visible argument containers and apply signature metadata themselves.
//! - Named+spread calls are lowered to a Mixed associative hash so defaults, parameter names, and variadics
//!   stay behind the descriptor invoker instead of being normalized at the callsite.

use crate::codegen::builtins::arrays::call_user_func_array;
use crate::codegen::builtins::arrays::descriptor_arg_builder;
use crate::codegen::context::{Context, HeapOwnership};
use crate::codegen::data_section::DataSection;
use crate::codegen::emit::Emitter;
use crate::codegen::{abi, platform::Arch};
use crate::parser::ast::{Expr, ExprKind};
use crate::span::Span;
use crate::types::{call_args, FunctionSig, PhpType};

/// Emits the argument container passed to a descriptor invoker for a direct expression call.
pub(super) fn emit_descriptor_invoker_arg_array(
    args_exprs: &[Expr],
    sig: Option<&FunctionSig>,
    span: Span,
    emitter: &mut Emitter,
    ctx: &mut Context,
    data: &mut DataSection,
) -> PhpType {
    let encode_ref_markers = should_encode_invoker_ref_args(sig, args_exprs);
    if has_explicit_named_and_spread(args_exprs) {
        if let Some(sig) = sig {
            return emit_named_spread_invoker_arg_hash(
                args_exprs,
                sig,
                span,
                encode_ref_markers,
                emitter,
                ctx,
                data,
            );
        }
        return emit_untyped_named_spread_invoker_arg_hash(
            args_exprs,
            span,
            encode_ref_markers,
            emitter,
            ctx,
            data,
        );
    }

    if let Some(spread_inner) = single_spread_inner(args_exprs) {
        return super::super::emit_expr(spread_inner, emitter, ctx, data);
    }

    if has_explicit_named(args_exprs) && encode_ref_markers {
        return emit_named_invoker_arg_hash(args_exprs, true, emitter, ctx, data);
    }

    if plain_positional_args(args_exprs) && encode_ref_markers {
        return descriptor_arg_builder::emit_indexed_invoker_arg_array(
            args_exprs,
            true,
            emitter,
            ctx,
            data,
        );
    }

    if has_spread(args_exprs) {
        if let Some(ty) = descriptor_arg_builder::emit_positional_spread_invoker_arg_array(
            &[],
            args_exprs,
            sig,
            encode_ref_markers,
            emitter,
            ctx,
            data,
        ) {
            return ty;
        }
    }

    let arg_array = descriptor_invoker_arg_array_expr(args_exprs, span);
    super::super::emit_expr(&arg_array, emitter, ctx, data)
}

/// Emits a descriptor-invoker argument container with a saved object receiver in descriptor slot zero.
#[allow(clippy::too_many_arguments)]
pub(super) fn emit_descriptor_invoker_arg_array_with_saved_object_prefix(
    object_stack_offset: usize,
    args_exprs: &[Expr],
    sig: Option<&FunctionSig>,
    span: Span,
    emitter: &mut Emitter,
    ctx: &mut Context,
    data: &mut DataSection,
) -> PhpType {
    let encode_ref_markers = should_encode_invoker_ref_args(sig, args_exprs);
    if has_explicit_named_and_spread(args_exprs) {
        if let Some(sig) = sig {
            return emit_named_spread_invoker_arg_hash_with_saved_object_prefix(
                object_stack_offset,
                args_exprs,
                sig,
                span,
                encode_ref_markers,
                emitter,
                ctx,
                data,
            );
        }
    }

    if has_explicit_named(args_exprs) {
        return emit_named_invoker_arg_hash_with_saved_object_prefix(
            object_stack_offset,
            args_exprs,
            encode_ref_markers,
            emitter,
            ctx,
            data,
        );
    }

    if has_spread(args_exprs) {
        if let Some(ty) =
            descriptor_arg_builder::emit_positional_spread_invoker_arg_array_with_saved_object_prefix(
                object_stack_offset,
                args_exprs,
                sig,
                encode_ref_markers,
                emitter,
                ctx,
                data,
            )
        {
            return ty;
        }
    }

    descriptor_arg_builder::emit_indexed_invoker_arg_array_with_saved_object_prefix(
        object_stack_offset,
        args_exprs,
        sig,
        encode_ref_markers,
        emitter,
        ctx,
        data,
    )
}

/// Returns true when a direct descriptor call mixes explicit named args with spread args.
fn has_explicit_named_and_spread(args_exprs: &[Expr]) -> bool {
    let has_spread = args_exprs
        .iter()
        .any(|arg| matches!(arg.kind, ExprKind::Spread(_)));
    has_explicit_named(args_exprs) && has_spread
}

/// Returns true when any descriptor-call argument uses named-argument syntax.
fn has_explicit_named(args_exprs: &[Expr]) -> bool {
    args_exprs
        .iter()
        .any(|arg| matches!(arg.kind, ExprKind::NamedArg { .. }))
}

/// Returns true when any direct descriptor-call argument uses spread syntax.
fn has_spread(args_exprs: &[Expr]) -> bool {
    args_exprs
        .iter()
        .any(|arg| matches!(arg.kind, ExprKind::Spread(_)))
}

/// Returns true when the descriptor call has only positional, non-spread arguments.
fn plain_positional_args(args_exprs: &[Expr]) -> bool {
    args_exprs
        .iter()
        .all(|arg| !matches!(arg.kind, ExprKind::NamedArg { .. } | ExprKind::Spread(_)))
}

/// Returns whether variable arguments should be encoded for runtime by-ref decisions.
fn should_encode_invoker_ref_args(sig: Option<&FunctionSig>, args_exprs: &[Expr]) -> bool {
    if !args_exprs.iter().any(arg_value_is_variable) {
        return false;
    }
    sig.is_none_or(|sig| sig.ref_params.iter().any(|is_ref| *is_ref))
}

/// Returns true when an argument's runtime value is sourced from a variable.
fn arg_value_is_variable(arg: &Expr) -> bool {
    match &arg.kind {
        ExprKind::Variable(_) => true,
        ExprKind::NamedArg { value, .. } => matches!(value.kind, ExprKind::Variable(_)),
        _ => false,
    }
}

/// Returns the spread source when the entire descriptor call is `(...$args)`.
fn single_spread_inner(args_exprs: &[Expr]) -> Option<&Expr> {
    if let [arg] = args_exprs {
        if let ExprKind::Spread(inner) = &arg.kind {
            return Some(inner);
        }
    }
    None
}

/// Builds the synthetic argument container passed to a descriptor invoker.
fn descriptor_invoker_arg_array_expr(args_exprs: &[Expr], span: Span) -> Expr {
    let has_explicit_named = args_exprs
        .iter()
        .any(|arg| matches!(arg.kind, ExprKind::NamedArg { .. }));
    if !has_explicit_named {
        return Expr::new(ExprKind::ArrayLiteral(args_exprs.to_vec()), span);
    }

    let mut next_positional_key = 0i64;
    let mut entries = Vec::with_capacity(args_exprs.len());
    for arg in args_exprs {
        match &arg.kind {
            ExprKind::NamedArg { name, value } => {
                entries.push((
                    Expr::new(ExprKind::StringLiteral(name.clone()), arg.span),
                    (**value).clone(),
                ));
            }
            _ => {
                entries.push((
                    Expr::new(ExprKind::IntLiteral(next_positional_key), arg.span),
                    arg.clone(),
                ));
                next_positional_key += 1;
            }
        }
    }

    Expr::new(ExprKind::ArrayLiteralAssoc(entries), span)
}

/// Emits a Mixed associative hash for direct descriptor calls with spread prefixes and named suffixes.
fn emit_named_spread_invoker_arg_hash(
    args_exprs: &[Expr],
    sig: &FunctionSig,
    span: Span,
    encode_ref_markers: bool,
    emitter: &mut Emitter,
    ctx: &mut Context,
    data: &mut DataSection,
) -> PhpType {
    let regular_param_count = super::args::regular_param_count(Some(sig), args_exprs.len());
    let assoc_spread_sources = vec![false; args_exprs.len()];
    let plan = call_args::plan_call_args_with_regular_param_count_and_assoc_spreads(
        sig,
        args_exprs,
        span,
        regular_param_count,
        false,
        true,
        &assoc_spread_sources,
    )
    .expect("codegen received invalid descriptor named+spread arguments after type checking");
    let first_named_pos = plan
        .first_named_pos
        .expect("named+spread descriptor plan must contain a named suffix");

    emitter.comment("descriptor invoker named+spread argument hash");
    emit_descriptor_prefix_as_mixed_hash(&plan, sig, span, encode_ref_markers, emitter, ctx, data);
    abi::emit_push_reg(emitter, abi::int_result_reg(emitter));                  // keep the descriptor argument hash alive while named suffix entries are inserted

    for arg in plan.source_args.iter().skip(first_named_pos) {
        if let ExprKind::NamedArg { name, value } = &arg.kind {
            emit_named_suffix_entry(name, value, encode_ref_markers, emitter, ctx, data);
        }
    }

    abi::emit_pop_reg(emitter, abi::int_result_reg(emitter));                   // return the completed descriptor argument hash
    PhpType::AssocArray {
        key: Box::new(PhpType::Mixed),
        value: Box::new(PhpType::Mixed),
    }
}

/// Emits a Mixed hash for named+spread descriptor calls with a saved object receiver prefix.
#[allow(clippy::too_many_arguments)]
fn emit_named_spread_invoker_arg_hash_with_saved_object_prefix(
    object_stack_offset: usize,
    args_exprs: &[Expr],
    sig: &FunctionSig,
    span: Span,
    encode_ref_markers: bool,
    emitter: &mut Emitter,
    ctx: &mut Context,
    data: &mut DataSection,
) -> PhpType {
    let mut source_args = Vec::with_capacity(args_exprs.len() + 1);
    source_args.push(receiver_prefix_placeholder_expr(span));
    source_args.extend(args_exprs.iter().cloned());
    let regular_param_count = super::args::regular_param_count(Some(sig), source_args.len());
    let assoc_spread_sources = vec![false; source_args.len()];
    let plan = call_args::plan_call_args_with_regular_param_count_and_assoc_spreads(
        sig,
        &source_args,
        span,
        regular_param_count,
        false,
        true,
        &assoc_spread_sources,
    )
    .expect("codegen received invalid receiver-prefixed descriptor named+spread arguments after type checking");
    let first_named_pos = plan
        .first_named_pos
        .expect("receiver-prefixed named+spread descriptor plan must contain a named suffix");

    emitter.comment("descriptor invoker receiver-prefixed named+spread argument hash");
    emit_receiver_prefixed_descriptor_prefix_as_mixed_hash(
        object_stack_offset,
        &plan.source_args[1..first_named_pos],
        sig,
        span,
        encode_ref_markers,
        emitter,
        ctx,
        data,
    );
    abi::emit_push_reg(emitter, abi::int_result_reg(emitter));                  // keep the receiver-prefixed descriptor argument hash alive while named suffix entries are inserted

    for arg in plan.source_args.iter().skip(first_named_pos) {
        if let ExprKind::NamedArg { name, value } = &arg.kind {
            emit_named_suffix_entry(name, value, encode_ref_markers, emitter, ctx, data);
        }
    }

    abi::emit_pop_reg(emitter, abi::int_result_reg(emitter));                   // return the completed receiver-prefixed descriptor argument hash
    PhpType::AssocArray {
        key: Box::new(PhpType::Mixed),
        value: Box::new(PhpType::Mixed),
    }
}

/// Emits the positional prefix of a named+spread descriptor call as a Mixed hash.
fn emit_descriptor_prefix_as_mixed_hash(
    plan: &call_args::CallArgPlan,
    sig: &FunctionSig,
    span: Span,
    encode_ref_markers: bool,
    emitter: &mut Emitter,
    ctx: &mut Context,
    data: &mut DataSection,
) {
    let prefix_expr = plan.positional_prefix_expr(span);
    let first_named_pos = plan
        .first_named_pos
        .expect("descriptor named+spread prefix must know the first named source position");
    emit_descriptor_prefix_args_as_mixed_hash(
        prefix_expr.as_ref(),
        &plan.source_args[..first_named_pos],
        Some(sig),
        span,
        encode_ref_markers,
        emitter,
        ctx,
        data,
    );
}

/// Emits positional-prefix args as a Mixed hash with a saved object receiver in numeric key zero.
#[allow(clippy::too_many_arguments)]
fn emit_receiver_prefixed_descriptor_prefix_as_mixed_hash(
    object_stack_offset: usize,
    prefix_args: &[Expr],
    sig: &FunctionSig,
    span: Span,
    encode_ref_markers: bool,
    emitter: &mut Emitter,
    ctx: &mut Context,
    data: &mut DataSection,
) {
    let prefix_ty = if prefix_args.iter().any(|arg| matches!(arg.kind, ExprKind::Spread(_))) {
        descriptor_arg_builder::emit_positional_spread_invoker_arg_array_with_saved_object_prefix(
            object_stack_offset,
            prefix_args,
            Some(sig),
            encode_ref_markers,
            emitter,
            ctx,
            data,
        )
        .unwrap_or_else(|| {
            descriptor_arg_builder::emit_indexed_invoker_arg_array_with_saved_object_prefix(
                object_stack_offset,
                prefix_args,
                Some(sig),
                encode_ref_markers,
                emitter,
                ctx,
                data,
            )
        })
    } else {
        let _ = span;
        descriptor_arg_builder::emit_indexed_invoker_arg_array_with_saved_object_prefix(
            object_stack_offset,
            prefix_args,
            Some(sig),
            encode_ref_markers,
            emitter,
            ctx,
            data,
        )
    };
    emit_indexed_prefix_as_mixed_hash(&prefix_ty, emitter);
}

/// Emits positional-prefix source arguments as a Mixed hash, preserving variable ref markers when needed.
#[allow(clippy::too_many_arguments)]
fn emit_descriptor_prefix_args_as_mixed_hash(
    prefix_expr: Option<&Expr>,
    prefix_args: &[Expr],
    sig: Option<&FunctionSig>,
    span: Span,
    encode_ref_markers: bool,
    emitter: &mut Emitter,
    ctx: &mut Context,
    data: &mut DataSection,
) {
    if prefix_args.is_empty() {
        emit_descriptor_prefix_expr_as_mixed_hash(None, emitter, ctx, data);
        return;
    }

    if encode_ref_markers {
        if let Some(prefix_ty) = descriptor_arg_builder::emit_positional_spread_invoker_arg_array(
            &[],
            prefix_args,
            sig,
            true,
            emitter,
            ctx,
            data,
        ) {
            emit_indexed_prefix_as_mixed_hash(&prefix_ty, emitter);
            return;
        }

        if plain_positional_args(prefix_args) {
            let prefix_ty = descriptor_arg_builder::emit_indexed_invoker_arg_array(
                prefix_args,
                true,
                emitter,
                ctx,
                data,
            );
            emit_indexed_prefix_as_mixed_hash(&prefix_ty, emitter);
            return;
        }
    }

    if let Some(prefix_expr) = prefix_expr {
        emit_descriptor_prefix_expr_as_mixed_hash(Some(prefix_expr), emitter, ctx, data);
    } else {
        let fallback_prefix = Expr::new(ExprKind::ArrayLiteral(prefix_args.to_vec()), span);
        emit_descriptor_prefix_expr_as_mixed_hash(Some(&fallback_prefix), emitter, ctx, data);
    }
}

/// Emits an optional positional prefix expression as a Mixed hash.
fn emit_descriptor_prefix_expr_as_mixed_hash(
    prefix_expr: Option<&Expr>,
    emitter: &mut Emitter,
    ctx: &mut Context,
    data: &mut DataSection,
) {
    let Some(prefix_expr) = prefix_expr else {
        crate::codegen::expr::arrays::emit_empty_assoc_array_literal(
            PhpType::Mixed,
            PhpType::Mixed,
            emitter,
        );
        return;
    };

    let prefix_ty = super::super::emit_expr(&prefix_expr, emitter, ctx, data);
    match prefix_ty {
        PhpType::AssocArray { .. } => {
            call_user_func_array::emit_clone_assoc_array_for_invoker(
                abi::int_result_reg(emitter),
                emitter,
            );
        }
        PhpType::Array(_) => {
            emit_indexed_prefix_as_mixed_hash(&prefix_ty, emitter);
        }
        PhpType::Mixed | PhpType::Union(_) => {
            emit_mixed_prefix_as_hash_or_abort(emitter, ctx, data);
        }
        _ => {
            crate::codegen::expr::arrays::emit_empty_assoc_array_literal(
                PhpType::Mixed,
                PhpType::Mixed,
                emitter,
            );
        }
    }
}

/// Emits a Mixed hash for named+spread descriptor calls without a local signature.
fn emit_untyped_named_spread_invoker_arg_hash(
    args_exprs: &[Expr],
    span: Span,
    encode_ref_markers: bool,
    emitter: &mut Emitter,
    ctx: &mut Context,
    data: &mut DataSection,
) -> PhpType {
    let first_named_pos = args_exprs
        .iter()
        .position(|arg| matches!(arg.kind, ExprKind::NamedArg { .. }))
        .expect("named+spread descriptor call must contain a named suffix");
    let prefix_expr = if first_named_pos == 0 {
        None
    } else {
        Some(Expr::new(
            ExprKind::ArrayLiteral(args_exprs[..first_named_pos].to_vec()),
            span,
        ))
    };

    emitter.comment("descriptor invoker untyped named+spread argument hash");
    emit_descriptor_prefix_args_as_mixed_hash(
        prefix_expr.as_ref(),
        &args_exprs[..first_named_pos],
        None,
        span,
        encode_ref_markers,
        emitter,
        ctx,
        data,
    );
    abi::emit_push_reg(emitter, abi::int_result_reg(emitter));                  // keep the descriptor argument hash alive while untyped named suffix entries are inserted

    for arg in args_exprs.iter().skip(first_named_pos) {
        if let ExprKind::NamedArg { name, value } = &arg.kind {
            emit_named_suffix_entry(name, value, encode_ref_markers, emitter, ctx, data);
        }
    }

    abi::emit_pop_reg(emitter, abi::int_result_reg(emitter));                   // return the completed untyped named+spread argument hash
    PhpType::AssocArray {
        key: Box::new(PhpType::Mixed),
        value: Box::new(PhpType::Mixed),
    }
}

/// Emits a Mixed associative hash for descriptor calls that use named arguments.
fn emit_named_invoker_arg_hash(
    args_exprs: &[Expr],
    encode_ref_markers: bool,
    emitter: &mut Emitter,
    ctx: &mut Context,
    data: &mut DataSection,
) -> PhpType {
    emitter.comment("descriptor invoker named argument hash");
    emit_descriptor_prefix_expr_as_mixed_hash(None, emitter, ctx, data);
    abi::emit_push_reg(emitter, abi::int_result_reg(emitter));                  // keep the descriptor named-argument hash alive while entries are inserted

    let mut next_positional_key = 0usize;
    for arg in args_exprs {
        match &arg.kind {
            ExprKind::NamedArg { name, value } => {
                emit_named_suffix_entry(name, value, encode_ref_markers, emitter, ctx, data);
            }
            ExprKind::Spread(_) => {}
            _ => {
                emit_numeric_suffix_entry(
                    next_positional_key,
                    arg,
                    encode_ref_markers,
                    emitter,
                    ctx,
                    data,
                );
                next_positional_key += 1;
            }
        }
    }

    abi::emit_pop_reg(emitter, abi::int_result_reg(emitter));                   // return the completed descriptor named-argument hash
    PhpType::AssocArray {
        key: Box::new(PhpType::Mixed),
        value: Box::new(PhpType::Mixed),
    }
}

/// Emits a Mixed hash for descriptor calls with named args and a saved object receiver prefix.
fn emit_named_invoker_arg_hash_with_saved_object_prefix(
    object_stack_offset: usize,
    args_exprs: &[Expr],
    encode_ref_markers: bool,
    emitter: &mut Emitter,
    ctx: &mut Context,
    data: &mut DataSection,
) -> PhpType {
    emitter.comment("descriptor invoker receiver-prefixed named argument hash");
    emit_descriptor_prefix_expr_as_mixed_hash(None, emitter, ctx, data);
    abi::emit_push_reg(emitter, abi::int_result_reg(emitter));                  // keep the receiver-prefixed descriptor named-argument hash alive while entries are inserted
    emit_saved_object_prefix_numeric_entry(object_stack_offset + 16, emitter);

    let mut next_positional_key = 1usize;
    for arg in args_exprs {
        match &arg.kind {
            ExprKind::NamedArg { name, value } => {
                emit_named_suffix_entry(name, value, encode_ref_markers, emitter, ctx, data);
            }
            ExprKind::Spread(_) => {}
            _ => {
                emit_numeric_suffix_entry(
                    next_positional_key,
                    arg,
                    encode_ref_markers,
                    emitter,
                    ctx,
                    data,
                );
                next_positional_key += 1;
            }
        }
    }

    abi::emit_pop_reg(emitter, abi::int_result_reg(emitter));                   // return the completed receiver-prefixed descriptor named-argument hash
    PhpType::AssocArray {
        key: Box::new(PhpType::Mixed),
        value: Box::new(PhpType::Mixed),
    }
}

/// Converts the current indexed-array prefix result into a Mixed associative hash.
fn emit_indexed_prefix_as_mixed_hash(prefix_ty: &PhpType, emitter: &mut Emitter) {
    abi::emit_push_reg(emitter, abi::int_result_reg(emitter));                  // preserve the evaluated positional prefix while allocating the destination hash
    crate::codegen::expr::arrays::emit_empty_assoc_array_literal(
        PhpType::Mixed,
        PhpType::Mixed,
        emitter,
    );
    emit_hash_array_union_with_saved_right_operand(emitter);
    abi::emit_release_temporary_stack(emitter, 16);                             // discard the preserved positional-prefix array after copying its elements
    if !matches!(prefix_ty, PhpType::Array(elem) if matches!(elem.codegen_repr(), PhpType::Mixed)) {
        if emitter.target.arch == Arch::X86_64 {
            emitter.instruction("mov rdi, rax");                                // pass the merged descriptor argument hash to the Mixed conversion helper
        }
        abi::emit_call_label(emitter, "__rt_hash_to_mixed");
    }
}

/// Merges the current hash result with the indexed array saved at the top of the temporary stack.
fn emit_hash_array_union_with_saved_right_operand(emitter: &mut Emitter) {
    match emitter.target.arch {
        Arch::AArch64 => {
            abi::emit_load_temporary_stack_slot(emitter, "x1", 0);
            abi::emit_call_label(emitter, "__rt_hash_array_union");
        }
        Arch::X86_64 => {
            emitter.instruction("mov rdi, rax");                                // pass the empty descriptor argument hash as the left union operand
            abi::emit_load_temporary_stack_slot(emitter, "rsi", 0);
            abi::emit_call_label(emitter, "__rt_hash_array_union");
        }
    }
}

/// Boxes a saved object pointer and inserts it as numeric key zero in the current hash.
fn emit_saved_object_prefix_numeric_entry(object_stack_offset: usize, emitter: &mut Emitter) {
    let object_reg = abi::secondary_scratch_reg(emitter);
    let zero_reg = abi::tertiary_scratch_reg(emitter);
    let tag_reg = abi::symbol_scratch_reg(emitter);
    abi::emit_load_temporary_stack_slot(emitter, object_reg, object_stack_offset);
    abi::emit_load_int_immediate(emitter, zero_reg, 0);
    abi::emit_load_int_immediate(
        emitter,
        tag_reg,
        crate::codegen::runtime_value_tag(&PhpType::Object(String::new())) as i64,
    );
    crate::codegen::emit_box_runtime_payload_as_mixed(emitter, tag_reg, object_reg, zero_reg);
    emit_hash_set_current_mixed_numeric_suffix(0, emitter);
}

/// Builds a placeholder source argument that occupies descriptor slot zero during planning.
fn receiver_prefix_placeholder_expr(span: Span) -> Expr {
    Expr::new(ExprKind::IntLiteral(0), span)
}

/// Converts a runtime Mixed prefix container into a Mixed hash or aborts on invalid shape.
fn emit_mixed_prefix_as_hash_or_abort(
    emitter: &mut Emitter,
    ctx: &mut Context,
    data: &mut DataSection,
) {
    let tag_reg = abi::secondary_scratch_reg(emitter);
    let payload_reg = abi::tertiary_scratch_reg(emitter);
    let indexed_label = ctx.next_label("descriptor_prefix_indexed");
    let assoc_label = ctx.next_label("descriptor_prefix_assoc");
    let done_label = ctx.next_label("descriptor_prefix_done");
    let indexed_ty = PhpType::Array(Box::new(PhpType::Mixed));
    let assoc_ty = PhpType::AssocArray {
        key: Box::new(PhpType::Mixed),
        value: Box::new(PhpType::Mixed),
    };

    abi::emit_load_from_address(emitter, tag_reg, abi::int_result_reg(emitter), 0);
    abi::emit_load_from_address(emitter, payload_reg, abi::int_result_reg(emitter), 8);
    abi::emit_push_reg(emitter, payload_reg);                                   // preserve the unboxed mixed prefix payload while dispatching by container tag
    emit_branch_if_mixed_tag(
        tag_reg,
        crate::codegen::runtime_value_tag(&indexed_ty),
        &indexed_label,
        emitter,
    );
    emit_branch_if_mixed_tag(
        tag_reg,
        crate::codegen::runtime_value_tag(&assoc_ty),
        &assoc_label,
        emitter,
    );
    emit_invalid_descriptor_prefix_abort(emitter, data);

    emitter.label(&indexed_label);
    abi::emit_load_temporary_stack_slot(emitter, abi::int_result_reg(emitter), 0);
    call_user_func_array::emit_clone_indexed_array_for_invoker_with_runtime_tag(
        abi::int_result_reg(emitter),
        emitter,
    );
    emit_indexed_prefix_as_mixed_hash(&indexed_ty, emitter);
    abi::emit_jump(emitter, &done_label);

    emitter.label(&assoc_label);
    abi::emit_load_temporary_stack_slot(emitter, abi::int_result_reg(emitter), 0);
    call_user_func_array::emit_clone_assoc_array_for_invoker(abi::int_result_reg(emitter), emitter);

    emitter.label(&done_label);
    abi::emit_release_temporary_stack(emitter, 16);                             // discard the preserved mixed prefix payload after normalization
}

/// Branches to `label` when `tag_reg` matches the expected Mixed runtime tag.
fn emit_branch_if_mixed_tag(tag_reg: &str, expected_tag: u8, label: &str, emitter: &mut Emitter) {
    match emitter.target.arch {
        Arch::AArch64 => {
            emitter.instruction(&format!("cmp {}, #{}", tag_reg, expected_tag)); // compare the mixed prefix payload tag with the expected container shape
            emitter.instruction(&format!("b.eq {}", label));                    // handle this prefix container shape when the tag matches
        }
        Arch::X86_64 => {
            emitter.instruction(&format!("cmp {}, {}", tag_reg, expected_tag)); // compare the mixed prefix payload tag with the expected container shape
            emitter.instruction(&format!("je {}", label));                      // handle this prefix container shape when the tag matches
        }
    }
}

/// Emits the fatal diagnostic for an invalid mixed prefix argument container.
fn emit_invalid_descriptor_prefix_abort(emitter: &mut Emitter, data: &mut DataSection) {
    let (message_label, message_len) = data.add_string(
        b"Fatal error: callable descriptor named-spread prefix must be an array\n",
    );
    match emitter.target.arch {
        Arch::AArch64 => {
            emitter.instruction("mov x0, #2");                                  // write the descriptor prefix diagnostic to stderr
            emitter.adrp("x1", &message_label);
            emitter.add_lo12("x1", "x1", &message_label);
            emitter.instruction(&format!("mov x2, #{}", message_len));          // pass the descriptor prefix diagnostic byte length to write()
            emitter.syscall(4);
            abi::emit_exit(emitter, 1);
        }
        Arch::X86_64 => {
            emitter.instruction("mov edi, 2");                                  // write the descriptor prefix diagnostic to stderr
            abi::emit_symbol_address(emitter, "rsi", &message_label);
            emitter.instruction(&format!("mov edx, {}", message_len));          // pass the descriptor prefix diagnostic byte length to write()
            emitter.instruction("mov eax, 1");                                  // Linux x86_64 syscall 1 = write
            emitter.instruction("syscall");                                     // emit the descriptor prefix diagnostic
            abi::emit_exit(emitter, 1);
        }
    }
}

/// Inserts one named suffix value into the descriptor invoker argument hash.
fn emit_named_suffix_entry(
    name: &str,
    value: &Expr,
    encode_ref_markers: bool,
    emitter: &mut Emitter,
    ctx: &mut Context,
    data: &mut DataSection,
) {
    emit_mixed_invoker_hash_value(value, encode_ref_markers, emitter, ctx, data);
    emit_hash_set_current_mixed_named_suffix(name, emitter, data);
}

/// Inserts one numeric positional-prefix value into the descriptor invoker argument hash.
fn emit_numeric_suffix_entry(
    index: usize,
    value: &Expr,
    encode_ref_markers: bool,
    emitter: &mut Emitter,
    ctx: &mut Context,
    data: &mut DataSection,
) {
    emit_mixed_invoker_hash_value(value, encode_ref_markers, emitter, ctx, data);
    emit_hash_set_current_mixed_numeric_suffix(index, emitter);
}

/// Emits one descriptor-invoker hash value as boxed Mixed or a variable ref-cell marker.
fn emit_mixed_invoker_hash_value(
    value: &Expr,
    encode_ref_markers: bool,
    emitter: &mut Emitter,
    ctx: &mut Context,
    data: &mut DataSection,
) {
    if encode_ref_markers {
        if let ExprKind::Variable(var_name) = &value.kind {
            if !super::args::emit_ref_arg_variable_address(
                var_name,
                "descriptor invoker named arg",
                emitter,
                ctx,
            ) {
                panic!("descriptor invoker named argument variable not found");
            }
            descriptor_arg_builder::emit_box_current_ref_arg_address_for_invoker(
                var_name,
                emitter,
                ctx,
            );
            return;
        }
    }

    let mut value_ty = super::super::emit_expr(value, emitter, ctx, data);
    let boxed_iterable =
        crate::codegen::emit_box_iterable_value_for_mixed_container(emitter, &mut value_ty);
    if !matches!(value_ty, PhpType::Mixed | PhpType::Union(_)) {
        crate::codegen::emit_box_current_expr_value_as_mixed_for_container(
            emitter,
            value,
            &value_ty,
        );
    } else if !boxed_iterable {
        retain_borrowed_mixed_named_suffix(emitter, value, &value_ty);
    }
}

/// Retains a borrowed Mixed suffix value before storing it in the descriptor argument hash.
fn retain_borrowed_mixed_named_suffix(emitter: &mut Emitter, value: &Expr, value_ty: &PhpType) {
    if value_ty.codegen_repr().is_refcounted()
        && super::super::expr_result_heap_ownership(value) != HeapOwnership::Owned
    {
        abi::emit_incref_if_refcounted(emitter, &value_ty.codegen_repr());
    }
}

/// Calls `__rt_hash_set` to store the current boxed Mixed value under a string key.
fn emit_hash_set_current_mixed_named_suffix(
    name: &str,
    emitter: &mut Emitter,
    data: &mut DataSection,
) {
    let (key_label, key_len) = data.add_string(name.as_bytes());
    let result_reg = abi::int_result_reg(emitter);
    match emitter.target.arch {
        Arch::AArch64 => {
            emitter.instruction("mov x3, x0");                                  // pass the boxed named argument as the hash value payload
            emitter.instruction("mov x4, xzr");                                 // boxed Mixed hash entries do not use a high payload word
            abi::emit_load_int_immediate(emitter, "x5", crate::codegen::runtime_value_tag(&PhpType::Mixed) as i64);
            abi::emit_load_temporary_stack_slot(emitter, "x0", 0);
            emitter.adrp("x1", &key_label);
            emitter.add_lo12("x1", "x1", &key_label);
            abi::emit_load_int_immediate(emitter, "x2", key_len as i64);
            abi::emit_call_label(emitter, "__rt_hash_set");
            abi::emit_store_to_address(emitter, result_reg, "sp", 0);
        }
        Arch::X86_64 => {
            emitter.instruction("mov rcx, rax");                                // pass the boxed named argument as the hash value payload
            abi::emit_load_int_immediate(emitter, "r8", 0);
            abi::emit_load_int_immediate(emitter, "r9", crate::codegen::runtime_value_tag(&PhpType::Mixed) as i64);
            abi::emit_load_temporary_stack_slot(emitter, "rdi", 0);
            abi::emit_symbol_address(emitter, "rsi", &key_label);
            abi::emit_load_int_immediate(emitter, "rdx", key_len as i64);
            abi::emit_call_label(emitter, "__rt_hash_set");
            abi::emit_store_to_address(emitter, result_reg, "rsp", 0);
        }
    }
}

/// Calls `__rt_hash_set` to store the current boxed Mixed value under an integer key.
fn emit_hash_set_current_mixed_numeric_suffix(index: usize, emitter: &mut Emitter) {
    let result_reg = abi::int_result_reg(emitter);
    match emitter.target.arch {
        Arch::AArch64 => {
            emitter.instruction("mov x3, x0");                                  // pass the boxed positional argument as the hash value payload
            emitter.instruction("mov x4, xzr");                                 // boxed Mixed hash entries do not use a high payload word
            abi::emit_load_int_immediate(emitter, "x5", crate::codegen::runtime_value_tag(&PhpType::Mixed) as i64);
            abi::emit_load_temporary_stack_slot(emitter, "x0", 0);
            abi::emit_load_int_immediate(emitter, "x1", index as i64);
            abi::emit_load_int_immediate(emitter, "x2", -1);
            abi::emit_call_label(emitter, "__rt_hash_set");
            abi::emit_store_to_address(emitter, result_reg, "sp", 0);
        }
        Arch::X86_64 => {
            emitter.instruction("mov rcx, rax");                                // pass the boxed positional argument as the hash value payload
            abi::emit_load_int_immediate(emitter, "r8", 0);
            abi::emit_load_int_immediate(emitter, "r9", crate::codegen::runtime_value_tag(&PhpType::Mixed) as i64);
            abi::emit_load_temporary_stack_slot(emitter, "rdi", 0);
            abi::emit_load_int_immediate(emitter, "rsi", index as i64);
            abi::emit_load_int_immediate(emitter, "rdx", -1);
            abi::emit_call_label(emitter, "__rt_hash_set");
            abi::emit_store_to_address(emitter, result_reg, "rsp", 0);
        }
    }
}
