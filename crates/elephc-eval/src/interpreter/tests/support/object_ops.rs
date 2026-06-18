//! Purpose:
//! Object, method, class-metadata, and identity fake runtime operations for interpreter tests.
//!
//! Called from:
//! - `crate::interpreter::tests::support::runtime_ops`.
//!
//! Key details:
//! - These helpers model only the object and class behavior needed by eval tests.

use super::*;

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
            (FakeValue::Object(properties), "getinterfacenames") if args.is_empty() => {
                Self::object_property(&properties, "__interface_names")
                    .map_or_else(|| self.runtime_array_new(0), Ok)
            }
            (FakeValue::Object(properties), "gettraitnames") if args.is_empty() => {
                Self::object_property(&properties, "__trait_names")
                    .map_or_else(|| self.runtime_array_new(0), Ok)
            }
            (FakeValue::Object(properties), "getarguments") if args.is_empty() => {
                Self::object_property(&properties, "__args")
                    .map_or_else(|| self.runtime_array_new(0), Ok)
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
        flags: u64,
    ) -> Result<RuntimeCellHandle, EvalStatus> {
        let class_name = match owner_kind {
            EVAL_REFLECTION_OWNER_CLASS => "ReflectionClass",
            EVAL_REFLECTION_OWNER_METHOD => "ReflectionMethod",
            EVAL_REFLECTION_OWNER_PROPERTY => "ReflectionProperty",
            EVAL_REFLECTION_OWNER_CLASS_CONSTANT => "ReflectionClassConstant",
            EVAL_REFLECTION_OWNER_ENUM_UNIT_CASE => "ReflectionEnumUnitCase",
            EVAL_REFLECTION_OWNER_ENUM_BACKED_CASE => "ReflectionEnumBackedCase",
            _ => return Err(EvalStatus::RuntimeFatal),
        };
        let name = self.string(reflected_name)?;
        let is_final = self.bool_value((flags & 1) != 0)?;
        let is_abstract = self.bool_value((flags & 2) != 0)?;
        let is_interface = self.bool_value((flags & 4) != 0)?;
        let is_trait = self.bool_value((flags & 8) != 0)?;
        let is_enum = self.bool_value((flags & 16) != 0)?;
        let mut properties = vec![("__name".to_string(), name), ("__attrs".to_string(), attrs)];
        if owner_kind == EVAL_REFLECTION_OWNER_CLASS {
            let (namespace_name, short_name) = reflection_name_parts(reflected_name);
            let has_namespace = !namespace_name.is_empty();
            let namespace_name = self.string(namespace_name)?;
            let short_name = self.string(short_name)?;
            let in_namespace = self.bool_value(has_namespace)?;
            properties.push(("__is_final".to_string(), is_final));
            properties.push(("__is_abstract".to_string(), is_abstract));
            properties.push(("__is_interface".to_string(), is_interface));
            properties.push(("__is_trait".to_string(), is_trait));
            properties.push(("__is_enum".to_string(), is_enum));
            properties.push(("__short_name".to_string(), short_name));
            properties.push(("__namespace_name".to_string(), namespace_name));
            properties.push(("__in_namespace".to_string(), in_namespace));
            properties.push(("__interface_names".to_string(), interface_names));
            properties.push(("__trait_names".to_string(), trait_names));
        }
        let object = self.alloc(FakeValue::Object(properties));
        self.object_classes
            .insert(object.as_ptr() as usize, class_name.to_string());
        Ok(object)
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
    ["Exception", "JsonException", "Error", "ValueError"]
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
        return ["Exception", "JsonException"]
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
