//! Purpose:
//! Array-related fake runtime operations for interpreter tests.
//!
//! Called from:
//! - `crate::interpreter::tests::support::runtime_ops`.
//!
//! Key details:
//! - These helpers back RuntimeValueOps array creation, reads, writes, and array tag checks.

use super::*;

impl FakeOps {
    /// Creates a fake indexed array cell.
    pub(super) fn runtime_array_new(
        &mut self,
        capacity: usize,
    ) -> Result<RuntimeCellHandle, EvalStatus> {
        Ok(self.alloc(FakeValue::Array(Vec::with_capacity(capacity))))
    }
    /// Creates a fake associative array cell.
    pub(super) fn runtime_assoc_new(
        &mut self,
        _capacity: usize,
    ) -> Result<RuntimeCellHandle, EvalStatus> {
        Ok(self.alloc(FakeValue::Assoc(Vec::new())))
    }
    /// Reads one fake indexed array element.
    pub(super) fn runtime_array_get(
        &mut self,
        array: RuntimeCellHandle,
        index: RuntimeCellHandle,
    ) -> Result<RuntimeCellHandle, EvalStatus> {
        let key = self.key(index)?;
        match self.get(array) {
            FakeValue::Array(elements) => {
                let FakeKey::Int(index) = key else {
                    return self.null();
                };
                if index < 0 {
                    return self.null();
                }
                elements
                    .get(index as usize)
                    .copied()
                    .map_or_else(|| self.null(), Ok)
            }
            FakeValue::Assoc(entries) => entries
                .iter()
                .find_map(|(entry_key, value)| (entry_key == &key).then_some(*value))
                .map_or_else(|| self.null(), Ok),
            _ => self.null(),
        }
    }
    /// Checks whether a fake array has the requested key without reading its value.
    pub(super) fn runtime_array_key_exists(
        &mut self,
        key: RuntimeCellHandle,
        array: RuntimeCellHandle,
    ) -> Result<RuntimeCellHandle, EvalStatus> {
        let key = self.key(key)?;
        let exists = match self.get(array) {
            FakeValue::Array(elements) => {
                matches!(key, FakeKey::Int(index) if index >= 0 && (index as usize) < elements.len())
            }
            FakeValue::Assoc(entries) => entries.iter().any(|(entry_key, _)| entry_key == &key),
            _ => return Err(EvalStatus::UnsupportedConstruct),
        };
        self.bool_value(exists)
    }
    /// Returns one fake foreach key by insertion-order position.
    pub(super) fn runtime_array_iter_key(
        &mut self,
        array: RuntimeCellHandle,
        position: usize,
    ) -> Result<RuntimeCellHandle, EvalStatus> {
        match self.get(array) {
            FakeValue::Array(elements) if position < elements.len() => self.int(position as i64),
            FakeValue::Assoc(entries) => {
                let Some((key, _)) = entries.get(position) else {
                    return self.null();
                };
                self.alloc_key(key)
            }
            FakeValue::Array(_) => self.null(),
            _ => Err(EvalStatus::UnsupportedConstruct),
        }
    }
    /// Writes one fake indexed or associative array element.
    pub(super) fn runtime_array_set(
        &mut self,
        array: RuntimeCellHandle,
        index: RuntimeCellHandle,
        value: RuntimeCellHandle,
    ) -> Result<RuntimeCellHandle, EvalStatus> {
        let key = self.key(index)?;
        let id = array.as_ptr() as usize;
        match self.values.get_mut(&id) {
            Some(FakeValue::Array(elements)) => {
                let FakeKey::Int(index) = key else {
                    return Err(EvalStatus::UnsupportedConstruct);
                };
                if index < 0 {
                    return Err(EvalStatus::UnsupportedConstruct);
                }
                let index = index as usize;
                while elements.len() <= index {
                    elements.push(RuntimeCellHandle::from_raw(std::ptr::null_mut()));
                }
                elements[index] = value;
            }
            Some(FakeValue::Assoc(entries)) => {
                if let Some((_, existing_value)) =
                    entries.iter_mut().find(|(entry_key, _)| entry_key == &key)
                {
                    *existing_value = value;
                } else {
                    entries.push((key, value));
                }
            }
            _ => return Err(EvalStatus::UnsupportedConstruct),
        }
        Ok(array)
    }
    /// Returns the visible element count for fake array values.
    pub(super) fn runtime_array_len(
        &mut self,
        array: RuntimeCellHandle,
    ) -> Result<usize, EvalStatus> {
        match self.get(array) {
            FakeValue::Array(elements) => Ok(elements.len()),
            FakeValue::Assoc(entries) => Ok(entries.len()),
            _ => Err(EvalStatus::UnsupportedConstruct),
        }
    }
    /// Returns whether a fake runtime cell is an indexed or associative array.
    pub(super) fn runtime_is_array_like(
        &mut self,
        value: RuntimeCellHandle,
    ) -> Result<bool, EvalStatus> {
        Ok(matches!(
            self.get(value),
            FakeValue::Array(_) | FakeValue::Assoc(_)
        ))
    }
}
