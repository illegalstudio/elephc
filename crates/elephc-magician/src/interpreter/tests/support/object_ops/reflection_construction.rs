//! Purpose:
//! Builds fake ReflectionAttribute and reflection owner/member objects for
//! interpreter tests.
//!
//! Called from:
//! - Reflection-related `RuntimeValueOps` hooks on `FakeOps`.
//!
//! Key details:
//! - Object layouts mirror the metadata consumed by eval Reflection builtins.

use super::*;

impl FakeOps {

    /// Materializes one fake populated `ReflectionAttribute` object.
    pub(in crate::interpreter::tests::support) fn runtime_reflection_attribute_new(
        &mut self,
        name: &str,
        args: RuntimeCellHandle,
        target: u64,
        repeated: bool,
    ) -> Result<RuntimeCellHandle, EvalStatus> {
        let name = self.string(name)?;
        let factory = self.int(0)?;
        let target = self.int(target as i64)?;
        let repeated = self.bool_value(repeated)?;
        let object = self.alloc(FakeValue::Object(vec![
            ("__name".to_string(), name),
            ("__args".to_string(), args),
            ("__factory".to_string(), factory),
            ("__target".to_string(), target),
            ("__is_repeated".to_string(), repeated),
        ]));
        self.object_classes
            .insert(object.as_ptr() as usize, "ReflectionAttribute".to_string());
        Ok(object)
    }
    /// Materializes one fake populated Reflection owner object.
    pub(in crate::interpreter::tests::support) fn runtime_reflection_owner_new(
        &mut self,
        owner_kind: u64,
        reflected_name: &str,
        attrs: RuntimeCellHandle,
        interface_names: RuntimeCellHandle,
        trait_names: RuntimeCellHandle,
        method_names: RuntimeCellHandle,
        property_names: RuntimeCellHandle,
        method_objects: RuntimeCellHandle,
        property_objects: RuntimeCellHandle,
        parent_class: RuntimeCellHandle,
        flags: u64,
        modifiers: u64,
        method_modifiers: u64,
        constant_value: RuntimeCellHandle,
        backing_value: RuntimeCellHandle,
        constructor: RuntimeCellHandle,
    ) -> Result<RuntimeCellHandle, EvalStatus> {
        let class_name = match owner_kind {
            EVAL_REFLECTION_OWNER_CLASS => "ReflectionClass",
            EVAL_REFLECTION_OWNER_OBJECT => "ReflectionObject",
            EVAL_REFLECTION_OWNER_ENUM => "ReflectionEnum",
            EVAL_REFLECTION_OWNER_FUNCTION => "ReflectionFunction",
            EVAL_REFLECTION_OWNER_METHOD => "ReflectionMethod",
            EVAL_REFLECTION_OWNER_PROPERTY => "ReflectionProperty",
            EVAL_REFLECTION_OWNER_CLASS_CONSTANT => "ReflectionClassConstant",
            EVAL_REFLECTION_OWNER_ENUM_UNIT_CASE => "ReflectionEnumUnitCase",
            EVAL_REFLECTION_OWNER_ENUM_BACKED_CASE => "ReflectionEnumBackedCase",
            EVAL_REFLECTION_OWNER_PARAMETER => "ReflectionParameter",
            EVAL_REFLECTION_OWNER_NAMED_TYPE => "ReflectionNamedType",
            EVAL_REFLECTION_OWNER_UNION_TYPE => "ReflectionUnionType",
            EVAL_REFLECTION_OWNER_INTERSECTION_TYPE => "ReflectionIntersectionType",
            _ => return Err(EvalStatus::RuntimeFatal),
        };
        let name = self.string(reflected_name)?;
        let is_final = self.bool_value((flags & 1) != 0)?;
        let is_abstract = self.bool_value((flags & 2) != 0)?;
        let is_interface = self.bool_value((flags & 4) != 0)?;
        let is_trait = self.bool_value((flags & 8) != 0)?;
        let is_enum = self.bool_value((flags & 16) != 0)?;
        let is_readonly = self.bool_value((flags & 32) != 0)?;
        let is_instantiable = self.bool_value((flags & 64) != 0)?;
        let is_cloneable = self.bool_value((flags & 128) != 0)?;
        let is_internal = self.bool_value((flags & 256) != 0)?;
        let is_user_defined = self.bool_value((flags & 512) != 0)?;
        let is_iterable = self.bool_value((flags & 1024) != 0)?;
        let is_anonymous = self.bool_value((flags & 2048) != 0)?;
        let modifiers_cell = self.int(modifiers as i64)?;
        let mut properties = vec![("__name".to_string(), name), ("__attrs".to_string(), attrs)];
        if matches!(
            owner_kind,
            EVAL_REFLECTION_OWNER_CLASS
                | EVAL_REFLECTION_OWNER_OBJECT
                | EVAL_REFLECTION_OWNER_ENUM
        ) {
            let (namespace_name, short_name) = reflection_name_parts(reflected_name);
            let has_namespace = !namespace_name.is_empty();
            let namespace_name = self.string(namespace_name)?;
            let short_name = self.string(short_name)?;
            let in_namespace = self.bool_value(has_namespace)?;
            let constant_names = self.runtime_array_new(0)?;
            let constants = self.runtime_assoc_new(0)?;
            let reflection_constants = self.runtime_array_new(0)?;
            properties.push(("__is_final".to_string(), is_final));
            properties.push(("__is_abstract".to_string(), is_abstract));
            properties.push(("__is_interface".to_string(), is_interface));
            properties.push(("__is_trait".to_string(), is_trait));
            properties.push(("__is_enum".to_string(), is_enum));
            properties.push(("__is_readonly".to_string(), is_readonly));
            properties.push(("__is_anonymous".to_string(), is_anonymous));
            properties.push(("__is_instantiable".to_string(), is_instantiable));
            properties.push(("__is_cloneable".to_string(), is_cloneable));
            properties.push(("__is_iterable".to_string(), is_iterable));
            properties.push(("__is_internal".to_string(), is_internal));
            properties.push(("__is_user_defined".to_string(), is_user_defined));
            properties.push(("__modifiers".to_string(), modifiers_cell));
            properties.push(("__short_name".to_string(), short_name));
            properties.push(("__namespace_name".to_string(), namespace_name));
            properties.push(("__in_namespace".to_string(), in_namespace));
            properties.push(("__interface_names".to_string(), interface_names));
            properties.push(("__trait_names".to_string(), trait_names));
            properties.push(("__method_names".to_string(), method_names));
            properties.push(("__property_names".to_string(), property_names));
            properties.push(("__constant_names".to_string(), constant_names));
            properties.push(("__constants".to_string(), constants));
            properties.push(("__reflection_constants".to_string(), reflection_constants));
            properties.push(("__methods".to_string(), method_objects));
            properties.push(("__constructor".to_string(), constructor));
            properties.push(("__parent_class".to_string(), parent_class));
            properties.push(("__properties".to_string(), property_objects));
        }
        if owner_kind == EVAL_REFLECTION_OWNER_METHOD
            || owner_kind == EVAL_REFLECTION_OWNER_PROPERTY
        {
            let is_static = self.bool_value((flags & EVAL_REFLECTION_MEMBER_FLAG_STATIC) != 0)?;
            let is_public = self.bool_value((flags & EVAL_REFLECTION_MEMBER_FLAG_PUBLIC) != 0)?;
            let is_protected =
                self.bool_value((flags & EVAL_REFLECTION_MEMBER_FLAG_PROTECTED) != 0)?;
            let is_private = self.bool_value((flags & EVAL_REFLECTION_MEMBER_FLAG_PRIVATE) != 0)?;
            properties.push(("__is_static".to_string(), is_static));
            properties.push(("__is_public".to_string(), is_public));
            properties.push(("__is_protected".to_string(), is_protected));
            properties.push(("__is_private".to_string(), is_private));
            if owner_kind == EVAL_REFLECTION_OWNER_PROPERTY {
                let is_final = self.bool_value((flags & EVAL_REFLECTION_MEMBER_FLAG_FINAL) != 0)?;
                let is_abstract =
                    self.bool_value((flags & EVAL_REFLECTION_MEMBER_FLAG_ABSTRACT) != 0)?;
                let is_readonly =
                    self.bool_value((flags & EVAL_REFLECTION_MEMBER_FLAG_READONLY) != 0)?;
                let has_default_value =
                    self.bool_value((flags & EVAL_REFLECTION_MEMBER_FLAG_HAS_DEFAULT_VALUE) != 0)?;
                let is_promoted =
                    self.bool_value((flags & EVAL_REFLECTION_MEMBER_FLAG_PROMOTED) != 0)?;
                let is_virtual =
                    self.bool_value((flags & EVAL_REFLECTION_MEMBER_FLAG_VIRTUAL) != 0)?;
                let is_dynamic =
                    self.bool_value((flags & EVAL_REFLECTION_MEMBER_FLAG_DYNAMIC) != 0)?;
                properties.push(("__is_final".to_string(), is_final));
                properties.push(("__is_abstract".to_string(), is_abstract));
                properties.push(("__is_readonly".to_string(), is_readonly));
                properties.push(("__modifiers".to_string(), modifiers_cell));
                properties.push(("__type".to_string(), method_objects));
                properties.push(("__settable_type".to_string(), constant_value));
                properties.push(("__has_default_value".to_string(), has_default_value));
                properties.push(("__is_promoted".to_string(), is_promoted));
                properties.push(("__is_virtual".to_string(), is_virtual));
                properties.push(("__is_dynamic".to_string(), is_dynamic));
                properties.push(("__default_value".to_string(), property_objects));
            }
        }
        if matches!(
            owner_kind,
            EVAL_REFLECTION_OWNER_METHOD
                | EVAL_REFLECTION_OWNER_PROPERTY
                | EVAL_REFLECTION_OWNER_CLASS_CONSTANT
                | EVAL_REFLECTION_OWNER_ENUM_UNIT_CASE
                | EVAL_REFLECTION_OWNER_ENUM_BACKED_CASE
                | EVAL_REFLECTION_OWNER_PARAMETER
        ) {
            properties.push(("__declaring_class".to_string(), parent_class));
        }
        if matches!(
            owner_kind,
            EVAL_REFLECTION_OWNER_METHOD | EVAL_REFLECTION_OWNER_FUNCTION
        ) {
            let is_deprecated =
                self.bool_value((flags & EVAL_REFLECTION_CALLABLE_FLAG_DEPRECATED) != 0)?;
            properties.push(("__parameters".to_string(), method_objects));
            properties.push(("__required_parameter_count".to_string(), modifiers_cell));
            properties.push(("__is_deprecated".to_string(), is_deprecated));
        }
        if owner_kind == EVAL_REFLECTION_OWNER_METHOD {
            let is_final = self.bool_value((flags & EVAL_REFLECTION_MEMBER_FLAG_FINAL) != 0)?;
            let is_abstract =
                self.bool_value((flags & EVAL_REFLECTION_MEMBER_FLAG_ABSTRACT) != 0)?;
            let method_modifiers = self.int(method_modifiers as i64)?;
            properties.push(("__is_final".to_string(), is_final));
            properties.push(("__is_abstract".to_string(), is_abstract));
            properties.push(("__modifiers".to_string(), method_modifiers));
        }
        if matches!(
            owner_kind,
            EVAL_REFLECTION_OWNER_CLASS_CONSTANT
                | EVAL_REFLECTION_OWNER_ENUM_UNIT_CASE
                | EVAL_REFLECTION_OWNER_ENUM_BACKED_CASE
        ) {
            let is_public = self.bool_value((flags & EVAL_REFLECTION_MEMBER_FLAG_PUBLIC) != 0)?;
            let is_protected =
                self.bool_value((flags & EVAL_REFLECTION_MEMBER_FLAG_PROTECTED) != 0)?;
            let is_private = self.bool_value((flags & EVAL_REFLECTION_MEMBER_FLAG_PRIVATE) != 0)?;
            let is_final = self.bool_value((flags & EVAL_REFLECTION_MEMBER_FLAG_FINAL) != 0)?;
            let is_enum_case =
                self.bool_value((flags & EVAL_REFLECTION_MEMBER_FLAG_ENUM_CASE) != 0)?;
            properties.push(("__is_public".to_string(), is_public));
            properties.push(("__is_protected".to_string(), is_protected));
            properties.push(("__is_private".to_string(), is_private));
            properties.push(("__is_final".to_string(), is_final));
            properties.push(("__is_enum_case".to_string(), is_enum_case));
            properties.push(("__modifiers".to_string(), modifiers_cell));
        }
        if owner_kind == EVAL_REFLECTION_OWNER_CLASS_CONSTANT {
            properties.push(("__value".to_string(), constant_value));
        }
        if matches!(
            owner_kind,
            EVAL_REFLECTION_OWNER_ENUM_UNIT_CASE | EVAL_REFLECTION_OWNER_ENUM_BACKED_CASE
        ) {
            properties.push(("__value".to_string(), constant_value));
        }
        if owner_kind == EVAL_REFLECTION_OWNER_ENUM_BACKED_CASE {
            properties.push(("__backing_value".to_string(), backing_value));
        }
        if owner_kind == EVAL_REFLECTION_OWNER_PARAMETER {
            let position = self.int(modifiers as i64)?;
            let is_optional =
                self.bool_value((flags & EVAL_REFLECTION_PARAMETER_FLAG_OPTIONAL) != 0)?;
            let is_variadic =
                self.bool_value((flags & EVAL_REFLECTION_PARAMETER_FLAG_VARIADIC) != 0)?;
            let is_passed_by_reference =
                self.bool_value((flags & EVAL_REFLECTION_PARAMETER_FLAG_BY_REF) != 0)?;
            let has_type =
                self.bool_value((flags & EVAL_REFLECTION_PARAMETER_FLAG_HAS_TYPE) != 0)?;
            let has_default_value =
                self.bool_value((flags & EVAL_REFLECTION_PARAMETER_FLAG_HAS_DEFAULT_VALUE) != 0)?;
            let is_promoted =
                self.bool_value((flags & EVAL_REFLECTION_PARAMETER_FLAG_PROMOTED) != 0)?;
            let allows_null =
                self.bool_value((flags & EVAL_REFLECTION_PARAMETER_FLAG_ALLOWS_NULL) != 0)?;
            let is_default_value_constant = self
                .bool_value((flags & EVAL_REFLECTION_PARAMETER_FLAG_DEFAULT_VALUE_CONSTANT) != 0)?;
            let is_array_type =
                self.bool_value((flags & EVAL_REFLECTION_PARAMETER_FLAG_ARRAY_TYPE) != 0)?;
            let is_callable_type =
                self.bool_value((flags & EVAL_REFLECTION_PARAMETER_FLAG_CALLABLE_TYPE) != 0)?;
            let class_value = self.reflection_parameter_class_value(method_objects)?;
            properties.push(("__position".to_string(), position));
            properties.push(("__is_optional".to_string(), is_optional));
            properties.push(("__is_variadic".to_string(), is_variadic));
            properties.push((
                "__is_passed_by_reference".to_string(),
                is_passed_by_reference,
            ));
            properties.push(("__is_promoted".to_string(), is_promoted));
            properties.push(("__has_type".to_string(), has_type));
            properties.push(("__allows_null".to_string(), allows_null));
            properties.push(("__is_array_type".to_string(), is_array_type));
            properties.push(("__is_callable_type".to_string(), is_callable_type));
            properties.push(("__type".to_string(), method_objects));
            properties.push(("__class".to_string(), class_value));
            properties.push(("__has_default_value".to_string(), has_default_value));
            properties.push((
                "__is_default_value_constant".to_string(),
                is_default_value_constant,
            ));
            properties.push(("__default_value_constant_name".to_string(), constant_value));
            properties.push(("__default_value".to_string(), property_objects));
            properties.push(("__declaring_function".to_string(), interface_names));
        }
        if owner_kind == EVAL_REFLECTION_OWNER_NAMED_TYPE {
            let allows_null = self.bool_value((flags & 1) != 0)?;
            let is_builtin = self.bool_value((flags & 2) != 0)?;
            properties.push(("__allows_null".to_string(), allows_null));
            properties.push(("__is_builtin".to_string(), is_builtin));
        }
        if owner_kind == EVAL_REFLECTION_OWNER_UNION_TYPE {
            let allows_null = self.bool_value((flags & 1) != 0)?;
            properties.push(("__types".to_string(), method_objects));
            properties.push(("__allows_null".to_string(), allows_null));
        }
        if owner_kind == EVAL_REFLECTION_OWNER_INTERSECTION_TYPE {
            let allows_null = self.bool_value(false)?;
            properties.push(("__types".to_string(), method_objects));
            properties.push(("__allows_null".to_string(), allows_null));
        }
        let object = self.alloc(FakeValue::Object(properties));
        self.object_classes
            .insert(object.as_ptr() as usize, class_name.to_string());
        Ok(object)
    }

}
