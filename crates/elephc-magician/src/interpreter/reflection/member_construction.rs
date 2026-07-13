//! Purpose:
//! Constructs ReflectionMethod and ReflectionProperty owners and their AOT metadata.
//!
//! Called from:
//! - `crate::interpreter::reflection` and reflected static method dispatch.
//!
//! Key details:
//! - Eval, object, dynamic-property, native, and AOT targets converge here.
//! - Native callable defaults are converted into eval expressions without execution.

use super::*;

/// Handles eval-backed `ReflectionMethod::createFromMethodName()` static calls.
pub(in crate::interpreter) fn eval_reflection_method_create_from_method_name_result(
    class_name: &str,
    method_name: &str,
    evaluated_args: Vec<EvaluatedCallArg>,
    context: &mut ElephcEvalContext,
    values: &mut impl RuntimeValueOps,
) -> Result<Option<RuntimeCellHandle>, EvalStatus> {
    if !class_name
        .trim_start_matches('\\')
        .eq_ignore_ascii_case("ReflectionMethod")
        || !method_name.eq_ignore_ascii_case("createFromMethodName")
    {
        return Ok(None);
    }
    let args = bind_evaluated_function_args(&[String::from("method")], evaluated_args)?;
    let target = eval_reflection_string_arg(args[0], values)?;
    let Some((class_name, method_name)) = eval_reflection_method_target_parts(&target) else {
        return eval_throw_reflection_exception(
            "ReflectionMethod::createFromMethodName(): Argument #1 ($method) must be a valid method name",
            context,
            values,
        );
    };
    eval_reflection_method_object_result_or_throw(
        &class_name,
        &method_name,
        context,
        values,
    )
}

/// Splits PHP's `ClassName::methodName` reflection-method target string.
pub(super) fn eval_reflection_method_target_parts(target: &str) -> Option<(String, String)> {
    let Some((class_name, method_name)) = target.rsplit_once("::") else {
        return None;
    };
    if class_name.is_empty() || method_name.is_empty() {
        return None;
    }
    Some((class_name.to_string(), method_name.to_string()))
}

/// Extracts the deprecated one-argument `ReflectionMethod("Class::method")` target.
pub(super) fn eval_reflection_method_single_target_arg(
    evaluated_args: Vec<EvaluatedCallArg>,
) -> Result<RuntimeCellHandle, EvalStatus> {
    let mut args = evaluated_args.into_iter();
    let Some(arg) = args.next() else {
        return Err(EvalStatus::RuntimeFatal);
    };
    if args.next().is_some() {
        return Err(EvalStatus::RuntimeFatal);
    }
    if let Some(name) = arg.name.as_deref() {
        if !matches!(name, "class_name" | "objectOrMethod") {
            return Err(EvalStatus::RuntimeFatal);
        }
    }
    Ok(arg.value)
}

/// Builds a `ReflectionMethod` object when the reflected method exists in eval or AOT metadata.
pub(super) fn eval_reflection_method_object_result_if_exists(
    class_name: &str,
    requested_method_name: &str,
    context: &mut ElephcEvalContext,
    values: &mut impl RuntimeValueOps,
) -> Result<Option<RuntimeCellHandle>, EvalStatus> {
    let reflected_name = context
        .resolve_class_like_name(class_name)
        .unwrap_or_else(|| class_name.trim_start_matches('\\').to_string());
    if !eval_reflection_class_like_exists(&reflected_name, context) {
        if let Some(method) = eval_reflection_aot_method_metadata_with_signature_if_exists(
            &reflected_name,
            requested_method_name,
            context,
            values,
        )? {
            let method_name = requested_method_name.to_ascii_lowercase();
            return eval_reflection_member_object_result(
                EVAL_REFLECTION_OWNER_METHOD,
                &method_name,
                &method,
                context,
                values,
            )
            .map(Some);
        }
        return Ok(None);
    }
    let method_name = eval_reflection_member_name(
        EVAL_REFLECTION_OWNER_METHOD,
        &reflected_name,
        requested_method_name,
        context,
    );
    let Some(method_name) = method_name else {
        return Ok(None);
    };
    let Some(method) = eval_reflection_method_metadata(&reflected_name, &method_name, context)
    else {
        return Ok(None);
    };
    eval_reflection_member_object_result(
        EVAL_REFLECTION_OWNER_METHOD,
        &method_name,
        &method,
        context,
        values,
    )
    .map(Some)
}

/// Builds a `ReflectionMethod` object or throws PHP's catchable reflection error.
pub(super) fn eval_reflection_method_object_result_or_throw(
    class_name: &str,
    requested_method_name: &str,
    context: &mut ElephcEvalContext,
    values: &mut impl RuntimeValueOps,
) -> Result<Option<RuntimeCellHandle>, EvalStatus> {
    if let Some(result) = eval_reflection_method_object_result_if_exists(
        class_name,
        requested_method_name,
        context,
        values,
    )? {
        return Ok(Some(result));
    }
    let reflected_name = context
        .resolve_class_like_name(class_name)
        .unwrap_or_else(|| class_name.trim_start_matches('\\').to_string());
    if !eval_reflection_class_like_or_runtime_exists(&reflected_name, context, values)? {
        return eval_throw_reflection_exception(
            &format!("Class \"{}\" does not exist", reflected_name),
            context,
            values,
        );
    }
    eval_throw_reflection_exception(
        &format!(
            "Method {}::{}() does not exist",
            reflected_name, requested_method_name
        ),
        context,
        values,
    )
}

/// Builds an eval-backed `ReflectionMethod` object when the reflected method exists in eval.
pub(super) fn eval_reflection_method_new(
    evaluated_args: Vec<EvaluatedCallArg>,
    context: &mut ElephcEvalContext,
    values: &mut impl RuntimeValueOps,
) -> Result<Option<RuntimeCellHandle>, EvalStatus> {
    if evaluated_args.len() == 1 {
        let target = eval_reflection_method_single_target_arg(evaluated_args)?;
        let target = eval_reflection_string_arg(target, values)?;
        let Some((class_name, method_name)) = eval_reflection_method_target_parts(&target) else {
            return eval_throw_reflection_exception(
                "ReflectionMethod::__construct(): Argument #1 ($objectOrMethod) must be a valid method name",
                context,
                values,
            );
        };
        return eval_reflection_method_object_result_or_throw(
            &class_name,
            &method_name,
            context,
            values,
        );
    }
    let args = bind_evaluated_function_args(
        &[String::from("class_name"), String::from("method_name")],
        evaluated_args,
    )?;
    let class_name = eval_reflection_class_target_name(args[0], context, values)?;
    let requested_method_name = eval_reflection_string_arg(args[1], values)?;
    eval_reflection_method_object_result_or_throw(
        &class_name,
        &requested_method_name,
        context,
        values,
    )
}

/// Builds an eval-backed `ReflectionProperty` object when the reflected property exists in eval.
pub(super) fn eval_reflection_property_new(
    evaluated_args: Vec<EvaluatedCallArg>,
    context: &mut ElephcEvalContext,
    values: &mut impl RuntimeValueOps,
) -> Result<Option<RuntimeCellHandle>, EvalStatus> {
    let args = bind_evaluated_function_args(
        &[String::from("class_name"), String::from("property_name")],
        evaluated_args,
    )?;
    let property_name = eval_reflection_string_arg(args[1], values)?;
    if values.type_tag(args[0])? == EVAL_TAG_OBJECT {
        return eval_reflection_property_new_for_object(args[0], &property_name, context, values);
    }
    let class_name = eval_reflection_string_arg(args[0], values)?;
    eval_reflection_property_object_result_or_throw(&class_name, &property_name, context, values)
}

/// Builds a `ReflectionProperty` object when the reflected property exists.
pub(super) fn eval_reflection_property_object_result_if_exists(
    class_name: &str,
    property_name: &str,
    context: &mut ElephcEvalContext,
    values: &mut impl RuntimeValueOps,
) -> Result<Option<RuntimeCellHandle>, EvalStatus> {
    let reflected_name = context
        .resolve_class_like_name(class_name)
        .unwrap_or_else(|| class_name.trim_start_matches('\\').to_string());
    if !eval_reflection_class_like_exists(&reflected_name, context) {
        if let Some((declaring_class, property)) =
            eval_reflection_native_interface_property_requirement(
                &reflected_name,
                property_name,
                context,
            )
        {
            let property = eval_reflection_interface_property_metadata(declaring_class, &property);
            return eval_reflection_member_object_result(
                EVAL_REFLECTION_OWNER_PROPERTY,
                property_name,
                &property,
                context,
                values,
            )
            .map(Some);
        }
        let Some(property) = eval_reflection_aot_property_metadata_if_exists(
            &reflected_name,
            property_name,
            context,
            values,
        )?
        else {
            return Ok(None);
        };
        return eval_reflection_member_object_result(
            EVAL_REFLECTION_OWNER_PROPERTY,
            property_name,
            &property,
            context,
            values,
        )
        .map(Some);
    }
    let Some(property) = eval_reflection_property_metadata(&reflected_name, property_name, context)
    else {
        return Ok(None);
    };
    eval_reflection_member_object_result(
        EVAL_REFLECTION_OWNER_PROPERTY,
        property_name,
        &property,
        context,
        values,
    )
    .map(Some)
}

/// Builds a `ReflectionProperty` object or throws PHP's catchable reflection error.
pub(super) fn eval_reflection_property_object_result_or_throw(
    class_name: &str,
    property_name: &str,
    context: &mut ElephcEvalContext,
    values: &mut impl RuntimeValueOps,
) -> Result<Option<RuntimeCellHandle>, EvalStatus> {
    if let Some(result) = eval_reflection_property_object_result_if_exists(
        class_name,
        property_name,
        context,
        values,
    )? {
        return Ok(Some(result));
    }
    let reflected_name = context
        .resolve_class_like_name(class_name)
        .unwrap_or_else(|| class_name.trim_start_matches('\\').to_string());
    if !eval_reflection_class_like_or_runtime_exists(&reflected_name, context, values)? {
        return eval_throw_reflection_exception(
            &format!("Class \"{}\" does not exist", reflected_name),
            context,
            values,
        );
    }
    let message = eval_reflection_missing_member_message(
        EVAL_REFLECTION_OWNER_PROPERTY,
        &reflected_name,
        property_name,
    );
    eval_throw_reflection_exception(&message, context, values)
}

/// Builds a ReflectionProperty from an object argument, including dynamic properties.
pub(super) fn eval_reflection_property_new_for_object(
    object: RuntimeCellHandle,
    property_name: &str,
    context: &mut ElephcEvalContext,
    values: &mut impl RuntimeValueOps,
) -> Result<Option<RuntimeCellHandle>, EvalStatus> {
    let class_name = eval_reflection_object_class_name(object, context, values)?;
    if let Some(property) = eval_reflection_property_metadata(&class_name, property_name, context) {
        return eval_reflection_member_object_result(
            EVAL_REFLECTION_OWNER_PROPERTY,
            property_name,
            &property,
            context,
            values,
        )
        .map(Some);
    }
    if !eval_reflection_object_dynamic_property_exists(object, property_name, values)? {
        let message = eval_reflection_missing_member_message(
            EVAL_REFLECTION_OWNER_PROPERTY,
            &class_name,
            property_name,
        );
        return eval_throw_reflection_exception(&message, context, values);
    }
    let property = eval_reflection_dynamic_property_metadata(&class_name);
    eval_reflection_member_object_result(
        EVAL_REFLECTION_OWNER_PROPERTY,
        property_name,
        &property,
        context,
        values,
    )
    .map(Some)
}

/// Returns the class name for an object passed to a Reflection constructor.
pub(super) fn eval_reflection_object_class_name(
    object: RuntimeCellHandle,
    context: &ElephcEvalContext,
    values: &mut impl RuntimeValueOps,
) -> Result<String, EvalStatus> {
    let identity = values.object_identity(object)?;
    if let Some(class_name) = context.dynamic_object_class_name(identity) {
        return Ok(class_name);
    }
    let class_name = values.object_class_name(object)?;
    let bytes = values.string_bytes(class_name);
    values.release(class_name)?;
    let class_name = String::from_utf8(bytes?).map_err(|_| EvalStatus::RuntimeFatal)?;
    Ok(class_name.trim_start_matches('\\').to_string())
}

/// Returns whether one object has a public dynamic property by exact PHP name.
pub(super) fn eval_reflection_object_dynamic_property_exists(
    object: RuntimeCellHandle,
    property_name: &str,
    values: &mut impl RuntimeValueOps,
) -> Result<bool, EvalStatus> {
    if property_name.contains('\0') {
        return Ok(false);
    }
    let property_count = values.object_property_len(object)?;
    for position in 0..property_count {
        let key = values.object_property_iter_key(object, position)?;
        let key_bytes = values.string_bytes(key);
        values.release(key)?;
        if key_bytes? == property_name.as_bytes() {
            return Ok(true);
        }
    }
    Ok(false)
}

/// Returns the object captured by a `ReflectionObject` instance, when present.
pub(super) fn eval_reflection_object_reflected_object(
    reflection_object: RuntimeCellHandle,
    context: &mut ElephcEvalContext,
    values: &mut impl RuntimeValueOps,
) -> Result<Option<RuntimeCellHandle>, EvalStatus> {
    if !eval_reflection_object_has_class(reflection_object, "ReflectionObject", values)? {
        return Ok(None);
    }
    let object = eval_reflection_with_declaring_class_scope("ReflectionObject", context, |_| {
        values.property_get(reflection_object, "__object")
    })?;
    if values.type_tag(object)? == EVAL_TAG_OBJECT {
        Ok(Some(object))
    } else {
        Ok(None)
    }
}

/// Appends dynamic public properties to `ReflectionObject::getProperties()` results.
pub(super) fn eval_reflection_object_dynamic_property_array_result(
    reflection_object: RuntimeCellHandle,
    owner_kind: u64,
    reflected_name: &str,
    filter: Option<u64>,
    mut result: RuntimeCellHandle,
    context: &mut ElephcEvalContext,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    if owner_kind != EVAL_REFLECTION_OWNER_PROPERTY {
        return Ok(result);
    }
    let Some(object) = eval_reflection_object_reflected_object(reflection_object, context, values)?
    else {
        return Ok(result);
    };
    let property_names =
        eval_reflection_object_dynamic_property_names(object, reflected_name, context, values);
    values.release(object)?;
    let property_names = property_names?;
    for name in property_names {
        let member = eval_reflection_dynamic_property_metadata(reflected_name);
        if !eval_reflection_member_matches_filter(&member, filter) {
            continue;
        }
        let member_object = eval_reflection_member_object_result(
            EVAL_REFLECTION_OWNER_PROPERTY,
            &name,
            &member,
            context,
            values,
        )?;
        let next_index = values.array_len(result)? as i64;
        let key = values.int(next_index)?;
        result = values.array_set(result, key, member_object)?;
    }
    Ok(result)
}

/// Enumerates public dynamic property names on the object behind `ReflectionObject`.
pub(super) fn eval_reflection_object_dynamic_property_names(
    object: RuntimeCellHandle,
    reflected_name: &str,
    context: &ElephcEvalContext,
    values: &mut impl RuntimeValueOps,
) -> Result<Vec<String>, EvalStatus> {
    let mut names = Vec::new();
    let property_count = values.object_property_len(object)?;
    for position in 0..property_count {
        let key = values.object_property_iter_key(object, position)?;
        let key_bytes = values.string_bytes(key);
        values.release(key)?;
        let property_name =
            String::from_utf8(key_bytes?).map_err(|_| EvalStatus::RuntimeFatal)?;
        if !eval_reflection_dynamic_property_name_is_visible(
            reflected_name,
            &property_name,
            context,
        ) {
            continue;
        }
        if !names.iter().any(|name| name == &property_name) {
            names.push(property_name);
        }
    }
    Ok(names)
}

/// Returns true when an object-storage name represents a public dynamic property.
pub(super) fn eval_reflection_dynamic_property_name_is_visible(
    reflected_name: &str,
    property_name: &str,
    context: &ElephcEvalContext,
) -> bool {
    !property_name.contains('\0')
        && eval_reflection_member_name(
            EVAL_REFLECTION_OWNER_PROPERTY,
            reflected_name,
            property_name,
            context,
        )
        .is_none()
}

/// Builds PHP reflection metadata for a public dynamic object property.
pub(super) fn eval_reflection_dynamic_property_metadata(class_name: &str) -> EvalReflectionMemberMetadata {
    EvalReflectionMemberMetadata {
        declaring_class_name: Some(class_name.trim_start_matches('\\').to_string()),
        source_file: None,
        source_location: None,
        attributes: Vec::new(),
        visibility: EvalVisibility::Public,
        is_static: false,
        is_final: false,
        is_abstract: false,
        is_readonly: false,
        is_promoted: false,
        is_dynamic: true,
        modifiers: eval_reflection_property_modifiers(
            EvalVisibility::Public,
            None,
            false,
            false,
            false,
            false,
            false,
        ),
        type_metadata: None,
        settable_type_metadata: None,
        return_type_metadata: None,
        default_value: None,
        default_value_trait_origin: None,
        required_parameter_count: 0,
        parameters: Vec::new(),
    }
}

/// Returns generated AOT ReflectionMethod metadata when the runtime table has a matching row.
pub(super) fn eval_reflection_aot_method_metadata_if_exists(
    class_name: &str,
    method_name: &str,
    values: &mut impl RuntimeValueOps,
) -> Result<Option<EvalReflectionMemberMetadata>, EvalStatus> {
    let runtime_class_name = class_name.trim_start_matches('\\');
    let Some(flags) = values.reflection_method_flags(runtime_class_name, method_name)? else {
        return Ok(None);
    };
    let declaring_class_name = values
        .reflection_method_declaring_class(runtime_class_name, method_name)?
        .unwrap_or_else(|| runtime_class_name.to_string());
    Ok(Some(eval_reflection_aot_method_metadata(
        &declaring_class_name,
        method_name,
        flags,
        Vec::new(),
        None,
        None,
        None,
    )))
}

/// Returns generated AOT ReflectionMethod metadata with registered signature details.
pub(super) fn eval_reflection_aot_method_metadata_with_signature_if_exists(
    class_name: &str,
    method_name: &str,
    context: &ElephcEvalContext,
    values: &mut impl RuntimeValueOps,
) -> Result<Option<EvalReflectionMemberMetadata>, EvalStatus> {
    let runtime_class_name = class_name.trim_start_matches('\\');
    let Some(flags) = values.reflection_method_flags(runtime_class_name, method_name)? else {
        return Ok(None);
    };
    let declaring_class_name = values
        .reflection_method_declaring_class(runtime_class_name, method_name)?
        .unwrap_or_else(|| runtime_class_name.to_string());
    let mut signature =
        eval_reflection_aot_method_signature(&declaring_class_name, method_name, flags, context);
    if signature.is_none() && declaring_class_name != runtime_class_name {
        signature =
            eval_reflection_aot_method_signature(runtime_class_name, method_name, flags, context);
    }
    let attributes = eval_reflection_aot_method_attributes(
        runtime_class_name,
        &declaring_class_name,
        method_name,
        context,
    );
    let (source_file, source_location) =
        eval_reflection_aot_method_source_metadata(flags, values)?;
    Ok(Some(eval_reflection_aot_method_metadata(
        &declaring_class_name,
        method_name,
        flags,
        attributes,
        source_file,
        source_location,
        signature.as_ref(),
    )))
}

/// Returns AOT source-file and line metadata encoded in ReflectionMethod flags.
pub(super) fn eval_reflection_aot_method_source_metadata(
    flags: u64,
    values: &mut impl RuntimeValueOps,
) -> Result<(Option<String>, Option<EvalSourceLocation>), EvalStatus> {
    let Some(source_location) = eval_reflection_aot_method_source_location_from_flags(flags) else {
        return Ok((None, None));
    };
    let Some(source_file) = values.reflection_source_file()? else {
        return Ok((None, None));
    };
    Ok((Some(source_file), Some(source_location)))
}

/// Decodes AOT ReflectionMethod source lines packed into high flag bits.
pub(super) fn eval_reflection_aot_method_source_location_from_flags(
    flags: u64,
) -> Option<EvalSourceLocation> {
    let start_line =
        ((flags >> EVAL_REFLECTION_METHOD_SOURCE_START_SHIFT)
            & EVAL_REFLECTION_METHOD_SOURCE_LINE_MASK) as i64;
    let end_line =
        ((flags >> EVAL_REFLECTION_METHOD_SOURCE_END_SHIFT)
            & EVAL_REFLECTION_METHOD_SOURCE_LINE_MASK) as i64;
    (start_line > 0 && end_line >= start_line)
        .then(|| EvalSourceLocation::new(start_line, end_line))
}

/// Returns generated/AOT method dispatch metadata for interpreter-only runtime decisions.
pub(in crate::interpreter) fn eval_aot_method_dispatch_metadata(
    class_name: &str,
    method_name: &str,
    values: &mut impl RuntimeValueOps,
) -> Result<Option<(String, EvalVisibility, bool, bool)>, EvalStatus> {
    Ok(
        eval_reflection_aot_method_metadata_if_exists(class_name, method_name, values)?.map(
            |member| {
                (
                    member
                        .declaring_class_name
                        .unwrap_or_else(|| class_name.trim_start_matches('\\').to_string()),
                    member.visibility,
                    member.is_static,
                    member.is_abstract,
                )
            },
        ),
    )
}

/// Converts AOT method flag metadata into the eval ReflectionMethod shape.
pub(super) fn eval_reflection_aot_method_metadata(
    class_name: &str,
    method_name: &str,
    flags: u64,
    attributes: Vec<EvalAttribute>,
    source_file: Option<String>,
    source_location: Option<EvalSourceLocation>,
    signature: Option<&NativeCallableSignature>,
) -> EvalReflectionMemberMetadata {
    let visibility = if flags & EVAL_REFLECTION_MEMBER_FLAG_PRIVATE != 0 {
        EvalVisibility::Private
    } else if flags & EVAL_REFLECTION_MEMBER_FLAG_PROTECTED != 0 {
        EvalVisibility::Protected
    } else {
        EvalVisibility::Public
    };
    let required_parameter_count =
        signature.map_or(0, NativeCallableSignature::required_param_count);
    let parameters = signature.map_or_else(Vec::new, |signature| {
        eval_reflection_native_callable_parameters(class_name, method_name, flags, signature)
    });
    let return_type_metadata = signature
        .and_then(NativeCallableSignature::return_type)
        .and_then(eval_reflection_parameter_type_metadata);
    EvalReflectionMemberMetadata {
        declaring_class_name: Some(class_name.trim_start_matches('\\').to_string()),
        source_file,
        source_location,
        attributes,
        visibility,
        is_static: flags & EVAL_REFLECTION_MEMBER_FLAG_STATIC != 0,
        is_final: flags & EVAL_REFLECTION_MEMBER_FLAG_FINAL != 0,
        is_abstract: flags & EVAL_REFLECTION_MEMBER_FLAG_ABSTRACT != 0,
        is_readonly: false,
        is_promoted: false,
        is_dynamic: false,
        modifiers: eval_reflection_method_modifiers_from_flags(flags),
        type_metadata: None,
        settable_type_metadata: None,
        return_type_metadata,
        default_value: None,
        default_value_trait_origin: None,
        required_parameter_count,
        parameters,
    }
}

/// Returns registered generated/AOT method attributes for one reflected method.
pub(super) fn eval_reflection_aot_method_attributes(
    runtime_class_name: &str,
    declaring_class_name: &str,
    method_name: &str,
    context: &ElephcEvalContext,
) -> Vec<EvalAttribute> {
    let attributes = context.native_method_attributes(declaring_class_name, method_name);
    if !attributes.is_empty() || declaring_class_name == runtime_class_name {
        return attributes;
    }
    context.native_method_attributes(runtime_class_name, method_name)
}

/// Selects the registered native signature for an AOT method-like member.
pub(super) fn eval_reflection_aot_method_signature(
    class_name: &str,
    method_name: &str,
    flags: u64,
    context: &ElephcEvalContext,
) -> Option<NativeCallableSignature> {
    if method_name.eq_ignore_ascii_case("__construct") {
        return context.native_constructor_signature(class_name);
    }
    if flags & EVAL_REFLECTION_MEMBER_FLAG_STATIC != 0 {
        context.native_static_method_signature(class_name, method_name)
    } else {
        context.native_method_signature(class_name, method_name)
    }
}

/// Builds ReflectionParameter metadata for one registered native AOT signature.
pub(super) fn eval_reflection_native_callable_parameters(
    declaring_class_name: &str,
    method_name: &str,
    flags: u64,
    signature: &NativeCallableSignature,
) -> Vec<EvalReflectionParameterMetadata> {
    let names = eval_reflection_native_callable_parameter_names(signature);
    let parameter_count = names.len();
    let parameter_types = eval_reflection_native_callable_parameter_types(signature);
    let has_type_flags = parameter_types
        .iter()
        .map(Option::is_some)
        .collect::<Vec<_>>();
    let parameter_attributes = vec![Vec::new(); parameter_count];
    let defaults = eval_reflection_native_callable_parameter_defaults(signature);
    let by_ref_flags = (0..parameter_count)
        .map(|index| signature.param_by_ref(index))
        .collect::<Vec<_>>();
    let variadic_flags = (0..parameter_count)
        .map(|index| signature.param_variadic(index))
        .collect::<Vec<_>>();
    let declaring_function = EvalReflectionDeclaringFunctionMetadata {
        name: method_name.to_ascii_lowercase(),
        declaring_class_name: Some(declaring_class_name.trim_start_matches('\\').to_string()),
        magic_scope: None,
        attributes: Vec::new(),
        flags,
        required_parameter_count: signature.required_param_count(),
    };
    eval_reflection_parameters_from_names_and_type_flags(
        Some(declaring_class_name.trim_start_matches('\\')),
        Some(&declaring_function),
        &names,
        &has_type_flags,
        &parameter_types,
        &parameter_attributes,
        &defaults,
        &by_ref_flags,
        &variadic_flags,
        &[],
    )
}

/// Returns declared parameter type metadata for a registered native callable.
pub(super) fn eval_reflection_native_callable_parameter_types(
    signature: &NativeCallableSignature,
) -> Vec<Option<EvalParameterType>> {
    (0..signature.param_count())
        .map(|index| signature.param_type(index).cloned())
        .collect()
}

/// Returns parameter names for a registered native callable, filling missing bridge names.
pub(super) fn eval_reflection_native_callable_parameter_names(
    signature: &NativeCallableSignature,
) -> Vec<String> {
    (0..signature.param_count())
        .map(|index| {
            signature
                .param_names()
                .get(index)
                .filter(|name| !name.is_empty())
                .cloned()
                .unwrap_or_else(|| format!("arg{}", index))
        })
        .collect()
}

/// Converts registered scalar native defaults into eval constant expressions.
pub(super) fn eval_reflection_native_callable_parameter_defaults(
    signature: &NativeCallableSignature,
) -> Vec<Option<EvalExpr>> {
    (0..signature.param_count())
        .map(|index| {
            signature
                .param_default(index)
                .map(eval_reflection_native_callable_default_expr)
        })
        .collect()
}

/// Converts one registered native default into an eval constant expression.
pub(super) fn eval_reflection_native_callable_default_expr(default: &NativeCallableDefault) -> EvalExpr {
    match default {
        NativeCallableDefault::Null => EvalExpr::Const(EvalConst::Null),
        NativeCallableDefault::Bool(value) => EvalExpr::Const(EvalConst::Bool(*value)),
        NativeCallableDefault::Int(value) => EvalExpr::Const(EvalConst::Int(*value)),
        NativeCallableDefault::Float(value) => EvalExpr::Const(EvalConst::Float(*value)),
        NativeCallableDefault::String(value) => EvalExpr::Const(EvalConst::String(value.clone())),
        NativeCallableDefault::EmptyArray => EvalExpr::Array(Vec::new()),
        NativeCallableDefault::Array(elements) => EvalExpr::Array(
            elements
                .iter()
                .map(eval_reflection_native_callable_default_array_element)
                .collect(),
        ),
        NativeCallableDefault::Object { class_name, args } => EvalExpr::NewObject {
            class_name: class_name.clone(),
            args: args
                .iter()
                .map(eval_reflection_native_callable_default_arg)
                .collect(),
        },
    }
}

/// Converts one native array-default element into an eval array literal element.
pub(super) fn eval_reflection_native_callable_default_array_element(
    element: &NativeCallableArrayDefaultElement,
) -> EvalArrayElement {
    let value = eval_reflection_native_callable_default_expr(&element.value);
    match &element.key {
        Some(NativeCallableArrayDefaultKey::Int(key)) => EvalArrayElement::KeyValue {
            key: EvalExpr::Const(EvalConst::Int(*key)),
            value,
        },
        Some(NativeCallableArrayDefaultKey::String(key)) => EvalArrayElement::KeyValue {
            key: EvalExpr::Const(EvalConst::String(key.clone())),
            value,
        },
        None => EvalArrayElement::Value(value),
    }
}

/// Converts one native object-default constructor argument into an eval call arg.
pub(super) fn eval_reflection_native_callable_default_arg(arg: &NativeCallableObjectDefaultArg) -> EvalCallArg {
    let value = eval_reflection_native_callable_default_expr(&arg.value);
    match &arg.name {
        Some(name) => EvalCallArg::named(name, value),
        None => EvalCallArg::positional(value),
    }
}

/// Returns generated AOT ReflectionProperty metadata when the runtime table has a matching row.
pub(super) fn eval_reflection_aot_property_metadata_if_exists(
    class_name: &str,
    property_name: &str,
    context: &ElephcEvalContext,
    values: &mut impl RuntimeValueOps,
) -> Result<Option<EvalReflectionMemberMetadata>, EvalStatus> {
    let runtime_class_name = class_name.trim_start_matches('\\');
    let Some(flags) = values.reflection_property_flags(runtime_class_name, property_name)? else {
        return Ok(None);
    };
    let declaring_class_name = values
        .reflection_property_declaring_class(runtime_class_name, property_name)?
        .unwrap_or_else(|| runtime_class_name.to_string());
    let type_metadata = eval_reflection_aot_property_type_metadata(
        runtime_class_name,
        &declaring_class_name,
        property_name,
        context,
    );
    let default_value = eval_reflection_aot_property_default_value(
        runtime_class_name,
        &declaring_class_name,
        property_name,
        context,
    );
    let attributes = eval_reflection_aot_property_attributes(
        runtime_class_name,
        &declaring_class_name,
        property_name,
        context,
    );
    Ok(Some(eval_reflection_aot_property_metadata(
        &declaring_class_name,
        flags,
        attributes,
        type_metadata,
        default_value,
    )))
}

/// Returns generated/AOT property access metadata for an exact class/property pair.
pub(in crate::interpreter) fn eval_reflection_aot_property_access_metadata(
    class_name: &str,
    property_name: &str,
    values: &mut impl RuntimeValueOps,
) -> Result<Option<(String, EvalVisibility, EvalVisibility, bool)>, EvalStatus> {
    let runtime_class_name = class_name.trim_start_matches('\\');
    let Some(flags) = values.reflection_property_flags(runtime_class_name, property_name)? else {
        return Ok(None);
    };
    let declaring_class = values
        .reflection_property_declaring_class(runtime_class_name, property_name)?
        .unwrap_or_else(|| runtime_class_name.to_string());
    Ok(Some(eval_reflection_aot_property_access_metadata_from_flags(
        declaring_class,
        flags,
    )))
}

/// Returns generated/AOT static property metadata from a class or native parent chain.
pub(in crate::interpreter) fn eval_reflection_aot_static_property_access_metadata(
    class_name: &str,
    property_name: &str,
    context: &ElephcEvalContext,
    values: &mut impl RuntimeValueOps,
) -> Result<Option<(String, EvalVisibility, EvalVisibility, bool)>, EvalStatus> {
    let mut current = class_name.trim_start_matches('\\').to_string();
    let mut seen = std::collections::HashSet::new();
    loop {
        if !seen.insert(current.to_ascii_lowercase()) {
            return Ok(None);
        }
        if let Some(metadata) =
            eval_reflection_aot_property_access_metadata(&current, property_name, values)?
        {
            return Ok(Some(metadata));
        }
        let Some(parent) = context.native_class_parent(&current) else {
            return Ok(None);
        };
        current = parent.to_string();
    }
}

/// Converts AOT ReflectionProperty flags into access-check metadata.
pub(super) fn eval_reflection_aot_property_access_metadata_from_flags(
    declaring_class: String,
    flags: u64,
) -> (String, EvalVisibility, EvalVisibility, bool) {
    let visibility = if flags & EVAL_REFLECTION_MEMBER_FLAG_PRIVATE != 0 {
        EvalVisibility::Private
    } else if flags & EVAL_REFLECTION_MEMBER_FLAG_PROTECTED != 0 {
        EvalVisibility::Protected
    } else {
        EvalVisibility::Public
    };
    let write_visibility = if flags & EVAL_REFLECTION_MEMBER_FLAG_PRIVATE_SET != 0 {
        EvalVisibility::Private
    } else if flags & EVAL_REFLECTION_MEMBER_FLAG_PROTECTED_SET != 0 {
        EvalVisibility::Protected
    } else {
        visibility
    };
    (
        declaring_class,
        visibility,
        write_visibility,
        flags & EVAL_REFLECTION_MEMBER_FLAG_STATIC != 0,
    )
}

/// Returns registered generated/AOT property type metadata for one reflected property.
pub(super) fn eval_reflection_aot_property_type_metadata(
    runtime_class_name: &str,
    declaring_class_name: &str,
    property_name: &str,
    context: &ElephcEvalContext,
) -> Option<EvalReflectionParameterTypeMetadata> {
    context
        .native_property_type(declaring_class_name, property_name)
        .or_else(|| context.native_property_type(runtime_class_name, property_name))
        .as_ref()
        .and_then(eval_reflection_parameter_type_metadata)
}

/// Returns registered generated/AOT property default metadata for one reflected property.
pub(super) fn eval_reflection_aot_property_default_value(
    runtime_class_name: &str,
    declaring_class_name: &str,
    property_name: &str,
    context: &ElephcEvalContext,
) -> Option<EvalExpr> {
    context
        .native_property_default(declaring_class_name, property_name)
        .or_else(|| context.native_property_default(runtime_class_name, property_name))
        .as_ref()
        .map(eval_reflection_native_callable_default_expr)
}

/// Returns registered generated/AOT property attributes for one reflected property.
pub(super) fn eval_reflection_aot_property_attributes(
    runtime_class_name: &str,
    declaring_class_name: &str,
    property_name: &str,
    context: &ElephcEvalContext,
) -> Vec<EvalAttribute> {
    let attributes = context.native_property_attributes(declaring_class_name, property_name);
    if !attributes.is_empty() || declaring_class_name == runtime_class_name {
        return attributes;
    }
    context.native_property_attributes(runtime_class_name, property_name)
}

/// Converts AOT property flag metadata into the eval ReflectionProperty shape.
pub(super) fn eval_reflection_aot_property_metadata(
    class_name: &str,
    flags: u64,
    attributes: Vec<EvalAttribute>,
    type_metadata: Option<EvalReflectionParameterTypeMetadata>,
    default_value: Option<EvalExpr>,
) -> EvalReflectionMemberMetadata {
    let visibility = if flags & EVAL_REFLECTION_MEMBER_FLAG_PRIVATE != 0 {
        EvalVisibility::Private
    } else if flags & EVAL_REFLECTION_MEMBER_FLAG_PROTECTED != 0 {
        EvalVisibility::Protected
    } else {
        EvalVisibility::Public
    };
    let is_static = flags & EVAL_REFLECTION_MEMBER_FLAG_STATIC != 0;
    let is_final = flags & EVAL_REFLECTION_MEMBER_FLAG_FINAL != 0;
    let is_abstract = flags & EVAL_REFLECTION_MEMBER_FLAG_ABSTRACT != 0;
    let is_readonly = flags & EVAL_REFLECTION_MEMBER_FLAG_READONLY != 0;
    let is_virtual = flags & EVAL_REFLECTION_MEMBER_FLAG_VIRTUAL != 0;
    let mut modifiers = eval_reflection_property_modifiers(
        visibility,
        None,
        is_static,
        is_final,
        is_abstract,
        is_readonly,
        is_virtual,
    );
    if flags & EVAL_REFLECTION_MEMBER_FLAG_PRIVATE_SET != 0 {
        modifiers |= 32 | 4096;
    } else if flags & EVAL_REFLECTION_MEMBER_FLAG_PROTECTED_SET != 0 {
        modifiers |= 2048;
    }
    let settable_type_metadata = type_metadata.clone();
    EvalReflectionMemberMetadata {
        declaring_class_name: Some(class_name.trim_start_matches('\\').to_string()),
        source_file: None,
        source_location: None,
        attributes,
        visibility,
        is_static,
        is_final,
        is_abstract,
        is_readonly,
        is_promoted: flags & EVAL_REFLECTION_MEMBER_FLAG_PROMOTED != 0,
        is_dynamic: false,
        modifiers,
        type_metadata,
        settable_type_metadata,
        return_type_metadata: None,
        default_value,
        default_value_trait_origin: None,
        required_parameter_count: 0,
        parameters: Vec::new(),
    }
}
