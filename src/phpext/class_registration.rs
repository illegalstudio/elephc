//! Purpose:
//! Generates host-owned class, method, property and class-constant registration.
//!
//! Called from:
//! - The real-SDK producer adapter generator after export metadata validation.
//!
//! Key details:
//! - Zend owns class metadata and persistent default values.
//! - Instance storage and method invocation remain connected to the native object registry.

use crate::{parser::ast::{TypeExpr, Visibility}, types::PhpType};
use super::{classes::ExportedClass, class_helpers::ObjectOperation, producer::c_string};

/// Declares class entries early enough for handler argument checks and object callbacks.
pub fn prelude(classes: &[ExportedClass]) -> String {
    if classes.is_empty() { return String::new(); }
    let mut text = format!("#define ELEPHC_PHPEXT_OBJECTS 1\n{}\nstatic zend_class_entry *ep_class_entries[{}];\n",
        include_str!("objects.h"), classes.len());
    if classes.iter().any(|class| class.property_defaults.iter().flatten().chain(class.constants.iter().map(|constant| &constant.value))
        .any(|value| matches!(value, super::constants::ConstantValue::Array(_)))) {
        text.push_str(super::constant_values::support());
    }
    for helper in classes.iter().flat_map(|class| &class.helpers) {
        text.push_str(&format!("extern void {}(const zval *const *);\n", helper.export.c_name));
    }
    text
}

/// Registers each real internal class and its public surface using the selected PHP headers.
pub fn render(classes: &[ExportedClass]) -> Result<String, String> {
    let mut text = String::new();
    for (index, class) in classes.iter().enumerate() {
        text.push_str(&format!("static const zend_function_entry ep_methods_{index}[] = {{\n"));
        for method in &class.methods {
            let mut flags = "ZEND_ACC_PUBLIC".to_string();
            if method.is_static { flags.push_str(" | ZEND_ACC_STATIC"); }
            if method.is_final { flags.push_str(" | ZEND_ACC_FINAL"); }
            text.push_str(&format!("    ZEND_RAW_FENTRY({}, zif_{}, arginfo_{}, {flags}, NULL, NULL)\n",
                c_string(method.name.as_bytes()), method.export.c_name, method.export.c_name));
        }
        text.push_str("    ZEND_FE_END\n};\n");
        text.push_str(&format!("static ep_property_descriptor ep_properties_{index}[{}] = {{\n", class.info.properties.len().max(1)));
        if class.info.properties.is_empty() { text.push_str("    {0, NULL, NULL, NULL, NULL},\n"); }
        for property in 0..class.info.properties.len() {
            let lookup = |predicate: fn(usize) -> ObjectOperation| class.helpers.iter()
                .find(|helper| std::mem::discriminant(&helper.operation) == std::mem::discriminant(&predicate(property))
                    && match helper.operation { ObjectOperation::Read(index) | ObjectOperation::Write(index)
                        | ObjectOperation::Unset(index) | ObjectOperation::Initialized(index) => index == property, _ => false })
                .map(|helper| helper.export.c_name.as_str()).expect("all property operations described");
            text.push_str(&format!("    {{0, {}, {}, {}, {}}},\n", lookup(ObjectOperation::Read),
                lookup(ObjectOperation::Write), lookup(ObjectOperation::Unset), lookup(ObjectOperation::Initialized)));
        }
        text.push_str("};\n");
    }
    text.push_str(&format!("static ep_class_descriptor ep_classes[{}] = {{\n", classes.len().max(1)));
    for (index, class) in classes.iter().enumerate() {
        let helper = |operation| class.helpers.iter().find(|helper| std::mem::discriminant(&helper.operation)
            == std::mem::discriminant(&operation)).map(|helper| helper.export.c_name.as_str()).unwrap();
        text.push_str(&format!("    {{NULL, UINT64_C({}), {}, ep_properties_{index}, {}, {}, {}, {}}},\n",
            class.info.class_id, class.info.properties.len(), helper(ObjectOperation::Allocate),
            helper(ObjectOperation::Release), helper(ObjectOperation::PublishRoots), helper(ObjectOperation::Collect)));
    }
    text.push_str("};\nstatic zend_result ep_register_classes(void)\n{\n");
    for class in classes {
        let lower = crate::names::php_symbol_key(&class.name);
        text.push_str(&format!("    if (zend_hash_str_exists(CG(class_table), {}, {})) {{ php_error_docref(NULL, E_WARNING, \"PHP extension class already defined: %s\", {}); return FAILURE; }}\n",
            c_string(lower.as_bytes()), lower.len(), c_string(class.name.as_bytes())));
    }
    text.push_str("    ep_object_handlers = *zend_get_std_object_handlers();\n    ep_object_handlers.offset = 0;\n    ep_object_handlers.free_obj = ep_object_free;\n    ep_object_handlers.clone_obj = ep_object_clone;\n");
    for (index, class) in classes.iter().enumerate() {
        text.push_str(&format!("    {{\n        zend_class_entry declaration;\n        INIT_CLASS_ENTRY_EX(declaration, {}, {}, ep_methods_{index});\n        zend_class_entry *ce = zend_register_internal_class_with_flags(&declaration, NULL, ZEND_ACC_FINAL | ZEND_ACC_NO_DYNAMIC_PROPERTIES);\n        if (!ce) {{ return FAILURE; }}\n        ep_class_entries[{index}] = ep_classes[{index}].ce = ce;\n",
            c_string(class.name.as_bytes()), class.name.len()));
        for (property, ((name, ty), default)) in class.info.properties.iter().zip(&class.property_defaults).enumerate() {
            let flags = match class.info.property_visibilities.get(name) {
                Some(Visibility::Private) => "ZEND_ACC_PRIVATE", Some(Visibility::Protected) => "ZEND_ACC_PROTECTED", _ => "ZEND_ACC_PUBLIC",
            };
            let final_flag = if class.info.final_properties.contains(name) { " | ZEND_ACC_FINAL" } else { "" };
            let promoted_flag = if class.info.promoted_properties.contains(name) { " | ZEND_ACC_PROMOTED" } else { "" };
            let initial = default.as_ref().map(|value| persistent_typed_value(value, ty, "&value"))
                .unwrap_or_else(|| "ZVAL_UNDEF(&value);".into());
            text.push_str(&format!("        {{ zval value; {initial} zend_string *name = zend_string_init({}, {}, true);\n            zend_property_info *property = zend_declare_typed_property(ce, name, &value, {flags}{final_flag}{promoted_flag}, NULL, (zend_type){});\n            zend_string_release(name);\n            if (!property) {{ return FAILURE; }}\n            ep_properties_{index}[{property}].offset = property->offset;\n        }}\n",
                c_string(name.as_bytes()), name.len(), reflection_type_initializer(ty, Some(&class.property_types[property]), false, "0")?));
        }
        for constant in &class.constants {
            let ty_for_value = constant.declared_type.as_ref().map(super::constants::declared_type).transpose()?.unwrap_or(PhpType::Mixed);
            let initial = persistent_typed_value(&constant.value, &ty_for_value, "&value");
            let ty = constant.declared_type.as_ref().map(source_type_initializer).transpose()?.unwrap_or_else(|| "ZEND_TYPE_INIT_NONE(0)".into());
            let final_flag = if constant.is_final { " | ZEND_ACC_FINAL" } else { "" };
            text.push_str(&format!("        {{ zval value; {initial} zend_string *name = zend_string_init({}, {}, true);\n            zend_class_constant *constant = zend_declare_typed_class_constant(ce, name, &value, ZEND_ACC_PUBLIC{final_flag}, NULL, (zend_type){ty});\n            zend_string_release(name);\n            if (!constant) {{ return FAILURE; }}\n        }}\n", c_string(constant.name.as_bytes()), constant.name.len()));
        }
        text.push_str("    }\n");
    }
    text.push_str("    return SUCCESS;\n}\n");
    Ok(text)
}

/// Emits literal arginfo names or owned persistent Zend names for one boundary type.
pub fn type_initializer(ty: &PhpType, arginfo: bool) -> Result<String, String> {
    type_initializer_with_flags(ty, arginfo, "0")
}

/// Keeps argument metadata flags separate from the PHP-visible type mask.
fn type_initializer_with_flags(ty: &PhpType, arginfo: bool, flags: &str) -> Result<String, String> {
    let (base, nullable) = super::signature::parameter_type(ty).unwrap_or_else(|_| (ty.clone(), false));
    if let PhpType::Object(name) = &base {
        let name = name.trim_start_matches('\\');
        let encoded = c_string(name.as_bytes());
        return Ok(if arginfo { format!("ZEND_TYPE_INIT_CLASS_CONST({encoded}, {}, {flags})", u8::from(nullable)) }
            else { format!("ZEND_TYPE_INIT_CLASS(zend_string_init({encoded}, {}, true), {}, {flags})", name.len(), u8::from(nullable)) });
    }
    Ok(format!("ZEND_TYPE_INIT_MASK(({}) | ({flags}))", super::producer::mask(ty)?))
}

/// Uses the same resolved constant contract for Reflection and persistent default coercion.
fn source_type_initializer(ty: &TypeExpr) -> Result<String, String> {
    type_initializer(&super::constants::declared_type(ty)?, false)
}

/// Preserves lexical self/static names while physical invocation uses the resolved class.
pub(super) fn reflection_type_initializer(ty: &PhpType, source: Option<&TypeExpr>, arginfo: bool, flags: &str) -> Result<String, String> {
    let mut source = source;
    let nullable = matches!(ty, PhpType::Union(members) if members.contains(&PhpType::Void));
    if let Some(TypeExpr::Nullable(inner)) = source { source = Some(inner); }
    if let Some(TypeExpr::Union(members)) = source {
        let mut non_null = members.iter().filter(|member| !matches!(member,
            TypeExpr::Named(name) if name.as_str().eq_ignore_ascii_case("null")));
        source = non_null.next().filter(|_| non_null.next().is_none());
    }
    if let Some(TypeExpr::Named(name)) = source {
        if name.as_str().eq_ignore_ascii_case("static") {
            return Ok(format!("ZEND_TYPE_INIT_MASK(MAY_BE_STATIC{} | ({flags}))", if nullable { " | MAY_BE_NULL" } else { "" }));
        }
        if name.as_str().eq_ignore_ascii_case("self") {
            let encoded = c_string(b"self");
            return Ok(if arginfo { format!("ZEND_TYPE_INIT_CLASS_CONST({encoded}, {}, {flags})", u8::from(nullable)) }
                else { format!("ZEND_TYPE_INIT_CLASS(zend_string_init({encoded}, 4, true), {}, {flags})", u8::from(nullable)) });
        }
    }
    type_initializer_with_flags(ty, arginfo, flags)
}

/// Zend's internal metadata requires the already coerced float value for an integer default.
fn persistent_typed_value(value: &super::constants::ConstantValue, ty: &PhpType, destination: &str) -> String {
    if matches!(ty, PhpType::Float | PhpType::Union(_)) && ty.coerces_integer_to_float() {
        if let super::constants::ConstantValue::Int(value) = value {
            return super::constants::persistent_value(&super::constants::ConstantValue::Float(*value as f64), destination);
        }
    }
    super::constants::persistent_value(value, destination)
}
