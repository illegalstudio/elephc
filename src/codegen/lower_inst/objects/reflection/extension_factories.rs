//! Purpose:
//! Emits fresh-object factories for the bounded DOM-family reflection graph.
//!
//! Called from:
//! - `crate::codegen::block_emit::emit_module()` before ordinary EIR bodies.
//! - Reflection owner emitters and runtime-name dispatches for nested owners.
//!
//! Key details:
//! - Each factory allocates a new object, preserving PHP reflection identity.
//! - Every factory returns a new object, so sharing code never shares reflection
//!   identity or mutable collection slots.
//! - Class, enum, and function factory calls break the metadata graph's cyclic
//!   owner/member edges instead of merely moving the extension registry payload.

use crate::codegen::context::FunctionContext;
use crate::codegen::data_section::DataSection;
use crate::codegen::emit::Emitter;
use crate::codegen::shared_helper::emit_shared_helper;
use crate::codegen::shared_state::SharedCodegenState;
use crate::ir::Module;
use crate::types::PhpType;

use super::*;

const DOM_FACTORY_LABEL: &str = "_eir_shared_reflection_extension_dom";
const LIBXML_FACTORY_LABEL: &str = "_eir_shared_reflection_extension_libxml";
const SIMPLEXML_FACTORY_LABEL: &str = "_eir_shared_reflection_extension_simplexml";

/// Returns the shared fresh-object factory label for one bounded DOM-family extension.
fn reflection_extension_factory_label(extension_name: &str) -> Result<&'static str> {
    match php_symbol_key(extension_name).as_str() {
        "dom" => Ok(DOM_FACTORY_LABEL),
        "libxml" => Ok(LIBXML_FACTORY_LABEL),
        "simplexml" => Ok(SIMPLEXML_FACTORY_LABEL),
        _ => Err(CodegenIrError::unsupported(format!(
            "ReflectionExtension factory for unknown extension {}",
            extension_name
        ))),
    }
}

/// Calls the emitted factory that returns a new ReflectionExtension object in result registers.
pub(super) fn emit_reflection_extension_factory(
    ctx: &mut FunctionContext<'_>,
    extension_name: &str,
) -> Result<()> {
    let label = reflection_extension_factory_label(extension_name)?;
    abi::emit_call_label(ctx.emitter, label);
    Ok(())
}

/// Encodes a PHP symbol into an assembler-safe suffix for a generated factory label.
fn reflection_factory_label_suffix(name: &str) -> String {
    name.bytes().map(|byte| format!("{byte:02x}")).collect()
}

/// Returns the deterministic label for one shared reflection owner factory.
fn reflection_owner_factory_label(
    reflector_class: &str,
    reflected_name: &str,
    shallow: bool,
) -> String {
    let depth = if shallow { "shallow" } else { "full" };
    format!(
        "_eir_shared_{}_{}_{}",
        php_symbol_key(reflector_class),
        depth,
        reflection_factory_label_suffix(reflected_name),
    )
}

/// Formats an internal `ReflectionFunction` object exactly as PHP exposes it through
/// `ReflectionFunction::__toString()`.
///
/// The shared DOM-family factories bypass the normal constructor lowering, so they
/// must initialize the synthetic `__string` slot themselves rather than leaving the
/// shell class's empty default observable through object-to-string coercion.
fn reflection_internal_function_to_string(
    metadata: &ReflectionOwnerMetadata,
    extension_name: &str,
) -> String {
    let reflected_name = metadata.reflected_name.as_deref().unwrap_or_default();
    let deprecation = if metadata.is_deprecated {
        ", deprecated"
    } else {
        ""
    };
    let mut rendered = format!(
        "Function [ <internal{deprecation}:{extension_name}> function {reflected_name} ] {{\n\n  - Parameters [{}] {{\n",
        metadata.parameter_members.len(),
    );
    for parameter in &metadata.parameter_members {
        let requirement = if parameter.is_optional { "optional" } else { "required" };
        let type_label = parameter
            .type_metadata
            .as_ref()
            .map(reflection_type_metadata_to_string)
            .map(|label| format!("{label} "))
            .unwrap_or_default();
        let reference = if parameter.is_passed_by_reference { "&" } else { "" };
        let variadic = if parameter.is_variadic { "..." } else { "" };
        let default = parameter
            .default_value_display
            .as_ref()
            .map(|display| match display {
                ReflectionParameterDefaultDisplay::ClassNameConstant(class_name) => {
                    format!(" = {class_name}::class")
                }
            })
            .or_else(|| {
                parameter
                    .default_value_constant_name
                    .as_deref()
                    .map(|value| format!(" = {value}"))
                    .or_else(|| {
                        parameter.default_value.as_ref().map(|value| match value {
                            ReflectionParameterDefaultValue::Int(value) => format!(" = {value}"),
                            ReflectionParameterDefaultValue::Bool(value) => format!(" = {value}"),
                            ReflectionParameterDefaultValue::Float(value) => format!(" = {value}"),
                            ReflectionParameterDefaultValue::Str(value) => {
                                format!(" = \"{}\"", value.replace('"', "\\\""))
                            }
                            ReflectionParameterDefaultValue::Null => String::from(" = null"),
                            ReflectionParameterDefaultValue::Object { class_name, args } if args.is_empty() => {
                                format!(" = new {class_name}()")
                            }
                            ReflectionParameterDefaultValue::Object { class_name, .. } => {
                                format!(" = new {class_name}(...)" )
                            }
                            ReflectionParameterDefaultValue::Array(_) => String::from(" = []"),
                            ReflectionParameterDefaultValue::AssocArray(_) => String::from(" = []"),
                        })
                    })
                })
            .unwrap_or_default();
        rendered.push_str(&format!(
            "    Parameter #{} [ <{requirement}> {type_label}{reference}{variadic}${}{} ]\n",
            parameter.position, parameter.name, default,
        ));
    }
    rendered.push_str("  }\n");
    if let Some(return_type) = metadata
        .type_metadata
        .as_ref()
        .map(reflection_type_metadata_to_string)
    {
        rendered.push_str(&format!("  - Return [ {return_type} ]\n"));
    }
    rendered.push_str("}\n");
    rendered
}

/// Resolves a canonical DOM-family class name when its factory is emitted for this module.
fn shared_reflection_class_factory_name(
    ctx: &FunctionContext<'_>,
    reflected_name: &str,
    reflector_class: &str,
) -> Result<Option<String>> {
    let metadata = reflection_class_metadata_for_name(ctx, reflected_name)?;
    let Some(canonical) = metadata.reflected_name else {
        return Ok(None);
    };
    if reflection_extension_name_for_class(&canonical).is_none() {
        return Ok(None);
    }
    let has_factory = match reflector_class {
        "ReflectionClass" => ctx.module.class_infos.contains_key(&canonical)
            || ctx.module.enum_infos.contains_key(&canonical),
        "ReflectionEnum" => ctx.module.enum_infos.contains_key(&canonical),
        _ => false,
    };
    Ok(has_factory.then_some(canonical))
}

/// Resolves a canonical DOM-family function name when its factory is emitted for this module.
fn shared_reflection_function_factory_name(
    ctx: &FunctionContext<'_>,
    reflected_name: &str,
) -> Result<Option<String>> {
    let metadata = reflection_function_metadata_for_name(ctx, reflected_name)?;
    let Some(canonical) = metadata.reflected_name else {
        return Ok(None);
    };
    Ok(reflection_extension_name_for_function(&canonical)
        .is_some()
        .then_some(canonical))
}

/// Calls a shared DOM-family reflection owner factory when one owns this metadata.
///
/// Returns whether the call was emitted so callers can retain their existing inline
/// path for user classes and non-registry reflection objects.
pub(super) fn emit_shared_reflection_owner_factory(
    ctx: &mut FunctionContext<'_>,
    reflector_class: &str,
    reflected_name: &str,
    shallow: bool,
) -> Result<bool> {
    let canonical = match reflector_class {
        "ReflectionClass" | "ReflectionEnum" => {
            shared_reflection_class_factory_name(ctx, reflected_name, reflector_class)?
        }
        "ReflectionFunction" if !shallow => {
            shared_reflection_function_factory_name(ctx, reflected_name)?
        }
        _ => None,
    };
    let Some(canonical) = canonical else {
        return Ok(false);
    };
    let label_name = if reflector_class == "ReflectionFunction" {
        php_symbol_key(&canonical)
    } else {
        canonical
    };
    let label = reflection_owner_factory_label(reflector_class, &label_name, shallow);
    abi::emit_call_label(ctx.emitter, &label);
    Ok(true)
}

/// Returns whether this module contains the synthetic ReflectionExtension layout the factories use.
fn module_uses_reflection_extension_factories(module: &Module) -> bool {
    module.class_infos.contains_key("ReflectionExtension")
}

/// Emits bounded DOM-family reflection factories once before ordinary EIR bodies.
pub(in crate::codegen) fn emit_shared_reflection_extension_factories(
    module: &Module,
    emitter: &mut Emitter,
    data: &mut DataSection,
    shared: &mut SharedCodegenState,
    regalloc_linear: bool,
) -> Result<()> {
    if !module_uses_reflection_extension_factories(module) {
        return Ok(());
    }

    for (extension_name, label) in [
        ("dom", DOM_FACTORY_LABEL),
        ("libxml", LIBXML_FACTORY_LABEL),
        ("SimpleXML", SIMPLEXML_FACTORY_LABEL),
    ] {
        let metadata = reflection_extension_metadata_for_name(extension_name)?;
        emit_shared_helper(
            module,
            emitter,
            data,
            shared,
            regalloc_linear,
            label,
            PhpType::Mixed,
            &format!("--- shared ReflectionExtension factory: {} ---", extension_name),
            |ctx| emit_reflection_owner_object(ctx, "ReflectionExtension", &metadata),
        )?;
    }

    let mut class_names = module
        .class_infos
        .keys()
        .chain(module.enum_infos.keys())
        .filter(|name| reflection_extension_name_for_class(name).is_some())
        .cloned()
        .collect::<Vec<_>>();
    class_names.sort_unstable();
    class_names.dedup();

    for class_name in &class_names {
        let label = reflection_owner_factory_label("ReflectionClass", class_name, false);
        emit_shared_helper(
            module,
            emitter,
            data,
            shared,
            regalloc_linear,
            &label,
            PhpType::Mixed,
            &format!("--- shared ReflectionClass factory: {} ---", class_name),
            |ctx| {
                let metadata = reflection_class_metadata_for_name(ctx, class_name)?;
                emit_reflection_owner_object(ctx, "ReflectionClass", &metadata)
            },
        )?;

        let shallow_label = reflection_owner_factory_label("ReflectionClass", class_name, true);
        emit_shared_helper(
            module,
            emitter,
            data,
            shared,
            regalloc_linear,
            &shallow_label,
            PhpType::Mixed,
            &format!("--- shared shallow ReflectionClass factory: {} ---", class_name),
            |ctx| {
                let metadata = reflection_shallow_class_metadata_for_name(ctx, class_name)?;
                emit_reflection_owner_object(ctx, "ReflectionClass", &metadata)
            },
        )?;

        if module.enum_infos.contains_key(class_name) {
            let label = reflection_owner_factory_label("ReflectionEnum", class_name, false);
            emit_shared_helper(
                module,
                emitter,
                data,
                shared,
                regalloc_linear,
                &label,
                PhpType::Mixed,
                &format!("--- shared ReflectionEnum factory: {} ---", class_name),
                |ctx| {
                    let metadata = reflection_enum_metadata_for_name(ctx, class_name)?;
                    emit_reflection_owner_object(ctx, "ReflectionEnum", &metadata)
                },
            )?;
        }
    }

    let mut function_names = crate::internal_extensions::registry()
        .function_names()
        .collect::<Vec<_>>();
    function_names.sort_unstable_by_key(|name| php_symbol_key(name));
    for function_name in function_names {
        // Factory calls resolve the function through `php_symbol_key` before they
        // select the internal signature.  Keep the label on that same key while
        // passing the exported spelling into metadata generation, which preserves
        // ReflectionFunction's PHP-visible declaration case.
        let function_key = php_symbol_key(function_name);
        let label = reflection_owner_factory_label("ReflectionFunction", &function_key, false);
        emit_shared_helper(
            module,
            emitter,
            data,
            shared,
            regalloc_linear,
            &label,
            PhpType::Mixed,
            &format!("--- shared ReflectionFunction factory: {} ---", function_name),
            |ctx| {
                let metadata = reflection_function_metadata_for_name(ctx, function_name)?;
                emit_reflection_owner_object(ctx, "ReflectionFunction", &metadata)?;
                let extension_name = reflection_extension_name_for_function(function_name)
                    .ok_or_else(|| {
                        CodegenIrError::unsupported(format!(
                            "shared ReflectionFunction factory missing extension for {}",
                            function_name
                        ))
                    })?;
                let string = reflection_internal_function_to_string(&metadata, extension_name);
                emit_reflection_owner_string_property_by_name(
                    ctx,
                    "ReflectionFunction",
                    "__string",
                    &string,
                )
            },
        )?;
    }
    Ok(())
}
