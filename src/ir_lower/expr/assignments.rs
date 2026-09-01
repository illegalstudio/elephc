//! Purpose:
//! Assignment-expression, non-local write, and increment/decrement lowering.
//!
//! Called from:
//! - `crate::ir_lower::expr`.
//!
//! Key details:
//! - Preserves source-order evaluation, EIR typing, effects, and ownership contracts.

use super::*;

/// Lowers an assignment expression while preserving target evaluation and result semantics.
pub(super) fn lower_assignment_expr(
    ctx: &mut LoweringContext<'_, '_>,
    target: &Expr,
    value: &Expr,
    result_target: Option<&Expr>,
    prelude: &[crate::parser::ast::Stmt],
    conditional_value_temp: Option<&str>,
    expr: &Expr,
) -> LoweredValue {
    for stmt in prelude {
        crate::ir_lower::stmt::lower_stmt(ctx, stmt);
    }
    if let Some(temp_name) = conditional_value_temp {
        if let Some(result) = lower_conditional_non_local_null_coalesce_assignment(
            ctx,
            temp_name,
            target,
            value,
            result_target,
            expr,
        ) {
            return result;
        }
    }
    let assigned_name = match &target.kind {
        ExprKind::Variable(name) => Some(name.as_str()),
        _ => None,
    };
    if let Some(name) = assigned_name {
        if is_compound_assignment_self_read(value, name, expr.span) && !ctx.has_local_slot(name) {
            let null_value = ctx.builder.emit_const_null();
            let null_lowered = LoweredValue { value: null_value, ir_type: IrType::I64 };
            ctx.store_local(name, null_lowered, PhpType::Void, Some(expr.span));
            ctx.mark_local_initialized(name);
        }
    }
    let static_callable = assigned_name.and_then(|_| static_callable_binding_for_expr(ctx, value));
    let reflected_class = assigned_name.and_then(|_| reflection_class_binding_for_expr(ctx, value));
    let reflected_function =
        assigned_name.and_then(|_| reflection_function_binding_for_expr(ctx, value));
    let reflected_property =
        assigned_name.and_then(|_| reflection_property_binding_for_expr(ctx, value));
    let reflected_method =
        assigned_name.and_then(|_| reflection_method_binding_for_expr(ctx, value));
    let reflected_args = assigned_name.and_then(|_| reflection_arg_array_binding_for_expr(value));
    let fiber_start_sig =
        assigned_name.and_then(|_| crate::ir_lower::fibers::start_sig_for_expr(ctx, value));
    let callable_array = assigned_name
        .and_then(|_| lower_callable_array_for_assignment(ctx, value, static_callable.as_ref()));
    let lowered = assigned_name
        .and_then(|_| callable_array.as_ref().map(|assignment| assignment.value))
        .or_else(|| assigned_name.and_then(|name| lower_closure_for_assignment(ctx, name, value)))
        .unwrap_or_else(|| lower_expr(ctx, value));
    let mut result = lowered;
    if let ExprKind::Variable(name) = &target.kind {
        // For static locals and ref-bound locals, keep the declared type to
        // avoid widening Int→Mixed. The codegen narrows Mixed→Int when the slot
        // is Int-typed. Without this, ref cells would hold Mixed boxes instead
        // of raw ints, breaking the ref cell ownership model.
        let value_php_type = ctx.builder.value_php_type(lowered.value);
        let is_static = matches!(
            ctx.local_kinds.get(name).copied(),
            Some(crate::ir::LocalKind::StaticLocal)
        );
        let is_ref_bound = ctx.is_ref_bound_local(name);
        let existing_type = ctx.local_types.get(name).cloned();
        let php_type = if is_static || is_ref_bound {
            existing_type.unwrap_or(value_php_type)
        } else {
            value_php_type
        };
        ctx.store_local(name, lowered, php_type, Some(expr.span));
        result = ctx.load_local(name, Some(expr.span));
        let static_callable = callable_array
            .map(|assignment| assignment.target)
            .or(static_callable);
        if let Some(target) = static_callable {
            ctx.bind_static_callable_local(name, target);
        }
        if let Some(reflected_class) = reflected_class {
            ctx.bind_reflection_class_local(name, reflected_class);
        }
        if let Some(reflected_function) = reflected_function {
            ctx.bind_reflection_function_local(name, reflected_function);
        }
        if let Some((reflected_class, reflected_property)) = reflected_property {
            ctx.bind_reflection_property_local(name, reflected_class, reflected_property);
        }
        if let Some((reflected_class, reflected_method)) = reflected_method {
            ctx.bind_reflection_method_local(name, reflected_class, reflected_method);
        }
        if let Some(reflected_args) = reflected_args {
            ctx.bind_reflection_arg_array_local(name, reflected_args);
        }
        if let Some(sig) = fiber_start_sig {
            ctx.bind_fiber_start_sig(name, sig);
        }
    } else {
        lower_non_local_assignment_write(ctx, target, value, expr.span);
    }
    if let Some(result_target) = result_target {
        return lower_expr(ctx, result_target);
    }
    result
}

/// Lowers a non-local `??=` assignment expression with lazy RHS evaluation.
pub(super) fn lower_conditional_non_local_null_coalesce_assignment(
    ctx: &mut LoweringContext<'_, '_>,
    temp_name: &str,
    target: &Expr,
    value: &Expr,
    _result_target: Option<&Expr>,
    expr: &Expr,
) -> Option<LoweredValue> {
    let ExprKind::NullCoalesce {
        value: current,
        default,
    } = &value.kind
    else {
        return None;
    };
    // `??=` reads its target the way `??` does — the whole point of the operator is that the
    // target is allowed to be absent — so this must go through the suppressing read rather than
    // a plain one. `$a[$k] ??= 5` on an absent key warned `Undefined array key`, which reference
    // PHP does not, and `$o->p ??= 5` on an uninitialized typed property would fatal.
    let current = lower_null_coalesce_value(ctx, current);
    let is_null = ctx.emit_value(
        Op::IsNull,
        vec![current.value],
        None,
        PhpType::Bool,
        Op::IsNull.default_effects(),
        Some(expr.span),
    );
    let result_type = null_coalesce_result_type(ctx, current.value, default);
    ctx.declare_owned_hidden_temp_with_name(temp_name, result_type.clone());
    let assign_block = ctx.builder.create_named_block("coalesce_assign.default", Vec::new());
    let keep_block = ctx.builder.create_named_block("coalesce_assign.value", Vec::new());
    let merge = ctx.builder.create_named_block("coalesce_assign.merge", Vec::new());
    ctx.builder.terminate(Terminator::CondBr {
        cond: is_null.value,
        then_target: assign_block,
        then_args: Vec::new(),
        else_target: keep_block,
        else_args: Vec::new(),
    });

    ctx.builder.position_at_end(assign_block);
    // The target was read once, before the branch, to decide which way to go. The keep path
    // adopts that value; THIS path discards it, so it has to be released here or it leaks one
    // block per execution — a leak the old over-release used to cancel, which is why the two
    // defects hid each other.
    crate::ir_lower::ownership::release_if_owned(ctx, current, Some(expr.span));
    store_expr_into_temp(ctx, temp_name, result_type.clone(), default, expr.span);
    let temp_value = Expr::new(ExprKind::Variable(temp_name.to_string()), expr.span);
    // The write BORROWS the temporary: `array_set` takes its own reference by retaining, and
    // the slot keeps hers for the merge below to hand to the consumer. One store, one owned
    // load — the merge's — per execution.
    ctx.with_borrowed_write_operand(|ctx| {
        lower_non_local_assignment_write(ctx, target, &temp_value, expr.span);
    });
    branch_to(ctx, merge);

    ctx.builder.position_at_end(keep_block);
    store_value_into_temp(ctx, temp_name, result_type, current, expr.span);
    branch_to(ctx, merge);

    ctx.builder.position_at_end(merge);
    Some(take_owned_temp(ctx, temp_name, expr.span))
}

/// Emits the write side of an assignment expression whose target is not a local variable.
pub(super) fn lower_non_local_assignment_write(
    ctx: &mut LoweringContext<'_, '_>,
    target: &Expr,
    value: &Expr,
    span: Span,
) {
    if let ExprKind::DynamicPropertyAccess { object, property } = &target.kind {
        lower_dynamic_property_assign(ctx, object, property, value, span);
        return;
    }
    let Some(kind) = non_local_assignment_stmt_kind(target, value) else {
        lower_expr(ctx, value);
        return;
    };
    crate::ir_lower::stmt::lower_stmt(ctx, &Stmt::new(kind, span));
}

/// Builds the statement form that already owns lowering for non-local writes.
pub(super) fn non_local_assignment_stmt_kind(target: &Expr, value: &Expr) -> Option<StmtKind> {
    match &target.kind {
        ExprKind::ArrayAccess { array, index } => match &array.kind {
            ExprKind::Variable(array) => Some(StmtKind::ArrayAssign {
                array: array.clone(),
                index: (**index).clone(),
                value: value.clone(),
            }),
            ExprKind::PropertyAccess { object, property } => Some(StmtKind::PropertyArrayAssign {
                object: object.clone(),
                property: property.clone(),
                index: (**index).clone(),
                value: value.clone(),
            }),
            ExprKind::StaticPropertyAccess { receiver, property } => {
                Some(StmtKind::StaticPropertyArrayAssign {
                    receiver: receiver.clone(),
                    property: property.clone(),
                    index: (**index).clone(),
                    value: value.clone(),
                })
            }
            _ => Some(StmtKind::NestedArrayAssign {
                target: target.clone(),
                value: value.clone(),
            }),
        },
        ExprKind::PropertyAccess { object, property } => Some(StmtKind::PropertyAssign {
            object: object.clone(),
            property: property.clone(),
            value: value.clone(),
        }),
        ExprKind::StaticPropertyAccess { receiver, property } => {
            Some(StmtKind::StaticPropertyAssign {
                receiver: receiver.clone(),
                property: property.clone(),
                value: value.clone(),
            })
        }
        _ => None,
    }
}

/// Lowers a runtime-name property write (`$object->{$property} = $value`).
pub(super) fn lower_dynamic_property_assign(
    ctx: &mut LoweringContext<'_, '_>,
    object: &Expr,
    property: &Expr,
    value: &Expr,
    span: Span,
) {
    let object = lower_expr(ctx, object);
    let property_value = lower_expr(ctx, property);
    let property_value = coerce_to_string(ctx, property_value, property);
    let value = lower_expr(ctx, value);
    emit_dynamic_property_readonly_guard(ctx, object.value, property_value.value, span);
    ctx.emit_void(
        Op::DynamicPropSet,
        vec![object.value, property_value.value, value.value],
        None,
        Op::DynamicPropSet.default_effects(),
        Some(span),
    );
    release_owned_call_arg_temporaries(
        ctx,
        &[property_value.value],
        None,
        &ReturnArgAlias::None,
        span,
    );
}

/// Lowers an append through a runtime property name while evaluating every operand once.
pub(crate) fn lower_dynamic_property_array_push(
    ctx: &mut LoweringContext<'_, '_>,
    object: &Expr,
    property: &Expr,
    value: &Expr,
    span: Span,
) {
    let object = lower_expr(ctx, object);
    let property_value = lower_expr(ctx, property);
    let property_value = coerce_to_string(ctx, property_value, property);
    let value = lower_expr(ctx, value);
    emit_dynamic_property_readonly_guard(ctx, object.value, property_value.value, span);
    let current = ctx.emit_value(
        Op::DynamicPropGet,
        vec![object.value, property_value.value],
        None,
        PhpType::Mixed,
        Op::DynamicPropGet.default_effects(),
        Some(span),
    );
    ctx.emit_void(
        Op::MixedArrayAppend,
        vec![current.value, value.value],
        None,
        Op::MixedArrayAppend.default_effects(),
        Some(span),
    );
    ctx.emit_void(
        Op::DynamicPropSet,
        vec![object.value, property_value.value, current.value],
        None,
        Op::DynamicPropSet.default_effects(),
        Some(span),
    );
    release_owned_call_arg_temporaries(
        ctx,
        &[property_value.value],
        None,
        &ReturnArgAlias::None,
        span,
    );
    if ctx.value_is_owning_temporary(object) {
        crate::ir_lower::ownership::release_if_owned(ctx, object, Some(span));
    }
}

/// Emits runtime-name checks for readonly or get-only visible properties.
fn emit_dynamic_property_readonly_guard(
    ctx: &mut LoweringContext<'_, '_>,
    object: ValueId,
    property: ValueId,
    span: Span,
) {
    let PhpType::Object(class_name) = ctx.builder.value_php_type(object).codegen_repr() else {
        return;
    };
    let normalized = class_name.trim_start_matches('\\');
    let Some(class_info) = ctx.classes.get(normalized) else {
        return;
    };
    let candidates = class_info
        .properties
        .iter()
        .enumerate()
        .filter(|(index, (name, _))| class_info.visible_property_index(name) == Some(*index))
        .filter(|(_, (name, _))| {
            property_is_accessible_for_ir(ctx, normalized, class_info, name)
        })
        .filter_map(|(_, (name, _))| {
            if ctx.in_own_property_accessor(name) {
                return None;
            }
            let getter = php_symbol_key(&property_hook_get_method(name));
            let setter = php_symbol_key(&property_hook_set_method(name));
            let readonly = class_info.readonly_properties.contains(name)
                || (class_info.methods.contains_key(&getter)
                    && !class_info.methods.contains_key(&setter));
            readonly.then(|| {
                let declaring_class = class_info
                    .property_declaring_classes
                    .get(name)
                    .cloned()
                    .unwrap_or_else(|| normalized.to_string());
                (name.clone(), declaring_class)
            })
        })
        .collect::<Vec<_>>();

    for (name, declaring_class) in candidates {
        let throw_block = ctx
            .builder
            .create_named_block("dynamic.property.readonly", Vec::new());
        let next_block = ctx
            .builder
            .create_named_block("dynamic.property.writable", Vec::new());
        let name_expr = Expr::new(ExprKind::StringLiteral(name.clone()), span);
        let expected = lower_string_literal(ctx, &name, &name_expr);
        let matches = ctx.emit_value(
            Op::StrictEq,
            vec![property, expected.value],
            None,
            PhpType::Bool,
            Op::StrictEq.default_effects(),
            Some(span),
        );
        ctx.builder.terminate(Terminator::CondBr {
            cond: matches.value,
            then_target: throw_block,
            then_args: Vec::new(),
            else_target: next_block,
            else_args: Vec::new(),
        });
        ctx.builder.position_at_end(throw_block);
        crate::ir_lower::stmt::lower_throw_access_error(
            ctx,
            &format!("Cannot modify readonly property {}::${}", declaring_class, name),
            span,
        );
        ctx.builder.position_at_end(next_block);
    }
}

/// Lowers pre/post increment and decrement expressions.
///
/// Three paths, all of which can retype the local, so all of them store a boxed Mixed:
/// - a `Str` or boxed `Mixed` local goes through [`lower_str_inc_dec`], which applies PHP's
///   string rules (`"az"++` is `"ba"`, `"9"++` is `int(10)`) to a string payload and keeps
///   every other payload on the existing numeric helper;
/// - a `Float` local adds or subtracts exactly `1.0` and stays a float;
/// - an `Int` local uses the checked helper, so PHP's overflow promotion applies
///   (`PHP_INT_MAX + 1` becomes float).
///
/// The post-forms return the value the local held before the store; the pre-forms re-read
/// the local afterwards.
pub(super) fn lower_inc_dec(
    ctx: &mut LoweringContext<'_, '_>,
    name: &str,
    increment: bool,
    post: bool,
    expr: &Expr,
) -> LoweredValue {
    let old = ctx.load_local(name, Some(expr.span));
    let existing_type = ctx.local_type(name);
    if matches!(existing_type.codegen_repr(), PhpType::Mixed | PhpType::Str) {
        let return_old = if post {
            crate::ir_lower::ownership::acquire_if_refcounted(ctx, old, Some(expr.span))
        } else {
            old
        };
        let new = lower_str_inc_dec(ctx, old, increment, expr);
        ctx.store_local(name, new, PhpType::Mixed, Some(expr.span));
        return if post {
            return_old
        } else {
            ctx.load_local(name, Some(expr.span))
        };
    }
    if matches!(existing_type.codegen_repr(), PhpType::Float) {
        return lower_float_inc_dec(ctx, name, increment, post, old, expr);
    }
    let one = lower_int_literal(ctx, 1, expr);
    let operand = coerce_to_int(ctx, old, expr);
    let checked_int_local = matches!(existing_type.codegen_repr(), PhpType::Int);
    let iop = match (increment, checked_int_local) {
        (true, true) => Op::ICheckedAdd,
        (false, true) => Op::ICheckedSub,
        (true, false) => Op::IAdd,
        (false, false) => Op::ISub,
    };
    let result_php_type = if checked_int_local { PhpType::Mixed } else { PhpType::Int };
    let result_ir_type = if checked_int_local {
        IrType::Heap(IrHeapKind::Mixed)
    } else {
        IrType::I64
    };
    let new = ctx
        .builder
        .emit_with_effects(
            iop,
            vec![operand.value, one.value],
            None,
            result_ir_type,
            result_php_type.clone(),
            Ownership::for_php_type(&result_php_type),
            iop.default_effects(),
            Some(expr.span),
        )
        .expect("integer inc/dec produces a value");
    let new = LoweredValue { value: new, ir_type: result_ir_type };
    ctx.store_local(name, new, result_php_type, Some(expr.span));
    if post {
        old
    } else {
        ctx.load_local(name, Some(expr.span))
    }
}
