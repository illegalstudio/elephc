//! Purpose:
//! Lowers postfix chains containing PHP nullsafe member access as one lazy unit.
//! Keeps the short-circuit path separate from real null values observed later in
//! ordinary chain segments.
//!
//! Called from:
//! - `crate::ir_lower::expr::lower_expr()`.
//!
//! Key details:
//! - `?->` null receivers branch to a shared boxed-null result without lowering
//!   later array indexes, callable arguments, or method arguments.
//! - Ordinary `->` method calls on real null receivers fatal before arguments,
//!   matching PHP's observable evaluation order.

use crate::ir::{BlockId, Immediate, Op, Terminator};
use crate::ir_lower::context::{LoweredValue, LoweringContext};
use crate::parser::ast::{Expr, ExprKind};
use crate::types::PhpType;

use super::{
    branch_to, lower_array_access_from_value, lower_boxed_null,
    lower_dynamic_property_get_from_value, lower_expr, lower_expr_call_from_value,
    lower_method_call_with_receiver, lower_property_get_from_value, property_can_be_uninitialized,
    store_value_into_temp, take_owned_temp, value_is_definitely_null, value_is_nullable,
};

/// Lowers `expr` when it is a postfix chain containing `?->`.
pub(super) fn lower(ctx: &mut LoweringContext<'_, '_>, expr: &Expr) -> Option<LoweredValue> {
    lower_with_missing_warning(ctx, expr, true)
}

/// Lowers a nullsafe postfix chain while configuring native array-miss warnings.
pub(super) fn lower_with_missing_warning(
    ctx: &mut LoweringContext<'_, '_>,
    expr: &Expr,
    warn_on_missing: bool,
) -> Option<LoweredValue> {
    let chain = flatten_nullsafe_postfix_chain(expr)?;
    Some(lower_nullsafe_postfix_chain(
        ctx,
        chain,
        expr,
        warn_on_missing,
    ))
}

/// Flattened left-to-right postfix chain rooted at a base expression.
struct PostfixChain<'a> {
    base: &'a Expr,
    segments: Vec<PostfixSegment<'a>>,
}

/// One property, method, index, or callable segment in a postfix chain.
enum PostfixSegment<'a> {
    Property {
        expr: &'a Expr,
        property: &'a str,
        nullsafe: bool,
    },
    DynamicProperty {
        expr: &'a Expr,
        property: &'a Expr,
        nullsafe: bool,
    },
    Method {
        expr: &'a Expr,
        method: &'a str,
        args: &'a [Expr],
        nullsafe: bool,
    },
    DynamicMethod {
        expr: &'a Expr,
        method: &'a Expr,
        args: &'a [Expr],
        nullsafe: bool,
    },
    Array {
        expr: &'a Expr,
        index: &'a Expr,
    },
    ExprCall {
        expr: &'a Expr,
        args: &'a [Expr],
    },
}

impl PostfixSegment<'_> {
    /// Returns true for `?->` member segments that can short-circuit the chain.
    fn is_nullsafe_member(&self) -> bool {
        matches!(
            self,
            PostfixSegment::Property {
                nullsafe: true,
                ..
            } | PostfixSegment::DynamicProperty {
                nullsafe: true,
                ..
            } | PostfixSegment::Method {
                nullsafe: true,
                ..
            } | PostfixSegment::DynamicMethod {
                nullsafe: true,
                ..
            }
        )
    }
}

/// Flattens a postfix expression when any member segment uses `?->`.
fn flatten_nullsafe_postfix_chain(expr: &Expr) -> Option<PostfixChain<'_>> {
    let mut base = expr;
    let mut segments = Vec::new();

    loop {
        match &base.kind {
            ExprKind::PropertyAccess { object, property } => {
                segments.push(PostfixSegment::Property {
                    expr: base,
                    property,
                    nullsafe: false,
                });
                base = object;
            }
            ExprKind::NullsafePropertyAccess { object, property } => {
                segments.push(PostfixSegment::Property {
                    expr: base,
                    property,
                    nullsafe: true,
                });
                base = object;
            }
            ExprKind::DynamicPropertyAccess { object, property } => {
                segments.push(PostfixSegment::DynamicProperty {
                    expr: base,
                    property,
                    nullsafe: false,
                });
                base = object;
            }
            ExprKind::NullsafeDynamicPropertyAccess { object, property } => {
                segments.push(PostfixSegment::DynamicProperty {
                    expr: base,
                    property,
                    nullsafe: true,
                });
                base = object;
            }
            ExprKind::MethodCall {
                object,
                method,
                args,
            } => {
                segments.push(PostfixSegment::Method {
                    expr: base,
                    method,
                    args,
                    nullsafe: false,
                });
                base = object;
            }
            ExprKind::NullsafeMethodCall {
                object,
                method,
                args,
            } => {
                segments.push(PostfixSegment::Method {
                    expr: base,
                    method,
                    args,
                    nullsafe: true,
                });
                base = object;
            }
            ExprKind::NullsafeDynamicMethodCall {
                object,
                method,
                args,
            } => {
                segments.push(PostfixSegment::DynamicMethod {
                    expr: base,
                    method,
                    args,
                    nullsafe: true,
                });
                base = object;
            }
            ExprKind::ArrayAccess { array, index } => {
                segments.push(PostfixSegment::Array { expr: base, index });
                base = array;
            }
            ExprKind::ExprCall { callee, args } => {
                segments.push(PostfixSegment::ExprCall { expr: base, args });
                base = callee;
            }
            _ => break,
        }
    }

    if !segments.iter().any(PostfixSegment::is_nullsafe_member) {
        return None;
    }

    segments.reverse();
    Some(PostfixChain { base, segments })
}

/// Lowers a postfix chain containing `?->` as one lazy short-circuiting unit.
fn lower_nullsafe_postfix_chain(
    ctx: &mut LoweringContext<'_, '_>,
    chain: PostfixChain<'_>,
    expr: &Expr,
    warn_on_missing: bool,
) -> LoweredValue {
    let result_type = PhpType::Mixed;
    let temp_name = ctx.declare_owned_hidden_temp(result_type.clone());
    let null_block = ctx.builder.create_named_block("nullsafe.chain.null", Vec::new());
    let done = ctx.builder.create_named_block("nullsafe.chain.done", Vec::new());
    let mut current = Some(lower_expr(ctx, chain.base));

    for segment in chain.segments {
        let Some(value) = current.take() else {
            break;
        };
        if ctx.builder.insertion_block_is_terminated() {
            break;
        }
        current = lower_nullsafe_postfix_segment(
            ctx,
            value,
            segment,
            null_block,
            warn_on_missing,
        );
    }

    if let Some(value) = current {
        if !ctx.builder.insertion_block_is_terminated() {
            store_value_into_temp(ctx, &temp_name, result_type.clone(), value, expr.span);
            branch_to(ctx, done);
        }
    }

    ctx.builder.position_at_end(null_block);
    let null_value = lower_boxed_null(ctx, expr);
    store_value_into_temp(ctx, &temp_name, result_type, null_value, expr.span);
    branch_to(ctx, done);

    ctx.builder.position_at_end(done);
    take_owned_temp(ctx, &temp_name, expr.span)
}

/// Lowers one segment of an already flattened nullsafe postfix chain.
fn lower_nullsafe_postfix_segment(
    ctx: &mut LoweringContext<'_, '_>,
    current: LoweredValue,
    segment: PostfixSegment<'_>,
    null_block: BlockId,
    warn_on_missing: bool,
) -> Option<LoweredValue> {
    match segment {
        PostfixSegment::Property {
            expr,
            property,
            nullsafe,
        } => {
            if nullsafe && !guard_nullsafe_chain_receiver(ctx, current, null_block, expr) {
                return None;
            }
            // Under `??` (`warn_on_missing == false`) a typed property with no default must
            // read as null rather than raise, exactly as `isset()` reads it. The chain already
            // owns a "the answer is null" block for its nullsafe hops, so this reuses that
            // rather than building a second merge: an uninitialized slot short-circuits the
            // whole chain, which is what `$o?->p?->q ?? "d"` means.
            if !warn_on_missing
                && !guard_initialized_chain_property(ctx, current, null_block, property, expr)
            {
                return None;
            }
            Some(lower_property_get_from_value(
                ctx,
                current,
                property,
                Op::PropGet,
                expr,
            ))
        }
        PostfixSegment::DynamicProperty {
            expr,
            property,
            nullsafe,
        } => {
            if nullsafe && !guard_nullsafe_chain_receiver(ctx, current, null_block, expr) {
                return None;
            }
            Some(lower_dynamic_property_get_from_value(ctx, current, property, expr))
        }
        PostfixSegment::Method {
            expr,
            method,
            args,
            nullsafe,
        } => {
            if nullsafe {
                if !guard_nullsafe_chain_receiver(ctx, current, null_block, expr) {
                    return None;
                }
            } else if !guard_regular_method_receiver(ctx, current, method, expr) {
                return None;
            }
            Some(lower_method_call_with_receiver(
                ctx,
                current,
                method,
                args,
                Op::MethodCall,
                expr,
            ))
        }
        PostfixSegment::DynamicMethod {
            expr,
            method,
            args,
            nullsafe,
        } => {
            if nullsafe && !guard_nullsafe_chain_receiver(ctx, current, null_block, expr) {
                return None;
            }
            Some(super::lower_dynamic_method_call_with_receiver(
                ctx, current, method, args, expr,
            ))
        }
        PostfixSegment::Array { expr, index } => {
            Some(lower_array_access_from_value(
                ctx,
                current,
                index,
                expr,
                warn_on_missing,
            ))
        }
        PostfixSegment::ExprCall { expr, args } => {
            Some(lower_expr_call_from_value(ctx, current, args, expr))
        }
    }
}

/// Branches to the chain-null block when a `?->` receiver is null.
/// Sends the chain to its null block when `property` is a typed slot that is still
/// uninitialized, instead of letting the read raise.
///
/// Always returns `true`: the divert is a CONDITIONAL branch, so the caller must go on lowering
/// the segment for the initialized path. That differs from `guard_nullsafe_chain_receiver`, which
/// returns `false` when it diverts UNCONDITIONALLY on a provably-null receiver. Only an UNTYPED
/// property is left unprobed — it is plain null from the start and keeps its exact previous
/// lowering. A default does not exempt a typed property, because `unset()` returns it to the
/// uninitialized state whatever its default.
fn guard_initialized_chain_property(
    ctx: &mut LoweringContext<'_, '_>,
    current: LoweredValue,
    null_block: BlockId,
    property: &str,
    expr: &Expr,
) -> bool {
    if !property_can_be_uninitialized(ctx, current.value, property) {
        return true;
    }
    let data = ctx.intern_string(property);
    let initialized = ctx.emit_value(
        Op::PropInitialized,
        vec![current.value],
        Some(Immediate::Data(data)),
        PhpType::Bool,
        Op::PropInitialized.default_effects(),
        Some(expr.span),
    );
    let continue_block = ctx
        .builder
        .create_named_block("nullsafe.chain.initialized", Vec::new());
    // Diverting to the chain's null block skips every later segment, including whatever would
    // have consumed this receiver — so an OWNING receiver has to be released on the way out,
    // exactly as the null-receiver guard below does. `mk()?->p?->c ?? "none"` leaked one object
    // per call without this.
    let cleanup_block = ctx
        .value_is_owning_temporary(current)
        .then(|| ctx.builder.create_named_block("nullsafe.chain.uninit_cleanup", Vec::new()));
    ctx.builder.terminate(Terminator::CondBr {
        cond: initialized.value,
        then_target: continue_block,
        then_args: Vec::new(),
        else_target: cleanup_block.unwrap_or(null_block),
        else_args: Vec::new(),
    });
    if let Some(cleanup_block) = cleanup_block {
        ctx.builder.position_at_end(cleanup_block);
        crate::ir_lower::ownership::release_if_owned(ctx, current, Some(expr.span));
        branch_to(ctx, null_block);
    }
    ctx.builder.position_at_end(continue_block);
    true
}

fn guard_nullsafe_chain_receiver(
    ctx: &mut LoweringContext<'_, '_>,
    current: LoweredValue,
    null_block: BlockId,
    expr: &Expr,
) -> bool {
    if value_is_definitely_null(ctx, current.value) {
        branch_to(ctx, null_block);
        return false;
    }
    if !value_is_nullable(ctx, current.value) {
        return true;
    }
    let is_null = ctx.emit_value(
        Op::IsNull,
        vec![current.value],
        None,
        PhpType::Bool,
        Op::IsNull.default_effects(),
        Some(expr.span),
    );
    let continue_block = ctx.builder.create_named_block("nullsafe.chain.cont", Vec::new());
    let cleanup_block = ctx
        .value_is_owning_temporary(current)
        .then(|| ctx.builder.create_named_block("nullsafe.chain.cleanup", Vec::new()));
    ctx.builder.terminate(Terminator::CondBr {
        cond: is_null.value,
        then_target: cleanup_block.unwrap_or(null_block),
        then_args: Vec::new(),
        else_target: continue_block,
        else_args: Vec::new(),
    });
    if let Some(cleanup_block) = cleanup_block {
        ctx.builder.position_at_end(cleanup_block);
        crate::ir_lower::ownership::release_if_owned(ctx, current, Some(expr.span));
        branch_to(ctx, null_block);
    }
    ctx.builder.position_at_end(continue_block);
    true
}

/// Fatals before argument lowering when an ordinary method call receives null.
fn guard_regular_method_receiver(
    ctx: &mut LoweringContext<'_, '_>,
    current: LoweredValue,
    method: &str,
    expr: &Expr,
) -> bool {
    if value_is_definitely_null(ctx, current.value) {
        terminate_method_call_on_null(ctx, method);
        return false;
    }
    if !value_is_nullable(ctx, current.value) {
        return true;
    }
    let is_null = ctx.emit_value(
        Op::IsNull,
        vec![current.value],
        None,
        PhpType::Bool,
        Op::IsNull.default_effects(),
        Some(expr.span),
    );
    let fatal_block = ctx.builder.create_named_block("method.null.fatal", Vec::new());
    let call_block = ctx.builder.create_named_block("method.non_null.call", Vec::new());
    ctx.builder.terminate(Terminator::CondBr {
        cond: is_null.value,
        then_target: fatal_block,
        then_args: Vec::new(),
        else_target: call_block,
        else_args: Vec::new(),
    });

    ctx.builder.position_at_end(fatal_block);
    terminate_method_call_on_null(ctx, method);

    ctx.builder.position_at_end(call_block);
    true
}

/// Emits the PHP fatal diagnostic for calling a method on null.
fn terminate_method_call_on_null(ctx: &mut LoweringContext<'_, '_>, method: &str) {
    let message = format!("Fatal error: Call to a member function {}() on null\n", method);
    let message = ctx.intern_string(&message);
    ctx.builder.terminate(Terminator::Fatal { message });
}
