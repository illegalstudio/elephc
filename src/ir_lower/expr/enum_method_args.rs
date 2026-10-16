//! Purpose:
//! Binds runtime unpack containers to selected concrete enum method parameters in EIR.
//!
//! Called from:
//! - Concrete instance method argument lowering after enum interface dispatch.
//!
//! Key details:
//! - Shared call planning and descriptor container construction evaluate source arguments once.
//! - Binding validates keys before parameter-order types, defaults and positional shortages.
//! - Ordinary method dispatch still owns reference-return adoption and argument cleanup.
//! - Hidden argument-count and collector slots use the same physical signature contract.

use super::*;
use super::enum_argument_guards::{concat, guard_parameter, guard_variadic_element, throw_message};

/// Materializes a selected enum's runtime unpack without changing its return ABI.
pub(super) fn lower_arguments(
    ctx: &mut LoweringContext<'_, '_>, signature: &FunctionSig, args: &[Expr],
    class: &str, method: &str, span: Span,
) -> Option<Vec<ValueId>> {
    if signature.ref_params.iter().any(|by_ref| *by_ref)
        || !args.iter().any(is_spread_arg)
        || crate::types::call_args::expand_planned_positional_spreads(args).is_some()
    { return None; }
    let plan = crate::types::call_args::plan_call_args_with_regular_param_count_and_assoc_spreads(
        signature, args, span, crate::types::call_args::regular_param_count(signature),
        false, signature.variadic.is_some() && !crate::func_args::sig_collects_surplus_args(signature),
        &assoc_spread_sources(ctx, args),
    ).expect("selected enum runtime binding consumes the validated shared plan");
    let container = lower_named_descriptor_invoker_arg_container(
        ctx, &plan.source_args, Some(signature), span,
    );
    let container = unbox_array_param_storage(ctx, container, PhpType::AssocArray {
        key: Box::new(PhpType::Mixed), value: Box::new(PhpType::Mixed),
    }, Some(span));
    let container_slot = call_intermediate(ctx, container, span);
    let regular = crate::types::call_args::regular_param_count(signature);
    let argc = ctx.declare_hidden_temp(PhpType::Int);
    store_expr_into_temp(ctx, &argc, PhpType::Int, &literal_int(0, span), span);
    let named = ctx.declare_hidden_temp(PhpType::Bool);
    store_expr_into_temp(ctx, &named, PhpType::Bool,
        &Expr::new(ExprKind::BoolLiteral(false), span), span);
    validate_container_keys(ctx, signature, &container_slot, &argc, &named, span);
    let required = signature.defaults.iter().take(regular).rposition(Option::is_none)
        .map_or(0, |index| index + 1);
    let mut values = Vec::new();
    for index in 0..regular {
        let null = lower_null(ctx, &Expr::new(ExprKind::Null, span));
        let null = ctx.box_value_as_mixed(null, PhpType::Mixed, Some(span));
        let slot = call_intermediate(ctx, null, span);
        let position = literal_int(index as i64, span);
        let name = literal_string(&signature.params[index].0, span);
        let present_position = key_exists(ctx, &container_slot, &position, span);
        let present_name = key_exists(ctx, &container_slot, &name, span);
        let both = ctx.emit_value(Op::IBitAnd, vec![present_position.value, present_name.value],
            None, PhpType::Bool, Op::IBitAnd.default_effects(), Some(span));
        reject_if(ctx, both, "Error", literal_string(&format!(
            "Named parameter ${} overwrites previous argument", signature.params[index].0,
        ), span), span);
        let supplied = ctx.emit_value(Op::IBitOr, vec![present_position.value, present_name.value],
            None, PhpType::Bool, Op::IBitOr.default_effects(), Some(span));
        let provided = ctx.builder.create_named_block("enum.unpack.provided", Vec::new());
        let omitted = ctx.builder.create_named_block("enum.unpack.omitted", Vec::new());
        let ready = ctx.builder.create_named_block("enum.unpack.ready", Vec::new());
        conditional(ctx, supplied.value, provided, omitted);
        ctx.builder.position_at_end(provided);
        let named_value = ctx.builder.create_named_block("enum.unpack.value.named", Vec::new());
        let position_value = ctx.builder.create_named_block("enum.unpack.value.positional", Vec::new());
        let value_ready = ctx.builder.create_named_block("enum.unpack.value.ready", Vec::new());
        conditional(ctx, present_name.value, named_value, position_value);
        ctx.builder.position_at_end(named_value);
        let value = hash_value(ctx, &container_slot, &name, span);
        store_value_into_temp(ctx, &slot, PhpType::Mixed, value, span);
        branch(ctx, value_ready);
        ctx.builder.position_at_end(position_value);
        let value = hash_value(ctx, &container_slot, &position, span);
        store_value_into_temp(ctx, &slot, PhpType::Mixed, value, span);
        branch(ctx, value_ready);
        ctx.builder.position_at_end(value_ready);
        guard_parameter(ctx, signature, index, &variable(&slot, span), class, method, span);
        branch(ctx, ready);
        ctx.builder.position_at_end(omitted);
        if index < required {
            throw_missing(ctx, signature, class, method, index, required,
                &argc, &named, span);
        } else if let Some(default) = signature.defaults.get(index).and_then(Option::as_ref) {
            store_expr_into_temp(ctx, &slot, PhpType::Mixed, default, span);
            branch(ctx, ready);
        } else {
            unreachable!("every required selected enum parameter has an omission check");
        }
        ctx.builder.position_at_end(ready);
        let value = ctx.load_local(&slot, Some(span));
        let value = crate::ir_lower::ownership::acquire_if_refcounted(ctx, value, Some(span));
        values.push(root_evaluated_call_argument(ctx, value, span).value);
    }
    if crate::func_args::sig_has_hidden_argc_param(signature) {
        values.push(ctx.load_local(&argc, Some(span)).value);
    }
    if signature.variadic.is_some() {
        let tail = collect_tail(ctx, signature, &container_slot, &argc, regular, class, method, span);
        values.push(root_evaluated_call_argument(ctx, tail, span).value);
    }
    Some(coerce_operands_to_params(ctx, signature, values))
}

/// Lets ordinary call cleanup retain a mutable materialization slot through the callee.
fn call_intermediate(ctx: &mut LoweringContext<'_, '_>, value: LoweredValue, span: Span) -> String {
    let ty = ctx.builder.value_php_type(value.value);
    let slot = own_value(ctx, value, ty, span);
    let snapshot = ctx.load_local(&slot, Some(span));
    ctx.builder.set_value_ownership(snapshot.value, Ownership::Borrowed);
    let owner = crate::ir_lower::context::CallArgumentEvaluationOwner {
        value: Some(snapshot.value), borrow: Some(snapshot.value), temp_name: slot.clone(),
        slot: ctx.local_slots[&slot], span, ref_cell_payload: None, scoped_receiver_alias: false,
    };
    ctx.call_argument_evaluation_scopes.last_mut().expect("selected method argument scope")
        .owners.push(owner);
    slot
}

/// Checks unknown names before types, and counts only PHP-visible positional slots.
fn validate_container_keys(
    ctx: &mut LoweringContext<'_, '_>, signature: &FunctionSig, source: &str,
    argc: &str, named: &str, span: Span,
) {
    walk_container(ctx, source, span, |ctx, key, _| {
        let is_name = lower_expr(ctx, &call("is_string", vec![key.clone()], span));
        let name_block = ctx.builder.create_named_block("enum.unpack.key.name", Vec::new());
        let position_block = ctx.builder.create_named_block("enum.unpack.key.position", Vec::new());
        let done = ctx.builder.create_named_block("enum.unpack.key.done", Vec::new());
        conditional(ctx, is_name.value, name_block, position_block);
        ctx.builder.position_at_end(position_block);
        let count = Expr::new(ExprKind::BinaryOp {
            left: Box::new(key.clone()), op: BinOp::Add, right: Box::new(literal_int(1, span)),
        }, span);
        store_expr_into_temp(ctx, argc, PhpType::Int, &count, span);
        branch(ctx, done);
        ctx.builder.position_at_end(name_block);
        store_expr_into_temp(ctx, named, PhpType::Bool,
            &Expr::new(ExprKind::BoolLiteral(true), span), span);
        for index in 0..crate::types::call_args::regular_param_count(signature) {
            let matches = lower_expr(ctx, &Expr::new(ExprKind::BinaryOp {
                left: Box::new(key.clone()), op: BinOp::StrictEq,
                right: Box::new(literal_string(&signature.params[index].0, span)),
            }, span));
            let matched = ctx.builder.create_named_block("enum.unpack.name.match", Vec::new());
            let next = ctx.builder.create_named_block("enum.unpack.name.next", Vec::new());
            conditional(ctx, matches.value, matched, next);
            ctx.builder.position_at_end(matched);
            let occupied = key_exists(ctx, source, &literal_int(index as i64, span), span);
            reject_if(ctx, occupied, "Error", literal_string(&format!(
                "Named parameter ${} overwrites previous argument", signature.params[index].0,
            ), span), span);
            let maximum = call("max", vec![variable(argc, span), literal_int((index + 1) as i64, span)], span);
            store_expr_into_temp(ctx, argc, PhpType::Int, &maximum, span);
            branch(ctx, done);
            ctx.builder.position_at_end(next);
        }
        if signature.variadic.is_some() && !crate::func_args::sig_collects_surplus_args(signature) {
            branch(ctx, done);
        } else {
            throw_message(ctx, "Error", concat(literal_string("Unknown named parameter $", span),
                key, span), span);
        }
        ctx.builder.position_at_end(done);
    });
}

/// Copies surplus values into the physical variadic or generated introspection collector.
fn collect_tail(
    ctx: &mut LoweringContext<'_, '_>, signature: &FunctionSig, source: &str,
    argc: &str, regular: usize, class: &str, method: &str, span: Span,
) -> LoweredValue {
    let ty = PhpType::AssocArray { key: Box::new(PhpType::Mixed), value: Box::new(PhpType::Mixed) };
    let hash = ctx.emit_owned_value(Op::HashNew, Vec::new(), Some(Immediate::Capacity(0)),
        ty.clone(), Op::HashNew.default_effects(), Some(span));
    let tail = own_value(ctx, hash, ty, span);
    if crate::func_args::sig_collects_optional_arg_count(signature) {
        let count = ctx.load_local(argc, Some(span));
        let count = ctx.box_value_as_mixed(count, PhpType::Mixed, Some(span));
        let hash = ctx.load_local(&tail, Some(span));
        ctx.emit_void(Op::HashAppend, vec![hash.value, count.value], None,
            Op::HashAppend.default_effects(), Some(span));
        crate::ir_lower::ownership::release_if_owned(ctx, count, Some(span));
    }
    walk_container(ctx, source, span, |ctx, key, value| {
        let is_name = call("is_string", vec![key.clone()], span);
        let beyond = Expr::new(ExprKind::BinaryOp {
            left: Box::new(key.clone()), op: BinOp::GtEq,
            right: Box::new(literal_int(regular as i64, span)),
        }, span);
        let mut wanted = beyond;
        if !crate::func_args::sig_collects_surplus_args(signature) {
            let mut unknown = Expr::new(ExprKind::BoolLiteral(true), span);
            for (name, _) in signature.params.iter().take(regular) {
                unknown = Expr::new(ExprKind::BinaryOp {
                    left: Box::new(unknown), op: BinOp::And,
                    right: Box::new(Expr::new(ExprKind::BinaryOp {
                        left: Box::new(key.clone()), op: BinOp::StrictNotEq,
                        right: Box::new(literal_string(name, span)),
                    }, span)),
                }, span);
            }
            wanted = Expr::new(ExprKind::Ternary {
                condition: Box::new(is_name.clone()), then_expr: Box::new(unknown),
                else_expr: Box::new(wanted),
            }, span);
        } else {
            wanted = Expr::new(ExprKind::BinaryOp {
                left: Box::new(Expr::new(ExprKind::Not(Box::new(is_name.clone())), span)),
                op: BinOp::And, right: Box::new(wanted),
            }, span);
        }
        let wanted = lower_expr(ctx, &wanted);
        let append = ctx.builder.create_named_block("enum.unpack.tail.append", Vec::new());
        let done = ctx.builder.create_named_block("enum.unpack.tail.done", Vec::new());
        conditional(ctx, wanted.value, append, done);
        ctx.builder.position_at_end(append);
        let mut element_signature = signature.clone();
        let parameter = element_signature.params.len() - 1;
        if let Some(element) = crate::types::signatures::variadic_source_element_type_expr(signature) {
            element_signature.params[parameter].1 = ctx.type_expr_to_php_type_for_value(element);
        } else {
            element_signature.declared_params[parameter] = false;
        }
        // The regular arguments have already passed their type and omission checks.
        // Variadic values now receive their own weak binding rather than raw Mixed casts.
        let named_base = Expr::new(ExprKind::Ternary {
            condition: Box::new(Expr::new(ExprKind::BinaryOp {
                left: Box::new(variable(argc, span)), op: BinOp::Gt,
                right: Box::new(literal_int(regular as i64, span)),
            }, span)),
            then_expr: Box::new(variable(argc, span)),
            else_expr: Box::new(literal_int(regular as i64, span)),
        }, span);
        let position = Expr::new(ExprKind::BinaryOp {
            left: Box::new(Expr::new(ExprKind::Ternary {
                condition: Box::new(is_name.clone()), then_expr: Box::new(named_base),
                else_expr: Box::new(key.clone()),
            }, span)),
            op: BinOp::Add, right: Box::new(literal_int(1, span)),
        }, span);
        guard_variadic_element(ctx, &element_signature, parameter, &value, &position, class, method, span);
        let hash = ctx.load_local(&tail, Some(span));
        let value_expr = value;
        let value = lower_arg_with_signature(ctx, &element_signature, parameter, &value_expr);
        let value = lowered_value_from_id(ctx, value);
        let value = ctx.box_value_as_mixed(value, PhpType::Mixed, Some(span));
        let name = lower_expr(ctx, &is_name);
        let named = ctx.builder.create_named_block("enum.unpack.tail.named", Vec::new());
        let positional = ctx.builder.create_named_block("enum.unpack.tail.positional", Vec::new());
        let stored = ctx.builder.create_named_block("enum.unpack.tail.stored", Vec::new());
        conditional(ctx, name.value, named, positional);
        ctx.builder.position_at_end(named);
        let key = lower_expr(ctx, &key);
        ctx.emit_void(Op::DescriptorArgSet, vec![hash.value, key.value, value.value],
            None, Op::DescriptorArgSet.default_effects(), Some(span));
        branch(ctx, stored);
        ctx.builder.position_at_end(positional);
        ctx.emit_void(Op::HashAppend, vec![hash.value, value.value], None,
            Op::HashAppend.default_effects(), Some(span));
        branch(ctx, stored);
        ctx.builder.position_at_end(stored);
        crate::ir_lower::ownership::release_if_owned(ctx, value, Some(span));
        branch(ctx, done);
        ctx.builder.position_at_end(done);
    });
    let value = ctx.load_local(&tail, Some(span));
    let value = crate::ir_lower::ownership::acquire_if_refcounted(ctx, value, Some(span));
    retire_slot(ctx, &tail, span);
    value
}

/// Raises selected-method omission diagnostics after earlier supplied type checks.
fn throw_missing(
    ctx: &mut LoweringContext<'_, '_>, signature: &FunctionSig, class: &str, method: &str,
    index: usize, required: usize, argc: &str, named: &str, span: Span,
) {
    let flag = ctx.load_local(named, Some(span));
    let name = ctx.builder.create_named_block("enum.unpack.missing.named", Vec::new());
    let position = ctx.builder.create_named_block("enum.unpack.missing.positional", Vec::new());
    conditional(ctx, flag.value, name, position);
    ctx.builder.position_at_end(name);
    throw_message(ctx, "ArgumentCountError", literal_string(&format!(
        "{class}::{method}(): Argument #{} (${}) not passed", index + 1, signature.params[index].0,
    ), span), span);
    ctx.builder.position_at_end(position);
    let bound = if required < crate::types::call_args::regular_param_count(signature) {
        "at least"
    } else { "exactly" };
    let message = concat(literal_string(&format!("Too few arguments to function {class}::{method}(), "), span),
        concat(variable(argc, span), literal_string(&format!(" passed and {bound} {required} expected"), span), span), span);
    throw_message(ctx, "ArgumentCountError", message, span);
}

/// Walks a rooted normalized argument hash with stable boxed key and value slots.
fn walk_container(
    ctx: &mut LoweringContext<'_, '_>, source: &str, span: Span,
    mut body: impl FnMut(&mut LoweringContext<'_, '_>, Expr, Expr),
) {
    let source = ctx.load_local(source, Some(span));
    let (iterator, owner, _) = ctx.emit_iter_start(source, false, span);
    let key = ctx.declare_synthetic_php_local(PhpType::Mixed);
    let value = ctx.declare_synthetic_php_local(PhpType::Mixed);
    for slot in [&key, &value] {
        // A hand-built EIR backedge is not represented in ctx.loop_stack. Seed
        // initialization before it so each iteration retires the previous occupant.
        store_expr_into_temp(ctx, slot, PhpType::Mixed, &Expr::new(ExprKind::Null, span), span);
        register_owned_call_operand(ctx, ctx.local_slots[slot], span);
    }
    let header = ctx.builder.create_named_block("enum.unpack.iter.next", Vec::new());
    let current = ctx.builder.create_named_block("enum.unpack.iter.current", Vec::new());
    let done = ctx.builder.create_named_block("enum.unpack.iter.done", Vec::new());
    branch(ctx, header);
    ctx.builder.position_at_end(header);
    let next = ctx.emit_value(Op::IterNext, vec![iterator.value], None, PhpType::Bool,
        Op::IterNext.default_effects(), Some(span));
    conditional(ctx, next.value, current, done);
    ctx.builder.position_at_end(current);
    for (op, slot) in [(Op::IterCurrentKey, &key), (Op::IterCurrentValue, &value)] {
        let value = ctx.emit_value(op, vec![iterator.value], None, PhpType::Mixed,
            op.default_effects(), Some(span));
        ctx.store_local(slot, value, PhpType::Mixed, Some(span));
    }
    body(ctx, variable(&key, span), variable(&value, span));
    branch(ctx, header);
    ctx.builder.position_at_end(done);
    retire_slot(ctx, &value, span);
    retire_slot(ctx, &key, span);
    if let Some(owner) = owner { ctx.retire_iter_start_owner(owner, span); }
}

/// Reads a parameter value from a normalized hash without changing its key space.
fn hash_value(ctx: &mut LoweringContext<'_, '_>, source: &str, key: &Expr, span: Span) -> LoweredValue {
    let source = ctx.load_local(source, Some(span));
    let key = lower_expr(ctx, key);
    ctx.emit_value(Op::HashGetSilent, vec![source.value, key.value], None,
        PhpType::Mixed, Op::HashGetSilent.default_effects(), Some(span))
}

/// Probes the normalized descriptor hash, including entries whose value is null.
fn key_exists(ctx: &mut LoweringContext<'_, '_>, source: &str, key: &Expr, span: Span) -> LoweredValue {
    let source = ctx.load_local(source, Some(span));
    let key = lower_expr(ctx, key);
    ctx.emit_value(Op::DescriptorArgKeyExists, vec![source.value, key.value], None,
        PhpType::Bool, Op::DescriptorArgKeyExists.default_effects(), Some(span))
}

/// Publishes one owned value for exception cleanup before more binding work runs.
fn own_value(ctx: &mut LoweringContext<'_, '_>, value: LoweredValue, ty: PhpType, span: Span) -> String {
    let slot = ctx.declare_synthetic_php_local(ty.clone());
    store_value_into_temp(ctx, &slot, ty, value, span);
    register_owned_call_operand(ctx, ctx.local_slots[&slot], span);
    slot
}

/// Retires a materialization root after all emitted operands hold their own owners.
fn retire_slot(ctx: &mut LoweringContext<'_, '_>, slot: &str, span: Span) {
    retire_owned_call_operand(ctx, ctx.local_slots[slot], span);
}

/// Rejects an invalid binding through ordinary native exception cleanup.
fn reject_if(ctx: &mut LoweringContext<'_, '_>, invalid: LoweredValue, class: &str, message: Expr, span: Span) {
    let bad = ctx.builder.create_named_block("enum.unpack.guard.bad", Vec::new());
    let ok = ctx.builder.create_named_block("enum.unpack.guard.ok", Vec::new());
    conditional(ctx, invalid.value, bad, ok);
    ctx.builder.position_at_end(bad);
    throw_message(ctx, class, message, span);
    ctx.builder.position_at_end(ok);
}

/// Creates a private variable expression with its original source diagnostic span.
fn variable(name: &str, span: Span) -> Expr { Expr::new(ExprKind::Variable(name.to_string()), span) }

/// Creates a diagnostic or parameter-name literal.
fn literal_string(value: &str, span: Span) -> Expr { Expr::new(ExprKind::StringLiteral(value.to_string()), span) }

/// Creates a parameter-index or counter literal.
fn literal_int(value: i64, span: Span) -> Expr { Expr::new(ExprKind::IntLiteral(value), span) }

/// Calls a contract-backed builtin through ordinary EIR lowering.
fn call(name: &str, args: Vec<Expr>, span: Span) -> Expr {
    Expr::new(ExprKind::FunctionCall { name: Name::unqualified(name), args }, span)
}

/// Connects one binding branch to its continuation.
fn branch(ctx: &mut LoweringContext<'_, '_>, target: BlockId) {
    ctx.builder.terminate(Terminator::Br { target, args: Vec::new() });
}

/// Splits binding control flow without duplicating source evaluation.
fn conditional(ctx: &mut LoweringContext<'_, '_>, cond: ValueId, yes: BlockId, no: BlockId) {
    ctx.builder.terminate(Terminator::CondBr {
        cond, then_target: yes, then_args: Vec::new(), else_target: no, else_args: Vec::new(),
    });
}
