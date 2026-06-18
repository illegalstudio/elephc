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

const EVAL_REFLECTION_CLASS_FLAG_FINAL: u64 = 1;
const EVAL_REFLECTION_CLASS_FLAG_ABSTRACT: u64 = 2;

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
        Some(EVAL_REFLECTION_OWNER_CLASS_CONSTANT) => {
            eval_reflection_class_constant_new(evaluated_args, context, values)
        }
        Some(EVAL_REFLECTION_OWNER_ENUM_UNIT_CASE) => eval_reflection_enum_case_new(
            EVAL_REFLECTION_OWNER_ENUM_UNIT_CASE,
            evaluated_args,
            context,
            values,
        ),
        Some(EVAL_REFLECTION_OWNER_ENUM_BACKED_CASE) => eval_reflection_enum_case_new(
            EVAL_REFLECTION_OWNER_ENUM_BACKED_CASE,
            evaluated_args,
            context,
            values,
        ),
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
    let Some((resolved_name, attributes, flags)) =
        eval_reflection_class_like_attributes(&class_name, context)
    else {
        return Ok(None);
    };
    eval_reflection_owner_object(
        EVAL_REFLECTION_OWNER_CLASS,
        &resolved_name,
        &attributes,
        flags,
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
        &method_name,
        &attributes,
        0,
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
        &property_name,
        &attributes,
        0,
        context,
        values,
    )
    .map(Some)
}

/// Builds an eval-backed `ReflectionClassConstant` object for a class constant or enum case.
fn eval_reflection_class_constant_new(
    evaluated_args: Vec<EvaluatedCallArg>,
    context: &mut ElephcEvalContext,
    values: &mut impl RuntimeValueOps,
) -> Result<Option<RuntimeCellHandle>, EvalStatus> {
    let args = bind_evaluated_function_args(
        &[String::from("class_name"), String::from("constant_name")],
        evaluated_args,
    )?;
    let class_name = eval_reflection_string_arg(args[0], values)?;
    if !eval_reflection_class_like_exists(&class_name, context) {
        return Ok(None);
    }
    let constant_name = eval_reflection_string_arg(args[1], values)?;
    let attributes =
        eval_reflection_class_constant_attributes(&class_name, &constant_name, context)
            .ok_or(EvalStatus::RuntimeFatal)?;
    eval_reflection_owner_object(
        EVAL_REFLECTION_OWNER_CLASS_CONSTANT,
        &constant_name,
        &attributes,
        0,
        context,
        values,
    )
    .map(Some)
}

/// Builds an eval-backed ReflectionEnumUnitCase/BackedCase object for an enum case.
fn eval_reflection_enum_case_new(
    owner_kind: u64,
    evaluated_args: Vec<EvaluatedCallArg>,
    context: &mut ElephcEvalContext,
    values: &mut impl RuntimeValueOps,
) -> Result<Option<RuntimeCellHandle>, EvalStatus> {
    let args = bind_evaluated_function_args(
        &[String::from("class_name"), String::from("constant_name")],
        evaluated_args,
    )?;
    let enum_name = eval_reflection_string_arg(args[0], values)?;
    let Some(enum_decl) = context.enum_decl(&enum_name) else {
        return if eval_reflection_class_like_exists(&enum_name, context) {
            Err(EvalStatus::RuntimeFatal)
        } else {
            Ok(None)
        };
    };
    if owner_kind == EVAL_REFLECTION_OWNER_ENUM_BACKED_CASE && enum_decl.backing_type().is_none() {
        return Err(EvalStatus::RuntimeFatal);
    }
    let case_name = eval_reflection_string_arg(args[1], values)?;
    let attributes = enum_decl
        .case(&case_name)
        .map(|case| case.attributes().to_vec())
        .ok_or(EvalStatus::RuntimeFatal)?;
    eval_reflection_owner_object(owner_kind, &case_name, &attributes, 0, context, values).map(Some)
}

/// Materializes one Reflection owner object and transfers the temporary attribute array.
fn eval_reflection_owner_object(
    owner_kind: u64,
    reflected_name: &str,
    attributes: &[EvalAttribute],
    flags: u64,
    context: &mut ElephcEvalContext,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    let attrs = eval_reflection_attribute_array_result(attributes, context, values)?;
    let object = values.reflection_owner_new(owner_kind, reflected_name, attrs, flags)?;
    values.release(attrs)?;
    Ok(object)
}

/// Returns the eval-retained class-like attributes plus canonical reflected name.
fn eval_reflection_class_like_attributes(
    name: &str,
    context: &ElephcEvalContext,
) -> Option<(String, Vec<EvalAttribute>, u64)> {
    if let Some(class) = context.class(name) {
        let mut flags = 0;
        if class.is_final() {
            flags |= EVAL_REFLECTION_CLASS_FLAG_FINAL;
        }
        if class.is_abstract() {
            flags |= EVAL_REFLECTION_CLASS_FLAG_ABSTRACT;
        }
        return Some((
            class.name().trim_start_matches('\\').to_string(),
            class.attributes().to_vec(),
            flags,
        ));
    }
    if let Some(interface) = context.interface(name) {
        return Some((
            interface.name().trim_start_matches('\\').to_string(),
            interface.attributes().to_vec(),
            0,
        ));
    }
    if let Some(trait_decl) = context.trait_decl(name) {
        return Some((
            trait_decl.name().trim_start_matches('\\').to_string(),
            trait_decl.attributes().to_vec(),
            0,
        ));
    }
    context.enum_decl(name).map(|enum_decl| {
        (
            enum_decl.name().trim_start_matches('\\').to_string(),
            enum_decl.attributes().to_vec(),
            EVAL_REFLECTION_CLASS_FLAG_FINAL,
        )
    })
}

/// Returns attributes attached to an eval class constant or enum case.
fn eval_reflection_class_constant_attributes(
    class_name: &str,
    constant_name: &str,
    context: &ElephcEvalContext,
) -> Option<Vec<EvalAttribute>> {
    if let Some(enum_decl) = context.enum_decl(class_name) {
        if let Some(case) = enum_decl.case(constant_name) {
            return Some(case.attributes().to_vec());
        }
    }
    context
        .class_constant(class_name, constant_name)
        .map(|(_, constant)| constant.attributes().to_vec())
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
        "reflectionclassconstant" => Some(EVAL_REFLECTION_OWNER_CLASS_CONSTANT),
        "reflectionenumunitcase" => Some(EVAL_REFLECTION_OWNER_ENUM_UNIT_CASE),
        "reflectionenumbackedcase" => Some(EVAL_REFLECTION_OWNER_ENUM_BACKED_CASE),
        _ => None,
    }
}
