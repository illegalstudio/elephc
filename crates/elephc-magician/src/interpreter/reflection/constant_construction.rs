//! Purpose:
//! Constructs reflected class constants and enum cases.
//!
//! Called from:
//! - `crate::interpreter::reflection` for ReflectionClassConstant and enum-case owners.
//!
//! Key details:
//! - Missing constants and non-case targets preserve PHP exception categories.

use super::*;

/// Builds an eval-backed `ReflectionClassConstant` object for a class constant or enum case.
pub(super) fn eval_reflection_class_constant_new(
    evaluated_args: Vec<EvaluatedCallArg>,
    context: &mut ElephcEvalContext,
    values: &mut impl RuntimeValueOps,
) -> Result<Option<RuntimeCellHandle>, EvalStatus> {
    let args = bind_evaluated_function_args(
        &[String::from("class_name"), String::from("constant_name")],
        evaluated_args,
    )?;
    let class_name = eval_reflection_string_arg(args[0], values)?;
    let constant_name = eval_reflection_string_arg(args[1], values)?;
    eval_reflection_class_constant_object_result_or_throw(
        &class_name,
        &constant_name,
        context,
        values,
    )
}

/// Builds a `ReflectionClassConstant` object or throws PHP's catchable error.
pub(super) fn eval_reflection_class_constant_object_result_or_throw(
    class_name: &str,
    constant_name: &str,
    context: &mut ElephcEvalContext,
    values: &mut impl RuntimeValueOps,
) -> Result<Option<RuntimeCellHandle>, EvalStatus> {
    let Some((declaring_class_name, attributes, visibility, is_final, is_enum_case)) =
        eval_reflection_class_constant_metadata(class_name, constant_name, context, values)?
    else {
        return eval_reflection_missing_class_constant_exception(
            class_name,
            constant_name,
            context,
            values,
        );
    };
    let constant_value = eval_reflection_constant_value(class_name, constant_name, context, values)?
        .ok_or(EvalStatus::RuntimeFatal)?;
    let mut flags = eval_reflection_member_flags(visibility, false, is_final, false, false);
    if is_enum_case {
        flags |= EVAL_REFLECTION_MEMBER_FLAG_ENUM_CASE;
    }
    let modifiers = eval_reflection_class_constant_modifiers(visibility, is_final);
    eval_reflection_owner_object(
        EVAL_REFLECTION_OWNER_CLASS_CONSTANT,
        constant_name,
        &attributes,
        &[],
        &[],
        &[],
        &[],
        Some(&declaring_class_name),
        &[],
        None,
        None,
        None,
        None,
        flags,
        modifiers,
        0,
        Some(constant_value),
        None,
        context,
        values,
    )
    .map(Some)
}

/// Throws the catchable ReflectionException for a missing class-like constant.
pub(super) fn eval_reflection_missing_class_constant_exception(
    class_name: &str,
    constant_name: &str,
    context: &mut ElephcEvalContext,
    values: &mut impl RuntimeValueOps,
) -> Result<Option<RuntimeCellHandle>, EvalStatus> {
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
        &format!("Constant {}::{} does not exist", reflected_name, constant_name),
        context,
        values,
    )
}

/// Builds an eval-backed ReflectionEnumUnitCase/BackedCase object for an enum case.
pub(super) fn eval_reflection_enum_case_new(
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
    let case_name = eval_reflection_string_arg(args[1], values)?;
    let Some(enum_decl) = context.enum_decl(&enum_name) else {
        if eval_reflection_class_constant_metadata(&enum_name, &case_name, context, values)?
            .is_some()
        {
            return eval_reflection_not_enum_case_exception(
                &enum_name, &case_name, context, values,
            );
        }
        if !eval_reflection_class_like_exists(&enum_name, context)
            && eval_reflection_class_like_or_runtime_exists(&enum_name, context, values)?
        {
            return Ok(None);
        }
        return eval_reflection_missing_class_constant_exception(
            &enum_name, &case_name, context, values,
        );
    };
    let declaring_class_name = enum_decl.name().to_string();
    let has_case = enum_decl.case(&case_name).is_some();
    let is_backed = enum_decl.backing_type().is_some();
    if !has_case {
        if eval_reflection_class_constant_metadata(
            &declaring_class_name,
            &case_name,
            context,
            values,
        )?
        .is_some()
        {
            return eval_reflection_not_enum_case_exception(
                &declaring_class_name,
                &case_name,
                context,
                values,
            );
        }
        return eval_reflection_missing_class_constant_exception(
            &declaring_class_name,
            &case_name,
            context,
            values,
        );
    }
    if owner_kind == EVAL_REFLECTION_OWNER_ENUM_BACKED_CASE && !is_backed {
        return eval_throw_reflection_exception(
            &format!("Enum case {}::{} is not a backed case", declaring_class_name, case_name),
            context,
            values,
        );
    }
    eval_reflection_enum_case_object_result(
        owner_kind,
        &declaring_class_name,
        &case_name,
        context,
        values,
    )
    .map(Some)
}

/// Throws the catchable ReflectionException for a constant that is not an enum case.
pub(super) fn eval_reflection_not_enum_case_exception(
    class_name: &str,
    constant_name: &str,
    context: &mut ElephcEvalContext,
    values: &mut impl RuntimeValueOps,
) -> Result<Option<RuntimeCellHandle>, EvalStatus> {
    let reflected_name = context
        .resolve_class_like_name(class_name)
        .unwrap_or_else(|| class_name.trim_start_matches('\\').to_string());
    eval_throw_reflection_exception(
        &format!("Constant {}::{} is not a case", reflected_name, constant_name),
        context,
        values,
    )
}

/// Builds one eval-backed enum-case reflection owner object.
pub(super) fn eval_reflection_enum_case_object_result(
    owner_kind: u64,
    enum_name: &str,
    case_name: &str,
    context: &mut ElephcEvalContext,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    let (declaring_class_name, attributes, is_backed) = {
        let enum_decl = context.enum_decl(enum_name).ok_or(EvalStatus::RuntimeFatal)?;
        let case = enum_decl.case(case_name).ok_or(EvalStatus::RuntimeFatal)?;
        (
            enum_decl.name().to_string(),
            case.attributes().to_vec(),
            enum_decl.backing_type().is_some(),
        )
    };
    if owner_kind == EVAL_REFLECTION_OWNER_ENUM_BACKED_CASE && !is_backed {
        return Err(EvalStatus::RuntimeFatal);
    }
    let case_value = context
        .enum_case(&declaring_class_name, case_name)
        .ok_or(EvalStatus::RuntimeFatal)?;
    let backing_value = if owner_kind == EVAL_REFLECTION_OWNER_ENUM_BACKED_CASE {
        Some(
            context
                .enum_case_value(&declaring_class_name, case_name)
                .ok_or(EvalStatus::RuntimeFatal)?,
        )
    } else {
        None
    };
    let flags = eval_reflection_member_flags(EvalVisibility::Public, false, false, false, false)
        | EVAL_REFLECTION_MEMBER_FLAG_ENUM_CASE;
    let modifiers = eval_reflection_class_constant_modifiers(EvalVisibility::Public, false);
    eval_reflection_owner_object(
        owner_kind,
        case_name,
        &attributes,
        &[],
        &[],
        &[],
        &[],
        Some(&declaring_class_name),
        &[],
        None,
        None,
        None,
        None,
        flags,
        modifiers,
        0,
        Some(case_value),
        backing_value,
        context,
        values,
    )
}

/// Selects the concrete enum-case reflector class for one enum.
pub(super) fn eval_reflection_enum_case_owner_kind(
    enum_name: &str,
    context: &ElephcEvalContext,
) -> Result<u64, EvalStatus> {
    let enum_decl = context.enum_decl(enum_name).ok_or(EvalStatus::RuntimeFatal)?;
    Ok(if enum_decl.backing_type().is_some() {
        EVAL_REFLECTION_OWNER_ENUM_BACKED_CASE
    } else {
        EVAL_REFLECTION_OWNER_ENUM_UNIT_CASE
    })
}

/// Builds `ReflectionNamedType` metadata for an enum backing type.
pub(super) fn eval_reflection_enum_backing_type_metadata(
    backing_type: EvalEnumBackingType,
) -> EvalReflectionParameterTypeMetadata {
    let name = match backing_type {
        EvalEnumBackingType::Int => "int",
        EvalEnumBackingType::String => "string",
    };
    EvalReflectionParameterTypeMetadata {
        kind: EvalReflectionParameterTypeKind::Named(eval_reflection_builtin_named_type(
            name, false,
        )),
    }
}
