//! Purpose:
//! Object, method, class-metadata, and identity fake runtime operations for interpreter tests.
//!
//! Called from:
//! - `crate::interpreter::tests::support::runtime_ops`.
//!
//! Key details:
//! - These helpers model only the object and class behavior needed by eval tests.

use super::*;

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

impl FakeOps {
    /// Reads one fake object property by name.
    pub(super) fn runtime_property_get(
        &mut self,
        object: RuntimeCellHandle,
        property: &str,
    ) -> Result<RuntimeCellHandle, EvalStatus> {
        match self.get(object) {
            FakeValue::Object(properties) => properties
                .iter()
                .find_map(|(name, value)| (name == property).then_some(*value))
                .map_or_else(|| self.null(), Ok),
            _ => Err(EvalStatus::UnsupportedConstruct),
        }
    }
    /// Writes one fake object property by name.
    pub(super) fn runtime_property_set(
        &mut self,
        object: RuntimeCellHandle,
        property: &str,
        value: RuntimeCellHandle,
    ) -> Result<(), EvalStatus> {
        let id = object.as_ptr() as usize;
        let Some(FakeValue::Object(properties)) = self.values.get_mut(&id) else {
            return Err(EvalStatus::UnsupportedConstruct);
        };
        if let Some((_, existing_value)) = properties.iter_mut().find(|(name, _)| name == property)
        {
            *existing_value = value;
        } else {
            properties.push((property.to_string(), value));
        }
        Ok(())
    }
    /// Returns the number of fake object properties in insertion order.
    pub(super) fn runtime_object_property_len(
        &mut self,
        object: RuntimeCellHandle,
    ) -> Result<usize, EvalStatus> {
        match self.get(object) {
            FakeValue::Object(properties) => Ok(properties.len()),
            FakeValue::Iterator { .. } => Ok(0),
            _ => Err(EvalStatus::UnsupportedConstruct),
        }
    }
    /// Returns one fake object property key by insertion-order position.
    pub(super) fn runtime_object_property_iter_key(
        &mut self,
        object: RuntimeCellHandle,
        position: usize,
    ) -> Result<RuntimeCellHandle, EvalStatus> {
        match self.get(object) {
            FakeValue::Object(properties) => {
                let Some((name, _)) = properties.get(position) else {
                    return self.null();
                };
                self.string(name)
            }
            _ => Err(EvalStatus::UnsupportedConstruct),
        }
    }
    /// Calls one fake object method by name.
    pub(super) fn runtime_method_call(
        &mut self,
        object: RuntimeCellHandle,
        method: &str,
        args: Vec<RuntimeCellHandle>,
    ) -> Result<RuntimeCellHandle, EvalStatus> {
        let method = method.to_ascii_lowercase();
        match (self.get(object), method.as_str()) {
            (FakeValue::Iterator { .. }, "rewind") if args.is_empty() => {
                let id = object.as_ptr() as usize;
                let Some(FakeValue::Iterator { position, .. }) = self.values.get_mut(&id) else {
                    return Err(EvalStatus::UnsupportedConstruct);
                };
                *position = 0;
                self.null()
            }
            (FakeValue::Iterator { len, position }, "valid") if args.is_empty() => {
                self.bool_value(position < len)
            }
            (FakeValue::Iterator { .. }, "next") if args.is_empty() => {
                let id = object.as_ptr() as usize;
                let Some(FakeValue::Iterator { position, .. }) = self.values.get_mut(&id) else {
                    return Err(EvalStatus::UnsupportedConstruct);
                };
                *position += 1;
                self.null()
            }
            (FakeValue::Object(_), "answer") if args.is_empty() => self.int(42),
            (FakeValue::Object(properties), "getname") if args.is_empty() => {
                Self::object_property(&properties, "__name").map_or_else(|| self.string(""), Ok)
            }
            (FakeValue::Object(properties), "getshortname") if args.is_empty() => {
                Self::object_property(&properties, "__short_name")
                    .map_or_else(|| self.string(""), Ok)
            }
            (FakeValue::Object(properties), "getnamespacename") if args.is_empty() => {
                Self::object_property(&properties, "__namespace_name")
                    .map_or_else(|| self.string(""), Ok)
            }
            (FakeValue::Object(properties), "innamespace") if args.is_empty() => {
                Self::object_property(&properties, "__in_namespace")
                    .map_or_else(|| self.bool_value(false), Ok)
            }
            (FakeValue::Object(properties), "isfinal") if args.is_empty() => {
                Self::object_property(&properties, "__is_final")
                    .map_or_else(|| self.bool_value(false), Ok)
            }
            (FakeValue::Object(properties), "isabstract") if args.is_empty() => {
                Self::object_property(&properties, "__is_abstract")
                    .map_or_else(|| self.bool_value(false), Ok)
            }
            (FakeValue::Object(properties), "isinterface") if args.is_empty() => {
                Self::object_property(&properties, "__is_interface")
                    .map_or_else(|| self.bool_value(false), Ok)
            }
            (FakeValue::Object(properties), "istrait") if args.is_empty() => {
                Self::object_property(&properties, "__is_trait")
                    .map_or_else(|| self.bool_value(false), Ok)
            }
            (FakeValue::Object(properties), "isenum") if args.is_empty() => {
                Self::object_property(&properties, "__is_enum")
                    .map_or_else(|| self.bool_value(false), Ok)
            }
            (FakeValue::Object(properties), "isreadonly") if args.is_empty() => {
                Self::object_property(&properties, "__is_readonly")
                    .map_or_else(|| self.bool_value(false), Ok)
            }
            (FakeValue::Object(properties), "isinstantiable") if args.is_empty() => {
                Self::object_property(&properties, "__is_instantiable")
                    .map_or_else(|| self.bool_value(false), Ok)
            }
            (FakeValue::Object(properties), "getparentclass") if args.is_empty() => {
                Self::object_property(&properties, "__parent_class")
                    .map_or_else(|| self.bool_value(false), Ok)
            }
            (FakeValue::Object(properties), "getmodifiers") if args.is_empty() => {
                Self::object_property(&properties, "__modifiers").map_or_else(|| self.int(0), Ok)
            }
            (FakeValue::Object(properties), "isstatic") if args.is_empty() => {
                Self::object_property(&properties, "__is_static")
                    .map_or_else(|| self.bool_value(false), Ok)
            }
            (FakeValue::Object(properties), "ispublic") if args.is_empty() => {
                Self::object_property(&properties, "__is_public")
                    .map_or_else(|| self.bool_value(false), Ok)
            }
            (FakeValue::Object(properties), "isprotected") if args.is_empty() => {
                Self::object_property(&properties, "__is_protected")
                    .map_or_else(|| self.bool_value(false), Ok)
            }
            (FakeValue::Object(properties), "isprivate") if args.is_empty() => {
                Self::object_property(&properties, "__is_private")
                    .map_or_else(|| self.bool_value(false), Ok)
            }
            (FakeValue::Object(properties), "hasmethod") if args.len() == 1 => {
                self.object_string_array_contains(&properties, "__method_names", args[0], true)
            }
            (FakeValue::Object(properties), "hasproperty") if args.len() == 1 => {
                self.object_string_array_contains(&properties, "__property_names", args[0], false)
            }
            (FakeValue::Object(properties), "hasconstant") if args.len() == 1 => {
                self.object_string_array_contains(&properties, "__constant_names", args[0], false)
            }
            (FakeValue::Object(properties), "getconstant") if args.len() == 1 => {
                let Some(constants) = Self::object_property(&properties, "__constants") else {
                    return self.bool_value(false);
                };
                let exists = self.runtime_array_key_exists(args[0], constants)?;
                if matches!(self.get(exists), FakeValue::Bool(true)) {
                    self.runtime_array_get(constants, args[0])
                } else {
                    self.bool_value(false)
                }
            }
            (FakeValue::Object(properties), "implementsinterface") if args.len() == 1 => {
                let direct = self.object_string_array_contains(
                    &properties,
                    "__interface_names",
                    args[0],
                    true,
                )?;
                if matches!(self.get(direct), FakeValue::Bool(true)) {
                    return Ok(direct);
                }
                let Some(is_interface) = Self::object_property(&properties, "__is_interface")
                else {
                    return Ok(direct);
                };
                if !matches!(self.get(is_interface), FakeValue::Bool(true)) {
                    return Ok(direct);
                }
                let Some(reflected_name) = Self::object_property(&properties, "__name") else {
                    return Ok(direct);
                };
                let FakeValue::String(reflected_name) = self.get(reflected_name) else {
                    return Ok(direct);
                };
                let FakeValue::String(interface_name) = self.get(args[0]) else {
                    return Ok(direct);
                };
                self.bool_value(reflected_name.eq_ignore_ascii_case(&interface_name))
            }
            (FakeValue::Object(properties), "getinterfacenames") if args.is_empty() => {
                Self::object_property(&properties, "__interface_names")
                    .map_or_else(|| self.runtime_array_new(0), Ok)
            }
            (FakeValue::Object(properties), "gettraitnames") if args.is_empty() => {
                Self::object_property(&properties, "__trait_names")
                    .map_or_else(|| self.runtime_array_new(0), Ok)
            }
            (FakeValue::Object(properties), "getmethods") if args.is_empty() => {
                Self::object_property(&properties, "__methods")
                    .map_or_else(|| self.runtime_array_new(0), Ok)
            }
            (FakeValue::Object(properties), "getmethod") if args.len() == 1 => {
                self.object_named_member(&properties, "__methods", args[0], true)
            }
            (FakeValue::Object(properties), "getproperties") if args.is_empty() => {
                Self::object_property(&properties, "__properties")
                    .map_or_else(|| self.runtime_array_new(0), Ok)
            }
            (FakeValue::Object(properties), "getproperty") if args.len() == 1 => {
                self.object_named_member(&properties, "__properties", args[0], false)
            }
            (FakeValue::Object(properties), "getconstants") if args.is_empty() => {
                Self::object_property(&properties, "__constants")
                    .map_or_else(|| self.runtime_assoc_new(0), Ok)
            }
            (FakeValue::Object(properties), "getarguments") if args.is_empty() => {
                Self::object_property(&properties, "__args")
                    .map_or_else(|| self.runtime_array_new(0), Ok)
            }
            (FakeValue::Object(properties), "getparameters") if args.is_empty() => {
                Self::object_property(&properties, "__parameters")
                    .map_or_else(|| self.runtime_array_new(0), Ok)
            }
            (FakeValue::Object(properties), "getnumberofparameters") if args.is_empty() => {
                match Self::object_property(&properties, "__parameters") {
                    Some(parameters) => {
                        let len = self.array_len(parameters)?;
                        self.int(len as i64)
                    }
                    None => self.int(0),
                }
            }
            (FakeValue::Object(properties), "getnumberofrequiredparameters") if args.is_empty() => {
                Self::object_property(&properties, "__required_parameter_count")
                    .map_or_else(|| self.int(0), Ok)
            }
            (FakeValue::Object(properties), "getposition") if args.is_empty() => {
                Self::object_property(&properties, "__position").map_or_else(|| self.int(0), Ok)
            }
            (FakeValue::Object(properties), "isoptional") if args.is_empty() => {
                Self::object_property(&properties, "__is_optional")
                    .map_or_else(|| self.bool_value(false), Ok)
            }
            (FakeValue::Object(properties), "isvariadic") if args.is_empty() => {
                Self::object_property(&properties, "__is_variadic")
                    .map_or_else(|| self.bool_value(false), Ok)
            }
            (FakeValue::Object(properties), "ispassedbyreference") if args.is_empty() => {
                Self::object_property(&properties, "__is_passed_by_reference")
                    .map_or_else(|| self.bool_value(false), Ok)
            }
            (FakeValue::Object(properties), "hastype") if args.is_empty() => {
                Self::object_property(&properties, "__has_type")
                    .map_or_else(|| self.bool_value(false), Ok)
            }
            (FakeValue::Object(_), "newinstance") if args.is_empty() => self.null(),
            (FakeValue::Object(properties), "getattributes") if args.is_empty() => {
                Self::object_property(&properties, "__attrs")
                    .map_or_else(|| self.runtime_array_new(0), Ok)
            }
            (FakeValue::Object(properties), "getmessage") if args.is_empty() => {
                Self::object_property(&properties, "message").map_or_else(|| self.string(""), Ok)
            }
            (FakeValue::Object(properties), "getcode") if args.is_empty() => {
                Self::object_property(&properties, "code").map_or_else(|| self.int(0), Ok)
            }
            (FakeValue::Object(properties), "read_x") => {
                if !args.is_empty() {
                    return Err(EvalStatus::UnsupportedConstruct);
                }
                Self::object_property(&properties, "x").map_or_else(|| self.null(), Ok)
            }
            (FakeValue::Object(properties), "add_x") => {
                let [arg] = args.as_slice() else {
                    return Err(EvalStatus::UnsupportedConstruct);
                };
                let x = Self::object_property(&properties, "x").ok_or(EvalStatus::RuntimeFatal)?;
                let FakeValue::Int(x) = self.get(x) else {
                    return Err(EvalStatus::UnsupportedConstruct);
                };
                let FakeValue::Int(arg) = self.get(*arg) else {
                    return Err(EvalStatus::UnsupportedConstruct);
                };
                self.int(x + arg)
            }
            (FakeValue::Object(properties), "add2_x") => {
                let [left, right] = args.as_slice() else {
                    return Err(EvalStatus::UnsupportedConstruct);
                };
                let x = Self::object_property(&properties, "x").ok_or(EvalStatus::RuntimeFatal)?;
                let FakeValue::Int(x) = self.get(x) else {
                    return Err(EvalStatus::UnsupportedConstruct);
                };
                let FakeValue::Int(left) = self.get(*left) else {
                    return Err(EvalStatus::UnsupportedConstruct);
                };
                let FakeValue::Int(right) = self.get(*right) else {
                    return Err(EvalStatus::UnsupportedConstruct);
                };
                self.int(x + left + right)
            }
            _ => Err(EvalStatus::UnsupportedConstruct),
        }
    }
    /// Calls one fake public static AOT method by class and method name.
    pub(super) fn runtime_static_method_call(
        &mut self,
        class_name: &str,
        method: &str,
        args: Vec<RuntimeCellHandle>,
    ) -> Result<RuntimeCellHandle, EvalStatus> {
        let method = method.to_ascii_lowercase();
        if !class_name.eq_ignore_ascii_case("KnownClass") {
            return Err(EvalStatus::UnsupportedConstruct);
        }
        match method.as_str() {
            "join" => {
                let [left, right] = args.as_slice() else {
                    return Err(EvalStatus::UnsupportedConstruct);
                };
                let FakeValue::String(left) = self.get(*left) else {
                    return Err(EvalStatus::UnsupportedConstruct);
                };
                let FakeValue::String(right) = self.get(*right) else {
                    return Err(EvalStatus::UnsupportedConstruct);
                };
                self.string(&format!("{}{}", left, right))
            }
            "sum" => {
                let [left, right] = args.as_slice() else {
                    return Err(EvalStatus::UnsupportedConstruct);
                };
                let FakeValue::Int(left) = self.get(*left) else {
                    return Err(EvalStatus::UnsupportedConstruct);
                };
                let FakeValue::Int(right) = self.get(*right) else {
                    return Err(EvalStatus::UnsupportedConstruct);
                };
                self.int(left + right)
            }
            _ => Err(EvalStatus::UnsupportedConstruct),
        }
    }
    /// Materializes one fake populated `ReflectionAttribute` object.
    pub(super) fn runtime_reflection_attribute_new(
        &mut self,
        name: &str,
        args: RuntimeCellHandle,
    ) -> Result<RuntimeCellHandle, EvalStatus> {
        let name = self.string(name)?;
        let factory = self.int(0)?;
        let object = self.alloc(FakeValue::Object(vec![
            ("__name".to_string(), name),
            ("__args".to_string(), args),
            ("__factory".to_string(), factory),
        ]));
        self.object_classes
            .insert(object.as_ptr() as usize, "ReflectionAttribute".to_string());
        Ok(object)
    }
    /// Materializes one fake populated Reflection owner object.
    pub(super) fn runtime_reflection_owner_new(
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
    ) -> Result<RuntimeCellHandle, EvalStatus> {
        let class_name = match owner_kind {
            EVAL_REFLECTION_OWNER_CLASS => "ReflectionClass",
            EVAL_REFLECTION_OWNER_METHOD => "ReflectionMethod",
            EVAL_REFLECTION_OWNER_PROPERTY => "ReflectionProperty",
            EVAL_REFLECTION_OWNER_CLASS_CONSTANT => "ReflectionClassConstant",
            EVAL_REFLECTION_OWNER_ENUM_UNIT_CASE => "ReflectionEnumUnitCase",
            EVAL_REFLECTION_OWNER_ENUM_BACKED_CASE => "ReflectionEnumBackedCase",
            EVAL_REFLECTION_OWNER_PARAMETER => "ReflectionParameter",
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
        let modifiers_cell = self.int(modifiers as i64)?;
        let mut properties = vec![("__name".to_string(), name), ("__attrs".to_string(), attrs)];
        if owner_kind == EVAL_REFLECTION_OWNER_CLASS {
            let (namespace_name, short_name) = reflection_name_parts(reflected_name);
            let has_namespace = !namespace_name.is_empty();
            let namespace_name = self.string(namespace_name)?;
            let short_name = self.string(short_name)?;
            let in_namespace = self.bool_value(has_namespace)?;
            let constant_names = self.runtime_array_new(0)?;
            let constants = self.runtime_assoc_new(0)?;
            properties.push(("__is_final".to_string(), is_final));
            properties.push(("__is_abstract".to_string(), is_abstract));
            properties.push(("__is_interface".to_string(), is_interface));
            properties.push(("__is_trait".to_string(), is_trait));
            properties.push(("__is_enum".to_string(), is_enum));
            properties.push(("__is_readonly".to_string(), is_readonly));
            properties.push(("__is_instantiable".to_string(), is_instantiable));
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
            properties.push(("__methods".to_string(), method_objects));
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
        }
        if owner_kind == EVAL_REFLECTION_OWNER_METHOD {
            let is_final = self.bool_value((flags & EVAL_REFLECTION_MEMBER_FLAG_FINAL) != 0)?;
            let is_abstract =
                self.bool_value((flags & EVAL_REFLECTION_MEMBER_FLAG_ABSTRACT) != 0)?;
            properties.push(("__is_final".to_string(), is_final));
            properties.push(("__is_abstract".to_string(), is_abstract));
            properties.push(("__parameters".to_string(), method_objects));
            properties.push(("__required_parameter_count".to_string(), modifiers_cell));
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
            properties.push(("__position".to_string(), position));
            properties.push(("__is_optional".to_string(), is_optional));
            properties.push(("__is_variadic".to_string(), is_variadic));
            properties.push((
                "__is_passed_by_reference".to_string(),
                is_passed_by_reference,
            ));
            properties.push(("__has_type".to_string(), has_type));
        }
        let object = self.alloc(FakeValue::Object(properties));
        self.object_classes
            .insert(object.as_ptr() as usize, class_name.to_string());
        Ok(object)
    }
    /// Checks whether a private fake object array property contains one string.
    fn object_string_array_contains(
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

    /// Finds one fake ReflectionMethod/ReflectionProperty object by its private name slot.
    fn object_named_member(
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

    /// Creates one fake object for eval `new` unit tests.
    pub(super) fn runtime_new_object(
        &mut self,
        class_name: &str,
    ) -> Result<RuntimeCellHandle, EvalStatus> {
        let object = self.alloc(FakeValue::Object(Vec::new()));
        self.object_classes
            .insert(object.as_ptr() as usize, class_name.to_string());
        Ok(object)
    }
    /// Applies fake constructor side effects for eval `new` unit tests.
    pub(super) fn runtime_construct_object(
        &mut self,
        object: RuntimeCellHandle,
        args: Vec<RuntimeCellHandle>,
    ) -> Result<(), EvalStatus> {
        let id = object.as_ptr() as usize;
        let class_name = self.object_classes.get(&id).cloned();
        let Some(FakeValue::Object(properties)) = self.values.get_mut(&id) else {
            return Err(EvalStatus::UnsupportedConstruct);
        };
        if class_name
            .as_deref()
            .is_some_and(fake_runtime_exception_like_class)
        {
            if let Some(message) = args.first().copied() {
                if let Some((_, value)) = properties.iter_mut().find(|(name, _)| name == "message")
                {
                    *value = message;
                } else {
                    properties.push(("message".to_string(), message));
                }
            }
            if let Some(code) = args.get(1).copied() {
                if let Some((_, value)) = properties.iter_mut().find(|(name, _)| name == "code") {
                    *value = code;
                } else {
                    properties.push(("code".to_string(), code));
                }
            }
            return Ok(());
        }
        if let Some(first) = args.first().copied() {
            if let Some((_, value)) = properties.iter_mut().find(|(name, _)| name == "x") {
                *value = first;
            } else {
                properties.push(("x".to_string(), first));
            }
        }
        Ok(())
    }
    /// Reports one fake AOT class for eval `class_exists` unit tests.
    pub(super) fn runtime_class_exists(&mut self, name: &str) -> Result<bool, EvalStatus> {
        Ok(name.eq_ignore_ascii_case("KnownClass"))
    }
    /// Reports one fake AOT interface for eval `interface_exists` unit tests.
    pub(super) fn runtime_interface_exists(&mut self, name: &str) -> Result<bool, EvalStatus> {
        Ok(name.eq_ignore_ascii_case("KnownInterface"))
    }
    /// Reports one fake AOT trait for eval `trait_exists` unit tests.
    pub(super) fn runtime_trait_exists(&mut self, name: &str) -> Result<bool, EvalStatus> {
        Ok(name.eq_ignore_ascii_case("KnownTrait"))
    }
    /// Reports one fake AOT enum for eval `enum_exists` unit tests.
    pub(super) fn runtime_enum_exists(&mut self, name: &str) -> Result<bool, EvalStatus> {
        Ok(name.eq_ignore_ascii_case("KnownEnum"))
    }
    /// Reports fake class relations for eval `is_a` and `is_subclass_of` unit tests.
    pub(super) fn runtime_object_is_a(
        &mut self,
        object_or_class: RuntimeCellHandle,
        target_class: &str,
        exclude_self: bool,
    ) -> Result<bool, EvalStatus> {
        let object_id = object_or_class.as_ptr() as usize;
        match self.get(object_or_class) {
            FakeValue::Object(_) if self.object_classes.contains_key(&object_id) => Ok(self
                .object_classes
                .get(&object_id)
                .is_some_and(|class_name| {
                    fake_runtime_object_is_a(class_name, target_class, exclude_self)
                })),
            FakeValue::Object(_)
                if target_class.eq_ignore_ascii_case("Exception")
                    || target_class.eq_ignore_ascii_case("Throwable") =>
            {
                Ok(!exclude_self)
            }
            FakeValue::Object(_) if target_class.eq_ignore_ascii_case("KnownClass") => {
                Ok(!exclude_self)
            }
            FakeValue::Object(_) if target_class.eq_ignore_ascii_case("ParentClass") => Ok(true),
            _ => Ok(false),
        }
    }
    /// Returns a fake PHP class name for object-tagged test values.
    pub(super) fn runtime_object_class_name(
        &mut self,
        object: RuntimeCellHandle,
    ) -> Result<RuntimeCellHandle, EvalStatus> {
        let object_id = object.as_ptr() as usize;
        if let Some(class_name) = self.object_classes.get(&object_id).cloned() {
            return self.string(&class_name);
        }
        match self.get(object) {
            FakeValue::Object(_) => self.string("stdClass"),
            FakeValue::Iterator { .. } => self.string("Iterator"),
            _ => Err(EvalStatus::RuntimeFatal),
        }
    }
    /// Returns fake parent-class names for eval introspection unit tests.
    pub(super) fn runtime_parent_class_name(
        &mut self,
        object_or_class: RuntimeCellHandle,
    ) -> Result<RuntimeCellHandle, EvalStatus> {
        match self.get(object_or_class) {
            FakeValue::Object(_) => self.string("ParentClass"),
            FakeValue::String(name) if name.eq_ignore_ascii_case("ChildClass") => {
                self.string("ParentClass")
            }
            _ => self.string(""),
        }
    }
    /// Returns the fake object handle as a stable object identity.
    pub(super) fn runtime_object_identity(
        &mut self,
        object: RuntimeCellHandle,
    ) -> Result<u64, EvalStatus> {
        match self.get(object) {
            FakeValue::Object(_) | FakeValue::Iterator { .. } => Ok(object.as_ptr() as u64),
            _ => Err(EvalStatus::RuntimeFatal),
        }
    }
}

/// Returns whether a fake runtime class stores PHP Throwable constructor state.
fn fake_runtime_exception_like_class(class_name: &str) -> bool {
    [
        "Exception",
        "JsonException",
        "ReflectionException",
        "Error",
        "ValueError",
    ]
    .iter()
    .any(|known| class_name.eq_ignore_ascii_case(known))
}

/// Splits one PHP class-like name into namespace and short-name parts.
fn reflection_name_parts(reflected_name: &str) -> (&str, &str) {
    match reflected_name.rfind('\\') {
        Some(separator) => (
            &reflected_name[..separator],
            &reflected_name[separator + 1..],
        ),
        None => ("", reflected_name),
    }
}

/// Checks the small fake Throwable inheritance graph used by eval interpreter tests.
fn fake_runtime_object_is_a(class_name: &str, target_class: &str, exclude_self: bool) -> bool {
    if class_name.eq_ignore_ascii_case(target_class) {
        return !exclude_self;
    }
    if class_name.eq_ignore_ascii_case("KnownClass")
        && target_class.eq_ignore_ascii_case("ParentClass")
    {
        return true;
    }
    if target_class.eq_ignore_ascii_case("Throwable") {
        return fake_runtime_exception_like_class(class_name);
    }
    if target_class.eq_ignore_ascii_case("Exception") {
        return ["Exception", "JsonException", "ReflectionException"]
            .iter()
            .any(|known| class_name.eq_ignore_ascii_case(known));
    }
    if target_class.eq_ignore_ascii_case("Error") {
        return ["Error", "ValueError"]
            .iter()
            .any(|known| class_name.eq_ignore_ascii_case(known));
    }
    false
}
