//! Purpose:
//! Fallback members and ReflectionParameter metadata assembly.
//!
//! Called from:
//! - `crate::codegen::lower_inst::objects::reflection`.
//!
//! Key details:
//! - Preserves compile-time metadata, target-aware object layout, and ownership.

use super::*;

/// Builds placeholder ReflectionMethod entries for class-like metadata without full method schemas.
pub(super) fn default_method_members(
    method_names: &[String],
    is_interface: bool,
    declaring_class_name: &str,
) -> Vec<ReflectionListedMember> {
    method_names
        .iter()
        .map(|name| ReflectionListedMember {
            name: name.clone(),
            declaring_class_name: Some(declaring_class_name.to_string()),
            attr_names: Vec::new(),
            attr_args: Vec::new(),
            constant_value: None,
            backing_value: None,
            is_enum_case: false,
            flags: reflection_member_flags(
                false,
                &Visibility::Public,
                false,
                is_interface,
                false,
                false,
            ),
            modifiers: reflection_method_modifiers_from_flags(reflection_member_flags(
                false,
                &Visibility::Public,
                false,
                is_interface,
                false,
                false,
            )),
            type_metadata: None,
            default_value: None,
        property_hook_members: Vec::new(),
            required_parameter_count: 0,
            is_deprecated: false,
            is_generator: false,
            prototype_member: None,
            parameters: Vec::new(),
        })
        .collect()
}

/// Builds placeholder ReflectionProperty entries for class-like metadata without full property schemas.
pub(super) fn default_property_members(
    property_names: &[String],
    is_interface: bool,
    declaring_class_name: &str,
) -> Vec<ReflectionListedMember> {
    property_names
        .iter()
        .map(|name| ReflectionListedMember {
            name: name.clone(),
            declaring_class_name: Some(declaring_class_name.to_string()),
            attr_names: Vec::new(),
            attr_args: Vec::new(),
            constant_value: None,
            backing_value: None,
            is_enum_case: false,
            flags: reflection_member_flags(
                false,
                &Visibility::Public,
                false,
                is_interface,
                false,
                false,
            ),
            modifiers: reflection_property_modifiers(
                &Visibility::Public,
                false,
                false,
                is_interface,
                false,
                is_interface,
                None,
            ),
            type_metadata: None,
            default_value: None,
        property_hook_members: Vec::new(),
            required_parameter_count: 0,
            is_deprecated: false,
            is_generator: false,
            prototype_member: None,
            parameters: Vec::new(),
        })
        .collect()
}

/// Returns PHP's required parameter count for a reflected native signature.
pub(super) fn reflection_required_parameter_count(sig: &FunctionSig) -> i64 {
    let fixed_count = sig
        .variadic
        .as_deref()
        .and_then(|variadic| {
            sig.params
                .iter()
                .position(|(name, _)| name.as_str() == variadic)
        })
        .unwrap_or(sig.params.len());
    (0..fixed_count)
        .rfind(|index| !sig.defaults.get(*index).is_some_and(Option::is_some))
        .map_or(0, |index| index as i64 + 1)
}

/// Returns promoted constructor property names for ReflectionParameter metadata.
pub(super) fn reflection_promoted_constructor_parameter_names(
    info: &crate::types::ClassInfo,
    method_key: &str,
) -> Vec<String> {
    if method_key.eq_ignore_ascii_case("__construct") {
        info.promoted_properties.iter().cloned().collect()
    } else {
        Vec::new()
    }
}

/// Returns lexical parameter defaults from the method's declaring class or interface.
/// Semantic signatures may canonicalize relative receivers for runtime materialization, while
/// ReflectionParameter must preserve source-visible names such as `self::X` and `parent::Y`.
pub(super) fn reflection_source_method_defaults(
    ctx: &FunctionContext<'_>,
    declaring_class_name: &str,
    method_key: &str,
    is_static: bool,
) -> Option<Vec<Option<Expr>>> {
    let declaring_class_name = declaring_class_name.trim_start_matches('\\');
    let declarations = ctx
        .module
        .class_infos
        .get(declaring_class_name)
        .map(|info| info.method_decls.as_slice())
        .or_else(|| {
            ctx.module
                .interface_infos
                .get(declaring_class_name)
                .map(|info| info.method_decls.as_slice())
        })?;
    let method = declarations.iter().find(|method| {
        method.is_static == is_static && php_symbol_key(&method.name) == method_key
    })?;
    Some(
        method
            .params
            .iter()
            .map(|(_, _, default, _)| default.clone())
            .collect(),
    )
}

/// Builds reflected parameter metadata and attaches declaring class metadata when present.
pub(super) fn reflection_parameter_members_with_declaring_class(
    ctx: &FunctionContext<'_>,
    sig: &FunctionSig,
    current_class: &str,
    current_info: Option<&crate::types::ClassInfo>,
    declaring_class_name: Option<&str>,
    declaring_function: Option<ReflectionDeclaringFunctionMember>,
    promoted_parameter_names: &[String],
    source_defaults: Option<&[Option<Expr>]>,
) -> Result<Vec<ReflectionParameterMember>> {
    reflection_parameter_members_with_declaring_function(
        ctx,
        sig,
        current_class,
        current_info,
        declaring_class_name,
        declaring_function,
        promoted_parameter_names,
        source_defaults,
    )
}

/// Builds reflected parameter metadata with optional declaring owner metadata.
pub(super) fn reflection_parameter_members_with_declaring_function(
    ctx: &FunctionContext<'_>,
    sig: &FunctionSig,
    current_class: &str,
    current_info: Option<&crate::types::ClassInfo>,
    declaring_class_name: Option<&str>,
    declaring_function: Option<ReflectionDeclaringFunctionMember>,
    promoted_parameter_names: &[String],
    source_defaults: Option<&[Option<Expr>]>,
) -> Result<Vec<ReflectionParameterMember>> {
    let mut parameters = Vec::new();
    for (index, (name, ty)) in sig.params.iter().enumerate() {
        let is_variadic = sig.variadic.as_deref() == Some(name.as_str());
        // `declared_params` doubles as the runtime invoker's boxed-ABI marker
        // (see `eir_runtime_metadata_signature`), which only ever raises the
        // flag for Mixed/Union params. Non-Mixed declared params are always
        // genuine (source hints, builtin signatures, variadics); a declared
        // Mixed param needs the source type expression to distinguish a real
        // `mixed` hint from an untyped param widened for the boxed ABI.
        let declared = sig.declared_params.get(index).copied().unwrap_or(false);
        let has_type = declared
            && (!matches!(ty.codegen_repr(), PhpType::Mixed | PhpType::Union(_))
                || sig
                    .param_type_exprs
                    .get(index)
                    .and_then(Option::as_ref)
                    .is_some());
        let type_metadata = reflection_parameter_type_metadata(
            sig.param_type_exprs.get(index).and_then(Option::as_ref),
            ty,
        )
        .filter(|_| has_type);
        let default_expr = sig.defaults.get(index).and_then(Option::as_ref);
        let default_value = default_expr
            .map(|default| {
                reflection_parameter_default_value(ctx, current_class, current_info, default)
            })
            .transpose()?
            .flatten();
        let source_default_expr = source_defaults
            .and_then(|defaults| defaults.get(index))
            .and_then(Option::as_ref);
        let default_value_constant_name = source_default_expr
            .and_then(reflection_parameter_default_constant_name)
            .or_else(|| {
                default_expr.and_then(reflection_parameter_default_constant_name)
            });
        let is_array_type = reflection_parameter_has_named_type(type_metadata.as_ref(), "array");
        let is_callable_type =
            reflection_parameter_has_named_type(type_metadata.as_ref(), "callable");
        parameters.push(ReflectionParameterMember {
            name: name.clone(),
            declaring_class_name: declaring_class_name.map(str::to_string),
            declaring_function: declaring_function.clone(),
            attr_names: sig
                .param_attributes
                .get(index)
                .map(|groups| crate::types::collect_attribute_names(groups))
                .unwrap_or_default(),
            attr_args: sig
                .param_attributes
                .get(index)
                .map(|groups| crate::types::collect_attribute_args(groups))
                .unwrap_or_default(),
            position: index as i64,
            is_optional: is_variadic
                || sig
                    .defaults
                    .get(index)
                    .map(|default| default.is_some())
                    .unwrap_or(false),
            is_variadic,
            is_passed_by_reference: sig.ref_params.get(index).copied().unwrap_or(false),
            is_promoted: promoted_parameter_names
                .iter()
                .any(|promoted_name| promoted_name == name),
            has_type,
            allows_null: reflection_parameter_allows_null(
                has_type,
                type_metadata.as_ref(),
                default_value.as_ref(),
            ),
            is_array_type,
            is_callable_type,
            type_metadata,
            default_value,
            default_value_constant_name,
        });
    }
    Ok(parameters)
}

/// Returns whether retained parameter metadata is one named type with the requested name.
pub(super) fn reflection_parameter_has_named_type(
    type_metadata: Option<&ReflectionParameterTypeMetadata>,
    expected_name: &str,
) -> bool {
    matches!(
        type_metadata,
        Some(ReflectionParameterTypeMetadata::Named(named))
            if named.name.eq_ignore_ascii_case(expected_name)
    )
}

/// Returns PHP's `ReflectionParameter::allowsNull()` value for static metadata.
pub(super) fn reflection_parameter_allows_null(
    has_type: bool,
    type_metadata: Option<&ReflectionParameterTypeMetadata>,
    default_value: Option<&ReflectionParameterDefaultValue>,
) -> bool {
    !has_type
        || matches!(default_value, Some(ReflectionParameterDefaultValue::Null))
        || type_metadata.is_some_and(reflection_type_allows_null)
}

/// Returns whether one retained ReflectionType metadata value accepts null.
pub(super) fn reflection_type_allows_null(type_metadata: &ReflectionParameterTypeMetadata) -> bool {
    match type_metadata {
        ReflectionParameterTypeMetadata::Named(named_type) => named_type.allows_null,
        ReflectionParameterTypeMetadata::Union(union_type) => union_type.allows_null,
        ReflectionParameterTypeMetadata::Intersection(_) => false,
    }
}
