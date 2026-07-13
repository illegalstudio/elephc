//! Purpose:
//! Fake runtime operations for object property reads, writes, initialization,
//! shallow cloning, and property iteration.
//!
//! Called from:
//! - `FakeOps`'s `RuntimeValueOps` implementation in interpreter tests.
//!
//! Key details:
//! - Operations mutate only the in-memory fake value store.

use super::*;

impl FakeOps {
    /// Reads one fake object property by name.
    pub(in crate::interpreter::tests::support) fn runtime_property_get(
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
    /// Returns whether one fake object property exists by name.
    pub(in crate::interpreter::tests::support) fn runtime_property_is_initialized(
        &self,
        object: RuntimeCellHandle,
        property: &str,
    ) -> Result<bool, EvalStatus> {
        match self.get(object) {
            FakeValue::Object(properties) => {
                Ok(properties.iter().any(|(name, _)| name == property))
            }
            _ => Err(EvalStatus::UnsupportedConstruct),
        }
    }
    /// Writes one fake object property by name.
    pub(in crate::interpreter::tests::support) fn runtime_property_set(
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
    /// Creates one shallow fake object clone, preserving stored property handles.
    pub(in crate::interpreter::tests::support) fn runtime_object_clone_shallow(
        &mut self,
        object: RuntimeCellHandle,
    ) -> Result<RuntimeCellHandle, EvalStatus> {
        let id = object.as_ptr() as usize;
        let properties = match self.get(object) {
            FakeValue::Object(properties) => properties.clone(),
            _ => return Err(EvalStatus::UnsupportedConstruct),
        };
        let clone = self.alloc(FakeValue::Object(properties));
        if let Some(class_name) = self.object_classes.get(&id).cloned() {
            self.object_classes
                .insert(clone.as_ptr() as usize, class_name);
        }
        Ok(clone)
    }
    /// Returns the number of fake object properties in insertion order.
    pub(in crate::interpreter::tests::support) fn runtime_object_property_len(
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
    pub(in crate::interpreter::tests::support) fn runtime_object_property_iter_key(
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

}
