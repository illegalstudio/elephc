//! Purpose:
//! Resolves metadata shared by `ReflectionFunction` and `ReflectionMethod`.
//! It covers callable identity, source data, closures, statics, and prototypes.
//!
//! Called from:
//! - `crate::interpreter::reflection` while answering callable Reflection APIs.
//!
//! Key details:
//! - Eval, native, closure, and AOT method targets converge on one metadata shape.
//! - Prototype traversal preserves parent and interface declaration order.

use super::*;

/// Returns function or method metadata registered for a synthetic reflection owner object.
pub(super) fn eval_reflection_function_method_target(
    identity: u64,
    context: &ElephcEvalContext,
    values: &mut impl RuntimeValueOps,
) -> Result<Option<EvalReflectionFunctionMethodTarget>, EvalStatus> {
    if let Some(name) = context.eval_reflection_function_name(identity) {
        let closure_target = context
            .eval_reflection_function_closure_target(identity)
            .cloned();
        if let Some(closure) = context.closure(name) {
            let function = closure.function();
            let is_variadic = function.parameter_is_variadic().iter().any(|flag| *flag);
            let parameters = eval_reflection_function_parameters(
                function.name(),
                function.params(),
                function.attributes().to_vec(),
                function.parameter_attributes(),
                function.parameter_types(),
                function.parameter_defaults(),
                function.parameter_is_by_ref(),
                function.parameter_is_variadic(),
            );
            let source_location = function.source_location();
            let return_type_metadata = function
                .return_type()
                .and_then(eval_reflection_parameter_type_metadata);
            let static_key = Some(function.name().to_string());
            let static_variables = static_var_initializers(function.body());
            let is_deprecated =
                eval_reflection_attributes_include_deprecated(function.attributes());
            return Ok(Some(EvalReflectionFunctionMethodTarget::Function {
                name: name.to_string(),
                static_key,
                static_variables,
                closure_captures: closure.captures().to_vec(),
                parameters,
                source_location,
                closure_target,
                is_variadic,
                is_static: closure.is_static(),
                is_closure: true,
                is_deprecated,
                return_type_metadata,
            }));
        }
        let lookup_name = name.to_ascii_lowercase();
        if let Some(function) = context.function(&lookup_name) {
            let is_variadic = function
                .parameter_is_variadic()
                .iter()
                .any(|flag| *flag);
            let parameters = eval_reflection_function_parameters(
                function.name(),
                function.params(),
                function.attributes().to_vec(),
                function.parameter_attributes(),
                function.parameter_types(),
                function.parameter_defaults(),
                function.parameter_is_by_ref(),
                function.parameter_is_variadic(),
            );
            let source_location = function.source_location();
            let return_type_metadata = function
                .return_type()
                .and_then(eval_reflection_parameter_type_metadata);
            let static_key = Some(function.name().to_string());
            let static_variables = static_var_initializers(function.body());
            let is_deprecated =
                eval_reflection_attributes_include_deprecated(function.attributes());
            return Ok(Some(EvalReflectionFunctionMethodTarget::Function {
                name: name.to_string(),
                static_key,
                static_variables,
                closure_captures: Vec::new(),
                parameters,
                source_location,
                closure_target: closure_target.clone(),
                is_variadic,
                is_static: eval_reflection_closure_target_is_static(closure_target.as_ref()),
                is_closure: closure_target.is_some(),
                is_deprecated,
                return_type_metadata,
            }));
        }
        if let Some(function) = context.native_function(&lookup_name) {
            let parameters = eval_reflection_native_function_parameters(name, &function);
            let is_variadic =
                (0..function.param_count()).any(|index| function.param_variadic(index));
            let return_type_metadata = function
                .return_type()
                .and_then(eval_reflection_parameter_type_metadata);
            return Ok(Some(EvalReflectionFunctionMethodTarget::Function {
                name: name.to_string(),
                static_key: None,
                static_variables: Vec::new(),
                closure_captures: Vec::new(),
                parameters,
                source_location: None,
                closure_target: closure_target.clone(),
                is_variadic,
                is_static: eval_reflection_closure_target_is_static(closure_target.as_ref()),
                is_closure: closure_target.is_some(),
                is_deprecated: false,
                return_type_metadata,
            }));
        }
        return Ok(Some(EvalReflectionFunctionMethodTarget::Function {
            name: name.to_string(),
            static_key: None,
            static_variables: Vec::new(),
            closure_captures: Vec::new(),
            parameters: Vec::new(),
            source_location: None,
            closure_target: closure_target.clone(),
            is_variadic: false,
            is_static: eval_reflection_closure_target_is_static(closure_target.as_ref()),
            is_closure: closure_target.is_some(),
            is_deprecated: false,
            return_type_metadata: None,
        }));
    }
    let Some((declaring_class, method_name)) = context.eval_reflection_method(identity) else {
        return Ok(None);
    };
    let method_metadata = if let Some(method_metadata) =
        eval_reflection_method_metadata(declaring_class, method_name, context)
    {
        Some(method_metadata)
    } else {
        eval_reflection_aot_method_metadata_with_signature_if_exists(
            declaring_class,
            method_name,
            context,
            values,
        )?
    };
    let (
        parameters,
        source_file,
        source_location,
        visibility,
        is_variadic,
        is_static,
        is_final,
        is_abstract,
        is_deprecated,
        return_type_metadata,
    ) = match method_metadata {
        Some(method) => {
            let is_variadic = method
                .parameters
                .iter()
                .any(|parameter| parameter.is_variadic);
            let is_deprecated =
                eval_reflection_attributes_include_deprecated(&method.attributes);
            (
                method.parameters,
                method.source_file,
                method.source_location,
                Some(method.visibility),
                is_variadic,
                method.is_static,
                method.is_final,
                method.is_abstract,
                is_deprecated,
                method.return_type_metadata,
            )
        }
        None => (
            Vec::new(),
            None,
            None,
            None,
            false,
            false,
            false,
            false,
            false,
            None,
        ),
    };
    let static_method =
        eval_reflection_eval_method_static_target(declaring_class, method_name, context);
    let declaring_class = static_method
        .as_ref()
        .map(|(declaring_class, _)| declaring_class.clone());
    let static_key = static_method
        .as_ref()
        .map(|(declaring_class, method)| eval_method_static_local_key(declaring_class, method.name()));
    let static_variables = static_method
        .as_ref()
        .map(|(_, method)| static_var_initializers(method.body()))
        .unwrap_or_default();
    Ok(Some(EvalReflectionFunctionMethodTarget::Method {
        declaring_class,
        name: method_name.to_string(),
        static_key,
        static_variables,
        parameters,
        source_file,
        source_location,
        visibility,
        is_variadic,
        is_static,
        is_final,
        is_abstract,
        is_deprecated,
        return_type_metadata,
    }))
}

/// Returns an eval method body that can contribute ReflectionMethod static locals.
pub(super) fn eval_reflection_eval_method_static_target(
    declaring_class: &str,
    method_name: &str,
    context: &ElephcEvalContext,
) -> Option<(String, EvalClassMethod)> {
    if context.has_class(declaring_class) || context.has_enum(declaring_class) {
        return context.class_method(declaring_class, method_name);
    }
    let trait_decl = context.trait_decl(declaring_class)?;
    trait_decl
        .methods()
        .iter()
        .find(|method| method.name().eq_ignore_ascii_case(method_name))
        .map(|method| (trait_decl.name().to_string(), method.clone()))
}

/// Builds the static-local storage key shared by method execution and reflection.
pub(super) fn eval_method_static_local_key(class_name: &str, method_name: &str) -> String {
    format!("{}::{}", class_name.trim_start_matches('\\'), method_name)
}

/// Builds the associative `getStaticVariables()` result for eval-backed reflection.
pub(super) fn eval_reflection_function_method_static_variables_result(
    target: &EvalReflectionFunctionMethodTarget,
    context: &mut ElephcEvalContext,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    let (Some(static_key), static_variables, declaring_class) =
        eval_reflection_function_method_static_target(target)
    else {
        return values.array_new(0);
    };
    let mut result = values.assoc_new(static_variables.len())?;
    for variable in static_variables {
        let key = values.string(&variable.name)?;
        let value = eval_reflection_static_local_value(
            static_key,
            variable,
            declaring_class,
            context,
            values,
        )?;
        result = values.array_set(result, key, value)?;
    }
    Ok(result)
}

/// Returns static-local storage details retained for a reflected eval function or method.
pub(super) fn eval_reflection_function_method_static_target(
    target: &EvalReflectionFunctionMethodTarget,
) -> (
    Option<&str>,
    &[EvalStaticVarInitializer],
    Option<&str>,
) {
    match target {
        EvalReflectionFunctionMethodTarget::Function {
            static_key,
            static_variables,
            ..
        } => (static_key.as_deref(), static_variables, None),
        EvalReflectionFunctionMethodTarget::Method {
            declaring_class,
            static_key,
            static_variables,
            ..
        } => (
            static_key.as_deref(),
            static_variables,
            declaring_class.as_deref(),
        ),
    }
}

/// Returns the retained current static value or initializes it for reflection.
pub(super) fn eval_reflection_static_local_value(
    static_key: &str,
    variable: &EvalStaticVarInitializer,
    declaring_class: Option<&str>,
    context: &mut ElephcEvalContext,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    if let Some(value) = context.static_local(static_key, &variable.name) {
        return values.retain(value);
    }
    let value = eval_reflection_static_local_initializer_value(
        static_key,
        &variable.init,
        declaring_class,
        context,
        values,
    )?;
    if let Some(replaced) =
        context.set_static_local(static_key.to_string(), variable.name.clone(), value)
    {
        values.release(replaced)?;
    }
    values.retain(value)
}

/// Evaluates a static-local initializer with PHP magic class/function context.
pub(super) fn eval_reflection_static_local_initializer_value(
    static_key: &str,
    init: &EvalExpr,
    declaring_class: Option<&str>,
    context: &mut ElephcEvalContext,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    if let Some(declaring_class) = declaring_class {
        context.push_class_scope(declaring_class.to_string());
        context.push_called_class_scope(declaring_class.to_string());
    }
    context.push_function(static_key.to_string());
    let mut scope = ElephcEvalScope::new();
    let result = eval_expr(init, context, &mut scope, values);
    for cell in scope.drain_owned_cells() {
        values.release(cell)?;
    }
    context.pop_function();
    if declaring_class.is_some() {
        context.pop_called_class_scope();
        context.pop_class_scope();
    }
    result
}

/// Validates that a synthetic reflection metadata call received no arguments.
pub(super) fn eval_reflection_bind_no_args(evaluated_args: Vec<EvaluatedCallArg>) -> Result<(), EvalStatus> {
    let _ = bind_evaluated_function_args(&[], evaluated_args)?;
    Ok(())
}

/// Returns a no-argument reflection metadata predicate result that is always false.
pub(super) fn eval_reflection_false_metadata_result(
    evaluated_args: Vec<EvaluatedCallArg>,
    values: &mut impl RuntimeValueOps,
) -> Result<Option<RuntimeCellHandle>, EvalStatus> {
    eval_reflection_bind_no_args(evaluated_args)?;
    values.bool_value(false).map(Some)
}

/// Returns source file or line metadata for eval-backed reflection objects.
pub(super) fn eval_reflection_source_location_result(
    method_key: &str,
    source_file: Option<&str>,
    source_location: Option<EvalSourceLocation>,
    evaluated_args: Vec<EvaluatedCallArg>,
    context: &ElephcEvalContext,
    values: &mut impl RuntimeValueOps,
) -> Result<Option<RuntimeCellHandle>, EvalStatus> {
    eval_reflection_bind_no_args(evaluated_args)?;
    let Some(source_location) = source_location else {
        return values.bool_value(false).map(Some);
    };
    match method_key {
        "getfilename" => {
            let eval_file;
            let file = if let Some(source_file) = source_file {
                source_file
            } else {
                eval_file = context.eval_file_magic();
                &eval_file
            };
            values.string(file).map(Some)
        }
        "getstartline" => values.int(source_location.start_line()).map(Some),
        "getendline" => values.int(source_location.end_line()).map(Some),
        _ => Ok(None),
    }
}

/// Returns PHP's short name for a ReflectionFunction or ReflectionMethod target.
pub(super) fn eval_reflection_function_method_short_name(
    target: &EvalReflectionFunctionMethodTarget,
) -> String {
    match target {
        EvalReflectionFunctionMethodTarget::Function { name, .. } => {
            eval_reflection_short_name(name)
        }
        EvalReflectionFunctionMethodTarget::Method { name, .. } => name.clone(),
    }
}

/// Returns eval-fragment source metadata for a ReflectionFunction or ReflectionMethod target.
pub(super) fn eval_reflection_function_method_source_location(
    target: &EvalReflectionFunctionMethodTarget,
) -> (Option<&str>, Option<EvalSourceLocation>) {
    match target {
        EvalReflectionFunctionMethodTarget::Function {
            source_location, ..
        } => (None, *source_location),
        EvalReflectionFunctionMethodTarget::Method {
            source_file,
            source_location, ..
        } => (source_file.as_deref(), *source_location),
    }
}

/// Returns PHP's namespace name for a ReflectionFunction or ReflectionMethod target.
pub(super) fn eval_reflection_function_method_namespace_name(
    target: &EvalReflectionFunctionMethodTarget,
) -> String {
    match target {
        EvalReflectionFunctionMethodTarget::Function { name, .. } => {
            eval_reflection_namespace_name(name)
        }
        EvalReflectionFunctionMethodTarget::Method { .. } => String::new(),
    }
}

/// Returns whether the reflected function or method has a variadic parameter.
pub(super) fn eval_reflection_function_method_is_variadic(
    target: &EvalReflectionFunctionMethodTarget,
) -> bool {
    match target {
        EvalReflectionFunctionMethodTarget::Function { is_variadic, .. }
        | EvalReflectionFunctionMethodTarget::Method { is_variadic, .. } => *is_variadic,
    }
}

/// Returns whether the reflected function-like target is an eval closure literal.
pub(super) fn eval_reflection_function_method_is_closure(
    target: &EvalReflectionFunctionMethodTarget,
) -> bool {
    match target {
        EvalReflectionFunctionMethodTarget::Function { is_closure, .. } => *is_closure,
        EvalReflectionFunctionMethodTarget::Method { .. } => false,
    }
}

/// Returns whether the reflected function-like target is static.
pub(super) fn eval_reflection_function_method_is_static(target: &EvalReflectionFunctionMethodTarget) -> bool {
    match target {
        EvalReflectionFunctionMethodTarget::Function { is_static, .. }
        | EvalReflectionFunctionMethodTarget::Method { is_static, .. } => *is_static,
    }
}

/// Returns whether retained Closure target metadata represents a static callable.
pub(super) fn eval_reflection_closure_target_is_static(target: Option<&EvalClosureObjectTarget>) -> bool {
    matches!(target, Some(EvalClosureObjectTarget::StaticMethod { .. }))
}

/// Returns whether the reflected function-like target carries `#[Deprecated]`.
pub(super) fn eval_reflection_function_method_is_deprecated(
    target: &EvalReflectionFunctionMethodTarget,
) -> bool {
    match target {
        EvalReflectionFunctionMethodTarget::Function { is_deprecated, .. }
        | EvalReflectionFunctionMethodTarget::Method { is_deprecated, .. } => *is_deprecated,
    }
}

/// Builds `ReflectionFunction::getClosureUsedVariables()` for eval closure targets.
pub(super) fn eval_reflection_function_closure_used_variables_result(
    target: &EvalReflectionFunctionMethodTarget,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    let EvalReflectionFunctionMethodTarget::Function {
        is_closure: true,
        closure_captures,
        ..
    } = target
    else {
        return values.array_new(0);
    };
    let mut result = values.assoc_new(closure_captures.len())?;
    for capture in closure_captures {
        let key = values.string(capture.name())?;
        let value = values.retain(capture.value())?;
        result = values.array_set(result, key, value)?;
    }
    Ok(result)
}

/// Builds `ReflectionFunction::getClosureThis()` from retained Closure target metadata.
pub(super) fn eval_reflection_function_closure_this_result(
    target: &EvalReflectionFunctionMethodTarget,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    let Some(target) = eval_reflection_function_closure_target(target) else {
        return values.null();
    };
    match target {
        EvalClosureObjectTarget::BoundNamed {
            bound_this: Some(object),
            ..
        }
        | EvalClosureObjectTarget::InvokableObject { object }
        | EvalClosureObjectTarget::ObjectMethod { object, .. } => values.retain(*object),
        EvalClosureObjectTarget::Named(_)
        | EvalClosureObjectTarget::BoundNamed {
            bound_this: None, ..
        }
        | EvalClosureObjectTarget::StaticMethod { .. } => values.null(),
    }
}

/// Builds `ReflectionFunction::getClosureScopeClass()` from retained Closure metadata.
pub(super) fn eval_reflection_function_closure_scope_class_result(
    target: &EvalReflectionFunctionMethodTarget,
    context: &mut ElephcEvalContext,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    let class_name =
        eval_reflection_function_closure_scope_class_name(target, context, values)?;
    eval_reflection_function_closure_class_object_result(class_name, context, values)
}

/// Builds `ReflectionFunction::getClosureCalledClass()` from retained Closure metadata.
pub(super) fn eval_reflection_function_closure_called_class_result(
    target: &EvalReflectionFunctionMethodTarget,
    context: &mut ElephcEvalContext,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    let class_name =
        eval_reflection_function_closure_called_class_name(target, context, values)?;
    eval_reflection_function_closure_class_object_result(class_name, context, values)
}

/// Returns the retained callable target for a Closure-backed ReflectionFunction.
pub(super) fn eval_reflection_function_closure_target(
    target: &EvalReflectionFunctionMethodTarget,
) -> Option<&EvalClosureObjectTarget> {
    match target {
        EvalReflectionFunctionMethodTarget::Function { closure_target, .. } => {
            closure_target.as_ref()
        }
        EvalReflectionFunctionMethodTarget::Method { .. } => None,
    }
}

/// Resolves the PHP closure scope class name for retained Closure target metadata.
pub(super) fn eval_reflection_function_closure_scope_class_name(
    target: &EvalReflectionFunctionMethodTarget,
    context: &ElephcEvalContext,
    values: &mut impl RuntimeValueOps,
) -> Result<Option<String>, EvalStatus> {
    let Some(target) = eval_reflection_function_closure_target(target) else {
        return Ok(None);
    };
    match target {
        EvalClosureObjectTarget::Named(_) => Ok(None),
        EvalClosureObjectTarget::BoundNamed {
            name,
            bound_this,
            bound_scope,
        } => {
            if context.closure(name).is_none() {
                return Ok(bound_this.map(|_| String::from("Closure")));
            }
            if let Some(bound_scope) = bound_scope {
                return Ok(Some(bound_scope.clone()));
            }
            match bound_this {
                Some(object) => eval_closure_bound_object_class_name(*object, context, values)
                    .map(Some),
                None => Ok(None),
            }
        }
        EvalClosureObjectTarget::InvokableObject { object }
        | EvalClosureObjectTarget::ObjectMethod { object, .. } => {
            eval_closure_bound_object_class_name(*object, context, values).map(Some)
        }
        EvalClosureObjectTarget::StaticMethod { class_name, .. } => {
            Ok(Some(class_name.trim_start_matches('\\').to_string()))
        }
    }
}

/// Resolves the PHP closure called class name for retained Closure target metadata.
pub(super) fn eval_reflection_function_closure_called_class_name(
    target: &EvalReflectionFunctionMethodTarget,
    context: &ElephcEvalContext,
    values: &mut impl RuntimeValueOps,
) -> Result<Option<String>, EvalStatus> {
    let Some(target) = eval_reflection_function_closure_target(target) else {
        return Ok(None);
    };
    match target {
        EvalClosureObjectTarget::Named(_) => Ok(None),
        EvalClosureObjectTarget::BoundNamed {
            bound_this,
            bound_scope,
            ..
        } => match bound_this {
            Some(object) => eval_closure_bound_object_class_name(*object, context, values)
                .map(Some),
            None => Ok(bound_scope.clone()),
        },
        EvalClosureObjectTarget::InvokableObject { object } => {
            eval_closure_bound_object_class_name(*object, context, values).map(Some)
        }
        EvalClosureObjectTarget::ObjectMethod {
            object,
            called_class,
            ..
        } => match called_class {
            Some(called_class) => Ok(Some(called_class.clone())),
            None => eval_closure_bound_object_class_name(*object, context, values).map(Some),
        },
        EvalClosureObjectTarget::StaticMethod {
            class_name,
            called_class,
            ..
        } => Ok(Some(
            called_class
                .as_deref()
                .unwrap_or(class_name)
                .trim_start_matches('\\')
                .to_string(),
        )),
    }
}

/// Materializes a nullable ReflectionClass result for Closure scope metadata.
pub(super) fn eval_reflection_function_closure_class_object_result(
    class_name: Option<String>,
    context: &mut ElephcEvalContext,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    let Some(class_name) = class_name else {
        return values.null();
    };
    eval_reflection_full_class_object_result(&class_name, context, values)
}

/// Returns the retained return type metadata for a reflected function or method.
pub(super) fn eval_reflection_function_method_return_type(
    target: &EvalReflectionFunctionMethodTarget,
) -> Option<&EvalReflectionParameterTypeMetadata> {
    match target {
        EvalReflectionFunctionMethodTarget::Function {
            return_type_metadata,
            ..
        }
        | EvalReflectionFunctionMethodTarget::Method {
            return_type_metadata,
            ..
        } => return_type_metadata.as_ref(),
    }
}

/// Returns the final namespace segment-free name component from a PHP symbol name.
pub(super) fn eval_reflection_short_name(name: &str) -> String {
    let name = name.trim_start_matches('\\');
    name.rsplit_once('\\').map_or_else(
        || name.to_string(),
        |(_, short_name)| short_name.to_string(),
    )
}

/// Returns the namespace prefix from a PHP function name, or an empty string.
pub(super) fn eval_reflection_namespace_name(name: &str) -> String {
    name.trim_start_matches('\\')
        .rsplit_once('\\')
        .map_or_else(String::new, |(namespace_name, _)| {
            namespace_name.to_string()
        })
}

/// Builds ReflectionMethod metadata for a resolved eval or AOT prototype target.
pub(super) fn eval_reflection_prototype_method_metadata(
    prototype_class: &str,
    prototype_method: &str,
    context: &ElephcEvalContext,
    values: &mut impl RuntimeValueOps,
) -> Result<Option<EvalReflectionMemberMetadata>, EvalStatus> {
    if let Some(metadata) =
        eval_reflection_method_metadata(prototype_class, prototype_method, context)
    {
        return Ok(Some(metadata));
    }
    eval_reflection_aot_method_metadata_with_signature_if_exists(
        prototype_class,
        prototype_method,
        context,
        values,
    )
}

/// Finds the PHP ReflectionMethod prototype target for an eval or AOT method.
pub(super) fn eval_reflection_method_prototype_target(
    declaring_class: &str,
    method_name: &str,
    context: &ElephcEvalContext,
    values: &mut impl RuntimeValueOps,
) -> Result<Option<(String, String)>, EvalStatus> {
    if context.has_class(declaring_class) || context.has_enum(declaring_class) {
        let want_static = eval_reflection_method_metadata(declaring_class, method_name, context)
            .map_or(false, |metadata| metadata.is_static);
        if let Some(prototype) =
            eval_reflection_parent_method_prototype_target(declaring_class, method_name, context)
        {
            return Ok(Some(prototype));
        }
        if let Some(prototype) = eval_reflection_eval_aot_parent_method_prototype_target(
            declaring_class,
            method_name,
            context,
            want_static,
            values,
        )? {
            return Ok(Some(prototype));
        }
        if let Some(prototype) =
            eval_reflection_interface_method_prototype_target(declaring_class, method_name, context)
        {
            return Ok(Some(prototype));
        }
        return eval_reflection_aot_interface_method_prototype_target_for_eval(
            declaring_class,
            method_name,
            context,
            want_static,
            values,
        );
    }
    eval_reflection_aot_method_prototype_target(declaring_class, method_name, values)
}

/// Finds the PHP ReflectionMethod prototype target for a generated/AOT method.
pub(super) fn eval_reflection_aot_method_prototype_target(
    declaring_class: &str,
    method_name: &str,
    values: &mut impl RuntimeValueOps,
) -> Result<Option<(String, String)>, EvalStatus> {
    let Some(flags) = values.reflection_method_flags(declaring_class, method_name)? else {
        return Ok(None);
    };
    if let Some(prototype) =
        eval_reflection_aot_parent_method_prototype_target(declaring_class, method_name, flags, values)?
    {
        return Ok(Some(prototype));
    }
    eval_reflection_aot_interface_method_prototype_target(
        declaring_class,
        method_name,
        flags,
        values,
    )
}

/// Finds the nearest generated/AOT parent-class method that can act as prototype.
pub(super) fn eval_reflection_aot_parent_method_prototype_target(
    declaring_class: &str,
    method_name: &str,
    method_flags: u64,
    values: &mut impl RuntimeValueOps,
) -> Result<Option<(String, String)>, EvalStatus> {
    let want_static = method_flags & EVAL_REFLECTION_MEMBER_FLAG_STATIC != 0;
    let mut current = eval_reflection_aot_parent_class_name(declaring_class, values)?;
    let mut seen = std::collections::HashSet::new();
    while let Some(parent_class) = current {
        if !seen.insert(parent_class.to_ascii_lowercase()) {
            break;
        }
        if let Some(prototype) =
            eval_reflection_aot_method_candidate(&parent_class, method_name, want_static, values)?
        {
            return Ok(Some(prototype));
        }
        current = eval_reflection_aot_parent_class_name(&parent_class, values)?;
    }
    Ok(None)
}

/// Finds the first generated/AOT interface method that can act as prototype.
pub(super) fn eval_reflection_aot_interface_method_prototype_target(
    declaring_class: &str,
    method_name: &str,
    method_flags: u64,
    values: &mut impl RuntimeValueOps,
) -> Result<Option<(String, String)>, EvalStatus> {
    let want_static = method_flags & EVAL_REFLECTION_MEMBER_FLAG_STATIC != 0;
    for interface_name in eval_reflection_aot_class_interface_names(declaring_class, values)? {
        if let Some(prototype) =
            eval_reflection_aot_method_candidate(&interface_name, method_name, want_static, values)?
        {
            return Ok(Some(prototype));
        }
    }
    Ok(None)
}

/// Returns one generated/AOT method prototype candidate if staticness and visibility match.
pub(super) fn eval_reflection_aot_method_candidate(
    class_name: &str,
    method_name: &str,
    want_static: bool,
    values: &mut impl RuntimeValueOps,
) -> Result<Option<(String, String)>, EvalStatus> {
    let Some(flags) = values.reflection_method_flags(class_name, method_name)? else {
        return Ok(None);
    };
    let is_static = flags & EVAL_REFLECTION_MEMBER_FLAG_STATIC != 0;
    let is_private = flags & EVAL_REFLECTION_MEMBER_FLAG_PRIVATE != 0;
    if is_static != want_static || is_private {
        return Ok(None);
    }
    let declaring_class = values
        .reflection_method_declaring_class(class_name, method_name)?
        .unwrap_or_else(|| class_name.trim_start_matches('\\').to_string());
    Ok(Some((declaring_class, method_name.to_ascii_lowercase())))
}

/// Finds the nearest parent-class method prototype for an eval-declared override.
pub(super) fn eval_reflection_parent_method_prototype_target(
    declaring_class: &str,
    method_name: &str,
    context: &ElephcEvalContext,
) -> Option<(String, String)> {
    for parent_class in context.class_parent_names(declaring_class) {
        if let Some((prototype_class, prototype_method)) =
            context.class_own_method(&parent_class, method_name)
        {
            return Some((prototype_class, prototype_method.name().to_string()));
        }
    }
    None
}

/// Finds the nearest generated/AOT parent-class method prototype for an eval method.
pub(super) fn eval_reflection_eval_aot_parent_method_prototype_target(
    declaring_class: &str,
    method_name: &str,
    context: &ElephcEvalContext,
    want_static: bool,
    values: &mut impl RuntimeValueOps,
) -> Result<Option<(String, String)>, EvalStatus> {
    let Some(parent_class) = context.class_native_parent_name(declaring_class) else {
        return Ok(None);
    };
    eval_reflection_aot_method_candidate(&parent_class, method_name, want_static, values)
}

/// Finds the interface method prototype for an eval-declared class method.
pub(super) fn eval_reflection_interface_method_prototype_target(
    declaring_class: &str,
    method_name: &str,
    context: &ElephcEvalContext,
) -> Option<(String, String)> {
    let mut seen = std::collections::HashSet::new();
    for interface_name in context.class_interface_names(declaring_class) {
        if let Some(prototype) = eval_reflection_interface_declared_method_target(
            &interface_name,
            method_name,
            context,
            &mut seen,
        ) {
            return Some(prototype);
        }
    }
    None
}

/// Finds an AOT interface method prototype for an eval-declared class method.
pub(super) fn eval_reflection_aot_interface_method_prototype_target_for_eval(
    declaring_class: &str,
    method_name: &str,
    context: &ElephcEvalContext,
    want_static: bool,
    values: &mut impl RuntimeValueOps,
) -> Result<Option<(String, String)>, EvalStatus> {
    for interface_name in eval_reflection_eval_class_interface_names(
        declaring_class,
        context,
        values,
    )? {
        if context.has_interface(&interface_name) {
            continue;
        }
        if let Some(prototype) =
            eval_reflection_aot_method_candidate(&interface_name, method_name, want_static, values)?
        {
            return Ok(Some(prototype));
        }
    }
    Ok(None)
}

/// Finds the interface that actually declares a method in an interface hierarchy.
pub(super) fn eval_reflection_interface_declared_method_target(
    interface_name: &str,
    method_name: &str,
    context: &ElephcEvalContext,
    seen: &mut std::collections::HashSet<String>,
) -> Option<(String, String)> {
    let interface = context.interface(interface_name)?;
    if !seen.insert(interface.name().to_ascii_lowercase()) {
        return None;
    }
    if let Some(method) = interface
        .methods()
        .iter()
        .find(|method| method.name().eq_ignore_ascii_case(method_name))
    {
        return Some((interface.name().to_string(), method.name().to_string()));
    }
    for parent in interface.parents() {
        if let Some(prototype) =
            eval_reflection_interface_declared_method_target(parent, method_name, context, seen)
        {
            return Some(prototype);
        }
    }
    None
}
