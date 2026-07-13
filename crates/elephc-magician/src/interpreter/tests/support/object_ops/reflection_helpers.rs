//! Purpose:
//! Shared fake reflection value, modifier, type-formatting, and named-member
//! helpers.
//!
//! Called from:
//! - Fake reflection owner construction and metadata lookup modules.
//!
//! Key details:
//! - Missing members retain the false-versus-fatal behavior expected by PHP APIs.

use super::*;

impl FakeOps {

    /// Builds the fake `ReflectionParameter::getClass()` value from a named non-builtin type.
    pub(super) fn reflection_parameter_class_value(
        &mut self,
        type_value: RuntimeCellHandle,
    ) -> Result<RuntimeCellHandle, EvalStatus> {
        if !self
            .object_classes
            .get(&(type_value.as_ptr() as usize))
            .is_some_and(|class_name| class_name == "ReflectionNamedType")
        {
            return self.null();
        }
        let FakeValue::Object(type_properties) = self.get(type_value) else {
            return self.null();
        };
        let is_builtin = Self::object_property(&type_properties, "__is_builtin")
            .is_some_and(|value| matches!(self.get(value), FakeValue::Bool(true)));
        if is_builtin {
            return self.null();
        }
        let Some(name) = Self::object_property(&type_properties, "__name") else {
            return self.null();
        };
        let FakeValue::String(name) = self.get(name) else {
            return self.null();
        };
        self.fake_reflection_class_object(&name)
    }

    /// Checks whether a private fake object array property contains one string.
    pub(super) fn object_string_array_contains(
        &mut self,
        properties: &[(String, RuntimeCellHandle)],
        property: &str,
        needle: RuntimeCellHandle,
        case_insensitive: bool,
    ) -> Result<RuntimeCellHandle, EvalStatus> {
        let FakeValue::String(mut needle) = self.get(needle) else {
            return Err(EvalStatus::UnsupportedConstruct);
        };
        if case_insensitive {
            needle = needle.to_ascii_lowercase();
        }
        let Some(array) = Self::object_property(properties, property) else {
            return self.bool_value(false);
        };
        let contains = match self.get(array) {
            FakeValue::Array(elements) => elements.iter().any(|element| match self.get(*element) {
                FakeValue::String(value) if case_insensitive => {
                    value.to_ascii_lowercase() == needle
                }
                FakeValue::String(value) => value == needle,
                _ => false,
            }),
            _ => false,
        };
        self.bool_value(contains)
    }

    /// Returns whether a fake Reflection owner stores one modifier bit.
    pub(super) fn reflection_modifier_mask(
        &mut self,
        properties: &[(String, RuntimeCellHandle)],
        mask: i64,
    ) -> Result<RuntimeCellHandle, EvalStatus> {
        let modifiers = Self::object_property(properties, "__modifiers")
            .map(|handle| self.fake_int(&self.get(handle)))
            .unwrap_or(0);
        self.bool_value((modifiers & mask) != 0)
    }

    /// Builds a name-keyed fake ReflectionClass map from a private string-array property.
    pub(super) fn object_relation_reflection_classes(
        &mut self,
        properties: &[(String, RuntimeCellHandle)],
        property: &str,
    ) -> Result<RuntimeCellHandle, EvalStatus> {
        let result = self.runtime_assoc_new(0)?;
        let Some(array) = Self::object_property(properties, property) else {
            return Ok(result);
        };
        let FakeValue::Array(elements) = self.get(array) else {
            return Err(EvalStatus::UnsupportedConstruct);
        };
        for element in elements {
            let FakeValue::String(name) = self.get(element) else {
                return Err(EvalStatus::UnsupportedConstruct);
            };
            let key = self.runtime_string(&name)?;
            let object = self.fake_reflection_class_object(&name)?;
            self.runtime_array_set(result, key, object)?;
        }
        Ok(result)
    }

    /// Builds a minimal fake ReflectionClass object with a working `getName()` slot.
    pub(super) fn fake_reflection_class_object(
        &mut self,
        class_name: &str,
    ) -> Result<RuntimeCellHandle, EvalStatus> {
        let name = self.string(class_name)?;
        let object = self.alloc(FakeValue::Object(vec![("__name".to_string(), name)]));
        self.object_classes
            .insert(object.as_ptr() as usize, "ReflectionClass".to_string());
        Ok(object)
    }

    /// Formats fake ReflectionType objects through their synthetic `__toString()` method.
    pub(super) fn reflection_type_to_string(
        &mut self,
        class_name: Option<&str>,
        properties: &[(String, RuntimeCellHandle)],
    ) -> Result<RuntimeCellHandle, EvalStatus> {
        match class_name {
            Some("ReflectionNamedType") => self.reflection_named_type_to_string(properties),
            Some("ReflectionUnionType") => self.reflection_composite_type_to_string(
                properties,
                "|",
                true,
            ),
            Some("ReflectionIntersectionType") => self.reflection_composite_type_to_string(
                properties,
                "&",
                false,
            ),
            _ => Err(EvalStatus::UnsupportedConstruct),
        }
    }

    /// Formats one fake ReflectionNamedType object using retained name/nullability slots.
    pub(super) fn reflection_named_type_to_string(
        &mut self,
        properties: &[(String, RuntimeCellHandle)],
    ) -> Result<RuntimeCellHandle, EvalStatus> {
        let Some(name) = Self::object_property(properties, "__name") else {
            return self.string("");
        };
        let FakeValue::String(name) = self.get(name) else {
            return Err(EvalStatus::UnsupportedConstruct);
        };
        let allows_null = Self::object_property(properties, "__allows_null")
            .is_some_and(|value| matches!(self.get(value), FakeValue::Bool(true)));
        if allows_null && name != "mixed" {
            self.string(&format!("?{name}"))
        } else {
            self.string(&name)
        }
    }

    /// Formats one fake ReflectionUnionType or ReflectionIntersectionType object.
    pub(super) fn reflection_composite_type_to_string(
        &mut self,
        properties: &[(String, RuntimeCellHandle)],
        separator: &str,
        append_null: bool,
    ) -> Result<RuntimeCellHandle, EvalStatus> {
        let mut names = Vec::new();
        if let Some(types) = Self::object_property(properties, "__types") {
            let FakeValue::Array(elements) = self.get(types) else {
                return Err(EvalStatus::UnsupportedConstruct);
            };
            for element in elements {
                let FakeValue::Object(type_properties) = self.get(element) else {
                    return Err(EvalStatus::UnsupportedConstruct);
                };
                let Some(name) = Self::object_property(&type_properties, "__name") else {
                    return Err(EvalStatus::UnsupportedConstruct);
                };
                let FakeValue::String(name) = self.get(name) else {
                    return Err(EvalStatus::UnsupportedConstruct);
                };
                names.push(name);
            }
        }
        let allows_null = Self::object_property(properties, "__allows_null")
            .is_some_and(|value| matches!(self.get(value), FakeValue::Bool(true)));
        if append_null && allows_null {
            names.push("null".to_string());
        }
        self.string(&names.join(separator))
    }

    /// Finds one fake ReflectionMethod/ReflectionProperty object by its private name slot.
    pub(super) fn object_named_member(
        &mut self,
        properties: &[(String, RuntimeCellHandle)],
        property: &str,
        needle: RuntimeCellHandle,
        case_insensitive: bool,
    ) -> Result<RuntimeCellHandle, EvalStatus> {
        let FakeValue::String(mut needle) = self.get(needle) else {
            return Err(EvalStatus::UnsupportedConstruct);
        };
        if case_insensitive {
            needle = needle.to_ascii_lowercase();
        }
        let Some(array) = Self::object_property(properties, property) else {
            return Err(EvalStatus::RuntimeFatal);
        };
        let FakeValue::Array(elements) = self.get(array) else {
            return Err(EvalStatus::UnsupportedConstruct);
        };
        for element in elements {
            let FakeValue::Object(member_properties) = self.get(element) else {
                continue;
            };
            let Some(name) = Self::object_property(&member_properties, "__name") else {
                continue;
            };
            let FakeValue::String(mut name) = self.get(name) else {
                continue;
            };
            if case_insensitive {
                name = name.to_ascii_lowercase();
            }
            if name == needle {
                return Ok(element);
            }
        }
        Err(EvalStatus::RuntimeFatal)
    }

    /// Finds one fake reflection member by name, returning PHP `false` when absent.
    pub(super) fn object_named_member_or_false(
        &mut self,
        properties: &[(String, RuntimeCellHandle)],
        property: &str,
        needle: RuntimeCellHandle,
        case_insensitive: bool,
    ) -> Result<RuntimeCellHandle, EvalStatus> {
        match self.object_named_member(properties, property, needle, case_insensitive) {
            Ok(member) => Ok(member),
            Err(EvalStatus::RuntimeFatal) => self.bool_value(false),
            Err(status) => Err(status),
        }
    }

}
