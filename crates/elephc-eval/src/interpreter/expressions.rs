//! Purpose:
//! Evaluates EvalIR expressions, match expressions, function-like calls, and positional builtin dispatch.
//!
//! Called from:
//! - `crate::interpreter::statements` for expression statements and expression-bearing statements.
//! - Eval builtin modules when they need to evaluate unevaluated argument expressions.
//!
//! Key details:
//! - PHP call argument evaluation order is preserved before binding or ABI-like materialization.
//! - Language constructs such as `eval`, `isset`, `empty`, and `unset` receive unevaluated expressions.

use super::*;

/// Evaluates one expression to an opaque runtime-cell handle.
pub(in crate::interpreter) fn eval_expr(
    expr: &EvalExpr,
    context: &mut ElephcEvalContext,
    scope: &mut ElephcEvalScope,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    match expr {
        EvalExpr::Array(elements) => {
            if elements
                .iter()
                .any(|element| matches!(element, EvalArrayElement::KeyValue { .. }))
            {
                eval_assoc_array(elements, context, scope, values)
            } else {
                eval_indexed_array(elements, context, scope, values)
            }
        }
        EvalExpr::ArrayGet { array, index } => {
            let array = eval_expr(array, context, scope, values)?;
            let index = eval_expr(index, context, scope, values)?;
            values.array_get(array, index)
        }
        EvalExpr::Call { name, args } => eval_call(name, args, context, scope, values),
        EvalExpr::Const(value) => eval_const(value, values),
        EvalExpr::ConstFetch(name) => eval_const_fetch(name, context, values),
        EvalExpr::DynamicCall { callee, args } => {
            eval_dynamic_call(callee, args, context, scope, values)
        }
        EvalExpr::Include {
            path,
            required,
            once,
        } => eval_include_expr(path, *required, *once, context, scope, values),
        EvalExpr::InstanceOf { value, target } => {
            eval_instanceof_expr(value, target, context, scope, values)
        }
        EvalExpr::LoadVar(name) => {
            visible_scope_cell(context, scope, name).map_or_else(|| values.null(), Ok)
        }
        EvalExpr::Magic(magic) => eval_magic_const(magic, context, values),
        EvalExpr::Match {
            subject,
            arms,
            default,
        } => eval_match_expr(subject, arms, default.as_deref(), context, scope, values),
        EvalExpr::Clone(object) => {
            let object = eval_expr(object, context, scope, values)?;
            eval_object_clone_result(object, context, values)
        }
        EvalExpr::NamespacedCall {
            name,
            fallback_name,
            args,
        } => eval_namespaced_call(name, fallback_name, args, context, scope, values),
        EvalExpr::NamespacedConstFetch {
            name,
            fallback_name,
        } => eval_namespaced_const_fetch(name, fallback_name, context, values),
        EvalExpr::NewObject { class_name, args } => {
            let args = eval_method_call_arg_values(args, context, scope, values)?;
            let class_name = eval_new_object_class_name(class_name, context)?;
            if let Some(object) =
                eval_reflection_owner_new_object(&class_name, args.clone(), context, values)?
            {
                return Ok(object);
            }
            if let Some(class) = context.class(&class_name).cloned() {
                eval_dynamic_class_new_object(&class, args, context, scope, values)
            } else {
                let args = bind_native_callable_args(
                    context.native_constructor_signature(&class_name),
                    args,
                    values,
                )?;
                values
                    .new_object(&class_name)
                    .and_then(|object| values.construct_object(object, args).map(|()| object))
            }
        }
        EvalExpr::NewAnonymousClass { class, args } => {
            ensure_eval_anonymous_class_decl(class, context, scope, values)?;
            let evaluated_args = eval_method_call_arg_values(args, context, scope, values)?;
            let class = context
                .class(class.name())
                .cloned()
                .ok_or(EvalStatus::RuntimeFatal)?;
            eval_dynamic_class_new_object(&class, evaluated_args, context, scope, values)
        }
        EvalExpr::StaticMethodCall {
            class_name,
            method,
            args,
        } => {
            let evaluated_args = eval_method_call_arg_values(args, context, scope, values)?;
            eval_static_method_call_result(class_name, method, evaluated_args, context, values)
        }
        EvalExpr::StaticPropertyGet {
            class_name,
            property,
        } => eval_static_property_get_result(class_name, property, context, values),
        EvalExpr::ClassConstantFetch {
            class_name,
            constant,
        } => eval_class_constant_fetch_result(class_name, constant, context, values),
        EvalExpr::ClassNameFetch { class_name } => {
            eval_class_name_fetch_result(class_name, context, values)
        }
        EvalExpr::MethodCall {
            object,
            method,
            args,
        } => {
            let object = eval_expr(object, context, scope, values)?;
            let evaluated_args = eval_method_call_arg_values(args, context, scope, values)?;
            eval_method_call_result_with_evaluated_args(
                object,
                method,
                evaluated_args,
                context,
                values,
            )
        }
        EvalExpr::NullCoalesce { value, default } => {
            let value = eval_expr(value, context, scope, values)?;
            if values.is_null(value)? {
                eval_expr(default, context, scope, values)
            } else {
                Ok(value)
            }
        }
        EvalExpr::PropertyGet { object, property } => {
            let object = eval_expr(object, context, scope, values)?;
            eval_property_get_result(object, property, context, values)
        }
        EvalExpr::Print(inner) => {
            let value = eval_expr(inner, context, scope, values)?;
            let value = eval_string_context_value(value, context, values)?;
            values.echo(value)?;
            values.int(1)
        }
        EvalExpr::Ternary {
            condition,
            then_branch,
            else_branch,
        } => {
            let condition = eval_expr(condition, context, scope, values)?;
            if values.truthy(condition)? {
                if let Some(then_branch) = then_branch {
                    eval_expr(then_branch, context, scope, values)
                } else {
                    Ok(condition)
                }
            } else {
                eval_expr(else_branch, context, scope, values)
            }
        }
        EvalExpr::Unary { op, expr } => {
            let value = eval_expr(expr, context, scope, values)?;
            match op {
                EvalUnaryOp::Plus => {
                    let zero = values.int(0)?;
                    values.add(zero, value)
                }
                EvalUnaryOp::Negate => {
                    let zero = values.int(0)?;
                    values.sub(zero, value)
                }
                EvalUnaryOp::LogicalNot => {
                    let truthy = values.truthy(value)?;
                    values.bool_value(!truthy)
                }
                EvalUnaryOp::BitNot => values.bit_not(value),
            }
        }
        EvalExpr::Binary { op, left, right } => {
            if *op == EvalBinOp::LogicalAnd {
                let left = eval_expr(left, context, scope, values)?;
                if !values.truthy(left)? {
                    return values.bool_value(false);
                }
                let right = eval_expr(right, context, scope, values)?;
                let truthy = values.truthy(right)?;
                return values.bool_value(truthy);
            }
            if *op == EvalBinOp::LogicalOr {
                let left = eval_expr(left, context, scope, values)?;
                if values.truthy(left)? {
                    return values.bool_value(true);
                }
                let right = eval_expr(right, context, scope, values)?;
                let truthy = values.truthy(right)?;
                return values.bool_value(truthy);
            }
            let left = eval_expr(left, context, scope, values)?;
            let right = eval_expr(right, context, scope, values)?;
            match op {
                EvalBinOp::Add => values.add(left, right),
                EvalBinOp::Sub => values.sub(left, right),
                EvalBinOp::Mul => values.mul(left, right),
                EvalBinOp::Div => values.div(left, right),
                EvalBinOp::Mod => values.modulo(left, right),
                EvalBinOp::Pow => values.pow(left, right),
                EvalBinOp::BitAnd
                | EvalBinOp::BitOr
                | EvalBinOp::BitXor
                | EvalBinOp::ShiftLeft
                | EvalBinOp::ShiftRight => values.bitwise(*op, left, right),
                EvalBinOp::Concat => {
                    let left = eval_string_context_value(left, context, values)?;
                    let right = eval_string_context_value(right, context, values)?;
                    values.concat(left, right)
                }
                EvalBinOp::LogicalXor => {
                    let left_truthy = values.truthy(left)?;
                    let right_truthy = values.truthy(right)?;
                    values.bool_value(left_truthy ^ right_truthy)
                }
                EvalBinOp::LooseEq
                | EvalBinOp::LooseNotEq
                | EvalBinOp::StrictEq
                | EvalBinOp::StrictNotEq
                | EvalBinOp::Lt
                | EvalBinOp::LtEq
                | EvalBinOp::Gt
                | EvalBinOp::GtEq => values.compare(*op, left, right),
                EvalBinOp::Spaceship => values.spaceship(left, right),
                EvalBinOp::LogicalAnd | EvalBinOp::LogicalOr => {
                    Err(EvalStatus::UnsupportedConstruct)
                }
            }
        }
    }
}

/// Resolves special class names used by `new` while preserving AOT fallback names.
fn eval_new_object_class_name(
    class_name: &str,
    context: &ElephcEvalContext,
) -> Result<String, EvalStatus> {
    match class_name.to_ascii_lowercase().as_str() {
        "self" | "parent" | "static" => resolve_eval_static_class_name(class_name, context),
        _ => Ok(context
            .resolve_class_name(class_name)
            .unwrap_or_else(|| class_name.trim_start_matches('\\').to_string())),
    }
}

/// Evaluates PHP's `instanceof` operator over static and dynamic class targets.
fn eval_instanceof_expr(
    value: &EvalExpr,
    target: &EvalInstanceOfTarget,
    context: &mut ElephcEvalContext,
    scope: &mut ElephcEvalScope,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    let value = eval_expr(value, context, scope, values)?;
    let result = match target {
        EvalInstanceOfTarget::ClassName(class_name) => {
            if values.type_tag(value)? != EVAL_TAG_OBJECT {
                return values.bool_value(false);
            }
            let target_class = eval_instanceof_static_target_name(class_name, context)?;
            eval_instanceof_object_result(value, &target_class, context, values)?
        }
        EvalInstanceOfTarget::Expr(target) => {
            let target = eval_expr(target, context, scope, values)?;
            let target_class = eval_instanceof_dynamic_target_name(target, context, values)?;
            if values.type_tag(value)? == EVAL_TAG_OBJECT {
                eval_instanceof_object_result(value, &target_class, context, values)?
            } else {
                false
            }
        }
    };
    values.bool_value(result)
}

/// Resolves a static `instanceof` target according to eval class aliases and scope keywords.
fn eval_instanceof_static_target_name(
    class_name: &str,
    context: &ElephcEvalContext,
) -> Result<String, EvalStatus> {
    match class_name.to_ascii_lowercase().as_str() {
        "self" | "parent" | "static" => resolve_eval_static_class_name(class_name, context),
        _ => Ok(eval_instanceof_resolved_target_name(class_name, context)),
    }
}

/// Resolves a dynamic `instanceof` target cell to the PHP class name it represents.
fn eval_instanceof_dynamic_target_name(
    target: RuntimeCellHandle,
    context: &ElephcEvalContext,
    values: &mut impl RuntimeValueOps,
) -> Result<String, EvalStatus> {
    match values.type_tag(target)? {
        EVAL_TAG_STRING => {
            let target = eval_instanceof_string_target_name(target, values)?;
            Ok(eval_instanceof_resolved_target_name(&target, context))
        }
        EVAL_TAG_OBJECT => eval_instanceof_object_target_name(target, context, values),
        _ => Err(EvalStatus::RuntimeFatal),
    }
}

/// Reads and normalizes one string-valued dynamic `instanceof` target.
fn eval_instanceof_string_target_name(
    target: RuntimeCellHandle,
    values: &mut impl RuntimeValueOps,
) -> Result<String, EvalStatus> {
    let bytes = values.string_bytes(target)?;
    let target = String::from_utf8(bytes).map_err(|_| EvalStatus::RuntimeFatal)?;
    Ok(target.trim_start_matches('\\').to_string())
}

/// Reads the runtime class of an object-valued dynamic `instanceof` target.
fn eval_instanceof_object_target_name(
    target: RuntimeCellHandle,
    context: &ElephcEvalContext,
    values: &mut impl RuntimeValueOps,
) -> Result<String, EvalStatus> {
    let identity = values.object_identity(target)?;
    if let Some(class) = context.dynamic_object_class(identity) {
        return Ok(class.name().to_string());
    }
    let class_name = values.object_class_name(target)?;
    let bytes = values.string_bytes(class_name);
    values.release(class_name)?;
    let class_name = String::from_utf8(bytes?).map_err(|_| EvalStatus::RuntimeFatal)?;
    Ok(class_name.trim_start_matches('\\').to_string())
}

/// Applies eval alias resolution to a target class name without requiring it to exist.
fn eval_instanceof_resolved_target_name(target: &str, context: &ElephcEvalContext) -> String {
    context
        .resolve_class_name(target)
        .unwrap_or_else(|| target.trim_start_matches('\\').to_string())
}

/// Tests one object cell against a resolved `instanceof` target class/interface name.
fn eval_instanceof_object_result(
    value: RuntimeCellHandle,
    target_class: &str,
    context: &ElephcEvalContext,
    values: &mut impl RuntimeValueOps,
) -> Result<bool, EvalStatus> {
    dynamic_object_is_a(value, target_class, false, context, values)?
        .map_or_else(|| values.object_is_a(value, target_class, false), Ok)
}

/// Evaluates a PHP `match` expression with strict comparison and lazy arm values.
pub(in crate::interpreter) fn eval_match_expr(
    subject: &EvalExpr,
    arms: &[EvalMatchArm],
    default: Option<&EvalExpr>,
    context: &mut ElephcEvalContext,
    scope: &mut ElephcEvalScope,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    let subject = eval_expr(subject, context, scope, values)?;
    for arm in arms {
        for pattern in &arm.patterns {
            let pattern = eval_expr(pattern, context, scope, values)?;
            let matched = values.compare(EvalBinOp::StrictEq, subject, pattern)?;
            if values.truthy(matched)? {
                return eval_expr(&arm.value, context, scope, values);
            }
        }
    }
    default
        .map(|expr| eval_expr(expr, context, scope, values))
        .unwrap_or(Err(EvalStatus::RuntimeFatal))
}

/// Returns cloned positional argument expressions, rejecting named arguments.
pub(in crate::interpreter) fn positional_call_arg_exprs(
    args: &[EvalCallArg],
) -> Result<Vec<EvalExpr>, EvalStatus> {
    if args
        .iter()
        .any(|arg| arg.name().is_some() || arg.is_spread())
    {
        return Err(EvalStatus::RuntimeFatal);
    }
    Ok(args.iter().map(|arg| arg.value().clone()).collect())
}

/// Evaluates a positional-only call argument list in source order.
pub(in crate::interpreter) fn eval_positional_call_arg_values(
    args: &[EvalCallArg],
    context: &mut ElephcEvalContext,
    scope: &mut ElephcEvalScope,
    values: &mut impl RuntimeValueOps,
) -> Result<Vec<RuntimeCellHandle>, EvalStatus> {
    if args
        .iter()
        .any(|arg| arg.name().is_some() || arg.is_spread())
    {
        return Err(EvalStatus::RuntimeFatal);
    }
    let mut evaluated_args = Vec::with_capacity(args.len());
    for arg in args {
        evaluated_args.push(eval_expr(arg.value(), context, scope, values)?);
    }
    Ok(evaluated_args)
}

/// Evaluates method-call arguments, preserving named metadata for eval method binding.
pub(in crate::interpreter) fn eval_method_call_arg_values(
    args: &[EvalCallArg],
    context: &mut ElephcEvalContext,
    scope: &mut ElephcEvalScope,
    values: &mut impl RuntimeValueOps,
) -> Result<Vec<EvaluatedCallArg>, EvalStatus> {
    eval_call_arg_values(args, context, scope, values)
}

/// Evaluates supported function-like calls from a runtime eval fragment.
pub(in crate::interpreter) fn eval_call(
    name: &str,
    args: &[EvalCallArg],
    context: &mut ElephcEvalContext,
    scope: &mut ElephcEvalScope,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    if eval_expr_language_construct_name(name) {
        let args = positional_call_arg_exprs(args)?;
        return eval_positional_expr_call(name, &args, context, scope, values);
    }
    if name == "flock" {
        return eval_builtin_flock(args, context, scope, values);
    }
    if matches!(
        name,
        "array_pop"
            | "array_push"
            | "array_shift"
            | "array_splice"
            | "array_unshift"
            | "arsort"
            | "asort"
            | "krsort"
            | "ksort"
            | "natcasesort"
            | "natsort"
            | "rsort"
            | "shuffle"
            | "sort"
            | "settype"
            | "uasort"
            | "uksort"
            | "usort"
    ) {
        return eval_builtin_array_pop_shift_call(name, args, context, scope, values);
    }
    if eval_php_visible_builtin_exists(name) {
        if eval_call_args_are_plain_positional(args) {
            let args = positional_call_arg_exprs(args)?;
            return eval_positional_expr_call(name, &args, context, scope, values);
        }
        return eval_builtin_call(name, args, context, scope, values);
    }

    if let Some(function) = context.function(name).cloned() {
        return eval_dynamic_function(&function, args, context, scope, values);
    }
    if let Some(function) = context.native_function(name) {
        return eval_native_function(function, args, context, scope, values);
    }
    Err(EvalStatus::UnsupportedConstruct)
}

/// Evaluates an unqualified namespaced function call with PHP's global fallback.
pub(in crate::interpreter) fn eval_namespaced_call(
    name: &str,
    fallback_name: &str,
    args: &[EvalCallArg],
    context: &mut ElephcEvalContext,
    scope: &mut ElephcEvalScope,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    if let Some(function) = context.function(name).cloned() {
        return eval_dynamic_function(&function, args, context, scope, values);
    }
    if let Some(function) = context.native_function(name) {
        return eval_native_function(function, args, context, scope, values);
    }
    eval_call(fallback_name, args, context, scope, values)
}

/// Evaluates a variable or expression callable and dispatches it with source-order arguments.
pub(in crate::interpreter) fn eval_dynamic_call(
    callee: &EvalExpr,
    args: &[EvalCallArg],
    context: &mut ElephcEvalContext,
    scope: &mut ElephcEvalScope,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    let callback = eval_expr(callee, context, scope, values)?;
    let callback = eval_callable(callback, context, values)?;
    let evaluated_args = eval_call_arg_values(args, context, scope, values)?;
    eval_evaluated_callable_with_call_array_args(&callback, evaluated_args, context, values)
}

/// Returns true for language constructs that need unevaluated argument expressions.
pub(in crate::interpreter) fn eval_expr_language_construct_name(name: &str) -> bool {
    matches!(name, "empty" | "eval" | "isset" | "unset")
}

/// Returns true when every source argument is plain positional.
pub(in crate::interpreter) fn eval_call_args_are_plain_positional(args: &[EvalCallArg]) -> bool {
    args.iter()
        .all(|arg| arg.name().is_none() && !arg.is_spread())
}

/// Evaluates builtins and language constructs after positional-only argument validation.
pub(in crate::interpreter) fn eval_positional_expr_call(
    name: &str,
    args: &[EvalExpr],
    context: &mut ElephcEvalContext,
    scope: &mut ElephcEvalScope,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    match name {
        "abs" => eval_builtin_abs(args, context, scope, values),
        "addslashes" | "stripslashes" => eval_builtin_slashes(name, args, context, scope, values),
        "array_combine" => eval_builtin_array_combine(args, context, scope, values),
        "array_chunk" => eval_builtin_array_chunk(args, context, scope, values),
        "array_column" => eval_builtin_array_column(args, context, scope, values),
        "array_fill" => eval_builtin_array_fill(args, context, scope, values),
        "array_fill_keys" => eval_builtin_array_fill_keys(args, context, scope, values),
        "array_filter" => eval_builtin_array_filter(args, context, scope, values),
        "array_flip" => eval_builtin_array_flip(args, context, scope, values),
        "array_map" => eval_builtin_array_map(args, context, scope, values),
        "array_reduce" => eval_builtin_array_reduce(args, context, scope, values),
        "array_walk" => eval_builtin_array_walk(args, context, scope, values),
        "array_keys" | "array_values" => {
            eval_builtin_array_projection(name, args, context, scope, values)
        }
        "array_key_exists" => eval_builtin_array_key_exists(args, context, scope, values),
        "array_diff" | "array_intersect" => {
            eval_builtin_array_value_set(name, args, context, scope, values)
        }
        "array_diff_key" | "array_intersect_key" => {
            eval_builtin_array_key_set(name, args, context, scope, values)
        }
        "array_merge" => eval_builtin_array_merge(args, context, scope, values),
        "array_product" | "array_sum" => {
            eval_builtin_array_aggregate(name, args, context, scope, values)
        }
        "array_pad" => eval_builtin_array_pad(args, context, scope, values),
        "array_rand" => eval_builtin_array_rand(args, context, scope, values),
        "array_reverse" => eval_builtin_array_reverse(args, context, scope, values),
        "array_search" | "in_array" => {
            eval_builtin_array_search(name, args, context, scope, values)
        }
        "array_slice" => eval_builtin_array_slice(args, context, scope, values),
        "array_unique" => eval_builtin_array_unique(args, context, scope, values),
        "acos" | "asin" | "atan" | "cos" | "cosh" | "deg2rad" | "exp" | "log2" | "log10"
        | "rad2deg" | "sin" | "sinh" | "tan" | "tanh" => {
            eval_builtin_float_unary(name, args, context, scope, values)
        }
        "atan2" | "hypot" => eval_builtin_float_pair(name, args, context, scope, values),
        "base64_encode" => eval_builtin_base64_encode(args, context, scope, values),
        "base64_decode" => eval_builtin_base64_decode(args, context, scope, values),
        "basename" => eval_builtin_basename(args, context, scope, values),
        "bin2hex" => eval_builtin_bin2hex(args, context, scope, values),
        "ceil" => eval_builtin_ceil(args, context, scope, values),
        "chdir" | "mkdir" | "rmdir" => {
            eval_builtin_unary_path_bool(name, args, context, scope, values)
        }
        "chmod" => eval_builtin_chmod(args, context, scope, values),
        "chr" => eval_builtin_chr(args, context, scope, values),
        "clamp" => eval_builtin_clamp(args, context, scope, values),
        "clearstatcache" => eval_builtin_clearstatcache(args, context, scope, values),
        "call_user_func" => eval_builtin_call_user_func(args, context, scope, values),
        "call_user_func_array" => eval_builtin_call_user_func_array(args, context, scope, values),
        "class_alias" => eval_builtin_class_alias(args, context, scope, values),
        "class_attribute_args" => {
            eval_builtin_class_attribute_metadata(name, args, context, scope, values)
        }
        "class_attribute_names" | "class_get_attributes" => {
            eval_builtin_class_attribute_metadata(name, args, context, scope, values)
        }
        "class_exists" => eval_builtin_class_exists(args, context, scope, values),
        "class_implements" | "class_parents" | "class_uses" => {
            eval_builtin_class_relation(name, args, context, scope, values)
        }
        "method_exists" | "property_exists" => {
            eval_builtin_member_exists(name, args, context, scope, values)
        }
        "get_class_methods" => eval_builtin_get_class_methods(args, context, scope, values),
        "get_object_vars" => eval_builtin_get_object_vars(args, context, scope, values),
        "interface_exists" => eval_builtin_interface_exists(args, context, scope, values),
        "trait_exists" | "enum_exists" => {
            eval_builtin_class_like_exists(name, args, context, scope, values)
        }
        "is_a" | "is_subclass_of" => eval_builtin_is_a_relation(name, args, context, scope, values),
        "closedir" | "readdir" | "rewinddir" => {
            eval_builtin_unary_directory(name, args, context, scope, values)
        }
        "chop" => eval_builtin_trim_like(name, args, context, scope, values),
        "boolval" | "floatval" | "intval" | "strval" => {
            eval_builtin_cast(name, args, context, scope, values)
        }
        "count" => eval_builtin_count(args, context, scope, values),
        "copy" | "link" | "rename" | "symlink" => {
            eval_builtin_binary_path_bool(name, args, context, scope, values)
        }
        "crc32" => eval_builtin_crc32(args, context, scope, values),
        "ctype_alnum" | "ctype_alpha" | "ctype_digit" | "ctype_space" => {
            eval_builtin_ctype(name, args, context, scope, values)
        }
        "date" => eval_builtin_date(args, context, scope, values),
        "define" => eval_builtin_define(args, context, scope, values),
        "defined" => eval_builtin_defined(args, context, scope, values),
        "dirname" => eval_builtin_dirname(args, context, scope, values),
        "die" | "exit" => eval_builtin_exit(args, context, scope, values),
        "disk_free_space" | "disk_total_space" => {
            eval_builtin_disk_space(name, args, context, scope, values)
        }
        "empty" => eval_builtin_empty(args, context, scope, values),
        "exec" | "shell_exec" | "system" | "passthru" => {
            eval_builtin_process_command(name, args, context, scope, values)
        }
        "eval" => eval_nested_eval(args, context, scope, values),
        "explode" => eval_builtin_explode(args, context, scope, values),
        "fdiv" | "fmod" => eval_builtin_float_binary(name, args, context, scope, values),
        "file" => eval_builtin_file(args, context, scope, values),
        "file_exists" => eval_builtin_file_probe(name, args, context, scope, values),
        "fileatime" | "filectime" | "filegroup" | "fileinode" | "filemtime" | "fileowner"
        | "fileperms" => eval_builtin_file_stat_scalar(name, args, context, scope, values),
        "file_get_contents" => eval_builtin_file_get_contents(args, context, scope, values),
        "file_put_contents" => eval_builtin_file_put_contents(args, context, scope, values),
        "fclose"
        | "fgetc"
        | "fgets"
        | "feof"
        | "fflush"
        | "fpassthru"
        | "fsync"
        | "fdatasync"
        | "ftell"
        | "rewind"
        | "fstat"
        | "stream_get_meta_data" => eval_builtin_unary_stream(name, args, context, scope, values),
        "filesize" => eval_builtin_filesize(args, context, scope, values),
        "filetype" => eval_builtin_filetype(args, context, scope, values),
        "fnmatch" => eval_builtin_fnmatch(args, context, scope, values),
        "fgetcsv" => eval_builtin_fgetcsv(args, context, scope, values),
        "fopen" => eval_builtin_fopen(args, context, scope, values),
        "fputcsv" => eval_builtin_fputcsv(args, context, scope, values),
        "fprintf" => eval_builtin_fprintf(args, context, scope, values),
        "fread" => eval_builtin_fread(args, context, scope, values),
        "fsockopen" | "pfsockopen" => eval_builtin_fsockopen(args, context, scope, values),
        "fscanf" => eval_builtin_fscanf(args, context, scope, values),
        "fseek" => eval_builtin_fseek(args, context, scope, values),
        "ftruncate" => eval_builtin_ftruncate(args, context, scope, values),
        "fwrite" => eval_builtin_fwrite(args, context, scope, values),
        "stat" | "lstat" => eval_builtin_stat_array(name, args, context, scope, values),
        "floor" => eval_builtin_floor(args, context, scope, values),
        "function_exists" | "is_callable" => {
            eval_builtin_function_probe(name, args, context, scope, values)
        }
        "gethostbyaddr" => eval_builtin_gethostbyaddr(args, context, scope, values),
        "gethostbyname" => eval_builtin_gethostbyname(args, context, scope, values),
        "gethostname" => eval_builtin_gethostname(args, values),
        "getprotobyname" => eval_builtin_getprotobyname(args, context, scope, values),
        "getprotobynumber" => eval_builtin_getprotobynumber(args, context, scope, values),
        "getservbyname" => eval_builtin_getservbyname(args, context, scope, values),
        "getservbyport" => eval_builtin_getservbyport(args, context, scope, values),
        "get_class" => eval_builtin_get_class(args, context, scope, values),
        "get_declared_classes" | "get_declared_interfaces" | "get_declared_traits" => {
            eval_builtin_get_declared_symbols(name, args, context, values)
        }
        "get_parent_class" => eval_builtin_get_parent_class(args, context, scope, values),
        "get_resource_id" | "get_resource_type" => {
            eval_builtin_resource_introspection(name, args, context, scope, values)
        }
        "getcwd" => eval_builtin_getcwd(args, values),
        "getenv" => eval_builtin_getenv(args, context, scope, values),
        "gettype" => eval_builtin_gettype(args, context, scope, values),
        "glob" => eval_builtin_glob(args, context, scope, values),
        "grapheme_strrev" => eval_builtin_grapheme_strrev(args, context, scope, values),
        "gzcompress" | "gzdeflate" | "gzinflate" | "gzuncompress" => {
            eval_builtin_gzip(name, args, context, scope, values)
        }
        "hash" | "hash_file" | "hash_hmac" | "md5" | "sha1" => {
            eval_builtin_hash_one_shot(name, args, context, scope, values)
        }
        "chown" | "chgrp" | "lchown" | "lchgrp" => {
            eval_builtin_chown_like(name, args, context, scope, values)
        }
        "hash_algos" => eval_builtin_hash_algos(args, values),
        "hash_copy" => eval_builtin_hash_copy(args, context, scope, values),
        "hash_equals" => eval_builtin_hash_equals(args, context, scope, values),
        "hash_final" => eval_builtin_hash_final(args, context, scope, values),
        "hash_init" => eval_builtin_hash_init(args, context, scope, values),
        "hash_update" => eval_builtin_hash_update(args, context, scope, values),
        "hex2bin" => eval_builtin_hex2bin(args, context, scope, values),
        "html_entity_decode" | "htmlentities" | "htmlspecialchars" => {
            eval_builtin_html_entity(name, args, context, scope, values)
        }
        "implode" => eval_builtin_implode(args, context, scope, values),
        "inet_ntop" => eval_builtin_inet_ntop(args, context, scope, values),
        "inet_pton" => eval_builtin_inet_pton(args, context, scope, values),
        "intdiv" => eval_builtin_intdiv(args, context, scope, values),
        "iterator_apply" => eval_builtin_iterator_apply(args, context, scope, values),
        "iterator_count" => eval_builtin_iterator_count(args, context, scope, values),
        "iterator_to_array" => eval_builtin_iterator_to_array(args, context, scope, values),
        "is_dir" | "is_executable" | "is_file" | "is_link" | "is_readable" | "is_writable"
        | "is_writeable" => eval_builtin_file_probe(name, args, context, scope, values),
        "is_array" | "is_bool" | "is_double" | "is_finite" | "is_float" | "is_infinite"
        | "is_int" | "is_integer" | "is_iterable" | "is_long" | "is_nan" | "is_null"
        | "is_numeric" | "is_object" | "is_real" | "is_resource" | "is_string" => {
            eval_builtin_type_predicate(name, args, context, scope, values)
        }
        "ip2long" => eval_builtin_ip2long(args, context, scope, values),
        "json_decode" => eval_builtin_json_decode(args, context, scope, values),
        "json_encode" => eval_builtin_json_encode(args, context, scope, values),
        "json_last_error" => eval_builtin_json_last_error(args, context, values),
        "json_last_error_msg" => eval_builtin_json_last_error_msg(args, context, values),
        "json_validate" => eval_builtin_json_validate(args, context, scope, values),
        "linkinfo" => eval_builtin_linkinfo(args, context, scope, values),
        "ltrim" | "rtrim" => eval_builtin_trim_like(name, args, context, scope, values),
        "log" => eval_builtin_log(args, context, scope, values),
        "max" | "min" => eval_builtin_min_max(name, args, context, scope, values),
        "microtime" => eval_builtin_microtime(args, context, scope, values),
        "mktime" => eval_builtin_mktime(args, context, scope, values),
        "nl2br" => eval_builtin_nl2br(args, context, scope, values),
        "number_format" => eval_builtin_number_format(args, context, scope, values),
        "ord" => eval_builtin_ord(args, context, scope, values),
        "opendir" => eval_builtin_opendir(args, context, scope, values),
        "pathinfo" => eval_builtin_pathinfo(args, context, scope, values),
        "pi" => eval_builtin_pi(args, values),
        "php_uname" => eval_builtin_php_uname(args, context, scope, values),
        "phpversion" => eval_builtin_phpversion(args, values),
        "pclose" => eval_builtin_pclose(args, context, scope, values),
        "popen" => eval_builtin_popen(args, context, scope, values),
        "pow" => eval_builtin_pow(args, context, scope, values),
        "preg_match" => eval_builtin_preg_match(args, context, scope, values),
        "preg_match_all" => eval_builtin_preg_match_all(args, context, scope, values),
        "preg_replace" => eval_builtin_preg_replace(args, context, scope, values),
        "preg_replace_callback" => eval_builtin_preg_replace_callback(args, context, scope, values),
        "preg_split" => eval_builtin_preg_split(args, context, scope, values),
        "print_r" => eval_builtin_print_r(args, context, scope, values),
        "putenv" => eval_builtin_putenv(args, context, scope, values),
        "rand" | "mt_rand" => eval_builtin_rand(args, context, scope, values),
        "random_int" => eval_builtin_random_int(args, context, scope, values),
        "range" => eval_builtin_range(args, context, scope, values),
        "rawurldecode" | "urldecode" => eval_builtin_url_decode(name, args, context, scope, values),
        "rawurlencode" | "urlencode" => eval_builtin_url_encode(name, args, context, scope, values),
        "readfile" => eval_builtin_readfile(args, context, scope, values),
        "readline" => eval_builtin_readline(args, context, scope, values),
        "readlink" => eval_builtin_readlink(args, context, scope, values),
        "realpath" => eval_builtin_realpath(args, context, scope, values),
        "realpath_cache_get" => eval_builtin_realpath_cache_get(args, values),
        "realpath_cache_size" => eval_builtin_realpath_cache_size(args, values),
        "round" => eval_builtin_round(args, context, scope, values),
        "scandir" => eval_builtin_scandir(args, context, scope, values),
        "isset" => eval_builtin_isset(args, context, scope, values),
        "sleep" => eval_builtin_sleep(args, context, scope, values),
        "sqrt" => eval_builtin_sqrt(args, context, scope, values),
        "spl_autoload_register" | "spl_autoload_unregister" => {
            eval_builtin_spl_autoload_bool(name, args, context, scope, values)
        }
        "spl_autoload" | "spl_autoload_call" => {
            eval_builtin_spl_autoload_void(name, args, context, scope, values)
        }
        "spl_autoload_functions" => {
            eval_builtin_spl_autoload_functions(args, context, scope, values)
        }
        "spl_autoload_extensions" => {
            eval_builtin_spl_autoload_extensions(args, context, scope, values)
        }
        "spl_classes" => eval_builtin_spl_classes(args, values),
        "spl_object_id" | "spl_object_hash" => {
            eval_builtin_spl_object_identity(name, args, context, scope, values)
        }
        "sscanf" => eval_builtin_sscanf(args, context, scope, values),
        "sprintf" | "printf" => eval_builtin_sprintf_like(name, args, context, scope, values),
        "sys_get_temp_dir" => eval_builtin_sys_get_temp_dir(args, values),
        "tempnam" => eval_builtin_tempnam(args, context, scope, values),
        "time" => eval_builtin_time(args, values),
        "touch" => eval_builtin_touch(args, context, scope, values),
        "tmpfile" => eval_builtin_tmpfile(args, context, values),
        "stream_is_local" | "stream_supports_lock" => {
            eval_builtin_stream_bool_predicate(name, args, context, scope, values)
        }
        "stream_get_filters" | "stream_get_transports" | "stream_get_wrappers" => {
            eval_builtin_stream_introspection(name, args, values)
        }
        "stream_resolve_include_path" => {
            eval_builtin_stream_resolve_include_path(args, context, scope, values)
        }
        "stream_copy_to_stream" => eval_builtin_stream_copy_to_stream(args, context, scope, values),
        "stream_context_create" => eval_builtin_stream_context_create(args, context, scope, values),
        "stream_context_get_default" => {
            eval_builtin_stream_context_get_default(args, context, scope, values)
        }
        "stream_context_get_options" => {
            eval_builtin_stream_context_get_options(args, context, scope, values)
        }
        "stream_context_get_params" => {
            eval_builtin_stream_context_get_params(args, context, scope, values)
        }
        "stream_context_set_default" => {
            eval_builtin_stream_context_set_default(args, context, scope, values)
        }
        "stream_context_set_option" => {
            eval_builtin_stream_context_set_option(args, context, scope, values)
        }
        "stream_context_set_params" => {
            eval_builtin_stream_context_set_params(args, context, scope, values)
        }
        "stream_wrapper_register" | "stream_wrapper_unregister" | "stream_wrapper_restore" => {
            eval_builtin_stream_wrapper_registry(name, args, context, scope, values)
        }
        "stream_filter_register" => {
            eval_builtin_stream_filter_register(args, context, scope, values)
        }
        "stream_filter_append" | "stream_filter_prepend" => {
            eval_builtin_stream_filter_attach(name, args, context, scope, values)
        }
        "stream_filter_remove" => eval_builtin_stream_filter_remove(args, context, scope, values),
        "stream_bucket_new" => eval_builtin_stream_bucket_new(args, context, scope, values),
        "stream_bucket_make_writeable" => {
            eval_builtin_stream_bucket_make_writeable(args, context, scope, values)
        }
        "stream_bucket_append" | "stream_bucket_prepend" => {
            eval_builtin_stream_bucket_push(name, args, context, scope, values)
        }
        "stream_select" => eval_builtin_stream_select(args, context, scope, values),
        "stream_socket_server" => eval_builtin_stream_socket_server(args, context, scope, values),
        "stream_socket_client" => eval_builtin_stream_socket_client(args, context, scope, values),
        "stream_socket_accept" => eval_builtin_stream_socket_accept(args, context, scope, values),
        "stream_socket_get_name" => {
            eval_builtin_stream_socket_get_name(args, context, scope, values)
        }
        "stream_socket_shutdown" => {
            eval_builtin_stream_socket_shutdown(args, context, scope, values)
        }
        "stream_socket_enable_crypto" => {
            eval_builtin_stream_socket_enable_crypto(args, context, scope, values)
        }
        "stream_socket_sendto" => eval_builtin_stream_socket_sendto(args, context, scope, values),
        "stream_socket_recvfrom" => {
            eval_builtin_stream_socket_recvfrom(args, context, scope, values)
        }
        "stream_socket_pair" => eval_builtin_stream_socket_pair(args, context, scope, values),
        "stream_get_contents" => eval_builtin_stream_get_contents(args, context, scope, values),
        "stream_get_line" => eval_builtin_stream_get_line(args, context, scope, values),
        "stream_isatty" => eval_builtin_stream_isatty(args, context, scope, values),
        "stream_set_blocking" => eval_builtin_stream_set_blocking(args, context, scope, values),
        "stream_set_chunk_size" | "stream_set_read_buffer" | "stream_set_write_buffer" => {
            eval_builtin_stream_set_buffer_like(name, args, context, scope, values)
        }
        "stream_set_timeout" => eval_builtin_stream_set_timeout(args, context, scope, values),
        "strtotime" => eval_builtin_strtotime(args, context, scope, values),
        "unlink" => eval_builtin_unlink(args, context, scope, values),
        "strrev" => eval_builtin_strrev(args, context, scope, values),
        "str_repeat" => eval_builtin_str_repeat(args, context, scope, values),
        "str_replace" | "str_ireplace" => {
            eval_builtin_str_replace(name, args, context, scope, values)
        }
        "str_pad" => eval_builtin_str_pad(args, context, scope, values),
        "str_split" => eval_builtin_str_split(args, context, scope, values),
        "strstr" => eval_builtin_strstr(args, context, scope, values),
        "substr" => eval_builtin_substr(args, context, scope, values),
        "substr_replace" => eval_builtin_substr_replace(args, context, scope, values),
        "str_contains" | "str_starts_with" | "str_ends_with" => {
            eval_builtin_string_search(name, args, context, scope, values)
        }
        "strcmp" | "strcasecmp" => eval_builtin_string_compare(name, args, context, scope, values),
        "strlen" => eval_builtin_strlen(args, context, scope, values),
        "strpos" | "strrpos" => eval_builtin_string_position(name, args, context, scope, values),
        "lcfirst" | "strtolower" | "strtoupper" | "ucfirst" => {
            eval_builtin_string_case(name, args, context, scope, values)
        }
        "long2ip" => eval_builtin_long2ip(args, context, scope, values),
        "trim" => eval_builtin_trim_like(name, args, context, scope, values),
        "ucwords" => eval_builtin_ucwords(args, context, scope, values),
        "unset" => eval_builtin_unset(args, context, scope, values),
        "umask" => eval_builtin_umask(args, context, scope, values),
        "usleep" => eval_builtin_usleep(args, context, scope, values),
        "var_dump" => eval_builtin_var_dump(args, context, scope, values),
        "vfprintf" => eval_builtin_vfprintf(args, context, scope, values),
        "vsprintf" | "vprintf" => eval_builtin_vsprintf_like(name, args, context, scope, values),
        "wordwrap" => eval_builtin_wordwrap(args, context, scope, values),
        _ => Err(EvalStatus::UnsupportedConstruct),
    }
}
