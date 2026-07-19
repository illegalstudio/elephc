//! Purpose:
//! Binds and coerces evaluated method arguments against eval signatures.
//!
//! Called from:
//! - Dynamic instance, static, closure, and reflected method invocation.
//!
//! Key details:
//! - Reference targets, scalar coercion, defaults, and object type checks stay aligned by index.

use super::*;

/// Binds evaluated method arguments using a selected by-reference target policy.
pub(in crate::interpreter) fn bind_evaluated_method_args_with_ref_mode(
    params: &[String],
    parameter_types: &[Option<EvalParameterType>],
    parameter_defaults: &[Option<EvalExpr>],
    parameter_is_by_ref: &[bool],
    parameter_is_variadic: &[bool],
    evaluated_args: Vec<EvaluatedCallArg>,
    by_ref_mode: EvalByRefBindingMode<'_>,
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
                by_ref_mode,
                &mut variadic_named_args,
                context,
                values,
            )?;
        } else {
            bind_dynamic_positional_method_arg(
                params,
                &mut bound_args,
                parameter_types,
                parameter_is_by_ref,
                variadic_index,
                &mut next_positional,
                &mut next_variadic_index,
                arg.value,
                arg.ref_target,
                by_ref_mode,
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
    params: &[String],
    bound_args: &mut [Option<BoundMethodArg>],
    parameter_types: &[Option<EvalParameterType>],
    parameter_is_by_ref: &[bool],
    variadic_index: Option<usize>,
    next_positional: &mut usize,
    next_variadic_index: &mut i64,
    value: RuntimeCellHandle,
    ref_target: Option<EvalReferenceTarget>,
    by_ref_mode: EvalByRefBindingMode<'_>,
    context: &mut ElephcEvalContext,
    values: &mut impl RuntimeValueOps,
) -> Result<(), EvalStatus> {
    if variadic_index.is_some_and(|index| *next_positional >= index) {
        let argument_number = variadic_index
            .and_then(|index| {
                usize::try_from(*next_variadic_index)
                    .ok()
                    .and_then(|offset| index.checked_add(offset))
            })
            .and_then(|index| index.checked_add(1))
            .ok_or(EvalStatus::RuntimeFatal)?;
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
        let ref_target = method_parameter_ref_target(
            params,
            parameter_is_by_ref,
            variadic_index,
            argument_number,
            ref_target,
            by_ref_mode,
            values,
        )?;
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
    let ref_target = method_parameter_ref_target(
        params,
        parameter_is_by_ref,
        Some(param_index),
        param_index + 1,
        ref_target,
        by_ref_mode,
        values,
    )?;
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
    by_ref_mode: EvalByRefBindingMode<'_>,
    variadic_named_args: &mut std::collections::HashSet<String>,
    context: &mut ElephcEvalContext,
    values: &mut impl RuntimeValueOps,
) -> Result<(), EvalStatus> {
    if let Some(param_index) = regular_method_param_index(params, variadic_index, name) {
        if bound_args[param_index].is_some() {
            return Err(EvalStatus::RuntimeFatal);
        }
        let ref_target = method_parameter_ref_target(
            params,
            parameter_is_by_ref,
            Some(param_index),
            param_index + 1,
            ref_target,
            by_ref_mode,
            values,
        )?;
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
    let argument_number = variadic_index
        .and_then(|index| index.checked_add(1))
        .ok_or(EvalStatus::RuntimeFatal)?;
    let ref_target = method_parameter_ref_target(
        params,
        parameter_is_by_ref,
        variadic_index,
        argument_number,
        ref_target,
        by_ref_mode,
        values,
    )?;
    bind_dynamic_variadic_arg(bound_args, variadic_index, key, value, ref_target, values)
}

/// Returns the caller writeback target required by a by-reference method parameter.
fn method_parameter_ref_target(
    params: &[String],
    parameter_is_by_ref: &[bool],
    param_index: Option<usize>,
    argument_number: usize,
    ref_target: Option<EvalReferenceTarget>,
    by_ref_mode: EvalByRefBindingMode<'_>,
    values: &mut impl RuntimeValueOps,
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
    if let Some(ref_target) = ref_target {
        return Ok(Some(ref_target));
    }
    match by_ref_mode {
        EvalByRefBindingMode::RequireTarget => Err(EvalStatus::RuntimeFatal),
        EvalByRefBindingMode::WarnByValue { callable_name } => {
            let param_name = params
                .get(param_index)
                .map(String::as_str)
                .unwrap_or("arg");
            values.warning(&format!(
                "{callable_name}(): Argument #{} (${param_name}) must be passed by reference, value given",
                argument_number
            ))?;
            Ok(None)
        }
    }
}

/// Returns the by-reference flags that should be installed into the callee scope.
pub(in crate::interpreter) fn method_scope_parameter_ref_flags(
    parameter_is_by_ref: &[bool],
    bound_args: &[BoundMethodArg],
    by_ref_mode: EvalByRefBindingMode<'_>,
) -> Vec<bool> {
    if matches!(by_ref_mode, EvalByRefBindingMode::RequireTarget) {
        return parameter_is_by_ref.to_vec();
    }
    parameter_is_by_ref
        .iter()
        .enumerate()
        .map(|(position, is_by_ref)| {
            *is_by_ref
                && bound_args.get(position).is_some_and(|arg| {
                    arg.ref_target.is_some() || !arg.variadic_ref_targets.is_empty()
                })
        })
        .collect()
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
        EvalArrayElement::Reference(_) => false,
        EvalArrayElement::KeyValue { key, value } => {
            eval_method_default_expr_is_supported(key)
                && eval_method_default_expr_is_supported(value)
        }
        EvalArrayElement::KeyReference { .. } => false,
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
