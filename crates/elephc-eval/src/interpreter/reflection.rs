//! Purpose:
//! Handles eval-aware construction of builtin reflection owner objects.
//! These objects need private metadata slots populated from eval-declared class
//! metadata, which ordinary public property writes cannot express.
//!
//! Called from:
//! - `crate::interpreter::expressions::eval_expr()` for `new Reflection*`.
//!
//! Key details:
//! - Only eval-declared classes/interfaces/traits/enums are handled here.
//! - Non-eval targets fall back to the generated AOT runtime bridge.

use super::*;

/// Attempts to construct a ReflectionClass/Method/Property object for eval metadata.
pub(in crate::interpreter) fn eval_reflection_owner_new_object(
    class_name: &str,
    evaluated_args: Vec<EvaluatedCallArg>,
    context: &mut ElephcEvalContext,
    values: &mut impl RuntimeValueOps,
) -> Result<Option<RuntimeCellHandle>, EvalStatus> {
    match reflection_owner_kind(class_name) {
        Some(EVAL_REFLECTION_OWNER_CLASS) => {
            eval_reflection_class_new(evaluated_args, context, values)
        }
        Some(EVAL_REFLECTION_OWNER_METHOD) => {
            eval_reflection_method_new(evaluated_args, context, values)
        }
        Some(EVAL_REFLECTION_OWNER_PROPERTY) => {
            eval_reflection_property_new(evaluated_args, context, values)
        }
        Some(_) => Err(EvalStatus::RuntimeFatal),
        None => Ok(None),
    }
}

/// Builds an eval-backed `ReflectionClass` object when the reflected class-like exists in eval.
fn eval_reflection_class_new(
    evaluated_args: Vec<EvaluatedCallArg>,
    context: &mut ElephcEvalContext,
    values: &mut impl RuntimeValueOps,
) -> Result<Option<RuntimeCellHandle>, EvalStatus> {
    let args = bind_evaluated_function_args(&[String::from("class_name")], evaluated_args)?;
    let class_name = eval_reflection_string_arg(args[0], values)?;
    let Some((resolved_name, attributes)) =
        eval_reflection_class_like_attributes(&class_name, context)
    else {
        return Ok(None);
    };
    eval_reflection_owner_object(
        EVAL_REFLECTION_OWNER_CLASS,
        &resolved_name,
        &attributes,
        context,
        values,
    )
    .map(Some)
}

/// Builds an eval-backed `ReflectionMethod` object when the reflected method exists in eval.
fn eval_reflection_method_new(
    evaluated_args: Vec<EvaluatedCallArg>,
    context: &mut ElephcEvalContext,
    values: &mut impl RuntimeValueOps,
) -> Result<Option<RuntimeCellHandle>, EvalStatus> {
    let args = bind_evaluated_function_args(
        &[String::from("class_name"), String::from("method_name")],
        evaluated_args,
    )?;
    let class_name = eval_reflection_string_arg(args[0], values)?;
    if !eval_reflection_class_like_exists(&class_name, context) {
        return Ok(None);
    }
    let method_name = eval_reflection_string_arg(args[1], values)?;
    let attributes = eval_reflection_method_attributes(&class_name, &method_name, context)
        .ok_or(EvalStatus::RuntimeFatal)?;
    eval_reflection_owner_object(
        EVAL_REFLECTION_OWNER_METHOD,
        "",
        &attributes,
        context,
        values,
    )
    .map(Some)
}

/// Builds an eval-backed `ReflectionProperty` object when the reflected property exists in eval.
fn eval_reflection_property_new(
    evaluated_args: Vec<EvaluatedCallArg>,
    context: &mut ElephcEvalContext,
    values: &mut impl RuntimeValueOps,
) -> Result<Option<RuntimeCellHandle>, EvalStatus> {
    let args = bind_evaluated_function_args(
        &[String::from("class_name"), String::from("property_name")],
        evaluated_args,
    )?;
    let class_name = eval_reflection_string_arg(args[0], values)?;
    if !eval_reflection_class_like_exists(&class_name, context) {
        return Ok(None);
    }
    let property_name = eval_reflection_string_arg(args[1], values)?;
    let attributes = eval_reflection_property_attributes(&class_name, &property_name, context)
        .ok_or(EvalStatus::RuntimeFatal)?;
    eval_reflection_owner_object(
        EVAL_REFLECTION_OWNER_PROPERTY,
        "",
        &attributes,
        context,
        values,
    )
    .map(Some)
}

/// Materializes one Reflection owner object and transfers the temporary attribute array.
fn eval_reflection_owner_object(
    owner_kind: u64,
    reflected_name: &str,
    attributes: &[EvalAttribute],
    context: &mut ElephcEvalContext,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    let attrs = eval_reflection_attribute_array_result(attributes, context, values)?;
    let object = values.reflection_owner_new(owner_kind, reflected_name, attrs)?;
    values.release(attrs)?;
    Ok(object)
}

/// Returns the eval-retained class-like attributes plus canonical reflected name.
fn eval_reflection_class_like_attributes(
    name: &str,
    context: &ElephcEvalContext,
) -> Option<(String, Vec<EvalAttribute>)> {
    if let Some(class) = context.class(name) {
        return Some((
            class.name().trim_start_matches('\\').to_string(),
            class.attributes().to_vec(),
        ));
    }
    if let Some(interface) = context.interface(name) {
        return Some((
            interface.name().trim_start_matches('\\').to_string(),
            interface.attributes().to_vec(),
        ));
    }
    if let Some(trait_decl) = context.trait_decl(name) {
        return Some((
            trait_decl.name().trim_start_matches('\\').to_string(),
            trait_decl.attributes().to_vec(),
        ));
    }
    context.enum_decl(name).map(|enum_decl| {
        (
            enum_decl.name().trim_start_matches('\\').to_string(),
            enum_decl.attributes().to_vec(),
        )
    })
}

/// Returns true when a name resolves to an eval-declared class-like symbol.
fn eval_reflection_class_like_exists(name: &str, context: &ElephcEvalContext) -> bool {
    context.has_class(name)
        || context.has_interface(name)
        || context.has_trait(name)
        || context.has_enum(name)
}

/// Returns attributes attached to a method-like member on an eval class-like symbol.
fn eval_reflection_method_attributes(
    class_name: &str,
    method_name: &str,
    context: &ElephcEvalContext,
) -> Option<Vec<EvalAttribute>> {
    if context.has_class(class_name) || context.has_enum(class_name) {
        return context
            .class_method(class_name, method_name)
            .map(|(_, method)| method.attributes().to_vec());
    }
    if context.has_interface(class_name) {
        return context
            .interface_method_requirements(class_name)
            .into_iter()
            .find(|method| method.name().eq_ignore_ascii_case(method_name))
            .map(|method| method.attributes().to_vec());
    }
    context.trait_decl(class_name).and_then(|trait_decl| {
        trait_decl
            .methods()
            .iter()
            .find(|method| method.name().eq_ignore_ascii_case(method_name))
            .map(|method| method.attributes().to_vec())
    })
}

/// Returns attributes attached to a property-like member on an eval class-like symbol.
fn eval_reflection_property_attributes(
    class_name: &str,
    property_name: &str,
    context: &ElephcEvalContext,
) -> Option<Vec<EvalAttribute>> {
    if context.has_class(class_name) || context.has_enum(class_name) {
        return context
            .class_property(class_name, property_name)
            .map(|(_, property)| property.attributes().to_vec());
    }
    if context.has_interface(class_name) {
        return context
            .interface_property_requirements(class_name)
            .into_iter()
            .find(|property| property.name() == property_name)
            .map(|property| property.attributes().to_vec());
    }
    context.trait_decl(class_name).and_then(|trait_decl| {
        trait_decl
            .properties()
            .iter()
            .find(|property| property.name() == property_name)
            .map(|property| property.attributes().to_vec())
    })
}

/// Converts one reflection constructor argument to a Rust UTF-8 string.
fn eval_reflection_string_arg(
    value: RuntimeCellHandle,
    values: &mut impl RuntimeValueOps,
) -> Result<String, EvalStatus> {
    let bytes = values.string_bytes(value)?;
    String::from_utf8(bytes).map_err(|_| EvalStatus::RuntimeFatal)
}

/// Maps a PHP reflection owner class name to the helper owner kind.
fn reflection_owner_kind(class_name: &str) -> Option<u64> {
    match class_name
        .trim_start_matches('\\')
        .to_ascii_lowercase()
        .as_str()
    {
        "reflectionclass" => Some(EVAL_REFLECTION_OWNER_CLASS),
        "reflectionmethod" => Some(EVAL_REFLECTION_OWNER_METHOD),
        "reflectionproperty" => Some(EVAL_REFLECTION_OWNER_PROPERTY),
        _ => None,
    }
}
