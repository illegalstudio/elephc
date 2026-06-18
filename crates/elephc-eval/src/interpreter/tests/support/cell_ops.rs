//! Purpose:
//! Scalar cell construction, type-tag, byte, and truthiness fake runtime operations.
//!
//! Called from:
//! - `crate::interpreter::tests::support::runtime_ops`.
//!
//! Key details:
//! - These helpers allocate primitive fake values and expose PHP-like truthiness.

use super::*;

impl FakeOps {
    /// Returns whether a fake runtime cell is null.
    pub(super) fn runtime_is_null(&mut self, value: RuntimeCellHandle) -> Result<bool, EvalStatus> {
        Ok(matches!(self.get(value), FakeValue::Null))
    }
    /// Returns the fake runtime tag corresponding to a test value.
    pub(super) fn runtime_type_tag(&mut self, value: RuntimeCellHandle) -> Result<u64, EvalStatus> {
        Ok(match self.get(value) {
            FakeValue::Int(_) => EVAL_TAG_INT,
            FakeValue::String(_) | FakeValue::Bytes(_) => EVAL_TAG_STRING,
            FakeValue::Float(_) => EVAL_TAG_FLOAT,
            FakeValue::Bool(_) => EVAL_TAG_BOOL,
            FakeValue::Array(_) => EVAL_TAG_ARRAY,
            FakeValue::Assoc(_) => EVAL_TAG_ASSOC,
            FakeValue::Object(_) | FakeValue::Iterator { .. } => EVAL_TAG_OBJECT,
            FakeValue::Resource(_) => EVAL_TAG_RESOURCE,
            FakeValue::Null => EVAL_TAG_NULL,
        })
    }
    /// Creates a fake null cell.
    pub(super) fn runtime_null(&mut self) -> Result<RuntimeCellHandle, EvalStatus> {
        Ok(self.alloc(FakeValue::Null))
    }
    /// Creates a fake bool cell.
    pub(super) fn runtime_bool_value(
        &mut self,
        value: bool,
    ) -> Result<RuntimeCellHandle, EvalStatus> {
        Ok(self.alloc(FakeValue::Bool(value)))
    }
    /// Creates a fake int cell.
    pub(super) fn runtime_int(&mut self, value: i64) -> Result<RuntimeCellHandle, EvalStatus> {
        Ok(self.alloc(FakeValue::Int(value)))
    }
    /// Creates a fake resource cell.
    pub(super) fn runtime_resource(&mut self, value: i64) -> Result<RuntimeCellHandle, EvalStatus> {
        Ok(self.alloc(FakeValue::Resource(value)))
    }
    /// Creates a fake float cell.
    pub(super) fn runtime_float(&mut self, value: f64) -> Result<RuntimeCellHandle, EvalStatus> {
        Ok(self.alloc(FakeValue::Float(value)))
    }
    /// Creates a fake string cell.
    pub(super) fn runtime_string(&mut self, value: &str) -> Result<RuntimeCellHandle, EvalStatus> {
        Ok(self.alloc(FakeValue::String(value.to_string())))
    }
    /// Creates a fake string cell from raw PHP bytes.
    pub(super) fn runtime_string_bytes_value(
        &mut self,
        value: &[u8],
    ) -> Result<RuntimeCellHandle, EvalStatus> {
        match std::str::from_utf8(value) {
            Ok(value) => self.string(value),
            Err(_) => Ok(self.alloc(FakeValue::Bytes(value.to_vec()))),
        }
    }
    /// Casts one fake runtime cell to bytes for nested eval parsing.
    pub(super) fn runtime_string_bytes(
        &mut self,
        value: RuntimeCellHandle,
    ) -> Result<Vec<u8>, EvalStatus> {
        Ok(self.string_bytes_for_value(&self.get(value)))
    }
    /// Returns PHP-like truthiness for fake runtime cells.
    pub(super) fn runtime_truthy(&mut self, value: RuntimeCellHandle) -> Result<bool, EvalStatus> {
        Ok(match self.get(value) {
            FakeValue::Null => false,
            FakeValue::Bool(value) => value,
            FakeValue::Int(value) => value != 0,
            FakeValue::Float(value) => value != 0.0,
            FakeValue::String(value) => !value.is_empty() && value != "0",
            FakeValue::Bytes(value) => !value.is_empty() && value.as_slice() != b"0",
            FakeValue::Array(value) => !value.is_empty(),
            FakeValue::Assoc(value) => !value.is_empty(),
            FakeValue::Object(_) | FakeValue::Iterator { .. } => true,
            FakeValue::Resource(_) => true,
        })
    }
}
