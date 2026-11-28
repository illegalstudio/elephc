//! Purpose:
//! Prepares constant array unions and equality from shared key and scalar primitives.
//!
//! Called from:
//! - The silent static initializer reducer after excluding objects and diagnostics.
//!
//! Key details:
//! - Union keeps the left value and appends only new right keys in source order.
//! - Equality recurses over arrays and delegates scalar leaves to existing kernels.

use super::*;

impl<V: RuntimeValueOps> Reducer<'_, V> {
    /// Builds an independent union without numeric reindexing or overwriting left entries.
    pub(super) fn array_union(&mut self, left: RuntimeCellHandle, right: RuntimeCellHandle)
        -> Result<Option<RuntimeCellHandle>, EvalStatus> {
        let capacity = self.values.array_len(left)?.checked_add(self.values.array_len(right)?)
            .ok_or(EvalStatus::RuntimeFatal)?;
        let Some(result) = self.make(|_, values| values.assoc_new(capacity))? else { unreachable!(); };
        for source in [left, right] {
            for index in 0..self.values.array_len(source)? {
                let Some(key) = self.make(|_, values| values.array_iter_key(source, index))? else { unreachable!(); };
                let Some(exists) = self.make(|_, values| values.array_key_exists(key, result))? else { unreachable!(); };
                if self.values.truthy(exists)? { continue; }
                let Some(value) = self.make(|_, values| values.array_get(source, key))? else { unreachable!(); };
                self.operation(|_, values| values.array_set(result, key, value))?;
            }
        }
        Ok(Some(result))
    }

    /// Compares constant arrays with PHP key presence, order and recursive value semantics.
    pub(super) fn array_equal(&mut self, left: RuntimeCellHandle, right: RuntimeCellHandle,
        strict: bool, depth: usize,
    ) -> Result<Option<bool>, EvalStatus> {
        if depth >= 256 { return Ok(None); }
        let left_tag = self.values.type_tag(left)?;
        let right_tag = self.values.type_tag(right)?;
        let array = |tag| matches!(tag, EVAL_TAG_ARRAY | EVAL_TAG_ASSOC);
        if !array(left_tag) && !array(right_tag) {
            let op = if strict { EvalBinOp::StrictEq } else { EvalBinOp::LooseEq };
            let Some(equal) = self.make(|_, values| values.compare(op, left, right))? else { unreachable!(); };
            return self.values.truthy(equal).map(Some);
        }
        if !array(left_tag) || !array(right_tag) {
            if strict { return Ok(Some(false)); }
            let scalar_tag = if array(left_tag) { right_tag } else { left_tag };
            if !matches!(scalar_tag, EVAL_TAG_NULL | EVAL_TAG_BOOL) { return Ok(Some(false)); }
            return Ok(Some(self.values.truthy(left)? == self.values.truthy(right)?));
        }
        let len = self.values.array_len(left)?;
        if len != self.values.array_len(right)? { return Ok(Some(false)); }
        for index in 0..len {
            let Some(key) = self.make(|_, values| values.array_iter_key(left, index))? else { unreachable!(); };
            if strict {
                let Some(right_key) = self.make(|_, values| values.array_iter_key(right, index))? else { unreachable!(); };
                let Some(equal) = self.make(|_, values| values.compare(EvalBinOp::StrictEq, key, right_key))? else { unreachable!(); };
                if !self.values.truthy(equal)? { return Ok(Some(false)); }
            } else {
                let Some(exists) = self.make(|_, values| values.array_key_exists(key, right))? else { unreachable!(); };
                if !self.values.truthy(exists)? { return Ok(Some(false)); }
            }
            let Some(left_value) = self.make(|_, values| values.array_get(left, key))? else { unreachable!(); };
            let Some(right_value) = self.make(|_, values| values.array_get(right, key))? else { unreachable!(); };
            match self.array_equal(left_value, right_value, strict, depth + 1)? {
                Some(true) => {}, other => return Ok(other),
            }
        }
        Ok(Some(true))
    }
}
