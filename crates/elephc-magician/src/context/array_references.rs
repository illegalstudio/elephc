//! Purpose:
//! Owns eval array-element reference metadata and its propagation across value copies.
//!
//! Called from:
//! - Array literal construction, array reads, and by-value eval array assignments.
//!
//! Key details:
//! - Box handles are opaque identity keys; payload COW does not detach PHP references.
//! - A fresh destination replaces any old metadata associated with a reused boxed address.

use super::*;

impl ElephcEvalContext {
    /// Removes obsolete side metadata when a fresh array literal receives a reused boxed identity.
    pub(crate) fn clear_array_element_aliases(&mut self, array: RuntimeCellHandle) {
        let identity = array.as_ptr() as usize;
        self.array_element_aliases.retain(|(owner, _), _| *owner != identity);
    }
    /// Binds one runtime array element slot to a PHP reference target.
    pub fn bind_array_element_alias(
        &mut self,
        array: RuntimeCellHandle,
        key: EvalArrayReferenceKey,
        target: EvalReferenceTarget,
    ) -> Option<EvalReferenceTarget> {
        self.array_element_aliases
            .insert((array.as_ptr() as usize, key), target)
    }

    /// Returns the persistent reference target bound to one runtime array element slot.
    pub fn array_element_alias(
        &self,
        array: RuntimeCellHandle,
        key: &EvalArrayReferenceKey,
    ) -> Option<&EvalReferenceTarget> {
        self.array_element_aliases
            .get(&(array.as_ptr() as usize, key.clone()))
    }

    /// Copies PHP element references when a by-value assignment creates a distinct array box.
    pub(crate) fn copy_array_element_aliases(&mut self, source: RuntimeCellHandle, target: RuntimeCellHandle) {
        let source = source.as_ptr() as usize;
        let target = target.as_ptr() as usize;
        if source == target { return; }
        let copied = self.array_element_aliases.iter()
            .filter(|((identity, _), _)| *identity == source)
            .map(|((_, key), value)| ((target, key.clone()), value.clone()))
            .collect::<Vec<_>>();
        self.array_element_aliases.retain(|(identity, _), _| *identity != target);
        self.array_element_aliases.extend(copied);
    }
}
