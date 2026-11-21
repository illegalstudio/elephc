//! Purpose:
//! Late-static calls whose lexical target is a private static method of the scope class.
//!
//! Called from:
//! - `crate::ir_lower::expr::static_method_calls::lower_static_method_call_bound()`.
//!
//! Key details:
//! - A private static method is not inherited, so it has no static-vtable slot and
//!   `static::m()` used to bind `A::m()` for every called class. php-src looks `m` up in the
//!   late-bound class instead: a subclass that redeclares `m` runs its own method, or raises
//!   `Error` when that redeclaration is private to it.
//! - Only called classes whose lookup differs from the scope's method get a dispatch arm. A
//!   call whose method no subclass redeclares lowers exactly as before.
//! - Each arm lowers the arguments against the signature of the method it calls, so a
//!   redeclaration with its own parameter types or defaults binds them like a direct call.

use super::*;

/// What `static::m()` resolves to for one group of late-bound called classes.
#[derive(Clone, Debug, PartialEq, Eq)]
enum PrivateLateStaticArm {
    /// The called class inherits a non-private `m` from `class`: call it, forwarding `static`.
    Call(String),
    /// The called class sees a private `m` declared by `class`, which the scope cannot call.
    PrivateError(String),
}

/// Lowers `static::m()` when `m` is a private static method of the lexical class and some
/// subclass redeclares it.
///
/// Returns `None` when the ordinary lowering already matches php-src: the receiver is not
/// `static`, the scope's method is not private, or no subclass redeclares it. `$this->m()`
/// reaches the static path with an object receiver and keeps the scope's private method, as
/// php-src's instance-call lookup does.
pub(super) fn try_lower_private_late_static_call(
    ctx: &mut LoweringContext<'_, '_>,
    receiver: &StaticReceiver,
    method: &str,
    args: &[Expr],
    expr: &Expr,
    object_receiver: Option<crate::ir::ValueId>,
) -> Option<LoweredValue> {
    if object_receiver.is_some() || !matches!(receiver, StaticReceiver::Static) {
        return None;
    }
    let scope = ctx.current_class.clone()?;
    let arms = private_late_static_arms(ctx, &scope, method);
    if arms.is_empty() {
        return None;
    }
    Some(lower_private_late_static_dispatch(ctx, receiver, method, args, expr, &scope, arms))
}

/// Groups the descendants of `scope` whose late-bound lookup of `method` differs from the
/// scope's own private static method, sorted by class id for deterministic dispatch.
fn private_late_static_arms(
    ctx: &LoweringContext<'_, '_>,
    scope: &str,
    method: &str,
) -> Vec<(PrivateLateStaticArm, Vec<u64>)> {
    let key = php_symbol_key(method);
    let Some(scope_info) = ctx.classes.get(scope) else {
        return Vec::new();
    };
    let scope_private = scope_info.static_method_visibilities.get(&key) == Some(&Visibility::Private)
        && scope_info.static_method_impl_classes.get(&key).map(String::as_str) == Some(scope);
    if !scope_private {
        return Vec::new();
    }
    let mut classes = ctx
        .classes
        .iter()
        .filter(|(name, _)| name.as_str() != scope)
        .map(|(name, info)| (name.as_str(), info.class_id))
        .collect::<Vec<_>>();
    classes.sort_by_key(|(_, class_id)| *class_id);
    let mut arms: Vec<(PrivateLateStaticArm, Vec<u64>)> = Vec::new();
    for (class_name, class_id) in classes {
        let Some(arm) = late_bound_lookup(ctx, class_name, scope, &key) else {
            continue;
        };
        // php-src hands a refused call to `__callStatic()` instead; that class keeps the scope's
        // method (a known gap) rather than raising an `Error` php-src would not raise.
        if matches!(arm, PrivateLateStaticArm::PrivateError(_))
            && ctx.classes.get(class_name).is_some_and(|info| {
                info.static_methods.contains_key(&php_symbol_key("__callStatic"))
            })
        {
            continue;
        }
        match arms.iter_mut().find(|(existing, _)| *existing == arm) {
            Some((_, ids)) => ids.push(class_id),
            None => arms.push((arm, vec![class_id])),
        }
    }
    arms
}

/// Walks from `class_name` up to `scope` and returns the redeclaration of `key` that php-src's
/// lookup in the called class finds, or `None` when it finds the scope's own method (or when
/// `class_name` does not descend from `scope`).
fn late_bound_lookup(
    ctx: &LoweringContext<'_, '_>,
    class_name: &str,
    scope: &str,
    key: &str,
) -> Option<PrivateLateStaticArm> {
    let mut cursor = class_name;
    let mut found = None;
    while cursor != scope {
        let info = ctx.classes.get(cursor)?;
        if found.is_none() {
            if info.static_method_impl_classes.get(key).map(String::as_str) == Some(cursor)
                && info.static_methods.contains_key(key)
            {
                found = Some(
                    if info.static_method_visibilities.get(key) == Some(&Visibility::Private) {
                        PrivateLateStaticArm::PrivateError(cursor.to_string())
                    } else {
                        PrivateLateStaticArm::Call(cursor.to_string())
                    },
                );
            } else if info.method_impl_classes.get(key).map(String::as_str) == Some(cursor) {
                // An instance-method redeclaration keeps the scope's method (a known gap).
                return None;
            }
        }
        cursor = info.parent.as_deref()?;
    }
    found
}

/// Returns the result type of `static::m()` when it dispatches over redeclarations of a private
/// static method, or `None` when the call binds the scope's method alone.
///
/// The dispatch stores every arm into one merge temporary of this type, and callers that pick
/// storage before lowering the call (array literals, merges) must see the same type: a
/// redeclaration returning `int` beside a private method returning `string` joins to `mixed`.
pub(in crate::ir_lower) fn private_late_static_result_type(
    ctx: &LoweringContext<'_, '_>,
    receiver: &StaticReceiver,
    method: &str,
) -> Option<PhpType> {
    if !matches!(receiver, StaticReceiver::Static) {
        return None;
    }
    let scope = ctx.current_class.as_deref()?;
    let arms = private_late_static_arms(ctx, scope, method);
    if arms.is_empty() {
        return None;
    }
    Some(joined_result_type(ctx, receiver, method, &arms))
}

/// Joins the scope method's result type with every called redeclaration's: the shared type when
/// they agree (`void` when all are `void`), `mixed` otherwise.
fn joined_result_type(
    ctx: &LoweringContext<'_, '_>,
    receiver: &StaticReceiver,
    method: &str,
    arms: &[(PrivateLateStaticArm, Vec<u64>)],
) -> PhpType {
    let fallback_type = lexical_static_method_call_expr_type_for_ir(ctx, receiver, method)
        .unwrap_or(PhpType::Mixed);
    let same = arms.iter().all(|(arm, _)| match arm {
        PrivateLateStaticArm::Call(class_name) => {
            let target = static_receiver_for_class_name(class_name);
            lexical_static_method_call_expr_type_for_ir(ctx, &target, method)
                .is_none_or(|ty| ty == fallback_type)
        }
        PrivateLateStaticArm::PrivateError(_) => true,
    });
    if same { fallback_type } else { PhpType::Mixed }
}

/// Emits the called-class dispatch: one compare per affected class, one arm per resolution,
/// and the scope's own method for every other called class.
fn lower_private_late_static_dispatch(
    ctx: &mut LoweringContext<'_, '_>,
    receiver: &StaticReceiver,
    method: &str,
    args: &[Expr],
    expr: &Expr,
    scope: &str,
    arms: Vec<(PrivateLateStaticArm, Vec<u64>)>,
) -> LoweredValue {
    let span = expr.span;
    let result_type = joined_result_type(ctx, receiver, method, &arms);
    // `$r =& static::m()`: each arm adopts its own reference cell into the assignment's staging
    // (a by-value arm gets a fresh cell), so no arm can leave the binding to another's lease.
    let reference_assignment = ctx
        .reference_call_context
        .as_ref()
        .is_some_and(|context| context.depth == ctx.expression_depth);
    if reference_assignment {
        let fallback = static_method_implementation_signature(ctx, receiver, method);
        let targets = arms.iter().filter_map(|(arm, _)| match arm {
            PrivateLateStaticArm::Call(class_name) => static_method_implementation_signature(
                ctx, &static_receiver_for_class_name(class_name), method,
            ),
            PrivateLateStaticArm::PrivateError(_) => None,
        });
        if fallback.into_iter().chain(targets).any(|signature| signature.by_ref_return
            && !signature.return_type.reference_payload_compatible(&result_type))
        {
            crate::ir_lower::diagnostics::refuse(span,
                "Unsupported reference assignment: a late-bound redeclaration returns a \
                 different reference payload than the private method");
        }
    }
    let temp_name = (!reference_assignment && result_type != PhpType::Void)
        .then(|| ctx.declare_owned_hidden_temp(result_type.clone()));
    let split_initialized = ctx.initialized_slots_snapshot();
    let merge = ctx.builder.create_named_block("private_static.merge", Vec::new());
    let called_class_id = ctx.emit_value(
        Op::LoadCalledClassId,
        Vec::new(),
        None,
        PhpType::Int,
        Op::LoadCalledClassId.default_effects(),
        Some(span),
    );
    let mut arm_blocks = Vec::new();
    for (arm, class_ids) in arms {
        let block = ctx.builder.create_named_block("private_static.arm", Vec::new());
        for class_id in class_ids {
            let class_id_value = ctx.emit_value(
                Op::ConstI64,
                Vec::new(),
                Some(Immediate::I64(class_id as i64)),
                PhpType::Int,
                Op::ConstI64.default_effects(),
                Some(span),
            );
            let is_called_class = ctx.emit_value(
                Op::ICmp,
                vec![called_class_id.value, class_id_value.value],
                Some(Immediate::CmpPredicate(CmpPredicate::Eq)),
                PhpType::Bool,
                Op::ICmp.default_effects(),
                Some(span),
            );
            let skip = ctx.builder.create_named_block("private_static.skip", Vec::new());
            ctx.builder.terminate(Terminator::CondBr {
                cond: is_called_class.value,
                then_target: block,
                then_args: Vec::new(),
                else_target: skip,
                else_args: Vec::new(),
            });
            ctx.builder.position_at_end(skip);
        }
        arm_blocks.push((block, arm));
    }
    let mut reachable_initialized = Vec::new();
    // -- every other called class runs the scope's own private method --
    let fallback_type = lexical_static_method_call_expr_type_for_ir(ctx, receiver, method)
        .unwrap_or(PhpType::Mixed);
    if reference_assignment {
        ctx.reference_call_context.as_mut().unwrap().adopted = false;
    }
    let value = lower_static_method_call_bound(
        ctx, receiver, method, args, expr, None, StaticCallBinding::Resolved,
    );
    finish_private_late_static_arm(
        ctx, temp_name.as_deref(), &result_type, value, merge, &mut reachable_initialized,
        reference_assignment, &fallback_type, span,
    );
    for (block, arm) in arm_blocks {
        ctx.builder.position_at_end(block);
        ctx.restore_initialized_slots(split_initialized.clone());
        match arm {
            PrivateLateStaticArm::Call(class_name) => {
                let target = static_receiver_for_class_name(&class_name);
                // The checker bound the arguments against the private method only. A
                // redeclaration with other parameters throws its own binding error, and only
                // when a class that selects it is the called class.
                let signature = static_method_implementation_signature(ctx, &target, method)
                    .cloned();
                if let Some(signature) = signature.as_ref() {
                    if super::selected_method_arguments::reject_invalid(
                        ctx, &class_name, method, signature, args, span,
                    ) {
                        continue;
                    }
                }
                let arm_type = lexical_static_method_call_expr_type_for_ir(ctx, &target, method)
                    .unwrap_or_else(|| result_type.clone());
                if reference_assignment {
                    ctx.reference_call_context.as_mut().unwrap().adopted = false;
                }
                let value = lower_static_method_call_bound(
                    ctx, &target, method, args, expr, None, StaticCallBinding::Forwarded,
                );
                finish_private_late_static_arm(
                    ctx, temp_name.as_deref(), &result_type, value, merge,
                    &mut reachable_initialized, reference_assignment, &arm_type, span,
                );
            }
            PrivateLateStaticArm::PrivateError(class_name) => {
                // php-src refuses the call before evaluating its arguments.
                crate::ir_lower::stmt::lower_throw_access_error(
                    ctx,
                    &format!("Call to private method {class_name}::{method}() from scope {scope}"),
                    span,
                );
            }
        }
    }
    ctx.builder.position_at_end(merge);
    let mut merged = reachable_initialized.into_iter();
    let mut initialized = merged.next().unwrap_or_else(|| split_initialized.clone());
    for arm_initialized in merged {
        initialized.retain(|slot| arm_initialized.contains(slot));
    }
    ctx.restore_initialized_slots(initialized);
    if reference_assignment {
        // Every arm that reaches the merge adopted a cell into the assignment's staging.
        ctx.reference_call_context.as_mut().unwrap().adopted = true;
    }
    match temp_name {
        Some(temp_name) => take_owned_temp(ctx, &temp_name, span),
        None => lower_null(ctx, expr),
    }
}

/// Stores one arm's call result into the merge temporary (or, under `$r =& static::m()`, gives a
/// by-value result its own reference cell) and joins the merge block.
#[allow(clippy::too_many_arguments)]
fn finish_private_late_static_arm(
    ctx: &mut LoweringContext<'_, '_>,
    temp_name: Option<&str>,
    result_type: &PhpType,
    value: LoweredValue,
    merge: BlockId,
    reachable_initialized: &mut Vec<HashSet<LocalSlotId>>,
    reference_assignment: bool,
    arm_type: &PhpType,
    span: Span,
) {
    if ctx.builder.insertion_block_is_terminated() {
        return;
    }
    super::enum_interface_calls::complete_reference_branch(
        ctx, value, reference_assignment, arm_type, span,
    );
    if let Some(temp_name) = temp_name {
        let value = if ctx.builder.value_php_type(value.value) == PhpType::Void {
            lower_expr(ctx, &Expr::new(ExprKind::Null, span))
        } else {
            value
        };
        store_value_into_temp(ctx, temp_name, result_type.clone(), value, span);
    }
    reachable_initialized.push(ctx.initialized_slots_snapshot());
    branch_to(ctx, merge);
}
