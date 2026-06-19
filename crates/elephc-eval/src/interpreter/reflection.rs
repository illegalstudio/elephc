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
const EVAL_REFLECTION_CLASS_FLAG_INTERFACE: u64 = 4;
const EVAL_REFLECTION_CLASS_FLAG_TRAIT: u64 = 8;
const EVAL_REFLECTION_CLASS_FLAG_ENUM: u64 = 16;
const EVAL_REFLECTION_CLASS_FLAG_READONLY: u64 = 32;
const EVAL_REFLECTION_MEMBER_FLAG_STATIC: u64 = 1;
const EVAL_REFLECTION_MEMBER_FLAG_PUBLIC: u64 = 2;
const EVAL_REFLECTION_MEMBER_FLAG_PROTECTED: u64 = 4;
const EVAL_REFLECTION_MEMBER_FLAG_PRIVATE: u64 = 8;
const EVAL_REFLECTION_MEMBER_FLAG_FINAL: u64 = 16;
const EVAL_REFLECTION_MEMBER_FLAG_ABSTRACT: u64 = 32;
const EVAL_REFLECTION_PARAMETER_FLAG_OPTIONAL: u64 = 1;
const EVAL_REFLECTION_PARAMETER_FLAG_VARIADIC: u64 = 2;
const EVAL_REFLECTION_PARAMETER_FLAG_BY_REF: u64 = 4;
const EVAL_REFLECTION_PARAMETER_FLAG_HAS_TYPE: u64 = 8;

/// Eval metadata needed to materialize one `ReflectionClass` owner object.
struct EvalReflectionClassMetadata {
    resolved_name: String,
    attributes: Vec<EvalAttribute>,
    flags: u64,
    modifiers: u64,
    interface_names: Vec<String>,
    trait_names: Vec<String>,
    method_names: Vec<String>,
    property_names: Vec<String>,
}

/// Eval metadata needed to materialize one `ReflectionMethod` or `ReflectionProperty` owner object.
struct EvalReflectionMemberMetadata {
    attributes: Vec<EvalAttribute>,
    visibility: EvalVisibility,
    is_static: bool,
    is_final: bool,
    is_abstract: bool,
    parameters: Vec<EvalReflectionParameterMetadata>,
}

/// Eval metadata needed to materialize one `ReflectionParameter` object.
struct EvalReflectionParameterMetadata {
    name: String,
    position: usize,
    is_optional: bool,
    is_variadic: bool,
    is_passed_by_reference: bool,
    has_type: bool,
}

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
    let Some(metadata) = eval_reflection_class_like_attributes(&class_name, context) else {
        return Ok(None);
    };
    eval_reflection_owner_object(
        EVAL_REFLECTION_OWNER_CLASS,
        &metadata.resolved_name,
        &metadata.attributes,
        &metadata.interface_names,
        &metadata.trait_names,
        &metadata.method_names,
        &metadata.property_names,
        &[],
        metadata.flags,
        metadata.modifiers,
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
    let method = eval_reflection_method_metadata(&class_name, &method_name, context)
        .ok_or(EvalStatus::RuntimeFatal)?;
    let flags = eval_reflection_member_flags(
        method.visibility,
        method.is_static,
        method.is_final,
        method.is_abstract,
    );
    eval_reflection_owner_object(
        EVAL_REFLECTION_OWNER_METHOD,
        &method_name,
        &method.attributes,
        &[],
        &[],
        &[],
        &[],
        &method.parameters,
        flags,
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
    let property = eval_reflection_property_metadata(&class_name, &property_name, context)
        .ok_or(EvalStatus::RuntimeFatal)?;
    let flags = eval_reflection_member_flags(property.visibility, property.is_static, false, false);
    eval_reflection_owner_object(
        EVAL_REFLECTION_OWNER_PROPERTY,
        &property_name,
        &property.attributes,
        &[],
        &[],
        &[],
        &[],
        &[],
        flags,
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
        &[],
        &[],
        &[],
        &[],
        &[],
        0,
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
    eval_reflection_owner_object(
        owner_kind,
        &case_name,
        &attributes,
        &[],
        &[],
        &[],
        &[],
        &[],
        0,
        0,
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
    interface_names: &[String],
    trait_names: &[String],
    method_names: &[String],
    property_names: &[String],
    parameter_metadata: &[EvalReflectionParameterMetadata],
    flags: u64,
    modifiers: u64,
    context: &mut ElephcEvalContext,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    let attrs = eval_reflection_attribute_array_result(attributes, context, values)?;
    let interface_names_array = eval_reflection_string_array_result(interface_names, values)?;
    let trait_names_array = eval_reflection_string_array_result(trait_names, values)?;
    let method_names_array = eval_reflection_string_array_result(method_names, values)?;
    let property_names_array = eval_reflection_string_array_result(property_names, values)?;
    let method_objects = if owner_kind == EVAL_REFLECTION_OWNER_CLASS {
        eval_reflection_member_object_array_result(
            EVAL_REFLECTION_OWNER_METHOD,
            reflected_name,
            &method_names,
            context,
            values,
        )?
    } else if owner_kind == EVAL_REFLECTION_OWNER_METHOD {
        eval_reflection_parameter_object_array_result(parameter_metadata, values)?
    } else {
        values.array_new(0)?
    };
    let property_objects = if owner_kind == EVAL_REFLECTION_OWNER_CLASS {
        eval_reflection_member_object_array_result(
            EVAL_REFLECTION_OWNER_PROPERTY,
            reflected_name,
            &property_names,
            context,
            values,
        )?
    } else {
        values.array_new(0)?
    };
    let object = values.reflection_owner_new(
        owner_kind,
        reflected_name,
        attrs,
        interface_names_array,
        trait_names_array,
        method_names_array,
        property_names_array,
        method_objects,
        property_objects,
        flags,
        modifiers,
    )?;
    if owner_kind == EVAL_REFLECTION_OWNER_CLASS {
        let identity = values.object_identity(object)?;
        context.register_eval_reflection_class(identity, reflected_name);
    }
    values.release(attrs)?;
    values.release(interface_names_array)?;
    values.release(trait_names_array)?;
    values.release(method_names_array)?;
    values.release(property_names_array)?;
    values.release(method_objects)?;
    values.release(property_objects)?;
    Ok(object)
}

/// Builds an indexed PHP string array for ReflectionClass metadata names.
fn eval_reflection_string_array_result(
    names: &[String],
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    let mut result = values.string_array_new(names.len())?;
    for name in names {
        result = values.string_array_push(result, name)?;
    }
    Ok(result)
}

/// Builds an indexed array of populated ReflectionParameter objects.
fn eval_reflection_parameter_object_array_result(
    parameters: &[EvalReflectionParameterMetadata],
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    let mut result = values.array_new(parameters.len())?;
    for parameter in parameters {
        let parameter_object = eval_reflection_parameter_object_result(parameter, values)?;
        let key = values.int(parameter.position as i64)?;
        result = values.array_set(result, key, parameter_object)?;
    }
    Ok(result)
}

/// Materializes one ReflectionParameter object through the shared reflection helper.
fn eval_reflection_parameter_object_result(
    parameter: &EvalReflectionParameterMetadata,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    let attrs = values.array_new(0)?;
    let interface_names = values.array_new(0)?;
    let trait_names = values.array_new(0)?;
    let method_names = values.array_new(0)?;
    let property_names = values.array_new(0)?;
    let method_objects = values.array_new(0)?;
    let property_objects = values.array_new(0)?;
    let flags = eval_reflection_parameter_flags(parameter);
    let object = values.reflection_owner_new(
        EVAL_REFLECTION_OWNER_PARAMETER,
        &parameter.name,
        attrs,
        interface_names,
        trait_names,
        method_names,
        property_names,
        method_objects,
        property_objects,
        flags,
        parameter.position as u64,
    )?;
    values.release(attrs)?;
    values.release(interface_names)?;
    values.release(trait_names)?;
    values.release(method_names)?;
    values.release(property_names)?;
    values.release(method_objects)?;
    values.release(property_objects)?;
    Ok(object)
}

/// Builds an indexed array of ReflectionMethod or ReflectionProperty objects for a ReflectionClass.
fn eval_reflection_member_object_array_result(
    owner_kind: u64,
    class_name: &str,
    names: &[String],
    context: &mut ElephcEvalContext,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    let mut result = values.array_new(names.len())?;
    let mut index = 0;
    for name in names {
        let Some(member) = eval_reflection_member_metadata(owner_kind, class_name, name, context)
        else {
            continue;
        };
        let flags = eval_reflection_member_flags(
            member.visibility,
            member.is_static,
            member.is_final,
            member.is_abstract,
        );
        let member_object = eval_reflection_owner_object(
            owner_kind,
            name,
            &member.attributes,
            &[],
            &[],
            &[],
            &[],
            &member.parameters,
            flags,
            0,
            context,
            values,
        )?;
        let key = values.int(index)?;
        result = values.array_set(result, key, member_object)?;
        index += 1;
    }
    Ok(result)
}

/// Returns member metadata for one ReflectionClass member-array entry.
fn eval_reflection_member_metadata(
    owner_kind: u64,
    class_name: &str,
    name: &str,
    context: &ElephcEvalContext,
) -> Option<EvalReflectionMemberMetadata> {
    match owner_kind {
        EVAL_REFLECTION_OWNER_METHOD => eval_reflection_method_metadata(class_name, name, context),
        EVAL_REFLECTION_OWNER_PROPERTY => {
            eval_reflection_property_metadata(class_name, name, context)
        }
        _ => None,
    }
}

/// Returns the eval-retained class-like attributes plus canonical reflected name.
fn eval_reflection_class_like_attributes(
    name: &str,
    context: &ElephcEvalContext,
) -> Option<EvalReflectionClassMetadata> {
    if let Some(class) = context.class(name) {
        let is_enum = context.has_enum(class.name());
        let mut flags = 0;
        if class.is_final() {
            flags |= EVAL_REFLECTION_CLASS_FLAG_FINAL;
        }
        if class.is_abstract() {
            flags |= EVAL_REFLECTION_CLASS_FLAG_ABSTRACT;
        }
        if is_enum {
            flags |= EVAL_REFLECTION_CLASS_FLAG_ENUM;
        }
        if class.is_readonly_class() && !is_enum {
            flags |= EVAL_REFLECTION_CLASS_FLAG_READONLY;
        }
        let modifiers = eval_reflection_class_modifiers(
            class.is_final(),
            class.is_abstract(),
            class.is_readonly_class(),
            is_enum,
        );
        return Some(EvalReflectionClassMetadata {
            resolved_name: class.name().trim_start_matches('\\').to_string(),
            attributes: class.attributes().to_vec(),
            interface_names: context.class_interface_names(class.name()),
            trait_names: context.class_trait_names(class.name()),
            method_names: context.class_method_names(class.name()),
            property_names: context.class_property_names(class.name()),
            flags,
            modifiers,
        });
    }
    if let Some(interface) = context.interface(name) {
        return Some(EvalReflectionClassMetadata {
            resolved_name: interface.name().trim_start_matches('\\').to_string(),
            attributes: interface.attributes().to_vec(),
            interface_names: context.interface_parent_names(interface.name()),
            trait_names: Vec::new(),
            method_names: context.interface_method_names(interface.name()),
            property_names: context.interface_property_names(interface.name()),
            flags: EVAL_REFLECTION_CLASS_FLAG_INTERFACE,
            modifiers: 0,
        });
    }
    if let Some(trait_decl) = context.trait_decl(name) {
        return Some(EvalReflectionClassMetadata {
            resolved_name: trait_decl.name().trim_start_matches('\\').to_string(),
            attributes: trait_decl.attributes().to_vec(),
            interface_names: Vec::new(),
            trait_names: Vec::new(),
            method_names: context.trait_method_names(trait_decl.name()),
            property_names: context.trait_property_names(trait_decl.name()),
            flags: EVAL_REFLECTION_CLASS_FLAG_TRAIT,
            modifiers: 0,
        });
    }
    context
        .enum_decl(name)
        .map(|enum_decl| EvalReflectionClassMetadata {
            resolved_name: enum_decl.name().trim_start_matches('\\').to_string(),
            attributes: enum_decl.attributes().to_vec(),
            interface_names: context.class_interface_names(enum_decl.name()),
            trait_names: Vec::new(),
            method_names: context.class_method_names(enum_decl.name()),
            property_names: context.class_property_names(enum_decl.name()),
            flags: EVAL_REFLECTION_CLASS_FLAG_FINAL | EVAL_REFLECTION_CLASS_FLAG_ENUM,
            modifiers: 32,
        })
}

/// Computes PHP's `ReflectionClass::getModifiers()` bitmask for eval metadata.
fn eval_reflection_class_modifiers(
    is_final: bool,
    is_abstract: bool,
    is_readonly_class: bool,
    is_enum: bool,
) -> u64 {
    let mut modifiers = 0;
    if is_final {
        modifiers |= 32;
    }
    if is_abstract {
        modifiers |= 64;
    }
    if is_readonly_class && !is_enum {
        modifiers |= 65_536;
    }
    modifiers
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

/// Returns method metadata for a method-like member on an eval class-like symbol.
fn eval_reflection_method_metadata(
    class_name: &str,
    method_name: &str,
    context: &ElephcEvalContext,
) -> Option<EvalReflectionMemberMetadata> {
    if context.has_class(class_name) || context.has_enum(class_name) {
        return context
            .class_method(class_name, method_name)
            .map(|(_, method)| EvalReflectionMemberMetadata {
                attributes: method.attributes().to_vec(),
                visibility: method.visibility(),
                is_static: method.is_static(),
                is_final: method.is_final(),
                is_abstract: method.is_abstract(),
                parameters: eval_reflection_parameters_from_names_and_type_flags(
                    method.params(),
                    method.parameter_has_types(),
                    method.parameter_defaults(),
                ),
            });
    }
    if context.has_interface(class_name) {
        return context
            .interface_method_requirements(class_name)
            .into_iter()
            .find(|method| method.name().eq_ignore_ascii_case(method_name))
            .map(|method| EvalReflectionMemberMetadata {
                attributes: method.attributes().to_vec(),
                visibility: EvalVisibility::Public,
                is_static: false,
                is_final: false,
                is_abstract: true,
                parameters: eval_reflection_parameters_from_names_and_type_flags(
                    method.params(),
                    method.parameter_has_types(),
                    method.parameter_defaults(),
                ),
            });
    }
    context.trait_decl(class_name).and_then(|trait_decl| {
        trait_decl
            .methods()
            .iter()
            .find(|method| method.name().eq_ignore_ascii_case(method_name))
            .map(|method| EvalReflectionMemberMetadata {
                attributes: method.attributes().to_vec(),
                visibility: method.visibility(),
                is_static: method.is_static(),
                is_final: method.is_final(),
                is_abstract: method.is_abstract(),
                parameters: eval_reflection_parameters_from_names_and_type_flags(
                    method.params(),
                    method.parameter_has_types(),
                    method.parameter_defaults(),
                ),
            })
    })
}

/// Returns property metadata for a property-like member on an eval class-like symbol.
fn eval_reflection_property_metadata(
    class_name: &str,
    property_name: &str,
    context: &ElephcEvalContext,
) -> Option<EvalReflectionMemberMetadata> {
    if context.has_class(class_name) || context.has_enum(class_name) {
        return context
            .class_property(class_name, property_name)
            .map(|(_, property)| EvalReflectionMemberMetadata {
                attributes: property.attributes().to_vec(),
                visibility: property.visibility(),
                is_static: property.is_static(),
                is_final: false,
                is_abstract: property.is_abstract(),
                parameters: Vec::new(),
            });
    }
    if context.has_interface(class_name) {
        return context
            .interface_property_requirements(class_name)
            .into_iter()
            .find(|property| property.name() == property_name)
            .map(|property| EvalReflectionMemberMetadata {
                attributes: property.attributes().to_vec(),
                visibility: EvalVisibility::Public,
                is_static: false,
                is_final: false,
                is_abstract: true,
                parameters: Vec::new(),
            });
    }
    context.trait_decl(class_name).and_then(|trait_decl| {
        trait_decl
            .properties()
            .iter()
            .find(|property| property.name() == property_name)
            .map(|property| EvalReflectionMemberMetadata {
                attributes: property.attributes().to_vec(),
                visibility: property.visibility(),
                is_static: property.is_static(),
                is_final: false,
                is_abstract: property.is_abstract(),
                parameters: Vec::new(),
            })
    })
}

/// Builds parameter reflection metadata from eval parameter names and type flags.
fn eval_reflection_parameters_from_names_and_type_flags(
    names: &[String],
    has_type_flags: &[bool],
    defaults: &[Option<EvalExpr>],
) -> Vec<EvalReflectionParameterMetadata> {
    names
        .iter()
        .enumerate()
        .map(|(position, name)| EvalReflectionParameterMetadata {
            name: name.clone(),
            position,
            is_optional: defaults.get(position).is_some_and(Option::is_some),
            is_variadic: false,
            is_passed_by_reference: false,
            has_type: has_type_flags.get(position).copied().unwrap_or(false),
        })
        .collect()
}

/// Packs ReflectionMethod/ReflectionProperty predicate flags for the runtime owner factory.
fn eval_reflection_member_flags(
    visibility: EvalVisibility,
    is_static: bool,
    is_final: bool,
    is_abstract: bool,
) -> u64 {
    let mut flags = 0;
    if is_static {
        flags |= EVAL_REFLECTION_MEMBER_FLAG_STATIC;
    }
    match visibility {
        EvalVisibility::Public => flags |= EVAL_REFLECTION_MEMBER_FLAG_PUBLIC,
        EvalVisibility::Protected => flags |= EVAL_REFLECTION_MEMBER_FLAG_PROTECTED,
        EvalVisibility::Private => flags |= EVAL_REFLECTION_MEMBER_FLAG_PRIVATE,
    }
    if is_final {
        flags |= EVAL_REFLECTION_MEMBER_FLAG_FINAL;
    }
    if is_abstract {
        flags |= EVAL_REFLECTION_MEMBER_FLAG_ABSTRACT;
    }
    flags
}

/// Packs ReflectionParameter predicate flags for the runtime parameter factory.
fn eval_reflection_parameter_flags(parameter: &EvalReflectionParameterMetadata) -> u64 {
    let mut flags = 0;
    if parameter.is_optional {
        flags |= EVAL_REFLECTION_PARAMETER_FLAG_OPTIONAL;
    }
    if parameter.is_variadic {
        flags |= EVAL_REFLECTION_PARAMETER_FLAG_VARIADIC;
    }
    if parameter.is_passed_by_reference {
        flags |= EVAL_REFLECTION_PARAMETER_FLAG_BY_REF;
    }
    if parameter.has_type {
        flags |= EVAL_REFLECTION_PARAMETER_FLAG_HAS_TYPE;
    }
    flags
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
