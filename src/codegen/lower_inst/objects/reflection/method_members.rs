//! Purpose:
//! ReflectionMethod member construction for classes, interfaces, and traits.
//!
//! Called from:
//! - `crate::codegen::lower_inst::objects::reflection`.
//!
//! Key details:
//! - Preserves compile-time metadata, target-aware object layout, and ownership.

use super::*;

/// Returns a method's DECLARED spelling for reflected metadata.
///
/// Lookup stays case-insensitive — `php_symbol_key` lowercases — but PHP's reflection reports
/// the declaration, never the caller's lookup text, and elephc reported neither consistently:
/// the direct `ReflectionMethod` constructor echoed the spelling the caller typed
/// (`"mAtCh"` → `mAtCh`) while every listing path exposed the lowercase key (`match`), where
/// PHP answers `Match` from both (issue #571).
///
/// The declared spelling is already retained — `method_decls` holds a class's or interface's
/// source method declarations (including the methods a `use`d trait flattened into it), and
/// `TraitMethodInfo::declared_name` a trait's own — so this reads it back instead of keeping a
/// second copy that could drift from the declaration.
///
/// `owners` is searched in order: the declaring class-like first, then the reflected one, so an
/// inherited method takes its spelling from where it was actually written. The key is returned
/// unchanged when nothing matches, which is how a compiler-injected class-like (SPL,
/// `Exception`, the reflection classes themselves) is reported: there is no user source to take
/// a spelling from.
pub(super) fn reflection_declared_method_name(
    ctx: &FunctionContext<'_>,
    owners: &[&str],
    method_key: &str,
) -> String {
    owners
        .iter()
        .find_map(|owner| {
            reflection_declared_method_name_in(ctx, owner.trim_start_matches('\\'), method_key)
        })
        .unwrap_or_else(|| method_key.to_string())
}

/// [`reflection_declared_method_name`] for one class-like, interface or trait owner.
fn reflection_declared_method_name_in(
    ctx: &FunctionContext<'_>,
    owner: &str,
    method_key: &str,
) -> Option<String> {
    let declarations = ctx
        .module
        .class_infos
        .get(owner)
        .map(|info| info.method_decls.as_slice())
        .or_else(|| {
            ctx.module
                .interface_infos
                .get(owner)
                .map(|info| info.method_decls.as_slice())
        });
    if let Some(declarations) = declarations {
        if let Some(method) = declarations
            .iter()
            .find(|method| php_symbol_key(&method.name) == method_key)
        {
            return Some(method.name.clone());
        }
    }
    ctx.module
        .declared_trait_methods
        .get(owner)
        .and_then(|methods| methods.get(method_key))
        .map(|info| info.declared_name.clone())
}

/// Builds ReflectionMethod array entries for the methods visible on one class.
pub(super) fn reflection_class_method_members(
    ctx: &FunctionContext<'_>,
    class_name: &str,
    info: &crate::types::ClassInfo,
    method_names: &[String],
) -> Result<Vec<ReflectionListedMember>> {
    let mut members = Vec::new();
    for method_name in method_names {
        if let Some(member) = reflection_class_method_member(ctx, class_name, info, method_name)? {
            members.push(member);
        }
    }
    Ok(members)
}

/// Builds one ReflectionMethod array entry from class metadata.
pub(super) fn reflection_class_method_member(
    ctx: &FunctionContext<'_>,
    class_name: &str,
    info: &crate::types::ClassInfo,
    method_name: &str,
) -> Result<Option<ReflectionListedMember>> {
    let method_key = php_symbol_key(method_name);
    let sig = info
        .methods
        .get(&method_key)
        .or_else(|| info.static_methods.get(&method_key));
    let Some(sig) = sig else {
        return Ok(None);
    };
    let declaring_class_name =
        reflection_method_declaring_class_name(info, class_name, &method_key);
    let declared_name = reflection_declared_method_name(
        ctx,
        &[
            declaring_class_name.as_deref().unwrap_or(class_name),
            class_name,
        ],
        &method_key,
    );
    let attr_names = info
        .method_attribute_names
        .get(&method_key)
        .cloned()
        .unwrap_or_default();
    let attr_args = info
        .method_attribute_args
        .get(&method_key)
        .cloned()
        .unwrap_or_default();
    let Some(flags) = reflection_method_member_flags(info, &method_key) else {
        return Ok(None);
    };
    let required_parameter_count = reflection_required_parameter_count(sig);
    let late_static_return = if flags.is_static {
        info.late_static_static_method_returns.get(&method_key)
    } else {
        info.late_static_method_returns.get(&method_key)
    };
    let type_metadata = reflection_method_return_type_metadata(sig, late_static_return);
    let is_generator = reflection_method_is_generator(
        ctx,
        declaring_class_name.as_deref().unwrap_or(class_name),
        &method_key,
    );
    let prototype_member =
        reflection_class_method_prototype_member(ctx, class_name, info, &method_key, flags)?;
    let declaring_function = ReflectionDeclaringFunctionMember::Method {
        name: declared_name.clone(),
        declaring_class_name: declaring_class_name.clone(),
        attr_names: attr_names.clone(),
        attr_args: attr_args.clone(),
        flags,
        required_parameter_count,
        type_metadata: type_metadata.clone(),
        is_deprecated: sig.deprecation.is_some(),
        is_generator,
    };
    let source_defaults = declaring_class_name
        .as_deref()
        .and_then(|declaring_class| {
            reflection_source_method_defaults(
                ctx,
                declaring_class,
                &method_key,
                flags.is_static,
            )
        });
    let parameters = reflection_parameter_members_with_declaring_class(
        ctx,
        sig,
        class_name,
        Some(info),
        declaring_class_name.as_deref(),
        Some(declaring_function),
        &reflection_promoted_constructor_parameter_names(info, &method_key),
        source_defaults.as_deref(),
    )?;
    Ok(Some(ReflectionListedMember {
        name: declared_name,
        declaring_class_name,
        attr_names,
        attr_args,
        constant_value: None,
        backing_value: None,
        is_enum_case: false,
        flags,
        modifiers: reflection_method_modifiers_from_flags(flags),
        type_metadata,
        default_value: None,
        property_hook_members: Vec::new(),
        required_parameter_count,
        is_deprecated: sig.deprecation.is_some(),
        is_generator,
        prototype_member,
        parameters,
    }))
}

/// Builds ReflectionMethod array entries for methods declared by an interface.
pub(super) fn reflection_interface_method_members(
    ctx: &FunctionContext<'_>,
    info: &InterfaceInfo,
    interface_name: &str,
    method_names: &[String],
) -> Result<Vec<ReflectionListedMember>> {
    let mut members = Vec::new();
    for method_name in method_names {
        if let Some(member) =
            reflection_interface_method_member(ctx, info, interface_name, method_name)?
        {
            members.push(member);
        }
    }
    Ok(members)
}

/// Builds one ReflectionMethod array entry from interface metadata.
pub(super) fn reflection_interface_method_member(
    ctx: &FunctionContext<'_>,
    info: &InterfaceInfo,
    interface_name: &str,
    method_name: &str,
) -> Result<Option<ReflectionListedMember>> {
    let method_key = php_symbol_key(method_name);
    let Some((sig, is_static)) = info
        .methods
        .get(&method_key)
        .map(|sig| (sig, false))
        .or_else(|| info.static_methods.get(&method_key).map(|sig| (sig, true)))
    else {
        return Ok(None);
    };
    let declaring_class_name = info
        .method_declaring_interfaces
        .get(&method_key)
        .or_else(|| info.static_method_declaring_interfaces.get(&method_key))
        .cloned()
        .unwrap_or_else(|| interface_name.to_string());
    let required_parameter_count = reflection_required_parameter_count(sig);
    let flags = reflection_member_flags(is_static, &Visibility::Public, false, true, false, false);
    let late_static_return = if is_static {
        info.late_static_static_method_returns.get(&method_key)
    } else {
        info.late_static_method_returns.get(&method_key)
    };
    let type_metadata = reflection_method_return_type_metadata(sig, late_static_return);
    let declared_name = reflection_declared_method_name(
        ctx,
        &[declaring_class_name.as_str(), interface_name],
        &method_key,
    );
    let declaring_function = ReflectionDeclaringFunctionMember::Method {
        name: declared_name.clone(),
        declaring_class_name: Some(declaring_class_name.clone()),
        attr_names: Vec::new(),
        attr_args: Vec::new(),
        flags,
        required_parameter_count,
        type_metadata: type_metadata.clone(),
        is_deprecated: sig.deprecation.is_some(),
        is_generator: false,
    };
    let source_defaults = reflection_source_method_defaults(
        ctx,
        declaring_class_name.as_str(),
        &method_key,
        is_static,
    );
    let parameters = reflection_parameter_members_with_declaring_class(
        ctx,
        sig,
        declaring_class_name.as_str(),
        None,
        Some(declaring_class_name.as_str()),
        Some(declaring_function),
        &[],
        source_defaults.as_deref(),
    )?;
    Ok(Some(ReflectionListedMember {
        name: declared_name,
        declaring_class_name: Some(declaring_class_name),
        attr_names: Vec::new(),
        attr_args: Vec::new(),
        constant_value: None,
        backing_value: None,
        is_enum_case: false,
        flags,
        modifiers: reflection_method_modifiers_from_flags(flags),
        type_metadata,
        default_value: None,
        property_hook_members: Vec::new(),
        required_parameter_count,
        is_deprecated: sig.deprecation.is_some(),
        is_generator: false,
        prototype_member: None,
        parameters,
    }))
}

/// Builds ReflectionMethod array entries for methods declared by a trait.
pub(super) fn reflection_trait_method_members(
    ctx: &FunctionContext<'_>,
    methods: &std::collections::HashMap<String, TraitMethodInfo>,
    trait_name: &str,
    method_names: &[String],
) -> Result<Vec<ReflectionListedMember>> {
    let mut members = Vec::new();
    for method_name in method_names {
        if let Some(member) = reflection_trait_method_member(ctx, methods, trait_name, method_name)?
        {
            members.push(member);
        }
    }
    Ok(members)
}

/// Builds one ReflectionMethod array entry from retained trait metadata.
pub(super) fn reflection_trait_method_member(
    ctx: &FunctionContext<'_>,
    methods: &std::collections::HashMap<String, TraitMethodInfo>,
    trait_name: &str,
    method_name: &str,
) -> Result<Option<ReflectionListedMember>> {
    let method_key = php_symbol_key(method_name);
    let Some(info) = methods.get(&method_key) else {
        return Ok(None);
    };
    let flags = reflection_member_flags(
        info.is_static,
        &info.visibility,
        info.is_final,
        info.is_abstract,
        false,
        false,
    );
    let required_parameter_count = reflection_required_parameter_count(&info.signature);
    let type_metadata = reflection_return_type_metadata(&info.signature);
    let is_generator = reflection_method_is_generator(ctx, trait_name, &method_key);
    let declaring_function = ReflectionDeclaringFunctionMember::Method {
        name: info.declared_name.clone(),
        declaring_class_name: Some(trait_name.to_string()),
        attr_names: Vec::new(),
        attr_args: Vec::new(),
        flags,
        required_parameter_count,
        type_metadata: type_metadata.clone(),
        is_deprecated: info.signature.deprecation.is_some(),
        is_generator,
    };
    let parameters = reflection_parameter_members_with_declaring_class(
        ctx,
        &info.signature,
        trait_name,
        None,
        Some(trait_name),
        Some(declaring_function),
        &[],
        None,
    )?;
    Ok(Some(ReflectionListedMember {
        name: info.declared_name.clone(),
        declaring_class_name: Some(trait_name.to_string()),
        attr_names: Vec::new(),
        attr_args: Vec::new(),
        constant_value: None,
        backing_value: None,
        is_enum_case: false,
        flags,
        modifiers: reflection_method_modifiers_from_flags(flags),
        type_metadata,
        default_value: None,
        property_hook_members: Vec::new(),
        required_parameter_count,
        is_deprecated: info.signature.deprecation.is_some(),
        is_generator,
        prototype_member: None,
        parameters,
    }))
}

/// Returns whether the lowered method body is a generator function.
pub(super) fn reflection_method_is_generator(
    ctx: &FunctionContext<'_>,
    declaring_class_name: &str,
    method_name: &str,
) -> bool {
    let expected_key = php_symbol_key(&format!(
        "{}::{}",
        declaring_class_name.trim_start_matches('\\'),
        method_name
    ));
    ctx.module.class_methods.iter().any(|function| {
        php_symbol_key(function.name.trim_start_matches('\\')) == expected_key
            && function.flags.is_generator
    })
}

