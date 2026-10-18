//! Purpose:
//! Plans runtime class selection when constructors need named arguments, defaults, or unpack binding.
//!
//! Called from:
//! - Fixed and late-static dynamic construction in `object_construction` and `scoped_values`.
//!
//! Key details:
//! - Each selected class reuses signature-based fixed construction and its ordinary dispatch ABI.
//! - The class-name root remains borrowed across candidate probes and is retired once at the merge.

use super::*;

/// Lowers `new $class(...)` as a class-name dispatch chain when the call shape needs
/// per-class argument planning.
///
/// Returns `None` when the raw operand list is already correct for every class the
/// runtime dispatch could select, which keeps today's operand ABI for the common
/// exact-arity positional call. Otherwise the class name is evaluated once into a hidden
/// temporary, compared case-insensitively against each candidate class, and each matching
/// branch lowers a fixed-class `new` so named arguments, defaults for omitted optional
/// parameters, and runtime spreads all go through `plan_call_args`. The final `else`
/// branch keeps the generic dynamic-new opcode so runtime-registry classes, the eval
/// bridge, and PHP's class-not-found fatal behave exactly as before.
pub(super) fn lower_new_dynamic_planned_dispatch(
    ctx: &mut LoweringContext<'_, '_>,
    name_expr: &Expr,
    args: &[Expr],
    expr: &Expr,
) -> Option<LoweredValue> {
    let candidates = dynamic_new_planned_candidate_classes(ctx, args, false);
    if candidates.is_empty() {
        return None;
    }

    let name_value = lower_expr(ctx, name_expr);
    Some(lower_new_dynamic_planned_value(ctx, name_value, &candidates, args, expr))
}

/// Dispatches a retained runtime class name without consuming it on nonmatching branches.
pub(super) fn lower_new_dynamic_planned_value(
    ctx: &mut LoweringContext<'_, '_>,
    name_value: LoweredValue,
    candidates: &[String],
    args: &[Expr],
    expr: &Expr,
) -> LoweredValue {
    let name_type = match ctx.builder.value_php_type(name_value.value).codegen_repr() {
        PhpType::Str => PhpType::Str,
        _ => PhpType::Mixed,
    };
    let name_temp = ctx.declare_hidden_temp(name_type.clone());
    store_value_into_temp(ctx, &name_temp, name_type.clone(), name_value, expr.span);
    register_owned_call_operand(ctx, ctx.local_slots[&name_temp], expr.span);
    let name_var = Expr::new(ExprKind::Variable(name_temp.clone()), expr.span);

    let result_temp = ctx.declare_owned_hidden_temp(PhpType::Mixed);
    let merge = ctx
        .builder
        .create_named_block("new.dynamic.planned.merge", Vec::new());

    for class_name in candidates {
        let match_block = ctx
            .builder
            .create_named_block("new.dynamic.planned.match", Vec::new());
        let next_block = ctx
            .builder
            .create_named_block("new.dynamic.planned.next", Vec::new());
        let condition = dynamic_new_class_name_match_expr(
            &name_var,
            class_name,
            name_type == PhpType::Str,
            expr.span,
        );
        let condition = lower_expr(ctx, &condition);
        let condition = coerce_to_int_at_span(ctx, condition, Some(expr.span));
        ctx.builder.terminate(Terminator::CondBr {
            cond: condition.value,
            then_target: match_block,
            then_args: Vec::new(),
            else_target: next_block,
            else_args: Vec::new(),
        });

        ctx.builder.position_at_end(match_block);
        let class = Name::unqualified(class_name.clone());
        let object = lower_new_object(ctx, &class, args, expr);
        store_value_into_temp(ctx, &result_temp, PhpType::Mixed, object, expr.span);
        branch_to(ctx, merge);

        ctx.builder.position_at_end(next_block);
    }

    let name_value = ctx.load_local(&name_temp, Some(expr.span));
    let fallback = super::object_construction::lower_new_dynamic_generic(ctx, name_value, args, expr);
    store_value_into_temp(ctx, &result_temp, PhpType::Mixed, fallback, expr.span);
    branch_to(ctx, merge);

    ctx.builder.position_at_end(merge);
    retire_owned_call_operand(ctx, ctx.local_slots[&name_temp], expr.span);
    take_owned_temp(ctx, &result_temp, expr.span)
}

/// Builds the case-insensitive class-name test for one dispatch-chain branch.
///
/// PHP class names are case-insensitive, so the comparison goes through `strcasecmp`.
/// A class-name expression that is not statically a string is guarded by `is_string`
/// first: `new $object()` and other non-string operands must keep falling through to the
/// generic dynamic-new opcode instead of being stringified here.
pub(super) fn dynamic_new_class_name_match_expr(
    name_var: &Expr,
    class_name: &str,
    name_is_string: bool,
    span: Span,
) -> Expr {
    let compare = Expr::new(
        ExprKind::BinaryOp {
            left: Box::new(Expr::new(
                ExprKind::FunctionCall {
                    name: Name::unqualified("strcasecmp"),
                    args: vec![
                        name_var.clone(),
                        Expr::new(ExprKind::StringLiteral(class_name.to_string()), span),
                    ],
                },
                span,
            )),
            op: BinOp::StrictEq,
            right: Box::new(Expr::new(ExprKind::IntLiteral(0), span)),
        },
        span,
    );
    if name_is_string {
        return compare;
    }
    Expr::new(
        ExprKind::BinaryOp {
            left: Box::new(Expr::new(
                ExprKind::FunctionCall {
                    name: Name::unqualified("is_string"),
                    args: vec![name_var.clone()],
                },
                span,
            )),
            op: BinOp::And,
            right: Box::new(compare),
        },
        span,
    )
}

/// Returns the classes whose constructor would receive different arguments than the raw
/// dynamic-new operand list, in deterministic order.
///
/// Only classes that EIR can construct as a fixed class are considered: compiler-internal
/// classes, runtime-managed builtin classes, and classes without an emitted constructor body
/// keep the generic opcode. A class qualifies when the shared call-argument rules accept the
/// call for its constructor, `lower_args_with_signature` is known to resolve it to exactly one
/// operand per declared parameter, *and* the result differs from the raw source arguments ,
/// that is exactly the set of classes the generic opcode's exact-arity candidate match would
/// otherwise silently skip or feed in source order.
pub(super) fn dynamic_new_planned_candidate_classes(
    ctx: &LoweringContext<'_, '_>,
    args: &[Expr],
    include_hidden_collectors: bool,
) -> Vec<String> {
    if args.is_empty() {
        return Vec::new();
    }
    let constructor_key = php_symbol_key("__construct");
    let mut candidates = ctx
        .classes
        .iter()
        .filter(|(class_name, class_info)| {
            dynamic_new_class_is_planning_candidate(
                ctx,
                class_name,
                class_info,
                &constructor_key,
                args,
                include_hidden_collectors,
            )
        })
        .map(|(class_name, _)| class_name.clone())
        .collect::<Vec<_>>();
    candidates.sort();
    candidates
}

/// Returns true when a dynamic `new` should construct `class_name` through a fixed-class
/// branch instead of the generic dynamic-new opcode.
fn dynamic_new_class_is_planning_candidate(
    ctx: &LoweringContext<'_, '_>,
    class_name: &str,
    class_info: &crate::types::ClassInfo,
    constructor_key: &str,
    args: &[Expr],
    include_hidden_collectors: bool,
) -> bool {
    if class_info.is_abstract || ctx.enums.contains_key(class_name) {
        return false;
    }
    if php_symbol_key(class_name).starts_with("__elephc") {
        return false;
    }
    if crate::codegen_support::dynamic_new::known_dynamic_new_builtin_class_names()
        .contains(&class_name)
    {
        return false;
    }
    if include_hidden_collectors && constructor_signature(ctx, &Name::from(class_name.to_string())).is_some_and(|sig| {
        crate::func_args::sig_collects_surplus_args(sig)
            && args.iter().any(is_spread_arg)
            && !sig.ref_params.iter().any(|by_ref| *by_ref)
            && !crate::func_args::sig_has_hidden_argc_param(sig)
            && crate::types::call_args::plan_call_args(sig, args, args[0].span, false, false).is_ok()
    }) {
        return true;
    }
    // Runtime-registered builtin classes carry no source constructor body, so a fixed-class
    // `ObjectNew` branch would reference a method symbol EIR never emits.
    if class_info.declaration_span == Span::dummy() {
        return false;
    }
    if !class_info
        .method_decls
        .iter()
        .any(|method| php_symbol_key(&method.name) == constructor_key && method.has_body)
    {
        return false;
    }
    let Some(sig) = class_info.methods.get(constructor_key) else {
        return false;
    };
    if sig.variadic.is_some() {
        return false;
    }
    if !dynamic_new_args_need_planning(sig, args) {
        return false;
    }
    dynamic_new_args_lower_to_exact_arity(ctx, sig, args)
}

/// Returns true when a constructor signature would reshape the source argument list.
///
/// The generic dynamic-new opcode forwards operands positionally and only selects a
/// candidate whose constructor arity matches the operand count exactly, so any named
/// argument, spread, or omitted optional parameter needs the planned path.
fn dynamic_new_args_need_planning(sig: &FunctionSig, args: &[Expr]) -> bool {
    crate::types::call_args::has_named_args(args)
        || args.iter().any(is_spread_arg)
        || sig.params.len() != args.len()
}

/// Returns true when `lower_args_with_signature` resolves this call to exactly one operand
/// per declared constructor parameter.
///
/// The fixed-class `ObjectNew` opcode is arity-exact, and the shared argument lowering falls
/// back to a raw source-order operand list for call shapes it cannot resolve (dynamic
/// associative spreads, multiple spreads, spreads that do not feed the parameter tail). Those
/// shapes must keep the generic dynamic-new opcode rather than produce a mis-arity call.
fn dynamic_new_args_lower_to_exact_arity(
    ctx: &LoweringContext<'_, '_>,
    sig: &FunctionSig,
    args: &[Expr],
) -> bool {
    let regular_param_count = crate::types::call_args::regular_param_count(sig);
    if crate::types::call_args::has_named_args(args) {
        let Ok(plan) =
            crate::types::call_args::plan_call_args_with_regular_param_count_and_assoc_spreads(
                sig,
                args,
                args[0].span,
                regular_param_count,
                false,
                true,
                &assoc_spread_sources(ctx, args),
            )
        else {
            return false;
        };
        return !plan.has_spread_args() && plan.regular_args.len() == sig.params.len();
    }
    if args.iter().any(is_spread_arg) {
        return single_trailing_indexed_spread_arg(ctx, args)
            .is_some_and(|spread_idx| spread_idx <= regular_param_count);
    }
    if args.len() > sig.params.len() {
        return false;
    }
    (args.len()..sig.params.len()).all(|index| {
        sig.defaults
            .get(index)
            .is_some_and(|default| default.is_some())
    })
}

