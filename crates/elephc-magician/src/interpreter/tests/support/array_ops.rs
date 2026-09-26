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
    /// Returns the next stored index and reports saturation independently of value allocation.
    pub(super) fn runtime_array_next_index(&mut self, array: RuntimeCellHandle) -> Result<Option<i64>, EvalStatus> {
        let mut receiver = array;
        while let Some(current) = self.references.get(&(receiver.as_ptr() as usize)) { receiver = *current; }
        match self.get(receiver) {
            FakeValue::Array(elements) => Ok(Some(elements.len() as i64)),
            FakeValue::Assoc(entries) => {
                let next = *self.array_next_indices.get(&(receiver.as_ptr() as usize)).unwrap_or(&i64::MIN);
                if next == i64::MAX && entries.iter().any(|(key, _)| *key == FakeKey::Int(i64::MAX)) {
                    Ok(None)
                } else {
                    Ok(Some(if next == i64::MIN { 0 } else { next }))
                }
            }
            _ => Err(EvalStatus::UnsupportedConstruct),
        }
    }

    /// Creates a fake indexed array cell.
    pub(super) fn runtime_array_new(
        &mut self,
        capacity: usize,
    ) -> Result<RuntimeCellHandle, EvalStatus> {
        Ok(self.alloc(FakeValue::Array(Vec::with_capacity(capacity))))
    }
    /// Creates a fake direct-string indexed array cell.
    pub(super) fn runtime_string_array_new(
        &mut self,
        capacity: usize,
    ) -> Result<RuntimeCellHandle, EvalStatus> {
        self.runtime_array_new(capacity)
    }
    /// Appends one string to a fake direct-string indexed array.
    pub(super) fn runtime_string_array_push(
        &mut self,
        array: RuntimeCellHandle,
        value: &str,
    ) -> Result<RuntimeCellHandle, EvalStatus> {
        let value = self.runtime_string(value)?;
        let id = array.as_ptr() as usize;
        let Some(FakeValue::Array(elements)) = self.values.get_mut(&id) else {
            return Err(EvalStatus::UnsupportedConstruct);
        };
        elements.push(value);
        Ok(array)
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
        let result = self.runtime_array_get_preserving_references(array, index)?;
        if self.is_reference(result)? { self.copy_value(result) } else { Ok(result) }
    }

    /// Reads a fake stored cell without discarding persistent reference identity.
    pub(super) fn runtime_array_get_preserving_references(
        &mut self,
        array: RuntimeCellHandle,
        index: RuntimeCellHandle,
    ) -> Result<RuntimeCellHandle, EvalStatus> {
        let key = self.key(index)?;
        let result = match self.get(array) {
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
        }?;
        self.retain(result)
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
    ///
    /// String keys promote indexed arrays to associative arrays so fake PHP arrays can
    /// model mixed integer/string keys produced by runtime metadata helpers.
    pub(super) fn runtime_array_set(
        &mut self,
        array: RuntimeCellHandle,
        index: RuntimeCellHandle,
        value: RuntimeCellHandle,
    ) -> Result<RuntimeCellHandle, EvalStatus> {
        let call_index = self.array_set_calls;
        self.array_set_calls += 1;
        if self.fail_array_set_call == Some(call_index) {
            return Err(EvalStatus::UnsupportedConstruct);
        }
        let key = self.key(index)?;
        let mut receiver = array;
        while let Some(current) = self.references.get(&(receiver.as_ptr() as usize)) {
            receiver = *current;
        }
        let existing = match self.get(receiver) {
            FakeValue::Array(elements) => match &key {
                FakeKey::Int(index) if *index >= 0 => elements.get(*index as usize).copied(),
                _ => None,
            },
            FakeValue::Assoc(entries) => entries.iter().find_map(|(entry_key, value)| (entry_key == &key).then_some(*value)),
            _ => None,
        };
        if let Some(reference) = existing {
            if self.is_reference(reference)? {
                let previous = self.reference_replace(reference, value)?;
                self.release(previous)?;
                return Ok(array);
            }
        }
        let id = receiver.as_ptr() as usize;
        let prior_next = match self.get(receiver) {
            FakeValue::Array(elements) if !elements.is_empty() => elements.len() as i64,
            _ => *self.array_next_indices.get(&id).unwrap_or(&i64::MIN),
        };
        let next = match (&key, existing) {
            (FakeKey::Int(key), None) if *key >= prior_next => key.saturating_add(1),
            _ => prior_next,
        };
        let Some(slot) = self.values.get_mut(&id) else {
            return Err(EvalStatus::UnsupportedConstruct);
        };
        match slot {
            FakeValue::Array(elements) => match key {
                FakeKey::Int(index) => {
                    if index < 0 {
                        return Err(EvalStatus::UnsupportedConstruct);
                    }
                    let index = index as usize;
                    while elements.len() <= index {
                        elements.push(RuntimeCellHandle::from_raw(std::ptr::null_mut()));
                    }
                    elements[index] = value;
                }
                key => {
                    let mut entries = std::mem::take(elements)
                        .into_iter()
                        .enumerate()
                        .filter_map(|(index, value)| {
                            (!value.as_ptr().is_null())
                                .then_some((FakeKey::Int(index as i64), value))
                        })
                        .collect::<Vec<_>>();
                    entries.push((key, value));
                    *slot = FakeValue::Assoc(entries);
                }
            },
            FakeValue::Assoc(entries) => {
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
        self.array_next_indices.insert(id, next);
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
