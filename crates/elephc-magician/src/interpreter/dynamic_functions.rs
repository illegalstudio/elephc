//! Purpose:
//! Evaluates user-declared and native dynamic functions, including named/spread argument binding.
//!
//! Called from:
//! - `crate::interpreter::eval_call()` and dynamic callable dispatch helpers.
//!
//! Key details:
//! - PHP source evaluation order is preserved before argument binding.
//! - Static locals are persisted through `ElephcEvalContext` after function execution.

use super::*;

/// Evaluates an eval-declared user function with PHP-style argument binding.
pub(in crate::interpreter) fn eval_dynamic_function(
    function: &EvalFunction,
    args: &[EvalCallArg],
    context: &mut ElephcEvalContext,
    caller_scope: &mut ElephcEvalScope,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    let evaluated_args = eval_call_arg_values(args, context, caller_scope, values)?;
    eval_dynamic_function_with_evaluated_args(function, evaluated_args, context, values)
}

/// Evaluates and binds function-like arguments to parameter order.
pub(in crate::interpreter) fn eval_function_call_args(
    params: &[String],
    args: &[EvalCallArg],
    context: &mut ElephcEvalContext,
    caller_scope: &mut ElephcEvalScope,
    values: &mut impl RuntimeValueOps,
) -> Result<Vec<RuntimeCellHandle>, EvalStatus> {
    let evaluated_args = eval_call_arg_values(args, context, caller_scope, values)?;
    bind_evaluated_function_args(params, evaluated_args)
}

/// Evaluates source-order call arguments while preserving named-argument metadata.
pub(in crate::interpreter) fn eval_call_arg_values(
    args: &[EvalCallArg],
    context: &mut ElephcEvalContext,
    caller_scope: &mut ElephcEvalScope,
    values: &mut impl RuntimeValueOps,
) -> Result<Vec<EvaluatedCallArg>, EvalStatus> {
    let mut evaluated_args = Vec::with_capacity(args.len());
    let mut saw_named = false;

    for arg in args {
        if arg.is_spread() {
            if saw_named {
                return Err(EvalStatus::RuntimeFatal);
            }
            let spread = eval_expr(arg.value(), context, caller_scope, values)?;
            if !values.is_array_like(spread)? {
                return Err(EvalStatus::RuntimeFatal);
            }
            append_unpacked_call_arg_values(spread, &mut evaluated_args, &mut saw_named, values)?;
            continue;
        }

        if let Some(name) = arg.name() {
            saw_named = true;
            let (value, ref_target) =
                eval_call_arg_value(arg.value(), context, caller_scope, values)?;
            evaluated_args.push(EvaluatedCallArg {
                name: Some(name.to_string()),
                value,
                ref_target,
            });
            continue;
        }

        if saw_named {
            return Err(EvalStatus::RuntimeFatal);
        }
        let (value, ref_target) = eval_call_arg_value(arg.value(), context, caller_scope, values)?;
        evaluated_args.push(EvaluatedCallArg {
            name: None,
            value,
            ref_target,
        });
    }

    Ok(evaluated_args)
}

/// Evaluates one call arg and captures caller-side storage for by-reference parameters.
fn eval_call_arg_value(
    expr: &EvalExpr,
    context: &mut ElephcEvalContext,
    caller_scope: &mut ElephcEvalScope,
    values: &mut impl RuntimeValueOps,
) -> Result<(RuntimeCellHandle, Option<EvalReferenceTarget>), EvalStatus> {
    match expr {
        EvalExpr::LoadVar(name) => {
            let value = visible_scope_cell(context, caller_scope, name)
                .map_or_else(|| values.null(), Ok)?;
            Ok((
                value,
                Some(EvalReferenceTarget::Variable {
                    scope: caller_scope as *mut ElephcEvalScope,
                    name: name.clone(),
                }),
            ))
        }
        EvalExpr::ArrayGet { array, index } => {
            let EvalExpr::LoadVar(array_name) = array.as_ref() else {
                return eval_expr(expr, context, caller_scope, values).map(|value| (value, None));
            };
            let array = visible_scope_cell(context, caller_scope, array_name)
                .map_or_else(|| values.null(), Ok)?;
            let index = eval_expr(index, context, caller_scope, values)?;
            let value = eval_array_get_result(array, index, context, values)?;
            if values.type_tag(array)? == EVAL_TAG_OBJECT {
                return Ok((value, None));
            }
            Ok((
                value,
                Some(EvalReferenceTarget::ArrayElement {
                    scope: caller_scope as *mut ElephcEvalScope,
                    array_name: array_name.clone(),
                    index,
                }),
            ))
        }
        EvalExpr::PropertyGet { object, property } => {
            let access_scope = context.execution_scope();
            let object = eval_expr(object, context, caller_scope, values)?;
            let value = eval_property_get_result(object, property, context, values)?;
            validate_property_ref_target(object, property, context, values)?;
            Ok((
                value,
                Some(EvalReferenceTarget::ObjectProperty {
                    object,
                    property: property.clone(),
                    access_scope,
                }),
            ))
        }
        _ => eval_expr(expr, context, caller_scope, values).map(|value| (value, None)),
    }
}

/// Converts a `call_user_func_array` argument array into ordered call arguments.
pub(in crate::interpreter) fn eval_array_call_arg_values(
    arg_array: RuntimeCellHandle,
    values: &mut impl RuntimeValueOps,
) -> Result<Vec<EvaluatedCallArg>, EvalStatus> {
    let len = values.array_len(arg_array)?;
    let mut evaluated_args = Vec::with_capacity(len);
    let mut saw_named = false;
    append_unpacked_call_arg_values(arg_array, &mut evaluated_args, &mut saw_named, values)?;
    Ok(evaluated_args)
}

/// Appends one unpacked array's values using PHP named-argument key semantics.
pub(in crate::interpreter) fn append_unpacked_call_arg_values(
    array: RuntimeCellHandle,
    evaluated_args: &mut Vec<EvaluatedCallArg>,
    saw_named: &mut bool,
    values: &mut impl RuntimeValueOps,
) -> Result<(), EvalStatus> {
    let len = values.array_len(array)?;
    for position in 0..len {
        let key = values.array_iter_key(array, position)?;
        let value = values.array_get(array, key)?;
        match values.type_tag(key)? {
            EVAL_TAG_INT => {
                if *saw_named {
                    return Err(EvalStatus::RuntimeFatal);
                }
                evaluated_args.push(EvaluatedCallArg {
                    name: None,
                    value,
                    ref_target: None,
                });
            }
            EVAL_TAG_STRING => {
                *saw_named = true;
                let name = values.string_bytes(key)?;
                let name = String::from_utf8(name).map_err(|_| EvalStatus::RuntimeFatal)?;
                evaluated_args.push(EvaluatedCallArg {
                    name: Some(name),
                    value,
                    ref_target: None,
                });
            }
            _ => return Err(EvalStatus::RuntimeFatal),
        }
    }
    Ok(())
}

/// Binds evaluated positional and named values to declared parameter order.
pub(in crate::interpreter) fn bind_evaluated_function_args(
    params: &[String],
    evaluated_args: Vec<EvaluatedCallArg>,
) -> Result<Vec<RuntimeCellHandle>, EvalStatus> {
    let mut bound_args = vec![None; params.len()];
    let mut next_positional = 0;

    for arg in evaluated_args {
        if let Some(name) = arg.name {
            bind_dynamic_named_arg(params, &mut bound_args, &name, arg.value)?;
        } else {
            bind_dynamic_positional_arg(&mut bound_args, &mut next_positional, arg.value)?;
        }
    }

    bound_args
        .into_iter()
        .collect::<Option<Vec<_>>>()
        .ok_or(EvalStatus::RuntimeFatal)
}

/// Binds evaluated method arguments and fills omitted parameters from defaults.
pub(in crate::interpreter) fn bind_evaluated_method_args(
    params: &[String],
    parameter_types: &[Option<EvalParameterType>],
    parameter_defaults: &[Option<EvalExpr>],
    parameter_is_by_ref: &[bool],
    parameter_is_variadic: &[bool],
    evaluated_args: Vec<EvaluatedCallArg>,
    context: &mut ElephcEvalContext,
    values: &mut impl RuntimeValueOps,
) -> Result<Vec<BoundMethodArg>, EvalStatus> {
    let mut bound_args = vec![None; params.len()];
    let variadic_index = parameter_is_variadic
        .iter()
        .position(|is_variadic| *is_variadic);
    let required_count =
        method_required_param_count(params.len(), parameter_defaults, parameter_is_variadic);
    let mut next_positional = 0;
    let mut next_variadic_index = 0_i64;
    let mut variadic_named_args = std::collections::HashSet::new();

    if let Some(index) = variadic_index {
        let array = if evaluated_args_contain_named_variadic_values(
            params,
            variadic_index,
            &evaluated_args,
        ) {
            values.assoc_new(evaluated_args.len())?
        } else {
            values.array_new(evaluated_args.len())?
        };
        bound_args[index] = Some(BoundMethodArg {
            value: array,
            ref_target: None,
            variadic_ref_targets: Vec::new(),
        });
    }

    for arg in evaluated_args {
        if let Some(name) = arg.name {
            bind_dynamic_named_method_arg(
                params,
                parameter_types,
                parameter_is_by_ref,
                variadic_index,
                &mut bound_args,
                &name,
                arg.value,
                arg.ref_target,
                &mut variadic_named_args,
                context,
                values,
            )?;
        } else {
            bind_dynamic_positional_method_arg(
                &mut bound_args,
                parameter_types,
                parameter_is_by_ref,
                variadic_index,
                &mut next_positional,
                &mut next_variadic_index,
                arg.value,
                arg.ref_target,
                context,
                values,
            )?;
        }
    }

    for (position, value) in bound_args.iter_mut().enumerate() {
        if Some(position) == variadic_index {
            continue;
        }
        if value.is_none() {
            if position < required_count {
                return Err(EvalStatus::RuntimeFatal);
            }
            let Some(Some(default)) = parameter_defaults.get(position) else {
                return Err(EvalStatus::RuntimeFatal);
            };
            *value = Some(BoundMethodArg {
                value: eval_method_parameter_default(default, context, values)?,
                ref_target: None,
                variadic_ref_targets: Vec::new(),
            });
        }
        if let Some(param_type) = parameter_types.get(position).and_then(Option::as_ref) {
            let bound = value.as_mut().ok_or(EvalStatus::RuntimeFatal)?;
            bound.value = eval_method_parameter_value(param_type, bound.value, context, values)?;
        }
    }

    bound_args
        .into_iter()
        .collect::<Option<Vec<_>>>()
        .ok_or(EvalStatus::RuntimeFatal)
}

/// Returns the minimum argument count for a PHP method signature.
fn method_required_param_count(
    param_count: usize,
    defaults: &[Option<EvalExpr>],
    variadics: &[bool],
) -> usize {
    let fixed_count = variadics
        .iter()
        .position(|is_variadic| *is_variadic)
        .unwrap_or(param_count);
    (0..fixed_count)
        .rfind(|position| !defaults.get(*position).is_some_and(Option::is_some))
        .map_or(0, |position| position + 1)
}

/// Returns true when evaluated args contain named values captured by a variadic parameter.
fn evaluated_args_contain_named_variadic_values(
    params: &[String],
    variadic_index: Option<usize>,
    evaluated_args: &[EvaluatedCallArg],
) -> bool {
    let Some(variadic_index) = variadic_index else {
        return false;
    };
    evaluated_args.iter().any(|arg| {
        arg.name.as_ref().is_some_and(|name| {
            regular_method_param_index(params, Some(variadic_index), name).is_none()
        })
    })
}

/// Binds one positional method argument to a fixed parameter or variadic array.
fn bind_dynamic_positional_method_arg(
    bound_args: &mut [Option<BoundMethodArg>],
    parameter_types: &[Option<EvalParameterType>],
    parameter_is_by_ref: &[bool],
    variadic_index: Option<usize>,
    next_positional: &mut usize,
    next_variadic_index: &mut i64,
    value: RuntimeCellHandle,
    ref_target: Option<EvalReferenceTarget>,
    context: &mut ElephcEvalContext,
    values: &mut impl RuntimeValueOps,
) -> Result<(), EvalStatus> {
    if variadic_index.is_some_and(|index| *next_positional >= index) {
        let key = values.int(*next_variadic_index)?;
        *next_variadic_index = next_variadic_index
            .checked_add(1)
            .ok_or(EvalStatus::RuntimeFatal)?;
        let value = eval_variadic_method_parameter_value(
            parameter_types,
            variadic_index,
            value,
            context,
            values,
        )?;
        let ref_target =
            method_parameter_ref_target(parameter_is_by_ref, variadic_index, ref_target)?;
        return bind_dynamic_variadic_arg(
            bound_args,
            variadic_index,
            key,
            value,
            ref_target,
            values,
        );
    }
    let param_index = *next_positional;
    if param_index >= bound_args.len() || bound_args[param_index].is_some() {
        return Err(EvalStatus::RuntimeFatal);
    }
    let ref_target =
        method_parameter_ref_target(parameter_is_by_ref, Some(param_index), ref_target)?;
    bound_args[param_index] = Some(BoundMethodArg {
        value,
        ref_target,
        variadic_ref_targets: Vec::new(),
    });
    *next_positional += 1;
    Ok(())
}

/// Binds one named method argument to a fixed parameter or variadic array.
fn bind_dynamic_named_method_arg(
    params: &[String],
    parameter_types: &[Option<EvalParameterType>],
    parameter_is_by_ref: &[bool],
    variadic_index: Option<usize>,
    bound_args: &mut [Option<BoundMethodArg>],
    name: &str,
    value: RuntimeCellHandle,
    ref_target: Option<EvalReferenceTarget>,
    variadic_named_args: &mut std::collections::HashSet<String>,
    context: &mut ElephcEvalContext,
    values: &mut impl RuntimeValueOps,
) -> Result<(), EvalStatus> {
    if let Some(param_index) = regular_method_param_index(params, variadic_index, name) {
        if bound_args[param_index].is_some() {
            return Err(EvalStatus::RuntimeFatal);
        }
        let ref_target =
            method_parameter_ref_target(parameter_is_by_ref, Some(param_index), ref_target)?;
        bound_args[param_index] = Some(BoundMethodArg {
            value,
            ref_target,
            variadic_ref_targets: Vec::new(),
        });
        return Ok(());
    }
    if variadic_index.is_none() || !variadic_named_args.insert(name.to_string()) {
        return Err(EvalStatus::RuntimeFatal);
    }
    let key = values.string(name)?;
    let value = eval_variadic_method_parameter_value(
        parameter_types,
        variadic_index,
        value,
        context,
        values,
    )?;
    let ref_target = method_parameter_ref_target(parameter_is_by_ref, variadic_index, ref_target)?;
    bind_dynamic_variadic_arg(bound_args, variadic_index, key, value, ref_target, values)
}

/// Returns the caller writeback target required by a by-reference method parameter.
fn method_parameter_ref_target(
    parameter_is_by_ref: &[bool],
    param_index: Option<usize>,
    ref_target: Option<EvalReferenceTarget>,
) -> Result<Option<EvalReferenceTarget>, EvalStatus> {
    let Some(param_index) = param_index else {
        return Ok(None);
    };
    if !parameter_is_by_ref
        .get(param_index)
        .copied()
        .unwrap_or(false)
    {
        return Ok(None);
    }
    ref_target.map(Some).ok_or(EvalStatus::RuntimeFatal)
}

/// Applies a variadic parameter type to one captured argument value.
fn eval_variadic_method_parameter_value(
    parameter_types: &[Option<EvalParameterType>],
    variadic_index: Option<usize>,
    value: RuntimeCellHandle,
    context: &mut ElephcEvalContext,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    let Some(param_type) =
        variadic_index.and_then(|index| parameter_types.get(index).and_then(Option::as_ref))
    else {
        return Ok(value);
    };
    eval_method_parameter_value(param_type, value, context, values)
}

/// Returns the matching non-variadic parameter index for one PHP named argument.
fn regular_method_param_index(
    params: &[String],
    variadic_index: Option<usize>,
    name: &str,
) -> Option<usize> {
    params
        .iter()
        .enumerate()
        .position(|(index, param)| Some(index) != variadic_index && param == name)
}

/// Appends one value into the method variadic array.
fn bind_dynamic_variadic_arg(
    bound_args: &mut [Option<BoundMethodArg>],
    variadic_index: Option<usize>,
    key: RuntimeCellHandle,
    value: RuntimeCellHandle,
    ref_target: Option<EvalReferenceTarget>,
    values: &mut impl RuntimeValueOps,
) -> Result<(), EvalStatus> {
    let index = variadic_index.ok_or(EvalStatus::RuntimeFatal)?;
    let bound = bound_args[index].as_mut().ok_or(EvalStatus::RuntimeFatal)?;
    bound.value = values.array_set(bound.value, key, value)?;
    if let Some(ref_target) = ref_target {
        bound.variadic_ref_targets.push((key, ref_target));
    }
    Ok(())
}

/// Applies one eval method parameter type to a bound runtime value.
pub(in crate::interpreter) fn eval_method_parameter_value(
    param_type: &EvalParameterType,
    value: RuntimeCellHandle,
    context: &mut ElephcEvalContext,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    if eval_method_parameter_type_accepts_exact(param_type, value, context, values)? {
        return Ok(value);
    }
    if param_type.is_intersection() {
        return Err(EvalStatus::RuntimeFatal);
    }
    for variant in param_type.variants() {
        if let Some(coerced) =
            eval_method_parameter_scalar_coercion(variant, value, context, values)?
        {
            return Ok(coerced);
        }
    }
    Err(EvalStatus::RuntimeFatal)
}

/// Returns whether a value satisfies one eval parameter type without scalar coercion.
fn eval_method_parameter_type_accepts_exact(
    param_type: &EvalParameterType,
    value: RuntimeCellHandle,
    context: &ElephcEvalContext,
    values: &mut impl RuntimeValueOps,
) -> Result<bool, EvalStatus> {
    let tag = values.type_tag(value)?;
    if tag == EVAL_TAG_NULL && param_type.allows_null() {
        return Ok(true);
    }
    if param_type.is_intersection() {
        for variant in param_type.variants() {
            if !eval_method_parameter_variant_accepts_exact(variant, value, tag, context, values)? {
                return Ok(false);
            }
        }
        return Ok(true);
    }
    for variant in param_type.variants() {
        if eval_method_parameter_variant_accepts_exact(variant, value, tag, context, values)? {
            return Ok(true);
        }
    }
    Ok(false)
}

/// Returns whether a value exactly satisfies one non-null eval parameter type atom.
fn eval_method_parameter_variant_accepts_exact(
    variant: &EvalParameterTypeVariant,
    value: RuntimeCellHandle,
    tag: u64,
    context: &ElephcEvalContext,
    values: &mut impl RuntimeValueOps,
) -> Result<bool, EvalStatus> {
    match variant {
        EvalParameterTypeVariant::Array => Ok(matches!(tag, EVAL_TAG_ARRAY | EVAL_TAG_ASSOC)),
        EvalParameterTypeVariant::Bool => Ok(tag == EVAL_TAG_BOOL),
        EvalParameterTypeVariant::Callable => Ok(matches!(
            tag,
            EVAL_TAG_STRING | EVAL_TAG_ARRAY | EVAL_TAG_ASSOC | EVAL_TAG_OBJECT
        )),
        EvalParameterTypeVariant::Class(class_name) => {
            eval_method_parameter_class_accepts(value, tag, class_name, context, values)
        }
        EvalParameterTypeVariant::Float => Ok(tag == EVAL_TAG_FLOAT),
        EvalParameterTypeVariant::Int => Ok(tag == EVAL_TAG_INT),
        EvalParameterTypeVariant::Iterable => {
            if matches!(tag, EVAL_TAG_ARRAY | EVAL_TAG_ASSOC) {
                return Ok(true);
            }
            if eval_method_parameter_class_accepts(value, tag, "Traversable", context, values)? {
                return Ok(true);
            }
            eval_method_parameter_class_accepts(value, tag, "Iterator", context, values)
        }
        EvalParameterTypeVariant::Mixed => Ok(true),
        EvalParameterTypeVariant::Never | EvalParameterTypeVariant::Void => Ok(false),
        EvalParameterTypeVariant::Object => Ok(tag == EVAL_TAG_OBJECT),
        EvalParameterTypeVariant::String => Ok(tag == EVAL_TAG_STRING),
    }
}

/// Returns whether an object value satisfies one class/interface parameter target.
fn eval_method_parameter_class_accepts(
    value: RuntimeCellHandle,
    tag: u64,
    class_name: &str,
    context: &ElephcEvalContext,
    values: &mut impl RuntimeValueOps,
) -> Result<bool, EvalStatus> {
    if tag != EVAL_TAG_OBJECT {
        return Ok(false);
    }
    let target = eval_method_parameter_runtime_class_name(class_name, context)?;
    let identity = values.object_identity(value)?;
    if let Some(class) = context.dynamic_object_class(identity) {
        return Ok(context.class_is_a(class.name(), &target, false));
    }
    values.object_is_a(value, &target, false)
}

/// Resolves late-bound class keywords inside eval method parameter type checks.
fn eval_method_parameter_runtime_class_name(
    class_name: &str,
    context: &ElephcEvalContext,
) -> Result<String, EvalStatus> {
    match class_name.to_ascii_lowercase().as_str() {
        "self" | "static" => context
            .current_class_scope()
            .map(str::to_string)
            .ok_or(EvalStatus::RuntimeFatal),
        "parent" => {
            let current = context
                .current_class_scope()
                .ok_or(EvalStatus::RuntimeFatal)?;
            context
                .class(current)
                .and_then(EvalClass::parent)
                .map(str::to_string)
                .ok_or(EvalStatus::RuntimeFatal)
        }
        _ => Ok(context
            .resolve_class_name(class_name)
            .unwrap_or_else(|| class_name.trim_start_matches('\\').to_string())),
    }
}

/// Applies PHP weak-mode scalar coercion for supported scalar parameter types.
pub(in crate::interpreter) fn eval_method_parameter_scalar_coercion(
    variant: &EvalParameterTypeVariant,
    value: RuntimeCellHandle,
    context: &mut ElephcEvalContext,
    values: &mut impl RuntimeValueOps,
) -> Result<Option<RuntimeCellHandle>, EvalStatus> {
    let tag = values.type_tag(value)?;
    match variant {
        EvalParameterTypeVariant::Bool if eval_method_scalar_coercible_tag(tag) => {
            values.cast_bool(value).map(Some)
        }
        EvalParameterTypeVariant::Float
            if eval_method_numeric_coercible_value(value, tag, values)? =>
        {
            values.cast_float(value).map(Some)
        }
        EvalParameterTypeVariant::Int
            if eval_method_numeric_coercible_value(value, tag, values)? =>
        {
            values.cast_int(value).map(Some)
        }
        EvalParameterTypeVariant::String if eval_method_scalar_coercible_tag(tag) => {
            values.cast_string(value).map(Some)
        }
        EvalParameterTypeVariant::String if tag == EVAL_TAG_OBJECT => {
            let coerced = eval_dynamic_object_string_context_value(value, context, values)?;
            if values.type_tag(coerced)? == EVAL_TAG_STRING {
                Ok(Some(coerced))
            } else {
                Ok(None)
            }
        }
        _ => Ok(None),
    }
}

/// Converts objects in string contexts through the applicable `__toString()` dispatch path.
pub(in crate::interpreter) fn eval_string_context_value(
    value: RuntimeCellHandle,
    context: &mut ElephcEvalContext,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    if values.type_tag(value)? != EVAL_TAG_OBJECT {
        return Ok(value);
    }
    eval_dynamic_object_string_context_value(value, context, values)
}

/// Invokes `__toString()` for eval-declared objects or throws PHP's missing-hook error.
fn eval_dynamic_object_string_context_value(
    value: RuntimeCellHandle,
    context: &mut ElephcEvalContext,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    let identity = values.object_identity(value)?;
    let Some(class) = context.dynamic_object_class(identity) else {
        return eval_runtime_object_string_context_value(value, context, values);
    };
    let called_class_name = class.name().to_string();
    let Some((declaring_class, method)) = context.class_method(&called_class_name, "__toString")
    else {
        return eval_throw_object_to_string_error(&called_class_name, context, values);
    };
    if method.visibility() != EvalVisibility::Public
        || method.is_static()
        || method.is_abstract()
        || !method.params().is_empty()
    {
        return Err(EvalStatus::RuntimeFatal);
    }
    let result = eval_dynamic_method_with_values(
        &declaring_class,
        &called_class_name,
        &method,
        value,
        Vec::new(),
        context,
        values,
    )?;
    eval_tostring_result_to_string(result, values)
}

/// Invokes the interpreter method dispatcher for AOT/native objects in string contexts.
fn eval_runtime_object_string_context_value(
    value: RuntimeCellHandle,
    context: &mut ElephcEvalContext,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    let identity = values.object_identity(value)?;
    let class_name = runtime_object_class_name(value, values)?;
    if !eval_runtime_object_has_interpreter_tostring(identity, &class_name, context)
        && eval_aot_method_dispatch_metadata_in_hierarchy(
            &class_name,
            "__toString",
            context,
            values,
        )?
        .is_none()
    {
        return eval_throw_object_to_string_error(&class_name, context, values);
    }
    let result = eval_method_call_result_with_evaluated_args(
        value,
        "__toString",
        Vec::new(),
        context,
        values,
    )?;
    eval_tostring_result_to_string(result, values)
}

/// Returns whether eval owns a synthetic `__toString()` implementation for the runtime object.
fn eval_runtime_object_has_interpreter_tostring(
    identity: u64,
    class_name: &str,
    context: &ElephcEvalContext,
) -> bool {
    context.eval_reflection_class_name(identity).is_some()
        || context.eval_reflection_function_name(identity).is_some()
        || context.eval_reflection_method(identity).is_some()
        || context.eval_reflection_property(identity).is_some()
        || context.eval_reflection_class_constant(identity).is_some()
        || eval_runtime_class_name_has_reflection_tostring(class_name)
}

/// Returns whether a synthetic reflection class has `__toString()` handled by eval dispatch.
fn eval_runtime_class_name_has_reflection_tostring(class_name: &str) -> bool {
    let class_name = class_name.trim_start_matches('\\');
    [
        "ReflectionParameter",
        "ReflectionNamedType",
        "ReflectionUnionType",
        "ReflectionIntersectionType",
    ]
    .iter()
    .any(|reflection_class| class_name.eq_ignore_ascii_case(reflection_class))
}

/// Throws PHP's catchable object-to-string conversion error.
fn eval_throw_object_to_string_error<T>(
    class_name: &str,
    context: &mut ElephcEvalContext,
    values: &mut impl RuntimeValueOps,
) -> Result<T, EvalStatus> {
    eval_throw_error(
        &format!(
            "Object of class {} could not be converted to string",
            class_name.trim_start_matches('\\')
        ),
        context,
        values,
    )
}

/// Normalizes one `__toString()` result to a boxed string cell.
fn eval_tostring_result_to_string(
    result: RuntimeCellHandle,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    if values.type_tag(result)? == EVAL_TAG_STRING {
        return Ok(result);
    }
    let coerced = values.cast_string(result)?;
    values.release(result)?;
    Ok(coerced)
}

/// Returns whether a runtime tag can be weakly coerced to string/bool parameters.
fn eval_method_scalar_coercible_tag(tag: u64) -> bool {
    matches!(
        tag,
        EVAL_TAG_INT | EVAL_TAG_FLOAT | EVAL_TAG_STRING | EVAL_TAG_BOOL
    )
}

/// Returns whether a runtime value can be weakly coerced to a numeric parameter.
fn eval_method_numeric_coercible_value(
    value: RuntimeCellHandle,
    tag: u64,
    values: &mut impl RuntimeValueOps,
) -> Result<bool, EvalStatus> {
    match tag {
        EVAL_TAG_INT | EVAL_TAG_FLOAT | EVAL_TAG_BOOL => Ok(true),
        EVAL_TAG_STRING => Ok(eval_is_numeric_string(&values.string_bytes(value)?)),
        _ => Ok(false),
    }
}

/// Materializes a supported eval method parameter default expression.
pub(in crate::interpreter) fn eval_method_parameter_default(
    default: &EvalExpr,
    context: &mut ElephcEvalContext,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    if !eval_method_default_expr_is_supported(default) {
        return Err(EvalStatus::UnsupportedConstruct);
    }
    let mut default_scope = ElephcEvalScope::new();
    eval_expr(default, context, &mut default_scope, values)
}

/// Returns whether an EvalIR expression can be safely evaluated as a method default.
fn eval_method_default_expr_is_supported(expr: &EvalExpr) -> bool {
    match expr {
        EvalExpr::Array(elements) => elements
            .iter()
            .all(eval_method_default_array_element_is_supported),
        EvalExpr::Const(_) | EvalExpr::Magic(_) => true,
        EvalExpr::ConstFetch(_) | EvalExpr::NamespacedConstFetch { .. } => true,
        EvalExpr::ClassConstantFetch { class_name, .. }
        | EvalExpr::ClassNameFetch { class_name } => {
            eval_method_default_class_receiver_is_supported(class_name)
        }
        EvalExpr::NewObject { class_name, args } => {
            eval_method_default_class_receiver_is_supported(class_name)
                && args.iter().all(eval_method_default_call_arg_is_supported)
        }
        EvalExpr::NewAnonymousClass { .. } => false,
        EvalExpr::NullCoalesce { value, default } => {
            eval_method_default_expr_is_supported(value)
                && eval_method_default_expr_is_supported(default)
        }
        EvalExpr::Ternary {
            condition,
            then_branch,
            else_branch,
        } => {
            eval_method_default_expr_is_supported(condition)
                && then_branch
                    .as_deref()
                    .is_none_or(eval_method_default_expr_is_supported)
                && eval_method_default_expr_is_supported(else_branch)
        }
        EvalExpr::Cast { expr, .. } => eval_method_default_expr_is_supported(expr),
        EvalExpr::Unary { expr, .. } => eval_method_default_expr_is_supported(expr),
        EvalExpr::Binary { left, right, .. } => {
            eval_method_default_expr_is_supported(left)
                && eval_method_default_expr_is_supported(right)
        }
        _ => false,
    }
}

/// Returns whether one object-construction argument is safe inside a method default.
fn eval_method_default_call_arg_is_supported(arg: &EvalCallArg) -> bool {
    !arg.is_spread() && eval_method_default_expr_is_supported(arg.value())
}

/// Returns whether one array default element contains only supported constant expressions.
fn eval_method_default_array_element_is_supported(element: &EvalArrayElement) -> bool {
    match element {
        EvalArrayElement::Value(value) => eval_method_default_expr_is_supported(value),
        EvalArrayElement::KeyValue { key, value } => {
            eval_method_default_expr_is_supported(key)
                && eval_method_default_expr_is_supported(value)
        }
    }
}

/// Returns whether a class-like receiver is legal in a compile-time method default.
fn eval_method_default_class_receiver_is_supported(class_name: &str) -> bool {
    !class_name
        .trim_start_matches('\\')
        .eq_ignore_ascii_case("static")
}

/// Binds one positional dynamic-call value to the next declared parameter slot.
pub(in crate::interpreter) fn bind_dynamic_positional_arg(
    bound_args: &mut [Option<RuntimeCellHandle>],
    next_positional: &mut usize,
    value: RuntimeCellHandle,
) -> Result<(), EvalStatus> {
    if *next_positional >= bound_args.len() || bound_args[*next_positional].is_some() {
        return Err(EvalStatus::RuntimeFatal);
    }
    bound_args[*next_positional] = Some(value);
    *next_positional += 1;
    Ok(())
}

/// Binds one named dynamic-call value to the matching declared parameter slot.
pub(in crate::interpreter) fn bind_dynamic_named_arg(
    params: &[String],
    bound_args: &mut [Option<RuntimeCellHandle>],
    name: &str,
    value: RuntimeCellHandle,
) -> Result<(), EvalStatus> {
    let Some(param_index) = params.iter().position(|param| param == name) else {
        return Err(EvalStatus::RuntimeFatal);
    };
    if bound_args[param_index].is_some() {
        return Err(EvalStatus::RuntimeFatal);
    }
    bound_args[param_index] = Some(value);
    Ok(())
}

/// Evaluates an eval-declared function after its positional arguments are prepared.
pub(super) fn eval_dynamic_function_with_values(
    function: &EvalFunction,
    evaluated_args: Vec<RuntimeCellHandle>,
    context: &mut ElephcEvalContext,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    let evaluated_args = evaluated_args
        .into_iter()
        .map(|value| EvaluatedCallArg {
            name: None,
            value,
            ref_target: None,
        })
        .collect();
    eval_dynamic_function_with_evaluated_args(function, evaluated_args, context, values)
}

/// Evaluates an eval-declared function after call arguments preserve names and ref targets.
pub(super) fn eval_dynamic_function_with_evaluated_args(
    function: &EvalFunction,
    evaluated_args: Vec<EvaluatedCallArg>,
    context: &mut ElephcEvalContext,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    eval_dynamic_function_with_evaluated_args_and_ref_flags(
        function,
        function.parameter_is_by_ref(),
        evaluated_args,
        context,
        values,
    )
}

/// Evaluates an eval-declared function with caller-selected by-ref binding flags.
pub(in crate::interpreter) fn eval_dynamic_function_with_evaluated_args_and_ref_flags(
    function: &EvalFunction,
    parameter_is_by_ref: &[bool],
    evaluated_args: Vec<EvaluatedCallArg>,
    context: &mut ElephcEvalContext,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    let static_names = static_var_names(function.body());
    context.push_function(function.name());
    let evaluated_args = match bind_evaluated_method_args(
        function.params(),
        function.parameter_types(),
        function.parameter_defaults(),
        parameter_is_by_ref,
        function.parameter_is_variadic(),
        evaluated_args,
        context,
        values,
    ) {
        Ok(args) => args,
        Err(status) => {
            context.pop_function();
            return Err(status);
        }
    };
    let mut function_scope = ElephcEvalScope::new();
    bind_method_scope_args(
        &mut function_scope,
        function.params(),
        parameter_is_by_ref,
        &evaluated_args,
    );
    let result = execute_statements(function.body(), context, &mut function_scope, values);
    let persist_result = persist_static_locals(
        context,
        function.name(),
        &static_names,
        &function_scope,
        values,
    );
    let writeback_result = write_back_method_ref_args(
        function.params(),
        &evaluated_args,
        &function_scope,
        context,
        values,
    );
    let return_result = match (persist_result, writeback_result, result) {
        (Err(status), _, _) | (_, Err(status), _) | (_, _, Err(status)) => Err(status),
        (Ok(()), Ok(()), Ok(control)) => eval_declared_return_control_value(
            function.return_type(),
            None,
            None,
            control,
            context,
            values,
        ),
    };
    context.pop_function();
    return_result
}

/// Persists static local variables from one eval-declared function activation.
pub(super) fn persist_static_locals(
    context: &mut ElephcEvalContext,
    function_name: &str,
    names: &[String],
    scope: &ElephcEvalScope,
    values: &mut impl RuntimeValueOps,
) -> Result<(), EvalStatus> {
    for name in names {
        if let Some(cell) = scope.visible_cell(name) {
            if let Some(replaced) =
                context.set_static_local(function_name.to_string(), name.clone(), cell)
            {
                values.release(replaced)?;
            }
        }
    }
    Ok(())
}

/// One source-order static local declaration and its initializer expression.
#[derive(Clone)]
pub(in crate::interpreter) struct EvalStaticVarInitializer {
    pub name: String,
    pub init: EvalExpr,
}

/// Returns the distinct static local names declared anywhere in an eval function body.
pub(in crate::interpreter) fn static_var_names(body: &[EvalStmt]) -> Vec<String> {
    let mut names = Vec::new();
    let mut seen = std::collections::HashSet::new();
    visit_static_var_declarations(body, &mut seen, &mut |name, _| {
        names.push(name.to_string());
    });
    names
}

/// Returns static local declarations and initializers in first-seen source order.
pub(in crate::interpreter) fn static_var_initializers(
    body: &[EvalStmt],
) -> Vec<EvalStaticVarInitializer> {
    let mut vars = Vec::new();
    let mut seen = std::collections::HashSet::new();
    visit_static_var_declarations(body, &mut seen, &mut |name, init| {
        vars.push(EvalStaticVarInitializer {
            name: name.to_string(),
            init: init.clone(),
        });
    });
    vars
}

/// Visits distinct static local declarations in first-seen source order.
fn visit_static_var_declarations(
    body: &[EvalStmt],
    seen: &mut std::collections::HashSet<String>,
    visitor: &mut impl FnMut(&str, &EvalExpr),
) {
    for stmt in body {
        match stmt {
            EvalStmt::StaticVar { name, init } => {
                if seen.insert(name.clone()) {
                    visitor(name, init);
                }
            }
            EvalStmt::DoWhile { body, .. }
            | EvalStmt::Foreach { body, .. }
            | EvalStmt::For { body, .. }
            | EvalStmt::While { body, .. } => visit_static_var_declarations(body, seen, visitor),
            EvalStmt::FunctionDecl { .. } => {}
            EvalStmt::If {
                then_branch,
                else_branch,
                ..
            } => {
                visit_static_var_declarations(then_branch, seen, visitor);
                visit_static_var_declarations(else_branch, seen, visitor);
            }
            EvalStmt::Switch { cases, .. } => {
                for case in cases {
                    visit_static_var_declarations(&case.body, seen, visitor);
                }
            }
            EvalStmt::Try {
                body,
                catches,
                finally_body,
            } => {
                visit_static_var_declarations(body, seen, visitor);
                for catch in catches {
                    visit_static_var_declarations(&catch.body, seen, visitor);
                }
                visit_static_var_declarations(finally_body, seen, visitor);
            }
            EvalStmt::ArrayAppendVar { .. }
            | EvalStmt::ArraySetVar { .. }
            | EvalStmt::Break
            | EvalStmt::ClassDecl(_)
            | EvalStmt::Continue
            | EvalStmt::Echo(_)
            | EvalStmt::EnumDecl(_)
            | EvalStmt::Expr(_)
            | EvalStmt::Global { .. }
            | EvalStmt::InterfaceDecl(_)
            | EvalStmt::DynamicPropertyArrayAppend { .. }
            | EvalStmt::DynamicPropertyArraySet { .. }
            | EvalStmt::DynamicPropertyCompoundAssign { .. }
            | EvalStmt::DynamicPropertyIncDec { .. }
            | EvalStmt::DynamicPropertySet { .. }
            | EvalStmt::DynamicStaticPropertyIncDec { .. }
            | EvalStmt::DynamicStaticPropertySet { .. }
            | EvalStmt::PropertyReferenceBind { .. }
            | EvalStmt::PropertyArrayAppend { .. }
            | EvalStmt::PropertyArraySet { .. }
            | EvalStmt::PropertyCompoundAssign { .. }
            | EvalStmt::PropertyIncDec { .. }
            | EvalStmt::PropertySet { .. }
            | EvalStmt::ReferenceAssign { .. }
            | EvalStmt::Return(_)
            | EvalStmt::StaticPropertyIncDec { .. }
            | EvalStmt::StaticPropertySet { .. }
            | EvalStmt::StoreVar { .. }
            | EvalStmt::Throw(_)
            | EvalStmt::TraitDecl(_)
            | EvalStmt::UnsetArrayElement { .. }
            | EvalStmt::UnsetDynamicProperty { .. }
            | EvalStmt::UnsetDynamicStaticProperty { .. }
            | EvalStmt::UnsetProperty { .. }
            | EvalStmt::UnsetStaticProperty { .. }
            | EvalStmt::UnsetVar { .. } => {}
        }
    }
}

/// Evaluates a registered AOT function through its descriptor-compatible invoker.
pub(super) fn eval_native_function(
    function: NativeFunction,
    args: &[EvalCallArg],
    context: &mut ElephcEvalContext,
    caller_scope: &mut ElephcEvalScope,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    let evaluated_args = if function.param_names().len() == function.param_count() {
        eval_function_call_args(function.param_names(), args, context, caller_scope, values)?
    } else {
        eval_positional_call_arg_values(args, context, caller_scope, values)?
    };
    eval_native_function_with_values(function, evaluated_args, values)
}

/// Invokes a registered AOT function after its positional arguments are prepared.
pub(super) fn eval_native_function_with_values(
    function: NativeFunction,
    evaluated_args: Vec<RuntimeCellHandle>,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    if evaluated_args.len() != function.param_count() {
        return Err(EvalStatus::RuntimeFatal);
    }
    let arg_array = values.array_new(evaluated_args.len())?;
    for (index, value) in evaluated_args.into_iter().enumerate() {
        let index = values.int(index as i64)?;
        let _ = values.array_set(arg_array, index, value)?;
    }
    let result = unsafe { function.call(arg_array) };
    values.release(arg_array)?;
    if result.is_null() {
        return Err(EvalStatus::RuntimeFatal);
    }
    Ok(result)
}
