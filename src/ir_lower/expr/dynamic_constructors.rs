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
    name_value: LoweredValue,
    args: &[Expr],
    expr: &Expr,
) -> Option<LoweredValue> {
    let candidates = dynamic_new_planned_candidate_classes(ctx, args, false);
    if candidates.is_empty() {
        return None;
    }

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

    let fallback_block = ctx
        .builder
        .create_named_block("new.dynamic.planned.fallback", Vec::new());
    let result_type = PhpType::Mixed;
    let branches = PlannedNewBranches {
        name_var: &name_var,
        args,
        expr,
        result_temp: &result_temp,
        result_type: &result_type,
        merge,
        fallback: fallback_block,
    };
    if candidates.len() >= PLANNED_NEW_HASH_DISPATCH_MIN {
        emit_planned_new_hashed_dispatch(
            ctx,
            &branches,
            candidates,
            name_type == PhpType::Str,
            expr.span,
        );
    } else {
        for class_name in candidates {
            emit_planned_new_candidate_test(
                ctx,
                &branches,
                class_name,
                name_type == PhpType::Str,
                expr.span,
            );
        }
        branch_to(ctx, fallback_block);
    }
    ctx.builder.position_at_end(fallback_block);

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
    if crate::names::has_internal_elephc_prefix(class_name) {
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
        || dynamic_new_binds_array_element_by_reference(sig, args)
}

/// Returns true when a constructor takes an array-element argument BY REFERENCE.
///
/// `new $class($params['timeout'])` hands the site's operands over by value, which is right for
/// every constructor that does not declare that position as a reference. A candidate that does
/// has to bind the element's storage, so it is constructed through a fixed-class branch that
/// lowers the arguments against its own signature, while every other class keeps the by-value
/// generic path. php decides the same thing per class at run time.
fn dynamic_new_binds_array_element_by_reference(sig: &FunctionSig, args: &[Expr]) -> bool {
    args.iter().enumerate().any(|(index, arg)| {
        matches!(arg.kind, ExprKind::ArrayAccess { .. })
            && sig.ref_params.get(index).copied().unwrap_or(false)
    })
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

/// Lowers `new static(...)` (and the dynamic form constrained to a late-static parent) as a
/// class-name dispatch chain when the call shape needs per-class argument planning.
///
/// Returns `None` when the raw operand list is already correct for every class the
/// runtime dispatch could select, which keeps today's operand ABI for the common
/// exact-arity positional call. Otherwise the class name is evaluated once into a hidden
/// temporary, compared case-insensitively against each candidate class, and each matching
/// branch lowers a fixed-class `new` so named arguments, defaults for omitted optional
/// parameters, and runtime spreads all go through `plan_call_args`. The final `else`
/// branch keeps the generic dynamic-new opcode so runtime-registry classes, the eval
/// bridge, and PHP's class-not-found fatal behave exactly as before.
pub(super) fn lower_new_late_static_planned_dispatch(
    ctx: &mut LoweringContext<'_, '_>,
    name_expr: &Expr,
    args: &[Expr],
    expr: &Expr,
    required_parent: Option<&str>,
) -> Option<LoweredValue> {
    let candidates = late_static_planned_candidate_classes(ctx, args, required_parent);
    if candidates.is_empty() {
        return None;
    }

    let name_value = lower_expr(ctx, name_expr);
    let name_type = match ctx.builder.value_php_type(name_value.value).codegen_repr() {
        PhpType::Str => PhpType::Str,
        _ => PhpType::Mixed,
    };
    // Candidate guards read the class name repeatedly. A one-shot `OwnedTemp` would make
    // every `LoadLocal` look like an ownership transfer and the first guard cleanup would
    // free the value while later guards and the fallback still reference the slot.
    let name_temp = ctx.declare_hidden_temp(name_type.clone());
    store_value_into_temp(ctx, &name_temp, name_type.clone(), name_value, expr.span);
    let name_var = Expr::new(ExprKind::Variable(name_temp.clone()), name_expr.span);

    let result_type = required_parent
        .map(|parent| PhpType::Object(parent.to_string()))
        .unwrap_or(PhpType::Mixed);
    let result_temp = ctx.declare_owned_hidden_temp(result_type.clone());
    let merge = ctx
        .builder
        .create_named_block("new.dynamic.planned.merge", Vec::new());
    let fallback_block = ctx
        .builder
        .create_named_block("new.dynamic.planned.fallback", Vec::new());
    let branches = PlannedNewBranches {
        name_var: &name_var,
        args,
        expr,
        result_temp: &result_temp,
        result_type: &result_type,
        merge,
        fallback: fallback_block,
    };

    if candidates.len() >= PLANNED_NEW_HASH_DISPATCH_MIN {
        emit_planned_new_hashed_dispatch(
            ctx,
            &branches,
            &candidates,
            name_type == PhpType::Str,
            name_expr.span,
        );
    } else {
        for class_name in &candidates {
            emit_planned_new_candidate_test(
                ctx,
                &branches,
                class_name,
                name_type == PhpType::Str,
                name_expr.span,
            );
        }
        branch_to(ctx, fallback_block);
    }

    ctx.builder.position_at_end(fallback_block);

    let positional_only = !args.iter().any(variadic_args::is_spread_arg)
        && !crate::types::call_args::has_named_args(args);
    if let (Some(parent), true) = (required_parent, positional_only) {
        // No AOT candidate matched: the late-static class may be an interpreted subclass. The
        // constrained dynamic-new op builds it through the eval bridge when one is linked, and
        // raises the factory fatal otherwise.
        let name_value = ctx.load_local(&name_temp, Some(expr.span));
        let mut operands = vec![name_value.value];
        operands.extend(lower_args(ctx, args));
        let metadata = format!("{}|{}", parent, parent);
        let data = ctx.intern_class_name(&metadata);
        let fallback = ctx.emit_value(
            Op::DynamicObjectNew,
            operands,
            Some(Immediate::Data(data)),
            PhpType::Object(parent.to_string()),
            Op::DynamicObjectNew.default_effects(),
            Some(expr.span),
        );
        store_value_into_temp(ctx, &result_temp, result_type, fallback, expr.span);
        branch_to(ctx, merge);
    } else if required_parent.is_some() {
        // A spread or named argument list for an interpreted late-static class. The constrained
        // op takes positional operands only, so this goes through the generic dynamic new, which
        // packs the arguments into a runtime container. Laravel's `Component::resolve()` is
        // `new static(...array_intersect_key($data, array_flip($parameters)))` (string keys,
        // so named arguments) for every anonymous Blade component, and this used to be a fatal.
        let name_value = ctx.load_local(&name_temp, Some(expr.span));
        let fallback = lower_new_dynamic_generic(ctx, name_value, args, expr);
        store_value_into_temp(ctx, &result_temp, result_type.clone(), fallback, expr.span);
        branch_to(ctx, merge);
    } else {
        let name_value = ctx.load_local(&name_temp, Some(expr.span));
        let fallback = lower_new_dynamic_generic(ctx, name_value, args, expr);
        store_value_into_temp(ctx, &result_temp, result_type, fallback, expr.span);
        branch_to(ctx, merge);
    }

    ctx.builder.position_at_end(merge);
    if name_type == PhpType::Str {
        // A null scalar has no string-register payload. Writing it through a Str hidden slot
        // would leave stale pointer/length registers behind, then release an unrelated live
        // allocation. Replace the consumed class-name temporary with a static empty string so
        // the ordinary retaining store clears its prior owner using the correct ABI shape.
        let empty_name_data = ctx.intern_string("");
        let empty_name = ctx.emit_value(
            Op::ConstStr,
            Vec::new(),
            Some(Immediate::Data(empty_name_data)),
            PhpType::Str,
            Op::ConstStr.default_effects(),
            Some(expr.span),
        );
        ctx.store_local(&name_temp, empty_name, PhpType::Str, Some(expr.span));
    } else {
        let null = lower_null(ctx, expr);
        ctx.unset_local(&name_temp, null, Some(expr.span));
    }
    Some(take_owned_temp(ctx, &result_temp, expr.span))
}

/// Candidate count from which `new $class(...)` dispatches through a hash tree instead of
/// testing every candidate in turn.
///
/// Below it the linear chain is cheaper than lower-casing and hashing the name once. Above it
/// the chain is the cost: a whole-program build has one candidate per class whose constructor
/// the call can plan, and a Symfony build gives `ReflectionAttribute::newInstance()`'s spread
/// fallback 1,376 of them -- a `strcasecmp` and two async-signal polls each, on every call.
const PLANNED_NEW_HASH_DISPATCH_MIN: usize = 16;

/// What every candidate branch of a planned dynamic `new` shares.
struct PlannedNewBranches<'a> {
    name_var: &'a Expr,
    args: &'a [Expr],
    expr: &'a Expr,
    result_temp: &'a str,
    result_type: &'a PhpType,
    merge: BlockId,
    /// Reached when no candidate matched: the generic opcode, the eval bridge, or the fatal.
    fallback: BlockId,
}

/// Emits one candidate test at the current position: a match constructs that class and
/// branches to `merge`; a miss leaves the builder positioned for the next test.
fn emit_planned_new_candidate_test(
    ctx: &mut LoweringContext<'_, '_>,
    branches: &PlannedNewBranches<'_>,
    class_name: &str,
    name_is_string: bool,
    name_span: Span,
) {
    let match_block = ctx
        .builder
        .create_named_block("new.dynamic.planned.match", Vec::new());
    let next_block = ctx
        .builder
        .create_named_block("new.dynamic.planned.next", Vec::new());
    let condition =
        dynamic_new_class_name_match_expr(branches.name_var, class_name, name_is_string, name_span);
    let condition = lower_expr(ctx, &condition);
    let condition = coerce_to_int_at_span(ctx, condition, Some(branches.expr.span));
    ctx.builder.terminate(Terminator::CondBr {
        cond: condition.value,
        then_target: match_block,
        then_args: Vec::new(),
        else_target: next_block,
        else_args: Vec::new(),
    });

    ctx.builder.position_at_end(match_block);
    let class = Name::unqualified(class_name.to_string());
    let args = branches.args;
    // A descendant that narrows the constructor binds only its own parameters; PHP evaluates
    // the surplus and discards it, and `static_new_droppable_surplus_args` already refused
    // any surplus whose evaluation could be observed.
    let branch_args = ctx
        .classes
        .get(class_name)
        .and_then(|class_info| class_info.methods.get(&php_symbol_key("__construct")))
        .and_then(|sig| static_new_droppable_surplus_args(sig, args))
        .map_or(args, |keep| &args[..keep]);
    let object = lower_new_object(ctx, &class, branch_args, branches.expr);
    store_value_into_temp(
        ctx,
        branches.result_temp,
        branches.result_type.clone(),
        object,
        branches.expr.span,
    );
    branch_to(ctx, branches.merge);

    ctx.builder.position_at_end(next_block);
}

/// Dispatches a planned dynamic `new` through `crc32(strtolower($class))` and a balanced tree
/// of integer compares, so a call tests O(log n) hashes and one name instead of every candidate.
///
/// The leaf still confirms with the same `strcasecmp` the linear chain uses, so the hash only
/// narrows the search and a collision costs one more compare, never a wrong class. The two
/// agree on every match: PHP's `strcasecmp` and `strtolower` both fold ASCII only, so two
/// names `strcasecmp` calls equal lower-case to the same bytes and hash alike. A name that is
/// not a string skips the hash and goes to the fallback, as the chain's `is_string` guard did.
fn emit_planned_new_hashed_dispatch(
    ctx: &mut LoweringContext<'_, '_>,
    branches: &PlannedNewBranches<'_>,
    candidates: &[String],
    name_is_string: bool,
    name_span: Span,
) {
    let span = name_span;
    if !name_is_string {
        let hash_block = ctx
            .builder
            .create_named_block("new.dynamic.planned.hash", Vec::new());
        let is_string = Expr::new(
            ExprKind::FunctionCall {
                name: Name::unqualified("is_string"),
                args: vec![branches.name_var.clone()],
            },
            span,
        );
        let is_string = lower_expr(ctx, &is_string);
        let is_string = coerce_to_int_at_span(ctx, is_string, Some(branches.expr.span));
        ctx.builder.terminate(Terminator::CondBr {
            cond: is_string.value,
            then_target: hash_block,
            then_args: Vec::new(),
            else_target: branches.fallback,
            else_args: Vec::new(),
        });
        ctx.builder.position_at_end(hash_block);
    }

    let lowered_name = Expr::new(
        ExprKind::FunctionCall {
            name: Name::unqualified("strtolower"),
            args: vec![branches.name_var.clone()],
        },
        span,
    );
    let hash = Expr::new(
        ExprKind::FunctionCall {
            name: Name::unqualified("crc32"),
            args: vec![lowered_name],
        },
        span,
    );
    let hash = lower_expr(ctx, &hash);
    let hash_temp = ctx.declare_hidden_temp(PhpType::Int);
    store_value_into_temp(ctx, &hash_temp, PhpType::Int, hash, branches.expr.span);
    let hash_var = Expr::new(ExprKind::Variable(hash_temp), span);

    let mut buckets: std::collections::BTreeMap<i64, Vec<&str>> = std::collections::BTreeMap::new();
    for class_name in candidates {
        buckets
            .entry(php_class_name_crc32(class_name))
            .or_default()
            .push(class_name.as_str());
    }
    let buckets = buckets.into_iter().collect::<Vec<_>>();
    emit_planned_new_hash_tree(ctx, branches, &hash_var, &buckets, span);
}

/// Emits the compare tree over `buckets`, sorted by hash; each leaf tests its bucket's names.
fn emit_planned_new_hash_tree(
    ctx: &mut LoweringContext<'_, '_>,
    branches: &PlannedNewBranches<'_>,
    hash_var: &Expr,
    buckets: &[(i64, Vec<&str>)],
    span: Span,
) {
    if let [(_, class_names)] = buckets {
        for class_name in class_names {
            // The hash block already proved the name is a string.
            emit_planned_new_candidate_test(ctx, branches, class_name, true, span);
        }
        branch_to(ctx, branches.fallback);
        return;
    }
    let mid = buckets.len() / 2;
    let lower_half = ctx
        .builder
        .create_named_block("new.dynamic.planned.hash_lt", Vec::new());
    let upper_half = ctx
        .builder
        .create_named_block("new.dynamic.planned.hash_ge", Vec::new());
    let condition = Expr::new(
        ExprKind::BinaryOp {
            left: Box::new(hash_var.clone()),
            op: BinOp::Lt,
            right: Box::new(Expr::new(ExprKind::IntLiteral(buckets[mid].0), span)),
        },
        span,
    );
    let condition = lower_expr(ctx, &condition);
    let condition = coerce_to_int_at_span(ctx, condition, Some(branches.expr.span));
    ctx.builder.terminate(Terminator::CondBr {
        cond: condition.value,
        then_target: lower_half,
        then_args: Vec::new(),
        else_target: upper_half,
        else_args: Vec::new(),
    });
    ctx.builder.position_at_end(lower_half);
    emit_planned_new_hash_tree(ctx, branches, hash_var, &buckets[..mid], span);
    ctx.builder.position_at_end(upper_half);
    emit_planned_new_hash_tree(ctx, branches, hash_var, &buckets[mid..], span);
}

/// PHP's `crc32(strtolower($name))` for a class name, as the dispatch computes it at runtime.
fn php_class_name_crc32(class_name: &str) -> i64 {
    let mut crc = flate2::Crc::new();
    crc.update(class_name.to_ascii_lowercase().as_bytes());
    i64::from(crc.sum())
}

/// Returns the classes whose constructor would receive different arguments than the raw
/// dynamic-new operand list, in deterministic order.
///
/// Only classes that EIR can construct as a fixed class are considered: compiler-internal
/// classes, runtime-managed builtin classes, and classes without an emitted constructor body
/// keep the generic opcode. A class qualifies when the shared call-argument rules accept the
/// call for its constructor, `lower_args_with_signature` is known to resolve it to exactly one
/// operand per declared parameter, *and* the result differs from the raw source arguments —
/// that is exactly the set of classes the generic opcode's exact-arity candidate match would
/// otherwise silently skip or feed in source order.
fn late_static_planned_candidate_classes(
    ctx: &LoweringContext<'_, '_>,
    args: &[Expr],
    required_parent: Option<&str>,
) -> Vec<String> {
    if let Some(required_parent) = required_parent {
        let mut candidates = ctx
            .classes
            .iter()
            .filter(|(class_name, class_info)| {
                static_new_class_is_planning_candidate(
                    ctx,
                    class_name,
                    class_info,
                    required_parent,
                    args,
                )
            })
            .map(|(class_name, _)| class_name.clone())
            .collect::<Vec<_>>();
        candidates.sort();
        return candidates;
    }
    if args.is_empty() {
        return Vec::new();
    }
    let constructor_key = php_symbol_key("__construct");
    let mut candidates = ctx
        .classes
        .iter()
        .filter(|(class_name, class_info)| {
            if required_parent.is_some_and(|parent| {
                !is_same_or_descendant_class(ctx, class_name, parent)
            }) {
                return false;
            }
            dynamic_new_class_is_planning_candidate(
                ctx,
                class_name,
                class_info,
                &constructor_key,
                args,
                required_parent.is_some(),
            )
        })
        .map(|(class_name, _)| class_name.clone())
        .collect::<Vec<_>>();
    candidates.sort();
    candidates
}

/// Returns whether a concrete descendant can be emitted as one fixed late-static branch.
fn static_new_class_is_planning_candidate(
    ctx: &LoweringContext<'_, '_>,
    class_name: &str,
    class_info: &crate::types::ClassInfo,
    required_parent: &str,
    args: &[Expr],
) -> bool {
    if class_info.is_abstract
        || ctx.enums.contains_key(class_name)
        || class_info.declaration_span == Span::dummy()
        || crate::names::has_internal_elephc_prefix(class_name)
        || !is_same_or_descendant_class(ctx, class_name, required_parent)
    {
        return false;
    }
    let constructor_key = php_symbol_key("__construct");
    let Some(sig) = class_info.methods.get(&constructor_key) else {
        return args.is_empty();
    };
    if !static_new_args_match_param_storage(ctx, sig, args) {
        return false;
    }
    dynamic_new_args_lower_to_exact_arity(ctx, sig, args)
        || static_new_droppable_surplus_args(sig, args).is_some()
}

/// Returns whether every argument can physically reach the parameter it would bind.
///
/// A descendant may declare a constructor whose parameters are nothing like the ones the inherited
/// `new static` factory passes — Symfony's `MethodNotAllowedHttpException` takes `array $allow`
/// where `HttpException::fromStatusCode()` passes `$statusCode`. PHP raises a `TypeError` there,
/// and only for a program that actually calls `MethodNotAllowedHttpException::fromStatusCode()`.
/// Such a class is therefore not a construction candidate: its branch would have to coerce an int
/// into an array slot. Only STORAGE-class disagreement is checked; nominal object mismatches keep
/// their existing runtime guard.
fn static_new_args_match_param_storage(
    ctx: &LoweringContext<'_, '_>,
    sig: &FunctionSig,
    args: &[Expr],
) -> bool {
    args.iter().enumerate().all(|(index, arg)| {
        let Some((_, param_ty)) = sig.params.get(index) else {
            return true;
        };
        if crate::types::late_static_dispatch::literal_string_cannot_bind_numeric(param_ty, arg) {
            return false;
        }
        let Some(arg_repr) = static_new_argument_storage(ctx, arg) else {
            return true;
        };
        !static_new_param_storage_conflicts(&param_ty.late_bound_param_storage(), &arg_repr)
    })
}

/// Returns an argument's representation when it is known well enough to rule a parameter out.
///
/// Only a local whose slot type the lowering already holds, or a literal, answers: the syntactic
/// guess types every unknown expression `Int`, which would rule out the very classes this factory
/// is written for.
fn static_new_argument_storage(ctx: &LoweringContext<'_, '_>, arg: &Expr) -> Option<PhpType> {
    match &arg.kind {
        ExprKind::Variable(name) => match ctx.local_type(name) {
            PhpType::Mixed | PhpType::Union(_) => None,
            known => Some(known.codegen_repr()),
        },
        ExprKind::IntLiteral(_) => Some(PhpType::Int),
        ExprKind::FloatLiteral(_) => Some(PhpType::Float),
        ExprKind::BoolLiteral(_) => Some(PhpType::Bool),
        ExprKind::StringLiteral(_) => Some(PhpType::Str),
        ExprKind::ArrayLiteral(_) => Some(PhpType::Array(Box::new(PhpType::Mixed))),
        ExprKind::ArrayLiteralAssoc(_) => Some(PhpType::AssocArray {
            key: Box::new(PhpType::Mixed),
            value: Box::new(PhpType::Mixed),
        }),
        _ => None,
    }
}

/// Returns whether two concrete representations can never hold one another's values.
fn static_new_param_storage_conflicts(param: &PhpType, arg: &PhpType) -> bool {
    let array_like = |ty: &PhpType| matches!(ty, PhpType::Array(_) | PhpType::AssocArray { .. });
    let scalar_like = |ty: &PhpType| {
        matches!(
            ty,
            PhpType::Int | PhpType::Float | PhpType::Bool | PhpType::False | PhpType::Str
        )
    };
    let object_like = |ty: &PhpType| matches!(ty, PhpType::Object(_));
    (array_like(param) && (scalar_like(arg) || object_like(arg)))
        || ((scalar_like(param) || object_like(param)) && array_like(arg))
}

/// Returns how many leading arguments a narrowing descendant's constructor binds, when the
/// surplus beyond them can simply be dropped.
///
/// PHP raises `ArgumentCountError` only for too FEW arguments; surplus positional arguments to a
/// userland function are evaluated and then ignored. A class that INHERITS a `new static` factory
/// while narrowing its own constructor relies on exactly that: Symfony's `HttpException`
/// ends `fromStatusCode()` with `new static($statusCode, $message, $previous, $headers, $code)`,
/// and `AccessDeniedHttpException` declares four parameters. Without this the descendant was not a
/// planning candidate at all and its branch fell through to the late-static fatal.
///
/// The surplus arguments still have to be EVALUATED in source order, which this branch cannot do
/// once it stops passing them — so only side-effect-free surplus is droppable. Anything else keeps
/// the old behavior rather than silently skipping an evaluation PHP performs.
fn static_new_droppable_surplus_args(sig: &FunctionSig, args: &[Expr]) -> Option<usize> {
    let keep = sig.params.len();
    if crate::func_args::sig_collects_surplus_args(sig) || args.len() <= keep {
        return None;
    }
    if args.iter().any(|arg| {
        matches!(
            arg.kind,
            ExprKind::Spread(_) | ExprKind::NamedArg { .. }
        )
    }) {
        return None;
    }
    if !args[keep..].iter().all(expr_is_side_effect_free_argument) {
        return None;
    }
    Some(keep)
}

/// Returns whether evaluating this argument can be skipped without changing what the program does.
fn expr_is_side_effect_free_argument(expr: &Expr) -> bool {
    matches!(
        expr.kind,
        ExprKind::Variable(_)
            | ExprKind::This
            | ExprKind::IntLiteral(_)
            | ExprKind::FloatLiteral(_)
            | ExprKind::StringLiteral(_)
            | ExprKind::BoolLiteral(_)
            | ExprKind::Null
    )
}

