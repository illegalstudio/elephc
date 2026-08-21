//! Purpose:
//! Object construction, clone, and ReflectionParameter constructor operands.
//!
//! Called from:
//! - `crate::ir_lower::expr`.
//!
//! Key details:
//! - Preserves source-order evaluation, EIR typing, effects, and ownership contracts.

use super::*;

/// Lowers fixed-class object construction.
pub(super) fn lower_new_object(
    ctx: &mut LoweringContext<'_, '_>,
    class_name: &Name,
    args: &[Expr],
    expr: &Expr,
) -> LoweredValue {
    if let Some(opcode) = crate::ir_lower::internal_extensions::method_opcode(
        ctx,
        class_name.as_str(),
        "__construct",
    ) {
        let sig = constructor_signature(ctx, class_name).cloned();
        let operands = lower_internal_extension_args(ctx, sig.as_ref(), args, false);
        let (operands, argument_guards) =
            prepare_internal_extension_arguments_for_throw(ctx, sig.as_ref(), operands, expr.span);
        let call = crate::ir_lower::internal_extensions::emit_call(
            ctx,
            opcode,
            crate::ir_lower::internal_extensions::FLAG_WRAPPER_RESULT,
            operands.clone(),
            PhpType::Object(class_name.as_str().to_string()),
            expr.span,
        );
        clear_owning_call_arg_temporary_guards(ctx, &argument_guards, expr.span);
        release_owned_call_arg_temporaries(
            ctx,
            &operands,
            Some(call.value),
            &ReturnArgAlias::None,
            expr.span,
        );
        return call;
    }
    if php_symbol_key(class_name.as_str().trim_start_matches('\\')) == "reflectionclass" {
        if let Some(operands) = lower_reflection_class_constructor_operands(ctx, args) {
            let php_type = PhpType::Object(class_name.as_str().to_string());
            return emit_fixed_object_new(ctx, class_name.as_str(), operands, php_type, expr.span);
        }
    }
    if php_symbol_key(class_name.as_str().trim_start_matches('\\')) == "reflectionparameter" {
        if let Some(operands) = lower_reflection_parameter_constructor_operands(ctx, args) {
            let php_type = PhpType::Object(class_name.as_str().to_string());
            return emit_fixed_object_new(ctx, class_name.as_str(), operands, php_type, expr.span);
        }
    }
    if php_symbol_key(class_name.as_str().trim_start_matches('\\')) == "reflectionmethod" {
        if let Some(operands) = lower_reflection_method_constructor_operands(ctx, args) {
            let php_type = PhpType::Object(class_name.as_str().to_string());
            return emit_fixed_object_new(ctx, class_name.as_str(), operands, php_type, expr.span);
        }
    }
    if ctx.has_eval_barrier()
        && !ctx.classes.contains_key(class_name.as_str())
        && plain_positional_call_args(args)
    {
        let operands = lower_args_with_signature(ctx, None, args);
        let data = ctx.intern_class_name(class_name.as_str());
        return ctx.emit_value(
            Op::EvalObjectNew,
            operands,
            Some(Immediate::Data(data)),
            PhpType::Mixed,
            Op::EvalObjectNew.default_effects(),
            Some(expr.span),
        );
    }
    let sig = constructor_signature(ctx, class_name).cloned();
    let operands = lower_args_with_signature(ctx, sig.as_ref(), args);
    let php_type = PhpType::Object(class_name.as_str().to_string());
    emit_fixed_object_new(ctx, class_name.as_str(), operands, php_type, expr.span)
}

/// Emits fixed-class object construction and releases owned constructor argument temporaries.
///
/// A newly allocated object cannot alias a constructor argument. The constructor has already
/// retained or copied every argument it keeps by the time `ObjectNew` returns, so the caller's
/// owning temporary references can be dropped without the general call-result alias guard.
pub(super) fn emit_fixed_object_new(
    ctx: &mut LoweringContext<'_, '_>,
    class_name: &str,
    operands: Vec<ValueId>,
    php_type: PhpType,
    span: Span,
) -> LoweredValue {
    let data = ctx.intern_class_name(class_name);
    let object = ctx.emit_value(
        Op::ObjectNew,
        operands.clone(),
        Some(Immediate::Data(data)),
        php_type,
        Op::ObjectNew.default_effects(),
        Some(span),
    );
    release_owned_call_arg_temporaries(
        ctx,
        &operands,
        None,
        &ReturnArgAlias::None,
        span,
    );
    object
}

/// Lowers `ReflectionClass(object)` while preserving object operands for runtime class metadata.
pub(super) fn lower_reflection_class_constructor_operands(
    ctx: &mut LoweringContext<'_, '_>,
    args: &[Expr],
) -> Option<Vec<ValueId>> {
    let reflected_arg = reflection_class_constructor_class_arg(ctx, args)?;
    let class_name = instance_callable_object_class(ctx, &reflected_arg)?;
    let lowered = lower_expr(ctx, &reflected_arg);
    if matches!(
        ctx.builder.value_php_type(lowered.value).codegen_repr(),
        PhpType::Object(_)
    ) {
        return Some(vec![lowered.value]);
    }
    if ctx.value_is_owning_temporary(lowered) {
        crate::ir_lower::ownership::release_if_owned(ctx, lowered, Some(reflected_arg.span));
    }
    let data = ctx.intern_class_name(&class_name);
    let value = ctx.emit_value(
        Op::ConstClassName,
        Vec::new(),
        Some(Immediate::Data(data)),
        PhpType::Str,
        Op::ConstClassName.default_effects(),
        Some(reflected_arg.span),
    );
    Some(vec![value.value])
}

/// Lowers direct `ReflectionMethod` constructor operands to literal class and method names.
pub(super) fn lower_reflection_method_constructor_operands(
    ctx: &mut LoweringContext<'_, '_>,
    args: &[Expr],
) -> Option<Vec<ValueId>> {
    let (class_arg, method_arg) = reflection_method_constructor_regular_args(ctx, args)?;
    Some(vec![
        lower_expr(ctx, &class_arg).value,
        lower_expr(ctx, &method_arg).value,
    ])
}

/// Lowers PHP `clone $object` to a shallow object-copy opcode and optional `__clone()` hook.
pub(super) fn lower_clone(ctx: &mut LoweringContext<'_, '_>, inner: &Expr, expr: &Expr) -> LoweredValue {
    let object = lower_expr(ctx, inner);
    let object_ty = ctx.builder.value_php_type(object.value);
    if let Some(PhpType::Object(class_name)) =
        crate::ir_lower::internal_extensions::simplexml_object_result_type(ctx, &object_ty)
    {
        return lower_simplexml_clone(ctx, object, &object_ty, &class_name, expr);
    }
    let Some((class_name, false)) = singular_object_class(&object_ty) else {
        unreachable!("clone expressions must be type-checked as non-null objects before lowering");
    };
    let class_name = class_name.to_string();
    let result_ty = PhpType::Object(class_name.clone());
    let cloned = if crate::internal_extensions::is_native_wrapper_class(&class_name)
        || crate::internal_extensions::is_native_wrapper_descendant(ctx.classes, &class_name)
    {
        let opcode = crate::ir_lower::internal_extensions::operation_opcode(
            "internal:bridge.object.clone",
        )
        .expect("locked DOM surface must contain bridge object cloning");
        crate::ir_lower::internal_extensions::emit_call(
            ctx,
            opcode,
            crate::ir_lower::internal_extensions::FLAG_RECEIVER
                | crate::ir_lower::internal_extensions::FLAG_WRAPPER_RESULT,
            vec![object.value],
            result_ty,
            expr.span,
        )
    } else {
        let data = ctx.intern_class_name(&class_name);
        ctx.emit_value(
            Op::ObjectCloneShallow,
            vec![object.value],
            Some(Immediate::Data(data)),
            result_ty,
            Op::ObjectCloneShallow.default_effects(),
            Some(expr.span),
        )
    };
    if !crate::ir_lower::internal_extensions::is_simplexml_element_class(ctx, &class_name)
        && class_method_signature(ctx, &class_name, &php_symbol_key("__clone")).is_some()
    {
        lower_method_call_with_receiver(ctx, cloned, "__clone", &[], Op::MethodCall, expr);
    }
    cloned
}

/// Clones a direct or fallible SimpleXML result after preserving PHP's runtime TypeErrors.
fn lower_simplexml_clone(
    ctx: &mut LoweringContext<'_, '_>,
    object: LoweredValue,
    object_ty: &PhpType,
    class_name: &str,
    expr: &Expr,
) -> LoweredValue {
    let has_false = union_contains_clone_failure(object_ty, PhpType::False);
    let has_null = union_contains_clone_failure(object_ty, PhpType::Void);
    let receiver_guard = guard_owning_receiver_temporary_for_throw(ctx, object, expr.span);
    if !has_false && !has_null {
        let cloned = emit_simplexml_clone_call(ctx, object, class_name, expr);
        release_guarded_owning_receiver_temporary(ctx, object, receiver_guard.as_deref(), expr.span);
        return cloned;
    }
    let result_ty = PhpType::Object(class_name.to_string());
    let result_temp = ctx.declare_owned_hidden_temp(result_ty.clone());
    let live_block = ctx.builder.create_named_block("simplexml.clone.object", Vec::new());
    let merge_block = ctx.builder.create_named_block("simplexml.clone.merge", Vec::new());
    if has_false {
        let false_block = ctx.builder.create_named_block("simplexml.clone.false", Vec::new());
        let non_false_block = if has_null {
            ctx.builder.create_named_block("simplexml.clone.not_false", Vec::new())
        } else {
            live_block
        };
        let false_value = emit_bool_literal(ctx, false, Some(expr.span));
        let is_false = ctx.emit_value(
            Op::StrictEq,
            vec![object.value, false_value.value],
            None,
            PhpType::Bool,
            Op::StrictEq.default_effects(),
            Some(expr.span),
        );
        ctx.builder.terminate(Terminator::CondBr {
            cond: is_false.value,
            then_target: false_block,
            then_args: Vec::new(),
            else_target: non_false_block,
            else_args: Vec::new(),
        });
        ctx.builder.position_at_end(false_block);
        terminate_simplexml_clone_type_error(ctx, "false", expr.span);
        if has_null {
            ctx.builder.position_at_end(non_false_block);
        }
    }
    if has_null {
        let null_block = ctx.builder.create_named_block("simplexml.clone.null", Vec::new());
        let is_null = ctx.emit_value(
            Op::IsNull,
            vec![object.value],
            None,
            PhpType::Bool,
            Op::IsNull.default_effects(),
            Some(expr.span),
        );
        ctx.builder.terminate(Terminator::CondBr {
            cond: is_null.value,
            then_target: null_block,
            then_args: Vec::new(),
            else_target: live_block,
            else_args: Vec::new(),
        });
        ctx.builder.position_at_end(null_block);
        terminate_simplexml_clone_type_error(ctx, "null", expr.span);
    }
    ctx.builder.position_at_end(live_block);
    let cloned = emit_simplexml_clone_call(ctx, object, class_name, expr);
    release_guarded_owning_receiver_temporary(ctx, object, receiver_guard.as_deref(), expr.span);
    store_value_into_temp(ctx, &result_temp, result_ty, cloned, expr.span);
    branch_to(ctx, merge_block);
    ctx.builder.position_at_end(merge_block);
    take_owned_temp(ctx, &result_temp, expr.span)
}

/// Returns whether a loader result union contains a clone failure member.
fn union_contains_clone_failure(object_ty: &PhpType, failure: PhpType) -> bool {
    matches!(object_ty, PhpType::Union(members) if members.contains(&failure))
}

/// Emits the native clone operation once its SimpleXML receiver has been guarded.
fn emit_simplexml_clone_call(
    ctx: &mut LoweringContext<'_, '_>,
    object: LoweredValue,
    class_name: &str,
    expr: &Expr,
) -> LoweredValue {
    let opcode = crate::ir_lower::internal_extensions::operation_opcode(
        "internal:bridge.object.clone",
    )
    .expect("locked DOM surface must contain bridge object cloning");
    crate::ir_lower::internal_extensions::emit_call(
        ctx,
        opcode,
        crate::ir_lower::internal_extensions::FLAG_RECEIVER
            | crate::ir_lower::internal_extensions::FLAG_WRAPPER_RESULT,
        vec![object.value],
        PhpType::Object(class_name.to_string()),
        expr.span,
    )
}

/// Throws the catchable PHP `TypeError` produced when cloning a failed loader result.
fn terminate_simplexml_clone_type_error(ctx: &mut LoweringContext<'_, '_>, given: &str, span: Span) {
    let message = format!("clone(): Argument #1 ($object) must be of type object, {given} given");
    let exception = Expr::new(
        ExprKind::NewObject {
            class_name: Name::unqualified("TypeError"),
            args: vec![Expr::new(ExprKind::StringLiteral(message), span)],
        },
        span,
    );
    let exception = lower_expr(ctx, &exception);
    ctx.builder.terminate(Terminator::Throw { value: exception.value });
}

/// Metadata operand source for direct `ReflectionParameter` constructor lowering.
pub(super) enum ReflectionParameterConstructorOperand {
    Expr(Expr),
    ClassName { name: String, span: Span },
    ObjectExpr { expr: Expr, span: Span },
}

/// Lowers validated `ReflectionParameter` constructor arguments into metadata operands.
///
/// Method targets lower as `[class, method, parameter]`; function targets lower
/// as `[function, parameter]`.
pub(super) fn lower_reflection_parameter_constructor_operands(
    ctx: &mut LoweringContext<'_, '_>,
    args: &[Expr],
) -> Option<Vec<ValueId>> {
    let arg_exprs = reflection_parameter_constructor_arg_exprs(ctx, args)?;
    Some(
        arg_exprs
            .iter()
            .map(|arg| lower_reflection_parameter_constructor_operand(ctx, arg))
            .collect(),
    )
}

/// Lowers one direct `ReflectionParameter` metadata operand.
pub(super) fn lower_reflection_parameter_constructor_operand(
    ctx: &mut LoweringContext<'_, '_>,
    operand: &ReflectionParameterConstructorOperand,
) -> ValueId {
    match operand {
        ReflectionParameterConstructorOperand::Expr(expr) => lower_expr(ctx, expr).value,
        ReflectionParameterConstructorOperand::ObjectExpr { expr, span } => {
            let object = lower_expr(ctx, expr);
            let class_name = reflection_parameter_lowered_object_class_name(ctx, object.value)
                .expect("ReflectionParameter object target must be type-checked as a known object");
            if ctx.value_is_owning_temporary(object) {
                crate::ir_lower::ownership::release_if_owned(ctx, object, Some(*span));
            }
            emit_reflection_parameter_class_name_operand(ctx, &class_name, *span)
        }
        ReflectionParameterConstructorOperand::ClassName { name, span } => {
            emit_reflection_parameter_class_name_operand(ctx, name, *span)
        }
    }
}

/// Emits one class-name operand for direct `ReflectionParameter` metadata.
pub(super) fn emit_reflection_parameter_class_name_operand(
    ctx: &mut LoweringContext<'_, '_>,
    name: &str,
    span: Span,
) -> ValueId {
    let data = ctx.intern_class_name(name);
    ctx.emit_value(
        Op::ConstClassName,
        Vec::new(),
        Some(Immediate::Data(data)),
        PhpType::Str,
        Op::ConstClassName.default_effects(),
        Some(span),
    )
    .value
}

/// Returns metadata operand expressions from a normalized static `ReflectionParameter` call.
pub(super) fn reflection_parameter_constructor_arg_exprs(
    ctx: &LoweringContext<'_, '_>,
    args: &[Expr],
) -> Option<Vec<ReflectionParameterConstructorOperand>> {
    let args = expand_static_call_spread_args(args);
    if args.iter().any(is_spread_arg) {
        return None;
    }
    let (target, parameter) = if crate::types::call_args::has_named_args(&args) {
        let sig = ctx
            .classes
            .get("ReflectionParameter")
            .and_then(|class_info| class_info.methods.get("__construct"))?;
        let call_span = args
            .first()
            .map(|arg| arg.span)
            .unwrap_or_else(crate::span::Span::dummy);
        let plan =
            crate::types::call_args::plan_call_args_with_regular_param_count_and_assoc_spreads(
                sig,
                &args,
                call_span,
                crate::types::call_args::regular_param_count(sig),
                false,
                true,
                &assoc_spread_sources(ctx, &args),
            )
            .ok()?;
        if plan.has_spread_args() {
            return None;
        }
        (
            planned_regular_arg_expr(plan.regular_args.first()?)?.clone(),
            planned_regular_arg_expr(plan.regular_args.get(1)?)?.clone(),
        )
    } else {
        (args.first()?.clone(), args.get(1)?.clone())
    };
    match &target.kind {
        ExprKind::ArrayLiteral(items) if items.len() == 2 => {
            let owner = reflection_parameter_method_owner_operand(ctx, &items[0])?;
            Some(vec![
                owner,
                ReflectionParameterConstructorOperand::Expr(items[1].clone()),
                ReflectionParameterConstructorOperand::Expr(parameter),
            ])
        }
        ExprKind::StringLiteral(_) => Some(vec![
            ReflectionParameterConstructorOperand::Expr(target),
            ReflectionParameterConstructorOperand::Expr(parameter),
        ]),
        _ => None,
    }
}

/// Returns the static class-name operand for a ReflectionParameter method target.
pub(super) fn reflection_parameter_method_owner_operand(
    ctx: &LoweringContext<'_, '_>,
    owner: &Expr,
) -> Option<ReflectionParameterConstructorOperand> {
    match &owner.kind {
        ExprKind::StringLiteral(name) => Some(ReflectionParameterConstructorOperand::ClassName {
            name: name.clone(),
            span: owner.span,
        }),
        ExprKind::ClassConstant { receiver } => {
            static_receiver_class_name(ctx, receiver).map(|name| {
                ReflectionParameterConstructorOperand::ClassName {
                    name,
                    span: owner.span,
                }
            })
        }
        ExprKind::Variable(name) => {
            let PhpType::Object(class_name) = ctx.local_type(name).codegen_repr() else {
                return None;
            };
            if class_name.is_empty() {
                return None;
            }
            Some(ReflectionParameterConstructorOperand::ClassName {
                name: class_name,
                span: owner.span,
            })
        }
        ExprKind::This => {
            ctx.current_class
                .clone()
                .map(|name| ReflectionParameterConstructorOperand::ClassName {
                    name,
                    span: owner.span,
                })
        }
        _ => Some(ReflectionParameterConstructorOperand::ObjectExpr {
            expr: owner.clone(),
            span: owner.span,
        }),
    }
}

/// Returns the concrete class name from a lowered object target.
pub(super) fn reflection_parameter_lowered_object_class_name(
    ctx: &LoweringContext<'_, '_>,
    value: ValueId,
) -> Option<String> {
    let PhpType::Object(class_name) = ctx.builder.value_php_type(value).codegen_repr() else {
        return None;
    };
    if class_name.is_empty() || !ctx.classes.contains_key(class_name.as_str()) {
        return None;
    }
    Some(class_name)
}

/// Lowers PHP `new $class(...)` into the generic dynamic-new EIR opcode.
///
/// Arguments go through the same shared normalization as every other call surface first:
/// statically-known spreads are flattened to positional/named arguments (`f(...[1, 2])`
/// behaves like `f(1, 2)` and `f(...["a" => 1])` like `f(a: 1)`). The generic dynamic-new
/// opcode passes operands straight through to a runtime class-name dispatch that matches
/// candidates by exact constructor arity, so any call shape that needs per-class planning
/// (named arguments, omitted optional parameters, runtime spreads) is lowered as an
/// explicit class-name dispatch chain instead, where each branch constructs a fixed class
/// through `lower_new_object` and therefore reuses `plan_call_args` in full.
pub(super) fn lower_new_dynamic(
    ctx: &mut LoweringContext<'_, '_>,
    name_expr: &Expr,
    args: &[Expr],
    expr: &Expr,
) -> LoweredValue {
    let args = expand_static_call_spread_args(args);
    if let Some(value) = lower_new_dynamic_planned_dispatch(ctx, name_expr, &args, expr) {
        return value;
    }
    let name_value = lower_expr(ctx, name_expr);
    lower_new_dynamic_generic(ctx, name_value, &args, expr)
}

/// Emits the generic runtime class-name dispatch for `new $class(...)`.
///
/// The class-name operand is already lowered so both the direct path and the
/// planned-dispatch fallback branch can share it.
///
/// Static spread flattening and planned dispatch run before this, so most call shapes
/// arrive as plain positional arguments. What survives both — a spread whose operand is
/// only known at runtime, or named arguments the planner could not resolve to a class —
/// is passed through the runtime argument container rather than dropped on the floor.
fn lower_new_dynamic_generic(
    ctx: &mut LoweringContext<'_, '_>,
    name_value: LoweredValue,
    args: &[Expr],
    expr: &Expr,
) -> LoweredValue {
    let mut operands = vec![name_value.value];
    let uses_runtime_arg_container =
        args.iter().any(is_spread_arg) || crate::types::call_args::has_named_args(args);
    if uses_runtime_arg_container {
        let arg_container = lower_untyped_descriptor_invoker_arg_container(ctx, args, expr.span)
            .expect("dynamic constructor arguments always have a runtime container form");
        operands.push(arg_container.value);
    } else {
        operands.extend(lower_args(ctx, args));
    }
    ctx.emit_value(
        Op::DynamicObjectNewMixed,
        operands,
        uses_runtime_arg_container.then_some(Immediate::Bool(true)),
        PhpType::Mixed,
        Op::DynamicObjectNewMixed.default_effects(),
        Some(expr.span),
    )
}

/// Lowers dynamic object construction.
pub(super) fn lower_new_dynamic_object(
    ctx: &mut LoweringContext<'_, '_>,
    class_name: &Expr,
    fallback_class: &Name,
    required_parent: &Name,
    args: &[Expr],
    expr: &Expr,
) -> LoweredValue {
    let mut operands = vec![lower_expr(ctx, class_name).value];
    operands.extend(lower_args(ctx, args));
    let name = format!("{}|{}", fallback_class.as_str(), required_parent.as_str());
    let data = ctx.intern_class_name(&name);
    ctx.emit_value(
        Op::DynamicObjectNew,
        operands,
        Some(Immediate::Data(data)),
        PhpType::Object(fallback_class.as_str().to_string()),
        Op::DynamicObjectNew.default_effects(),
        Some(expr.span),
    )
}

/// Returns constructor signature metadata when available for a fixed class.
pub(super) fn constructor_signature<'a>(
    ctx: &'a LoweringContext<'_, '_>,
    class_name: &Name,
) -> Option<&'a FunctionSig> {
    let key = php_symbol_key("__construct");
    ctx.classes
        .get(class_name.as_str().trim_start_matches('\\'))
        .and_then(|class_info| class_info.methods.get(&key))
}
