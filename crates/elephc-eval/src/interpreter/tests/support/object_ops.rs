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
