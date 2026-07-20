//! Purpose:
//! Fake runtime operations for object construction, class-like existence,
//! reflection metadata, inheritance checks, class names, and object identity.
//!
//! Called from:
//! - `FakeOps`'s object/class `RuntimeValueOps` methods.
//!
//! Key details:
//! - The intentionally small fake class graph is shared with relation helpers.

use super::*;

impl FakeOps {

    /// Creates one fake object for eval `new` unit tests.
    pub(in crate::interpreter::tests::support) fn runtime_new_object(
        &mut self,
        class_name: &str,
    ) -> Result<RuntimeCellHandle, EvalStatus> {
        let object = self.alloc(FakeValue::Object(Vec::new()));
        self.object_classes
            .insert(object.as_ptr() as usize, class_name.to_string());
        Ok(object)
    }
    /// Applies fake constructor side effects for eval `new` unit tests.
    pub(in crate::interpreter::tests::support) fn runtime_construct_object(
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
        if class_name
            .as_deref()
            .is_some_and(|name| name.eq_ignore_ascii_case("KnownFailingConstructor"))
        {
            return Err(EvalStatus::RuntimeFatal);
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
    pub(in crate::interpreter::tests::support) fn runtime_class_exists(&mut self, name: &str) -> Result<bool, EvalStatus> {
        Ok(name.eq_ignore_ascii_case("KnownClass")
            || name.eq_ignore_ascii_case("KnownFailingConstructor"))
    }
    /// Reports fake generated AOT ReflectionClass flags for eval metadata unit tests.
    pub(in crate::interpreter::tests::support) fn runtime_reflection_class_flags(
        &mut self,
        class_name: &str,
    ) -> Result<Option<u64>, EvalStatus> {
        match class_name.to_ascii_lowercase().as_str() {
            "knownabstract" => Ok(Some(EVAL_REFLECTION_CLASS_FLAG_ABSTRACT)),
            "knownfinal" => Ok(Some(EVAL_REFLECTION_CLASS_FLAG_FINAL)),
            "knownreadonly" => Ok(Some(EVAL_REFLECTION_CLASS_FLAG_READONLY)),
            _ => Ok(None),
        }
    }
    /// Reports fake generated AOT ReflectionMethod flags for eval metadata unit tests.
    pub(in crate::interpreter::tests::support) fn runtime_reflection_method_flags(
        &mut self,
        class_name: &str,
        method_name: &str,
    ) -> Result<Option<u64>, EvalStatus> {
        if !class_name.eq_ignore_ascii_case("KnownClass") {
            return Ok(None);
        }
        match method_name.to_ascii_lowercase().as_str() {
            "answer" | "add_x" | "add2_x" | "read_x" | "run" => {
                Ok(Some(EVAL_REFLECTION_MEMBER_FLAG_PUBLIC))
            }
            "helper" => Ok(Some(
                EVAL_REFLECTION_MEMBER_FLAG_STATIC | EVAL_REFLECTION_MEMBER_FLAG_PROTECTED,
            )),
            "locked" => Ok(Some(
                EVAL_REFLECTION_MEMBER_FLAG_PUBLIC | EVAL_REFLECTION_MEMBER_FLAG_FINAL,
            )),
            _ => Ok(None),
        }
    }
    /// Reports fake generated AOT ReflectionMethod declaring classes for metadata unit tests.
    pub(in crate::interpreter::tests::support) fn runtime_reflection_method_declaring_class(
        &mut self,
        class_name: &str,
        method_name: &str,
    ) -> Result<Option<String>, EvalStatus> {
        if !class_name.eq_ignore_ascii_case("KnownClass") {
            return Ok(None);
        }
        match method_name.to_ascii_lowercase().as_str() {
            "answer" | "add_x" | "add2_x" | "read_x" | "run" | "helper" | "locked" => {
                Ok(Some("KnownClass".to_string()))
            }
            _ => Ok(None),
        }
    }
    /// Reports fake generated AOT ReflectionMethod names for eval metadata unit tests.
    pub(in crate::interpreter::tests::support) fn runtime_reflection_method_names(
        &mut self,
        class_name: &str,
    ) -> Result<RuntimeCellHandle, EvalStatus> {
        let mut array = self.runtime_string_array_new(3)?;
        if class_name.eq_ignore_ascii_case("KnownClass") {
            array = self.runtime_string_array_push(array, "run")?;
            array = self.runtime_string_array_push(array, "helper")?;
            array = self.runtime_string_array_push(array, "locked")?;
        }
        Ok(array)
    }
    /// Reports fake generated AOT ReflectionProperty flags for eval metadata unit tests.
    pub(in crate::interpreter::tests::support) fn runtime_reflection_property_flags(
        &mut self,
        class_name: &str,
        property_name: &str,
    ) -> Result<Option<u64>, EvalStatus> {
        if class_name.eq_ignore_ascii_case("KnownClass") && property_name == "promoted" {
            return Ok(Some(
                EVAL_REFLECTION_MEMBER_FLAG_PUBLIC | EVAL_REFLECTION_MEMBER_FLAG_PROMOTED,
            ));
        }
        Ok(None)
    }
    /// Reports fake generated AOT ReflectionProperty declaring classes for metadata unit tests.
    pub(in crate::interpreter::tests::support) fn runtime_reflection_property_declaring_class(
        &mut self,
        class_name: &str,
        property_name: &str,
    ) -> Result<Option<String>, EvalStatus> {
        if class_name.eq_ignore_ascii_case("KnownClass") && property_name == "promoted" {
            Ok(Some("KnownClass".to_string()))
        } else {
            Ok(None)
        }
    }
    /// Reports fake generated AOT ReflectionProperty names for eval metadata unit tests.
    pub(in crate::interpreter::tests::support) fn runtime_reflection_property_names(
        &mut self,
        class_name: &str,
    ) -> Result<RuntimeCellHandle, EvalStatus> {
        let mut array = self.runtime_string_array_new(1)?;
        if class_name.eq_ignore_ascii_case("KnownClass") {
            array = self.runtime_string_array_push(array, "promoted")?;
        }
        Ok(array)
    }
    /// Reports fake generated/AOT ReflectionClass interface names for metadata unit tests.
    pub(in crate::interpreter::tests::support) fn runtime_reflection_class_interface_names(
        &mut self,
        class_name: &str,
    ) -> Result<RuntimeCellHandle, EvalStatus> {
        let mut array = self.runtime_string_array_new(1)?;
        if class_name.eq_ignore_ascii_case("KnownClass") {
            array = self.runtime_string_array_push(array, "KnownInterface")?;
        } else if class_name.eq_ignore_ascii_case("KnownInterface") {
            array = self.runtime_string_array_push(array, "Traversable")?;
        }
        Ok(array)
    }
    /// Reports fake generated/AOT ReflectionClass trait names for metadata unit tests.
    pub(in crate::interpreter::tests::support) fn runtime_reflection_class_trait_names(
        &mut self,
        class_name: &str,
    ) -> Result<RuntimeCellHandle, EvalStatus> {
        let mut array = self.runtime_string_array_new(1)?;
        if class_name.eq_ignore_ascii_case("KnownClass") {
            array = self.runtime_string_array_push(array, "KnownTrait")?;
        } else if class_name.eq_ignore_ascii_case("KnownTrait") {
            array = self.runtime_string_array_push(array, "KnownInnerTrait")?;
        }
        Ok(array)
    }
    /// Reports fake generated/AOT ReflectionClass trait alias names for metadata unit tests.
    pub(in crate::interpreter::tests::support) fn runtime_reflection_class_trait_alias_names(
        &mut self,
        class_name: &str,
    ) -> Result<RuntimeCellHandle, EvalStatus> {
        let mut array = self.runtime_string_array_new(1)?;
        if class_name.eq_ignore_ascii_case("KnownClass") {
            array = self.runtime_string_array_push(array, "knownAlias")?;
        }
        Ok(array)
    }
    /// Reports fake generated/AOT ReflectionClass trait alias sources for metadata unit tests.
    pub(in crate::interpreter::tests::support) fn runtime_reflection_class_trait_alias_sources(
        &mut self,
        class_name: &str,
    ) -> Result<RuntimeCellHandle, EvalStatus> {
        let mut array = self.runtime_string_array_new(1)?;
        if class_name.eq_ignore_ascii_case("KnownClass") {
            array = self.runtime_string_array_push(array, "KnownTrait::source")?;
        }
        Ok(array)
    }
    /// Reports one fake AOT interface for eval `interface_exists` unit tests.
    pub(in crate::interpreter::tests::support) fn runtime_interface_exists(&mut self, name: &str) -> Result<bool, EvalStatus> {
        Ok([
            "KnownInterface",
            "ArrayAccess",
            "Countable",
            "Iterator",
            "IteratorAggregate",
            "JsonSerializable",
            "OuterIterator",
            "RecursiveIterator",
            "SeekableIterator",
            "SplObserver",
            "SplSubject",
            "Stringable",
            "Throwable",
            "Traversable",
        ]
        .iter()
        .any(|known| name.eq_ignore_ascii_case(known)))
    }
    /// Reports one fake AOT trait for eval `trait_exists` unit tests.
    pub(in crate::interpreter::tests::support) fn runtime_trait_exists(&mut self, name: &str) -> Result<bool, EvalStatus> {
        Ok(["KnownTrait", "KnownInnerTrait"]
            .iter()
            .any(|known| name.eq_ignore_ascii_case(known)))
    }
    /// Reports one fake AOT enum for eval `enum_exists` unit tests.
    pub(in crate::interpreter::tests::support) fn runtime_enum_exists(&mut self, name: &str) -> Result<bool, EvalStatus> {
        Ok(name.eq_ignore_ascii_case("KnownEnum"))
    }
    /// Reports fake class relations for eval `is_a` and `is_subclass_of` unit tests.
    pub(in crate::interpreter::tests::support) fn runtime_object_is_a(
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
            FakeValue::String(name) => {
                Ok(fake_runtime_object_is_a(&name, target_class, exclude_self))
            }
            _ => Ok(false),
        }
    }
    /// Returns a fake PHP class name for object-tagged test values.
    pub(in crate::interpreter::tests::support) fn runtime_object_class_name(
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
    pub(in crate::interpreter::tests::support) fn runtime_parent_class_name(
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
    pub(in crate::interpreter::tests::support) fn runtime_object_identity(
        &mut self,
        object: RuntimeCellHandle,
    ) -> Result<u64, EvalStatus> {
        match self.get(object) {
            FakeValue::Object(_) | FakeValue::Iterator { .. } => Ok(object.as_ptr() as u64),
            _ => Err(EvalStatus::RuntimeFatal),
        }
    }
}
